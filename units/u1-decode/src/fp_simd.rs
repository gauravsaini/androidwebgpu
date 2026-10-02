//! Floating-point and SIMD instruction decoder family stub.

use pathn_contracts::cpu::InsnKind;

/// Decode FP/SIMD instructions (stub returning None).
pub fn decode(_word: u32) -> Option<InsnKind> {
    None
}
