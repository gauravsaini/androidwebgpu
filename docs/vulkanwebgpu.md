

# 🧱 **vulkan2wgpu — Engineering TODO List (Grounded & Practical)**

## 0️⃣ **Prep (Before coding)**
- [ ] apk_gpu_analyzer me `requires_vulkan` ko “supported” pipeline se map karna  
- [ ] Vulkan mobile usage patterns collect karna (Unity URP, Unreal, Genshin, COD, PUBG)  
- [ ] Decide subset: **Vulkan 1.1 + mobile‑friendly 1.3 features**  
- [ ] gfxstream/vulkan TLV spec ko finalize karna (virtio bridge ke liye)

---

# 1️⃣ **Crate Setup**
- [ ] `crates/vulkan2wgpu` create  
- [ ] Dependencies: `naga`, `spirv`, `wgpu`, `ash`‑style API skeleton  
- [ ] Vulkan device/queue abstraction define karna (Rust struct)

---

# 2️⃣ **SPIR‑V → WGSL Pipeline**
- [ ] SPIR‑V loader implement  
- [ ] Naga SPIR‑V frontend integrate  
- [ ] Naga PushConstant storage class → uniform binding `(group=3, binding=0)` remapper
- [ ] WGSL output validation  
- [ ] Shader module cache (hash‑based)  
- [ ] Mobile shader patterns test (Unity/Unreal)

---

# 3️⃣ **Vulkan → WebGPU Object Model Mapping**
### Device & Queue
- [ ] `vkCreateDevice` → WebGPU device  
- [ ] `vkGetDeviceQueue` → WebGPU queue

### Push Constants Emulation
- [ ] Dynamic offset uniform ring buffer pool (`wgpu::BindingType::Buffer { has_dynamic_offset: true }`)
- [ ] Per-pipeline synthetic layout binding for push constants

### Buffers & Memory Model
- [ ] `vkCreateBuffer` → `wgpu::Buffer`  
- [ ] `vkMapMemory` → Shadow CPU memory buffer in Wasm linear memory
- [ ] `vkFlushMappedMemoryRanges` → Batched `wgpu::Queue::write_buffer` staging
- [ ] `vkBindBufferMemory` → buffer binding

### Images
- [ ] `vkCreateImage` → `wgpu::Texture`  
- [ ] `vkCreateImageView` → `wgpu::TextureView`

### Samplers
- [ ] `vkCreateSampler` → `wgpu::Sampler`

### Descriptor Sets & BindGroups
- [ ] DescriptorSet → BindGroup with dirty-tracking bit
- [ ] DescriptorLayout → BindGroupLayout  
- [ ] 64-bit hash LRU cache for `wgpu::BindGroup` (lazy allocation on draw)
- [ ] UpdateDescriptors → BindGroup recreate / invalidate cache

### Pipelines
- [ ] `vkCreateGraphicsPipelines` → WebGPU render pipeline  
- [ ] `vkCreateComputePipelines` → WebGPU compute pipeline

---

# 4️⃣ **Command Buffer Translation**
### Render Commands & Dynamic Rendering
- [ ] `vkCmdBeginRenderingKHR` / Dynamic Rendering → `wgpu::RenderPassDescriptor` (1:1 mapping)
- [ ] `vkCmdBeginRenderPass` → WebGPU render pass fallback
- [ ] `vkCmdBindPipeline` → pipeline set  
- [ ] `vkCmdBindDescriptorSets` → bind groups  
- [ ] `vkCmdPushConstants` → dynamic uniform ring buffer offset update
- [ ] `vkCmdBindVertexBuffers`  
- [ ] `vkCmdBindIndexBuffer`  
- [ ] `vkCmdDraw`  
- [ ] `vkCmdDrawIndexed`

### Compute Commands
- [ ] `vkCmdDispatch` → WebGPU dispatch  
- [ ] `vkCmdDispatchIndirect`

### Barriers (subset)
- [ ] `vkCmdPipelineBarrier` → minimal WebGPU equivalents  
- [ ] MemoryBarrier → no‑op or mapped

---

# 5️⃣ **Submission & Sync**
- [ ] Flush dirty mapped shadow memory buffers to GPU before submission
- [ ] `vkQueueSubmit` → WebGPU queue submit  
- [ ] `vkQueuePresentKHR` → swapchain present  
- [ ] Fences → JS/WASM non-blocking promises  
- [ ] Semaphores → minimal emulation

---

# 6️⃣ **virtio‑gpu Vulkan TLV Integration**
- [ ] gfxstream Vulkan TLV mapping finalize  
- [ ] v86 → Rust command routing  
- [ ] SharedArrayBuffer → Vulkan command stream  
- [ ] Validation layer (optional)

---

# 7️⃣ **Testing Milestones**
### Day 2  
- [ ] SPIR‑V → WGSL shader loads  
- [ ] vkCreateDevice + vkCreateBuffer + vkCreateImage

### Day 4  
- [ ] vkCmdDraw → WebGPU triangle  
- [ ] vkCmdDispatch → compute shader test

### Day 6  
- [ ] **Green Triangle Vulkan**  
- [ ] Unity URP minimal scene  
- [ ] Unreal mobile sample

---

# 8️⃣ **Conformance Gate**
- [ ] VK‑GL‑CTS subset run (4065 tests)  
- [ ] Only mobile‑relevant CTS pass target  
- [ ] No full Vulkan CTS (browser limitations)

---

# ⭐ **Final Deliverable**
> **vulkan2wgpu crate** that supports Vulkan 1.1 + mobile 1.3 subset → WebGPU, enabling 80% modern APKs (Unity/Unreal/Genshin/COD/PUBG).


