use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlendMode {
    None,
    Premultiplied,
    Coverage,
}

#[derive(Debug, Clone)]
pub struct CompositionLayer {
    pub id: u64,
    pub name: String,
    pub bounds: [f32; 4], // [x, y, width, height] in normalized device coordinates
    pub source_crop: [f32; 4], // [u_min, v_min, u_max, v_max]
    pub transform: [f32; 4], // [scale_x, scale_y, trans_x, trans_y]
    pub hwc_transform: u32,  // Android HWC_TRANSFORM_* (0=None, 1=FLIP_H, 2=FLIP_V, 4=ROT_90, 3=ROT_180, 7=ROT_270)
    pub damage_rect: Option<[f32; 4]>,
    pub z_order: i32,
    pub alpha: f32,
    pub blend_mode: BlendMode,
    pub color: Option<[f32; 4]>,
    pub texture_view: Option<wgpu::TextureView>,
    pub visible: bool,
}

impl CompositionLayer {
    pub fn new_color(
        id: u64,
        name: &str,
        bounds: [f32; 4],
        z_order: i32,
        color: [f32; 4],
    ) -> Self {
        Self {
            id,
            name: name.to_string(),
            bounds,
            source_crop: [0.0, 0.0, 1.0, 1.0],
            transform: [1.0, 1.0, 0.0, 0.0],
            hwc_transform: 0,
            damage_rect: None,
            z_order,
            alpha: color[3],
            blend_mode: BlendMode::Premultiplied,
            color: Some(color),
            texture_view: None,
            visible: true,
        }
    }

    pub fn new_textured(
        id: u64,
        name: &str,
        bounds: [f32; 4],
        z_order: i32,
        alpha: f32,
        texture_view: wgpu::TextureView,
    ) -> Self {
        Self {
            id,
            name: name.to_string(),
            bounds,
            source_crop: [0.0, 0.0, 1.0, 1.0],
            transform: [1.0, 1.0, 0.0, 0.0],
            hwc_transform: 0,
            damage_rect: None,
            z_order,
            alpha,
            blend_mode: BlendMode::Premultiplied,
            color: None,
            texture_view: Some(texture_view),
            visible: true,
        }
    }
}
