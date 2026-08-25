//! Surface lifecycle bridge routing WMS window surfaces to the host SurfaceFlinger compositor.

use crate::error::{WmsError, WmsResult};
use crate::types::{SurfaceControl, SurfaceControlTransaction};
use aidl_compat::pointer::SpIBinder;
use aidl_compat::IBinder;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use surfaceflinger_gpu_service::layer_translator::{ComposerState, LayerState};
use surfaceflinger_gpu_service::service::SurfaceComposerService;

/// SurfaceBridge connects WMS window sessions to the SurfaceFlinger GPU compositor.
pub struct SurfaceBridge {
    compositor_service: Option<Arc<SurfaceComposerService>>,
    managed_surfaces: Arc<Mutex<HashMap<u64, SurfaceControl>>>,
    next_layer_id: AtomicU64,
}

impl Default for SurfaceBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceBridge {
    /// Create a new SurfaceBridge without an active host compositor (standalone mode).
    pub fn new() -> Self {
        Self {
            compositor_service: None,
            managed_surfaces: Arc::new(Mutex::new(HashMap::new())),
            next_layer_id: AtomicU64::new(100),
        }
    }

    /// Create a SurfaceBridge attached to a live `SurfaceComposerService`.
    pub fn with_compositor(service: Arc<SurfaceComposerService>) -> Self {
        Self {
            compositor_service: Some(service),
            managed_surfaces: Arc::new(Mutex::new(HashMap::new())),
            next_layer_id: AtomicU64::new(100),
        }
    }

    /// Allocate or relayout a surface for a window.
    pub fn allocate_surface(
        &self,
        name: &str,
        width: u32,
        height: u32,
        flags: u32,
    ) -> WmsResult<SurfaceControl> {
        if let Some(ref svc) = self.compositor_service {
            match svc.create_surface(name, width, height, flags) {
                Ok(handle) => {
                    let producer_binder: Arc<dyn IBinder> = handle.producer;
                    let sc = SurfaceControl {
                        layer_id: handle.surface_id,
                        name: handle.name,
                        width: handle.width,
                        height: handle.height,
                        producer: Some(SpIBinder::from_arc(producer_binder)),
                    };
                    self.managed_surfaces
                        .lock()
                        .unwrap()
                        .insert(sc.layer_id, sc.clone());
                    Ok(sc)
                }
                Err(e) => Err(WmsError::Compositor(e.to_string())),
            }
        } else {
            let layer_id = self.next_layer_id.fetch_add(1, Ordering::SeqCst);
            let sc = SurfaceControl {
                layer_id,
                name: name.to_string(),
                width,
                height,
                producer: None,
            };
            self.managed_surfaces
                .lock()
                .unwrap()
                .insert(layer_id, sc.clone());
            Ok(sc)
        }
    }

    /// Apply a `SurfaceControlTransaction` to update layer position, size, alpha, z-order, or color.
    pub fn apply_transaction(&self, tx: &SurfaceControlTransaction) -> WmsResult<()> {
        if let Some(ref svc) = self.compositor_service {
            let mut state = LayerState::new(tx.layer_id, "WmsLayer");
            if let Some(pos) = tx.position {
                let sz = tx.size.unwrap_or([1280, 720]);
                state.set_bounds_pixels([pos[0], pos[1], sz[0] as f32, sz[1] as f32]);
            } else if let Some(sz) = tx.size {
                state.set_bounds_pixels([0.0, 0.0, sz[0] as f32, sz[1] as f32]);
            }
            if let Some(alpha) = tx.alpha {
                state.set_alpha(alpha);
            }
            if let Some(z) = tx.z_order {
                state.set_z_order(z);
            }
            if let Some(color) = tx.color {
                state.set_color(color);
            }

            let composer_state = ComposerState::new(tx.layer_id, state);
            svc.set_transaction_state(vec![composer_state], tx.flags)
                .map_err(|e| WmsError::Compositor(e.to_string()))?;
        }
        Ok(())
    }

    /// Destroy a surface by its layer ID.
    pub fn destroy_surface(&self, layer_id: u64) -> WmsResult<()> {
        self.managed_surfaces.lock().unwrap().remove(&layer_id);
        if let Some(ref svc) = self.compositor_service {
            svc.destroy_surface(layer_id)
                .map_err(|e| WmsError::Compositor(e.to_string()))?;
        }
        Ok(())
    }

    /// Query if a surface is tracked.
    pub fn get_surface(&self, layer_id: u64) -> Option<SurfaceControl> {
        self.managed_surfaces.lock().unwrap().get(&layer_id).cloned()
    }
}
