//! Port of the original's notice module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::game::{G_Game_GameTime_Get, U_Sound_Play, U_Utils_Safe_strncpy};
use super::resource::G_Res_GetPermFloat;
use super::umath::ftol;

// Globals (the current notice).
pub const TIME_LAST: u32 = 0x4eba72;
pub const ACTIVE: u32 = 0x4eba06; // byte
pub const IMMEDIATE: u32 = 0x4eba07; // byte
pub const FADING_OUT: u32 = 0x4eba08; // byte
pub const FADING_IN: u32 = 0x4eba09; // byte
pub const ALPHA: u32 = 0x4eba0a; // 0..0x20
pub const START_TIME: u32 = 0x4eba0e;
pub const TEXT: u32 = 0x4eba12; // char[0x40]
pub const COUNTDOWN: u32 = 0x4eba52;
pub const SOUND: u32 = 0x4eba56; // U_Sound_Settings (6 dwords)

fn pf_int(g: &mut G, i: i32) -> i32 {
    ftol(G_Res_GetPermFloat(g, i) as f64)
}

/// `void __cdecl G_Notice_ResetAtLevelStart(void)` — 0x42df90, 192 bytes, decomp: complete. 
pub fn G_Notice_ResetAtLevelStart(g: &mut G) {
    g.m.w32(TIME_LAST, 0);
    g.m.w8(ACTIVE, 0);
    g.m.w8(IMMEDIATE, 0);
    g.m.w8(FADING_OUT, 0);
    g.m.w8(FADING_IN, 0);
    g.m.w32(ALPHA, 0x20);
    g.m.w32(START_TIME, 0);
    // strcpy(text, "") from 0x4ebac6
    let src = g.m.cstr(0x4ebac6).to_vec();
    g.m.strcpy(TEXT, &src);
    g.m.w32(COUNTDOWN, 0);
    // The default sound settings at 0x4eb783.. (six dwords).
    for k in 0..6 {
        let v = g.m.r32(0x4eb783 + k * 4);
        g.m.w32(SOUND + k * 4, v);
    }
    g.m.w32(0x4eba6e, 0x41474543);
}

/// `void __cdecl G_Notice_Request(struct G_Notice_Settings *, int)` — 0x42e050, 304 bytes, decomp: matching. 
pub fn G_Notice_Request(g: &mut G, a0: u32, a1: i32) {
    if a0 == 0 || g.m.r32(a0) == 0 {
        if g.m.rb(ACTIVE) && !g.m.rb(IMMEDIATE) {
            if g.m.rb(FADING_OUT) {
                g.m.w8(ACTIVE, 0);
            } else if !g.m.rb(FADING_IN) {
                g.m.w8(FADING_IN, 1);
            }
        }
    } else if !g.m.rb(IMMEDIATE) {
        g.m.w8(ACTIVE, 1);
        let v = g.m.r8(a0 + 4);
        g.m.w8(IMMEDIATE, v);
        g.m.wi32(START_TIME, a1);
        let src = g.m.r32(a0);
        U_Utils_Safe_strncpy(g, TEXT, src, 0x3f);
        // +6 .. +0x26: the countdown and the sound settings.
        let mut k = 0;
        while k < 8 {
            let v = g.m.r32(a0 + 6 + k * 4);
            g.m.w32(COUNTDOWN + k * 4, v);
            k += 1;
        }
        if g.m.r8(a0 + 5) != 0 {
            g.m.w32(ALPHA, 0x20);
            g.m.w8(FADING_OUT, 1);
        } else {
            g.m.w32(ALPHA, 0);
            g.m.w8(FADING_OUT, 0);
        }
        g.m.w8(FADING_IN, 0);
    }
}

/// `void __cdecl G_Notice_Process(int)` — 0x42e180, 464 bytes, decomp: complete. 
pub fn G_Notice_Process(g: &mut G, a0: i32) {
    if a0 > g.m.ri32(TIME_LAST) {
        g.m.wi32(TIME_LAST, a0);
        if g.m.rb(ACTIVE) {
            if g.m.ri32(COUNTDOWN) == 0 {
                g.m.wi32(START_TIME, a0);
                if g.m.r32(SOUND) != 0x656e6f6e {
                    U_Sound_Play(g, SOUND, true);
                }
            }
            let c = g.m.ri32(COUNTDOWN).wrapping_sub(1);
            g.m.wi32(COUNTDOWN, c);
            if c <= 0 {
                if !g.m.rb(IMMEDIATE) {
                    let t = pf_int(g, 0x47);
                    if a0 > t.wrapping_add(g.m.ri32(START_TIME)) {
                        let now = G_Game_GameTime_Get(g);
                        G_Notice_Request(g, 0, now);
                    }
                }
                if g.m.rb(FADING_OUT) {
                    let d = pf_int(g, 0x48);
                    if g.m.r32(ALPHA) >= d as u32 {
                        let v = g.m.r32(ALPHA).wrapping_sub(d as u32);
                        g.m.w32(ALPHA, v);
                    } else {
                        g.m.w32(ALPHA, 0);
                        g.m.w8(FADING_OUT, 0);
                        g.m.w8(FADING_IN, 0);
                    }
                }
                if g.m.rb(FADING_IN) {
                    let a = pf_int(g, 0x49);
                    let v = g.m.r32(ALPHA).wrapping_add(a as u32);
                    g.m.w32(ALPHA, v);
                    if v >= 0x20 {
                        g.m.w32(ALPHA, 0x20);
                        g.m.w8(ACTIVE, 0);
                    }
                }
            }
        }
    }
}
