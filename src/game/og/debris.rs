//! Port of the original's debris module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `void __cdecl G_Debris_ResetAtLevelStart(void)` — 0x412b50, 112 bytes, decomp: complete. 
pub fn G_Debris_ResetAtLevelStart(g: &mut G) {
    todo!("G_Debris_ResetAtLevelStart 0x412b50")
}

/// `void __cdecl G_Debris_New(struct U_Rect *)` — 0x412bc0, 128 bytes, decomp: complete. 
pub fn G_Debris_New(g: &mut G, a0: u32) {
    todo!("G_Debris_New 0x412bc0")
}

/// `void __cdecl G_Debris_Process(void)` — 0x412c40, 144 bytes, decomp: complete. 
pub fn G_Debris_Process(g: &mut G) {
    todo!("G_Debris_Process 0x412c40")
}

/// `bool __cdecl G_Debris_CheckCollision(struct U_Rect *)` — 0x412cd0, 160 bytes, decomp: complete. 
pub fn G_Debris_CheckCollision(g: &mut G, a0: u32) -> bool {
    todo!("G_Debris_CheckCollision 0x412cd0")
}

/// `_sub_412d90` — 0x412d90, 224 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_412d90(g: &mut G) {
    todo!("sub_412d90 0x412d90")
}
