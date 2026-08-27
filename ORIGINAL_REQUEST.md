# Original User Request

## 2026-08-26T22:19:38Z

Completely eliminate all simulated HTML DOM screens and synthetic application mockups (e.g. renderFdroidActivity, renderSettingsActivity, renderBrowserActivity, and DOM div injections in android_runtime.js and android.html). Implement authentic Android binary XML layout inflation (res/layout/*.xml), resolve resource IDs through resources.arsc, build real in-memory Android View / ViewGroup trees, and rasterize them directly onto WebGPU textures / guest virtio-gpu framebuffers.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: development

## Requirements

### R1. Authentic APK Binary XML Layout Inflation & Resource Resolution
- Implement genuine Android layout inflation that extracts and decodes binary XML files (res/layout/*.xml) directly from APK archives (e.g. F-Droid.apk).
- Resolve Android resource identifiers (@string/*, @drawable/*, @color/*, @dimen/*, @layout/*, @id/*, @style/*) using the parsed resources.arsc string pool and type spec tables.
- Construct live in-memory Android View hierarchies supporting fundamental layout types: FrameLayout, LinearLayout (horizontal/vertical, weight distribution), RelativeLayout, ConstraintLayout, ScrollView, RecyclerView / ListView, TextView, ImageView, and Button.

### R2. WebGPU & SurfaceFlinger Hardware View Rasterization
- Delete all synthetic HTML DOM UI generation functions in android_runtime.js that build simulated web mockups.
- Render the active ViewHierarchy directly onto an OffscreenCanvas / WebGPU texture buffer with pixel-accurate Material Design 3 styling, text metrics, background tints, vector drawables, and rounded corners.
- Submit rasterized application buffers through SurfaceFlinger / VirtioGpuBridge so that all app pixels are presented exclusively via the WebGPU hardware swapchain.

### R3. Real Guest VM Framebuffer Integration & Seamless Switching
- Ensure guest Linux / x86 VM scanouts (via virtio-gpu / fbcon) render smoothly to the WebGPU canvas without HTML DOM overlays.
- Provide fluid switching between the guest OS display and running APK application activities through the WindowManager (wms_rs) surface stack.

### R4. Touch, Gesture, & Key Dispatch to Real View Trees
- Capture mouse / touch events from the canvas and route them through InputManager (inputflinger_rs) to the target ViewRootImpl.
- Implement hit-testing and event bubbling (onTouchEvent, onClick, onScroll) across the inflated View hierarchy.
- Dispatch hardware navigation buttons (Back, Home, Recents) to AMS and active Activity lifecycles without DOM screen manipulation.

## Acceptance Criteria

### Layout Inflation & Resource Resolution
- [ ] Binary XML decoder successfully extracts and parses layout structures from F-Droid.apk (e.g. activity_main.xml, app_list_item.xml).
- [ ] Resource IDs are correctly resolved from resources.arsc without falling back to hardcoded string constants.
- [ ] Inflated view hierarchy correctly represents child-parent relationships, layout dimensions (match_parent, wrap_content, explicit dp), and padding/margins.

### Hardware Rasterization & Zero-Mock Verification
- [ ] screen-app / phone viewport contains 0 synthetic HTML mockup divs (fdroid-authentic-root, hardcoded DOM app mockups deleted).
- [ ] Real app UI is visibly rendered into WebGPU canvas buffers with verified draw calls (text, shapes, drawables).
- [ ] Test bench validates that application launching routes purely through ViewHierarchy -> SurfaceFlinger -> WebGPU without DOM substitution.

### Interactive Input & Navigation
- [ ] Clicks and scrolling on the WebGPU canvas trigger hit-tested callbacks in the inflated View hierarchy.
- [ ] Activity backstack (goBack(), finish()) properly pops inflated activity views and restores the previous view state.

## 2026-08-27T01:34:48Z

Configure and verify direct boot of the real Android x86 Linux kernel (bzImage) with initrd.img in the v86 hypervisor, establishing real serial milestone streaming and BinderFS initialization.

Working directory: /Users/ektasaini/Desktop/androidwebgpu
Integrity mode: development

## Requirements

### R1. Direct Kernel Boot Configuration
Configure the v86 hypervisor in SystemBootstrap and V86GuestManager to boot directly from guest/build/bzImage and guest/build/initrd.img using explicit kernel cmdline (console=ttyS0 earlyprintk=serial,ttyS0,115200 root=/dev/ram0 rdinit=/init panic=1 loglevel=8).

### R2. Real Serial Milestone Streaming & Lifecycle Progression
Ensure real serial output from /dev/ttyS0 is streamed into Logcat ([v86Guest]) and drives the VM state machine milestones (BIOS_POST -> KERNEL_BOOT -> KERNEL_UNCOMPRESS -> KERNEL_READY -> BINDERFS_MOUNT -> INIT_USERSPACE -> SERVICEMANAGER_READY -> RUST_SERVICES_READY -> SYSTEM_BOOT_COMPLETED -> RUNNING).

### R3. BinderFS and Native AOSP Services Standup
Validate that the guest init system mounts BinderFS at /dev/binderfs, links /dev/binder, starts servicemanager (Handle 0), and initializes native Rust system services (pms_rs, ams_rs, wms_rs, inputflinger_rs).

## Acceptance Criteria

### Boot Verification
- [ ] v86 hypervisor loads guest/build/bzImage and guest/build/initrd.img with zero reliance on linux4.iso.
- [ ] Kernel boot messages (dmesg) stream to the serial console listener and appear in Logcat under tag v86Guest.
- [ ] State transitions advance through real parsed milestones without artificial delays or synthetic transitions.
- [ ] All automated E2E tests in tests/run_e2e_tests.mjs and tests/test_v86_guest_boot.mjs pass.
