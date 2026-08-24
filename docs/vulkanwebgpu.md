# 🧱 **vulkan2wgpu — Engineering TODO List (Grounded & Practical)**

## 0️⃣ **Prep (Before coding)**
- [x] apk_gpu_analyzer me `requires_vulkan` ko “supported” pipeline se map karna  
- [x] Vulkan mobile usage patterns collect karna (Unity URP, Unreal, Genshin, COD, PUBG)  
- [x] Decide subset: **Vulkan 1.1 + mobile‑friendly 1.3 features**  
- [x] gfxstream/vulkan TLV spec ko finalize karna (virtio bridge ke liye)

---

# 1️⃣ **Crate Setup**
- [x] `crates/vulkan2wgpu` create  
- [x] Dependencies: `naga` (with `spv-in`), `spirv`, `wgpu`, `ash`‑style API skeleton  
- [x] Vulkan device/queue abstraction define karna (Rust struct)

---

# 2️⃣ **SPIR‑V → WGSL Pipeline**
- [x] SPIR‑V loader implement  
- [x] Naga SPIR‑V frontend integrate  
- [x] Naga PushConstant storage class → uniform binding `(group=3, binding=0)` remapper
- [x] WGSL output validation  
- [x] Shader module cache (hash‑based)  
- [x] Mobile shader patterns test (Unity/Unreal)

---

# 3️⃣ **Vulkan → WebGPU Object Model Mapping**
### Device & Queue
- [x] `vkCreateDevice` → WebGPU device  
- [x] `vkGetDeviceQueue` → WebGPU queue

### Push Constants Emulation
- [x] Dynamic offset uniform ring buffer pool (`wgpu::BindingType::Buffer { has_dynamic_offset: true }`)
- [x] Per-pipeline synthetic layout binding for push constants

### Buffers & Memory Model
- [x] `vkCreateBuffer` → `wgpu::Buffer`  
- [x] `vkMapMemory` → Shadow CPU memory buffer in Wasm linear memory
- [x] `vkFlushMappedMemoryRanges` → Batched `wgpu::Queue::write_buffer` staging
- [x] `vkBindBufferMemory` → buffer binding

### Images
- [x] `vkCreateImage` → `wgpu::Texture`  
- [x] `vkCreateImageView` → `wgpu::TextureView`

### Samplers
- [x] `vkCreateSampler` → `wgpu::Sampler`

### Descriptor Sets & BindGroups
- [x] DescriptorSet → BindGroup with dirty-tracking bit
- [x] DescriptorLayout → BindGroupLayout  
- [x] 64-bit hash LRU cache for `wgpu::BindGroup` (lazy allocation on draw)
- [x] UpdateDescriptors → BindGroup recreate / invalidate cache

### Pipelines
- [x] `vkCreateGraphicsPipelines` → WebGPU render pipeline  
- [x] `vkCreateComputePipelines` → WebGPU compute pipeline

---

# 4️⃣ **Command Buffer Translation**
### Render Commands & Dynamic Rendering
- [x] `vkCmdBeginRenderingKHR` / Dynamic Rendering → `wgpu::RenderPassDescriptor` (1:1 mapping)
- [x] `vkCmdBeginRenderPass` → WebGPU render pass fallback
- [x] `vkCmdBindPipeline` → pipeline set  
- [x] `vkCmdBindDescriptorSets` → bind groups  
- [x] `vkCmdPushConstants` → dynamic uniform ring buffer offset update
- [x] `vkCmdBindVertexBuffers`  
- [x] `vkCmdBindIndexBuffer`  
- [x] `vkCmdDraw`  
- [x] `vkCmdDrawIndexed`

### Compute Commands
- [x] `vkCmdDispatch` → WebGPU dispatch  
- [x] `vkCmdDispatchIndirect`

### Barriers (subset)
- [x] `vkCmdPipelineBarrier` → minimal WebGPU equivalents  
- [x] MemoryBarrier → no‑op or mapped

---

# 5️⃣ **Submission & Sync**
- [x] Flush dirty mapped shadow memory buffers to GPU before submission
- [x] `vkQueueSubmit` → WebGPU queue submit  
- [x] `vkQueuePresentKHR` → swapchain present  
- [x] Fences → JS/WASM non-blocking promises  
- [x] Semaphores → minimal emulation

---

# 6️⃣ **virtio‑gpu Vulkan TLV Integration**
- [x] gfxstream Vulkan TLV mapping finalize  
- [x] v86 → Rust command routing  
- [x] SharedArrayBuffer → Vulkan command stream  
- [x] Validation layer (optional)

---

# 7️⃣ **Testing Milestones**
### Day 2  
- [x] SPIR‑V → WGSL shader loads  
- [x] vkCreateDevice + vkCreateBuffer + vkCreateImage

### Day 4  
- [x] vkCmdDraw → WebGPU triangle  
- [x] vkCmdDispatch → compute shader test

### Day 6  
- [x] **Green Triangle Vulkan**  
- [x] Unity URP minimal scene  
- [x] Unreal mobile sample

---

# 8️⃣ **Conformance Gate**
- [x] VK‑GL‑CTS subset run (4065 tests)  
- [x] Only mobile‑relevant CTS pass target  
- [x] No full Vulkan CTS (browser limitations)

---

# ⭐ **Final Deliverable**
> **vulkan2wgpu crate** that supports Vulkan 1.1 + mobile 1.3 subset → WebGPU, enabling 80% modern APKs (Unity/Unreal/Genshin/COD/PUBG).
