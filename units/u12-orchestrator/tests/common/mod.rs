use std::env;
use std::fs;
use std::path::PathBuf;

pub fn input_path(variable: &str, fallback: &str) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

pub fn load_initrd(ram: &mut [u8], ram_base: u64) {
    let Some(path) = env::var_os("BOOT_REPORT_INITRD") else {
        return;
    };
    let path = PathBuf::from(path);
    let bytes = fs::read(&path).unwrap_or_else(|error| {
        panic!("initramfs at {} could not be read: {error}", path.display())
    });
    assert_eq!(bytes.len(), 0x1a00, "initramfs must be 6,656 bytes");

    let offset = (0x4800_0000u64 - ram_base) as usize;
    let end = offset + bytes.len();
    assert!(end <= ram.len(), "initramfs does not fit in guest RAM");
    ram[offset..end].copy_from_slice(&bytes);
}
