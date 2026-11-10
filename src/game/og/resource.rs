//! Port of the original's resource module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `bool __cdecl G_Res_IsLoaded(enum G_Res_Type, long)` — 0x437a80, 144 bytes, decomp: complete. 
pub fn G_Res_IsLoaded(g: &mut G, a0: i32, a1: i32) -> bool {
    todo!("G_Res_IsLoaded 0x437a80")
}

/// `long __cdecl G_Res_GetPermObjectID(int)` — 0x438140, 16 bytes, decomp: complete. 
pub fn G_Res_GetPermObjectID(g: &mut G, a0: i32) -> i32 {
    todo!("G_Res_GetPermObjectID 0x438140")
}

/// `long __cdecl G_Res_GetPermSoundID(int)` — 0x438160, 16 bytes, decomp: complete. 
pub fn G_Res_GetPermSoundID(g: &mut G, a0: i32) -> i32 {
    todo!("G_Res_GetPermSoundID 0x438160")
}

/// `float __cdecl G_Res_GetPermFloat(int)` — 0x4381b0, 16 bytes, decomp: complete. 
pub fn G_Res_GetPermFloat(g: &mut G, a0: i32) -> f32 {
    todo!("G_Res_GetPermFloat 0x4381b0")
}

/// `char * __cdecl G_Res_GetPermGameString(enum G_Res_PermGameStringIndex)` — 0x4381c0, 16 bytes, decomp: complete. 
pub fn G_Res_GetPermGameString(g: &mut G, a0: i32) -> u32 {
    todo!("G_Res_GetPermGameString 0x4381c0")
}

/// `struct G_Res_Entry * __cdecl G_Res_FindEntry(enum G_Res_Type, long)` — 0x438320, 144 bytes, decomp: complete. 
pub fn G_Res_FindEntry(g: &mut G, a0: i32, a1: i32) -> u32 {
    todo!("G_Res_FindEntry 0x438320")
}
