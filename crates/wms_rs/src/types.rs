//! Android Window Manager data types, parcelable structures, and interface definitions.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::{Interface, Parcelable};
use aidl_compat::Parcel;
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Layout Constants
// -----------------------------------------------------------------------------

pub const TYPE_BASE_APPLICATION: i32 = 1;
pub const TYPE_APPLICATION: i32 = 2;
pub const TYPE_APPLICATION_STARTING: i32 = 3;
pub const TYPE_STATUS_BAR: i32 = 2000;
pub const TYPE_NAVIGATION_BAR: i32 = 2019;

pub const FLAG_FULLSCREEN: i32 = 0x00000400;
pub const FLAG_FORCE_NOT_FULLSCREEN: i32 = 0x00000800;
pub const FLAG_HARDWARE_ACCELERATED: i32 = 0x01000000;
pub const FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS: i32 = -0x80000000;

pub const FORMAT_RGBA_8888: i32 = 1;
pub const FORMAT_RGBX_8888: i32 = 2;
pub const FORMAT_RGB_565: i32 = 4;
pub const FORMAT_TRANSLUCENT: i32 = -3;
pub const FORMAT_TRANSPARENT: i32 = -2;
pub const FORMAT_OPAQUE: i32 = -1;

pub const RELAYOUT_RES_IN_SETS_CHANGED: i32 = 1 << 0;
pub const RELAYOUT_RES_SURFACE_CHANGED: i32 = 1 << 1;
pub const RELAYOUT_RES_FIRST_TIME: i32 = 1 << 2;

// -----------------------------------------------------------------------------
// Rect Structure
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

impl Parcelable for Rect {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.left).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.top).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.right).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.bottom).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.left = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.top = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.right = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.bottom = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// LayoutParams
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutParams {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub type_: i32,
    pub flags: i32,
    pub format: i32,
    pub title: String,
    pub alpha: f32,
    pub dim_amount: f32,
    #[serde(skip)]
    pub token: Option<SpIBinder>,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: -1, // MATCH_PARENT
            height: -1,
            type_: TYPE_BASE_APPLICATION,
            flags: FLAG_HARDWARE_ACCELERATED,
            format: FORMAT_RGBA_8888,
            title: "MainWindow".to_string(),
            alpha: 1.0,
            dim_amount: 0.0,
            token: None,
        }
    }
}

impl Parcelable for LayoutParams {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.x).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.y).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.width).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.height).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.type_).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.flags).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.format).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_utf8(Some(&self.title)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.alpha).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.dim_amount).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref tok) = self.token {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let handle = tok.handle().unwrap_or(1);
            parcel.write_u32(handle).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.x = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.y = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.width = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.height = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.type_ = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.flags = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.format = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.title = parcel.read_utf8(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.alpha = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.dim_amount = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let has_token = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_token {
            let handle = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.token = Some(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)));
        } else {
            self.token = None;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// InsetsState
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InsetsState {
    pub display_frame: Rect,
    pub visible_insets: Rect,
    pub stable_insets: Rect,
}

impl InsetsState {
    pub fn new(display_width: i32, display_height: i32) -> Self {
        Self {
            display_frame: Rect::new(0, 0, display_width, display_height),
            visible_insets: Rect::new(0, 48, display_width, display_height - 96),
            stable_insets: Rect::new(0, 48, display_width, display_height - 96),
        }
    }
}

impl Parcelable for InsetsState {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        self.display_frame.write_to_parcel(parcel)?;
        self.visible_insets.write_to_parcel(parcel)?;
        self.stable_insets.write_to_parcel(parcel)?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.display_frame.read_from_parcel_at(parcel, offset)?;
        self.visible_insets.read_from_parcel_at(parcel, offset)?;
        self.stable_insets.read_from_parcel_at(parcel, offset)?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// SurfaceControl
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SurfaceControl {
    pub layer_id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub producer: Option<SpIBinder>,
}

impl SurfaceControl {
    pub fn new(layer_id: u64, name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            layer_id,
            name: name.into(),
            width,
            height,
            producer: None,
        }
    }
}

impl Parcelable for SurfaceControl {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_u64(self.layer_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_utf8(Some(&self.name)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.width).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.height).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref prod) = self.producer {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let handle = prod.handle().unwrap_or(1);
            parcel.write_u32(handle).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.layer_id = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.name = parcel.read_utf8(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.width = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.height = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let has_producer = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_producer {
            let handle = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.producer = Some(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)));
        } else {
            self.producer = None;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// SurfaceControlTransaction
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SurfaceControlTransaction {
    pub layer_id: u64,
    pub position: Option<[f32; 2]>,
    pub size: Option<[u32; 2]>,
    pub alpha: Option<f32>,
    pub z_order: Option<i32>,
    pub color: Option<[f32; 4]>,
    pub flags: u32,
}

impl SurfaceControlTransaction {
    pub fn new(layer_id: u64) -> Self {
        Self {
            layer_id,
            ..Default::default()
        }
    }

    pub fn set_position(&mut self, x: f32, y: f32) -> &mut Self {
        self.position = Some([x, y]);
        self
    }

    pub fn set_size(&mut self, w: u32, h: u32) -> &mut Self {
        self.size = Some([w, h]);
        self
    }

    pub fn set_alpha(&mut self, alpha: f32) -> &mut Self {
        self.alpha = Some(alpha);
        self
    }

    pub fn set_z_order(&mut self, z: i32) -> &mut Self {
        self.z_order = Some(z);
        self
    }

    pub fn set_color(&mut self, color: [f32; 4]) -> &mut Self {
        self.color = Some(color);
        self
    }
}

impl Parcelable for SurfaceControlTransaction {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_u64(self.layer_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(pos) = self.position {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(pos[0]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(pos[1]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        if let Some(sz) = self.size {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_u32(sz[0]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_u32(sz[1]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        if let Some(alpha) = self.alpha {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(alpha).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        if let Some(z) = self.z_order {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_i32(z).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        if let Some(col) = self.color {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            for c in &col {
                parcel.write_f32(*c).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            }
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        parcel.write_u32(self.flags).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.layer_id = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))? {
            let x = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let y = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.position = Some([x, y]);
        } else {
            self.position = None;
        }

        if parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))? {
            let w = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let h = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.size = Some([w, h]);
        } else {
            self.size = None;
        }

        if parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))? {
            let alpha = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.alpha = Some(alpha);
        } else {
            self.alpha = None;
        }

        if parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))? {
            let z = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.z_order = Some(z);
        } else {
            self.z_order = None;
        }

        if parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))? {
            let r = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let g = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let b = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let a = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.color = Some([r, g, b, a]);
        } else {
            self.color = None;
        }

        self.flags = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// IWindow and IWindowSessionCallback Traits
// -----------------------------------------------------------------------------

pub const IWINDOW_DESCRIPTOR: &str = "android.view.IWindow";
pub const IWINDOW_SESSION_CALLBACK_DESCRIPTOR: &str = "android.view.IWindowSessionCallback";

pub trait IWindow: Interface + Send + Sync {
    fn resized(&self, frame: &Rect, insets: &InsetsState) -> AidlResult<()>;
}

pub trait IWindowSessionCallback: Interface + Send + Sync {
    fn on_session_created(&self, session: SpIBinder) -> AidlResult<()>;
}
