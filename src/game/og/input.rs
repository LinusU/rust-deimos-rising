//! Port of the original's input module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `void __cdecl G_Input_ResetAtLevelStart(void)` — 0x425b50, 16 bytes, decomp: complete. 
pub fn G_Input_ResetAtLevelStart(g: &mut G) {
    todo!("G_Input_ResetAtLevelStart 0x425b50")
}

/// `void __cdecl G_Input_AssumeControl(bool)` — 0x425b60, 16 bytes, decomp: complete. 
pub fn G_Input_AssumeControl(g: &mut G, a0: bool) {
    todo!("G_Input_AssumeControl 0x425b60")
}

/// `void __cdecl G_Input_ReleaseControl(void)` — 0x425b70, 16 bytes, decomp: complete. 
pub fn G_Input_ReleaseControl(g: &mut G) {
    todo!("G_Input_ReleaseControl 0x425b70")
}

/// `bool __cdecl G_Input_HasAssumedControl(void)` — 0x425b80, 16 bytes, decomp: complete. 
pub fn G_Input_HasAssumedControl(g: &mut G) -> bool {
    todo!("G_Input_HasAssumedControl 0x425b80")
}

/// `void __cdecl G_Input_Clear(void)` — 0x425b90, 64 bytes, decomp: complete. 
pub fn G_Input_Clear(g: &mut G) {
    todo!("G_Input_Clear 0x425b90")
}

/// `void __cdecl G_Input_CachePlayerInputs(void)` — 0x425bd0, 896 bytes, decomp: matching. 
pub fn G_Input_CachePlayerInputs(g: &mut G) {
    todo!("G_Input_CachePlayerInputs 0x425bd0")
}

/// `void __cdecl G_Input_GetCachedPlayerInputs(enum G_Game_PlayerNum, struct G_Input_PlayerInputs *)` — 0x425f50, 128 bytes, decomp: matching. 
pub fn G_Input_GetCachedPlayerInputs(g: &mut G, a0: i32, a1: u32) {
    todo!("G_Input_GetCachedPlayerInputs 0x425f50")
}
