//! Port of the original's scorebar module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::player::*;
use super::resource::G_Res_GetPermFloat;
use super::weaponhandler::*;

/// The two score bars, 0x149 bytes each, from 0x4f0a20. Fields used here:
/// +0x18 score, +0x1c lives, +0x20 shield display, +0x24 power display,
/// +0x128..+0x12d redraw flags (score, ?, lives, weapon, shield, power),
/// +0x12e player present, +0x12f player drawn, +0x131 weapon info.
pub const BARS: u32 = 0x4f0a20;
pub const BAR_SIZE: u32 = 0x149;

fn bar(i: i32) -> u32 {
    BARS.wrapping_add((i as u32).wrapping_mul(BAR_SIZE))
}

/// The jump tables at 0x4c0a1d / 0x4c0a26: player 0 -> bar 0, player 1 ->
/// bar 1, anything else bar 0.
fn bar_index(n: i32) -> i32 {
    if (n as u32) > 1 { 0 } else { n }
}

/// `void __cdecl G_ScoreBar_ResetAtLevelStart(class G_Player *, class G_Player *)` — 0x439420, 688 bytes, decomp: complete. 
pub fn G_ScoreBar_ResetAtLevelStart(g: &mut G, a0: u32, a1: u32) {
    // (Drawing the bar backgrounds into the display is not ported.)
    let sp = g.m.sp;
    let b0 = bar(0);
    let b1 = bar(1);
    G_WeaponHandler_AirWeapon_GetScoreBarInfo(g, a0 + 0x235, b0 + 0x131);
    G_WeaponHandler_AirWeapon_GetScoreBarInfo(g, a1 + 0x235, b1 + 0x131);
    let pd = G_Player_GetPlayerDefinitionRefPtr(g, a0);
    if pd != 0 {
        for k in 0..6 {
            let v = g.m.r32(pd + 0x30 + k * 4);
            g.m.w32(b0 + k * 4, v);
        }
    }
    let pd = G_Player_GetPlayerDefinitionRefPtr(g, a1);
    if pd != 0 {
        for k in 0..6 {
            let v = g.m.r32(pd + 0x30 + k * 4);
            g.m.w32(b1 + k * 4, v);
        }
    }
    let mut i = 0;
    while i < 2 {
        let s = bar(i);
        for k in 0x128..=0x12d {
            g.m.w8(s + k, 1);
        }
        g.m.w32(s + 0x20, 0);
        g.m.w32(s + 0x24, 0);
        let p = if i == 0 { a0 } else { a1 };
        let sc = G_Player_Score_Get(g, p);
        g.m.wi32(s + 0x18, sc);
        let lv = G_Player_Lives_Get(g, p);
        g.m.wi32(s + 0x1c, lv);
        let ing = G_Player_InGame(g, p) as u8;
        g.m.w8(s + 0x12e, ing);
        g.m.w8(s + 0x12f, ing);
        i += 1;
    }
    g.m.pop_to(sp);
}

/// `void __cdecl G_ScoreBar_SetShieldBarPercentage(enum G_Game_PlayerNum, float)` — 0x4396d0, 48 bytes, decomp: complete. 
pub fn G_ScoreBar_SetShieldBarPercentage(g: &mut G, a0: i32, a1: f32) {
    let s = bar(bar_index(a0));
    g.m.wf32(s + 0x20, a1);
}

/// `void __cdecl G_ScoreBar_SetPowerBarPercentage(enum G_Game_PlayerNum, float)` — 0x439700, 112 bytes, decomp: complete. 
pub fn G_ScoreBar_SetPowerBarPercentage(g: &mut G, a0: i32, a1: f32) {
    let s = bar(bar_index(a0));
    g.m.wf32(s + 0x24, a1);
    let v = a1 as f64;
    if v < g.m.rf64(0x4f0cb8) {
        g.m.w32(s + 0x24, 0);
    } else if v > g.m.rf64(0x4f0cc0) {
        g.m.w32(s + 0x24, 0x42c80000);
    }
}

/// `void __cdecl G_ScoreBar_Process(class G_Player *, class G_Player *)` — 0x439770, 816 bytes, decomp: compiling. 
pub fn G_ScoreBar_Process(g: &mut G, a0: u32, a1: u32) {
    let mut i = 0;
    while i < 2 {
        let s = bar(i);
        for k in 0x128..=0x12d {
            g.m.w8(s + k, 0);
        }
        let p = if i == 0 { a0 } else { a1 };
        if G_Player_InGame(g, p) && !G_Player_IsAboutToBeRemovedFromGame(g, p) {
            if g.m.ri32(s + 0x18) != G_Player_Score_Get(g, p) {
                let v = G_Player_Score_Get(g, p);
                g.m.wi32(s + 0x18, v);
                g.m.w8(s + 0x128, 1);
            }
            if g.m.ri32(s + 0x1c) != G_Player_Lives_Get(g, p) {
                let v = G_Player_Lives_Get(g, p);
                g.m.wi32(s + 0x1c, v);
                g.m.w8(s + 0x12a, 1);
            }
            if G_WeaponHandler_AirWeapon_HasChanged(g, p + 0x235) {
                g.m.w8(s + 0x12b, 1);
                G_WeaponHandler_AirWeapon_GetScoreBarInfo(g, p + 0x235, s + 0x131);
            }
            // Shield bar: eases towards the player's shield percentage.
            let sh = G_Player_Shields_GetPercentage(g, p) as f64;
            let cur = g.m.rf32(s + 0x20) as f64;
            if !(cur == sh) {
                g.m.w8(s + 0x12c, 1);
                if cur > sh {
                    let d = G_Res_GetPermFloat(g, 0x79) as f64;
                    let v = (cur - d) as f32;
                    g.m.wf32(s + 0x20, v);
                    if (v as f64) < sh {
                        g.m.wf32(s + 0x20, sh as f32);
                    }
                } else if cur < sh {
                    if G_Player_IsState(g, p, 4) {
                        let d = G_Res_GetPermFloat(g, 0x78) as f64;
                        let v = (d + cur) as f32;
                        g.m.wf32(s + 0x20, v);
                        if v as f64 > sh {
                            g.m.wf32(s + 0x20, sh as f32);
                        }
                    }
                }
            }
            // Power bar.
            let pw = G_WeaponHandler_AirPowerup_GetPercentage(g, p + 0x235) as f64;
            let cur = g.m.rf32(s + 0x24) as f64;
            if !(cur == pw) {
                g.m.w8(s + 0x12d, 1);
                if cur > pw {
                    let d = G_Res_GetPermFloat(g, 0x7f) as f64;
                    let v = (cur - d) as f32;
                    g.m.wf32(s + 0x24, v);
                    if (v as f64) < pw {
                        g.m.wf32(s + 0x24, pw as f32);
                    }
                } else if cur < pw {
                    if G_Player_IsState(g, p, 4) {
                        let d = G_Res_GetPermFloat(g, 0x7e) as f64;
                        let v = (d + cur) as f32;
                        g.m.wf32(s + 0x24, v);
                        if v as f64 > pw {
                            g.m.wf32(s + 0x24, pw as f32);
                        }
                    }
                }
            }
            let x = g.m.rf32(s + 0x24) as f64;
            if x < g.m.rf64(0x4f0cb8) {
                g.m.w32(s + 0x24, 0);
            } else if x > g.m.rf64(0x4f0cc0) {
                g.m.w32(s + 0x24, 0x42c80000);
            }
        } else if g.m.r8(s + 0x12e) != 0 {
            g.m.w8(s + 0x12f, 0);
            for k in 0x128..=0x12d {
                g.m.w8(s + k, 1);
            }
            g.m.w8(s + 0x12e, 0);
        }
        i += 1;
    }
}

/// `_sub_00439df0` — 0x439df0, 288 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_439df0(g: &mut G) {
    todo!("sub_439df0 0x439df0")
}

/// `_sub_00439f10` — 0x439f10, 560 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_439f10(g: &mut G) {
    todo!("sub_439f10 0x439f10")
}

/// `_sub_0043a140` — 0x43a140, 608 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_43a140(g: &mut G) {
    todo!("sub_43a140 0x43a140")
}

/// `_sub_0043a3a0` — 0x43a3a0, 928 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_43a3a0(g: &mut G) {
    todo!("sub_43a3a0 0x43a3a0")
}

/// `_sub_0043a740` — 0x43a740, 928 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_43a740(g: &mut G) {
    todo!("sub_43a740 0x43a740")
}

/// `_sub_0043aae0` — 0x43aae0, 1344 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_43aae0(g: &mut G) {
    todo!("sub_43aae0 0x43aae0")
}

/// `_sub_43b020` — 0x43b020, 144 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_43b020(g: &mut G) {
    todo!("sub_43b020 0x43b020")
}
