//! Port of the original's motionblur module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `void __cdecl G_MotionBlur_ResetAtLevelStart(void)` — 0x42d5b0, 112 bytes, decomp: complete. 
pub fn G_MotionBlur_ResetAtLevelStart(g: &mut G) {
    todo!("G_MotionBlur_ResetAtLevelStart 0x42d5b0")
}

/// `void __cdecl G_MotionBlur_New(struct G_MotionBlur_Request *)` — 0x42d620, 400 bytes, decomp: complete. 
pub fn G_MotionBlur_New(g: &mut G, a0: u32) {
    todo!("G_MotionBlur_New 0x42d620")
}

/// `void __cdecl G_MotionBlur_Process(void)` — 0x42d7b0, 192 bytes, decomp: complete. 
pub fn G_MotionBlur_Process(g: &mut G) {
    todo!("G_MotionBlur_Process 0x42d7b0")
}

/// `void __cdecl G_MotionBlur_sub_2d910(void)` — 0x42d910, 192 bytes, decomp: compiling. 
pub fn G_MotionBlur_sub_2d910(g: &mut G) {
    todo!("G_MotionBlur_sub_2d910 0x42d910")
}

/// `void __cdecl G_MotionBlur_sub_2dab0(void)` — 0x42dab0, 64 bytes, decomp: complete. 
pub fn G_MotionBlur_sub_2dab0(g: &mut G) {
    todo!("G_MotionBlur_sub_2dab0 0x42dab0")
}

/// `void __cdecl G_MotionBlur_sub_2daf0(void)` — 0x42daf0, 160 bytes, decomp: compiling. 
pub fn G_MotionBlur_sub_2daf0(g: &mut G) {
    todo!("G_MotionBlur_sub_2daf0 0x42daf0")
}

/// `class G_MotionBlurObject * __cdecl G_MotionBlur_Alloc(void)` — 0x42db90, 272 bytes, decomp: compiling. 
pub fn G_MotionBlur_Alloc(g: &mut G) -> u32 {
    todo!("G_MotionBlur_Alloc 0x42db90")
}

/// `void __cdecl G_MotionBlur_Free(class G_MotionBlurObject *)` — 0x42dca0, 48 bytes, decomp: complete. 
pub fn G_MotionBlur_Free(g: &mut G, a0: u32) {
    todo!("G_MotionBlur_Free 0x42dca0")
}
