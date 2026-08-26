#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;
use std::rc::Rc;
use std::cell::RefCell;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = debug)]
    fn console_debug(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(s: &str);
}

#[cfg(feature = "wasm")]
pub fn wasm_log(subsystem: &str, level: &str, message: &str) {
    let formatted = format!("[{}] [{}] {}", subsystem, level, message);
    match level {
        "E" => console_error(&formatted),
        "W" => console_warn(&formatted),
        "D" | "V" => console_debug(&formatted),
        _ => console_log(&formatted),
    }
}

#[cfg(not(feature = "wasm"))]
pub fn wasm_log(_subsystem: &str, _level: &str, _message: &str) {}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct WasmVirtioGpuBridge {
    bridge: Rc<RefCell<Option<crate::bridge::VirtioGpuBridge>>>,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl WasmVirtioGpuBridge {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        #[cfg(feature = "wasm")]
        console_error_panic_hook::set_once();
        Self {
            bridge: Rc::new(RefCell::new(None)),
        }
    }

    #[wasm_bindgen]
    pub fn log_bridge(&self, level: &str, message: &str) {
        wasm_log("bridge", level, message);
    }

    #[wasm_bindgen]
    pub fn log_compositor(&self, level: &str, message: &str) {
        wasm_log("compositor", level, message);
    }

    #[wasm_bindgen]
    pub async fn initialize(&self, width: u32, height: u32) -> Result<(), JsValue> {
        #[cfg(feature = "wasm")]
        console_error_panic_hook::set_once();
        wasm_log("bridge", "I", &format!("Virtio-GPU Bridge initialized (viewport: {}x{})", width, height));
        let b = crate::bridge::VirtioGpuBridge::new(width, height)
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        if let Ok(mut cell) = self.bridge.try_borrow_mut() {
            *cell = Some(b);
        }
        Ok(())
    }

    #[wasm_bindgen]
    pub fn process_command_packet(&self, packet: &[u8]) -> Vec<u8> {
        wasm_log("bridge", "D", &format!("Processing Virtio-GPU wire command packet ({} bytes)", packet.len()));
        if let Ok(mut cell) = self.bridge.try_borrow_mut() {
            if let Some(bridge) = cell.as_mut() {
                return bridge.process_binary_wire_command(packet);
            }
        }
        Vec::new()
    }

    #[wasm_bindgen]
    pub fn process_binder_packet(&self, packet: &[u8]) -> Vec<u8> {
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                return bridge.process_binder_packet(packet);
            }
        }
        Vec::new()
    }

    #[wasm_bindgen]
    pub fn swizzle_bgrx_to_rgba(&self, bgrx: &[u8]) -> Vec<u8> {
        crate::bridge::swizzle_bgrx_to_rgba(bgrx)
    }

    #[wasm_bindgen]
    pub fn get_scanout_format(&self, scanout_id: u32) -> u32 {
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                return bridge.get_scanout_format(scanout_id);
            }
        }
        0
    }

    #[wasm_bindgen]
    pub fn get_scanout_framebuffer_rgba(&self, scanout_id: u32) -> Vec<u8> {
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                return bridge.get_scanout_framebuffer_rgba(scanout_id).unwrap_or_default();
            }
        }
        Vec::new()
    }

    #[wasm_bindgen]
    pub fn get_scanout_framebuffer(&self, scanout_id: u32) -> Vec<u8> {
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                return bridge.get_scanout_framebuffer(scanout_id).unwrap_or_default();
            }
        }
        Vec::new()
    }

    #[wasm_bindgen]
    pub fn get_scanout_damage(&self, scanout_id: u32) -> Option<Vec<u32>> {
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                return bridge.get_scanout_damage(scanout_id).map(|d| d.to_vec());
            }
        }
        None
    }

    #[wasm_bindgen]
    pub fn clear_scanout_damage(&self, scanout_id: u32) {
        if let Ok(mut cell) = self.bridge.try_borrow_mut() {
            if let Some(bridge) = cell.as_mut() {
                bridge.clear_scanout_damage(scanout_id);
            }
        }
    }

    #[wasm_bindgen]
    pub fn compose_and_present(&self) -> Result<u64, JsValue> {
        wasm_log("compositor", "D", "WebGPU render pass submitted and presented");
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                if let Some(sf) = &bridge.surface_composer {
                    return sf.compose_and_present()
                        .map_err(|e| JsValue::from_str(&e.to_string()));
                } else {
                    return Err(JsValue::from_str("SurfaceComposer service not initialized"));
                }
            } else {
                return Err(JsValue::from_str("Bridge not initialized"));
            }
        }
        Err(JsValue::from_str("Bridge cell busy"))
    }

    #[wasm_bindgen]
    pub fn is_boot_finished(&self) -> bool {
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                if let Some(sf) = &bridge.surface_composer {
                    return sf.is_boot_finished();
                }
            }
        }
        false
    }

    #[wasm_bindgen]
    pub fn set_boot_finished(&self, finished: bool) {
        wasm_log("compositor", "I", &format!("SurfaceFlinger boot state changed (boot_finished: {})", finished));
        if let Ok(mut cell) = self.bridge.try_borrow_mut() {
            if let Some(bridge) = cell.as_mut() {
                if let Some(sf) = &bridge.surface_composer {
                    sf.set_boot_finished(finished);
                }
            }
        }
    }

    #[wasm_bindgen]
    pub fn has_surface_composer(&self) -> bool {
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                return bridge.surface_composer.is_some();
            }
        }
        false
    }

    #[wasm_bindgen]
    pub fn update_status_bar_layer(&self, r: f32, g: f32, b: f32, a: f32) {
        wasm_log("compositor", "D", "StatusBar composition layer updated");
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
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

    #[wasm_bindgen]
    pub fn enable_system_ui(&self) {
        wasm_log("compositor", "I", "SystemUI navigation and status bars enabled");
        if let Ok(cell) = self.bridge.try_borrow() {
            if let Some(bridge) = cell.as_ref() {
                if let Some(sf) = &bridge.surface_composer {
                    sf.enable_system_ui();
                }
            }
        }
    }
}



