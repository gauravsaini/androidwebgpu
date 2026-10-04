// Verify initrd wiring matches Track B QEMU layout
use u12_orchestrator::{Orchestrator, RAM_BASE};

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const INITRD_LOAD_PA: u64 = 0x4800_0000;
const DTB_LOAD_PA: u64 = 0x4820_0000;

fn main() {
    let mut orch = Orchestrator::new();
    
    // Load kernel
    let kernel = std::fs::read("guest-image/Image").expect("kernel");
    println!("Kernel size: {} bytes (0x{:x})", kernel.len(), kernel.len());
    
    // Load initrd
    let initrd = std::fs::read("/tmp/initramfs.cpio").expect("initrd");
    println!("Initrd size: {} bytes (0x{:x})", initrd.len(), initrd.len());
    
    // Load DTB
    let dtb = std::fs::read("guest-image/minimal-virt.dtb").expect("dtb");
    println!("DTB size: {} bytes (0x{:x})", dtb.len(), dtb.len());
    
    // Place in RAM per Track B layout
    {
        let ram = &mut orch.machine_mut().ram;
        let k_off = (KERNEL_LOAD_PA - RAM_BASE) as usize;
        ram[k_off..k_off+kernel.len()].copy_from_slice(&kernel);
        
        let i_off = (INITRD_LOAD_PA - RAM_BASE) as usize;
        ram[i_off..i_off+initrd.len()].copy_from_slice(&initrd);
        
        let d_off = (DTB_LOAD_PA - RAM_BASE) as usize;
        ram[d_off..d_off+dtb.len()].copy_from_slice(&dtb);
    }
    
    // Verify layout (Track B oracle)
    let kernel_end = KERNEL_LOAD_PA + kernel.len() as u64;
    let initrd_end = INITRD_LOAD_PA + initrd.len() as u64;
    let dtb_end = DTB_LOAD_PA + dtb.len() as u64;
    
    println!("\n=== Memory Layout (Track B) ===");
    println!("Kernel:  0x{:08x} - 0x{:08x} ({} bytes)", KERNEL_LOAD_PA, kernel_end, kernel.len());
    println!("Initrd:  0x{:08x} - 0x{:08x} ({} bytes)", INITRD_LOAD_PA, initrd_end, initrd.len());
    println!("DTB:     0x{:08x} - 0x{:08x} ({} bytes)", DTB_LOAD_PA, dtb_end, dtb.len());
    
    // Check for overlaps
    let mut pass = true;
    
    // Kernel vs Initrd
    if kernel_end > INITRD_LOAD_PA {
        println!("FAIL: Kernel overlaps initrd!");
        pass = false;
    } else {
        println!("PASS: Kernel does not overlap initrd");
    }
    
    // Initrd vs DTB
    if initrd_end > DTB_LOAD_PA {
        println!("FAIL: Initrd overlaps DTB!");
        pass = false;
    } else {
        println!("PASS: Initrd does not overlap DTB");
    }
    
    // Verify DTB chosen node has initrd properties
    // Find the strings in DTB
    let has_start = dtb.windows(b"linux,initrd-start".len()).any(|w| w == b"linux,initrd-start");
    let has_end = dtb.windows(b"linux,initrd-end".len()).any(|w| w == b"linux,initrd-end");
    
    if has_start && has_end {
        println!("PASS: DTB has linux,initrd-start and linux,initrd-end");
    } else {
        println!("FAIL: DTB missing initrd properties!");
        pass = false;
    }
    
    // Verify the values
    use std::io::Read;
    let start_val = [0u8, 0, 0, 0, 0x48, 0, 0, 0]; // 0x00, 0x48000000 as BE u32s
    let end_val = [0u8, 0, 0, 0, 0x48, 0, 0x1a, 0]; // 0x00, 0x48001a00 as BE u32s
    
    let has_start_val = dtb.windows(8).any(|w| w == start_val);
    let has_end_val = dtb.windows(8).any(|w| w == end_val);
    
    if has_start_val {
        println!("PASS: linux,initrd-start = 0x48000000");
    } else {
        println!("FAIL: linux,initrd-start value incorrect!");
        pass = false;
    }
    
    if has_end_val {
        println!("PASS: linux,initrd-end = 0x48001a00");
    } else {
        println!("FAIL: linux,initrd-end value incorrect!");
        pass = false;
    }
    
    // Verify bootargs has rdinit=/init
    let has_rdinit = dtb.windows(b"rdinit=/init".len()).any(|w| w == b"rdinit=/init");
    if has_rdinit {
        println!("PASS: bootargs contains rdinit=/init");
    } else {
        println!("FAIL: bootargs missing rdinit=/init!");
        pass = false;
    }
    
    println!("\n=== ORACLE: {} ===", if pass { "PASS" } else { "FAIL" });
    std::process::exit(if pass { 0 } else { 1 });
}
