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
    pub fn get_scanout_framebuffer(&self, scanout_id: u32) -> Vec<u8> {
        if let Some(bridge) = &self.bridge {
            bridge.get_scanout_framebuffer(scanout_id).unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

