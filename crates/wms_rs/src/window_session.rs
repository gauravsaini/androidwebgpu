//! AIDL `IWindowSession` Interface, Remotable Server Stub, and Client Proxy.

use crate::surface_bridge::SurfaceBridge;
use crate::types::*;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use input_channel::channel::InputChannel;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub const IWINDOW_SESSION_DESCRIPTOR: &str = "android.view.IWindowSession";

static ACTIVE_SESSIONS: RwLock<Option<HashMap<u32, Arc<WindowSession>>>> = RwLock::new(None);

pub fn register_active_session(session: Arc<WindowSession>) {
    let mut lock = ACTIVE_SESSIONS.write().unwrap();
    if lock.is_none() {
        *lock = Some(HashMap::new());
    }
    lock.as_mut().unwrap().insert(session.session_id, session);
}

pub fn get_active_session(session_id: u32) -> Option<Arc<WindowSession>> {
    ACTIVE_SESSIONS
        .read()
        .unwrap()
        .as_ref()
        .and_then(|map| map.get(&session_id).cloned())
}

// -----------------------------------------------------------------------------
// Transaction Opcodes
// -----------------------------------------------------------------------------

pub mod iwindow_session_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const ADD_TO_DISPLAY: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const RELAYOUT: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const FINISH_DRAWING: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const REMOVE: u32 = FIRST_CALL_TRANSACTION + 3; // 4
}

// -----------------------------------------------------------------------------
// IWindowSession Trait Definition
// -----------------------------------------------------------------------------

pub trait IWindowSession: Interface + Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn add_to_display(
        &self,
        window: Option<SpIBinder>,
        attrs: &LayoutParams,
        view_visibility: i32,
        display_id: i32,
        out_insets_state: &mut InsetsState,
        out_input_channel: &mut InputChannel,
    ) -> AidlResult<i32>;

    #[allow(clippy::too_many_arguments)]
    fn relayout(
        &self,
        window: Option<SpIBinder>,
        attrs: &LayoutParams,
        width: i32,
        height: i32,
        view_visibility: i32,
        flags: i32,
        out_surface_control: &mut SurfaceControl,
    ) -> AidlResult<i32>;

    fn finish_drawing(
        &self,
        window: Option<SpIBinder>,
        post_draw_transaction: Option<&SurfaceControlTransaction>,
    ) -> AidlResult<()>;

    fn remove(&self, window: Option<SpIBinder>) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// Window Record Tracking
// -----------------------------------------------------------------------------

pub struct ManagedWindow {
    pub window_token: Option<SpIBinder>,
    pub title: String,
    pub attrs: LayoutParams,
    pub visibility: i32,
    pub display_id: i32,
    pub surface_control: Option<SurfaceControl>,
    pub input_channel_server: Option<InputChannel>,
    pub drawn: bool,
}

// -----------------------------------------------------------------------------
// WindowSession Server Implementation
// -----------------------------------------------------------------------------

pub struct WindowSession {
    session_id: u32,
    surface_bridge: Arc<SurfaceBridge>,
    windows: Arc<Mutex<HashMap<u64, ManagedWindow>>>,
    next_window_id: AtomicU32,
    display_width: i32,
    display_height: i32,
}

impl WindowSession {
    pub fn new(session_id: u32, surface_bridge: Arc<SurfaceBridge>) -> Self {
        Self {
            session_id,
            surface_bridge,
            windows: Arc::new(Mutex::new(HashMap::new())),
            next_window_id: AtomicU32::new(1),
            display_width: 1280,
            display_height: 720,
        }
    }

    pub fn session_id(&self) -> u32 {
        self.session_id
    }

    pub fn set_display_metrics(&mut self, width: i32, height: i32) {
        self.display_width = width;
        self.display_height = height;
    }

    pub fn get_window_count(&self) -> usize {
        self.windows.lock().unwrap().len()
    }

    pub fn get_server_input_channel(&self, window_id: u64) -> Option<InputChannel> {
        self.windows
            .lock()
            .unwrap()
            .get(&window_id)
            .and_then(|w| w.input_channel_server.clone())
    }

    fn clone_internal(&self) -> Self {
        Self {
            session_id: self.session_id,
            surface_bridge: Arc::clone(&self.surface_bridge),
            windows: Arc::clone(&self.windows),
            next_window_id: AtomicU32::new(self.next_window_id.load(Ordering::SeqCst)),
            display_width: self.display_width,
            display_height: self.display_height,
        }
    }
}

impl Interface for WindowSession {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::from_arc(Arc::new(self.clone_internal()))
    }
}

impl IWindowSession for WindowSession {
    #[allow(clippy::too_many_arguments)]
    fn add_to_display(
        &self,
        window: Option<SpIBinder>,
        attrs: &LayoutParams,
        view_visibility: i32,
        display_id: i32,
        out_insets_state: &mut InsetsState,
        out_input_channel: &mut InputChannel,
    ) -> AidlResult<i32> {
        let win_id = self.next_window_id.fetch_add(1, Ordering::SeqCst) as u64;
        let channel_name = format!("win_{}_{}", self.session_id, win_id);

        let (server_chan, client_chan) = InputChannel::open_input_channel_pair(&channel_name)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        *out_insets_state = InsetsState::new(self.display_width, self.display_height);
        *out_input_channel = client_chan;

        let managed = ManagedWindow {
            window_token: window,
            title: attrs.title.clone(),
            attrs: attrs.clone(),
            visibility: view_visibility,
            display_id,
            surface_control: None,
            input_channel_server: Some(server_chan),
            drawn: false,
        };

        self.windows.lock().unwrap().insert(win_id, managed);
        Ok(0) // ADD_OKAY
    }

    #[allow(clippy::too_many_arguments)]
    fn relayout(
        &self,
        _window: Option<SpIBinder>,
        attrs: &LayoutParams,
        width: i32,
        height: i32,
        _view_visibility: i32,
        flags: i32,
        out_surface_control: &mut SurfaceControl,
    ) -> AidlResult<i32> {
        let effective_w = if width > 0 {
            width as u32
        } else {
            self.display_width as u32
        };
        let effective_h = if height > 0 {
            height as u32
        } else {
            self.display_height as u32
        };

        let sc = self
            .surface_bridge
            .allocate_surface(&attrs.title, effective_w, effective_h, flags as u32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        *out_surface_control = sc.clone();

        // Update in windows list
        let mut wins = self.windows.lock().unwrap();
        if let Some(win) = wins.values_mut().find(|w| w.title == attrs.title) {
            win.surface_control = Some(sc);
        } else if let Some(win) = wins.values_mut().next() {
            win.surface_control = Some(sc);
        }

        Ok(RELAYOUT_RES_SURFACE_CHANGED | RELAYOUT_RES_FIRST_TIME)
    }

    fn finish_drawing(
        &self,
        _window: Option<SpIBinder>,
        post_draw_transaction: Option<&SurfaceControlTransaction>,
    ) -> AidlResult<()> {
        if let Some(tx) = post_draw_transaction {
            self.surface_bridge
                .apply_transaction(tx)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        let mut wins = self.windows.lock().unwrap();
        for win in wins.values_mut() {
            win.drawn = true;
        }

        Ok(())
    }

    fn remove(&self, _window: Option<SpIBinder>) -> AidlResult<()> {
        let mut wins = self.windows.lock().unwrap();
        for win in wins.values() {
            if let Some(ref sc) = win.surface_control {
                let _ = self.surface_bridge.destroy_surface(sc.layer_id);
            }
        }
        wins.clear();
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Remotable and IBinder Implementations for WindowSession
// -----------------------------------------------------------------------------

impl Remotable for WindowSession {
    fn get_class_descriptor() -> &'static str {
        IWINDOW_SESSION_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply.write_utf8(Some(IWINDOW_SESSION_DESCRIPTOR)).unwrap();
                Ok(())
            }
            iwindow_session_codes::ADD_TO_DISPLAY => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IWINDOW_SESSION_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let has_win = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let window_token = if has_win {
                    let handle = data
                        .read_u32(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    Some(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)))
                } else {
                    None
                };

                let mut attrs = LayoutParams::default();
                attrs.read_from_parcel_at(data, &mut offset)?;

                let view_visibility = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let display_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let mut insets = InsetsState::default();
                let mut channel = InputChannel::default();

                let res = self.add_to_display(
                    window_token,
                    &attrs,
                    view_visibility,
                    display_id,
                    &mut insets,
                    &mut channel,
                );

                match res {
                    Ok(ret) => {
                        reply.write_status(&Status::ok()).unwrap();
                        reply.write_i32(ret).unwrap();
                        insets.write_to_parcel(reply)?;
                        channel.write_to_parcel(reply)?;
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            iwindow_session_codes::RELAYOUT => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IWINDOW_SESSION_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let has_win = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let window_token = if has_win {
                    let handle = data
                        .read_u32(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    Some(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)))
                } else {
                    None
                };

                let mut attrs = LayoutParams::default();
                attrs.read_from_parcel_at(data, &mut offset)?;

                let width = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let height = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let view_visibility = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let flags = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let mut sc = SurfaceControl::default();
                let res = self.relayout(
                    window_token,
                    &attrs,
                    width,
                    height,
                    view_visibility,
                    flags,
                    &mut sc,
                );

                match res {
                    Ok(ret) => {
                        reply.write_status(&Status::ok()).unwrap();
                        reply.write_i32(ret).unwrap();
                        sc.write_to_parcel(reply)?;
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            iwindow_session_codes::FINISH_DRAWING => {
                let mut offset = 0;
                let _ = data.read_utf16(&mut offset);

                let has_win = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let window_token = if has_win {
                    let handle = data
                        .read_u32(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    Some(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)))
                } else {
                    None
                };

                let has_tx = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let tx = if has_tx {
                    let mut t = SurfaceControlTransaction::default();
                    t.read_from_parcel_at(data, &mut offset)?;
                    Some(t)
                } else {
                    None
                };

                let res = self.finish_drawing(window_token, tx.as_ref());
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            iwindow_session_codes::REMOVE => {
                let mut offset = 0;
                let _ = data.read_utf16(&mut offset);

                let has_win = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let window_token = if has_win {
                    let handle = data
                        .read_u32(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    Some(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)))
                } else {
                    None
                };

                let res = self.remove(window_token);
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            _ => {
                reply
                    .write_status(&Status::from_status(STATUS_UNKNOWN_TRANSACTION))
                    .unwrap();
                Ok(())
            }
        }
    }
}

impl IBinder for WindowSession {
    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn ping_binder(&self) -> AidlResult<()> {
        Ok(())
    }

    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply.write_utf16(Some(IWINDOW_SESSION_DESCRIPTOR)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => self.on_transact(code, data, reply),
        }
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(IWINDOW_SESSION_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// WindowSessionProxy Client Implementation
// -----------------------------------------------------------------------------

pub struct WindowSessionProxy {
    binder: SpIBinder,
}

impl WindowSessionProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for WindowSessionProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for WindowSessionProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IWindowSession for WindowSessionProxy {
    #[allow(clippy::too_many_arguments)]
    fn add_to_display(
        &self,
        window: Option<SpIBinder>,
        attrs: &LayoutParams,
        view_visibility: i32,
        display_id: i32,
        out_insets_state: &mut InsetsState,
        out_input_channel: &mut InputChannel,
    ) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IWINDOW_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref win) = window {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let handle = win.handle().unwrap_or(1);
            data.write_u32(handle)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        attrs.write_to_parcel(&mut data)?;
        data.write_i32(view_visibility)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(display_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(iwindow_session_codes::ADD_TO_DISPLAY, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let ret = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        out_insets_state.read_from_parcel_at(&reply, &mut offset)?;
        out_input_channel.read_from_parcel_at(&reply, &mut offset)?;

        Ok(ret)
    }

    #[allow(clippy::too_many_arguments)]
    fn relayout(
        &self,
        window: Option<SpIBinder>,
        attrs: &LayoutParams,
        width: i32,
        height: i32,
        view_visibility: i32,
        flags: i32,
        out_surface_control: &mut SurfaceControl,
    ) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IWINDOW_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref win) = window {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let handle = win.handle().unwrap_or(1);
            data.write_u32(handle)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        attrs.write_to_parcel(&mut data)?;
        data.write_i32(width)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(height)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(view_visibility)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(iwindow_session_codes::RELAYOUT, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let ret = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        out_surface_control.read_from_parcel_at(&reply, &mut offset)?;

        Ok(ret)
    }

    fn finish_drawing(
        &self,
        window: Option<SpIBinder>,
        post_draw_transaction: Option<&SurfaceControlTransaction>,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IWINDOW_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref win) = window {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let handle = win.handle().unwrap_or(1);
            data.write_u32(handle)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        if let Some(tx) = post_draw_transaction {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            tx.write_to_parcel(&mut data)?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        let mut reply = Parcel::new();
        self.binder
            .transact(iwindow_session_codes::FINISH_DRAWING, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        Ok(())
    }

    fn remove(&self, window: Option<SpIBinder>) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IWINDOW_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref win) = window {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let handle = win.handle().unwrap_or(1);
            data.write_u32(handle)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        let mut reply = Parcel::new();
        self.binder
            .transact(iwindow_session_codes::REMOVE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        Ok(())
    }
}
