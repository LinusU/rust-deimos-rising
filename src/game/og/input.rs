//! Port of the original's input module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
/// `g_inputCached[2]`: two 7-byte `G_Input_PlayerInputs`, back to back.
pub const CACHED: u32 = 0x4e5a7b;

/// `void __cdecl G_Input_ResetAtLevelStart(void)` — 0x425b50, 16 bytes, decomp: complete. 
pub fn G_Input_ResetAtLevelStart(g: &mut G) {
    G_Input_Clear(g);
}

/// `void __cdecl G_Input_AssumeControl(bool)` — 0x425b60, 16 bytes, decomp: complete. 
pub fn G_Input_AssumeControl(g: &mut G, a0: bool) {

}

/// `void __cdecl G_Input_ReleaseControl(void)` — 0x425b70, 16 bytes, decomp: complete. 
pub fn G_Input_ReleaseControl(g: &mut G) {

}

/// `bool __cdecl G_Input_HasAssumedControl(void)` — 0x425b80, 16 bytes, decomp: complete. 
pub fn G_Input_HasAssumedControl(g: &mut G) -> bool {
    // A single `ret`: the return value is whatever was left in AL.
    false
}

/// `void __cdecl G_Input_Clear(void)` — 0x425b90, 64 bytes, decomp: complete. 
pub fn G_Input_Clear(g: &mut G) {
    let mut i = 0;
    while i < 2 {
        g.m.clear(CACHED + i * 7, 7);
        i += 1;
    }
}

/// `void __cdecl G_Input_CachePlayerInputs(void)` — 0x425bd0, 896 bytes, decomp: matching. 
pub fn G_Input_CachePlayerInputs(g: &mut G) {
    // The original polls the keyboard and joystick (U_Prefs key codes,
    // KeyboardIsPressed, JoystickReadButtons) and sets a byte of
    // `g_inputCached` to 1 per pressed control. Here the host fills
    // `g.live_inputs` (all zero during film replays, so the cache stays as
    // G_Input_Clear left it) and the pressed controls are copied over.
    for i in 0..14u32 {
        if g.live_inputs[i as usize] != 0 {
            g.m.w8(CACHED + i, 1);
        }
    }
}

/// `void __cdecl G_Input_GetCachedPlayerInputs(enum G_Game_PlayerNum, struct G_Input_PlayerInputs *)` — 0x425f50, 128 bytes, decomp: matching. 
pub fn G_Input_GetCachedPlayerInputs(g: &mut G, a0: i32, a1: u32) {
    if (a0 as u32) <= 1 {
        let src = CACHED + (a0 as u32) * 7;
        let v = g.m.r32(src);
        g.m.w32(a1, v);
        let v = g.m.r16(src + 4);
        g.m.w16(a1 + 4, v);
        let v = g.m.r8(src + 6);
        g.m.w8(a1 + 6, v);
    }
    // else: U_App_Log("DEBUG: Input - unknown Player Number '%i'")
}
