//! In-Memory Package Manager Service Logic and APK Registry.

use crate::arsc::{ArscParser, ArscTable};
use crate::axml::AxmlParser;
use crate::types::*;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use zip::ZipArchive;

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum PmsError {
    #[error("Failed to parse AXML manifest: {0}")]
    AxmlError(#[from] crate::axml::AxmlError),
    #[error("Failed to parse ARSC resources: {0}")]
    ArscError(#[from] crate::arsc::ArscError),
    #[error("APK Zip error: {0}")]
    ZipError(String),
    #[error("Missing AndroidManifest.xml in APK")]
    MissingManifest,
    #[error("Package '{0}' not found")]
    PackageNotFound(String),
    #[error("Activity '{0}' not found")]
    ActivityNotFound(String),
}

// -----------------------------------------------------------------------------
// Installed Package Representation
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub info: PackageInfo,
    pub resources: Option<Arc<ArscTable>>,
}

// -----------------------------------------------------------------------------
// Package Manager Service
// -----------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct PackageManagerService {
    packages: RwLock<HashMap<String, InstalledPackage>>,
    components: RwLock<HashMap<ComponentName, ActivityInfo>>,
}

impl PackageManagerService {
    /// Create a new empty `PackageManagerService`.
    pub fn new() -> Self {
        Self {
            packages: RwLock::new(HashMap::new()),
            components: RwLock::new(HashMap::new()),
        }
    }

    /// Install an APK from raw ZIP archive bytes.
    pub fn install_apk(&self, apk_bytes: &[u8]) -> Result<PackageInfo, PmsError> {
        let cursor = std::io::Cursor::new(apk_bytes);
        let mut zip = ZipArchive::new(cursor).map_err(|e| PmsError::ZipError(format!("{:?}", e)))?;

        let mut manifest_bytes = Vec::new();
        let mut arsc_bytes = Vec::new();

        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .map_err(|e| PmsError::ZipError(format!("{:?}", e)))?;
            let name = file.name().to_string();
            if name == "AndroidManifest.xml" {
                file.read_to_end(&mut manifest_bytes)
                    .map_err(|e| PmsError::ZipError(format!("{:?}", e)))?;
            } else if name == "resources.arsc" {
                file.read_to_end(&mut arsc_bytes)
                    .map_err(|e| PmsError::ZipError(format!("{:?}", e)))?;
            }
        }

        if manifest_bytes.is_empty() {
            return Err(PmsError::MissingManifest);
        }

        let arsc_opt = if !arsc_bytes.is_empty() {
            Some(arsc_bytes.as_slice())
        } else {
            None
        };

        self.install_package_bytes(&manifest_bytes, arsc_opt)
    }

    /// Install a package from raw `AndroidManifest.xml` bytes and optional `resources.arsc` bytes.
    pub fn install_package_bytes(
        &self,
        manifest_axml: &[u8],
        arsc_bytes: Option<&[u8]>,
    ) -> Result<PackageInfo, PmsError> {
        let parsed = AxmlParser::parse(manifest_axml)?;
        let mut pkg_info = parsed.to_package_info();

        let arsc_table = if let Some(arsc) = arsc_bytes {
            if let Ok(table) = ArscParser::parse(arsc) {
                // Resolve string references in ApplicationInfo and ActivityInfos
                if let Some(app) = &mut pkg_info.application_info {
                    if let Some(label) = &app.label {
                        app.label = Some(table.resolve_string_ref(label, &pkg_info.package_name));
                    }
                }
                for act in &mut pkg_info.activities {
                    if let Some(label) = &act.label {
                        act.label = Some(table.resolve_string_ref(label, &pkg_info.package_name));
                    }
                }
                Some(Arc::new(table))
            } else {
                None
            }
        } else {
            None
        };

        self.install_package_info(pkg_info.clone(), arsc_table);
        Ok(pkg_info)
    }

    /// Register a `PackageInfo` directly.
    pub fn install_package_info(
        &self,
        mut pkg_info: PackageInfo,
        resources: Option<Arc<ArscTable>>,
    ) {
        let mut pkgs = self.packages.write().unwrap();
        let mut comps = self.components.write().unwrap();

        // Index activities by ComponentName
        for act in &mut pkg_info.activities {
            if act.package_name.is_empty() {
                act.package_name = pkg_info.package_name.clone();
            }
            let comp = ComponentName::new(&act.package_name, &act.name);
            comps.insert(comp, act.clone());
        }

        pkgs.insert(
            pkg_info.package_name.clone(),
            InstalledPackage {
                info: pkg_info,
                resources,
            },
        );
    }

    // -------------------------------------------------------------------------
    // IPackageManager API Methods
    // -------------------------------------------------------------------------

    /// `getPackageInfo(packageName: String, flags: i64, userId: i32) -> Option<PackageInfo>`
    pub fn get_package_info(
        &self,
        package_name: &str,
        flags: i64,
        _user_id: i32,
    ) -> Option<PackageInfo> {
        let pkgs = self.packages.read().unwrap();
        let installed = pkgs.get(package_name)?;
        let mut info = installed.info.clone();

        // Filter components based on flags
        if (flags & GET_ACTIVITIES) == 0 {
            info.activities.clear();
        }
        if (flags & GET_PERMISSIONS) == 0 {
            info.requested_permissions.clear();
        }

        Some(info)
    }

    /// `getApplicationInfo(packageName: String, flags: i64, userId: i32) -> Option<ApplicationInfo>`
    pub fn get_application_info(
        &self,
        package_name: &str,
        _flags: i64,
        _user_id: i32,
    ) -> Option<ApplicationInfo> {
        let pkgs = self.packages.read().unwrap();
        let installed = pkgs.get(package_name)?;
        installed.info.application_info.clone()
    }

    /// `getActivityInfo(component: ComponentName, flags: i64, userId: i32) -> Option<ActivityInfo>`
    pub fn get_activity_info(
        &self,
        component: &ComponentName,
        _flags: i64,
        _user_id: i32,
    ) -> Option<ActivityInfo> {
        let comps = self.components.read().unwrap();
        if let Some(act) = comps.get(component) {
            return Some(act.clone());
        }

        // Try fuzzy lookup (if class name starts with '.')
        let pkgs = self.packages.read().unwrap();
        let installed = pkgs.get(&component.package_name)?;
        for act in &installed.info.activities {
            if act.name == component.class_name
                || act.name.ends_with(&component.class_name)
                || component.class_name.ends_with(&act.name)
            {
                return Some(act.clone());
            }
        }
        None
    }

    /// `resolveIntent(intent: Intent, resolvedType: String, flags: i64, userId: i32) -> Option<ResolveInfo>`
    pub fn resolve_intent(
        &self,
        intent: &Intent,
        resolved_type: &str,
        flags: i64,
        user_id: i32,
    ) -> Option<ResolveInfo> {
        let matches = self.query_intent_activities(intent, resolved_type, flags, user_id);
        matches.into_iter().next()
    }

    /// `queryIntentActivities(intent: Intent, resolvedType: String, flags: i64, userId: i32) -> Vec<ResolveInfo>`
    pub fn query_intent_activities(
        &self,
        intent: &Intent,
        _resolved_type: &str,
        flags: i64,
        _user_id: i32,
    ) -> Vec<ResolveInfo> {
        let mut results = Vec::new();

        // 1. If explicit component is set
        if let Some(comp) = &intent.component {
            if let Some(act) = self.get_activity_info(comp, flags, 0) {
                results.push(ResolveInfo {
                    activity_info: Some(act.clone()),
                    match_quality: 0x01000000,
                    priority: 0,
                    is_default: true,
                    label: act.label.clone(),
                    icon: act.icon,
                });
                return results;
            }
        }

        // 2. Scan all packages and activities
        let pkgs = self.packages.read().unwrap();
        for (pkg_name, installed) in pkgs.iter() {
            // If intent has target package filter, only inspect matching package
            if let Some(target_pkg) = &intent.package {
                if target_pkg != pkg_name {
                    continue;
                }
            }

            for act in &installed.info.activities {
                for filter in &act.intent_filters {
                    if filter.matches(intent) {
                        // Check MATCH_DEFAULT_ONLY
                        if (flags & MATCH_DEFAULT_ONLY) != 0 {
                            let is_launcher = intent.action.as_deref() == Some("android.intent.action.MAIN")
                                && intent.categories.iter().any(|c| c == "android.intent.category.LAUNCHER");
                            let has_default = filter.categories.iter().any(|c| c == "android.intent.category.DEFAULT");
                            if !is_launcher && !has_default {
                                continue;
                            }
                        }

                        results.push(ResolveInfo {
                            activity_info: Some(act.clone()),
                            match_quality: 0x00100000,
                            priority: filter.priority,
                            is_default: filter.categories.iter().any(|c| c == "android.intent.category.DEFAULT"),
                            label: act.label.clone(),
                            icon: act.icon,
                        });
                        break; // One match per activity is sufficient
                    }
                }
            }
        }

        // Sort descending by priority
        results.sort_by(|a, b| b.priority.cmp(&a.priority));
        results
    }

    /// `checkPermission(permName: String, pkgName: String, userId: i32) -> i32`
    ///
    /// Returns `0` (`PERMISSION_GRANTED`) for granted permissions, or `-1` (`PERMISSION_DENIED`).
    pub fn check_permission(&self, _perm_name: &str, _pkg_name: &str, _user_id: i32) -> i32 {
        // AndroidWebGPU grant-all permission model for native execution
        PERMISSION_GRANTED
    }

    /// `getInstalledPackages(flags: i64, userId: i32) -> Vec<PackageInfo>`
    pub fn get_installed_packages(&self, flags: i64, user_id: i32) -> Vec<PackageInfo> {
        let pkgs = self.packages.read().unwrap();
        let mut list = Vec::with_capacity(pkgs.len());
        for pkg_name in pkgs.keys() {
            if let Some(info) = self.get_package_info(pkg_name, flags, user_id) {
                list.push(info);
            }
        }
        list
    }

    /// `getInstalledApplications(flags: i64, userId: i32) -> Vec<ApplicationInfo>`
    pub fn get_installed_applications(&self, flags: i64, user_id: i32) -> Vec<ApplicationInfo> {
        let pkgs = self.packages.read().unwrap();
        let mut list = Vec::with_capacity(pkgs.len());
        for pkg_name in pkgs.keys() {
            if let Some(app) = self.get_application_info(pkg_name, flags, user_id) {
                list.push(app);
            }
        }
        list
    }

    /// Total count of installed packages.
    pub fn package_count(&self) -> usize {
        self.packages.read().unwrap().len()
    }
}
