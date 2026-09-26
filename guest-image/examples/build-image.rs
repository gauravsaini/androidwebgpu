//! Builder entrypoint: `cargo run -p guest-image --example build-image -- <out.bin>`
//!
//! Writes the guest image and its SBOM sidecar (`<out.bin>.sbom.json`).
//! Pure function of the manifest — no inputs, no clock, no network.

use guest_image::{build, GuestManifest};
use std::fs;

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/guest-image.bin".to_string());
    let (image, sbom) = build(&GuestManifest::pathn_sh());
    fs::write(&out, &image).expect("write image");
    fs::write(format!("{out}.sbom.json"), &sbom).expect("write SBOM");
    println!("wrote {out} ({} bytes) + {out}.sbom.json", image.len());
}
