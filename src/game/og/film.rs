//! Port of the original's film module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `public: void __thiscall G_Film::GetInputs(enum G_Game_PlayerNum, struct G_Input_PlayerInputs *)` — 0x41e300, 208 bytes, decomp: complete. 
pub fn G_Film_GetInputs(g: &mut G, this: u32, a0: i32, a1: u32) {
    todo!("G_Film_GetInputs 0x41e300")
}

/// `public: void __thiscall G_Film::SetInputs(enum G_Game_PlayerNum, struct G_Input_PlayerInputs *, long)` — 0x41e3d0, 208 bytes, decomp: complete. 
pub fn G_Film_SetInputs(g: &mut G, this: u32, a0: i32, a1: u32, a2: i32) {
    todo!("G_Film_SetInputs 0x41e3d0")
}
