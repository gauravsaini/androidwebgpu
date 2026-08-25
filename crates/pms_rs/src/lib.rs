//! # pms_rs
//!
//! Native Rust Android Package Manager Service (`android.content.pm.IPackageManager`)
//! for AndroidWebGPU.
//!
//! Features:
//! - Full Binary `AndroidManifest.xml` (AXML) chunk parser with UTF-8/UTF-16 string pools,
//!   resource maps, tag hierarchy stack, attributes, and permissions.
//! - Binary `resources.arsc` resource table parser resolving strings, drawables, colors,
//!   and integer resource IDs.
//! - Native `PackageManagerService` implementing package registration, APK loading from ZIP,
//!   component lookup, intent resolution engine, and permission queries.
//! - Direct AIDL IPC over `binder_sys::BinderKernelTransport` registered as `"package"`.

pub mod arsc;
pub mod axml;
pub mod package_manager;
pub mod service;
pub mod types;

// -----------------------------------------------------------------------------
// Top-Level Public Exports
// -----------------------------------------------------------------------------

pub use arsc::{
    ArscEntry, ArscError, ArscPackage, ArscParser, ArscTable, ArscType, ResourceValue,
    ENTRY_FLAG_COMPLEX, ENTRY_FLAG_PUBLIC, NO_ENTRY, RES_TABLE_PACKAGE_TYPE, RES_TABLE_TYPE,
    RES_TABLE_TYPE_SPEC_TYPE, RES_TABLE_TYPE_TYPE,
};

pub use axml::{
    AxmlError, AxmlParser, ParsedAxmlManifest, UsesFeatureInfo, XmlAttribute, ATTR_EXPORTED,
    ATTR_GLES_VERSION, ATTR_HAS_CODE, ATTR_HOST, ATTR_ICON, ATTR_LABEL, ATTR_LAUNCH_MODE,
    ATTR_MIME_TYPE, ATTR_MIN_SDK_VERSION, ATTR_NAME, ATTR_PERMISSION, ATTR_PRIORITY,
    ATTR_REQUIRED, ATTR_SCHEME, ATTR_TARGET_SDK_VERSION, ATTR_THEME, ATTR_VERSION_CODE,
    ATTR_VERSION_NAME, RES_STRING_POOL_TYPE, RES_XML_CDATA_TYPE, RES_XML_END_ELEMENT_TYPE,
    RES_XML_END_NAMESPACE_TYPE, RES_XML_FIRST_CHUNK_TYPE, RES_XML_RESOURCE_MAP_TYPE,
    RES_XML_START_ELEMENT_TYPE, RES_XML_START_NAMESPACE_TYPE, RES_XML_TYPE, TYPE_ATTRIBUTE,
    TYPE_DIMENSION, TYPE_DYNAMIC_ATTRIBUTE, TYPE_DYNAMIC_REFERENCE, TYPE_FLOAT, TYPE_FRACTION,
    TYPE_INT_BOOLEAN, TYPE_INT_DEC, TYPE_INT_HEX, TYPE_NULL, TYPE_REFERENCE, TYPE_STRING,
};

pub use package_manager::{InstalledPackage, PackageManagerService, PmsError};

pub use service::{
    ipackage_manager_codes, register_package_service, IPackageManager, PackageManagerClient,
    IPACKAGE_MANAGER_DESCRIPTOR,
};

pub use types::{
    ActivityInfo, ApplicationInfo, ComponentName, Intent, IntentFilter, PackageInfo,
    ReceiverInfo, ResolveInfo, ServiceInfo, GET_ACTIVITIES, GET_CONFIGURATIONS,
    GET_DISABLED_COMPONENTS, GET_GIDS, GET_INSTRUMENTATION, GET_INTENT_FILTERS, GET_META_DATA,
    GET_PERMISSIONS, GET_PROVIDERS, GET_RECEIVERS, GET_RESOLVED_FILTER, GET_SERVICES,
    GET_SHARED_LIBRARY_FILES, GET_SIGNATURES, GET_UNINSTALLED_PACKAGES,
    GET_URI_PERMISSION_PATTERNS, MATCH_ALL, MATCH_DEFAULT_ONLY, PERMISSION_DENIED,
    PERMISSION_GRANTED,
};
