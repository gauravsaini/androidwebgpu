//! Android Package Manager Data Structures and Parcelable Implementations.

use aidl_compat::{Parcel, Parcelable, Result as AidlResult, Status, STATUS_BAD_VALUE};
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Package Manager Query & Component Flags
// -----------------------------------------------------------------------------

pub const GET_ACTIVITIES: i64 = 0x00000001;
pub const GET_RECEIVERS: i64 = 0x00000002;
pub const GET_SERVICES: i64 = 0x00000004;
pub const GET_PROVIDERS: i64 = 0x00000008;
pub const GET_INSTRUMENTATION: i64 = 0x00000010;
pub const GET_INTENT_FILTERS: i64 = 0x00000020;
pub const GET_SIGNATURES: i64 = 0x00000040;
pub const GET_RESOLVED_FILTER: i64 = 0x00000040;
pub const GET_META_DATA: i64 = 0x00000080;
pub const GET_GIDS: i64 = 0x00000100;
pub const GET_DISABLED_COMPONENTS: i64 = 0x00000200;
pub const GET_SHARED_LIBRARY_FILES: i64 = 0x00000400;
pub const GET_URI_PERMISSION_PATTERNS: i64 = 0x00000800;
pub const GET_PERMISSIONS: i64 = 0x00001000;
pub const GET_UNINSTALLED_PACKAGES: i64 = 0x00002000;
pub const GET_CONFIGURATIONS: i64 = 0x00004000;
pub const MATCH_DEFAULT_ONLY: i64 = 0x00010000;
pub const MATCH_ALL: i64 = 0x00020000;

// Permission Results
pub const PERMISSION_GRANTED: i32 = 0;
pub const PERMISSION_DENIED: i32 = -1;

// -----------------------------------------------------------------------------
// Component Name
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ComponentName {
    pub package_name: String,
    pub class_name: String,
}

impl ComponentName {
    pub fn new(package_name: impl Into<String>, class_name: impl Into<String>) -> Self {
        Self {
            package_name: package_name.into(),
            class_name: class_name.into(),
        }
    }

    pub fn flatten_to_string(&self) -> String {
        format!("{}/{}", self.package_name, self.class_name)
    }

    pub fn unflatten_from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() == 2 {
            let pkg = parts[0];
            let mut cls = parts[1].to_string();
            if cls.starts_with('.') {
                cls = format!("{}{}", pkg, cls);
            }
            Some(Self {
                package_name: pkg.to_string(),
                class_name: cls,
            })
        } else {
            None
        }
    }
}

impl Parcelable for ComponentName {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(Some(&self.package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.class_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.package_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.class_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Intent & Intent Filter
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Intent {
    pub action: Option<String>,
    pub categories: Vec<String>,
    pub data_uri: Option<String>,
    pub mime_type: Option<String>,
    pub component: Option<ComponentName>,
    pub flags: u32,
    pub package: Option<String>,
}

impl Intent {
    pub fn new(action: Option<&str>) -> Self {
        Self {
            action: action.map(|a| a.to_string()),
            categories: Vec::new(),
            data_uri: None,
            mime_type: None,
            component: None,
            flags: 0,
            package: None,
        }
    }

    pub fn with_component(component: ComponentName) -> Self {
        Self {
            action: None,
            categories: Vec::new(),
            data_uri: None,
            mime_type: None,
            component: Some(component),
            flags: 0,
            package: None,
        }
    }

    pub fn add_category(&mut self, category: impl Into<String>) {
        self.categories.push(category.into());
    }

    pub fn set_component(&mut self, component: ComponentName) {
        self.component = Some(component);
    }
}

impl Parcelable for Intent {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(self.action.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.categories.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for cat in &self.categories {
            parcel
                .write_utf8(Some(cat))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        parcel
            .write_utf8(self.data_uri.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.mime_type.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if let Some(comp) = &self.component {
            parcel
                .write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            comp.write_to_parcel(parcel)?;
        } else {
            parcel
                .write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        parcel
            .write_u32(self.flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.package.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.action = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let cat_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.categories.clear();
        for _ in 0..cat_count.max(0) {
            if let Some(cat) = parcel
                .read_utf8(offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            {
                self.categories.push(cat);
            }
        }
        self.data_uri = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.mime_type = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let has_component = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_component {
            let mut comp = ComponentName::default();
            comp.read_from_parcel_at(parcel, offset)?;
            self.component = Some(comp);
        } else {
            self.component = None;
        }
        self.flags = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.package = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IntentFilter {
    pub actions: Vec<String>,
    pub categories: Vec<String>,
    pub data_schemes: Vec<String>,
    pub priority: i32,
}

impl IntentFilter {
    pub fn matches(&self, intent: &Intent) -> bool {
        // 1. Action Match: if intent specifies an action, filter must contain it
        if let Some(action) = &intent.action {
            if !self.actions.contains(action) {
                return false;
            }
        }

        // 2. Category Match: filter must contain all categories specified in the intent
        for cat in &intent.categories {
            if !self.categories.contains(cat) {
                return false;
            }
        }

        // 3. Scheme Match: if intent specifies a data URI scheme
        if let Some(data) = &intent.data_uri {
            if let Some(scheme) = data.split(':').next() {
                if !self.data_schemes.is_empty() && !self.data_schemes.iter().any(|s| s == scheme) {
                    return false;
                }
            }
        }

        true
    }
}

impl Parcelable for IntentFilter {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_i32(self.actions.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for act in &self.actions {
            parcel
                .write_utf8(Some(act))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        parcel
            .write_i32(self.categories.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for cat in &self.categories {
            parcel
                .write_utf8(Some(cat))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        parcel
            .write_i32(self.data_schemes.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for sch in &self.data_schemes {
            parcel
                .write_utf8(Some(sch))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        parcel
            .write_i32(self.priority)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let act_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.actions.clear();
        for _ in 0..act_count.max(0) {
            if let Some(a) = parcel
                .read_utf8(offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            {
                self.actions.push(a);
            }
        }
        let cat_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.categories.clear();
        for _ in 0..cat_count.max(0) {
            if let Some(c) = parcel
                .read_utf8(offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            {
                self.categories.push(c);
            }
        }
        let sch_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.data_schemes.clear();
        for _ in 0..sch_count.max(0) {
            if let Some(s) = parcel
                .read_utf8(offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            {
                self.data_schemes.push(s);
            }
        }
        self.priority = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ApplicationInfo
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApplicationInfo {
    pub package_name: String,
    pub name: Option<String>,
    pub label: Option<String>,
    pub icon: u32,
    pub target_sdk_version: i32,
    pub min_sdk_version: i32,
    pub flags: u32,
    pub data_dir: String,
    pub source_dir: String,
    pub public_source_dir: String,
    pub native_library_dir: String,
    pub uid: i32,
    pub enabled: bool,
}

impl Parcelable for ApplicationInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(Some(&self.package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.name.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.label.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.icon)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.target_sdk_version)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.min_sdk_version)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.data_dir))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.source_dir))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.public_source_dir))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.native_library_dir))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.uid)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.enabled)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.package_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.label = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.icon = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.target_sdk_version = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.min_sdk_version = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.flags = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.data_dir = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.source_dir = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.public_source_dir = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.native_library_dir = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.uid = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.enabled = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ActivityInfo
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ActivityInfo {
    pub name: String,
    pub package_name: String,
    pub label: Option<String>,
    pub icon: u32,
    pub theme: u32,
    pub launch_mode: i32,
    pub permission: Option<String>,
    pub exported: bool,
    pub enabled: bool,
    pub intent_filters: Vec<IntentFilter>,
    pub application_info: Option<ApplicationInfo>,
}

impl Parcelable for ActivityInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(Some(&self.name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.label.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.icon)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.theme)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.launch_mode)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.permission.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.exported)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.enabled)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        parcel
            .write_i32(self.intent_filters.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for f in &self.intent_filters {
            f.write_to_parcel(parcel)?;
        }

        if let Some(app) = &self.application_info {
            parcel
                .write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            app.write_to_parcel(parcel)?;
        } else {
            parcel
                .write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.package_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.label = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.icon = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.theme = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.launch_mode = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.permission = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.exported = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.enabled = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let filter_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.intent_filters.clear();
        for _ in 0..filter_count.max(0) {
            let mut f = IntentFilter::default();
            f.read_from_parcel_at(parcel, offset)?;
            self.intent_filters.push(f);
        }

        let has_app = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_app {
            let mut app = ApplicationInfo::default();
            app.read_from_parcel_at(parcel, offset)?;
            self.application_info = Some(app);
        } else {
            self.application_info = None;
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ServiceInfo, ReceiverInfo & ProviderInfo
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServiceInfo {
    pub name: String,
    pub package_name: String,
    pub permission: Option<String>,
    pub exported: bool,
    pub enabled: bool,
}

impl Parcelable for ServiceInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(Some(&self.name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.permission.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.exported)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.enabled)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.package_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.permission = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.exported = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.enabled = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReceiverInfo {
    pub name: String,
    pub package_name: String,
    pub permission: Option<String>,
    pub exported: bool,
    pub enabled: bool,
    pub intent_filters: Vec<IntentFilter>,
}

impl Parcelable for ReceiverInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(Some(&self.name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.permission.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.exported)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.enabled)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.intent_filters.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for f in &self.intent_filters {
            f.write_to_parcel(parcel)?;
        }
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.package_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.permission = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.exported = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.enabled = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let filter_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.intent_filters.clear();
        for _ in 0..filter_count.max(0) {
            let mut f = IntentFilter::default();
            f.read_from_parcel_at(parcel, offset)?;
            self.intent_filters.push(f);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderInfo {
    pub name: String,
    pub package_name: String,
    pub authority: String,
    pub exported: bool,
    pub grant_uri_permissions: bool,
    pub read_permission: Option<String>,
    pub write_permission: Option<String>,
    pub multiprocess: bool,
    pub init_order: i32,
    pub enabled: bool,
    pub application_info: Option<ApplicationInfo>,
}

impl Parcelable for ProviderInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(Some(&self.name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.authority))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.exported)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.grant_uri_permissions)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.read_permission.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.write_permission.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.multiprocess)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.init_order)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.enabled)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if let Some(app) = &self.application_info {
            parcel
                .write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            app.write_to_parcel(parcel)?;
        } else {
            parcel
                .write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.package_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.authority = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.exported = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.grant_uri_permissions = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.read_permission = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.write_permission = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.multiprocess = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.init_order = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.enabled = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let has_app = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_app {
            let mut app = ApplicationInfo::default();
            app.read_from_parcel_at(parcel, offset)?;
            self.application_info = Some(app);
        } else {
            self.application_info = None;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ResolveInfo
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResolveInfo {
    pub activity_info: Option<ActivityInfo>,
    pub match_quality: i32,
    pub priority: i32,
    pub is_default: bool,
    pub label: Option<String>,
    pub icon: u32,
}

impl Parcelable for ResolveInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        if let Some(act) = &self.activity_info {
            parcel
                .write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            act.write_to_parcel(parcel)?;
        } else {
            parcel
                .write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        parcel
            .write_i32(self.match_quality)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.priority)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_bool(self.is_default)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.label.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.icon)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let has_act = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_act {
            let mut act = ActivityInfo::default();
            act.read_from_parcel_at(parcel, offset)?;
            self.activity_info = Some(act);
        } else {
            self.activity_info = None;
        }
        self.match_quality = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.priority = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.is_default = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.label = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.icon = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// PackageInfo
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PackageInfo {
    pub package_name: String,
    pub version_code: i32,
    pub version_name: Option<String>,
    pub application_info: Option<ApplicationInfo>,
    pub activities: Vec<ActivityInfo>,
    pub services: Vec<ServiceInfo>,
    pub receivers: Vec<ReceiverInfo>,
    pub providers: Vec<ProviderInfo>,
    pub requested_permissions: Vec<String>,
    pub first_install_time: i64,
    pub last_update_time: i64,
}

impl Parcelable for PackageInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_utf8(Some(&self.package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.version_code)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.version_name.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(app) = &self.application_info {
            parcel
                .write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            app.write_to_parcel(parcel)?;
        } else {
            parcel
                .write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        parcel
            .write_i32(self.activities.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for act in &self.activities {
            act.write_to_parcel(parcel)?;
        }

        parcel
            .write_i32(self.services.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for svc in &self.services {
            svc.write_to_parcel(parcel)?;
        }

        parcel
            .write_i32(self.receivers.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for rcv in &self.receivers {
            rcv.write_to_parcel(parcel)?;
        }

        parcel
            .write_i32(self.providers.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for prov in &self.providers {
            prov.write_to_parcel(parcel)?;
        }

        parcel
            .write_i32(self.requested_permissions.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for perm in &self.requested_permissions {
            parcel
                .write_utf8(Some(perm))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        parcel
            .write_i64(self.first_install_time)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i64(self.last_update_time)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.package_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.version_code = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.version_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let has_app = parcel
            .read_bool(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_app {
            let mut app = ApplicationInfo::default();
            app.read_from_parcel_at(parcel, offset)?;
            self.application_info = Some(app);
        } else {
            self.application_info = None;
        }

        let act_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.activities.clear();
        for _ in 0..act_count.max(0) {
            let mut act = ActivityInfo::default();
            act.read_from_parcel_at(parcel, offset)?;
            self.activities.push(act);
        }

        let svc_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.services.clear();
        for _ in 0..svc_count.max(0) {
            let mut svc = ServiceInfo::default();
            svc.read_from_parcel_at(parcel, offset)?;
            self.services.push(svc);
        }

        let rcv_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.receivers.clear();
        for _ in 0..rcv_count.max(0) {
            let mut rcv = ReceiverInfo::default();
            rcv.read_from_parcel_at(parcel, offset)?;
            self.receivers.push(rcv);
        }

        let prov_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.providers.clear();
        for _ in 0..prov_count.max(0) {
            let mut prov = ProviderInfo::default();
            prov.read_from_parcel_at(parcel, offset)?;
            self.providers.push(prov);
        }

        let perm_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.requested_permissions.clear();
        for _ in 0..perm_count.max(0) {
            if let Some(p) = parcel
                .read_utf8(offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            {
                self.requested_permissions.push(p);
            }
        }

        self.first_install_time = parcel
            .read_i64(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.last_update_time = parcel
            .read_i64(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        Ok(())
    }
}
