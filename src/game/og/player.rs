//! G_Player (0x4305a0..0x435880): a player's state machine, score, lives,
//! shields, money counter, powerup overload and the per-tick movement.
//! The object embeds a `G_GameObject` base (0x8a bytes) and, at +0x235,
//! a `G_WeaponHandler` (and, at +0x2c2 inside that, the crosshair
//! `G_GameObject`).
//!
//! Field offsets (checked against the disassembly):
//! +0x10/+0x14 velocity x/y (floats), +0x20 sprite frame, +0x34 scaled
//! flag, +0x4a sprite name, +0x52..+0x60 powerup-overload bar,
//! +0x62..+0x6a perm floats, +0x7a scale, +0x8a G_PlayerDef*,
//! +0x8e lives, +0x92/+0x96 extra-life thresholds, +0x9a speed limit,
//! +0x9e shields, +0xa2 money, +0xa6 score, +0xaa multiplier, +0xae
//! multiplier entity number, +0xb3 cheating, +0xb4 last level id, +0xb8
//! in game, +0xb9 appearing flag, +0xba state, +0xbe state data, +0xc2
//! player number, +0xc6 game type, +0xca/+0xcb invulnerable, +0xcc
//! took-damage, +0xcd low-shields spawned, +0xce sprite timer, +0xd2..
//! +0x1f2 money counter, +0x1f6 inputs[7], +0x1fd/+0x201 hit timers,
//! +0x205 scroll lag, +0x209..+0x217 powerup overload, +0x21b..+0x233
//! licence record.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::bgnd::G_Bgnd_AdjustSideScroll;
use super::eg::{G_EG_DisposeByUniqueEntityNum, G_EG_DisposePlayersChildren, G_EG_RequestSpawn};
use super::film::{G_Film_GetInputs, G_Film_SetInputs};
use super::g::G;
use super::game::{G_Game_GetCurrentLevelNumber, G_Game_IsWaitingForLevelToEnd};
use super::gameobject::*;
use super::input::G_Input_GetCachedPlayerInputs;
use super::level::G_Level_HackTest_Fux0r;
use super::resource::*;
use super::scorebar::{G_ScoreBar_SetPowerBarPercentage, G_ScoreBar_SetShieldBarPercentage};
use super::umath::ftol;
use super::weaponhandler::*;

/// The 'none' id.
const NONE: u32 = 0x656e6f6e;
/// `g_shieldOffset` (0x4eca70): the float added to the shields.
const SHIELD_OFFSET: u32 = 0x4eca70;
/// `g_shieldDefault` (0x4eca57): 0.0f, also the "zero" passed to the bars.
const SHIELD_DEFAULT: u32 = 0x4eca57;
/// The default spawn request (40 bytes) every spawn starts from.
const SPAWN_DEFAULT: u32 = 0x4ec8eb;
/// Reset position globals.
const RESET_POS_X: u32 = 0x4ec360;
const RESET_POS_Y: u32 = 0x4ec364;

/// `(int)G_Res_GetPermFloat(i)` through the fist-and-fix-up idiom.
fn pf_int(g: &mut G, i: i32) -> i32 {
    ftol(G_Res_GetPermFloat(g, i) as f64)
}

/// A fresh spawn request on the scratch stack, copied from the default.
fn new_spawn_request(g: &mut G) -> u32 {
    let req = g.m.alloca(0x28);
    g.m.copy(req, SPAWN_DEFAULT, 0x28);
    req
}

/// The spawn every site below shares: default request, `id` at +0,
/// the player's location at +4, the player number at +0x12.
fn request_spawn_at_player(g: &mut G, this: u32, id: u32) {
    let sp = g.m.sp;
    let req = new_spawn_request(g);
    g.m.w32(req, id);
    G_GameObject_GetLoc_4240e0(g, this, req + 4);
    let n = g.m.r32(this + 0xc2);
    g.m.w32(req + 0x12, n);
    G_EG_RequestSpawn(g, req, 0, 0);
    g.m.pop_to(sp);
}

/// `U_Sound_Play(U_Sound_Settings*, bool)` (0x44f5f0): pitch from
/// RandomFloat(+0x10, +0x14), then RandomInt(+4, +4) (no draw).
fn sound_play_settings(g: &mut G, s: u32, flag: bool) {
    if g.m.r32(s) == NONE {
        return;
    }
    let vol = g.m.ri32(s + 4);
    let lo = g.m.rf32(s + 0x10);
    let hi = g.m.rf32(s + 0x14);
    let pitch = g.random_float(lo, hi) as f32;
    let v = g.random_int(vol, vol);
    let id = g.m.r32(s);
    g.host.sound_play(id, v, pitch);
}

/// `U_Sound_Play(long id, priority, volume, loop)` (0x44f660): the pitch
/// is the global at 0x4ffa86.
fn sound_play_id(g: &mut G, id: i32, prio: i32, vol: i32, looping: bool) {
    let pitch = f32::from_bits(g.m.r32(0x4ffa86));
    g.host.sound_play(id as u32, vol, pitch);
}

/// `RT3_GetLicenseName` (0x458a00): the name buffer at 0x52adb8, copied
/// from 0x52ac58 when there is no registration (flag byte 0x52b488).
fn rt3_get_license_name(g: &mut G) -> u32 {
    if g.m.r8(0x52b488) != 0 {
        todo!("RT3_GetLicenseName registered path (0x459f30)");
    }
    let s = g.m.cstr(0x52ac58).to_vec();
    g.m.strcpy(0x52adb8, &s);
    0x52adb8
}

/// `RT3_GetLicenseCode` (0x458a40): 64-bit code, 0 when unregistered.
fn rt3_get_license_code(g: &mut G) -> (u32, u32) {
    if g.m.r8(0x52b588) != 0 {
        todo!("RT3_GetLicenseCode registered path (0x463710, 0x459d20)");
    }
    (0, 0)
}

/// `RT3_GetLicenseCopies` (0x4589e0).
fn rt3_get_license_copies(g: &mut G) -> i32 {
    let v = g.m.ri32(0x52b688);
    if v > 0 {
        v
    } else {
        0
    }
}

/// An argument of the `sprintf` calls in the money counter.
enum Part {
    S(u32),
    I(i32),
}
/// `sprintf(dest, ...)` for strings and `%i`.
fn sprintf_parts(g: &mut G, dest: u32, parts: &[Part]) {
    let mut out: Vec<u8> = Vec::new();
    for p in parts {
        match p {
            Part::S(a) => out.extend_from_slice(g.m.cstr(*a)),
            Part::I(v) => out.extend_from_slice(v.to_string().as_bytes()),
        }
    }
    g.m.strcpy(dest, &out);
}

/// The powerup-overload bar reset the original inlines in several places
/// (+0x209..+0x217 and the +0x52..+0x60 group).
fn overload_clear(g: &mut G, this: u32) {
    g.m.w8(this + 0x209, 0);
    g.m.w8(this + 0x20a, 0);
    g.m.w32(this + 0x20b, 0);
    g.m.w32(this + 0x20f, 0);
    g.m.w32(this + 0x213, 0);
    g.m.w32(this + 0x217, 0);
    g.m.w8(this + 0x52, 0);
    g.m.w32(this + 0x54, 0);
    g.m.w32(this + 0x58, 0);
    g.m.w32(this + 0x5c, 0);
    g.m.w16(this + 0x60, 0x7fff);
}

/// `_sub_4305a0` — 0x4305a0: 7 * 3^(bit length of the byte); the unrolled
/// recursion bottoms out at the ninth level, unreachable for a byte.
pub fn sub_4305a0(g: &mut G, a0: u8) -> i32 {
    if a0 == 0 {
        return 7;
    }
    sub_4305a0(g, a0 >> 1).wrapping_mul(3)
}

/// `G_Player::ResetAtLevelStart` — 0x430f40.
pub fn G_Player_ResetAtLevelStart(g: &mut G, this: u32, a0: i32, a1: i32) {
    if !g.m.rb(this + 0xb8) {
        return;
    }
    g.m.w8(this + 0xcc, 0);
    G_WeaponHandler_SetUpForAppearance(g, this + 0x235, true);
    g.m.w8(this + 0xb9, 0);
    g.m.wi32(this + 0xb4, a0);
    G_Player_Priv_ResetSpriteInfo(g, this);
    G_GameObject_CalculateDimensions(g, this);
    G_Player_Priv_ResetPosition(g, this);
    G_Player_SetState(g, this, 2, a1);
    G_GameObject_Glow_Stop(g, this);
    G_Player_Money_Reset(g, this);
    G_Player_MoneyCounter_Reset(g, this);
    G_Player_Shields_Reset(g, this, true);
    G_Player_PowerupOverload_Reset(g, this);
    let v = G_Res_GetPermFloat(g, 0xa3);
    g.m.wf32(this + 0x62, v);
    let v = G_Res_GetPermFloat(g, 0xa4);
    g.m.wf32(this + 0x66, v);
    let v = G_Res_GetPermFloat(g, 0xa5);
    g.m.wf32(this + 0x6a, v);
    let r = g.random_int(400, 0x7d0);
    if g.m.ri32(this + 0xc2) == 0 {
        let name = rt3_get_license_name(g);
        g.m.w32(this + 0x21b, name);
        let (lo, hi) = rt3_get_license_code(g);
        g.m.w32(this + 0x21f, lo);
        g.m.w32(this + 0x223, hi);
        let c = rt3_get_license_copies(g);
        g.m.wi32(this + 0x227, c);
        g.m.wi32(this + 0x22b, r.wrapping_add(a1));
        g.m.w32(this + 0x22f, 0);
    }
}

/// `G_Player::Priv_ResetPosition` — 0x4310a0.
pub fn G_Player_Priv_ResetPosition(g: &mut G, this: u32) {
    let def = g.m.r32(this + 0x8a);
    if g.m.ri32(this + 0xc6) == 1 {
        let y = g.m.ri32(def + 0x92) as f32;
        let x = g.m.ri32(def + 0x8e) as f32;
        G_GameObject_SetLoc_424140(g, this, x, y);
    } else {
        let y = g.m.ri32(def + 0x9a) as f32;
        let x = g.m.ri32(def + 0x96) as f32;
        G_GameObject_SetLoc_424140(g, this, x, y);
    }
    let v = g.m.r32(RESET_POS_X);
    g.m.w32(this + 0x10, v);
    let v = g.m.r32(RESET_POS_Y);
    g.m.w32(this + 0x14, v);
    g.m.w32(this + 0x205, 0);
    G_Player_PowerupOverload_Reset(g, this);
}

/// `G_Player::InGame` — 0x431130.
pub fn G_Player_InGame(g: &mut G, this: u32) -> bool {
    g.m.rb(this + 0xb8)
}

/// `G_Player::IsAboutToBeRemovedFromGame` — 0x431140.
pub fn G_Player_IsAboutToBeRemovedFromGame(g: &mut G, this: u32) -> bool {
    g.m.rb(this + 0xb8) && g.m.ri32(this + 0xba) == 1
}

/// `G_Player::GetState` — 0x431160.
pub fn G_Player_GetState(g: &mut G, this: u32) -> i32 {
    g.m.ri32(this + 0xba)
}

/// `G_Player::IsState` — 0x431170.
pub fn G_Player_IsState(g: &mut G, this: u32, a0: i32) -> bool {
    g.m.ri32(this + 0xba) == a0
}

/// `G_Player::SetState` — 0x431190.
pub fn G_Player_SetState(g: &mut G, this: u32, a0: i32, a1: i32) {
    g.m.wi32(this + 0xba, a0);
    g.m.wi32(this + 0xbe, a1);
}

/// `G_Player::GetNumber` — 0x4311b0.
pub fn G_Player_GetNumber(g: &mut G, this: u32) -> i32 {
    g.m.ri32(this + 0xc2)
}

/// `G_Player::GetPlayerDefinitionRefPtr` — 0x4311c0.
pub fn G_Player_GetPlayerDefinitionRefPtr(g: &mut G, this: u32) -> u32 {
    g.m.r32(this + 0x8a)
}

/// `G_Player::Lives_Get` — 0x431270.
pub fn G_Player_Lives_Get(g: &mut G, this: u32) -> i32 {
    g.m.ri32(this + 0x8e).wrapping_add(0xeadb2311u32 as i32)
}

/// `G_Player::Lives_Add` — 0x4312a0.
pub fn G_Player_Lives_Add(g: &mut G, this: u32, a0: bool) {
    if !g.m.rb(this + 0xb8) {
        return;
    }
    let prev = g.m.ri32(this + 0x8e).wrapping_add(0xeadb2311u32 as i32);
    let mut new_lives = prev.wrapping_add(1);
    let def = g.m.r32(this + 0x8a);
    let max = g.m.ri32(def + 0x5e);
    if max > 0 && new_lives > max {
        new_lives = max;
    }
    if new_lives > prev {
        if g.m.r32(def + 0x6e) != NONE && a0 {
            let id = g.m.r32(def + 0x6e);
            request_spawn_at_player(g, this, id);
        }
        g.m.wi32(this + 0x8e, new_lives.wrapping_add(0x1524dcef));
    }
}

/// `G_Player::PowerupOverload_Reset` — 0x4313b0.
pub fn G_Player_PowerupOverload_Reset(g: &mut G, this: u32) {
    overload_clear(g, this);
}

/// `G_Player::PowerupOverload_Process` — 0x431410.
pub fn G_Player_PowerupOverload_Process(g: &mut G, this: u32, a0: i32) {
    if g.m.ri32(this + 0xba) != 4 {
        if g.m.rb(this + 0x209) {
            overload_clear(g, this);
        }
        return;
    }
    if !g.m.rb(this + 0x209) {
        return;
    }
    if G_Game_IsWaitingForLevelToEnd(g) {
        overload_clear(g, this);
        return;
    }
    let def = g.m.r32(this + 0x8a);
    if g.m.rb(this + 0x20a) {
        let limit = g.m.ri32(this + 0x20f).wrapping_add(g.m.ri32(this + 0x213));
        if a0 > limit {
            g.m.wi32(this + 0x20f, a0);
            let v = (g.m.rf32(this + 0x54) as f64 + 100.0) as f32;
            g.m.wf32(this + 0x54, v);
            // fcomp 100.0; test ah, 5: proceed when not less and ordered
            if !((g.m.rf32(this + 0x54) as f64) < 100.0) && !g.m.rf32(this + 0x54).is_nan() {
                g.m.wf32(this + 0x54, 100.0);
                g.m.w8(this + 0x20a, 0);
                let v = g.m.ri32(this + 0x213).wrapping_sub(1);
                g.m.wi32(this + 0x213, v);
                let def = g.m.r32(this + 0x8a);
                let floor = g.m.ri32(def + 0xe2);
                if g.m.ri32(this + 0x213) < floor {
                    g.m.wi32(this + 0x213, floor);
                }
                let v = g.m.ri32(this + 0x217).wrapping_add(1);
                g.m.wi32(this + 0x217, v);
                let def = g.m.r32(this + 0x8a);
                if g.m.ri32(this + 0x217) == g.m.ri32(def + 0xda) {
                    G_Player_Destroy(g, this, a0);
                    return;
                }
                sound_play_settings(g, def + 0xec, true);
            }
        }
    } else {
        let d = g.m.rf32(this + 0x54) as f64 - g.m.rf32(def + 0xe6) as f64;
        g.m.wf32(this + 0x54, d as f32);
        // fcomp 0.0 on the unrounded difference; proceed when less-or-equal
        if d <= 0.0 {
            g.m.wf32(this + 0x54, 0.0);
            g.m.w8(this + 0x20a, 1);
        }
    }
    g.m.w8(this + 0x52, 0);
    let v = g.m.r32(this + 0x54);
    g.m.w32(this + 0x58, v);
    g.m.w32(this + 0x5c, 0);
    let def = g.m.r32(this + 0x8a);
    let v = g.m.r16(def + 0xea);
    g.m.w16(this + 0x60, v);
}

/// `G_Player::Hit` — 0x431640.
pub fn G_Player_Hit(g: &mut G, this: u32, a0: f32, a1: i32) {
    if g.m.ri32(this + 0xba) != 4 {
        return;
    }
    let def = g.m.r32(this + 0x8a);
    let next = g.m.ri32(this + 0x1fd).wrapping_add(g.m.ri32(def + 0x54));
    if a1 < next {
        return;
    }
    g.m.wi32(this + 0x1fd, a1);
    if !G_Player_IsInvulnerable(g, this) {
        let def = g.m.r32(this + 0x8a);
        let dmg = (g.m.ri32(def + 0x50) as f64 * a0 as f64) as f32;
        let s = G_Player_Shields_GetPercentage(g, this);
        let new = (s as f64 - dmg as f64) as f32;
        if (dmg as f64) > 0.0 {
            g.m.w8(this + 0xcc, 1);
        }
        G_Player_Shields_SetPercentage(g, this, new);
    }
    let s = G_Player_Shields_GetPercentage(g, this);
    if (s as f64) < 0.0 {
        G_Player_Destroy(g, this, a1);
        return;
    }
    let def = g.m.r32(this + 0x8a);
    let gp = g.m.r16(def + 0x58);
    let gt = g.m.ri32(def + 0x5a);
    G_GameObject_Glow_Start(g, this, gp, gt, false);
    if !((a0 as f64) > 0.0) {
        return;
    }
    let def = g.m.r32(this + 0x8a);
    if g.m.r32(def + 0xc6) != NONE {
        let t = pf_int(g, 0xa2);
        let limit = t.wrapping_add(g.m.ri32(this + 0x201));
        if a1 >= limit {
            g.m.wi32(this + 0x201, a1);
            let id = g.m.r32(def + 0xc6);
            request_spawn_at_player(g, this, id);
        }
    }
    if !g.m.rb(this + 0xcd) {
        let def = g.m.r32(this + 0x8a);
        let s = G_Player_Shields_GetPercentage(g, this);
        // fcompp: proceed when def[0x4c] >= shields (ordered)
        if (g.m.ri32(def + 0x4c) as f64) >= s as f64 {
            if g.m.r32(def + 0xca) != NONE {
                let id = g.m.r32(def + 0xca);
                request_spawn_at_player(g, this, id);
            }
            g.m.w8(this + 0xcd, 1);
        }
    }
}

/// `G_Player::Shields_Reset` — 0x431910.
pub fn G_Player_Shields_Reset(g: &mut G, this: u32, a0: bool) {
    if !g.m.rb(this + 0xb8) {
        let v = f32::from_bits(g.m.r32(SHIELD_DEFAULT));
        G_Player_Shields_SetPercentage(g, this, v);
    } else if a0 {
        let def = g.m.r32(this + 0x8a);
        let v = g.m.ri32(def + 0x48) as f32;
        G_Player_Shields_SetPercentage(g, this, v);
    }
    g.m.w32(this + 0x1fd, 0);
    g.m.w32(this + 0x201, 0);
    g.m.w8(this + 0xcd, 0);
}

/// `G_Player::Shields_IncreasePercentage` — 0x431980.
pub fn G_Player_Shields_IncreasePercentage(g: &mut G, this: u32, a0: f32) {
    if !g.m.rb(this + 0xb8) {
        return;
    }
    // a0 == 0.0 returns
    if a0 as f64 == 0.0 {
        return;
    }
    let s = G_Player_Shields_GetPercentage(g, this);
    let sum = s as f64 + a0 as f64;
    let mut new = sum as f32;
    // fcomp 100.0 on the unrounded sum: proceed only when greater
    if sum > 100.0 {
        new = 100.0;
    } else if (new as f64) < 0.0 {
        new = 0.0;
    }
    G_Player_Shields_SetPercentage(g, this, new);
}

/// `G_Player::Shields_GetPercentage` — 0x431a20.
pub fn G_Player_Shields_GetPercentage(g: &mut G, this: u32) -> f32 {
    (g.m.rf32(this + 0x9e) as f64 - g.m.rf32(SHIELD_OFFSET) as f64) as f32
}

/// `G_Player::Shields_SetPercentage` — 0x431a30.
pub fn G_Player_Shields_SetPercentage(g: &mut G, this: u32, a0: f32) {
    let v = (a0 as f64 + g.m.rf32(SHIELD_OFFSET) as f64) as f32;
    g.m.wf32(this + 0x9e, v);
}

/// `G_Player::Money_Reset` — 0x431a50.
pub fn G_Player_Money_Reset(g: &mut G, this: u32) {
    G_Player_Money_Set(g, this, 0);
}

/// `G_Player::Money_Change` — 0x431a70.
pub fn G_Player_Money_Change(g: &mut G, this: u32, a0: i32) {
    if g.m.rb(this + 0xb8) {
        let v = G_Player_Money_Get(g, this).wrapping_add(a0);
        G_Player_Money_Set(g, this, v);
    }
}

/// `G_Player::Money_Get` — 0x431ab0.
pub fn G_Player_Money_Get(g: &mut G, this: u32) -> i32 {
    g.m.ri32(this + 0xa2).wrapping_add(0xfff4d332u32 as i32)
}

/// `G_Player::Money_Set` — 0x431ac0.
pub fn G_Player_Money_Set(g: &mut G, this: u32, a0: i32) {
    g.m.wi32(this + 0xa2, a0.wrapping_add(0xb2cce));
}

/// `G_Player::MoneyCounter_Reset` — 0x431ae0.
pub fn G_Player_MoneyCounter_Reset(g: &mut G, this: u32) {
    g.m.w32(this + 0xd2, 0);
    g.m.w32(this + 0xd6, 0);
    g.m.w32(this + 0xda, 0x20);
    g.m.w32(this + 0xde, 0);
    g.m.w32(this + 0xe2, 0);
    g.m.w8(this + 0xe6, 0);
    g.m.w32(this + 0x1e6, 0);
    g.m.w32(this + 0x1ea, 0);
    g.m.w32(this + 0x1ee, 0);
    g.m.w32(this + 0x1f2, 0);
}

/// `G_Player::MoneyCounter_Start` — 0x431b50.
pub fn G_Player_MoneyCounter_Start(g: &mut G, this: u32, a0: i32, a1: bool) -> bool {
    if !g.m.rb(this + 0xb8) {
        return false;
    }
    g.m.w32(this + 0xd2, 1);
    g.m.wi32(this + 0xd6, a0);
    g.m.wi32(this + 0xe2, a0);
    let s = G_Res_GetPermGameString(g, 0xe);
    let sv = g.m.cstr(s).to_vec();
    g.m.strcpy(this + 0xe6, &sv);
    let level = G_Game_GetCurrentLevelNumber(g);
    if pf_int(g, 0xaa) != 0 {
        let v = pf_int(g, 0xab).wrapping_mul(level);
        g.m.wi32(this + 0xde, v);
    } else {
        let v = pf_int(g, 0xab);
        g.m.wi32(this + 0xde, v);
    }
    let money = g.m.ri32(this + 0xa2).wrapping_add(0xfff4d332u32 as i32);
    g.m.wi32(this + 0x1e6, money);
    let prod = g.m.ri32(this + 0x1e6).wrapping_mul(g.m.ri32(this + 0xde));
    g.m.wi32(this + 0x1ea, prod);
    let prod = g.m.ri32(this + 0x1ea);
    if prod > 0 {
        let mut p = prod as f32;
        p = (G_Res_GetPermFloat(g, 0xc8) as f64 * p as f64) as f32;
        let cap = pf_int(g, 0xc7);
        if (cap as f64) > p as f64 {
            p = cap as f32;
        }
        g.m.wi32(this + 0x1ee, ftol(p as f64));
    } else {
        g.m.w32(this + 0x1ee, 0);
    }
    let def = g.m.r32(this + 0x8a);
    if g.m.r32(def + 0xc2) != NONE {
        let sp = g.m.sp;
        let req = new_spawn_request(g);
        let id = g.m.r32(def + 0xc2);
        g.m.w32(req, id);
        let v = G_Res_GetPermFloat(g, 0xb3);
        g.m.wf32(req + 4, v);
        let v = G_Res_GetPermFloat(g, 0xb4);
        g.m.wf32(req + 8, v);
        if a1 {
            let v = pf_int(g, 0xb5);
            g.m.wi32(this + 0x1f2, v);
            let y = (g.m.ri32(this + 0x1f2) as f64 + g.m.rf32(req + 8) as f64) as f32;
            g.m.wf32(req + 8, y);
        }
        let n = g.m.r32(this + 0xc2);
        g.m.w32(req + 0x12, n);
        G_EG_RequestSpawn(g, req, 0, 0);
        g.m.pop_to(sp);
    }
    true
}

/// `G_Player::MoneyCounter_Process` — 0x431ea0.
pub fn G_Player_MoneyCounter_Process(g: &mut G, this: u32, a0: i32) -> bool {
    let mut result = false;
    let state = g.m.r32(this + 0xd2);
    match state {
        0 => result = true,
        1 => {
            let t = pf_int(g, 0xac).wrapping_add(g.m.ri32(this + 0xd6));
            if a0 > t {
                g.m.w32(this + 0xd2, 2);
                g.m.wi32(this + 0xd6, a0);
                let sid = G_Res_GetPermSoundID(g, 0x14);
                sound_play_id(g, sid, 0x32, 0x64, false);
            }
        }
        2 => {
            let n = pf_int(g, 0xb1);
            let mut i = 0;
            while i < n {
                if g.m.r32(this + 0xda) != 0 {
                    let v = g.m.r32(this + 0xda).wrapping_sub(1);
                    g.m.w32(this + 0xda, v);
                }
                i += 1;
            }
            let t = pf_int(g, 0xad).wrapping_add(g.m.ri32(this + 0xd6));
            if a0 > t {
                g.m.w32(this + 0xd2, 3);
                g.m.wi32(this + 0xd6, a0);
                let sid = G_Res_GetPermSoundID(g, 0x14);
                sound_play_id(g, sid, 0x32, 0x64, false);
                let s0 = G_Res_GetPermGameString(g, 0xe);
                let m = g.m.ri32(this + 0x1e6);
                sprintf_parts(g, this + 0xe6, &[Part::S(s0), Part::I(m)]);
            }
        }
        3 => {
            let t = pf_int(g, 0xad).wrapping_add(g.m.ri32(this + 0xd6));
            if a0 > t {
                g.m.w32(this + 0xd2, 4);
                g.m.wi32(this + 0xd6, a0);
                let sid = G_Res_GetPermSoundID(g, 0x14);
                sound_play_id(g, sid, 0x32, 0x64, false);
                let s1 = G_Res_GetPermGameString(g, 0xf);
                let s0 = G_Res_GetPermGameString(g, 0xe);
                let (m, mult) = (g.m.ri32(this + 0x1e6), g.m.ri32(this + 0xde));
                sprintf_parts(g, this + 0xe6, &[Part::S(s0), Part::I(m), Part::S(s1), Part::I(mult)]);
            }
        }
        4 => {
            let t = pf_int(g, 0xad).wrapping_add(g.m.ri32(this + 0xd6));
            if a0 > t {
                g.m.w32(this + 0xd2, 5);
                g.m.wi32(this + 0xd6, a0);
                let sid = if g.m.ri32(this + 0x1ea) > 0 {
                    G_Res_GetPermSoundID(g, 0x14)
                } else {
                    G_Res_GetPermSoundID(g, 0x15)
                };
                sound_play_id(g, sid, 0x32, 0x64, false);
                let s2 = G_Res_GetPermGameString(g, 0x10);
                let s1 = G_Res_GetPermGameString(g, 0xf);
                let s0 = G_Res_GetPermGameString(g, 0xe);
                let (m, mult, total) =
                    (g.m.ri32(this + 0x1e6), g.m.ri32(this + 0xde), g.m.ri32(this + 0x1ea));
                sprintf_parts(
                    g,
                    this + 0xe6,
                    &[Part::S(s0), Part::I(m), Part::S(s1), Part::I(mult), Part::S(s2), Part::I(total)],
                );
            }
        }
        5 => {
            let t = pf_int(g, 0xad).wrapping_add(g.m.ri32(this + 0xd6));
            if a0 > t {
                g.m.w32(this + 0xd2, 6);
                g.m.wi32(this + 0xd6, a0);
            }
        }
        6 => {
            if g.m.ri32(this + 0x1ea) > 0 {
                let t = pf_int(g, 0xb0).wrapping_add(g.m.ri32(this + 0xe2));
                if a0 > t {
                    g.m.wi32(this + 0xe2, a0);
                    if g.m.rb(this + 0xb8) {
                        let v = g.m.r32(this + 0xa2).wrapping_sub(1);
                        g.m.w32(this + 0xa2, v);
                    }
                    let step = g.m.ri32(this + 0x1ee);
                    G_Player_Score_Adjust(g, this, step, false);
                    let step = g.m.ri32(this + 0x1ee);
                    let v = g.m.ri32(this + 0x1ea).wrapping_sub(step);
                    g.m.wi32(this + 0x1ea, v);
                    let sid = G_Res_GetPermSoundID(g, 0x14);
                    sound_play_id(g, sid, 0x32, 0x64, false);
                    let total = g.m.ri32(this + 0x1ea);
                    let s2 = G_Res_GetPermGameString(g, 0x10);
                    let mult = g.m.ri32(this + 0xde);
                    let s1 = G_Res_GetPermGameString(g, 0xf);
                    let m = g.m.ri32(this + 0x1e6);
                    let s0 = G_Res_GetPermGameString(g, 0xe);
                    sprintf_parts(
                        g,
                        this + 0xe6,
                        &[Part::S(s0), Part::I(m), Part::S(s1), Part::I(mult), Part::S(s2), Part::I(total)],
                    );
                }
            } else {
                g.m.w32(this + 0xd2, 7);
                g.m.wi32(this + 0xd6, a0);
            }
        }
        7 => {
            let t = pf_int(g, 0xae).wrapping_add(g.m.ri32(this + 0xd6));
            if a0 > t {
                g.m.w32(this + 0xd2, 8);
                g.m.wi32(this + 0xd6, a0);
            }
        }
        8 => {
            let t = pf_int(g, 0xaf).wrapping_add(g.m.ri32(this + 0xd6));
            if a0 > t {
                result = true;
            }
        }
        _ => {}
    }
    result
}

/// `G_Player::MoneyCounter_IsActive` — 0x432490.
pub fn G_Player_MoneyCounter_IsActive(g: &mut G, this: u32) -> bool {
    g.m.r32(this + 0xd2) != 0
}

/// `G_Player::IsInvulnerable` — 0x4324a0.
pub fn G_Player_IsInvulnerable(g: &mut G, this: u32) -> bool {
    g.m.rb(this + 0xca)
}

/// `G_Player::SetInvulnerability` — 0x4324b0.
pub fn G_Player_SetInvulnerability(g: &mut G, this: u32, a0: bool, a1: bool) {
    if !g.m.rb(this + 0xb8) {
        return;
    }
    if a0 {
        if a1 {
            g.m.w8(this + 0xcb, 1);
        }
        g.m.w8(this + 0xca, 1);
    } else if !g.m.rb(this + 0xcb) || a1 {
        g.m.w8(this + 0xca, 0);
        g.m.w8(this + 0xcb, 0);
    }
}

/// `G_Player::Destroy` — 0x432500.
pub fn G_Player_Destroy(g: &mut G, this: u32, a0: i32) {
    let sp = g.m.sp;
    let n = g.m.ri32(this + 0xc2);
    G_EG_DisposePlayersChildren(g, n);
    let def = g.m.r32(this + 0x8a);
    if g.m.r32(def + 0xba) != NONE {
        let id = g.m.r32(def + 0xba);
        request_spawn_at_player(g, this, id);
    }
    if !g.m.rb(this + 0xb8) {
        g.m.w32(this + 0x9e, 0x49a1aa70);
    }
    g.m.w32(this + 0x1fd, 0);
    g.m.w32(this + 0x201, 0);
    g.m.w8(this + 0xcd, 0);
    let zero = f32::from_bits(g.m.r32(SHIELD_DEFAULT));
    let n = g.m.ri32(this + 0xc2);
    G_ScoreBar_SetShieldBarPercentage(g, n, zero);
    let zero = f32::from_bits(g.m.r32(SHIELD_DEFAULT));
    let n = g.m.ri32(this + 0xc2);
    G_ScoreBar_SetPowerBarPercentage(g, n, zero);
    let id1 = G_Res_GetPermObjectID(g, 5) as u32;
    let id5 = G_Res_GetPermObjectID(g, 4) as u32;
    let id10 = G_Res_GetPermObjectID(g, 3) as u32;
    let id50 = G_Res_GetPermObjectID(g, 2) as u32;
    let mut money = g.m.ri32(this + 0xa2).wrapping_add(0xfff4d332u32 as i32);
    let req = new_spawn_request(g);
    let x = g.m.r32(this);
    g.m.w32(req + 4, x);
    let y = g.m.r32(this + 4);
    g.m.w32(req + 8, y);
    while money >= 0x32 {
        money = money.wrapping_sub(0x32);
        if id50 != NONE {
            g.m.w32(req, id50);
            G_EG_RequestSpawn(g, req, 0, 0);
        }
    }
    while money >= 10 {
        money = money.wrapping_sub(10);
        if id10 != NONE {
            g.m.w32(req, id10);
            G_EG_RequestSpawn(g, req, 0, 0);
        }
    }
    while money >= 5 {
        money = money.wrapping_sub(5);
        if id5 != NONE {
            g.m.w32(req, id5);
            G_EG_RequestSpawn(g, req, 0, 0);
        }
    }
    while money >= 1 {
        money = money.wrapping_sub(1);
        if id1 != NONE {
            g.m.w32(req, id1);
            G_EG_RequestSpawn(g, req, 0, 0);
        }
    }
    g.m.w32(this + 0xa2, 0xb2cce);
    g.m.w32(this + 0xba, 3);
    g.m.wi32(this + 0xbe, a0);
    if g.m.rb(this + 0xb8) {
        g.m.w8(this + 0xca, 1);
    }
    if G_Player_Multiplier_Get(g, this) != 1 {
        let e = g.m.ri32(this + 0xae);
        G_EG_DisposeByUniqueEntityNum(g, e);
        G_Player_Priv_Multiplier_Reset(g, this);
    }
    g.m.pop_to(sp);
}

/// `G_Player::Score_Get` — 0x434f10.
pub fn G_Player_Score_Get(g: &mut G, this: u32) -> i32 {
    g.m.ri32(this + 0xa6).wrapping_add(0xfaacd5c2u32 as i32)
}

/// `G_Player::Score_Adjust` — 0x434f40.
pub fn G_Player_Score_Adjust(g: &mut G, this: u32, a0: i32, a1: bool) {
    if !g.m.rb(this + 0xb8) {
        return;
    }
    let mut s = g.m.ri32(this + 0xa6).wrapping_add(0xfaacd5c2u32 as i32);
    if !a1 {
        s = s.wrapping_add(a0.wrapping_mul(g.m.ri32(this + 0xaa)));
    } else {
        s = s.wrapping_add(a0);
    }
    if a0 > 0 {
        if !a1 {
            if s > g.m.ri32(this + 0x92) {
                G_Player_Lives_Add(g, this, true);
                let def = g.m.r32(this + 0x8a);
                let v = g.m.ri32(this + 0x92).wrapping_add(g.m.ri32(def + 0x6a));
                g.m.wi32(this + 0x92, v);
                let v = g.m.ri32(this + 0x92).wrapping_add(g.m.ri32(this + 0x96));
                g.m.wi32(this + 0x92, v);
                let add = pf_int(g, 0xb6);
                let v = g.m.ri32(this + 0x96).wrapping_add(add);
                g.m.wi32(this + 0x96, v);
            }
        } else {
            let t = pf_int(g, 0xb6).wrapping_add(s);
            g.m.wi32(this + 0x96, t);
        }
    }
    g.m.wi32(this + 0xa6, s.wrapping_add(0x5532a3e));
}

/// `G_Player::Multiplier_Advance` — 0x435090.
pub fn G_Player_Multiplier_Advance(g: &mut G, this: u32) {
    if g.m.ri32(this + 0xba) != 4 {
        return;
    }
    // jump table 0x4c0828: multipliers 1..5 advance, the rest do nothing
    let next = match g.m.ri32(this + 0xaa) {
        1 => 2,
        2 => 3,
        3 => 4,
        4 => 5,
        5 => 10,
        _ => return,
    };
    g.m.wi32(this + 0xaa, next);
    G_Player_Priv_Multiplier_SpawnForCurrentMultiplier(g, this);
}

/// `G_Player::Multiplier_Get` — 0x435100.
pub fn G_Player_Multiplier_Get(g: &mut G, this: u32) -> i32 {
    g.m.ri32(this + 0xaa)
}

/// `G_Player::Priv_Appear` — 0x4351d0.
pub fn G_Player_Priv_Appear(g: &mut G, this: u32, a0: i32) {
    if !g.m.rb(this + 0xb8) {
        return;
    }
    G_Player_Priv_ResetSpriteInfo(g, this);
    G_GameObject_CalculateDimensions(g, this);
    let def = g.m.r32(this + 0x8a);
    if g.m.ri32(this + 0xc6) == 1 {
        let y = g.m.ri32(def + 0x92) as f32;
        let x = g.m.ri32(def + 0x8e) as f32;
        G_GameObject_SetLoc_424140(g, this, x, y);
    } else {
        let y = g.m.ri32(def + 0x9a) as f32;
        let x = g.m.ri32(def + 0x96) as f32;
        G_GameObject_SetLoc_424140(g, this, x, y);
    }
    let v = g.m.r32(RESET_POS_X);
    g.m.w32(this + 0x10, v);
    let v = g.m.r32(RESET_POS_Y);
    g.m.w32(this + 0x14, v);
    g.m.w32(this + 0x205, 0);
    overload_clear(g, this);
    g.m.w8(this + 0xb9, 1);
    g.m.w32(this + 0xba, 4);
    g.m.wi32(this + 0xbe, a0);
    let v = G_Res_GetPermFloat(g, 0xa3);
    g.m.wf32(this + 0x62, v);
    let v = G_Res_GetPermFloat(g, 0xa4);
    g.m.wf32(this + 0x66, v);
    let v = G_Res_GetPermFloat(g, 0xa5);
    g.m.wf32(this + 0x6a, v);
    let def = g.m.r32(this + 0x8a);
    if g.m.r32(def + 0x9e) != NONE {
        let id = g.m.r32(def + 0x9e);
        request_spawn_at_player(g, this, id);
    }
    G_Player_Priv_Multiplier_SpawnForCurrentMultiplier(g, this);
    G_WeaponHandler_SetUpForAppearance(g, this + 0x235, false);
}

/// `G_Player::Priv_ResetSpriteInfo` — 0x4353d0.
pub fn G_Player_Priv_ResetSpriteInfo(g: &mut G, this: u32) {
    G_Player_Priv_SetSpriteFaceAccordingToAirWeapon(g, this);
    g.m.w32(this + 0x20, 0);
    g.m.w32(this + 0xce, 0);
    g.m.w32(this + 0x4a, 0x79616c70);
}

/// `G_Player::Priv_SetSpriteFaceAccordingToAirWeapon` — 0x435410.
pub fn G_Player_Priv_SetSpriteFaceAccordingToAirWeapon(g: &mut G, this: u32) {
    let wep = G_WeaponHandler_AirWeapon_GetCurrentOrQueuedWeaponRefPtr(g, this + 0x235);
    match g.m.r32(this + 0xc2) {
        0 => {
            let v = g.m.r32(wep + 0x144);
            g.m.w32(this + 0x1c, v);
        }
        1 => {
            let v = g.m.r32(wep + 0x148);
            g.m.w32(this + 0x1c, v);
        }
        _ => {}
    }
    g.m.w8(this + 0x34, 1);
}

/// `G_Player::Priv_Multiplier_Reset` — 0x435460.
pub fn G_Player_Priv_Multiplier_Reset(g: &mut G, this: u32) {
    g.m.w32(this + 0xaa, 1);
}

/// `G_Player::Priv_Multiplier_SpawnForCurrentMultiplier` — 0x435470.
pub fn G_Player_Priv_Multiplier_SpawnForCurrentMultiplier(g: &mut G, this: u32) {
    if g.m.ri32(this + 0xba) != 4 {
        return;
    }
    // jump table 0x4c0862, by multiplier - 1
    let perm = match g.m.ri32(this + 0xaa).wrapping_sub(1) as u32 {
        1 => Some(0x23),
        2 => Some(0x24),
        3 => Some(0x25),
        4 => Some(0x26),
        9 => Some(0x27),
        _ => None,
    };
    let id = match perm {
        Some(p) => G_Res_GetPermObjectID(g, p) as u32,
        None => NONE,
    };
    if id == NONE {
        return;
    }
    let e = g.m.ri32(this + 0xae);
    G_EG_DisposeByUniqueEntityNum(g, e);
    let sp = g.m.sp;
    let req = new_spawn_request(g);
    g.m.w32(req, id);
    G_GameObject_GetLoc_4240e0(g, this, req + 4);
    let n = g.m.r32(this + 0xc2);
    g.m.w32(req + 0x12, n);
    // G_Entity_Ref out parameter: {link, uniqueNum}
    let rf = g.m.alloca(8);
    G_EG_RequestSpawn(g, req, rf, 0);
    let u = g.m.r32(rf + 4);
    g.m.w32(this + 0xae, u);
    g.m.pop_to(sp);
}

/// `G_Player::Priv_ProcessState` — 0x4355a0.
pub fn G_Player_Priv_ProcessState(g: &mut G, this: u32, a0: i32, a1: bool, a2: bool) {
    let in_game = g.m.rb(this + 0xb8);
    if !in_game {
        return;
    }
    let state = g.m.ri32(this + 0xba);
    let def = g.m.r32(this + 0x8a);
    match state {
        1 => {
            // leaving
            let limit = g.m.ri32(def + 0x7e).wrapping_add(g.m.ri32(this + 0xbe));
            if a0 > limit {
                g.m.w8(this + 0xb8, 0);
            }
        }
        2 => {
            // entering
            let limit = g.m.ri32(def + 0xb6).wrapping_add(g.m.ri32(this + 0xbe));
            if a0 > limit {
                G_Player_Priv_Appear(g, this, a0);
            } else {
                let zero = f32::from_bits(g.m.r32(SHIELD_DEFAULT));
                let n = g.m.ri32(this + 0xc2);
                G_ScoreBar_SetShieldBarPercentage(g, n, zero);
            }
        }
        3 => {
            // dying
            let zero = f32::from_bits(g.m.r32(SHIELD_DEFAULT));
            let n = g.m.ri32(this + 0xc2);
            G_ScoreBar_SetShieldBarPercentage(g, n, zero);
            let zero = f32::from_bits(g.m.r32(SHIELD_DEFAULT));
            let n = g.m.ri32(this + 0xc2);
            G_ScoreBar_SetPowerBarPercentage(g, n, zero);
            let mut lives = g.m.ri32(this + 0x8e).wrapping_add(0xeadb2311u32 as i32);
            let delay = if lives == 1 { g.m.ri32(def + 0x86) } else { g.m.ri32(def + 0x82) };
            if a0 > delay.wrapping_add(g.m.ri32(this + 0xbe)) {
                if a1 && g.m.rb(this + 0xb8) {
                    lives = lives.wrapping_sub(1);
                    if lives < 0 {
                        lives = 0;
                    }
                    g.m.wi32(this + 0x8e, lives.wrapping_add(0x1524dcef));
                }
                let now = g.m.ri32(this + 0x8e).wrapping_add(0xeadb2311u32 as i32);
                if now > 0 {
                    G_Player_Priv_Appear(g, this, a0);
                    if !g.m.rb(this + 0xb8) {
                        g.m.w32(this + 0x9e, 0x49a1aa70);
                    } else {
                        let def = g.m.r32(this + 0x8a);
                        let v = (g.m.ri32(def + 0x48) as f64 + g.m.rf32(SHIELD_OFFSET) as f64) as f32;
                        g.m.wf32(this + 0x9e, v);
                    }
                    g.m.w32(this + 0x1fd, 0);
                    g.m.w32(this + 0x201, 0);
                    g.m.w8(this + 0xcd, 0);
                } else {
                    g.m.w32(this + 0xba, 1);
                    g.m.wi32(this + 0xbe, a0);
                }
            }
        }
        4 => {
            // active
            if g.m.rb(this + 0xca) {
                if !G_Game_IsWaitingForLevelToEnd(g) {
                    let limit = g.m.ri32(def + 0x8a).wrapping_add(g.m.ri32(this + 0xbe));
                    if a0 > limit && g.m.rb(this + 0xb8) && !g.m.rb(this + 0xcb) {
                        g.m.w8(this + 0xca, 0);
                        g.m.w8(this + 0xcb, 0);
                    }
                }
            }
        }
        _ => {}
    }
}

/// `G_Player::Priv_GetInputs` — 0x4357d0.
pub fn G_Player_Priv_GetInputs(g: &mut G, this: u32, a0: u32, a1: i32) {
    if g.m.ri32(this + 0xba) != 4 {
        return;
    }
    g.m.clear(this + 0x1f6, 7);
    let n = g.m.ri32(this + 0xc2);
    if a1 == 1 {
        G_Film_GetInputs(g, a0, n, this + 0x1f6);
    } else {
        G_Input_GetCachedPlayerInputs(g, n, this + 0x1f6);
        let score = g.m.ri32(this + 0xa6).wrapping_add(0xfaacd5c2u32 as i32);
        G_Film_SetInputs(g, a0, n, this + 0x1f6, score);
    }
}

/// `G_Player::Process` — 0x432820. The registration check at the top
/// (0x432839..0x433f89) is not ported: it only runs for the registered
/// build (`+0x233` set from `RT3_IsRegistered`) on the tick that equals the
/// random seed chosen at level start.
pub fn G_Player_Process(g: &mut G, this: u32, a0: bool, a1: bool, a2: i32, a3: u32, a4: i32, a5: bool) {
    // Licence check block.
    if g.m.rb(this + 0x233) && a2 == g.m.ri32(this + 0x22b) {
        // The block (0x43285b..0x433f89) checks the licence code against the
        // name and the copies and a table of 20 blacklisted codes; a failure
        // calls G_Level_HackTest_Fux0r (0x433f5c). Only the valid outcome is
        // ported (the check itself needs the RT3 library): the check counter
        // +0x22f goes up and the first time the next check is scheduled at
        // twice this tick (0x433f63..0x433f83).
        let _ = G_Level_HackTest_Fux0r;
        let n = g.m.ri32(this + 0x22f).wrapping_add(1);
        g.m.wi32(this + 0x22f, n);
        if n == 1 {
            g.m.wi32(this + 0x22b, a2.wrapping_add(a2));
        }
    }

    if !g.m.rb(this + 0xb8) {
        return;
    }
    // Defence bonus: once, at the first tick with the flag.
    if a5 && !g.m.rb(this + 0xcc) {
        g.m.w8(this + 0xcc, 1);
        let def = g.m.r32(this + 0x8a);
        if g.m.r32(def + 0xce) != NONE {
            let id = g.m.r32(def + 0xce);
            request_spawn_at_player(g, this, id);
        }
        let level = G_Game_GetCurrentLevelNumber(g);
        let mult = pf_int(g, 0xb8);
        G_Player_Score_Adjust(g, this, level.wrapping_mul(mult), false);
    }
    G_Player_Priv_ProcessState(g, this, a2, a0, a1);
    G_Player_Priv_GetInputs(g, this, a3, a4);
    G_GameObject_DoScaling(g, this);
    G_Player_PowerupOverload_Process(g, this, a2);
    G_GameObject_AdjustVisibilityAndTinting(g, this);
    if g.m.rb(this + 0xb9) {
        // fcomp: equal and ordered
        if g.m.rf32(this + 0x62) == g.m.rf32(this + 0x66) {
            g.m.w8(this + 0xb9, 0);
        }
    }
    G_GameObject_CalculateDimensions(g, this);
    G_GameObject_Glow_Process(g, this);
    if g.m.ri32(this + 0xba) != 4 {
        return;
    }
    let sp = g.m.sp;
    // Weapons, unless the sprite is being scaled.
    if 1.0f64 == g.m.rf32(this + 0x7a) as f64 {
        let out = g.m.alloca(4);
        let ground = g.m.rb(this + 0x1fa);
        let air = g.m.rb(this + 0x1fb);
        let aux = g.m.rb(this + 0x1fc);
        let res = G_WeaponHandler_Process(g, this + 0x235, this, ground, air, aux, a2, out);
        if g.m.rb(out) {
            G_Player_Priv_SetSpriteFaceAccordingToAirWeapon(g, this);
        }
        match res {
            1 => {
                if g.m.ri32(this + 0xba) == 4
                    && !G_Game_IsWaitingForLevelToEnd(g)
                    && !g.m.rb(this + 0x209)
                {
                    g.m.w8(this + 0x209, 1);
                    g.m.w8(this + 0x20a, 1);
                    g.m.w32(this + 0x20b, 0);
                    g.m.wi32(this + 0x20f, a2);
                    let def = g.m.r32(this + 0x8a);
                    let v = g.m.r32(def + 0xde);
                    g.m.w32(this + 0x213, v);
                    g.m.w32(this + 0x217, 0);
                    let v = g.m.r16(def + 0xea);
                    g.m.w16(this + 0x60, v);
                }
            }
            2 => {
                g.m.w8(this + 0x209, 0);
                g.m.w8(this + 0x20a, 0);
                g.m.w32(this + 0x20b, 0);
                g.m.w32(this + 0x20f, 0);
                g.m.w32(this + 0x213, 0);
                g.m.w32(this + 0x217, 0);
                g.m.w8(this + 0x52, 0);
                g.m.w32(this + 0x54, 0);
                g.m.w32(this + 0x58, 0);
                g.m.w32(this + 0x5c, 0);
                g.m.w16(this + 0x60, 0x7fff);
            }
            _ => {}
        }
    }
    // weapon_done: movement from the inputs.
    let up = g.m.rb(this + 0x1f6);
    let down = g.m.rb(this + 0x1f8);
    let left = g.m.rb(this + 0x1f9);
    let right = g.m.rb(this + 0x1f7);
    let limit = g.m.rf32(this + 0x9a);
    let def = g.m.r32(this + 0x8a);
    let step = g.m.rf32(def + 0xd6);
    let (vx, vy) = (this + 0x10, this + 0x14);
    if up {
        let v = (g.m.rf32(vy) as f64 - step as f64) as f32;
        g.m.wf32(vy, v);
        let lim = -limit;
        if g.m.rf32(vy) < lim {
            g.m.wf32(vy, lim);
        }
    }
    if down {
        let v = (g.m.rf32(vy) as f64 + step as f64) as f32;
        g.m.wf32(vy, v);
        if g.m.rf32(vy) > limit {
            g.m.wf32(vy, limit);
        }
    }
    if left {
        let v = (g.m.rf32(vx) as f64 - step as f64) as f32;
        g.m.wf32(vx, v);
        let lim = -limit;
        if g.m.rf32(vx) < lim {
            g.m.wf32(vx, lim);
        }
    }
    if right {
        let v = (g.m.rf32(vx) as f64 + step as f64) as f32;
        g.m.wf32(vx, v);
        if g.m.rf32(vx) > limit {
            g.m.wf32(vx, limit);
        }
    }
    // Friction when no key is held on the axis.
    if !left && !right {
        if g.m.rf32(vx) > 0.0 {
            let v = (g.m.rf32(vx) as f64 - step as f64) as f32;
            g.m.wf32(vx, v);
            if g.m.rf32(vx) < 0.0 {
                g.m.w32(vx, 0);
            }
        }
        if g.m.rf32(vx) < 0.0 {
            let v = (g.m.rf32(vx) as f64 + step as f64) as f32;
            g.m.wf32(vx, v);
            if g.m.rf32(vx) > 0.0 {
                g.m.w32(vx, 0);
            }
        }
    }
    if !up && !down {
        if g.m.rf32(vy) > 0.0 {
            let v = (g.m.rf32(vy) as f64 - step as f64) as f32;
            g.m.wf32(vy, v);
            if g.m.rf32(vy) < 0.0 {
                g.m.w32(vy, 0);
            }
        }
        if g.m.rf32(vy) < 0.0 {
            let v = (g.m.rf32(vy) as f64 + step as f64) as f32;
            g.m.wf32(vy, v);
            if g.m.rf32(vy) > 0.0 {
                g.m.w32(vy, 0);
            }
        }
    }
    // Sprite frame (banking animation).
    let delay = pf_int(g, 0xa6).wrapping_add(g.m.ri32(this + 0xce));
    if a2 > delay {
        g.m.wi32(this + 0xce, a2);
        let f = g.m.r32(this + 0x20);
        let new: Option<u32> = if !left && !right {
            match f {
                1 => Some(0),
                2 => Some(1),
                3 => Some(2),
                4 => Some(0),
                5 => Some(4),
                6 => Some(5),
                _ => None,
            }
        } else if left {
            match f {
                0 => Some(1),
                1 => Some(2),
                2 => Some(3),
                4 => Some(3),
                5 => Some(4),
                6 => Some(5),
                _ => None,
            }
        } else {
            match f {
                0 => Some(4),
                1 => Some(0),
                2 => Some(1),
                3 => Some(2),
                4 => Some(5),
                5 => Some(6),
                _ => None,
            }
        };
        if let Some(n) = new {
            g.m.w32(this + 0x20, n);
        }
    }
    // Position.
    let v = (g.m.rf32(this) as f64 + g.m.rf32(this + 0x10) as f64) as f32;
    g.m.wf32(this, v);
    let v = (g.m.rf32(this + 4) as f64 + g.m.rf32(this + 0x14) as f64) as f32;
    g.m.wf32(this + 4, v);
    if left {
        G_Bgnd_AdjustSideScroll(g, 0);
    } else if right {
        G_Bgnd_AdjustSideScroll(g, 1);
    }
    // Keep inside the game area.
    let mut hit_bottom = false;
    let w = pf_int(g, 0x36);
    let h = pf_int(g, 0x37);
    let hw = g.m.ri32(this + 0x2c);
    let x = g.m.rf32(this);
    let neg32 = g.m.rf32(0x4ecb3c) as f64;
    if (x as f64 - hw as f64) < neg32 {
        let nx = hw.wrapping_sub(0x20);
        g.m.wf32(this, nx as f32);
        g.m.w32(this + 0x10, 0);
    } else if (hw as f64 + x as f64) > (w.wrapping_add(0x20)) as f64 {
        let nx = w.wrapping_sub(hw).wrapping_add(0x20);
        g.m.wf32(this, nx as f32);
        g.m.w32(this + 0x10, 0);
    }
    let top = pf_int(g, 0xb7);
    let hh = g.m.ri32(this + 0x30);
    let y = g.m.rf32(this + 4);
    if (top as f64) > (y as f64 - hh as f64) {
        g.m.wf32(this + 4, top.wrapping_add(hh) as f32);
        g.m.w32(this + 0x14, 0);
    } else if (h as f64) < (hh as f64 + y as f64) {
        g.m.wf32(this + 4, h.wrapping_sub(hh) as f32);
        g.m.w32(this + 0x14, 0);
        hit_bottom = true;
    }
    G_WeaponHandler_UpdateLoc(g, this + 0x235, this);
    // Crosshair follows, lagging when pushing against the bottom.
    if !g.m.rb(this + 0x34c) {
        g.m.pop_to(sp);
        return;
    }
    if down && hit_bottom {
        let add = pf_int(g, 0xb9);
        let v = g.m.ri32(this + 0x205).wrapping_add(add);
        g.m.wi32(this + 0x205, v);
        let cap = pf_int(g, 0xbb);
        if g.m.ri32(this + 0x205) > cap {
            g.m.wi32(this + 0x205, cap);
        }
    } else if g.m.ri32(this + 0x205) > 0 {
        let add = pf_int(g, 0xba);
        let v = g.m.ri32(this + 0x205).wrapping_add(add);
        g.m.wi32(this + 0x205, v);
        if g.m.ri32(this + 0x205) < 0 {
            g.m.w32(this + 0x205, 0);
        }
    }
    let pos = g.m.alloca(8);
    let dim = g.m.alloca(8);
    // [this+0x2ac] is the weapon handler's definition pointer.
    let wd = g.m.r32(this + 0x2ac);
    let px = (g.m.ri32(wd + 0x176) as f64 + g.m.rf32(this) as f64) as f32;
    g.m.wf32(pos, px);
    let wd = g.m.r32(this + 0x2ac);
    let mut py = (g.m.ri32(wd + 0x17a) as f64 + g.m.rf32(this + 4) as f64) as f32;
    py = (g.m.ri32(this + 0x205) as f64 + py as f64) as f32;
    g.m.wf32(pos + 4, py);
    G_GameObject_GetDimensions(g, this + 0x2c2, dim, dim + 4);
    let hgt = g.m.ri32(dim + 4);
    let half = hgt / 2;
    g.m.wi32(dim + 4, half);
    let py = g.m.rf32(pos + 4);
    if (py as f64 - half as f64) < g.m.rf32(SHIELD_DEFAULT) as f64 {
        g.m.wf32(pos + 4, half as f32);
    }
    G_GameObject_SetLoc_424120(g, this + 0x2c2, pos);
    g.m.pop_to(sp);
}
