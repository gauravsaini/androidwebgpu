//! Android `layer_state_t` / `ComposerState` representations and WebGPU layer translator.

use aidl_compat::{Parcel, Parcelable, Result, Status, STATUS_BAD_VALUE};
use serde::{Deserialize, Serialize};
use webgpu_compositor::{BlendMode, CompositionLayer};

/// Bitflags representing changed properties in an Android `layer_state_t`.
pub mod layer_change_flags {
    pub const POSITION_CHANGED: u64 = 1 << 0;
    pub const LAYER_CHANGED: u64 = 1 << 1; // Z-order
    pub const SIZE_CHANGED: u64 = 1 << 2;
    pub const ALPHA_CHANGED: u64 = 1 << 3;
    pub const MATRIX_CHANGED: u64 = 1 << 4;
    pub const CROP_CHANGED: u64 = 1 << 5;
    pub const COLOR_CHANGED: u64 = 1 << 6;
    pub const DAMAGE_CHANGED: u64 = 1 << 7;
    pub const TRANSFORM_CHANGED: u64 = 1 << 8;
    pub const BLEND_MODE_CHANGED: u64 = 1 << 9;
    pub const VISIBILITY_CHANGED: u64 = 1 << 10;
}

/// Comprehensive Android layer state representation corresponding to AOSP `layer_state_t`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerState {
    /// Mask of changed fields (using `layer_change_flags`).
    pub what: u64,
    /// Surface / Layer identifier.
    pub surface_id: u64,
    /// Layer debug name.
    pub name: String,
    /// Bounding rectangle `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// Whether bounds are already in Normalized Device Coordinates `[-1.0..1.0]`.
    pub is_ndc: bool,
    /// Source UV crop rectangle `[u_min, v_min, u_max, v_max]`.
    pub source_crop: [f32; 4],
    /// 2D Transform affine vector `[scale_x, scale_y, trans_x, trans_y]`.
    pub transform: [f32; 4],
    /// Android HWC transform flag (0=None, 1=FLIP_H, 2=FLIP_V, 4=ROT_90, 3=ROT_180, 7=ROT_270).
    pub hwc_transform: u32,
    /// Scissor damage rect in pixel coordinates `[x, y, width, height]`.
    pub damage_rect: Option<[f32; 4]>,
    /// Stacking order (higher renders on top).
    pub z_order: i32,
    /// Global opacity multiplier `[0.0, 1.0]`.
    pub alpha: f32,
    /// Blending mode.
    pub blend_mode: BlendMode,
    /// Solid RGBA fill color (if not using textured buffer).
    pub color: Option<[f32; 4]>,
    /// Layer visibility flag.
    pub visible: bool,
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            what: 0,
            surface_id: 0,
            name: String::new(),
            bounds: [-1.0, -1.0, 2.0, 2.0],
            is_ndc: true,
            source_crop: [0.0, 0.0, 1.0, 1.0],
            transform: [1.0, 1.0, 0.0, 0.0],
            hwc_transform: 0,
            damage_rect: None,
            z_order: 0,
            alpha: 1.0,
            blend_mode: BlendMode::Premultiplied,
            color: None,
            visible: true,
        }
    }
}

impl LayerState {
    /// Construct a new empty layer state for a surface.
    pub fn new(surface_id: u64, name: &str) -> Self {
        Self {
            surface_id,
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Set solid color quad parameters.
    pub fn set_color(&mut self, color: [f32; 4]) -> &mut Self {
        self.what |= layer_change_flags::COLOR_CHANGED | layer_change_flags::ALPHA_CHANGED;
        self.color = Some(color);
        self.alpha = color[3];
        self
    }

    /// Set NDC bounds `[x, y, w, h]`.
    pub fn set_bounds_ndc(&mut self, bounds: [f32; 4]) -> &mut Self {
        self.what |= layer_change_flags::POSITION_CHANGED | layer_change_flags::SIZE_CHANGED;
        self.bounds = bounds;
        self.is_ndc = true;
        self
    }

    /// Set pixel bounds `[x, y, w, h]`.
    pub fn set_bounds_pixels(&mut self, bounds: [f32; 4]) -> &mut Self {
        self.what |= layer_change_flags::POSITION_CHANGED | layer_change_flags::SIZE_CHANGED;
        self.bounds = bounds;
        self.is_ndc = false;
        self
    }

    /// Set z-order.
    pub fn set_z_order(&mut self, z_order: i32) -> &mut Self {
        self.what |= layer_change_flags::LAYER_CHANGED;
        self.z_order = z_order;
        self
    }

    /// Set alpha.
    pub fn set_alpha(&mut self, alpha: f32) -> &mut Self {
        self.what |= layer_change_flags::ALPHA_CHANGED;
        self.alpha = alpha;
        self
    }

    /// Set HWC transform flags.
    pub fn set_hwc_transform(&mut self, hwc_transform: u32) -> &mut Self {
        self.what |= layer_change_flags::TRANSFORM_CHANGED;
        self.hwc_transform = hwc_transform;
        self
    }

    /// Set source crop.
    pub fn set_source_crop(&mut self, crop: [f32; 4]) -> &mut Self {
        self.what |= layer_change_flags::CROP_CHANGED;
        self.source_crop = crop;
        self
    }

    /// Set blend mode.
    pub fn set_blend_mode(&mut self, blend_mode: BlendMode) -> &mut Self {
        self.what |= layer_change_flags::BLEND_MODE_CHANGED;
        self.blend_mode = blend_mode;
        self
    }

    /// Set damage rect.
    pub fn set_damage_rect(&mut self, damage: Option<[f32; 4]>) -> &mut Self {
        self.what |= layer_change_flags::DAMAGE_CHANGED;
        self.damage_rect = damage;
        self
    }
}

impl Parcelable for LayerState {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel.write_u64(self.what).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u64(self.surface_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_utf8(Some(&self.name)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.bounds[0]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.bounds[1]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.bounds[2]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.bounds[3]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_bool(self.is_ndc).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.source_crop[0]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.source_crop[1]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.source_crop[2]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.source_crop[3]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.transform[0]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.transform[1]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.transform[2]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.transform[3]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.hwc_transform).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.z_order).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.alpha).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let blend_code = match self.blend_mode {
            BlendMode::None => 0i32,
            BlendMode::Premultiplied => 1i32,
            BlendMode::Coverage => 2i32,
        };
        parcel.write_i32(blend_code).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(col) = self.color {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(col[0]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(col[1]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(col[2]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(col[3]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        if let Some(dmg) = self.damage_rect {
            parcel.write_bool(true).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(dmg[0]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(dmg[1]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(dmg[2]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            parcel.write_f32(dmg[3]).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            parcel.write_bool(false).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        parcel.write_bool(self.visible).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        self.what = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.surface_id = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.name = parcel.read_utf8(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?.unwrap_or_default();
        self.bounds[0] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.bounds[1] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.bounds[2] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.bounds[3] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.is_ndc = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.source_crop[0] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.source_crop[1] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.source_crop[2] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.source_crop[3] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.transform[0] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.transform[1] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.transform[2] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.transform[3] = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.hwc_transform = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.z_order = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.alpha = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let blend_code = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.blend_mode = match blend_code {
            0 => BlendMode::None,
            1 => BlendMode::Premultiplied,
            2 => BlendMode::Coverage,
            _ => BlendMode::Premultiplied,
        };

        let has_color = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_color {
            let r = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let g = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let b = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let a = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.color = Some([r, g, b, a]);
        } else {
            self.color = None;
        }

        let has_damage = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_damage {
            let dx = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let dy = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let dw = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let dh = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            self.damage_rect = Some([dx, dy, dw, dh]);
        } else {
            self.damage_rect = None;
        }

        self.visible = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

/// AOSP `ComposerState` wrapper associating a surface ID with a `LayerState`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposerState {
    /// Surface identifier.
    pub surface_id: u64,
    /// Detailed layer state update.
    pub state: LayerState,
}

impl ComposerState {
    /// Construct a new composer state.
    pub fn new(surface_id: u64, state: LayerState) -> Self {
        Self { surface_id, state }
    }
}

impl Parcelable for ComposerState {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel.write_u64(self.surface_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.state.write_to_parcel(parcel)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        self.surface_id = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.state.read_from_parcel_at(parcel, offset)
    }
}

/// Translator mapping Android LayerState and screen dimensions to WebGPU `CompositionLayer`.
pub struct LayerTranslator;

impl LayerTranslator {
    /// Convert pixel coordinate rectangle `[x, y, w, h]` into Normalized Device Coordinates `[ndc_x, ndc_y, ndc_w, ndc_h]`.
    pub fn screen_coords_to_ndc(bounds: [f32; 4], screen_width: u32, screen_height: u32) -> [f32; 4] {
        let sw = screen_width.max(1) as f32;
        let sh = screen_height.max(1) as f32;

        let ndc_x = (bounds[0] / sw) * 2.0 - 1.0;
        let ndc_y = 1.0 - ((bounds[1] + bounds[3]) / sh) * 2.0;
        let ndc_w = (bounds[2] / sw) * 2.0;
        let ndc_h = (bounds[3] / sh) * 2.0;

        [ndc_x, ndc_y, ndc_w, ndc_h]
    }

    /// Translate a `LayerState` into a standalone WebGPU `CompositionLayer`.
    pub fn translate_to_composition_layer(
        state: &LayerState,
        screen_width: u32,
        screen_height: u32,
        texture_view: Option<wgpu::TextureView>,
    ) -> CompositionLayer {
        let bounds = if state.is_ndc {
            state.bounds
        } else {
            Self::screen_coords_to_ndc(state.bounds, screen_width, screen_height)
        };

        CompositionLayer {
            id: state.surface_id,
            name: state.name.clone(),
            bounds,
            source_crop: state.source_crop,
            transform: state.transform,
            hwc_transform: state.hwc_transform,
            damage_rect: state.damage_rect,
            z_order: state.z_order,
            alpha: state.alpha,
            blend_mode: state.blend_mode,
            color: state.color,
            texture_view,
            visible: state.visible,
        }
    }

    /// Apply incremental updates from `LayerState` to an existing `CompositionLayer`.
    pub fn apply_state_update(
        layer: &mut CompositionLayer,
        state: &LayerState,
        screen_width: u32,
        screen_height: u32,
    ) {
        if state.what == 0 {
            // Apply all fields if what mask is not explicitly populated
            layer.bounds = if state.is_ndc {
                state.bounds
            } else {
                Self::screen_coords_to_ndc(state.bounds, screen_width, screen_height)
            };
            layer.source_crop = state.source_crop;
            layer.transform = state.transform;
            layer.hwc_transform = state.hwc_transform;
            layer.damage_rect = state.damage_rect;
            layer.z_order = state.z_order;
            layer.alpha = state.alpha;
            layer.blend_mode = state.blend_mode;
            layer.color = state.color;
            layer.visible = state.visible;
            return;
        }

        if (state.what & (layer_change_flags::POSITION_CHANGED | layer_change_flags::SIZE_CHANGED)) != 0 {
            layer.bounds = if state.is_ndc {
                state.bounds
            } else {
                Self::screen_coords_to_ndc(state.bounds, screen_width, screen_height)
            };
        }

        if (state.what & layer_change_flags::LAYER_CHANGED) != 0 {
            layer.z_order = state.z_order;
        }

        if (state.what & layer_change_flags::ALPHA_CHANGED) != 0 {
            layer.alpha = state.alpha;
        }

        if (state.what & layer_change_flags::TRANSFORM_CHANGED) != 0 {
            layer.hwc_transform = state.hwc_transform;
        }

        if (state.what & layer_change_flags::MATRIX_CHANGED) != 0 {
            layer.transform = state.transform;
        }

        if (state.what & layer_change_flags::CROP_CHANGED) != 0 {
            layer.source_crop = state.source_crop;
        }

        if (state.what & layer_change_flags::COLOR_CHANGED) != 0 {
            layer.color = state.color;
        }

        if (state.what & layer_change_flags::BLEND_MODE_CHANGED) != 0 {
            layer.blend_mode = state.blend_mode;
        }

        if (state.what & layer_change_flags::DAMAGE_CHANGED) != 0 {
            layer.damage_rect = state.damage_rect;
        }

        if (state.what & layer_change_flags::VISIBILITY_CHANGED) != 0 {
            layer.visible = state.visible;
        }
    }
}
