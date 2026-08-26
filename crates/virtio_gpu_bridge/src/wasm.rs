#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct WasmVirtioGpuBridge {
    bridge: Option<crate::bridge::VirtioGpuBridge>,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl WasmVirtioGpuBridge {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        #[cfg(feature = "wasm")]
        console_error_panic_hook::set_once();
        Self { bridge: None }
    }

    #[wasm_bindgen]
    pub async fn initialize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
        #[cfg(feature = "wasm")]
        console_error_panic_hook::set_once();
        let bridge = crate::bridge::VirtioGpuBridge::new(width, height)
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        self.bridge = Some(bridge);
        Ok(())
    }

    #[wasm_bindgen]
    pub fn process_command_packet(&mut self, packet: &[u8]) -> Vec<u8> {
        if let Some(bridge) = &mut self.bridge {
            bridge.process_binary_wire_command(packet)
        } else {
            Vec::new()
        }
    }

    #[wasm_bindgen]
    pub fn process_binder_packet(&self, packet: &[u8]) -> Vec<u8> {
        if let Some(bridge) = &self.bridge {
            bridge.process_binder_packet(packet)
        } else {
            Vec::new()
        }
    }

    #[wasm_bindgen]
    pub fn get_scanout_framebuffer(&self, scanout_id: u32) -> Vec<u8> {
        if let Some(bridge) = &self.bridge {
            bridge.get_scanout_framebuffer(scanout_id).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    #[wasm_bindgen]
    pub fn get_scanout_damage(&self, scanout_id: u32) -> Option<Vec<u32>> {
        if let Some(bridge) = &self.bridge {
            bridge.get_scanout_damage(scanout_id).map(|d| d.to_vec())
        } else {
            None
        }
    }

    #[wasm_bindgen]
    pub fn clear_scanout_damage(&mut self, scanout_id: u32) {
        if let Some(bridge) = &mut self.bridge {
            bridge.clear_scanout_damage(scanout_id);
        }
    }

    #[wasm_bindgen]
    pub fn compose_and_present(&mut self) -> Result<u64, JsValue> {
        if let Some(bridge) = &self.bridge {
            if let Some(sf) = &bridge.surface_composer {
                sf.compose_and_present()
                    .map_err(|e| JsValue::from_str(&e.to_string()))
            } else {
                Err(JsValue::from_str("SurfaceComposer service not initialized"))
            }
        } else {
            Err(JsValue::from_str("Bridge not initialized"))
        }
    }

    #[wasm_bindgen]
    pub fn is_boot_finished(&self) -> bool {
        if let Some(bridge) = &self.bridge {
            if let Some(sf) = &bridge.surface_composer {
                sf.is_boot_finished()
            } else {
                false
            }
        } else {
            false
        }
    }

    #[wasm_bindgen]
    pub fn set_boot_finished(&mut self, finished: bool) {
        if let Some(bridge) = &mut self.bridge {
            if let Some(sf) = &bridge.surface_composer {
                sf.set_boot_finished(finished);
            }
        }
    }

    #[wasm_bindgen]
    pub fn has_surface_composer(&self) -> bool {
        self.bridge
            .as_ref()
            .and_then(|b| b.surface_composer.as_ref())
            .is_some()
    }

    #[wasm_bindgen]
    pub fn update_status_bar_layer(&mut self, r: f32, g: f32, b: f32, a: f32) {
        if let Some(bridge) = &mut self.bridge {
            if let Some(sf) = &bridge.surface_composer {
                let layer = webgpu_compositor::CompositionLayer::new_color(
                    9999,
                    "StatusBar",
                    [-1.0, 0.88, 2.0, 0.12],
                    9999,
                    [r, g, b, a],
                );
                sf.add_or_update_composition_layer(layer);
            }
        }
    }
}



