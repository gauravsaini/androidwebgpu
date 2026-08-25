//! # wms_rs
//!
//! Native Rust Android Window Manager Service (`android.view.IWindowManager`, `android.view.IWindowSession`),
//! Surface Lifecycle Bridge, and SurfaceControl Management for AndroidWebGPU.

pub mod error;
pub mod surface_bridge;
pub mod types;
pub mod window_manager;
pub mod window_session;

pub use error::{WmsError, WmsResult};
pub use surface_bridge::SurfaceBridge;
pub use types::{
    InsetsState, LayoutParams, Rect, SurfaceControl, SurfaceControlTransaction,
    FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS, FLAG_FORCE_NOT_FULLSCREEN, FLAG_FULLSCREEN,
    FLAG_HARDWARE_ACCELERATED, FORMAT_OPAQUE, FORMAT_RGBA_8888, FORMAT_RGBX_8888, FORMAT_RGB_565,
    FORMAT_TRANSLUCENT, FORMAT_TRANSPARENT, IWINDOW_DESCRIPTOR,
    IWINDOW_SESSION_CALLBACK_DESCRIPTOR, RELAYOUT_RES_FIRST_TIME, RELAYOUT_RES_IN_SETS_CHANGED,
    RELAYOUT_RES_SURFACE_CHANGED, TYPE_APPLICATION, TYPE_APPLICATION_STARTING,
    TYPE_BASE_APPLICATION, TYPE_NAVIGATION_BAR, TYPE_STATUS_BAR,
};
pub use window_manager::{
    iwindow_manager_codes, register_window_service, IWindowManager, WindowManagerProxy,
    WindowManagerService, IWINDOW_MANAGER_DESCRIPTOR,
};
pub use window_session::{
    get_active_session, iwindow_session_codes, register_active_session, IWindowSession,
    ManagedWindow, WindowSession, WindowSessionProxy, IWINDOW_SESSION_DESCRIPTOR,
};

#[cfg(test)]
mod tests {
    use super::*;
    use aidl_compat::pointer::SpIBinder;
    use input_channel::InputChannel;
    use std::sync::Arc;

    #[test]
    fn test_wms_open_session_and_layout_pipeline() {
        let wms = Arc::new(WindowManagerService::new());
        let wms_binder = SpIBinder::from_arc(wms.clone());
        let wms_client = WindowManagerProxy::new(wms_binder);

        // 1. Open Session via Client Proxy
        let session_binder = wms_client.open_session(None).expect("Open session failed");
        let session_client = WindowSessionProxy::new(session_binder);

        // 2. Add Window to Display
        let mut attrs = LayoutParams::default();
        attrs.title = "MainActivity".to_string();

        let mut insets = InsetsState::default();
        let mut client_input_channel = InputChannel::default();

        let add_res = session_client
            .add_to_display(None, &attrs, 0, 0, &mut insets, &mut client_input_channel)
            .expect("Add to display failed");

        assert_eq!(add_res, 0);
        assert_eq!(insets.display_frame.right, 1280);
        assert_eq!(insets.display_frame.bottom, 720);
        assert!(client_input_channel.name().contains("MainActivity") || client_input_channel.name().contains("win_1"));

        // 3. Relayout Window
        let mut surface_control = SurfaceControl::default();
        let relayout_res = session_client
            .relayout(None, &attrs, 1280, 720, 0, 0, &mut surface_control)
            .expect("Relayout failed");

        assert_ne!(relayout_res, 0);
        assert_eq!(surface_control.name, "MainActivity");
        assert_eq!(surface_control.width, 1280);
        assert_eq!(surface_control.height, 720);

        // 4. Finish Drawing
        let mut tx = SurfaceControlTransaction::new(surface_control.layer_id);
        tx.set_alpha(1.0).set_z_order(1);

        session_client
            .finish_drawing(None, Some(&tx))
            .expect("Finish drawing failed");

        // 5. Remove Window
        session_client.remove(None).expect("Remove window failed");
    }
}
