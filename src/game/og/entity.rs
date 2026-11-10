//! Port of the original's entity module (G_Entity, 0x4130e0..0x417020), a
//! G_GameObject (see gameobject.rs) with a unit definition (`+0x8a`), a
//! current state index (`+0x9e`) and per-state spawn info lists
//! (`+0x194 + 4 * state`, 0x16-byte records).
//!
//! Fields as the disassembly uses them (`this`-relative):
//! `+0x8a` unit def, `+0x8e` family, `+0x92` owner entity number, `+0x96`
//! owner (-1: none), `+0x9a` state start time, `+0x9e` state index, `+0xa8`
//! last hit time, `+0xac` rolled on-timer delay, `+0xb0` last frame step
//! time, `+0xb4` animating backwards, `+0xb5` has a target, `+0xb6` animation
//! stopped, `+0xb7` has spawn sets, `+0xb8` spawn rotation pause, `+0xbc`
//! animates, `+0xbf` destroyed, `+0xc0` fleeing, `+0xc1` has shield
//! depletion state, `+0xc2` last collision spawn time, `+0xc6` collision
//! spawn count, `+0xca` player number, `+0xce` destroyer, `+0xd2` destroyed
//! flag, `+0xd8` orbit angle, `+0xe0` sound pass count, `+0xf0` weapon id,
//! `+0xf4` last on-hit change time, `+0x100/0x104` target velocity,
//! `+0x108/0x10c` acceleration, `+0x110` tracked player, `+0x114/0x118`
//! target location, `+0x12c` shields, `+0x130` heading, `+0x134` stationary,
//! `+0x144 + 4 * state` state entry counts.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::bgnd::{G_Bgnd_GetUpdateRectTop, G_Bgnd_MediaMask_GetSurfaceAtLoc};
use super::debris::G_Debris_New;
use super::eg::{
    G_EG_CacheOwnerLocOffsetsAndAngles, G_EG_RequestSpawn,
    G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType,
    G_EG_RuleCondition_IsAnyDestroyableAirEntityActive,
    G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive, G_EG_RuleCondition_IsEntityActive,
    G_EG_RuleCondition_IsEntityTrackingPlayer,
};
use super::g::G;
use super::game::{
    G_Game_GetCurrentLevelNumber, G_Game_GroundAccuracy_AddDestroyed,
    G_Game_GroundAccuracy_CheckForRewardThisLevel, G_Game_GroundAccuracy_SetRewardDone,
    G_Game_Player_AdjustScore, G_Game_Player_GetClosestActive_41f550, G_Game_Player_IsAnyActive,
};
use super::gameobject::*;
use super::mem::list;
use super::notice::G_Notice_Request;
use super::particle::G_Particle_NewGroup;
use super::resource::{G_Res_GetPermFloat, G_Res_GetPermObjectID};
use super::umath::{self, ftol};
use super::unitdefs::{
    G_UnitDef_MasterList_GetFamilyPtrForUnitDef, G_UnitDef_MasterList_GetRefPtrForID_ByFamily,
};
use super::wepdefs::G_WepDef_MasterList_GetNextRefPtrByType;

const NONE: u32 = 0x656e6f6e;
const GRND: u32 = 0x646e7267;
/// The default spawn request (40 bytes) at 0x4e02ab: id 'none', player -1
/// at +0x12, scale 1.0 at +0x24.
const SPAWN_DEFAULT: u32 = 0x4e02ab;

/// Signed 32-bit division as `cdq; idiv`. The original traps on a zero
/// divisor; here it yields 0.
fn idiv(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}

/// `xor dword [a], 0x80000000`.
fn flip(g: &mut G, a: u32) {
    let v = g.m.r32(a) ^ 0x8000_0000;
    g.m.w32(a, v);
}

/// `G_Res_GetPermFloat` as a double.
fn pf(g: &mut G, i: i32) -> f32 {
    G_Res_GetPermFloat(g, i)
}

/// `U_Sprite_ConvertScalePercentage` (0x40af60): percent / [0x4dce98].
fn convert_scale_percentage(g: &mut G, pct: i32) -> f32 {
    (pct as f64 / g.m.rf32(0x4dce98) as f64) as f32
}

/// `U_Sound_Play` (0x44f5f0): rolls the pitch, then the (never drawing)
/// volume, and starts the sound through the host.
fn u_sound_play(g: &mut G, settings: u32, flag: bool) {
    let id = g.m.r32(settings);
    if id == NONE {
        return;
    }
    let vol = g.m.ri32(settings + 4);
    let lo = g.m.rf32(settings + 0x10);
    let hi = g.m.rf32(settings + 0x14);
    let pitch = g.random_float(lo, hi) as f32;
    let v = g.random_int(vol, vol);
    g.host.sound_play(id, v, pitch);
}

/// A spawn request on the scratch stack, initialised from the default.
fn new_spawn_request(g: &mut G) -> u32 {
    let r = g.m.alloca(0x28);
    g.m.copy(r, SPAWN_DEFAULT, 0x28);
    r
}

/// A particle group request (0x14 bytes) for `G_Particle_NewGroup`.
fn new_particle(g: &mut G, this: u32, color: u16, id: u32) -> u32 {
    let p = g.m.alloca(0x14);
    let x = g.m.r32(this);
    g.m.w32(p, x);
    let y = g.m.r32(this + 4);
    g.m.w32(p + 4, y);
    g.m.w16(p + 8, color);
    g.m.w32(p + 0xa, 0);
    let def = g.m.r32(this + 0x8a);
    let ground = g.m.r32(def + 8) == GRND;
    g.m.w8(p + 0xe, ground as u8);
    g.m.w32(p + 0x10, id);
    p
}

/// `public: __thiscall G_Entity::~G_Entity(void)` — 0x4130e0.
pub fn G_Entity_dtor_G_Entity(g: &mut G, this: u32) {
    G_GameObject_dtor_G_GameObject(g, this);
}

/// `public: void __thiscall G_Entity::Reset(void)` — 0x413100.
pub fn G_Entity_Reset(g: &mut G, this: u32) {
    for i in 0..0x14u32 {
        g.m.w32(this + 0x144 + i * 4, 0);
    }
    g.m.w32(this + 0x8a, 0);
    g.m.w32(this + 0x8e, 0);
    for i in 0..0x14u32 {
        let l = g.m.r32(this + 0x194 + i * 4);
        G_Entity_Priv_DeleteSpawnInfoList(g, this, l);
        g.m.w32(this + 0x194 + i * 4, 0);
    }
    let v = g.m.r32(0x4e02a3);
    g.m.w32(this + 0x138, v);
    let v = g.m.r32(0x4e02a7);
    g.m.w32(this + 0x13c, v);
    g.m.wi32(this + 0x140, -1);
    g.m.wi32(this + 0x92, -1);
    g.m.wi32(this + 0x96, -1);
    g.m.w32(this + 0x9a, 0);
    g.m.wi32(this + 0x9e, -1);
    g.m.w8(this + 0xa2, 0);
    g.m.w32(this + 0xa4, 0);
    g.m.w32(this + 0xa8, 0);
    g.m.w32(this + 0xb0, 0);
    g.m.w8(this + 0xb4, 0);
    g.m.w8(this + 0xb6, 0);
    g.m.w8(this + 0xb5, 0);
    g.m.w32(this + 0xb8, 0);
    g.m.w8(this + 0xb7, 0);
    g.m.w8(this + 0xbc, 0);
    g.m.w32(this + 0xf4, 0);
    g.m.w8(this + 0xbe, 0);
    g.m.w8(this + 0xbf, 0);
    g.m.w8(this + 0xc0, 0);
    g.m.wi32(this + 0xca, -1);
    g.m.wi32(this + 0xce, -1);
    g.m.w8(this + 0xd2, 0);
    g.m.w32(this + 0xd4, 0);
    g.m.w32(this + 0xd8, 0);
    g.m.w32(this + 0xc2, 0);
    g.m.w32(this + 0xc6, 0);
    g.m.w32(this + 0xdc, 0);
    g.m.w32(this + 0xe0, 0);
    g.m.w32(this + 0xe4, 0);
    g.m.w32(this + 0xe8, 0);
    g.m.w32(this + 0xec, 0);
    g.m.w32(this + 0xf0, NONE);
    g.m.w8(this + 0xc1, 0);
    let v = g.m.r32(0x4dfda8);
    g.m.w32(this + 0x114, v);
    let v = g.m.r32(0x4dfdac);
    g.m.w32(this + 0x118, v);
    g.m.wi32(this + 0x110, -1);
    let v = g.m.r32(0x4dfda8);
    g.m.w32(this + 0x11c, v);
    let v = g.m.r32(0x4dfdac);
    g.m.w32(this + 0x120, v);
    let v = g.m.r32(0x4dfda8);
    g.m.w32(this + 0x124, v);
    let v = g.m.r32(0x4dfdac);
    g.m.w32(this + 0x128, v);
    g.m.w8(this + 0x134, 0);
    g.m.w8(this + 0x135, 0);
    g.m.w8(this + 0x136, 0);
    G_GameObject_SetDefaults(g, this);
}

/// `public: void __thiscall G_Entity::SetUnitRef(struct G_UnitDef *)` — 0x413330.
/// One 0x16-byte (zeroed) run-time record per spawn set of every state.
pub fn G_Entity_SetUnitRef(g: &mut G, this: u32, a0: u32) {
    g.m.w32(this + 0x8a, a0);
    let def = g.m.r32(this + 0x8a);
    let fam = G_UnitDef_MasterList_GetFamilyPtrForUnitDef(g, def);
    g.m.w32(this + 0x8e, fam);
    let def = g.m.r32(this + 0x8a);
    if g.m.r32(def + 0x2dc) == 0x20647568 {
        g.m.w8(this + 0x18, 0);
    }
    for i in 0..0x14u32 {
        let l = g.m.r32(this + 0x194 + i * 4);
        G_Entity_Priv_DeleteSpawnInfoList(g, this, l);
        g.m.w32(this + 0x194 + i * 4, 0);
    }
    if g.m.r32(this + 0x8a) == 0 {
        return;
    }
    for sc in 0..0x14u32 {
        let def = g.m.r32(this + 0x8a);
        let st = sc.wrapping_mul(0x5da).wrapping_add(def).wrapping_add(0x4d8);
        let mut count = 0;
        let sets = g.m.r32(st + 0x5d6);
        if sets != 0 {
            count = list::num(&g.m, sets);
            let l = list::new(&mut g.m);
            g.m.w32(this + 0x194 + sc * 4, l);
        }
        for i in 0..count {
            list::nth(&g.m, g.m.r32(st + 0x5d6), i);
            let info = g.m.alloc(0x16);
            let l = g.m.r32(this + 0x194 + sc * 4);
            list::add(&mut g.m, l, info);
            g.m.clear(info, 0x16);
            g.m.w8(this + 0x136, 1);
        }
    }
}

/// `public: struct G_UnitDef_FileData_State * __thiscall G_Entity::GetCurrentStatePtr(void)` — 0x413500.
pub fn G_Entity_GetCurrentStatePtr(g: &mut G, this: u32) -> u32 {
    g.m.r32(this + 0x9e)
        .wrapping_mul(0x5da)
        .wrapping_add(g.m.r32(this + 0x8a))
        .wrapping_add(0x4d8)
}

/// Shared loop of `ChangeToWeaponPowerupReleaseState` / `Priv_ChangeStateOnShieldDepletion`:
/// the first state whose flag byte (`+0x829` / `+0x82a` of the def) is set.
fn change_to_flagged_state(g: &mut G, this: u32, flag_off: u32, time: i32) {
    let sp = g.m.sp;
    let flags = g.m.alloca(2);
    let mut i = 0i32;
    loop {
        let def = g.m.r32(this + 0x8a);
        if i >= g.m.ri32(def + 0x12) {
            break;
        }
        let o = (i as u32).wrapping_mul(0x5da);
        if g.m.r8(o.wrapping_add(def).wrapping_add(flag_off)) != 0 {
            G_Entity_ChangeState(g, this, false, o.wrapping_add(def).wrapping_add(0x96e), time, flags + 1, flags);
            break;
        }
        i += 1;
    }
    g.m.pop_to(sp);
}

/// `public: void __thiscall G_Entity::ChangeToWeaponPowerupReleaseState(int)` — 0x413520.
pub fn G_Entity_ChangeToWeaponPowerupReleaseState(g: &mut G, this: u32, a0: i32) {
    change_to_flagged_state(g, this, 0x829, a0);
}

/// `public: void __thiscall G_Entity::ChangeState(bool, char const *, int, bool *, bool *)` — 0x4135a0.
/// `a0`: the entity is being set up (initial state: scale, visibility and
/// tint are set rather than moved to), `a1` the state name, `a2` the game
/// time, `a3`/`a4` receive "delete" and "destroy" requests.
/// The frame data (`+0x4e`) is a token, see `sprite_get_ptr_to_sprite_frame`.
pub fn G_Entity_ChangeState(g: &mut G, this: u32, a0: bool, a1: u32, a2: i32, a3: u32, a4: u32) {
    let sp = g.m.sp;
    change_state_impl(g, this, a0, a1, a2, a3, a4);
    g.m.pop_to(sp);
}

fn change_state_impl(g: &mut G, this: u32, a0: bool, name: u32, a2: i32, a3: u32, a4: u32) {
    g.m.w8(a3, 0);
    g.m.w8(a4, 0);
    if g.m.strcmp(name, b"Delete") == 0 {
        g.m.w8(a3, 1);
        return;
    }
    if g.m.strcmp(name, b"Destroy") == 0 {
        g.m.w8(a4, 1);
        return;
    }
    // The last state of that name wins.
    let mut found: i32 = -1;
    let mut i = 0i32;
    loop {
        let def = g.m.r32(this + 0x8a);
        if i >= g.m.ri32(def + 0x12) {
            break;
        }
        let sn = (i as u32).wrapping_mul(0x5da).wrapping_add(def).wrapping_add(0x96e);
        if g.m.strcmp_mem(sn, name) == 0 {
            found = i;
        }
        i += 1;
    }
    if found == -1 {
        return;
    }
    let old_st = G_Entity_GetCurrentStatePtr(g, this);
    let mut orbit = false;
    let mut orbit_angle = 0i32;
    if g.m.r8(old_st + 0x32c) != 0 {
        orbit = true;
        orbit_angle = g.m.ri32(this + 0xd8);
    }
    let had_flee = g.m.r32(old_st + 0x31c) != NONE;
    let old_sprite = g.m.r32(this + 0x1c);
    let old_frame = g.m.ri32(this + 0x20);

    g.m.wi32(this + 0x9e, found);
    g.m.wi32(this + 0x9a, a2);
    let ent = this + 0x144 + (found as u32).wrapping_mul(4);
    let v = g.m.r32(ent).wrapping_add(1);
    g.m.w32(ent, v);
    g.m.wi32(this + 0xc2, a2);
    g.m.w32(this + 0xc6, 0);
    g.m.w32(this + 0xe0, 0);
    g.m.w32(this + 0xec, 0);
    g.m.w8(this + 0xb6, 0);
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let lo = g.m.ri32(st + 0x3a6);
    let hi = g.m.ri32(st + 0x3aa);
    let r = g.random_int(lo, hi);
    g.m.wi32(this + 0xac, r);

    // Pickups take the sprite of the weapon they stand for.
    let def = g.m.r32(this + 0x8a);
    let class = g.m.r32(def + 0x4cc);
    let mut wt = NONE;
    let mut e = class.wrapping_sub(0x20726961);
    if e == 0 {
        wt = 0x41414550; // PEAA
    } else {
        e = e.wrapping_sub(0x42f30712);
        if e == 0 {
            wt = 0x43455053; // SPEC
        } else {
            e = e.wrapping_sub(0x010901f4);
            if e == 0 {
                wt = 0x47414550; // PEAG
            }
        }
    }
    let mut weapon = false;
    if wt != NONE {
        if g.m.r8(st + 0x353) == 0 {
            let lvl = G_Game_GetCurrentLevelNumber(g);
            let wid = g.m.ri32(this + 0xf0);
            let w = G_WepDef_MasterList_GetNextRefPtrByType(g, wt as i32, wid, lvl);
            if w != 0 {
                let v = g.m.r32(w + 4);
                g.m.w32(this + 0xf0, v);
            }
            let v = g.m.r32(w.wrapping_add(0x130));
            g.m.w32(this + 0x1c, v);
            g.m.w32(this + 0x4e, 0);
        }
        weapon = true;
    }
    if !weapon {
        let sprite = g.m.r32(st + 0x300);
        g.m.w32(this + 0x1c, sprite);
        if a0 || g.m.r32(this + 0x1c) != old_sprite {
            let def = g.m.r32(this + 0x8a);
            let f = if g.m.r8(def + 0x122) == 0 {
                let lo = g.m.ri32(st + 0x308);
                let hi = g.m.ri32(st + 0x30c);
                g.random_int(lo, hi)
            } else {
                let ang = g.m.ri32(this + 0x130);
                G_Entity_GetFrameForAngle(g, this, ang)
            };
            g.m.wi32(this + 0x20, f);
        }
        let id = g.m.r32(this + 0x1c);
        let fr = g.m.ri32(this + 0x20);
        let fd = sprite_get_ptr_to_sprite_frame(g, id, fr);
        g.m.w32(this + 0x4e, fd);
    }
    let v = g.m.r8(st + 0x34f);
    g.m.w8(this + 0x36, v);

    if a0 {
        let def = g.m.r32(this + 0x8a);
        g.m.wf32(this + 0x62, g.m.ri32(def + 0x1b0) as f32);
        g.m.wf32(this + 0x66, g.m.ri32(st + 0x3be) as f32);
        g.m.wf32(this + 0x6a, g.m.ri32(st + 0x3c2) as f32);
        g.m.wf32(this + 0x54, g.m.ri32(st + 0x3c6) as f32);
        g.m.wf32(this + 0x58, g.m.ri32(st + 0x3c6) as f32);
        g.m.wf32(this + 0x5c, g.m.ri32(st + 0x3ca) as f32);
        let mut vis = g.m.ri32(def + 0x1a8);
        let tol = g.m.ri32(def + 0x1ac);
        if tol != 0 {
            // Signed halving truncates toward zero.
            let half = tol / 2;
            let e = g.random_int(half.wrapping_neg(), half);
            vis = vis.wrapping_add(e);
            if vis < 0 {
                vis = 0;
            }
        }
        let s = convert_scale_percentage(g, vis);
        g.m.wf32(this + 0x7a, s);
        let p = g.m.ri32(st + 0x3b6);
        let s = convert_scale_percentage(g, p);
        g.m.wf32(this + 0x7e, s);
        let p = g.m.ri32(st + 0x3ba);
        let s = convert_scale_percentage(g, p);
        g.m.wf32(this + 0x82, s);
    }

    if a0 || g.m.r32(this + 0x1c) != old_sprite || g.m.ri32(this + 0x20) != old_frame {
        G_GameObject_CalculateDimensions(g, this);
    }
    let back = g.m.r8(st + 0x2fc) != 0;
    g.m.w8(this + 0xb4, back as u8);

    // Velocity targets for the new state.
    if g.m.r8(st + 0x32a) != 0 {
        // Locked to the owner: no velocity of its own.
        g.m.w32(this + 0x10, 0);
        g.m.w32(this + 0x14, 0);
        g.m.w32(this + 0x108, 0);
        g.m.w32(this + 0x10c, 0);
        g.m.w32(this + 0x100, 0);
        g.m.w32(this + 0x104, 0);
    } else {
        let vx = g.m.rf32(this + 0x10);
        let vy = g.m.rf32(this + 0x14);
        let speed = umath::speed_from_vector(vx, vy);
        let angle;
        if a0 {
            angle = umath::invert_angle(g.m.ri32(this + 0x130));
        } else if !orbit {
            angle = umath::angle_from_vector(vx, vy);
        } else {
            angle = orbit_angle;
        }
        let maxs = g.m.rf32(st + 0x452);
        let diff: f64 = if speed < maxs {
            maxs as f64 - speed as f64
        } else {
            speed as f64 - maxs as f64
        };
        let delta = g.m.rf32(st + 0x456);
        let step: f64 = if delta as f64 > diff { diff } else { delta as f64 };
        let newspeed: f64 = if maxs > speed {
            speed as f64 + step
        } else {
            speed as f64 - step
        };
        let (nx, ny) = umath::vector_from_angle_and_speed(angle, newspeed as f32);
        g.m.wf32(this + 0x108, (nx as f64 - g.m.rf32(this + 0x10) as f64) as f32);
        g.m.wf32(this + 0x10c, (ny as f64 - g.m.rf32(this + 0x14) as f64) as f32);
        let (tx, ty) = umath::vector_from_angle_and_speed(angle, maxs);
        g.m.wf32(this + 0x100, tx);
        g.m.wf32(this + 0x104, ty);
        if !a0 && g.m.ri32(this + 0x96) != -1 {
            G_EG_CacheOwnerLocOffsetsAndAngles(g, this);
        }
    }

    let flee = g.m.r32(st + 0x31c);
    if flee != NONE {
        G_Entity_Priv_Flee(g, this, flee as i32);
    } else if g.m.r8(this + 0xc0) != 0 && had_flee {
        g.m.w8(this + 0xc0, 0);
    }
    G_Entity_Priv_CheckSpawningAbilityAtStateChange(g, this, a2);

    // The state's counter: after that many entries, change again.
    let counter = g.m.ri32(st + 0x3ae);
    let idx = g.m.r32(this + 0x9e);
    if counter > 0 && g.m.ri32(this + 0x144 + idx.wrapping_mul(4)) == counter {
        let n = st + 0x516;
        if g.m.strcmp(n, b"Delete") == 0 {
            g.m.w8(a3, 1);
            return;
        }
        if g.m.strcmp(n, b"Destroy") == 0 {
            g.m.w8(a4, 1);
            return;
        }
        if g.m.r8(n) != 0 && g.m.strcmp(n, b"none") != 0 {
            let idx = g.m.r32(this + 0x9e);
            g.m.w32(this + 0x144 + idx.wrapping_mul(4), 0);
            G_Entity_ChangeState(g, this, false, n, a2, a3, a4);
        }
    }

    let animates = g.m.r32(this + 0x1c) != NONE && g.m.ri32(st + 0x318) > 0;
    g.m.w8(this + 0xbc, animates as u8);
    // (The original asserts here that +0x304 and +0x310 are positive.)
}

/// `public: float __thiscall G_Entity::Hit(float, enum G_Game_PlayerNum, int)` — 0x413d90.
/// `a0` damage, `a1` player, `a2` game time; returns the shields it took.
pub fn G_Entity_Hit(g: &mut G, this: u32, a0: f32, a1: i32, a2: i32) -> f32 {
    let sp = g.m.sp;
    let r = hit_impl(g, this, a0, a1, a2);
    g.m.pop_to(sp);
    r
}

fn hit_impl(g: &mut G, this: u32, dmg: f32, player: i32, time: i32) -> f32 {
    let mut result: f32 = 0.0;
    if g.m.r8(this + 0xbf) != 0 {
        return result;
    }
    let n = ftol(pf(g, 0xa7) as f64).wrapping_add(g.m.ri32(this + 0xa8));
    if !(time > n) {
        return result;
    }
    g.m.wi32(this + 0xa8, time);
    let s = g.m.rf32(this + 0x12c);
    let spent = s < 0.0 || s == 0.0;
    let nv = s as f64 - dmg as f64;
    g.m.wf32(this + 0x12c, nv as f32);
    if nv < 0.0 {
        g.m.w32(this + 0x12c, 0);
    }
    result = (s as f64 - g.m.rf32(this + 0x12c) as f64) as f32;
    let def = g.m.r32(this + 0x8a);
    let fd = def + 0x16;
    if spent {
        return result;
    }
    let cs = G_Entity_GetCurrentStatePtr(g, this);
    if g.m.r8(cs + 0x344) != 0 {
        g.m.wf32(this + 0x12c, s);
    }
    let delay = g.m.ri32(cs + 0x3b2);
    if delay != 0 {
        let nm = cs + 0x596;
        if g.m.r8(nm) != 0 {
            let lim = g.m.ri32(this + 0xf4).wrapping_add(delay);
            if time > lim {
                g.m.wi32(this + 0xf4, time);
                let flags = g.m.alloca(2);
                G_Entity_ChangeState(g, this, false, nm, time, flags + 1, flags);
                if g.m.r8(this + 0xbf) != 0 {
                    return result;
                }
            }
        }
    }
    let shields = g.m.rf32(this + 0x12c);
    if shields <= 0.0 {
        let score = g.m.ri32(fd + 0x49a);
        G_Game_Player_AdjustScore(g, player, score, false);
        if g.m.r8(this + 0xc1) == 0 {
            G_Entity_Destroy(g, this, player, time);
        } else {
            G_Entity_Priv_ChangeStateOnShieldDepletion(g, this, time);
        }
        return result;
    }
    if g.m.r8(cs + 0x350) == 0 {
        G_GameObject_Glow_Start(g, this, 0x7fff, 6, false);
    }
    if g.m.r32(fd + 0x2be) != NONE {
        let color = g.m.r16(fd + 0x166);
        let id = g.m.r32(fd + 0x2be);
        let p = new_particle(g, this, color, id);
        G_Particle_NewGroup(g, p);
    }
    if g.m.r8(cs + 0x344) == 0 {
        if g.m.r32(fd + 0x42e) != NONE {
            u_sound_play(g, fd + 0x42e, true);
        }
    } else if g.m.r32(fd + 0x446) != NONE {
        u_sound_play(g, fd + 0x446, true);
    }
    // Collision spawn.
    if g.m.r32(cs + 0x2e0) == NONE {
        return result;
    }
    if g.m.r8(cs + 0x2e4) == 0 && g.m.r32(this + 0xc6) != 0 {
        return result;
    }
    let due = g.m.ri32(this + 0xc2).wrapping_add(g.m.ri32(cs + 0x2e6));
    if time < due {
        return result;
    }
    let req = new_spawn_request(g);
    let id = g.m.r32(cs + 0x2e0);
    g.m.w32(req, id);
    let x = g.m.r32(this);
    g.m.w32(req + 4, x);
    let y = g.m.r32(this + 4);
    g.m.w32(req + 8, y);
    let pl = g.m.r32(this + 0xca);
    g.m.w32(req + 0x12, pl);
    g.m.w32(req + 0x1c, this);
    let ow = g.m.r32(this + 0x92);
    g.m.w32(req + 0x20, ow);
    G_EG_RequestSpawn(g, req, 0, 0);
    g.m.wi32(this + 0xc2, time);
    let c = g.m.r32(this + 0xc6).wrapping_add(1);
    g.m.w32(this + 0xc6, c);
    result
}

/// `public: void __thiscall G_Entity::DoMovementAI(int, bool *, bool *)` — 0x414180.
/// `a1`/`a2` receive the "delete" and "destroy" requests.
pub fn G_Entity_DoMovementAI(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32) {
    let sp = g.m.sp;
    movement_ai_impl(g, this, a0, a1, a2);
    g.m.pop_to(sp);
}

fn movement_ai_impl(g: &mut G, this: u32, time: i32, a1: u32, a2: u32) {
    g.m.w8(a1, 0);
    g.m.w8(a2, 0);
    if g.m.r8(this + 0xc0) != 0 {
        G_Entity_Priv_MoveToTargetLoc(g, this);
        G_Entity_Priv_RotateToTargetLoc(g, this, time);
        return;
    }
    let def = g.m.r32(this + 0x8a);
    let fd = def + 0x16;
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let loc = g.m.alloca(8);
    let dist = g.m.alloca(4);
    let num = g.m.alloca(4);
    let found = G_Game_Player_GetClosestActive_41f550(g, this, loc, dist, num);
    if !found {
        g.m.wi32(this + 0x110, -1);
        if g.m.r8(st + 0x34c) != 0 {
            g.m.w8(a1, 1);
            return;
        }
        if g.m.r8(st + 0x34d) != 0 {
            g.m.w8(a2, 1);
            return;
        }
        if g.m.r8(fd + 0x10e) != 0 {
            G_Entity_Priv_Flee(g, this, 0x61726f6e);
            return;
        }
        if g.m.r8(fd + 0x10f) != 0 {
            G_Entity_Priv_Flee(g, this, 0x61726f73);
            return;
        }
    }
    if g.m.r8(st + 0x346) != 0 {
        G_Entity_Priv_DoCyclicMotion(g, this);
    }
    if g.m.r8(fd + 0x105) != 0 {
        G_Entity_Priv_ConstrainInGameArea(g, this);
    }
    let mut hunts = false;
    let lx = g.m.r32(loc);
    g.m.w32(this + 0x114, lx);
    let ly = g.m.r32(loc + 4);
    g.m.w32(this + 0x118, ly);
    let n = g.m.r32(num);
    g.m.w32(this + 0x110, n);
    if found {
        let range = g.m.rf32(st + 0x446);
        let d = g.m.rf32(dist);
        if range == 0.0 || !(d < range) {
            hunts = g.m.r8(st + 0x345) != 0;
        } else {
            let nm = st + 0x556;
            if g.m.strcmp(nm, b"Delete") == 0 {
                g.m.w8(a1, 1);
                return;
            }
            if g.m.strcmp(nm, b"Destroy") == 0 {
                g.m.w8(a2, 1);
                return;
            }
            let mut cs = st;
            if g.m.r8(nm) != 0 && g.m.strcmp(nm, b"none") != 0 {
                G_Entity_ChangeState(g, this, false, nm, time, a1, a2);
                if g.m.r8(a1) != 0 || g.m.r8(a2) != 0 {
                    return;
                }
                cs = G_Entity_GetCurrentStatePtr(g, this);
                if g.m.r8(cs + 0x347) != 0 {
                    G_Entity_Priv_ReverseFromTarget(g, this, loc, d);
                }
            }
            if g.m.r8(cs + 0x348) != 0 {
                G_Entity_Priv_HoldToTarget(g, this, loc);
                hunts = false;
            } else {
                hunts = g.m.r8(cs + 0x345) != 0;
            }
        }
    }
    if g.m.r8(this + 0xc0) == 0 && hunts {
        G_Entity_Priv_MoveToTargetLoc(g, this);
    } else {
        G_Entity_Priv_AdjustToRequiredVelocity(g, this);
    }
}

/// `public: void __thiscall G_Entity::ProcessRules(int, bool *, bool *)` — 0x414490.
/// Up to five rules per state: a condition (named by the string table at
/// 0x4dfe63, 0x40 bytes apiece) selects an action, normally a state change.
pub fn G_Entity_ProcessRules(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32) {
    let sp = g.m.sp;
    process_rules_impl(g, this, a0, a1, a2);
    g.m.pop_to(sp);
}

fn process_rules_impl(g: &mut G, this: u32, time: i32, a1: u32, a2: u32) {
    g.m.w8(a1, 0);
    g.m.w8(a2, 0);
    let st = G_Entity_GetCurrentStatePtr(g, this);
    for rule in 0..5u32 {
        let rp = rule * 0x88 + st + 0x28;
        if g.m.r32(rp) == NONE {
            continue;
        }
        let unit = g.m.r32(rp);
        let fam = g.m.r32(this + 0x8e);
        if G_UnitDef_MasterList_GetRefPtrForID_ByFamily(g, unit as i32, fam) == 0 {
            // Unknown unit: logged, and the rule disabled.
            g.m.w32(rp, NONE);
            continue;
        }
        if g.m.r8(rp + 4) == 0 {
            continue;
        }
        let mut idx: i32 = -1;
        for i in 0..0x11u32 {
            if g.m.strcmp_mem(rp + 4, 0x4dfe63 + (i << 6)) == 0 {
                idx = i as i32;
                break;
            }
        }
        if idx == -1 {
            // Unknown condition: logged, and the rule disabled.
            g.m.w8(rp + 4, 0);
            continue;
        }
        let loc = g.m.alloca(8);
        G_GameObject_GetLoc_4240e0(g, this, loc);
        let range = g.m.ri32(rp + 0x84);
        let cond: bool = match idx {
            0 => G_EG_RuleCondition_IsEntityTrackingPlayer(g, unit as i32, loc, range),
            1 => !G_EG_RuleCondition_IsEntityTrackingPlayer(g, unit as i32, loc, range),
            2 => G_EG_RuleCondition_IsEntityActive(g, unit as i32, loc, range),
            3 => !G_EG_RuleCondition_IsEntityActive(g, unit as i32, loc, range),
            4 => !G_EG_RuleCondition_IsAnyDestroyableAirEntityActive(g),
            5 => !G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive(g),
            6 => {
                !G_EG_RuleCondition_IsAnyDestroyableAirEntityActive(g)
                    && !G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive(g)
            }
            7 => !G_Game_Player_IsAnyActive(g),
            8 => {
                if range != 0 {
                    let v = g.m.alloca(8);
                    let p = g.m.alloca(4);
                    G_Entity_Priv_CheckWithinRangeOfPlayers(g, this, range, v, p)
                } else {
                    false
                }
            }
            9 => {
                if range != 0 {
                    let v = g.m.alloca(8);
                    let p = g.m.alloca(4);
                    !G_Entity_Priv_CheckWithinRangeOfPlayers(g, this, range, v, p)
                } else {
                    false
                }
            }
            10 => g.m.r8(this + 0xb6) != 0,
            11 => g.m.rf32(this + 0x62) == g.m.rf32(this + 0x66),
            12 => g.m.rf32(this + 0x54) == g.m.rf32(this + 0x58),
            13 => g.m.rf32(this + 0x7a) == g.m.rf32(this + 0x7e),
            14 => range == G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType(g, unit as i32),
            15 => G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType(g, unit as i32) < range,
            16 => G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType(g, unit as i32) > range,
            _ => false,
        };
        if cond {
            G_Entity_ChangeState(g, this, false, rp + 0x44, time, a1, a2);
            return;
        }
    }
}

/// `public: void __thiscall G_Entity::Animate(int)` — 0x4148f0.
/// Steps the frame (`+0x20`) within the current heading's block of
/// `framesPerDirection` frames, every `frameDelay` ticks, `frameDelta`
/// frames at a time, looping, ping-ponging or stopping (`+0xb6`).
pub fn G_Entity_Animate(g: &mut G, this: u32, a0: i32) {
    if g.m.r8(this + 0xbc) == 0 {
        return;
    }
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let n = g.m.ri32(st + 0x318);
    if n <= 0 {
        return;
    }
    if g.m.r8(st + 0x2ff) != 0 {
        return;
    }
    if !(a0 > g.m.ri32(this + 0xb0).wrapping_add(g.m.ri32(st + 0x314))) {
        return;
    }
    let fpd = g.m.ri32(st + 0x310);
    let dirs = g.m.ri32(st + 0x304);
    let heading = g.m.ri32(this + 0x130);
    let base = ftol(heading as f64 / g.m.rf32(0x4e04d0) as f64 * dirs as f64);
    let first = base.wrapping_mul(fpd);
    let last = first.wrapping_add(fpd).wrapping_sub(1);
    if g.m.r8(st + 0x2fe) != 0 {
        let f = g.random_int(first, last);
        g.m.wi32(this + 0x20, f);
    } else if g.m.r8(this + 0xb6) == 0 {
        let mut i = 0;
        while i < n {
            if g.m.r8(this + 0xb6) != 0 {
                break;
            }
            let frame = g.m.ri32(this + 0x20);
            if g.m.r8(this + 0xb4) == 0 {
                if frame == last {
                    if g.m.r8(st + 0x2fd) != 0 {
                        if g.m.r8(st + 0x2fc) != 0 {
                            g.m.w8(this + 0xb4, 1);
                            g.m.wi32(this + 0x20, last.wrapping_sub(1));
                        } else {
                            g.m.wi32(this + 0x20, first);
                        }
                    } else {
                        g.m.w8(this + 0xb6, 1);
                    }
                } else {
                    g.m.wi32(this + 0x20, frame.wrapping_add(1));
                }
            } else if frame == first {
                if g.m.r8(st + 0x2fd) != 0 {
                    if g.m.r8(st + 0x2fc) != 0 {
                        g.m.w8(this + 0xb4, 0);
                        g.m.wi32(this + 0x20, first.wrapping_add(1));
                    } else {
                        g.m.wi32(this + 0x20, last);
                    }
                } else {
                    g.m.w8(this + 0xb6, 1);
                }
            } else {
                g.m.wi32(this + 0x20, frame.wrapping_sub(1));
            }
            i += 1;
        }
    }
    g.m.wi32(this + 0xb0, a0);
    g.m.w8(this + 0x34, 1);
    let id = g.m.r32(this + 0x1c);
    let fr = g.m.ri32(this + 0x20);
    let fd = sprite_get_ptr_to_sprite_frame(g, id, fr);
    g.m.w32(this + 0x4e, fd);
}

/// `public: void __thiscall G_Entity::SpawnControl(int)` — 0x414ae0.
/// Runs the current state's spawn sets: a set fires `volley` entities, one
/// per delay, then waits `rate` before the next volley (when it repeats).
/// Run-time record (0x16 bytes): +0 rate, +4 last volley time, +8 left in
/// the volley, +0xc volley size, +0x10 delay countdown, +0x14 active.
pub fn G_Entity_SpawnControl(g: &mut G, this: u32, a0: i32) {
    let sp = g.m.sp;
    spawn_control_impl(g, this, a0);
    g.m.pop_to(sp);
}

fn spawn_control_impl(g: &mut G, this: u32, time: i32) {
    G_Entity_Priv_DoRotationAsRequired(g, this, time);
    if g.m.r8(this + 0xb7) == 0 {
        return;
    }
    let idx = g.m.r32(this + 0x9e);
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let mut count = 0;
    let rl = g.m.r32(this + 0x194 + idx.wrapping_mul(4));
    if rl != 0 {
        count = list::num(&g.m, rl);
    }
    for i in 0..count {
        let ss = list::nth(&g.m, g.m.r32(st + 0x5d6), i);
        let idx = g.m.r32(this + 0x9e);
        let ri = list::nth(&g.m, g.m.r32(this + 0x194 + idx.wrapping_mul(4)), i);
        if g.m.r32(ss + 0x20) == NONE {
            continue;
        }
        if g.m.r8(ri + 0x14) == 0 {
            continue;
        }
        if g.m.ri32(ri) < 0 {
            continue;
        }
        if g.m.r8(this + 0xc0) != 0 && g.m.r8(ss + 0x4e) == 0 {
            continue;
        }
        if g.m.r8(ss + 0x47) != 0 {
            let c = g.m.ri32(ri + 8);
            if c > 0 && c >= g.m.ri32(ri + 0xc) && !G_Entity_IsWithinGameArea(g, this) {
                g.m.w32(ri + 8, 0);
                continue;
            }
        }
        let mut fire = false;
        if g.m.ri32(ri + 8) > 0 {
            if g.m.ri32(ri + 0x10) > 0 {
                let v = g.m.ri32(ri + 0x10) - 1;
                g.m.wi32(ri + 0x10, v);
            }
            if g.m.ri32(ri + 0x10) > 0 {
                continue;
            }
            fire = true;
            let v = g.m.ri32(ri + 8) - 1;
            g.m.wi32(ri + 8, v);
            let lo = g.m.ri32(ss + 0x3c);
            let hi = g.m.ri32(ss + 0x40);
            let d = g.random_int(lo, hi);
            g.m.wi32(ri + 0x10, d);
        } else {
            if g.m.r8(ss + 0x46) == 0 {
                g.m.w8(ri + 0x14, 0);
                continue;
            }
            if time < g.m.ri32(ri + 4).wrapping_add(g.m.ri32(ri)) {
                continue;
            }
            g.m.wi32(ri + 4, time);
            let lo = g.m.ri32(ss + 0x3c);
            let hi = g.m.ri32(ss + 0x40);
            let d = g.random_int(lo, hi);
            g.m.wi32(ri + 0x10, d);
            let lo = g.m.ri32(ss + 0x34);
            let hi = g.m.ri32(ss + 0x38);
            let n = g.random_int(lo, hi);
            g.m.wi32(ri + 0xc, n);
            let n = g.m.ri32(ri + 0xc);
            g.m.wi32(ri + 8, n);
            let lo = g.m.ri32(ss + 0x2c);
            let hi = g.m.ri32(ss + 0x30);
            let r = g.random_int(lo, hi);
            g.m.wi32(ri, r);
            if g.m.r8(ss + 0x48) != 0 && g.m.ri32(ss + 0x4a) > g.m.ri32(this + 0xb8) {
                let v = g.m.r32(ss + 0x4a);
                g.m.w32(this + 0xb8, v);
            }
        }
        if !fire {
            continue;
        }
        let id = g.m.r32(ss + 0x20);
        if id == NONE {
            continue;
        }
        let mut set_heading = false;
        let fam = g.m.r32(this + 0x8e);
        let unit = G_UnitDef_MasterList_GetRefPtrForID_ByFamily(g, id as i32, fam);
        if unit != 0 && g.m.r8(unit + 0x130) != 0 {
            if g.m.r8(this + 0x134) != 0 {
                continue;
            }
            if g.m.r8(this + 0x135) == 0 {
                continue;
            }
        }
        let req = new_spawn_request(g);
        g.m.w32(req, id);
        let xo = g.m.ri32(ss + 0x24);
        let yo = g.m.ri32(ss + 0x28);
        if g.m.r8(ss + 0x44) == 0 {
            if g.m.r8(ss + 0x45) == 0 {
                let x = g.m.r32(this);
                g.m.w32(req + 4, x);
                let y = g.m.r32(this + 4);
                g.m.w32(req + 8, y);
            }
            let mut scaled = false;
            let sc = g.m.rf32(this + 0x7a);
            if sc != 1.0 && unit != 0 && g.m.r8(unit + 0x12c) != 0 {
                let ys = yo as f64 * sc as f64;
                let rx = g.m.rf32(req + 4) as f64;
                g.m.wf32(req + 4, (xo as f64 * sc as f64 + rx) as f32);
                let ry = g.m.rf32(req + 8) as f64;
                g.m.wf32(req + 8, (ry + ys) as f32);
                scaled = true;
            }
            if g.m.r8(ss + 0x45) != 0 {
                g.m.wf32(req + 4, xo as f32);
                g.m.wf32(req + 8, yo as f32);
            } else if !scaled {
                let rx = g.m.rf32(req + 4) as f64;
                g.m.wf32(req + 4, (xo as f64 + rx) as f32);
                let ry = g.m.rf32(req + 8) as f64;
                g.m.wf32(req + 8, (yo as f64 + ry) as f32);
            }
        } else {
            let mut ang = G_Entity_GetAngleFromSpriteInfo(g, this);
            if g.m.r8(ss + 0x4f) != 0 {
                ang = ang.wrapping_add(g.m.ri32(ss + 0x50));
                if ang > 0x167 {
                    ang -= 0x168;
                }
                g.m.wi32(req + 0xe, ang);
                set_heading = true;
            }
            let c = umath::cos(ang);
            let s = umath::sin(ang);
            let sc = g.m.rf32(this + 0x7a);
            let (ox, oy);
            let mut scaled = false;
            let mut sx = 0i32;
            let mut sy = 0i32;
            if sc != 1.0 && unit != 0 && g.m.r8(unit + 0x12c) != 0 {
                let a = xo as f64 * sc as f64;
                let b = yo as f64 * sc as f64;
                sx = ftol(a * c as f64 - b * s as f64);
                sy = ftol(b * c as f64 + a * s as f64);
                scaled = true;
            }
            if !scaled {
                sx = ftol(xo as f64 * c as f64 - yo as f64 * s as f64);
                sy = ftol(yo as f64 * c as f64 + xo as f64 * s as f64);
            }
            ox = sx;
            oy = sy;
            let tx = g.m.rf32(this) as f64;
            g.m.wf32(req + 4, (ox as f64 + tx) as f32);
            let ty = g.m.rf32(this + 4) as f64;
            g.m.wf32(req + 8, (oy as f64 + ty) as f32);
        }
        let v = g.m.r8(ss + 0x4f);
        g.m.w8(req + 0xd, v);
        if !set_heading {
            let v = g.m.r32(ss + 0x50);
            g.m.w32(req + 0xe, v);
        }
        g.m.w32(req + 0x1c, this);
        let ow = g.m.r32(this + 0x92);
        g.m.w32(req + 0x20, ow);
        let pl = g.m.r32(this + 0xca);
        g.m.w32(req + 0x12, pl);
        let v = g.m.r8(ss + 0x54);
        g.m.w8(req + 0x1a, v);
        let v = g.m.r8(ss + 0x55);
        g.m.w8(req + 0x1b, v);
        G_EG_RequestSpawn(g, req, 0, unit);
    }
}

/// `public: int __thiscall G_Entity::GetAngleFromSpriteInfo(void)` — 0x415090.
pub fn G_Entity_GetAngleFromSpriteInfo(g: &mut G, this: u32) -> i32 {
    let st = G_Entity_GetCurrentStatePtr(g, this);
    if st == 0 {
        return 0;
    }
    let dirs = g.m.ri32(st + 0x304);
    let fpd = g.m.ri32(st + 0x310);
    let frame = g.m.ri32(this + 0x20);
    if dirs == 1 {
        return idiv(0x168, fpd).wrapping_mul(frame);
    }
    let mut d = idiv(frame, fpd);
    if d < 0 {
        d = 0;
    }
    idiv(0x168, dirs).wrapping_mul(d)
}

/// `public: int __thiscall G_Entity::GetFrameForAngle(int)` — 0x415120.
/// The first frame of the direction nearest to `a0` degrees.
pub fn G_Entity_GetFrameForAngle(g: &mut G, this: u32, a0: i32) -> i32 {
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let mut n = g.m.ri32(st + 0x304);
    if n <= 0 {
        n = 1;
    }
    let per = idiv(0x168, n);
    let q = a0 as f64 / per as f64;
    let mut t = ftol(q);
    let frac = q - t as f64;
    // test ah,5 / jne: skipped when the fraction is below 0.5 or NaN.
    if !(frac < 0.5) && !frac.is_nan() {
        t = t.wrapping_add(1);
    }
    if t < 0 {
        t = n - 1;
    } else if t > n - 1 {
        t = 0;
    }
    t.wrapping_mul(g.m.ri32(st + 0x310))
}

/// `public: void __thiscall G_Entity::Destroy(enum G_Game_PlayerNum, int)` — 0x415200.
/// `a0` the destroying player, `a1` the game time (the notice's argument).
pub fn G_Entity_Destroy(g: &mut G, this: u32, a0: i32, a1: i32) {
    let sp = g.m.sp;
    destroy_impl(g, this, a0, a1);
    g.m.pop_to(sp);
}

fn destroy_impl(g: &mut G, this: u32, player: i32, a1: i32) {
    if g.m.r8(this + 0xbf) != 0 {
        return;
    }
    let def = g.m.r32(this + 0x8a);
    let fd = def + 0x16;
    G_GameObject_Glow_Stop(g, this);
    if g.m.r8(this + 0x19) == 0 && g.m.r8(fd + 0x497) != 0 {
        let rect = g.m.alloca(0x10);
        G_GameObject_GetBounds_424210(g, this, rect);
        G_Debris_New(g, rect);
    }
    if g.m.r32(fd + 0x462) != NONE {
        let color = g.m.r16(fd + 0x466);
        let id = g.m.r32(fd + 0x462);
        let p = new_particle(g, this, color, id);
        G_Particle_NewGroup(g, p);
    }
    if g.m.r32(fd + 0x45e) != NONE && G_Entity_CanSpawnOnMedia(g, this) {
        let req = new_spawn_request(g);
        let id = g.m.r32(fd + 0x45e);
        g.m.w32(req, id);
        let x = g.m.r32(this);
        g.m.w32(req + 4, x);
        let y = g.m.r32(this + 4);
        g.m.w32(req + 8, y);
        let pl = g.m.r32(this + 0xca);
        g.m.w32(req + 0x12, pl);
        g.m.w32(req + 0x1c, this);
        let ow = g.m.r32(this + 0x92);
        g.m.w32(req + 0x20, ow);
        G_EG_RequestSpawn(g, req, 0, 0);
    }
    let notice = fd + 0x468;
    if g.m.r8(notice) != 0 && g.m.strcmp(notice, b"none") != 0 {
        // G_Notice_Settings: {type string, 2 flag bytes, ...}; the rest was
        // left uninitialised by the original.
        let ns = g.m.alloca(0x28);
        g.m.w32(ns, notice);
        g.m.w8(ns + 4, 0);
        g.m.w8(ns + 5, 0);
        G_Notice_Request(g, ns, a1);
    }
    if g.m.r32(fd + 0x49e) != NONE {
        u_sound_play(g, fd + 0x49e, true);
    }
    g.m.w8(this + 0xbf, 1);
    g.m.wi32(this + 0xce, player);
    g.m.w8(this + 0xd2, 1);
    if g.m.r8(fd + 0x11c) != 0 {
        G_Game_GroundAccuracy_AddDestroyed(g);
    }
    if g.m.r8(fd + 0x498) == 0 {
        return;
    }
    // Release a random bonus: the roll is compared against the
    // cumulative thresholds in the permanent floats 0xd1..0xdb.
    let roll = g.random_int(0, 100);
    let mut pick = |g: &mut G, i: i32| ftol(pf(g, i) as f64);
    let obj: i32;
    if roll < pick(g, 0xd1) {
        if G_Game_GroundAccuracy_CheckForRewardThisLevel(g) {
            if roll < pick(g, 0xda) {
                obj = G_Res_GetPermObjectID(g, 0x1e);
                G_Game_GroundAccuracy_SetRewardDone(g);
            } else {
                obj = G_Res_GetPermObjectID(g, 0x19);
            }
        } else {
            obj = G_Res_GetPermObjectID(g, 0x19);
        }
    } else if roll < pick(g, 0xd2) {
        obj = G_Res_GetPermObjectID(g, 0x1a);
    } else if roll < pick(g, 0xd3) {
        obj = G_Res_GetPermObjectID(g, 0x1b);
    } else if roll < pick(g, 0xd4) {
        obj = G_Res_GetPermObjectID(g, 0x1c);
    } else if roll < pick(g, 0xd5) {
        obj = G_Res_GetPermObjectID(g, 0x1d);
    } else if roll < pick(g, 0xd6) {
        obj = G_Res_GetPermObjectID(g, 0x1e);
    } else if roll < pick(g, 0xd7) {
        obj = G_Res_GetPermObjectID(g, 0x1f);
    } else if roll < pick(g, 0xd8) {
        obj = G_Res_GetPermObjectID(g, 0x20);
    } else if G_Game_GetCurrentLevelNumber(g) < pick(g, 0xdb) {
        obj = G_Res_GetPermObjectID(g, 0x20);
    } else if roll < pick(g, 0xd9) {
        obj = G_Res_GetPermObjectID(g, 0x21);
    } else {
        obj = G_Res_GetPermObjectID(g, 0x22);
    }
    if obj as u32 != NONE {
        let req = new_spawn_request(g);
        g.m.wi32(req, obj);
        let x = g.m.r32(this);
        g.m.w32(req + 4, x);
        let y = g.m.r32(this + 4);
        g.m.w32(req + 8, y);
        let pl = g.m.r32(this + 0xca);
        g.m.w32(req + 0x12, pl);
        g.m.w32(req + 0x1c, this);
        let ow = g.m.r32(this + 0x92);
        g.m.w32(req + 0x20, ow);
        G_EG_RequestSpawn(g, req, 0, 0);
    }
}

/// `public: bool __thiscall G_Entity::CanSpawnOnMedia(void)` — 0x415950.
/// True for air units and units that spawn on any media; a ground unit over
/// a medium (mask surface 1) instead spawns the media impact for its
/// `mediaImpactSize` (rolling which one) and answers false.
pub fn G_Entity_CanSpawnOnMedia(g: &mut G, this: u32) -> bool {
    let sp = g.m.sp;
    let r = can_spawn_on_media_impl(g, this);
    g.m.pop_to(sp);
    r
}

fn can_spawn_on_media_impl(g: &mut G, this: u32) -> bool {
    let def = g.m.r32(this + 0x8a);
    let fd = def + 0x16;
    if g.m.r8(fd + 0x10d) == 0 || g.m.r8(fd + 0x113) != 0 {
        return true;
    }
    let px = g.m.alloca(4);
    let py = g.m.alloca(4);
    G_GameObject_GetLoc_424100(g, this, px, py);
    let ix = ftol(g.m.rf32(px) as f64).wrapping_add(0x20);
    let top = G_Bgnd_GetUpdateRectTop(g);
    let iy = ftol(g.m.rf32(py) as f64);
    let loc = g.m.alloca(8);
    g.m.wi32(loc, ix);
    g.m.wi32(loc + 4, top.wrapping_add(iy));
    let surface = G_Bgnd_MediaMask_GetSurfaceAtLoc(g, loc);
    if surface as u32 > 1 {
        return false;
    }
    if surface == 0 {
        return true;
    }
    let mut obj: i32 = NONE as i32;
    let sz = g.m.r32(fd + 0x2ca);
    if sz != 0 && sz != NONE {
        let mut c = sz.wrapping_sub(0x2064656d);
        if c == 0 {
            obj = G_Res_GetPermObjectID(g, 8);
        } else {
            c = c.wrapping_sub(0x410dfbff);
            if c == 0 {
                obj = if g.random_int(0, 1) != 0 {
                    G_Res_GetPermObjectID(g, 8)
                } else {
                    G_Res_GetPermObjectID(g, 9)
                };
            } else {
                c = c.wrapping_sub(0x401);
                if c == 0 {
                    let r = g.random_int(0, 2);
                    obj = match r {
                        0 => G_Res_GetPermObjectID(g, 6),
                        1 => G_Res_GetPermObjectID(g, 7),
                        _ => G_Res_GetPermObjectID(g, 8),
                    };
                } else {
                    c = c.wrapping_sub(0x806);
                    if c == 0 {
                        obj = if g.random_int(0, 1) != 0 {
                            G_Res_GetPermObjectID(g, 6)
                        } else {
                            G_Res_GetPermObjectID(g, 7)
                        };
                    } else {
                        c = c.wrapping_sub(0x05fff3f9);
                        if c == 0 {
                            obj = G_Res_GetPermObjectID(g, 9);
                        } else {
                            c = c.wrapping_sub(0x04ef0c07);
                            if c == 0 {
                                obj = G_Res_GetPermObjectID(g, 7);
                            } else {
                                c = c.wrapping_sub(0x0d0cfc01);
                                if c == 0 {
                                    obj = G_Res_GetPermObjectID(g, 6);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if obj as u32 != NONE {
        let req = new_spawn_request(g);
        g.m.wi32(req, obj);
        let x = g.m.r32(this);
        g.m.w32(req + 4, x);
        let y = g.m.r32(this + 4);
        g.m.w32(req + 8, y);
        let pl = g.m.r32(this + 0xca);
        g.m.w32(req + 0x12, pl);
        g.m.w32(req + 0x1c, this);
        let ow = g.m.r32(this + 0x92);
        g.m.w32(req + 0x20, ow);
        G_EG_RequestSpawn(g, req, 0, 0);
    }
    false
}

/// `public: bool __thiscall G_Entity::IsWithinGameArea(void)` — 0x415c90.
pub fn G_Entity_IsWithinGameArea(g: &mut G, this: u32) -> bool {
    let x = g.m.rf32(this) as f64;
    if x < g.m.rf32(0x4e04e0) as f64 {
        return false;
    }
    if (pf(g, 0x36) as f64) < x {
        return false;
    }
    let y = g.m.rf32(this + 4) as f64;
    if y < g.m.rf32(0x4e04e0) as f64 {
        return false;
    }
    if (pf(g, 0x37) as f64) < y {
        return false;
    }
    true
}

/// `private: void __thiscall G_Entity::Priv_MoveToTargetLoc(void)` — 0x415d20.
/// Accelerates towards the target (`+0x114/0x118`) by the state's delta
/// (flee values while fleeing), clamping the velocity to +-max speed.
pub fn G_Entity_Priv_MoveToTargetLoc(g: &mut G, this: u32) {
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let (max, delta) = if g.m.r8(this + 0xc0) == 0 {
        (g.m.rf32(st + 0x452), g.m.rf32(st + 0x456))
    } else {
        (g.m.rf32(st + 0x45a), g.m.rf32(st + 0x45e))
    };
    let x = g.m.rf32(this);
    let tx = g.m.rf32(this + 0x114);
    g.m.wf32(this + 0x108, if x < tx { delta } else { -delta });
    let y = g.m.rf32(this + 4);
    let ty = g.m.rf32(this + 0x118);
    g.m.wf32(this + 0x10c, if y < ty { delta } else { -delta });
    // x
    let sum = g.m.rf32(this + 0x10) as f64 + g.m.rf32(this + 0x108) as f64;
    g.m.wf32(this + 0x10, sum as f32);
    if sum > max as f64 {
        g.m.wf32(this + 0x10, max);
    } else if sum < -(max as f64) {
        g.m.wf32(this + 0x10, -max);
    }
    // y
    let sum = g.m.rf32(this + 0x14) as f64 + g.m.rf32(this + 0x10c) as f64;
    g.m.wf32(this + 0x14, sum as f32);
    if sum > max as f64 {
        g.m.wf32(this + 0x14, max);
    } else if sum < -(max as f64) {
        g.m.wf32(this + 0x14, -max);
    }
}

/// `private: void __thiscall G_Entity::Priv_ConstrainInGameArea(void)` — 0x415e20.
/// Bounces off the left/right edges (-32 .. width + 32) and the top/bottom,
/// negating velocity, acceleration and target velocity.
pub fn G_Entity_Priv_ConstrainInGameArea(g: &mut G, this: u32) {
    let bx = ftol(pf(g, 0x36) as f64);
    let by = ftol(pf(g, 0x37) as f64);
    if (g.m.rf32(this) as f64) < g.m.rf32(0x4e04e4) as f64 {
        flip(g, this + 0x10);
        g.m.w32(this, 0xc2000000);
        flip(g, this + 0x108);
        flip(g, this + 0x100);
    }
    let w = g.m.ri32(this + 0x24);
    // fcompp: taken when (width + x) > (bx + 0x20), as `test ah,0x45` is zero.
    if (w as f64 + g.m.rf32(this) as f64) > bx.wrapping_add(0x20) as f64 {
        let nx = bx.wrapping_sub(w).wrapping_add(0x20);
        g.m.wf32(this, nx as f32);
        flip(g, this + 0x10);
        flip(g, this + 0x108);
        flip(g, this + 0x100);
    }
    let hh = g.m.ri32(this + 0x30);
    if (g.m.rf32(this + 4) as f64 - hh as f64) < g.m.rf32(0x4e04e0) as f64 {
        flip(g, this + 0x14);
        g.m.wf32(this + 4, hh as f32);
        flip(g, this + 0x10c);
        flip(g, this + 0x104);
    }
    if (by as f64) < hh as f64 + g.m.rf32(this + 4) as f64 {
        flip(g, this + 0x14);
        g.m.wf32(this + 4, by.wrapping_sub(hh) as f32);
        flip(g, this + 0x10c);
        flip(g, this + 0x104);
    }
}

/// `private: void __thiscall G_Entity::Priv_DoCyclicMotion(void)` — 0x415fc0.
/// Swings the velocity back and forth within +-(amplitude / 2 .. amplitude
/// + 0.01 .. 1.0), two rand() draws per call.
pub fn G_Entity_Priv_DoCyclicMotion(g: &mut G, this: u32) {
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let amp = ftol(g.m.rf32(st + 0x452) as f64);
    let r = g.random_int(amp / 2, amp);
    let mut step = r as f32;
    let r2 = g.random_int(1, 100);
    let adj = r2 as f64 / g.m.rf32(0x4e04e8) as f64;
    step = (step as f64 + adj) as f32;
    let neg = -(step as f64);
    if g.m.rf32(this + 0x10) > step {
        g.m.wf32(this + 0x10, step);
        flip(g, this + 0x108);
    }
    if (g.m.rf32(this + 0x10) as f64) < neg {
        g.m.wf32(this + 0x10, neg as f32);
        flip(g, this + 0x108);
    }
    if g.m.rf32(this + 0x14) > step {
        g.m.wf32(this + 0x14, step);
        flip(g, this + 0x10c);
    }
    if (g.m.rf32(this + 0x14) as f64) < neg {
        g.m.wf32(this + 0x14, neg as f32);
        flip(g, this + 0x10c);
    }
    let vx = g.m.rf32(this + 0x10) as f64 + g.m.rf32(this + 0x108) as f64;
    g.m.wf32(this + 0x10, vx as f32);
    let vy = g.m.rf32(this + 0x14) as f64 + g.m.rf32(this + 0x10c) as f64;
    g.m.wf32(this + 0x14, vy as f32);
    let v = g.m.r32(this + 0x10);
    g.m.w32(this + 0x100, v);
    let v = g.m.r32(this + 0x14);
    g.m.w32(this + 0x104, v);
}

/// `private: bool __thiscall G_Entity::Priv_DoRotationAsRequired(int)` — 0x416120.
/// True when the state does not rotate to its target; otherwise rotates
/// unless a spawn set is pausing rotation.
pub fn G_Entity_Priv_DoRotationAsRequired(g: &mut G, this: u32, a0: i32) -> bool {
    let mut result = false;
    let st = G_Entity_GetCurrentStatePtr(g, this);
    if g.m.r8(st + 0x2ff) == 0 {
        result = true;
        g.m.w8(this + 0xb5, 0);
    } else {
        if g.m.ri32(this + 0xb8) > 0 {
            let v = g.m.ri32(this + 0xb8) - 1;
            g.m.wi32(this + 0xb8, v);
            if g.m.ri32(this + 0xb8) < 0 {
                g.m.w32(this + 0xb8, 0);
            }
        }
        let mut flag = false;
        if g.m.ri32(this + 0xb8) > 0 {
            flag = true;
        } else {
            let mut count = 0;
            let idx = g.m.r32(this + 0x9e);
            let rl = g.m.r32(this + 0x194 + idx.wrapping_mul(4));
            if rl != 0 {
                count = list::num(&g.m, rl);
            }
            for i in 0..count {
                let ss = list::nth(&g.m, g.m.r32(st + 0x5d6), i);
                if g.m.r32(ss + 0x20) == NONE {
                    continue;
                }
                let idx = g.m.r32(this + 0x9e);
                let ri = list::nth(&g.m, g.m.r32(this + 0x194 + idx.wrapping_mul(4)), i);
                if g.m.r8(ri + 0x14) != 0 && g.m.r8(ss + 0x48) != 0 {
                    let c = g.m.ri32(ri + 8);
                    if c > 0 && c < g.m.ri32(ri + 0xc) {
                        flag = true;
                        break;
                    }
                }
            }
        }
        if !flag {
            result = G_Entity_Priv_RotateToTargetLoc(g, this, a0);
        }
    }
    result
}

/// `private: bool __thiscall G_Entity::Priv_RotateToTargetLoc(int)` — 0x416260.
/// Turns the sprite frame towards the target, `framesPerDirection` frames
/// per step; true when it already faces it (or the state does not rotate).
pub fn G_Entity_Priv_RotateToTargetLoc(g: &mut G, this: u32, a0: i32) -> bool {
    let mut result = false;
    let st = G_Entity_GetCurrentStatePtr(g, this);
    if g.m.r8(st + 0x2ff) == 0 {
        g.m.w8(this + 0xb5, 0);
        return true;
    }
    if g.m.r8(this + 0xc0) == 0 {
        if g.m.ri32(this + 0x110) == -1 {
            g.m.w8(this + 0xb5, 0);
            return result;
        }
        g.m.w8(this + 0xb5, 1);
    }
    let due = g.m.ri32(this + 0xb0).wrapping_add(g.m.ri32(st + 0x314));
    if !(a0 > due) {
        return result;
    }
    let tx = g.m.rf32(this + 0x114);
    let ty = g.m.rf32(this + 0x118);
    let tyi = ftol(ty as f64);
    let txi = ftol(tx as f64);
    let yi = ftol(g.m.rf32(this + 4) as f64);
    let xi = ftol(g.m.rf32(this) as f64);
    let angle = umath::intercept_angle(xi, yi, txi, tyi);
    let cur = G_Entity_GetFrameForAngle(g, this, angle);
    let frame = g.m.ri32(this + 0x20);
    if frame == cur {
        return true;
    }
    let mut ecx = 0i32;
    let st2 = G_Entity_GetCurrentStatePtr(g, this);
    if st2 != 0 {
        let dirs = g.m.ri32(st2 + 0x304);
        let fpd2 = g.m.ri32(st2 + 0x310);
        if dirs == 1 {
            ecx = idiv(0x168, fpd2).wrapping_mul(frame);
        } else {
            let mut d = idiv(frame, fpd2);
            if d < 0 {
                d = 0;
            }
            ecx = idiv(0x168, dirs).wrapping_mul(d);
        }
    }
    let fpd = g.m.ri32(st + 0x310);
    let total = fpd.wrapping_mul(g.m.ri32(st + 0x304));
    let mut a = angle.wrapping_sub(ecx);
    if a > 0xb4 {
        a -= 0x168;
    }
    if a < -0xb4 {
        a += 0x168;
    }
    if a > 0 {
        for _ in 0..fpd {
            let f = g.m.ri32(this + 0x20).wrapping_add(1);
            g.m.wi32(this + 0x20, f);
            if f >= total {
                g.m.w32(this + 0x20, 0);
            }
        }
    } else {
        for _ in 0..fpd {
            let f = g.m.ri32(this + 0x20).wrapping_sub(1);
            g.m.wi32(this + 0x20, f);
            if f < 0 {
                g.m.wi32(this + 0x20, total.wrapping_sub(1));
            }
        }
    }
    g.m.wi32(this + 0xb0, a0);
    g.m.w8(this + 0x34, 1);
    let id = g.m.r32(this + 0x1c);
    let fr = g.m.ri32(this + 0x20);
    let fd = sprite_get_ptr_to_sprite_frame(g, id, fr);
    g.m.w32(this + 0x4e, fd);
    result
}

/// `private: void __thiscall G_Entity::Priv_Flee(enum G_UnitDef_FleeID)` — 0x416520.
/// Picks a flee target (`+0x114/0x118`) off an edge of the screen from the
/// flee id (a four-character tag; the compare chain subtracts cumulative
/// differences, as the original does).
pub fn G_Entity_Priv_Flee(g: &mut G, this: u32, a0: i32) {
    g.m.w8(this + 0xc0, 1);
    let steps: [(u32, char); 14] = [
        (0x61676563, 'A'),
        (0x000afc02, 'B'),
        (0x00000412, 'C'),
        (0x000009f7, 'D'),
        (0x00000005, 'E'),
        (0x03f0f1f2, 'F'),
        (0x00000412, 'G'),
        (0x000009f7, 'H'),
        (0x00000005, 'I'),
        (0x000afffb, 'J'),
        (0x0007f204, 'K'),
        (0x00000efd, 'L'),
        (0x09f1f103, 'M'),
        (0x00000efd, 'N'),
    ];
    let mut e = a0 as u32;
    let mut case = ' ';
    for (d, c) in steps.iter() {
        e = e.wrapping_sub(*d);
        if e == 0 {
            case = *c;
            break;
        }
    }
    // A random x within the screen width / y within the height.
    fn rx(g: &mut G) -> f32 {
        let hi = pf(g, 0x36);
        let lo = g.m.rf32(0x4e04e0);
        g.random_float(lo, hi) as f32
    }
    fn ry(g: &mut G) -> f32 {
        let hi = pf(g, 0x37);
        let lo = g.m.rf32(0x4e04e0);
        g.random_float(lo, hi) as f32
    }
    fn half(g: &mut G, i: i32) -> f32 {
        (pf(g, i) as f64 / g.m.rf32(0x4e04ec) as f64) as f32
    }
    let (tx, ty): (f32, f32);
    match case {
        'D' => {
            tx = rx(g);
            ty = pf(g, 0xe);
        }
        'E' => {
            tx = rx(g);
            ty = pf(g, 0xf);
        }
        'C' => {
            tx = pf(g, 0x10);
            ty = ry(g);
        }
        'B' => {
            tx = pf(g, 0x11);
            ty = ry(g);
        }
        'H' => {
            tx = half(g, 0x36);
            ty = pf(g, 0xe);
        }
        'I' => {
            tx = half(g, 0x36);
            ty = pf(g, 0xf);
        }
        'G' => {
            tx = pf(g, 0x10);
            ty = half(g, 0x37);
        }
        'F' => {
            tx = pf(g, 0x11);
            ty = half(g, 0x37);
        }
        'A' => {
            tx = half(g, 0x36);
            ty = half(g, 0x37);
        }
        'L' => {
            if half(g, 0x37) < g.m.rf32(this + 4) {
                tx = rx(g);
                ty = pf(g, 0xe);
            } else {
                tx = rx(g);
                ty = pf(g, 0xf);
            }
        }
        'N' => {
            if half(g, 0x36) < g.m.rf32(this) {
                tx = pf(g, 0x10);
                ty = ry(g);
            } else {
                tx = pf(g, 0x11);
                ty = ry(g);
            }
        }
        'K' => {
            if g.random_int(0, 1) != 0 {
                tx = rx(g);
                ty = pf(g, 0xe);
            } else {
                tx = rx(g);
                ty = pf(g, 0xf);
            }
        }
        'M' => {
            if g.random_int(0, 1) != 0 {
                tx = pf(g, 0x11);
                ty = ry(g);
            } else {
                tx = pf(g, 0x10);
                ty = ry(g);
            }
        }
        _ => return,
    }
    g.m.wf32(this + 0x114, tx);
    g.m.wf32(this + 0x118, ty);
}

/// `private: void __thiscall G_Entity::Priv_AdjustToRequiredVelocity(void)` — 0x416a50.
/// Moves the velocity towards the required velocity (`+0x100/0x104`) by
/// the acceleration (`+0x108/0x10c`) without overshooting. An orbiting
/// state instead steers only vx towards the state's max speed by its delta.
pub fn G_Entity_Priv_AdjustToRequiredVelocity(g: &mut G, this: u32) {
    if g.m.r8(this + 0x134) != 0 {
        let a = g.m.r32(0x4dfda8);
        let b = g.m.r32(0x4dfdac);
        g.m.w32(this + 0x10, a);
        g.m.w32(this + 0x14, b);
        g.m.w32(this + 0x100, a);
        g.m.w32(this + 0x104, b);
        g.m.w32(this + 0x108, a);
        g.m.w32(this + 0x10c, b);
        return;
    }
    let st = G_Entity_GetCurrentStatePtr(g, this);
    if g.m.r8(st + 0x32c) == 0 {
        // Approach one axis: v, target t, acceleration a.
        for (v, t, a) in [(0x10u32, 0x100u32, 0x108u32), (0x14, 0x104, 0x10c)] {
            let vel = g.m.rf32(this + v);
            let tgt = g.m.rf32(this + t);
            if vel < tgt {
                let n = g.m.rf32(this + v) as f64 + g.m.rf32(this + a) as f64;
                g.m.wf32(this + v, n as f32);
                let tgt = g.m.rf32(this + t);
                if g.m.rf32(this + v) > tgt {
                    g.m.wf32(this + v, tgt);
                }
            } else if vel > tgt {
                let n = g.m.rf32(this + v) as f64 + g.m.rf32(this + a) as f64;
                g.m.wf32(this + v, n as f32);
                let tgt = g.m.rf32(this + t);
                if g.m.rf32(this + v) < tgt {
                    g.m.wf32(this + v, tgt);
                }
            }
        }
    } else {
        let max = g.m.rf32(st + 0x452);
        let d = g.m.rf32(st + 0x456);
        let vx = g.m.rf32(this + 0x10);
        if vx < max {
            let n = g.m.rf32(this + 0x10) as f64 + d as f64;
            g.m.wf32(this + 0x10, n as f32);
            if n > max as f64 {
                g.m.wf32(this + 0x10, max);
            }
        } else if vx > max {
            let n = g.m.rf32(this + 0x10) as f64 - d as f64;
            g.m.wf32(this + 0x10, n as f32);
            if n < max as f64 {
                g.m.wf32(this + 0x10, max);
            }
        }
    }
}

/// `private: void __thiscall G_Entity::Priv_HoldToTarget(struct U_Utils_Vector *)` — 0x416c10.
/// Accelerates by the hold delta (`+0x44e`; away from the target on the
/// side the entity is on), velocity clamped to the hold max speed (`+0x44a`).
pub fn G_Entity_Priv_HoldToTarget(g: &mut G, this: u32, a0: u32) {
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let hm = g.m.rf32(st + 0x44a);
    let hd = g.m.rf32(st + 0x44e);
    let x = g.m.rf32(this);
    let tx = g.m.rf32(a0);
    g.m.wf32(this + 0x108, if x < tx { -hd } else { hd });
    let y = g.m.rf32(this + 4);
    let ty = g.m.rf32(a0 + 4);
    g.m.wf32(this + 0x10c, if y < ty { -hd } else { hd });
    for (v, a) in [(0x10u32, 0x108u32), (0x14, 0x10c)] {
        let sum = g.m.rf32(this + v) as f64 + g.m.rf32(this + a) as f64;
        g.m.wf32(this + v, sum as f32);
        if sum > hm as f64 {
            g.m.wf32(this + v, hm);
        } else if sum < -(hm as f64) {
            g.m.wf32(this + v, -hm);
        }
    }
}

/// `private: void __thiscall G_Entity::Priv_ReverseFromTarget(struct U_Utils_Vector *, float)` — 0x416cf0.
/// Sets the required velocity pointing away from the target: the offset to
/// it divided by `a1` (its distance; unless 0), times -max speed.
pub fn G_Entity_Priv_ReverseFromTarget(g: &mut G, this: u32, a0: u32, a1: f32) {
    let mut dx = g.m.rf32(a0) as f64 - g.m.rf32(this) as f64;
    let mut dy = g.m.rf32(a0 + 4) as f64 - g.m.rf32(this + 4) as f64;
    if !(a1 as f64 == 0.0) {
        dx /= a1 as f64;
        dy /= a1 as f64;
    }
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let max = g.m.rf32(st + 0x452) as f64;
    g.m.wf32(this + 0x100, (-(dx * max)) as f32);
    g.m.wf32(this + 0x104, (-(dy * max)) as f32);
}

/// `private: void __thiscall G_Entity::Priv_CheckSpawningAbilityAtStateChange(int)` — 0x416d70.
/// Rolls the run-time records for the new state's spawn sets.
pub fn G_Entity_Priv_CheckSpawningAbilityAtStateChange(g: &mut G, this: u32, a0: i32) {
    let idx = g.m.r32(this + 0x9e);
    let st = G_Entity_GetCurrentStatePtr(g, this);
    let mut count = 0;
    let rl = g.m.r32(this + 0x194 + idx.wrapping_mul(4));
    if rl != 0 {
        count = list::num(&g.m, rl);
    }
    g.m.w8(this + 0xb7, (count > 0) as u8);
    for i in 0..count {
        let idx = g.m.r32(this + 0x9e);
        let ri = list::nth(&g.m, g.m.r32(this + 0x194 + idx.wrapping_mul(4)), i);
        let ss = list::nth(&g.m, g.m.r32(st + 0x5d6), i);
        if g.m.r32(ss + 0x20) != NONE {
            let lo = g.m.ri32(ss + 0x2c);
            let hi = g.m.ri32(ss + 0x30);
            let r = g.random_int(lo, hi);
            g.m.wi32(ri, r);
            g.m.wi32(ri + 4, a0);
            let lo = g.m.ri32(ss + 0x34);
            let hi = g.m.ri32(ss + 0x38);
            let n = g.random_int(lo, hi);
            g.m.wi32(ri + 0xc, n);
            let on = g.m.ri32(ri) >= 0 && g.m.ri32(ri + 0xc) > 0;
            g.m.w8(ri + 0x14, on as u8);
            let n = g.m.r32(ri + 0xc);
            g.m.w32(ri + 8, n);
            let lo = g.m.ri32(ss + 0x3c);
            let hi = g.m.ri32(ss + 0x40);
            let d = g.random_int(lo, hi);
            g.m.wi32(ri + 0x10, d);
            let v = g.m.r32(ss + 0x4a);
            g.m.w32(this + 0xb8, v);
        } else {
            g.m.wi32(ri + 4, a0);
            g.m.w32(ri, 0);
            g.m.w8(ri + 0x14, 0);
            g.m.w32(this + 0xb8, 0);
        }
    }
}

/// `private: void __thiscall G_Entity::Priv_DeleteSpawnInfoList(class U_LinkedList *)` — 0x416ed0.
/// Frees every record, then the list itself (`~U_LinkedList` 0x401900 and
/// `operator delete` 0x457320).
pub fn G_Entity_Priv_DeleteSpawnInfoList(g: &mut G, this: u32, a0: u32) {
    if a0 == 0 {
        return;
    }
    loop {
        let obj = list::delete_first(&mut g.m, a0);
        if obj == 0 {
            break;
        }
        g.m.free(obj);
    }
    list::delete_all(&mut g.m, a0);
    g.m.free(a0);
}

/// `private: void __thiscall G_Entity::Priv_ChangeStateOnShieldDepletion(int)` — 0x416f30.
pub fn G_Entity_Priv_ChangeStateOnShieldDepletion(g: &mut G, this: u32, a0: i32) {
    change_to_flagged_state(g, this, 0x82a, a0);
}

/// `private: bool __thiscall G_Entity::Priv_CheckWithinRangeOfPlayers(int, struct U_Utils_Vector *, enum G_Game_PlayerNum *)` — 0x416fb0.
/// True when the closest active player (location into `a1`, number into
/// `a2`) is nearer than `a0`.
pub fn G_Entity_Priv_CheckWithinRangeOfPlayers(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32) -> bool {
    let sp = g.m.sp;
    let dist = g.m.alloca(4);
    let mut r = false;
    let found = G_Game_Player_GetClosestActive_41f550(g, this, a1, dist, a2);
    if found && a0 != 0 {
        if (a0 as f64) > g.m.rf32(dist) as f64 {
            r = true;
        }
    }
    g.m.pop_to(sp);
    r
}
