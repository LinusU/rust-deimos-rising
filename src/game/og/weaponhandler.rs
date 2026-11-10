//! Port of the original's weaponhandler module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::eg::*;
use super::game::{sound_default, G_Game_GetCurrentLevelNumber};
use super::gameobject::*;
use super::mem::list;
use super::resource::*;
use super::umath::ftol;
use super::wepdefs::*;

// `this` is the handler inside G_Player (+0x235). Fields used:
//   +0 x, +4 y (f32)            +8 air weapon changed     +9/+0xa/+0xb previous fire/air/swap buttons
//   +0x11 air-powerup state     +0x15 state time          +0x19 spawned entity number
//   +0x1d fire-rate time        +0x21 count               +0x25 percentage (f32)
//   +0x29 release time          +0x2d hold counter
//   +0x35 ground state ... +0x39 time, +0x3d entity, +0x41 time, +0x45 count, +0x49 pct, +0x4d time, +0x51 hold
//   +0x55 air current, +0x59 pending ground, +0x5d air queued, +0x61/+0x65 last air spawn times,
//   +0x6d air spawn count, +0x73 auxiliary weapon-info list, +0x77 permanent (ground) weapon,
//   +0x7b/+0x7f ground spawn times, +0x87 ground burst count, +0x8d draw-list G_GameObject.
const NONE: u32 = 0x656e6f6e;
const REF_DEFAULT: u32 = 0x4f9fbe;
const SPAWN_DEFAULT: u32 = 0x4fa50d;
const C_HUNDRED: u32 = 0x4fa550; // 100.0 (double)
const C_ONE: u32 = 0x4fa558; // 1.0 (double)

pub const TYPE_AIR: i32 = 0x41414550;
pub const TYPE_GROUND: i32 = 0x47414550;
pub const TYPE_AUX: i32 = 0x20585541;

fn default_ref(g: &mut G, r: u32) {
    let v = g.m.r32(REF_DEFAULT);
    g.m.w32(r, v);
    let v = g.m.r32(REF_DEFAULT + 4);
    g.m.w32(r + 4, v);
}

/// The default `G_EG_SpawnRequest` (0x28 bytes at 0x4fa50d).
fn init_req(g: &mut G, req: u32) {
    for k in 0..8 {
        let v = g.m.r32(SPAWN_DEFAULT + k * 4);
        g.m.w32(req + k * 4, v);
    }
    let v = g.m.r32(0x4fa52d);
    g.m.w32(req + 0x20, v);
    let v = g.m.r32(0x4fa531);
    g.m.w32(req + 0x24, v);
}

/// `fild b; fild a; fdivrp; fmul 100.0; fstp` = a / b * 100 as a float,
/// then the clamp: below 1.0 becomes 0, above 100.0 becomes 100.0.
fn percentage(a: i32, b: i32, one: f64, hundred: f64) -> f32 {
    let q = if a == 0 && b == 0 {
        f64::from_bits(0xfff8_0000_0000_0000) // x87 "real indefinite"
    } else {
        a as f64 / b as f64
    };
    let v = (q * hundred) as f32;
    if (v as f64) < one {
        0.0
    } else if (v as f64) > hundred {
        100.0
    } else {
        v
    }
}

/// Spawn request for a spawn set (`G_UnitDef_SpawnSet`: +0x20 id, +0x24/+0x28
/// offsets, +0x2c byte, +0x2e dword) placed at `vec` + offset.
fn spawn_set_request(g: &mut G, this: u32, set: u32, vec: u32, req: u32) {
    init_req(g, req);
    let v = g.m.r32(set + 0x20);
    g.m.w32(req, v);
    let v = g.m.r32(this + 0x119);
    g.m.w32(req + 0x12, v);
    let x = (g.m.ri32(set + 0x24) as f64 + g.m.rf32(vec) as f64) as f32;
    g.m.wf32(req + 4, x);
    let y = (g.m.ri32(set + 0x28) as f64 + g.m.rf32(vec + 4) as f64) as f32;
    g.m.wf32(req + 8, y);
    let v = g.m.r8(set + 0x2c);
    g.m.w8(req + 0xd, v);
    let v = g.m.r32(set + 0x2e);
    g.m.w32(req + 0xe, v);
}

/// The spawn sets of a weapon (list at +0x1c0).
fn spawn_set_count(g: &mut G, wep: u32) -> i32 {
    let l = g.m.r32(wep + 0x1c0);
    if l != 0 {
        list::num(&g.m, l)
    } else {
        0
    }
}

/// `public: void __thiscall G_WeaponHandler::SetUpForAppearance(bool)` — 0x446f40, 464 bytes, decomp: complete. 
pub fn G_WeaponHandler_SetUpForAppearance(g: &mut G, this: u32, a0: bool) {
    let v = g.m.r32(0x4f9f98);
    g.m.w32(this, v);
    let v = g.m.r32(0x4f9f9c);
    g.m.w32(this + 4, v);
    g.m.w8(this + 9, 0);
    g.m.w8(this + 0xa, 0);
    g.m.w8(this + 0xb, 0);
    g.m.w8(this + 8, 1);
    g.m.w32(this + 0x7b, 0);
    g.m.w32(this + 0x7f, 0);
    g.m.w32(this + 0x87, 0);
    g.m.w8(this + 0x8b, 0);
    g.m.w8(this + 0x8c, 1);
    g.m.w32(this + 0x61, 0);
    g.m.w32(this + 0x65, 0);
    g.m.w32(this + 0x6d, 0);
    g.m.w8(this + 0x71, 0);
    g.m.w8(this + 0x72, 1);
    let l = g.m.r32(this + 0x73);
    if l != 0 {
        let mut i = 0;
        while i < list::num(&g.m, l) {
            let info = list::nth(&g.m, l, i);
            g.m.w32(info + 4, 0);
            g.m.w32(info + 8, 0);
            g.m.w32(info + 0x10, 0);
            g.m.w8(info + 0x14, 0);
            g.m.w8(info + 0x15, 1);
            i += 1;
        }
    }
    g.m.w32(this + 0xd, 0);
    g.m.w32(this + 0x11, 0);
    g.m.w32(this + 0x15, 0);
    g.m.w32(this + 0x2d, 0);
    g.m.wi32(this + 0x19, -1);
    g.m.w32(this + 0x1d, 0);
    g.m.w32(this + 0x21, 0);
    g.m.w32(this + 0x29, 0);
    g.m.w32(this + 0x25, 0);
    g.m.w32(this + 0x31, 1);
    g.m.w32(this + 0x35, 0);
    g.m.w32(this + 0x39, 0);
    g.m.w32(this + 0x51, 0);
    g.m.wi32(this + 0x3d, -1);
    g.m.w32(this + 0x41, 0);
    g.m.w32(this + 0x45, 0);
    g.m.w32(this + 0x4d, 0);
    g.m.w32(this + 0x49, 0);
    let pending = g.m.r32(this + 0x59);
    if pending != 0 {
        G_WeaponHandler_ChangeWeapon(g, this, TYPE_GROUND, pending);
        g.m.w32(this + 0x59, 0);
    }
    let cur = if a0 {
        let lvl = G_Game_GetCurrentLevelNumber(g);
        sub_448890(g, lvl)
    } else {
        g.m.r32(this + 0x55)
    };
    if cur != 0 {
        G_WeaponHandler_ChangeWeapon(g, this, TYPE_AIR, cur);
        g.m.w32(this + 0x55, 0);
    }
    G_WeaponHandler_Crosshair_Hilite(g, this, false);
    g.m.w32(this + 0xef, 0);
    g.m.w32(this + 0xf3, 0x42c80000);
    let v = g.m.r32(this + 0x11d);
    g.m.w32(this + 0xf7, v);
}

/// `public: void __thiscall G_WeaponHandler::ChangeWeapon(enum G_WepDef_Type, struct G_WepDef *)` — 0x447110, 400 bytes, decomp: complete. 
pub fn G_WeaponHandler_ChangeWeapon(g: &mut G, this: u32, a0: i32, a1: u32) {
    match a0 as u32 {
        0x20585541 => {
            // Auxiliary weapons toggle an entry in the weapon-info list.
            let l = g.m.r32(this + 0x73);
            let id = g.m.r32(a1 + 4) as i32;
            let info = G_WeaponHandler_Priv_WeaponInfoList_GetPtrToWeaponInfo(g, this, l, id);
            if info != 0 {
                list::find_and_delete(&mut g.m, l, info);
                g.m.free(info);
            } else {
                let info = g.m.alloc(0x16);
                list::add(&mut g.m, l, info);
                g.m.w32(info, a1);
                g.m.w32(info + 0xc, 0);
                g.m.w32(info + 0x10, 0);
                g.m.w8(info + 0x14, 0);
                g.m.w8(info + 0x15, 1);
            }
        }
        0x41414550 => {
            if g.m.r32(this + 0x5d) != a1 {
                if g.m.r32(this + 0x11) == 0 {
                    g.m.w32(this + 0x5d, a1);
                    g.m.w32(this + 0x69, 0);
                    g.m.w32(this + 0x6d, 0);
                    g.m.w8(this + 0x71, 0);
                    g.m.w8(this + 0x72, 1);
                    g.m.w32(this + 0x55, 0);
                } else {
                    g.m.w32(this + 0x55, a1);
                }
            } else {
                g.m.w32(this + 0x55, 0);
            }
        }
        0x47414550 => {
            if g.m.r32(this + 0x77) != a1 {
                if g.m.r32(this + 0x35) == 0 {
                    g.m.w32(this + 0x77, a1);
                    g.m.w32(this + 0x83, 0);
                    g.m.w32(this + 0x87, 0);
                    g.m.w8(this + 0x8b, 0);
                    g.m.w8(this + 0x8c, 1);
                    g.m.w32(this + 0x59, 0);
                } else {
                    g.m.w32(this + 0x59, a1);
                }
            } else {
                g.m.w32(this + 0x59, 0);
            }
        }
        _ => {}
    }
}

/// `public: enum G_WepHandler_Result __thiscall G_WeaponHandler::Process(struct U_Utils_Vector *, bool, bool, bool, int, bool *)` — 0x4472e0, 1552 bytes, decomp: compiling. 
pub fn G_WeaponHandler_Process(g: &mut G, this: u32, a0: u32, a1: bool, a2: bool, a3: bool, a4: i32, a5: u32) -> i32 {
    // a0 = location, a1 = ground fire, a2 = air fire, a3 = weapon swap,
    // a4 = game time, a5 = out: weapon changed (bool).
    let sp = g.m.sp;
    let res = g.m.alloca(4);
    let time = a4;
    let one = g.m.rf64(C_ONE);
    let hundred = g.m.rf64(C_HUNDRED);
    let mut spawn_ground = false;
    let mut spawn_air = false;
    let mut spawn_aux = false;
    g.m.w8(a5, 0);
    if a2 {
        let v = g.m.ri32(this + 0x2d).wrapping_add(1);
        g.m.wi32(this + 0x2d, v);
    } else {
        g.m.w32(this + 0x2d, 0);
    }
    if a1 {
        let v = g.m.ri32(this + 0x51).wrapping_add(1);
        g.m.wi32(this + 0x51, v);
    } else {
        g.m.w32(this + 0x51, 0);
    }
    if !a2 {
        let s = g.m.r32(this + 0x11);
        if s == 1 || s == 2 {
            g.m.w32(this + 0x11, 3);
            g.m.wi32(this + 0x15, time);
            let e = g.m.ri32(this + 0x19);
            G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum(g, time, e);
            g.m.wi32(res, 2);
        }
    }
    if !a1 {
        let s = g.m.r32(this + 0x35);
        if s == 1 || s == 2 {
            g.m.w32(this + 0x35, 3);
            g.m.wi32(this + 0x39, time);
            let e = g.m.ri32(this + 0x3d);
            G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum(g, time, e);
            g.m.wi32(res, 2);
        }
    }
    let w5d = g.m.r32(this + 0x5d);
    if g.m.r8(w5d + 0x1aa) == 0 {
        G_WeaponHandler_Priv_AirPowerup_Process(g, this, time, a0, w5d, res);
    }
    // The ground weapon's powerup state machine (an inlined copy of the
    // air one, on +0x35..+0x51 and the +0x1e2 bank).
    let w77 = g.m.r32(this + 0x77);
    if !(g.m.r32(w77 + 0x1e6) == NONE && g.m.r32(w77 + 0x1f6) == NONE) {
        match g.m.r32(this + 0x35) {
            0 => {
                if g.m.ri32(this + 0x51) >= g.m.ri32(w77 + 0x1e2) {
                    g.m.w32(this + 0x51, 0);
                    let w = g.m.r32(this + 0x77);
                    if g.m.r32(w + 0x1e6) != NONE {
                        let sp2 = g.m.sp;
                        let req = g.m.alloca(0x28);
                        let rf = g.m.alloca(8);
                        init_req(g, req);
                        let v = g.m.r32(w + 0x1e6);
                        g.m.w32(req, v);
                        let v = g.m.r32(this + 0x119);
                        g.m.w32(req + 0x12, v);
                        let v = g.m.r32(a0);
                        g.m.w32(req + 4, v);
                        let v = g.m.r32(a0 + 4);
                        g.m.w32(req + 8, v);
                        G_EG_RequestSpawn(g, req, rf, 0);
                        let v = g.m.r32(rf + 4);
                        g.m.w32(this + 0x3d, v);
                        g.m.pop_to(sp2);
                    }
                    g.m.w32(this + 0x35, 1);
                    g.m.wi32(this + 0x39, time);
                    g.m.wi32(this + 0x41, time);
                    g.m.w32(this + 0x45, 0);
                    g.m.wi32(this + 0x4d, time);
                    g.m.w32(this + 0x49, 0);
                }
            }
            1 => {
                if time > g.m.ri32(this + 0x41).wrapping_add(g.m.ri32(w77 + 0x1ea)) {
                    let c = g.m.ri32(this + 0x45).wrapping_add(1);
                    g.m.wi32(this + 0x45, c);
                    let w = g.m.r32(this + 0x77);
                    let max = g.m.ri32(w + 0x1ee);
                    let v = percentage(c, max, one, hundred);
                    g.m.wf32(this + 0x49, v);
                    let w = g.m.r32(this + 0x77);
                    let max = g.m.ri32(w + 0x1ee);
                    if g.m.ri32(this + 0x45) > max {
                        g.m.wi32(this + 0x45, max);
                        g.m.w32(this + 0x49, 0x42c80000);
                        let w = g.m.r32(this + 0x77);
                        if g.m.r8(w + 0x1fe) != 0 {
                            g.m.w32(this + 0x35, 3);
                            g.m.wi32(this + 0x39, time);
                            let e = g.m.ri32(this + 0x3d);
                            G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum(g, time, e);
                        }
                    } else {
                        g.m.wi32(this + 0x41, time);
                    }
                }
            }
            3 => {
                if g.m.ri32(this + 0x45) > 0 {
                    if time > g.m.ri32(this + 0x4d).wrapping_add(g.m.ri32(w77 + 0x1fa)) {
                        let sp2 = g.m.sp;
                        let req = g.m.alloca(0x28);
                        init_req(g, req);
                        let w = g.m.r32(this + 0x77);
                        let v = g.m.r32(w + 0x1f6);
                        g.m.w32(req, v);
                        let v = g.m.r32(this + 0x119);
                        g.m.w32(req + 0x12, v);
                        let v = g.m.r32(a0);
                        g.m.w32(req + 4, v);
                        let v = g.m.r32(a0 + 4);
                        g.m.w32(req + 8, v);
                        G_EG_RequestSpawn(g, req, 0, 0);
                        g.m.pop_to(sp2);
                        g.m.wi32(this + 0x4d, time);
                        let c = g.m.ri32(this + 0x45).wrapping_sub(1);
                        g.m.wi32(this + 0x45, c);
                        let w = g.m.r32(this + 0x77);
                        let max = g.m.ri32(w + 0x1ee);
                        let v = percentage(c, max, one, hundred);
                        g.m.wf32(this + 0x49, v);
                    }
                } else {
                    g.m.w32(this + 0x35, 0);
                    g.m.wi32(this + 0x39, time);
                    g.m.w32(this + 0x45, 0);
                    g.m.w32(this + 0x49, 0);
                    let pending = g.m.r32(this + 0x59);
                    if pending != 0 {
                        G_WeaponHandler_ChangeWeapon(g, this, TYPE_GROUND, pending);
                        g.m.w32(this + 0x59, 0);
                    }
                }
            }
            _ => {}
        }
    }
    if a3 && g.m.r8(this + 0xb) == 0 {
        // Weapon swap: step the air weapon to the next one of its type.
        let w = g.m.r32(this + 0x5d);
        let id = g.m.ri32(w + 4);
        let lvl = G_Game_GetCurrentLevelNumber(g);
        let next = G_WepDef_MasterList_GetNextRefPtrByType(g, TYPE_AIR, id, lvl);
        if next != 0 {
            G_WeaponHandler_ChangeWeapon(g, this, TYPE_AIR, next);
        }
        g.m.w8(this + 8, 1);
        g.m.w8(a5, 1);
        let id = G_Res_GetPermSoundID(g, 0x12);
        sound_default(g, id, 0x4b, 0x64);
    } else if a2 && g.m.r32(this + 0x11) == 0 {
        spawn_air = G_WeaponHandler_Priv_CheckSpawning_Air(g, this, time);
        spawn_aux = G_WeaponHandler_Priv_CheckSpawning_Auxilary(g, this, time);
    }
    if g.m.ri32(this + 0x87) > 0 {
        let w = g.m.r32(this + 0x77);
        let t = g.m.ri32(this + 0x7f).wrapping_add(g.m.ri32(w + 0x1b0));
        if time > t {
            let v = g.m.ri32(this + 0x87).wrapping_sub(1);
            g.m.wi32(this + 0x87, v);
            spawn_ground = true;
            g.m.wi32(this + 0x7f, time);
            g.m.wi32(this + 0x7b, time);
        }
    } else if a1 && g.m.r8(this + 9) == 0 && g.m.r32(this + 0x35) == 0 {
        spawn_ground = G_WeaponHandler_Priv_CheckSpawning_Ground(g, this, time);
    }
    g.m.w8(this + 9, a1 as u8);
    g.m.w8(this + 0xa, a2 as u8);
    g.m.w8(this + 0xb, a3 as u8);
    g.m.w8(this + 0x117, 1);
    let w = g.m.r32(this + 0x77);
    let v = g.m.r32(w + 0x166);
    g.m.w32(this + 0xa9, v);
    let w = g.m.r32(this + 0x77);
    let v = g.m.r32(w + 0x16a);
    g.m.w32(this + 0xad, v);
    g.m.w8(this + 0x118, 0);
    g.m.w8(this + 0xc5, 0);
    G_GameObject_CalculateDimensions(g, this + 0x8d);
    if g.m.r32(this + 0xa9) != NONE {
        G_GameObject_AdjustVisibilityAndTinting(g, this + 0x8d);
    }
    if spawn_ground {
        G_WeaponHandler_Priv_Spawn_Ground(g, this, a0);
    }
    if spawn_air {
        G_WeaponHandler_Priv_Spawn_Air(g, this, a0);
    }
    if spawn_aux {
        G_WeaponHandler_Priv_Spawn_Auxilary(g, this, a0);
    }
    G_WeaponHandler_Crosshair_Hilite(g, this, false);
    let r = g.m.ri32(res);
    g.m.pop_to(sp);
    r
}

/// `public: void __thiscall G_WeaponHandler::Crosshair_Hilite(bool)` — 0x4478f0, 80 bytes, decomp: complete. 
pub fn G_WeaponHandler_Crosshair_Hilite(g: &mut G, this: u32, a0: bool) {
    if g.m.r8(this + 0x117) != 0 {
        let w = g.m.r32(this + 0x77);
        if a0 {
            let v = g.m.r32(w + 0x172);
            g.m.w32(this + 0xad, v);
            g.m.w8(this + 0x118, 1);
        } else {
            let v = g.m.r32(w + 0x16a);
            g.m.w32(this + 0xad, v);
            g.m.w8(this + 0x118, 0);
        }
    }
}

/// `public: void __thiscall G_WeaponHandler::UpdateLoc(struct U_Utils_Vector *)` — 0x447940, 32 bytes, decomp: complete. 
pub fn G_WeaponHandler_UpdateLoc(g: &mut G, this: u32, a0: u32) {
    if a0 != 0 {
        let v = g.m.r32(a0);
        g.m.w32(this, v);
        let v = g.m.r32(a0 + 4);
        g.m.w32(this + 4, v);
    }
}

/// `public: float __thiscall G_WeaponHandler::AirPowerup_GetPercentage(void)` — 0x447960, 16 bytes, decomp: complete. 
pub fn G_WeaponHandler_AirPowerup_GetPercentage(g: &mut G, this: u32) -> f32 {
    g.m.rf32(this + 0x25)
}

/// `public: bool __thiscall G_WeaponHandler::AirWeapon_HasChanged(void)` — 0x447970, 16 bytes, decomp: complete. 
pub fn G_WeaponHandler_AirWeapon_HasChanged(g: &mut G, this: u32) -> bool {
    g.m.rb(this + 8)
}

/// `public: void __thiscall G_WeaponHandler::AirWeapon_GetScoreBarInfo(struct G_WepHandler_ScoreBarInfo *)` — 0x447980, 320 bytes, decomp: compiling. 
pub fn G_WeaponHandler_AirWeapon_GetScoreBarInfo(g: &mut G, this: u32, a0: u32) {
    // out: {face, frame} of the current weapon, of the next one, and of the
    // one after that (NONE when it would repeat).
    let cur = G_WeaponHandler_AirWeapon_GetCurrentOrQueuedWeaponRefPtr(g, this);
    let lvl = G_Game_GetCurrentLevelNumber(g);
    let v = g.m.r32(cur + 0x130);
    g.m.w32(a0, v);
    let v = g.m.r32(cur + 0x134);
    g.m.w32(a0 + 4, v);
    let id = g.m.ri32(cur + 4);
    let next = G_WepDef_MasterList_GetNextRefPtrByType(g, TYPE_AIR, id, lvl);
    if next == 0 {
        let v = g.m.r32(cur + 0x130);
        g.m.w32(a0 + 8, v);
        let v = g.m.r32(cur + 0x134);
        g.m.w32(a0 + 0xc, v);
    } else {
        let v = g.m.r32(next + 0x130);
        g.m.w32(a0 + 8, v);
        let v = g.m.r32(next + 0x134);
        g.m.w32(a0 + 0xc, v);
    }
    if g.m.r32(a0 + 8) == g.m.r32(a0) && g.m.r32(a0 + 0xc) == g.m.r32(a0 + 4) {
        g.m.w32(a0 + 8, NONE);
        g.m.w32(a0 + 0xc, 0);
        return;
    }
    // (`test eax,eax; jne` with eax == 0 reads through a null pointer in the
    // original; unreachable, as the equality above always holds then.)
    if next == 0 {
        let v = g.m.r32(0x130);
        g.m.w32(a0 + 0x10, v);
        let v = g.m.r32(0x134);
        g.m.w32(a0 + 0x14, v);
    } else {
        let id = g.m.ri32(next + 4);
        let next2 = G_WepDef_MasterList_GetNextRefPtrByType(g, TYPE_AIR, id, lvl);
        if next2 == 0 {
            let v = g.m.r32(cur + 0x130);
            g.m.w32(a0 + 0x10, v);
            let v = g.m.r32(cur + 0x134);
            g.m.w32(a0 + 0x14, v);
        } else {
            let v = g.m.r32(next2 + 0x130);
            g.m.w32(a0 + 0x10, v);
            let v = g.m.r32(next2 + 0x134);
            g.m.w32(a0 + 0x14, v);
        }
    }
    let e = g.m.r32(a0 + 0x10);
    let f = g.m.r32(a0 + 0x14);
    if (e == g.m.r32(a0) && f == g.m.r32(a0 + 4)) || (e == g.m.r32(a0 + 8) && f == g.m.r32(a0 + 0xc)) {
        g.m.w32(a0 + 0x10, NONE);
        g.m.w32(a0 + 0x14, 0);
    }
}

/// `public: struct G_WepDef * __thiscall G_WeaponHandler::AirWeapon_GetCurrentOrQueuedWeaponRefPtr(void)` — 0x447ac0, 16 bytes, decomp: complete. 
pub fn G_WeaponHandler_AirWeapon_GetCurrentOrQueuedWeaponRefPtr(g: &mut G, this: u32) -> u32 {
    let cur = g.m.r32(this + 0x55);
    if cur != 0 {
        cur
    } else {
        g.m.r32(this + 0x5d)
    }
}

/// `private: bool __thiscall G_WeaponHandler::Priv_CheckSpawning_Ground(int)` — 0x447c70, 240 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_CheckSpawning_Ground(g: &mut G, this: u32, a0: i32) -> bool {
    let mut result = false;
    let w = g.m.r32(this + 0x77);
    if a0 > g.m.ri32(this + 0x7b).wrapping_add(g.m.ri32(w + 0x1ac)) {
        let lvl = G_Game_GetCurrentLevelNumber(g);
        let b = lvl.wrapping_sub(1);
        let r = ftol(G_Res_GetPermFloat(g, 0x97) as f64);
        g.m.wi32(this + 0x87, b.wrapping_add(r));
        let max = ftol(G_Res_GetPermFloat(g, 0x98) as f64);
        if g.m.ri32(this + 0x87) > max {
            g.m.wi32(this + 0x87, max);
        }
        let v = g.m.ri32(this + 0x87).wrapping_sub(1);
        g.m.wi32(this + 0x87, v);
        g.m.wi32(this + 0x7b, a0);
        g.m.wi32(this + 0x7f, a0);
        result = true;
    }
    result
}

/// `private: bool __thiscall G_WeaponHandler::Priv_CheckSpawning_Air(int)` — 0x447d60, 80 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_CheckSpawning_Air(g: &mut G, this: u32, a0: i32) -> bool {
    let mut result = false;
    let w = g.m.r32(this + 0x5d);
    if a0 > g.m.ri32(this + 0x61).wrapping_add(g.m.ri32(w + 0x1ac)) {
        let mut ok = true;
        if g.m.r8(w + 0x1aa) == 0 && g.m.r8(this + 0xa) != 0 {
            ok = false;
        }
        if ok {
            let v = g.m.ri32(this + 0x6d).wrapping_add(1);
            g.m.wi32(this + 0x6d, v);
            g.m.wi32(this + 0x61, a0);
            g.m.wi32(this + 0x65, a0);
            result = true;
        }
    }
    result
}

/// `private: bool __thiscall G_WeaponHandler::Priv_CheckSpawning_Auxilary(int)` — 0x447db0, 160 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_CheckSpawning_Auxilary(g: &mut G, this: u32, a0: i32) -> bool {
    let mut result = false;
    let l = g.m.r32(this + 0x73);
    if l != 0 {
        let count = list::num(&g.m, l);
        let sp = g.m.sp;
        let r = g.m.alloca(8);
        default_ref(g, r);
        let mut i = 0;
        while i < count {
            list::next(&mut g.m, l, r);
            let info = g.m.r32(r + 4);
            let w = g.m.r32(info);
            let t = g.m.ri32(info + 4).wrapping_add(g.m.ri32(w + 0x1ac));
            if a0 > t {
                let mut ok = true;
                if g.m.r8(w + 0x1aa) == 0 && g.m.r8(this + 0xa) != 0 {
                    ok = false;
                }
                if ok {
                    let v = g.m.ri32(info + 0x10).wrapping_add(1);
                    g.m.wi32(info + 0x10, v);
                    g.m.wi32(info + 4, a0);
                    g.m.wi32(info + 8, a0);
                    result = true;
                }
            }
            i += 1;
        }
        g.m.pop_to(sp);
    }
    result
}

/// `private: void __thiscall G_WeaponHandler::Priv_AirPowerup_Process(int, struct U_Utils_Vector *, struct G_WepDef *, enum G_WepHandler_Result *)` — 0x447e50, 944 bytes, decomp: compiling. 
pub fn G_WeaponHandler_Priv_AirPowerup_Process(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32, a3: u32) {
    // a0 = time, a1 = location, a2 = the air weapon, a3 = out: result.
    let time = a0;
    let wep = a2;
    let one = g.m.rf64(C_ONE);
    let hundred = g.m.rf64(C_HUNDRED);
    if g.m.r32(wep + 0x1c8) == NONE && g.m.r32(wep + 0x1d8) == NONE {
        return;
    }
    match g.m.r32(this + 0x11) {
        0 => {
            if g.m.ri32(this + 0x2d) < g.m.ri32(wep + 0x1c4) {
                return;
            }
            g.m.w32(this + 0x2d, 0);
            if g.m.r32(wep + 0x1c8) != NONE {
                let sp = g.m.sp;
                let req = g.m.alloca(0x28);
                let rf = g.m.alloca(8);
                init_req(g, req);
                let v = g.m.r32(wep + 0x1c8);
                g.m.w32(req, v);
                let v = g.m.r32(this + 0x119);
                g.m.w32(req + 0x12, v);
                let v = g.m.r32(a1);
                g.m.w32(req + 4, v);
                let v = g.m.r32(a1 + 4);
                g.m.w32(req + 8, v);
                G_EG_RequestSpawn(g, req, rf, 0);
                let v = g.m.r32(rf + 4);
                g.m.w32(this + 0x19, v);
                g.m.pop_to(sp);
            }
            g.m.w32(this + 0x11, 1);
            g.m.wi32(this + 0x15, time);
            g.m.wi32(this + 0x1d, time);
            g.m.w32(this + 0x21, 0);
            g.m.wi32(this + 0x29, time);
            g.m.w32(this + 0x25, 0);
        }
        1 => {
            let d = g.m.ri32(wep + 0x1d4);
            if d > 0 && time > g.m.ri32(this + 0x15).wrapping_add(d) {
                g.m.w32(a3, 1);
                g.m.w32(this + 0x11, 2);
                g.m.wi32(this + 0x15, time);
            }
            if g.m.r32(this + 0x11) != 1 {
                return;
            }
            if time > g.m.ri32(this + 0x1d).wrapping_add(g.m.ri32(wep + 0x1cc)) {
                let c = g.m.ri32(this + 0x21).wrapping_add(1);
                g.m.wi32(this + 0x21, c);
                let max = g.m.ri32(wep + 0x1d0);
                let v = percentage(c, max, one, hundred);
                g.m.wf32(this + 0x25, v);
                if g.m.ri32(this + 0x21) > max {
                    g.m.wi32(this + 0x21, max);
                    g.m.w32(this + 0x25, 0x42c80000);
                    if g.m.r8(wep + 0x1e0) != 0 {
                        g.m.w32(this + 0x11, 3);
                        g.m.wi32(this + 0x15, time);
                        let e = g.m.ri32(this + 0x19);
                        G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum(g, time, e);
                        g.m.w32(a3, 2);
                    }
                } else {
                    g.m.wi32(this + 0x1d, time);
                }
            }
        }
        3 => {
            if g.m.ri32(this + 0x21) > 0 {
                if time > g.m.ri32(this + 0x29).wrapping_add(g.m.ri32(wep + 0x1dc)) {
                    let sp = g.m.sp;
                    let req = g.m.alloca(0x28);
                    init_req(g, req);
                    let v = g.m.r32(wep + 0x1d8);
                    g.m.w32(req, v);
                    let v = g.m.r32(this + 0x119);
                    g.m.w32(req + 0x12, v);
                    let v = g.m.r32(a1);
                    g.m.w32(req + 4, v);
                    let v = g.m.r32(a1 + 4);
                    g.m.w32(req + 8, v);
                    G_EG_RequestSpawn(g, req, 0, 0);
                    g.m.pop_to(sp);
                    g.m.wi32(this + 0x29, time);
                    let c = g.m.ri32(this + 0x21).wrapping_sub(1);
                    g.m.wi32(this + 0x21, c);
                    let max = g.m.ri32(wep + 0x1d0);
                    let v = percentage(c, max, one, hundred);
                    g.m.wf32(this + 0x25, v);
                }
            } else {
                g.m.w32(this + 0x11, 0);
                g.m.wi32(this + 0x15, time);
                g.m.w32(this + 0x21, 0);
                g.m.w32(this + 0x25, 0);
                let pending = g.m.r32(this + 0x55);
                if pending != 0 {
                    G_WeaponHandler_ChangeWeapon(g, this, TYPE_AIR, pending);
                    g.m.w32(this + 0x55, 0);
                }
            }
        }
        _ => {}
    }
}

/// `private: void __thiscall G_WeaponHandler::Priv_Spawn_Ground(struct U_Utils_Vector *)` — 0x448200, 592 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_Spawn_Ground(g: &mut G, this: u32, a0: u32) {
    let sp = g.m.sp;
    let req = g.m.alloca(0x28);
    let tmp = g.m.alloca(8);
    let r = g.m.alloca(8);
    let w = g.m.r32(this + 0x77);
    let n = spawn_set_count(g, w);
    if n > 0 {
        default_ref(g, r);
        let mut i = 0;
        while i < n {
            let l = g.m.r32(g.m.r32(this + 0x77) + 0x1c0);
            list::next(&mut g.m, l, r);
            let set = g.m.r32(r + 4);
            if g.m.r32(set + 0x20) != NONE {
                spawn_set_request(g, this, set, a0, req);
                G_GameObject_GetLoc_4240e0(g, this + 0x8d, tmp);
                let d = g.m.rf32(this + 4) as f64 - g.m.rf32(tmp + 4) as f64;
                let mut dy = ftol(d);
                if dy < 0 {
                    dy = 0;
                }
                let w = g.m.r32(this + 0x77);
                let c = g.m.ri32(w + 0x17a);
                let abs = if c < 0 { c.wrapping_neg() } else { c };
                let q = if dy == 0 && abs == 0 { f64::from_bits(0xfff8_0000_0000_0000) } else { dy as f64 / abs as f64 };
                g.m.wf32(req + 0x24, q as f32);
                G_EG_RequestSpawn(g, req, 0, 0);
            }
            i += 1;
        }
    }
    let w = g.m.r32(this + 0x77);
    if g.m.r32(w + 0x17e) != NONE {
        init_req(g, req);
        let v = g.m.r32(w + 0x17e);
        g.m.w32(req, v);
        let v = g.m.r32(this + 0x119);
        g.m.w32(req + 0x12, v);
        G_GameObject_GetLoc_4240e0(g, this + 0x8d, req + 4);
        G_EG_RequestSpawn(g, req, 0, 0);
    }
    g.m.pop_to(sp);
}

/// `private: void __thiscall G_WeaponHandler::Priv_Spawn_Air(struct U_Utils_Vector *)` — 0x448450, 320 bytes, decomp: compiling. 
pub fn G_WeaponHandler_Priv_Spawn_Air(g: &mut G, this: u32, a0: u32) {
    if g.m.ri32(this + 0x6d) >= 1 {
        let w = g.m.r32(this + 0x5d);
        let n = spawn_set_count(g, w);
        if n > 0 {
            let sp = g.m.sp;
            let req = g.m.alloca(0x28);
            let r = g.m.alloca(8);
            default_ref(g, r);
            let l = g.m.r32(w + 0x1c0);
            let mut i = 0;
            while i < n {
                list::next(&mut g.m, l, r);
                let set = g.m.r32(r + 4);
                if g.m.r32(set + 0x20) != NONE {
                    spawn_set_request(g, this, set, a0, req);
                    G_EG_RequestSpawn(g, req, 0, 0);
                }
                i += 1;
            }
            g.m.pop_to(sp);
        }
    }
}

/// `private: void __thiscall G_WeaponHandler::Priv_Spawn_Auxilary(struct U_Utils_Vector *)` — 0x448590, 400 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_Spawn_Auxilary(g: &mut G, this: u32, a0: u32) {
    let l = g.m.r32(this + 0x73);
    if l != 0 {
        let count = list::num(&g.m, l);
        let sp = g.m.sp;
        let req = g.m.alloca(0x28);
        let r = g.m.alloca(8);
        let r2 = g.m.alloca(8);
        default_ref(g, r);
        let mut i = 0;
        while i < count {
            list::next(&mut g.m, l, r);
            let info = g.m.r32(r + 4);
            if g.m.ri32(info + 0x10) >= 1 {
                let w = g.m.r32(info);
                let n = spawn_set_count(g, w);
                if n > 0 {
                    default_ref(g, r2);
                    let sl = g.m.r32(w + 0x1c0);
                    let mut k = 0;
                    while k < n {
                        list::next(&mut g.m, sl, r2);
                        let set = g.m.r32(r2 + 4);
                        if g.m.r32(set + 0x20) != NONE {
                            spawn_set_request(g, this, set, a0, req);
                            G_EG_RequestSpawn(g, req, 0, 0);
                        }
                        k += 1;
                    }
                }
            }
            i += 1;
        }
        g.m.pop_to(sp);
    }
}

/// `private: struct G_WeaponHandler::Priv_WeaponInfo * __thiscall G_WeaponHandler::Priv_WeaponInfoList_GetPtrToWeaponInfo(class U_LinkedList *, long)` — 0x4487b0, 128 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_WeaponInfoList_GetPtrToWeaponInfo(g: &mut G, this: u32, a0: u32, a1: i32) -> u32 {
    let mut result = 0;
    let count = list::num(&g.m, a0);
    if count >= 1 {
        let sp = g.m.sp;
        let r = g.m.alloca(8);
        default_ref(g, r);
        let mut i = 0;
        while i < count {
            list::next(&mut g.m, a0, r);
            let info = g.m.r32(r + 4);
            let w = g.m.r32(info);
            if g.m.ri32(w + 4) == a1 {
                result = info;
                break;
            }
            i += 1;
        }
        g.m.pop_to(sp);
    }
    result
}

/// `sub_448890(int level)` — 0x448890, 80 bytes: the first air (PEAA) weapon whose minimum level is `level`, 0 if none.
pub fn sub_448890(g: &mut G, a0: i32) -> u32 {
    // The first air ("PEAA") weapon whose minimum level is `a0`.
    let mut i = 0;
    loop {
        let w = G_WepDef_MasterList_GetRefPtrForIndex(g, i);
        i += 1;
        if w == 0 {
            return 0;
        }
        if g.m.r32(w + 8) == 0x41414550 && g.m.ri32(w + 0x13c) == a0 {
            return w;
        }
    }
}
