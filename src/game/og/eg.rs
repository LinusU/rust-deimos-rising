//! G_EntityGroup (0x417310..0x41d2e0): the lists of entity groups, spawning,
//! the per-tick `G_EG_Process`, reaping and the rule-condition queries.
//! Ported from the disassembly (docs/port-brief.md).
//!
//! Layout notes recovered while porting:
//!  * Globals: `[0x4e0d70]` active group list, `[0x4e0d6c]` required-groups
//!    list (map spawns by row), `[0x4e0d74]` notice-name list, `[0x4e0d78]`
//!    next entity unique number (starts 0x3e8), `[0x4e0d7c]` next group
//!    unique id (starts 0x1312d00), `[0x4e0d6b]` module initialised,
//!    `[0x4e0d80]` free pool slot hint, `[0x4e0d84]` entities in use, the
//!    entity pool at `0x4e0d88` (0x3e8 slots of 10 bytes: used byte, index
//!    dword, entity pointer dword), `[0x4e34a6]` live ground targets,
//!    `[0x4e3499]` "Reached Entity Limit" shown, `[0x4e34aa]` player-only
//!    spawns allowed, `[0x4e349a]` tracked entity id.
//!  * A group is a `G_GameObject` (0xb0 bytes) with: +0x8a unique id,
//!    +0x8e unit id ('PERM' for the permanent group), +0x92 x, +0x96 y,
//!    +0x9a spawn count, +0x9e live children, +0xa2 children disposed,
//!    +0xa6 child list, +0xaa/+0xae/+0xaf spawn parameters.
//!  * Entity (0x1e4 bytes): +0x8a unit def, +0x92 unique number, +0x96
//!    group id, +0x9e state index, +0xa2 "visible enough" byte, +0xa4
//!    activation delay, +0xb5 tracks-player flag, +0xbe/+0xbf collected /
//!    delete flags, +0xca player, +0xce destroyer, +0xd2 destroyed-by-
//!    weapon flag, +0x130 angle, +0x134 debris flag, +0x136 "has owned
//!    children", +0x138/+0x13c owner pointer and id, +0x140 pool index,
//!    +0x144.. per-state entry counts.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::bgnd::{G_Bgnd_GetUpdateRectTop, G_Bgnd_IsVerticalScrollingPaused};
use super::debris::{G_Debris_CheckCollision, G_Debris_New};
use super::entity::*;
use super::g::G;
use super::game::*;
use super::gameobject::*;
use super::mem::list;
use super::motionblur::G_MotionBlur_New;
use super::notice::G_Notice_Request;
use super::particle::G_Particle_NewGroup;
use super::player::*;
use super::resource::G_Res_GetPermFloat;
use super::umath;
use super::umath::ftol;
use super::unitdef::{GRND, NONE};
use super::unitdefs::G_UnitDef_MasterList_GetRefPtrForID;
use super::weaponhandler::G_WeaponHandler_Crosshair_Hilite;

// Globals.
const REQUIRED_LIST: u32 = 0x4e0d6c;
const INITIALISED: u32 = 0x4e0d6b;
const GROUP_LIST: u32 = 0x4e0d70;
const NOTICE_LIST: u32 = 0x4e0d74;
const NEXT_ENTITY_NUM: u32 = 0x4e0d78;
const NEXT_GROUP_ID: u32 = 0x4e0d7c;
const POOL_FREE_HINT: u32 = 0x4e0d80;
const POOL_IN_USE: u32 = 0x4e0d84;
const POOL_SLOTS: u32 = 0x4e0d88;
const LIMIT_SHOWN: u32 = 0x4e3499;
const POOL_READY: u32 = 0x4e3498;
const TRACKED_ID: u32 = 0x4e349a;
const GROUND_TARGETS: u32 = 0x4e34a6;
const PLAYER_ONLY_SPAWNS: u32 = 0x4e34aa;
/// `{link, object}` initialiser of a `U_LinkRef` (0, 0).
const START_REF: u32 = 0x4e0666;
/// The default `G_EG_SpawnRequest` (0x28 bytes): id 'none', f18 -1, scale 1.0.
const SPAWN_DEFAULT: u32 = 0x4e0b43;
const SPAWN_REQ_SIZE: u32 = 0x28;
const PERM: u32 = 0x5045524d;

// ---- small helpers ------------------------------------------------------

fn count(g: &G, l: u32) -> i32 {
    list::num(&g.m, l)
}

/// `GetNextLinkObject(&ref)`: advance the ref at `r`, return the object.
fn lnext(g: &mut G, l: u32, r: u32) -> u32 {
    list::next(&mut g.m, l, r);
    g.m.r32(r + 4)
}

/// A fresh `U_LinkRef` on the scratch stack, copied from the start ref.
fn start_ref(g: &mut G) -> u32 {
    let r = g.m.alloca(8);
    g.m.copy(r, START_REF, 8);
    r
}

fn f32of(g: &G, a: u32) -> f64 {
    g.m.rf32(a) as f64
}

/// `U_Sound_Play(settings, 1)` (0x44f5f0): the pitch is rolled first, then
/// the volume (whose bounds are equal, so it never draws).
fn u_sound_play(g: &mut G, settings: u32) {
    let id = g.m.r32(settings);
    if id != NONE {
        let lo = g.m.rf32(settings + 0x10);
        let hi = g.m.rf32(settings + 0x14);
        let pitch = g.random_float(lo, hi) as f32;
        let vmin = g.m.ri32(settings + 4);
        let volume = g.random_int(vmin, vmin);
        g.host.sound_play(id, volume, pitch);
    }
}

/// `strcmp(a, b) == 0` for two NUL-terminated strings in memory.
fn streq(g: &G, a: u32, b: u32) -> bool {
    g.m.strcmp_mem(a, b) == 0
}

/// `U_Sprite_ConvertScalePercentage` (0x40af60): the double on the x87.
fn scale_percentage(g: &G, pct: i32) -> f64 {
    pct as f64 / g.m.rf32(0x4dce98) as f64
}

/// Run `f(group, child)` over every child of every group, the way the many
/// query loops do (both counts read once up front). `f` returns true to
/// stop the whole scan.
fn scan(g: &mut G, mut f: impl FnMut(&mut G, u32, u32) -> bool) {
    let sp = g.m.sp;
    let gr = start_ref(g);
    let gl = g.m.r32(GROUP_LIST);
    let n = count(g, gl);
    let mut i = 0;
    'outer: while i < n {
        let grp = lnext(g, gl, gr);
        let cl = g.m.r32(grp + 0xa6);
        let m = count(g, cl);
        if m >= 1 {
            let cr = start_ref(g);
            let mut j = 0;
            while j < m {
                if grp == 0 {
                    break;
                }
                let c = lnext(g, cl, cr);
                if f(g, grp, c) {
                    break 'outer;
                }
                j += 1;
            }
        }
        i += 1;
    }
    g.m.pop_to(sp);
}

/// `G_Entity::G_Entity()` (0x413090).
fn g_entity_ctor(g: &mut G, e: u32) {
    G_GameObject_G_GameObject(g, e);
    g.m.w32(e + 0x8a, 0);
    g.m.w32(e + 0x8e, 0);
    for i in 0..0x14u32 {
        g.m.w32(e + 0x194 + i * 4, 0);
    }
    G_Entity_Reset(g, e);
}

// ---- level start / pool -------------------------------------------------

/// `sub_41d030` (0x41d030): allocate the entity pool.
pub fn sub_41d030(g: &mut G) {
    g.m.clear(POOL_FREE_HINT, 0x2718);
    g.m.w32(POOL_FREE_HINT, 0xffff_ffff);
    g.m.w32(POOL_IN_USE, 0);
    for i in 0..0x3e8u32 {
        let slot = POOL_SLOTS + i * 10;
        g.m.w8(slot, 0);
        g.m.w32(slot + 2, i);
        let e = g.m.alloc(0x1e4);
        g_entity_ctor(g, e);
        g.m.w32(slot + 6, e);
        g.m.w32(e + 0x140, i);
    }
}

/// The state part of `G_EG_Init` (0x417310): module flags and the pool. The
/// manager registration and console commands are not ported.
pub fn G_EG_Init(g: &mut G) {
    g.m.w8(INITIALISED, 1);
    g.m.w32(NEXT_ENTITY_NUM, 0x3e8);
    g.m.w32(NEXT_GROUP_ID, 0x1312d00);
    g.m.w8(LIMIT_SHOWN, 0);
    g.m.w32(TRACKED_ID, NONE);
    g.m.w32(0x4e349e, NONE);
    g.m.w8(0x4e34a2, 0);
    g.m.w8(0x4e34a3, 0);
    g.m.w8(0x4e34a4, 0);
    g.m.w8(0x4e34a5, 0);
    g.m.w32(GROUND_TARGETS, 0);
    if g.m.r8(POOL_READY) == 0 {
        sub_41d030(g);
        g.m.w8(POOL_READY, 1);
    }
}

/// `G_EntityGroup_sub_1d110` (0x41d110): release every pool slot.
pub fn G_EntityGroup_sub_1d110(g: &mut G) {
    g.m.w32(POOL_FREE_HINT, 0);
    g.m.w32(POOL_IN_USE, 0);
    for i in 0..0x3e8u32 {
        g.m.w8(POOL_SLOTS + i * 10, 0);
    }
}

/// `G_EntityGroup_sub_1d140` (0x41d140): destroy the entity pool.
pub fn G_EntityGroup_sub_1d140(g: &mut G) {
    for i in 0..0x3e8u32 {
        let slot = POOL_SLOTS + i * 10;
        g.m.w8(slot, 0);
        g.m.w32(slot + 2, 0xffff_ffff);
        let e = g.m.r32(slot + 6);
        if e != 0 {
            G_Entity_dtor_G_Entity(g, e);
            g.m.free(e);
            g.m.w32(slot + 6, 0);
        }
    }
    g.m.w8(POOL_READY, 0);
    g.m.w32(POOL_FREE_HINT, 0xffff_ffff);
    g.m.w32(POOL_IN_USE, 0);
}

/// `sub_41d1d0` (0x41d1d0): take an entity from the pool (reset), or 0.
pub fn sub_41d1d0(g: &mut G) -> u32 {
    let mut e = 0;
    let used = g.m.ri32(POOL_IN_USE);
    if used >= 0x3e8 {
        // U_App_Log("... %d entities in use") — logging is not ported.
        return 0;
    }
    let mut slot_i: i32 = -1;
    if g.m.ri32(POOL_FREE_HINT) == -1 {
        let mut i = 0;
        while i < 0x3e8 {
            if g.m.r8(POOL_SLOTS + i * 10) == 0 {
                slot_i = i as i32;
                break;
            }
            i += 1;
        }
        // slot_i == -1: U_Assert_DataFailure("... free slot ...")
    } else {
        slot_i = g.m.ri32(POOL_FREE_HINT);
    }
    if slot_i != -1 {
        g.m.wi32(POOL_IN_USE, g.m.ri32(POOL_IN_USE) + 1);
        let slot = POOL_SLOTS + (slot_i as u32) * 10;
        g.m.w8(slot, 1);
        e = g.m.r32(slot + 6);
        G_Entity_Reset(g, e);
        g.m.wi32(e + 0x140, slot_i);
        g.m.w32(POOL_FREE_HINT, 0xffff_ffff);
    }
    e
}

/// `sub_41d2b0` (0x41d2b0): give an entity back to the pool.
pub fn sub_41d2b0(g: &mut G, e: u32) {
    let idx = g.m.r32(e + 0x140);
    g.m.w8(POOL_SLOTS + idx * 10, 0);
    g.m.wi32(POOL_IN_USE, g.m.ri32(POOL_IN_USE) - 1);
    g.m.w32(POOL_FREE_HINT, idx);
}

/// `G_EntityGroup_sub_1a4d0` (0x41a4d0): destroy the required-groups list.
pub fn G_EntityGroup_sub_1a4d0(g: &mut G) {
    if g.m.r8(INITIALISED) != 0 && g.m.r32(REQUIRED_LIST) != 0 {
        let sp = g.m.sp;
        let l = g.m.r32(REQUIRED_LIST);
        let n = count(g, l);
        if n > 0 {
            let r = start_ref(g);
            let mut i = 0;
            while i < n {
                let obj = lnext(g, l, r);
                let link = g.m.r32(r);
                list::delete_link_ref(&mut g.m, l, link, r);
                if obj != 0 {
                    G_GameObject_dtor_G_GameObject(g, obj);
                    g.m.free(obj);
                }
                i += 1;
            }
        }
        let l = g.m.r32(REQUIRED_LIST);
        if l != 0 {
            list::delete_all(&mut g.m, l);
            g.m.free(l);
        }
        g.m.w32(REQUIRED_LIST, 0);
        g.m.pop_to(sp);
    }
}

/// `sub_41a5b0` (0x41a5b0): build the required-groups list from the level's
/// object list (`inList`): one lightweight group per object, kept until its
/// row scrolls into view. Unit resources are loaded by the host.
pub fn sub_41a5b0(g: &mut G, in_list: u32) {
    G_EntityGroup_sub_1a4d0(g);
    let l = list::new(&mut g.m);
    g.m.w32(REQUIRED_LIST, l);
    let sp = g.m.sp;
    let n = count(g, in_list);
    if n >= 1 {
        let r = start_ref(g);
        let mut i = 0;
        while i < n {
            let obj = lnext(g, in_list, r);
            let id = g.m.r32(obj);
            if id != NONE {
                let def = G_UnitDef_MasterList_GetRefPtrForID(g, id as i32);
                if def != 0 {
                    // G_UnitDef_LoadResourcesForUnit(def): resource loading is
                    // not ported.
                    let grp = g.m.alloc(0xb0);
                    G_GameObject_G_GameObject(g, grp);
                    let rl = g.m.r32(REQUIRED_LIST);
                    list::add(&mut g.m, rl, grp);
                    g.m.w32(grp + 0x8e, id);
                    g.m.w32(grp + 0x8a, 0xffff_ffff);
                    g.m.w32(grp + 0xa6, 0);
                    g.m.w32(grp + 0x9a, 0);
                    g.m.w32(grp + 0x9e, 0);
                    g.m.w32(grp + 0xa2, 0);
                    let v = g.m.r32(obj + 0x10);
                    g.m.w32(grp + 0xaa, v);
                    let v = g.m.r8(obj + 0x14);
                    g.m.w8(grp + 0xae, v);
                    let v = g.m.r8(obj + 0x15);
                    g.m.w8(grp + 0xaf, v);
                    let v = g.m.ri32(obj + 8);
                    g.m.wf32(grp + 0x92, (v as f64) as f32);
                    let v = g.m.ri32(obj + 0xc);
                    g.m.wf32(grp + 0x96, (v as f64) as f32);
                    if g.m.r32(def + 8) == GRND {
                        let v = f32of(g, grp + 0x92) - f32of(g, 0x4e34b8);
                        g.m.wf32(grp + 0x92, v as f32);
                    }
                }
                // else: U_App_Log("An invalid Unit ID ... was found in a Level Object")
            }
            i += 1;
        }
    }
    g.m.pop_to(sp);
}

/// `G_EntityGroup_sub_1a7c0` (0x41a7c0): destroy the active group list.
pub fn G_EntityGroup_sub_1a7c0(g: &mut G) {
    if g.m.r8(INITIALISED) != 0 && g.m.r32(GROUP_LIST) != 0 {
        let sp = g.m.sp;
        let r = start_ref(g);
        let l = g.m.r32(GROUP_LIST);
        let n = count(g, l);
        let mut i = 0;
        while i < n {
            let grp = lnext(g, l, r);
            let cl = g.m.r32(grp + 0xa6);
            while list::delete_first(&mut g.m, cl) != 0 {}
            if grp != 0 {
                G_GameObject_dtor_G_GameObject(g, grp);
                g.m.free(grp);
            }
            i += 1;
        }
        let l = g.m.r32(GROUP_LIST);
        if l != 0 {
            list::delete_all(&mut g.m, l);
            g.m.free(l);
        }
        g.m.w32(GROUP_LIST, 0);
        g.m.pop_to(sp);
    }
}

/// `G_EG_ResetAtLevelStart` (0x417690).
pub fn G_EG_ResetAtLevelStart(g: &mut G, a0: u32) {
    g.m.w8(LIMIT_SHOWN, 0);
    G_EntityGroup_sub_1d110(g);
    G_EntityGroup_sub_1a7c0(g);
    let l = list::new(&mut g.m);
    g.m.w32(GROUP_LIST, l);
    if g.m.r32(NOTICE_LIST) != 0 {
        let sp = g.m.sp;
        let nl = g.m.r32(NOTICE_LIST);
        let r = start_ref(g);
        let mut i = 0;
        while i < count(g, nl) {
            let obj = lnext(g, nl, r);
            if obj == 0 {
                break;
            }
            g.m.free(obj);
            i += 1;
        }
        let nl = g.m.r32(NOTICE_LIST);
        if nl != 0 {
            list::delete_all(&mut g.m, nl);
            g.m.free(nl);
        }
        g.m.w32(NOTICE_LIST, 0);
        g.m.pop_to(sp);
    }
    let nl = list::new(&mut g.m);
    g.m.w32(NOTICE_LIST, nl);
    g.m.w32(NEXT_ENTITY_NUM, 0x3e8);
    g.m.w32(NEXT_GROUP_ID, 0x1312d00);
    g.m.w32(GROUND_TARGETS, 0);
    // The permanent group, which holds entities spawned without a group.
    let grp = g.m.alloc(0xb0);
    G_GameObject_G_GameObject(g, grp);
    let gl = g.m.r32(GROUP_LIST);
    list::add(&mut g.m, gl, grp);
    g.m.w32(grp + 0x8e, PERM);
    g.m.w32(grp + 0x9a, 0);
    g.m.w32(grp + 0x9e, 0);
    g.m.w32(grp + 0xa2, 0);
    let id = g.m.r32(NEXT_GROUP_ID);
    g.m.w32(grp + 0x8a, id);
    g.m.w32(NEXT_GROUP_ID, id.wrapping_add(1));
    let cl = list::new(&mut g.m);
    g.m.w32(grp + 0xa6, cl);
    g.m.w32(grp + 0xaa, 0);
    g.m.w8(grp + 0xae, 0);
    g.m.w8(grp + 0xaf, 0);
    sub_41a5b0(g, a0);
}

/// `G_EG_CheckForMapSpawnsAtRow` (0x417900): spawn the required groups whose
/// row has been reached.
pub fn G_EG_CheckForMapSpawnsAtRow(g: &mut G, a0: i32) {
    if a0 < 0 {
        return;
    }
    let rl = g.m.r32(REQUIRED_LIST);
    let n = count(g, rl);
    if n < 1 {
        return;
    }
    let sp = g.m.sp;
    let r = start_ref(g);
    let req = g.m.alloca(SPAWN_REQ_SIZE);
    let mut i = 0;
    while i < n {
        let e = lnext(g, rl, r);
        let row = ftol(f32of(g, e + 0x96));
        if a0 == row {
            g.m.copy(req, SPAWN_DEFAULT, SPAWN_REQ_SIZE);
            let v = g.m.r32(e + 0x8e);
            g.m.w32(req, v);
            let v = g.m.r32(e + 0x92);
            g.m.w32(req + 4, v);
            let v = g.m.r32(e + 0x96);
            g.m.w32(req + 8, v);
            g.m.w8(req + 0xc, 1);
            let v = g.m.r32(e + 0xaa);
            g.m.w32(req + 0x16, v);
            let v = g.m.r8(e + 0xae);
            g.m.w8(req + 0x1a, v);
            let v = g.m.r8(e + 0xaf);
            g.m.w8(req + 0x1b, v);
            G_EG_RequestSpawn(g, req, 0, 0);
            let link = g.m.r32(r);
            list::delete_link_ref(&mut g.m, rl, link, r);
            if e != 0 {
                G_GameObject_dtor_G_GameObject(g, e);
                g.m.free(e);
            }
        }
        i += 1;
    }
    g.m.pop_to(sp);
}

// ---- spawning -----------------------------------------------------------

/// `sub_41b650` (0x41b650): how many entities a spawn of `def` makes.
pub fn sub_41b650(g: &mut G, def: u32) -> i32 {
    let mut n = g.m.ri32(def + 0x190);
    let max = g.m.ri32(def + 0x194);
    if n < 1 {
        n = 1;
    }
    if n > max {
        n = max;
    }
    if n != max {
        n = g.random_int(n, max);
    }
    if n > 0 {
        let chance = g.m.ri32(def + 0x1bc);
        let total = n;
        let mut i = 0;
        while i < total {
            if chance != 100 {
                if chance == 0 {
                    n -= 1;
                } else {
                    let r = g.random_int(0, 100);
                    if r > chance {
                        n -= 1;
                    }
                }
            }
            i += 1;
        }
    }
    n
}

/// `sub_41b700` (0x41b700): is the `{entity, id}` reference at `r` still
/// valid (same unique number, not deleted)?
pub fn sub_41b700(g: &mut G, r: u32) -> bool {
    let e = g.m.r32(r);
    if e == 0 {
        return false;
    }
    g.m.r32(r + 4) == g.m.r32(e + 0x92) && g.m.r8(e + 0xbf) == 0
}

/// `sub_41b740` (0x41b740): the first live entity of unit `id`, or 0.
pub fn sub_41b740(g: &mut G, id: u32) -> u32 {
    let mut found = 0;
    scan(g, |g, grp, c| {
        let def = g.m.r32(c + 0x8a);
        if g.m.r32(def + 4) == id {
            found = c;
            return true;
        }
        false
    });
    found
}

/// `sub_41b820` (0x41b820): dispose every entity of unit `id` owned by
/// player `arg`.
pub fn sub_41b820(g: &mut G, id: u32, arg: i32, flag: u8) {
    scan(g, |g, grp, c| {
        let def = g.m.r32(c + 0x8a);
        if g.m.r32(def + 4) == id && g.m.ri32(c + 0xca) == arg {
            sub_41ae10(g, grp, c, flag, false);
        }
        false
    });
}

/// `G_EG_RequestSpawn` (0x417a90).
pub fn G_EG_RequestSpawn(g: &mut G, a0: u32, a1: u32, a2: u32) {
    let req = a0;
    g.trace(|g| {
        format!("S {} at=({:.2},{:.2})", String::from_utf8_lossy(g.m.bytes(req, 4)), g.m.rf32(req + 4), g.m.rf32(req + 8))
    });
    if g.m.r32(req) == NONE {
        // U_Assert_DataFailure("inRequestPtr->id not_eq kG_GameObject_ID_None")
    }
    let time = G_Game_GameTime_Get(g);
    let mut def = a2;
    if def == 0 {
        let id = g.m.r32(req);
        def = G_UnitDef_MasterList_GetRefPtrForID(g, id as i32);
    }
    if def == 0 {
        // U_App_Log("ERROR: Couldn't get the Unit for Entity Group '%s'")
        return;
    }
    let d16 = def + 0x16;
    let n = sub_41b650(g, def);
    if n <= 0 {
        return;
    }
    if g.m.r8(d16 + 0x112) != 0 {
        if g.m.r8(PLAYER_ONLY_SPAWNS) == 0 {
            return;
        }
        if !G_Game_Player_IsAnyActive(g) {
            return;
        }
        if G_Game_IsWaitingForLevelToEnd(g) {
            return;
        }
    }
    if g.m.r8(d16 + 0x100) != 0 {
        let id = g.m.r32(req);
        if sub_41b740(g, id) != 0 {
            return;
        }
    }
    if g.m.ri32(POOL_IN_USE).wrapping_add(n) > 0x3e8 {
        if g.m.r8(LIMIT_SHOWN) != 0 {
            return;
        }
        g.m.w8(LIMIT_SHOWN, 1);
        // G_Message_New("Reached Entity Limit", ...): UI only.
        return;
    }
    if g.m.r8(d16 + 0x101) != 0 && g.m.ri32(req + 0x12) != -1 {
        let id = g.m.r32(req);
        let p = g.m.ri32(req + 0x12);
        sub_41b820(g, id, p, 0);
    }
    if g.m.r32(TRACKED_ID) == g.m.r32(req) {
        sub_41c3b0(g, req);
    }
    // Reuse the permanent group for a lone entity spawned by something that
    // is not itself a group member?
    let owner = g.m.r32(req + 0x1c);
    let reuse = n == 1 && (owner == 0 || g.m.r32(owner + 0x96) == 0x1312d00);
    let grp;
    if reuse {
        let gl = g.m.r32(GROUP_LIST);
        grp = list::nth(&g.m, gl, 0);
        g.m.wi32(grp + 0x9a, n);
        let v = g.m.ri32(grp + 0x9e).wrapping_add(n);
        g.m.wi32(grp + 0x9e, v);
        if g.m.r32(grp + 0xa6) == 0 {
            let cl = list::new(&mut g.m);
            g.m.w32(grp + 0xa6, cl);
        }
    } else {
        grp = g.m.alloc(0xb0);
        G_GameObject_G_GameObject(g, grp);
        let gl = g.m.r32(GROUP_LIST);
        list::add(&mut g.m, gl, grp);
        let v = g.m.r32(req);
        g.m.w32(grp + 0x8e, v);
        g.m.wi32(grp + 0x9a, n);
        g.m.wi32(grp + 0x9e, n);
        g.m.w32(grp + 0xa2, 0);
        let v = g.m.r32(req + 0x16);
        g.m.w32(grp + 0xaa, v);
        let v = g.m.r8(req + 0x1a);
        g.m.w8(grp + 0xae, v);
        let v = g.m.r8(req + 0x1b);
        g.m.w8(grp + 0xaf, v);
        let cl = list::new(&mut g.m);
        g.m.w32(grp + 0xa6, cl);
        let id = g.m.r32(NEXT_GROUP_ID);
        g.m.w32(grp + 0x8a, id);
        g.m.w32(NEXT_GROUP_ID, id.wrapping_add(1));
    }
    let v = g.m.r32(req + 4);
    g.m.w32(grp + 0x92, v);
    if g.m.r8(req + 0xc) != 0 {
        let top = G_Bgnd_GetUpdateRectTop(g);
        let y = ftol(f32of(g, req + 8));
        let v = y.wrapping_sub(top);
        g.m.wf32(grp + 0x96, v as f32);
    } else {
        let v = g.m.r32(req + 8);
        g.m.w32(grp + 0x96, v);
    }
    let (arg4, arg5);
    if g.m.r8(req + 0xd) == 0 {
        arg4 = g.m.r8(d16 + 0x10c);
        arg5 = g.m.ri32(req + 0x16);
    } else {
        arg4 = 1;
        arg5 = g.m.ri32(req + 0xe);
    }
    sub_41a890(g, grp, def, req, time, arg4, arg5, a1);
    if g.m.r8(d16 + 0x30a) != 0 {
        sub_41cdf0(g, d16 + 0x30a, d16, time);
    }
}

/// `sub_41a890` (0x41a890): create the entities of a spawn. The first one
/// of a multi-entity group gets the caller's reference, the rest do not.
pub fn sub_41a890(g: &mut G, grp: u32, def: u32, req: u32, time: i32, arg4: u8, arg5: i32, rf: u32) {
    let sp = g.m.sp;
    let out = g.m.alloca(4);
    if g.m.ri32(grp + 0x9a) == 1 {
        sub_41a990(g, grp, def, req, time, out, arg4, arg5, rf);
        g.m.pop_to(sp);
        return;
    }
    let mut i = 0;
    while i < g.m.ri32(grp + 0x9a) {
        if rf != 0 && i == 0 {
            sub_41a990(g, grp, def, req, time, out, arg4, arg5, rf);
        } else {
            sub_41a990(g, grp, def, req, time, out, arg4, arg5, 0);
        }
        i += 1;
    }
    g.m.pop_to(sp);
}

/// `sub_01a990` (0x41a990): create one entity of group `grp`.
pub fn sub_41a990(
    g: &mut G,
    grp: u32,
    def: u32,
    req: u32,
    time: i32,
    out: u32,
    flag: u8,
    arg7: i32,
    rf: u32,
) {
    let mut arg7 = arg7;
    let e = sub_41d1d0(g);
    if e == 0 {
        // U_Assert_DataFailure("newEntityPtr")
        return;
    }
    let cl = g.m.r32(grp + 0xa6);
    list::add(&mut g.m, cl, e);
    G_Entity_SetUnitRef(g, e, def);
    let d16 = g.m.r32(e + 0x8a).wrapping_add(0x16);
    let v = g.m.r32(req + 0x1c);
    g.m.w32(e + 0x138, v);
    let v = g.m.r32(req + 0x20);
    g.m.w32(e + 0x13c, v);
    let v = g.m.r32(d16 + 0x2c6);
    g.m.w32(e + 0x4a, v);
    g.m.w32(e + 0x9e, 0xffff_ffff);
    let num = g.m.r32(NEXT_ENTITY_NUM);
    g.m.w32(e + 0x92, num);
    g.m.w32(NEXT_ENTITY_NUM, num.wrapping_add(1));
    let v = g.m.r32(grp + 0x8a);
    g.m.w32(e + 0x96, v);
    g.m.w8(e + 0xbf, 0);
    let v = g.m.r32(req + 0x12);
    g.m.w32(e + 0xca, v);
    g.m.w32(e + 0xce, 0xffff_ffff);
    g.m.wi32(e + 0xb0, time);
    g.m.w8(e + 0xb4, 0);
    g.m.w8(e + 0xbe, 0);
    g.m.w8(e + 0xc1, 0);
    let mut s = 0;
    loop {
        let d = g.m.r32(e + 0x8a);
        if s >= g.m.ri32(d + 0x12) {
            break;
        }
        if g.m.r8(d.wrapping_add((s as u32).wrapping_mul(0x5da)).wrapping_add(0x82a)) != 0 {
            g.m.w8(e + 0xc1, 1);
            break;
        }
        s += 1;
    }
    let v = g.m.ri32(d16 + 0x19a) as f64;
    let mut dl: u8 = 1;
    let hundred = g.m.rf64(0x4e34b0);
    if v != hundred {
        let mut cl_ = false;
        if v < hundred && g.m.r8(d16 + 0x109) != 0 {
            cl_ = true;
        }
        if !cl_ {
            dl = 0;
        }
    }
    g.m.w8(e + 0xa2, dl);
    let v = g.m.r32(0x4e0640);
    g.m.w32(e + 0x108, v);
    let v = g.m.r32(0x4e0644);
    g.m.w32(e + 0x10c, v);
    let v = g.m.r32(d16 + 0x422);
    g.m.w32(e + 0x12c, v);
    if f32of(g, d16 + 0x426) > g.m.rf64(0x4e34c0) {
        let lvl = G_Game_GetCurrentLevelNumber(g);
        let v = (lvl.wrapping_sub(1)) as f64 * f32of(g, d16 + 0x426) + f32of(g, e + 0x12c);
        g.m.wf32(e + 0x12c, v as f32);
        let lim = f32of(g, d16 + 0x42a);
        if f32of(g, e + 0x12c) > lim {
            g.m.wf32(e + 0x12c, lim as f32);
        }
    }
    let angle;
    if flag == 0 {
        angle = g.m.ri32(d16 + 0x18a);
    } else {
        let spread = g.m.ri32(d16 + 0x18e);
        if spread != 0 {
            let half = spread / 2;
            let r = g.random_int(-half, half);
            arg7 = arg7.wrapping_add(r);
            if arg7 < 0 {
                arg7 += 0x168;
            } else if arg7 > 0x167 {
                arg7 -= 0x168;
            }
            if arg7 < 0 || arg7 > 0x167 {
                arg7 = 0;
            }
        }
        angle = arg7;
    }
    g.m.wi32(e + 0x130, angle);
    let v = g.m.r8(req + 0x1a);
    g.m.w8(e + 0x134, v);
    let v = g.m.r8(req + 0x1b);
    g.m.w8(e + 0x135, v);
    sub_41c540(g, grp, e);
    let owner = g.m.r32(req + 0x1c);
    let scale = g.m.ri32(req + 0x24);
    sub_41c840(g, grp, e, flag, arg7, owner as i32, scale);
    let sp = g.m.sp;
    let f1 = g.m.alloca(4);
    let name = g.m.r32(e + 0x8a).wrapping_add(0x96e);
    G_Entity_ChangeState(g, e, true, name, time, f1, f1 + 1);
    g.m.pop_to(sp);
    let st = G_Entity_GetCurrentStatePtr(g, e);
    let is_air = g.m.r32(def + 8) == super::unitdef::AIR;
    g.m.w8(e + 0x19, is_air as u8);
    let v = g.m.r8(d16 + 0x114);
    g.m.w8(e + 0x1a, v);
    G_EG_CacheOwnerLocOffsetsAndAngles(g, e);
    let hi = g.m.ri32(d16 + 0x186);
    let lo = g.m.ri32(d16 + 0x182);
    if lo != hi {
        let r = g.random_int(lo, hi);
        let v = g.m.ri32(out).wrapping_add(r);
        g.m.wi32(out, v);
        g.m.wi32(e + 0xa4, v);
    } else {
        let v = g.m.ri32(out).wrapping_add(lo);
        g.m.wi32(out, v);
        g.m.wi32(e + 0xa4, v);
    }
    if g.m.r8(st + 0x346) != 0 {
        sub_41cbc0(g, e);
    }
    if g.m.r8(e + 0x134) != 0 && g.m.r8(d16 + 0x497) != 0 {
        let sp = g.m.sp;
        let rect = g.m.alloca(16);
        G_GameObject_GetBounds_424210(g, e, rect);
        G_Debris_New(g, rect);
        g.m.pop_to(sp);
    }
    if g.m.r8(st + 0x320) != 0 {
        if sub_41b700(g, e + 0x138) {
            let ow = g.m.r32(e + 0x138);
            if G_Entity_GetCurrentStatePtr(g, ow) != 0 {
                let a = G_Entity_GetAngleFromSpriteInfo(g, ow);
                let fr = G_Entity_GetFrameForAngle(g, e, a);
                let sprite = g.m.r32(e + 0x1c);
                let nfr = g.host.sprite_num_frames(sprite);
                if fr >= 0 && fr < nfr {
                    g.m.wi32(e + 0x20, fr);
                } else {
                    g.m.w32(e + 0x20, 0);
                }
                // e+0x4e = U_Sprite_GetPtrToSpriteFrame(sprite, frame): the
                // host owns the sprite data, so there is no pointer.
                g.m.w32(e + 0x4e, 0);
            }
        }
    }
    if g.m.r8(d16 + 0x11c) != 0 {
        G_Game_GroundAccuracy_AddTarget(g);
        let v = g.m.ri32(GROUND_TARGETS).wrapping_add(1);
        g.m.wi32(GROUND_TARGETS, v);
    }
    if rf != 0 {
        g.m.w32(rf, e);
        let v = g.m.r32(e + 0x92);
        g.m.w32(rf + 4, v);
    }
}

/// `sub_01c540` (0x41c540): the entity's start position inside its group's
/// spawn area.
pub fn sub_41c540(g: &mut G, grp: u32, e: u32) {
    let d16 = g.m.r32(e + 0x8a).wrapping_add(0x16);
    let a = g.m.rf32(d16 + 0x242);
    let b = g.m.rf32(d16 + 0x246);
    let c = g.m.rf32(d16 + 0x24a);
    let d = g.m.rf32(d16 + 0x24e);
    let gx = g.m.rf32(grp + 0x92);
    let gy = g.m.rf32(grp + 0x96);
    let px: f32;
    let py: f32;
    if a != b && c != d {
        // Random point on a circle: angle, then a radius.
        let ang = g.random_int(0, 0x167);
        let (vx, vy) = umath::vector_from_angle(ang);
        if g.m.r8(d16 + 0x115) != 0 {
            let t = b.abs();
            let r = g.random_float(g.m.rf32(0x4e34c8), t) as f32;
            px = (vx as f64 * r as f64 + gx as f64) as f32;
            py = (vy as f64 * r as f64 + gy as f64) as f32;
        } else {
            let bb = b.abs();
            px = (vx as f64 * bb as f64 + gx as f64) as f32;
            let dd = d.abs();
            py = (vy as f64 * dd as f64 + gy as f64) as f32;
        }
    } else {
        if a == b {
            px = (gx as f64 + a as f64) as f32;
        } else {
            let (lo, hi) = if a as f64 > b as f64 { (b, a) } else { (a, b) };
            let r = g.random_int(ftol(lo as f64), ftol(hi as f64));
            px = (r as f64 + gx as f64) as f32;
        }
        if c == d {
            py = (gy as f64 + c as f64) as f32;
        } else {
            let (lo, hi) = if c as f64 > d as f64 { (d, c) } else { (c, d) };
            let r = g.random_int(ftol(lo as f64), ftol(hi as f64));
            py = (r as f64 + gy as f64) as f32;
        }
    }
    let sp = g.m.sp;
    let pos = g.m.alloca(8);
    g.m.wf32(pos, px);
    g.m.wf32(pos + 4, py);
    G_GameObject_SetLoc_424120(g, e, pos);
    g.m.pop_to(sp);
}

/// `sub_01c840` (0x41c840): the entity's initial velocity. `a4` is the
/// owner (a `G_Entity*`), `a5` the speed scale as float bits.
pub fn sub_41c840(g: &mut G, grp: u32, e: u32, flag: u8, a3: i32, a4: i32, a5: i32) {
    let owner = a4 as u32;
    let scale = f32::from_bits(a5 as u32);
    if g.m.r8(e + 0x134) != 0 {
        for (off, src) in [(0x10, 0x4e0640), (0x14, 0x4e0644), (0x100, 0x4e0640), (0x104, 0x4e0644), (0x108, 0x4e0640), (0x10c, 0x4e0644)] {
            let v = g.m.r32(src);
            g.m.w32(e + off, v);
        }
        return;
    }
    let d16 = g.m.r32(e + 0x8a).wrapping_add(0x16);
    let lo = g.m.rf32(d16 + 0x252);
    let hi = g.m.rf32(d16 + 0x256);
    let speed = g.random_float(lo, hi) as f32;
    let sp = g.m.sp;
    if flag != 0 || g.m.r8(d16 + 0x10c) != 0 {
        let ang = umath::invert_angle(a3);
        let (vx, vy) = umath::vector_from_angle_and_speed(ang, speed);
        g.m.wf32(e + 0x10, vx);
        g.m.wf32(e + 0x14, vy);
    } else if g.m.r8(d16 + 0x107) != 0 {
        // Aimed at the closest player.
        let my = g.m.alloca(8);
        G_GameObject_GetLoc_4240e0(g, e, my);
        let refv = g.m.alloca(8);
        let outv = g.m.alloca(8);
        let w = G_Res_GetPermFloat(g, 0x36) as f64 / g.m.rf32(0x4e34ab) as f64;
        g.m.wf32(refv, w as f32);
        g.m.w32(refv + 4, 0);
        if !G_Game_Player_GetClosestActive_41f6d0(g, refv, outv) {
            let v = g.m.r32(refv);
            g.m.w32(outv, v);
            g.m.w32(outv + 4, 0xc2c80000);
        }
        let x = (f32of(g, outv) - f32of(g, my)) as f32;
        let y = (f32of(g, outv + 4) - f32of(g, my + 4)) as f32;
        let (ux, uy) = umath::unit_vector(x, y);
        g.m.wf32(e + 0x10, (ux as f64 * speed as f64) as f32);
        g.m.wf32(e + 0x14, (uy as f64 * speed as f64) as f32);
    } else if g.m.r8(d16 + 0x10a) != 0 || g.m.r8(d16 + 0x10b) != 0 {
        let cur = g.m.alloca(8);
        G_GameObject_GetLoc_4240e0(g, e, cur);
        let (dx, dy);
        if g.m.r8(d16 + 0x10a) != 0 {
            dx = (f32of(g, cur) - f32of(g, grp + 0x92)) as f32;
            dy = (f32of(g, cur + 4) - f32of(g, grp + 0x96)) as f32;
        } else {
            dx = (f32of(g, grp + 0x92) - f32of(g, cur)) as f32;
            dy = (f32of(g, grp + 0x96) - f32of(g, cur + 4)) as f32;
        }
        let (ux, uy) = umath::unit_vector(dx, dy);
        g.m.wf32(e + 0x10, (ux as f64 * speed as f64) as f32);
        let vy = -((uy as f64 * speed as f64) as f32);
        g.m.wf32(e + 0x14, vy);
        let ang = umath::angle_from_vector(ux, uy);
        g.m.wi32(e + 0x130, umath::invert_angle(ang));
    } else {
        let mut spread = false;
        let mut ang;
        if g.m.r8(d16 + 0x111) == 0 {
            ang = g.m.ri32(d16 + 0x18a);
            spread = true;
        } else if owner != 0 {
            ang = G_Entity_GetAngleFromSpriteInfo(g, owner);
        } else {
            ang = g.m.ri32(d16 + 0x18a);
            spread = true;
        }
        if spread {
            let s = g.m.ri32(d16 + 0x18e);
            if s != 0 {
                let half = s / 2;
                let r = g.random_int(-half, half);
                ang = ang.wrapping_add(r);
                if ang < 0 {
                    ang += 0x168;
                } else if ang > 0x167 {
                    ang -= 0x168;
                }
                if ang < 0 || ang > 0x167 {
                    ang = 0;
                }
            }
        }
        g.m.wi32(e + 0x130, ang);
        let inv = umath::invert_angle(ang);
        let (vx, vy) = umath::vector_from_angle_and_speed(inv, speed);
        g.m.wf32(e + 0x10, vx);
        g.m.wf32(e + 0x14, vy);
    }
    g.m.pop_to(sp);
    if scale as f64 != 1.0 {
        let vx = (g.m.rf32(e + 0x10) as f64 * scale as f64) as f32;
        g.m.wf32(e + 0x10, vx);
        let vy = (g.m.rf32(e + 0x14) as f64 * scale as f64) as f32;
        g.m.wf32(e + 0x14, vy);
    }
    let vx = g.m.r32(e + 0x10);
    let vy = g.m.r32(e + 0x14);
    g.m.w32(e + 0xf8, vx);
    g.m.w32(e + 0xfc, vy);
    g.m.w32(e + 0x100, vx);
    g.m.w32(e + 0x104, vy);
    g.m.w32(e + 0x108, 0);
    g.m.w32(e + 0x10c, 0);
}

/// `sub_01cbc0` (0x41cbc0): a random drift velocity for the entity.
pub fn sub_41cbc0(g: &mut G, e: u32) {
    let r = g.random_int(0, 1);
    let mul: f32 = if r != 0 { 1.0 } else { f32::from_bits(0x3fb33333) };
    let ra = g.random_int(1, 4);
    let mut a = ra as f32;
    let rb = g.random_int(1, 100);
    let b = (rb as f64 / g.m.rf32(0x4e34cc) as f64) as f32;
    a = (a as f64 + b as f64) as f32;
    if g.random_int(0, 1) != 0 {
        a = -a;
    }
    let rc = g.random_int(1, 4);
    let mut c = (rc as f64 + b as f64) as f32;
    let sp = g.m.sp;
    let px = g.m.alloca(8);
    G_GameObject_GetLoc_424100(g, e, px, px + 4);
    let ly = g.m.rf32(px + 4);
    g.m.pop_to(sp);
    let t = G_Res_GetPermFloat(g, 0x37);
    let half = ftol(t as f64) / 4;
    if (half as f64) < ly as f64 {
        if g.random_int(0, 1) != 0 {
            c = -c;
        }
    }
    let sum = c as f64 * c as f64 + a as f64 * a as f64;
    let len = umath::sqrt(ftol(sum));
    a = (a as f64 / len as f64) as f32;
    c = (c as f64 / len as f64) as f32;
    let vx = (a as f64 * mul as f64) as f32;
    let vy = (c as f64 * mul as f64) as f32;
    g.m.wf32(e + 0x10, vx);
    g.m.wf32(e + 0x14, vy);
    g.m.wf32(e + 0x100, vx);
    g.m.wf32(e + 0x104, vy);
    g.m.wf32(e + 0xf8, vx);
    g.m.wf32(e + 0xfc, vy);
    let k = g.m.rf64(0x4e34d0);
    g.m.wf32(e + 0x108, (a as f64 * k) as f32);
    g.m.wf32(e + 0x10c, (c as f64 * k) as f32);
}

/// `sub_41cdf0` (0x41cdf0): request the one-time notice named at `name`
/// (a spawn-definition string at def+0x30a) when the unit spawns.
pub fn sub_41cdf0(g: &mut G, name: u32, base: u32, time: i32) {
    if !streq(g, name, 0x4e066e) {
        let mut go = false;
        if g.m.r8(base + 0x108) != 0 {
            if !sub_41cf10(g, name) {
                sub_41cfa0(g, name);
                go = true;
            }
        } else {
            go = true;
        }
        if go {
            let sp = g.m.sp;
            let s = g.m.alloca(0x26);
            g.m.copy(s, 0x4e0d37, 0x26);
            g.m.w32(s, name);
            g.m.w8(s + 4, 0);
            g.m.w8(s + 5, 1);
            let v = g.m.r32(base + 0x19e);
            g.m.w32(s + 6, v);
            let mut o = 0x40a;
            let mut d = 0xa;
            while o <= 0x41e {
                let v = g.m.r32(base + o);
                g.m.w32(s + d, v);
                o += 4;
                d += 4;
            }
            G_Notice_Request(g, s, time);
            g.m.pop_to(sp);
        }
    }
}

/// `sub_41cf10` (0x41cf10): is `name` in the notice-name list?
pub fn sub_41cf10(g: &mut G, name: u32) -> bool {
    let nl = g.m.r32(NOTICE_LIST);
    let n = count(g, nl);
    if n == 0 {
        return false;
    }
    let sp = g.m.sp;
    let r = start_ref(g);
    let mut found = false;
    let mut i = 0;
    while i < n {
        let o = lnext(g, nl, r);
        if streq(g, o, name) {
            found = true;
            break;
        }
        i += 1;
    }
    g.m.pop_to(sp);
    found
}

/// `sub_41cfa0` (0x41cfa0): add a copy of `name` to the notice-name list.
pub fn sub_41cfa0(g: &mut G, name: u32) {
    if name != 0 && g.m.r8(name) != 0 {
        let copy = g.m.alloc(0x40);
        // U_Utils_String_Safe_strncpy(copy, name, 0x3f)
        let s: Vec<u8> = g.m.cstr(name).iter().copied().take(0x3f).collect();
        g.m.write(copy, &s);
        g.m.w8(copy + 0x3f, 0);
        let nl = g.m.r32(NOTICE_LIST);
        list::add(&mut g.m, nl, copy);
    }
}

/// `sub_41c3b0` (0x41c3b0): the TRACKENT console notice for a spawn
/// (log lines and a message box only).
pub fn sub_41c3b0(g: &mut G, req: u32) {}

// ---- owner links --------------------------------------------------------

/// `G_EG_CacheOwnerLocOffsetsAndAngles` (0x417f00).
pub fn G_EG_CacheOwnerLocOffsetsAndAngles(g: &mut G, e: u32) {
    let v = g.m.r32(0x4e0640);
    g.m.w32(e + 0x11c, v);
    let v = g.m.r32(0x4e0644);
    g.m.w32(e + 0x120, v);
    let v = g.m.r32(0x4e0640);
    g.m.w32(e + 0x124, v);
    let v = g.m.r32(0x4e0644);
    g.m.w32(e + 0x128, v);
    let st = G_Entity_GetCurrentStatePtr(g, e);
    if g.m.r8(st + 0x32c) != 0 || g.m.r8(st + 0x32a) != 0 || g.m.r8(st + 0x32b) != 0 {
        let sp = g.m.sp;
        let ow = g.m.alloca(8);
        let mut got = false;
        if sub_41b700(g, e + 0x138) {
            let o = g.m.r32(e + 0x138);
            G_GameObject_GetLoc_4240e0(g, o, ow);
            got = true;
        } else if g.m.ri32(e + 0xca) != -1 {
            let p = g.m.ri32(e + 0xca);
            got = G_Game_Player_GetActiveLoc(g, p, ow);
        }
        if got {
            let v = g.m.r32(ow);
            g.m.w32(e + 0x124, v);
            let v = g.m.r32(ow + 4);
            g.m.w32(e + 0x128, v);
            let me = g.m.alloca(8);
            G_GameObject_GetLoc_4240e0(g, e, me);
            let (ox, oy) = (g.m.rf32(ow), g.m.rf32(ow + 4));
            let (sx, sy) = (g.m.rf32(me), g.m.rf32(me + 4));
            let set_offsets = |g: &mut G| {
                let fx = if ox < sx { sx as f64 - ox as f64 } else { -(ox as f64 - sx as f64) };
                g.m.wf32(e + 0x11c, fx as f32);
                let fy = if oy < sy { sy as f64 - oy as f64 } else { -(oy as f64 - sy as f64) };
                g.m.wf32(e + 0x120, fy as f32);
            };
            if g.m.r8(st + 0x32c) != 0 {
                set_offsets(g);
                let d = umath::distance_to_target((ox, oy), (sx, sy));
                g.m.wf32(e + 0xd4, ftol(d as f64) as f32);
                let ang = umath::intercept_angle(
                    ftol(ox as f64),
                    ftol(oy as f64),
                    ftol(sx as f64),
                    ftol(sy as f64),
                );
                g.m.wi32(e + 0xd8, umath::invert_angle(ang));
            }
            if g.m.r8(st + 0x32a) != 0 {
                set_offsets(g);
            }
        }
        g.m.pop_to(sp);
    }
}

/// The owner (or player) location shared by `sub_41bd90`, `sub_41be80` and
/// `sub_01bf70`: into `loc`, true when found.
fn owner_loc(g: &mut G, e: u32, loc: u32) -> bool {
    let mut got = false;
    let o = g.m.r32(e + 0x138);
    let valid = o != 0 && g.m.r32(e + 0x13c) == g.m.r32(o + 0x92) && g.m.r8(o + 0xbf) == 0;
    if valid {
        G_GameObject_GetLoc_4240e0(g, o, loc);
        got = true;
    }
    if !got && g.m.ri32(e + 0xca) != -1 {
        let p = g.m.ri32(e + 0xca);
        got = G_Game_Player_GetActiveLoc(g, p, loc);
    }
    got
}

/// `sub_41bd90` (0x41bd90): sit at the owner's location plus the cached offset.
pub fn sub_41bd90(g: &mut G, e: u32) {
    let sp = g.m.sp;
    let loc = g.m.alloca(8);
    let me = g.m.alloca(8);
    if owner_loc(g, e, loc) {
        G_GameObject_GetLoc_4240e0(g, e, me);
        let same = g.m.rf32(loc) == g.m.rf32(me) && g.m.rf32(loc + 4) == g.m.rf32(me + 4);
        if !same {
            let y = (g.m.rf32(loc + 4) as f64 + g.m.rf32(e + 0x120) as f64) as f32;
            let x = (g.m.rf32(loc) as f64 + g.m.rf32(e + 0x11c) as f64) as f32;
            G_GameObject_SetLoc_424140(g, e, x, y);
        }
    }
    g.m.pop_to(sp);
}

/// `sub_41be80` (0x41be80): follow the owner's movement since last tick.
pub fn sub_41be80(g: &mut G, e: u32) {
    let sp = g.m.sp;
    let loc = g.m.alloca(8);
    let me = g.m.alloca(8);
    if owner_loc(g, e, loc) {
        let dx = (g.m.rf32(e + 0x124) as f64 - g.m.rf32(loc) as f64) as f32;
        let dy = (g.m.rf32(e + 0x128) as f64 - g.m.rf32(loc + 4) as f64) as f32;
        G_GameObject_GetLoc_4240e0(g, e, me);
        let y = (g.m.rf32(me + 4) as f64 - dy as f64) as f32;
        let x = (g.m.rf32(me) as f64 - dx as f64) as f32;
        G_GameObject_SetLoc_424140(g, e, x, y);
        let v = g.m.r32(loc);
        g.m.w32(e + 0x124, v);
        let v = g.m.r32(loc + 4);
        g.m.w32(e + 0x128, v);
    }
    g.m.pop_to(sp);
}

/// `sub_01bf70` (0x41bf70): orbit the owner.
pub fn sub_41bf70(g: &mut G, e: u32) {
    let sp = g.m.sp;
    let loc = g.m.alloca(8);
    let me = g.m.alloca(8);
    if owner_loc(g, e, loc) {
        G_GameObject_GetLoc_4240e0(g, e, me);
        let same = g.m.rf32(loc) == g.m.rf32(me) && g.m.rf32(loc + 4) == g.m.rf32(me + 4);
        if !same {
            let mut fixed = true;
            if g.m.rf32(e + 0xd4) as f64 != g.m.rf64(0x4e34c0) {
                G_Entity_GetCurrentStatePtr(g, e);
                let step = ftol(g.m.rf32(e + 0x10) as f64);
                if step != 0 {
                    fixed = false;
                    let a = g.m.ri32(e + 0xd8).wrapping_add(step);
                    g.m.wi32(e + 0xd8, a);
                    if a > 0x167 {
                        g.m.wi32(e + 0xd8, a - 0x168);
                    } else if a < 0 {
                        g.m.wi32(e + 0xd8, a + 0x168);
                    }
                    let ang = g.m.ri32(e + 0xd8);
                    let spd = g.m.rf32(e + 0xd4);
                    let (vx, vy) = umath::vector_from_angle_and_speed(ang, spd);
                    let y = (g.m.rf32(loc + 4) as f64 + vy as f64) as f32;
                    let x = (g.m.rf32(loc) as f64 + vx as f64) as f32;
                    G_GameObject_SetLoc_424140(g, e, x, y);
                }
            }
            if fixed {
                let y = (g.m.rf32(loc + 4) as f64 + g.m.rf32(e + 0x120) as f64) as f32;
                let x = (g.m.rf32(loc) as f64 + g.m.rf32(e + 0x11c) as f64) as f32;
                G_GameObject_SetLoc_424140(g, e, x, y);
            }
            G_GameObject_GetLoc_4240e0(g, e, me);
            let (ox, oy) = (g.m.rf32(loc), g.m.rf32(loc + 4));
            let (sx, sy) = (g.m.rf32(me), g.m.rf32(me + 4));
            let fx = if ox < sx { sx as f64 - ox as f64 } else { -(ox as f64 - sx as f64) };
            g.m.wf32(e + 0x11c, fx as f32);
            let fy = if oy < sy { sy as f64 - oy as f64 } else { -(oy as f64 - sy as f64) };
            g.m.wf32(e + 0x120, fy as f32);
        }
    }
    g.m.pop_to(sp);
}

/// `sub_41b5d0` (0x41b5d0): copy the owner's look (tint, scale, glow).
pub fn sub_41b5d0(g: &mut G, a: u32, b: u32, c: u8, d: u8, e: u8) {
    if c != 0 {
        let v = g.m.r32(b + 0x62);
        g.m.w32(a + 0x62, v);
    }
    if d != 0 {
        let v = g.m.r8(b + 0x34);
        g.m.w8(a + 0x34, v);
        for off in [0x7a, 0x7e, 0x82] {
            let v = g.m.r32(b + off);
            g.m.w32(a + off, v);
        }
        G_GameObject_CalculateDimensions(g, a);
    }
    if e != 0 {
        let v = g.m.r8(b + 0x6e);
        g.m.w8(a + 0x6e, v);
        let v = g.m.r32(b + 0x70);
        g.m.w32(a + 0x70, v);
        let v = g.m.r8(b + 0x6f);
        g.m.w8(a + 0x6f, v);
        let v = g.m.r32(b + 0x74);
        g.m.w32(a + 0x74, v);
        let v = g.m.r16(b + 0x78);
        g.m.w16(a + 0x78, v);
    }
}

/// `sub_41c1b0` (0x41c1b0): a pickup's effect on a player; true when the
/// entity should be destroyed.
pub fn sub_41c1b0(g: &mut G, player: u32, e: u32) -> bool {
    let mut result = true;
    let d16 = g.m.r32(e + 0x8a).wrapping_add(0x16);
    match g.m.r32(d16 + 0x4b6) {
        0x20726961 | 0x646e7267 => {
            // 'air ' / 'grnd'
            if G_Player_IsInvulnerable(g, player) {
                result = false;
            }
        }
        0x6e696f63 => {
            // 'coin'
            let amt = g.m.ri32(d16 + 0x4be);
            if amt != 0 {
                G_Player_Money_Change(g, player, amt);
                G_GameObject_Glow_Start(g, player, 0x7fff, 6, false);
            }
        }
        0x746c756d => G_Player_Multiplier_Advance(g, player), // 'mult'
        0x696c7865 => G_Player_Lives_Add(g, player, true),    // 'exli'
        0x65696873 => {
            // 'shie'
            let amt = g.m.ri32(d16 + 0x4be);
            G_Player_Shields_IncreasePercentage(g, player, (amt as f64) as f32);
        }
        _ => {} // 'spec' and anything else
    }
    result
}

// ---- disposing ----------------------------------------------------------

/// `sub_01ae10` (0x41ae10): remove `child` from `grp`: spawn its death
/// spawns, destroy it and mark it deleted. True when the group is now empty
/// and should itself be deleted.
pub fn sub_41ae10(g: &mut G, grp: u32, child: u32, a2: u8, a3: bool) -> bool {
    let mut group_done = false;
    let mut result = false;
    let d16 = g.m.r32(child + 0x8a).wrapping_add(0x16);
    if g.m.r8(child + 0x136) != 0 {
        if a2 != 0 && g.m.r8(d16 + 0x494) != 0 {
            sub_41b090(g, child);
        }
        if g.m.r8(d16 + 0x495) != 0 {
            sub_41b1b0(g, child);
        }
    }
    if a2 != 0 {
        let v = g.m.ri32(grp + 0xa2).wrapping_add(1);
        g.m.wi32(grp + 0xa2, v);
        if g.m.ri32(grp + 0xa2) == g.m.ri32(grp + 0x9a) {
            group_done = true;
        }
    }
    if a3 && a2 != 0 && g.m.r8(child + 0xbe) == 0 {
        let sp = g.m.sp;
        if g.m.r32(d16 + 0x48c) != NONE && g.m.ri32(d16 + 0x488) > 0 {
            let req = g.m.alloca(SPAWN_REQ_SIZE);
            g.m.copy(req, SPAWN_DEFAULT, SPAWN_REQ_SIZE);
            let v = g.m.r32(d16 + 0x48c);
            g.m.w32(req, v);
            G_GameObject_GetLoc_4240e0(g, child, req + 4);
            g.m.w32(req + 0x1c, child);
            let v = g.m.r32(child + 0x92);
            g.m.w32(req + 0x20, v);
            let n = g.m.ri32(d16 + 0x488);
            let mut i = 0;
            while i < n {
                G_EG_RequestSpawn(g, req, 0, 0);
                i += 1;
            }
        }
        if g.m.r32(grp + 0x8e) != PERM && group_done && g.m.r32(d16 + 0x490) != NONE {
            let req = g.m.alloca(SPAWN_REQ_SIZE);
            g.m.copy(req, SPAWN_DEFAULT, SPAWN_REQ_SIZE);
            let v = g.m.r32(d16 + 0x490);
            g.m.w32(req, v);
            G_GameObject_GetLoc_4240e0(g, child, req + 4);
            g.m.w32(req + 0x1c, child);
            let v = g.m.r32(child + 0x92);
            g.m.w32(req + 0x20, v);
            G_EG_RequestSpawn(g, req, 0, 0);
        }
        g.m.pop_to(sp);
    }
    if a2 != 0 {
        let t = G_Game_GameTime_Get(g);
        let who = g.m.ri32(child + 0xce);
        G_Entity_Destroy(g, child, who, t);
    }
    g.m.w8(child + 0xbf, 1);
    let v = g.m.ri32(grp + 0x9e).wrapping_sub(1);
    g.m.wi32(grp + 0x9e, v);
    if g.m.ri32(grp + 0x9e) <= 0 && g.m.r32(grp + 0x8e) != PERM {
        result = true;
    }
    result
}

/// `sub_41b090` (0x41b090): dispose the entities owned by `child` that die
/// with their owner (state flag +0x325).
pub fn sub_41b090(g: &mut G, child: u32) {
    scan(g, |g, grp, e| {
        if e != child && g.m.r32(e + 0x13c) == g.m.r32(child + 0x92) {
            let st = G_Entity_GetCurrentStatePtr(g, e);
            if g.m.r8(st + 0x325) != 0 {
                let destroyed = g.m.ri32(e + 0xce) != -1;
                sub_41ae10(g, grp, e, 1, destroyed);
            }
        }
        false
    });
}

/// `sub_41b1b0` (0x41b1b0): like `sub_41b090` for state flag +0x326, without
/// destroying.
pub fn sub_41b1b0(g: &mut G, child: u32) {
    scan(g, |g, grp, e| {
        if e != child && g.m.r32(e + 0x13c) == g.m.r32(child + 0x92) {
            let st = G_Entity_GetCurrentStatePtr(g, e);
            if g.m.r8(st + 0x326) != 0 {
                sub_41ae10(g, grp, e, 0, false);
            }
        }
        false
    });
}

/// `sub_01b2d0` (0x41b2d0): reap the entities flagged deleted, and the
/// groups that emptied.
pub fn sub_41b2d0(g: &mut G) {
    let sp = g.m.sp;
    let gl = g.m.r32(GROUP_LIST);
    let n = count(g, gl);
    if n >= 1 {
        let gr = start_ref(g);
        let cr = g.m.alloca(8);
        let req = g.m.alloca(SPAWN_REQ_SIZE);
        let mut gi = 0;
        while gi < n {
            let grp = lnext(g, gl, gr);
            g.m.copy(cr, START_REF, 8);
            let cl = g.m.r32(grp + 0xa6);
            let m = count(g, cl);
            if m >= 1 {
                let mut ci = 0;
                while ci < m {
                    if grp == 0 {
                        break;
                    }
                    let e = lnext(g, cl, cr);
                    let d16 = g.m.r32(e + 0x8a).wrapping_add(0x16);
                    if g.m.r8(e + 0xbf) == 0 {
                        ci += 1;
                        continue;
                    }
                    if g.m.r8(d16 + 0x11c) != 0 {
                        let v = g.m.ri32(GROUND_TARGETS).wrapping_sub(1);
                        g.m.wi32(GROUND_TARGETS, v);
                    }
                    if g.m.r8(d16 + 0x496) != 0 {
                        g.m.w8(e + 0x36, 1);
                        g.m.w8(e + 0x35, 0);
                        let v = g.m.r8(d16 + 0x106);
                        g.m.w8(e + 0x38, v);
                        G_GameObject_Draw(g, e);
                    }
                    if g.m.r8(e + 0xd2) != 0 {
                        let st = G_Entity_GetCurrentStatePtr(g, e);
                        if g.m.r8(st + 0x329) != 0 && sub_41b700(g, e + 0x138) {
                            let o = g.m.r32(e + 0x138);
                            if g.m.r8(o + 0xbf) == 0 {
                                let t = G_Game_GameTime_Get(g);
                                let who = g.m.ri32(e + 0xce);
                                G_Entity_Destroy(g, o, who, t);
                            }
                        }
                    }
                    if g.m.r32(d16 + 0x2c2) != NONE
                        && g.m.r8(e + 0xd2) == 0
                        && G_Entity_CanSpawnOnMedia(g, e)
                    {
                        g.m.copy(req, SPAWN_DEFAULT, SPAWN_REQ_SIZE);
                        let v = g.m.r32(d16 + 0x2c2);
                        g.m.w32(req, v);
                        G_GameObject_GetLoc_4240e0(g, e, req + 4);
                        let v = g.m.r32(e + 0xca);
                        g.m.w32(req + 0x12, v);
                        g.m.w32(req + 0x1c, e);
                        let v = g.m.r32(e + 0x92);
                        g.m.w32(req + 0x20, v);
                        G_EG_RequestSpawn(g, req, 0, 0);
                    }
                    let destroyed = g.m.ri32(e + 0xce) != -1;
                    let weapon = g.m.r8(e + 0xd2);
                    let done = sub_41ae10(g, grp, e, weapon, destroyed);
                    let link = g.m.r32(cr);
                    list::delete_link_ref(&mut g.m, cl, link, cr);
                    if e != 0 {
                        sub_41d2b0(g, e);
                    }
                    if done && g.m.r32(grp + 0x8e) != PERM {
                        let link = g.m.r32(gr);
                        list::delete_link_ref(&mut g.m, gl, link, gr);
                        if grp != 0 {
                            let cl2 = g.m.r32(grp + 0xa6);
                            if cl2 != 0 {
                                list::delete_all(&mut g.m, cl2);
                            }
                            G_GameObject_dtor_G_GameObject(g, grp);
                            g.m.free(grp);
                        }
                        break;
                    }
                    ci += 1;
                }
            }
            gi += 1;
        }
    }
    g.m.pop_to(sp);
}

/// `G_EG_DisposePlayersChildren` (0x4198d0).
pub fn G_EG_DisposePlayersChildren(g: &mut G, a0: i32) {
    if a0 == -1 {
        return;
    }
    scan(g, |g, grp, c| {
        if g.m.ri32(c + 0xca) == a0 {
            let st = G_Entity_GetCurrentStatePtr(g, c);
            if g.m.r8(st + 0x325) != 0 {
                sub_41ae10(g, grp, c, 1, false);
            } else if g.m.r8(st + 0x326) != 0 {
                sub_41ae10(g, grp, c, 0, false);
            }
        }
        false
    });
}

/// `G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum` (0x419a10).
pub fn G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum(g: &mut G, a0: i32, a1: i32) {
    scan(g, |g, grp, c| {
        if g.m.ri32(c + 0x92) == a1 {
            G_Entity_ChangeToWeaponPowerupReleaseState(g, c, a0);
            return true;
        }
        false
    });
}

/// `G_EG_DisposeByUniqueEntityNum` (0x419af0).
pub fn G_EG_DisposeByUniqueEntityNum(g: &mut G, a0: i32) {
    scan(g, |g, grp, c| {
        if g.m.ri32(c + 0x92) == a0 {
            sub_41ae10(g, grp, c, 0, false);
            return true;
        }
        false
    });
}

// ---- rule conditions ----------------------------------------------------

/// The distance test shared by the two entity rule conditions: true when
/// `limit == 0` or `limit >= distance(vec, entity)`.
fn within_limit(g: &mut G, vec: u32, c: u32, limit: i32) -> bool {
    if limit == 0 {
        return true;
    }
    let loc = G_GameObject_GetLoc_4240d0(g, c);
    let d = umath::distance_to_target(
        (g.m.rf32(vec), g.m.rf32(vec + 4)),
        (g.m.rf32(loc), g.m.rf32(loc + 4)),
    );
    (limit as f64) >= d as f64
}

/// `G_EG_RuleCondition_IsEntityTrackingPlayer` (0x419bf0).
pub fn G_EG_RuleCondition_IsEntityTrackingPlayer(g: &mut G, a0: i32, a1: u32, a2: i32) -> bool {
    let mut result = false;
    if a0 as u32 == NONE {
        return false;
    }
    scan(g, |g, grp, c| {
        let def = g.m.r32(c + 0x8a);
        if g.m.ri32(def + 4) != a0 {
            return false;
        }
        if g.m.ri32(c + 0xa4) > 0 {
            return false;
        }
        if g.m.r8(c + 0xb5) == 0 {
            return false;
        }
        if within_limit(g, a1, c, a2) {
            result = true;
            return true;
        }
        false
    });
    result
}

/// `G_EG_RuleCondition_IsEntityActive` (0x419d70).
pub fn G_EG_RuleCondition_IsEntityActive(g: &mut G, a0: i32, a1: u32, a2: i32) -> bool {
    let mut result = false;
    if a0 as u32 == NONE {
        return false;
    }
    scan(g, |g, grp, c| {
        let def = g.m.r32(c + 0x8a);
        if g.m.ri32(def + 4) != a0 {
            return false;
        }
        if g.m.ri32(c + 0xa4) > 0 {
            return false;
        }
        if within_limit(g, a1, c, a2) {
            result = true;
            return true;
        }
        false
    });
    result
}

/// `G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType` (0x419ef0).
pub fn G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType(g: &mut G, a0: i32) -> i32 {
    let mut result = 0;
    if a0 as u32 == NONE {
        return 0;
    }
    scan(g, |g, grp, c| {
        let def = g.m.r32(c + 0x8a);
        if g.m.ri32(def + 4) == a0 && g.m.ri32(c + 0xa4) <= 0 {
            result += 1;
        }
        false
    });
    result
}

/// `G_EG_RuleCondition_IsAnyDestroyableAirEntityActive` (0x419fe0).
pub fn G_EG_RuleCondition_IsAnyDestroyableAirEntityActive(g: &mut G) -> bool {
    let mut result = false;
    scan(g, |g, grp, c| {
        let def = g.m.r32(c + 0x8a);
        if g.m.r8(def + 0x131) != 0 {
            result = true;
            return true;
        }
        false
    });
    result
}

/// `G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive` (0x41a0c0).
pub fn G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive(g: &mut G) -> bool {
    let mut result = false;
    scan(g, |g, grp, c| {
        let def = g.m.r32(c + 0x8a);
        if g.m.r8(def + 0x132) != 0 && G_Entity_IsWithinGameArea(g, c) {
            result = true;
            return true;
        }
        false
    });
    result
}

// ---- entity-vs-entity collision ----------------------------------------

/// `sub_01b920` (0x41b920): `e` against every other collidable entity of
/// the same class; both are hit by the other's damage.
pub fn sub_41b920(g: &mut G, e: u32, time: i32) {
    let gl = g.m.r32(GROUP_LIST);
    let n = count(g, gl);
    if n < 1 {
        return;
    }
    let sp = g.m.sp;
    let d16 = g.m.r32(e + 0x8a).wrapping_add(0x16);
    let cur = G_Entity_GetCurrentStatePtr(g, e);
    let gr = start_ref(g);
    let b = g.m.alloca(16); // left, top, right, bottom of e
    let ob = g.m.alloca(16); // ... of the other entity
    G_GameObject_GetBounds_424330(g, e, b, b + 4, b + 8, b + 12);
    let (l, t, r, bt) = (g.m.ri32(b), g.m.ri32(b + 4), g.m.ri32(b + 8), g.m.ri32(b + 12));
    if g.m.r8(d16 + 0x103) != 0 && bt < 0 {
        g.m.pop_to(sp);
        return;
    }
    let my = g.m.alloca(8);
    G_GameObject_GetLoc_4240e0(g, e, my);
    let my_id = g.m.r32(e + 0x92);
    let flag_a = g.m.r8(d16 + 0x102);
    let flag_b = g.m.r8(d16 + 0x103);
    let my_class = g.m.r32(g.m.r32(e + 0x8a) + 8);
    let oloc = g.m.alloca(8);
    let mut gi = 0;
    'groups: while gi < n {
        let grp = lnext(g, gl, gr);
        let cl = g.m.r32(grp + 0xa6);
        let m = count(g, cl);
        if m >= 1 {
            let cr = start_ref(g);
            let mut ci = 0;
            while ci < m {
                if grp == 0 {
                    ci += 1;
                    continue;
                }
                let o = lnext(g, cl, cr);
                let ost = G_Entity_GetCurrentStatePtr(g, o);
                let odef = g.m.r32(o + 0x8a);
                let od16 = odef.wrapping_add(0x16);
                let skip = g.m.r8(ost + 0x343) == 0
                    || g.m.r8(o + 0xbf) != 0
                    || g.m.r8(o + 0xa2) == 0
                    || g.m.r32(o + 0x92) == my_id
                    || g.m.r32(odef + 8) != my_class
                    || (flag_a != 0 && g.m.r8(od16 + 0x102) != 0)
                    || (flag_a == 0 && g.m.r8(od16 + 0x102) == 0)
                    || g.m.ri32(o + 0xa4) > 0
                    || !(g.m.r8(od16 + 0x104) != 0 && (flag_b != 0 || g.m.r8(od16 + 0x103) != 0));
                if skip {
                    ci += 1;
                    continue;
                }
                G_GameObject_GetBounds_424330(g, o, ob, ob + 4, ob + 8, ob + 12);
                let (ol, ot, or_, obt) =
                    (g.m.ri32(ob), g.m.ri32(ob + 4), g.m.ri32(ob + 8), g.m.ri32(ob + 12));
                let off = g.m.r8(odef + 0x119) != 0 && obt < 0;
                if off || bt < ot || t > obt || r < ol || l > or_ {
                    ci += 1;
                    continue;
                }
                G_GameObject_GetLoc_4240e0(g, o, oloc);
                // Radii: half the height of each entity.
                let ra = (bt.wrapping_sub(t) / 2) as f32;
                let rb = (obt.wrapping_sub(ot) / 2) as f32;
                let hit = umath::circle_collision(
                    (g.m.rf32(my), g.m.rf32(my + 4)),
                    ra,
                    (g.m.rf32(oloc), g.m.rf32(oloc + 4)),
                    rb,
                );
                if hit {
                    // The other entity's damage hits e (or e's owner).
                    let mut done = false;
                    let dmg_o = f32::from_bits(g.m.r32(odef + 0x270));
                    if g.m.r8(cur + 0x327) != 0 {
                        let ow = g.m.r32(e + 0x138);
                        let valid = ow != 0
                            && g.m.r32(e + 0x13c) == g.m.r32(ow + 0x92)
                            && g.m.r8(ow + 0xbf) == 0;
                        if valid {
                            let p = g.m.ri32(o + 0xca);
                            G_Entity_Hit(g, ow, dmg_o, p, time);
                            done = true;
                        }
                    }
                    if !done {
                        let p = g.m.ri32(o + 0xca);
                        G_Entity_Hit(g, e, dmg_o, p, time);
                    }
                    // Then e's damage hits the other entity (the original
                    // tests e's owner again here).
                    let ost2 = G_Entity_GetCurrentStatePtr(g, o);
                    let mut done = false;
                    let dmg_e = f32::from_bits(g.m.r32(d16 + 0x25a));
                    if g.m.r8(ost2 + 0x327) != 0 {
                        let ow = g.m.r32(e + 0x138);
                        let valid = ow != 0
                            && g.m.r32(e + 0x13c) == g.m.r32(ow + 0x92)
                            && g.m.r8(ow + 0xbf) == 0;
                        if valid {
                            let p = g.m.ri32(e + 0xca);
                            G_Entity_Hit(g, ow, dmg_e, p, time);
                            done = true;
                        }
                    }
                    if !done {
                        let p = g.m.ri32(e + 0xca);
                        G_Entity_Hit(g, o, dmg_e, p, time);
                    }
                    if g.m.r8(e + 0xbf) != 0 {
                        break 'groups;
                    }
                }
                ci += 1;
            }
        }
        gi += 1;
    }
    g.m.pop_to(sp);
}

// ---- the tick -----------------------------------------------------------

struct Players {
    count: i32,
    active: [bool; 2],
    player: [u32; 2],
    radius: [f32; 2],
    hilited: [bool; 2],
    /// Per player: crosshair location (8 bytes each).
    cross: u32,
    /// Per player: player location (8 bytes each).
    loc: u32,
    /// Per player: player bounds (16 bytes each).
    rect: u32,
}

/// `G_EG_Process` (0x418220): one tick for every entity of every group.
/// True when an entity of the pass wanted the scrolling to stop (state flag
/// +0x342).
pub fn G_EG_Process(g: &mut G, a0: i32) -> bool {
    let sp = g.m.sp;
    let r = eg_process(g, a0);
    g.m.pop_to(sp);
    r
}

fn eg_process(g: &mut G, gt: i32) -> bool {
    let mut result = false;
    let gl = g.m.r32(GROUP_LIST);
    if count(g, gl) < 1 {
        return false;
    }
    let left_bound = ftol(G_Res_GetPermFloat(g, 0x36) as f64).wrapping_add(0x20);
    let top_bound = ftol(G_Res_GetPermFloat(g, 0x37) as f64);
    let mut pl = Players {
        count: 0,
        active: [false; 2],
        player: [0; 2],
        radius: [0.0; 2],
        hilited: [false; 2],
        cross: g.m.alloca(16),
        loc: g.m.alloca(16),
        rect: g.m.alloca(32),
    };
    for i in 0..2usize {
        let p = G_Game_Player_GetPtr(g, i as i32);
        if G_Player_IsState(g, p, 4) {
            pl.count += 1;
            pl.active[i] = true;
            pl.player[i] = p;
            let rc = pl.rect + 16 * i as u32;
            G_GameObject_GetBounds_424210(g, p, rc);
            G_GameObject_GetLoc_4240e0(g, p + 0x2c2, pl.cross + 8 * i as u32);
            pl.hilited[i] = g.m.r8(p + 0x34d) != 0;
            G_GameObject_GetLoc_4240e0(g, p, pl.loc + 8 * i as u32);
            let h = g.m.ri32(rc + 8).wrapping_sub(g.m.ri32(rc));
            pl.radius[i] = (h as f64 / g.m.rf32(0x4e34ab) as f64) as f32;
        } else {
            pl.active[i] = false;
        }
    }
    let gr = start_ref(g);
    let cr = g.m.alloca(8);
    let flags = g.m.alloca(4);
    let b = g.m.alloca(16);
    let mut gi = 0;
    while gi < count(g, gl) {
        let grp = lnext(g, gl, gr);
        g.m.copy(cr, START_REF, 8);
        let cl = g.m.r32(grp + 0xa6);
        if count(g, cl) == 0 {
            gi += 1;
            continue;
        }
        let mut ci = 0;
        while ci < count(g, g.m.r32(grp + 0xa6)) {
            if grp == 0 {
                break;
            }
            let cl = g.m.r32(grp + 0xa6);
            let e = lnext(g, cl, cr);
            process_child(g, gt, e, &mut pl, &mut result, flags, b, left_bound, top_bound);
            ci += 1;
        }
        gi += 1;
    }
    sub_41b2d0(g);
    result
}

/// Mark the entity deleted (`+0xbf`, `+0xce = -1`).
fn mark_deleted(g: &mut G, e: u32) {
    g.m.w8(e + 0xbf, 1);
    g.m.w32(e + 0xce, 0xffff_ffff);
}

/// The body of the per-entity loop of `G_EG_Process`.
fn process_child(
    g: &mut G,
    gt: i32,
    e: u32,
    pl: &mut Players,
    result: &mut bool,
    flags: u32,
    b: u32,
    left_bound: i32,
    top_bound: i32,
) {
    let flag_a = flags;
    let flag_b = flags + 1;
    let mut st = G_Entity_GetCurrentStatePtr(g, e);
    let d16 = g.m.r32(e + 0x8a).wrapping_add(0x16);
    let delay = g.m.ri32(e + 0xa4).wrapping_sub(1);
    g.m.wi32(e + 0xa4, delay);
    if delay > 0 {
        g.m.wi32(e + 0x9a, gt);
        return;
    }
    // Particles.
    let mut cl = false;
    if g.m.r32(st + 0x2d0) != NONE {
        if g.m.r8(st + 0x2d6) != 0 {
            let last = g.m.ri32(e + 0xe8);
            if last == 0 {
                cl = true;
            } else if !(gt < last.wrapping_add(g.m.ri32(st + 0x2d8))) {
                cl = true;
            }
        } else if g.m.ri32(e + 0xec) == 0 {
            cl = true;
        }
    }
    if cl {
        let cap = g.m.ri32(st + 0x2dc);
        if cap == 0 || g.m.ri32(e + 0xec) < cap {
            let sp = g.m.sp;
            let loc = g.m.alloca(8);
            let s = g.m.alloca(0x14);
            G_GameObject_GetLoc_4240e0(g, e, loc);
            let v = g.m.r32(loc);
            g.m.w32(s, v);
            let v = g.m.r32(loc + 4);
            g.m.w32(s + 4, v);
            let v = g.m.r16(st + 0x2d4);
            g.m.w16(s + 8, v);
            g.m.w32(s + 0xa, 0);
            let grnd = g.m.r32(g.m.r32(e + 0x8a) + 8) == GRND;
            g.m.w8(s + 0xe, grnd as u8);
            let v = g.m.r32(st + 0x2d0);
            g.m.w32(s + 0x10, v);
            G_Particle_NewGroup(g, s);
            g.m.pop_to(sp);
        }
        let v = g.m.ri32(e + 0xec).wrapping_add(1);
        g.m.wi32(e + 0xec, v);
        g.m.wi32(e + 0xe8, gt);
    }
    // The state's sound.
    let snd = g.m.r32(st);
    let mut play = false;
    if snd != NONE {
        if g.m.r8(st + 0x18) != 0 {
            let last = g.m.ri32(e + 0xdc);
            if last == 0 {
                play = true;
            } else if !(gt < last.wrapping_add(g.m.ri32(st + 0x1c))) {
                play = true;
            }
        } else {
            let si = g.m.r32(e + 0x9e);
            let entries = g.m.ri32(e + 0x144 + si.wrapping_mul(4));
            if entries == 1 {
                if g.m.ri32(e + 0xe0) == 0 {
                    play = true;
                }
            } else if g.m.r8(st + 0x1a) != 0 && g.m.ri32(e + 0xe0) == 0 {
                play = true;
            }
        }
    }
    if play {
        if g.m.r8(st + 0x19) != 0 && g.host.sound_is_playing(snd) {
            play = false;
        }
        if play {
            let cap = g.m.ri32(st + 0x20);
            if cap == 0 || g.m.ri32(e + 0xe0) < cap {
                u_sound_play(g, st);
            }
        }
        let v = g.m.ri32(e + 0xe0).wrapping_add(1);
        g.m.wi32(e + 0xe0, v);
        g.m.wi32(e + 0xdc, gt);
    }
    // The on-timer state change.
    g.m.w8(flag_a, 0);
    g.m.w8(flag_b, 0);
    if gt == g.m.ri32(e + 0x9a).wrapping_add(g.m.ri32(e + 0xac)) {
        let name = st + 0x4d6;
        if streq(g, name, 0x4e398b) {
            // "Delete"
            mark_deleted(g, e);
            return;
        }
        if streq(g, name, 0x4e3992) {
            // "Destroy"
            G_Entity_Destroy(g, e, -1, gt);
            return;
        }
        if g.m.r8(name) != 0 && !streq(g, name, 0x4e066e) {
            // not "none"
            G_Entity_ChangeState(g, e, false, name, gt, flag_a, flag_b);
            if g.m.r8(flag_a) != 0 {
                mark_deleted(g, e);
                return;
            }
            if g.m.r8(flag_b) != 0 {
                G_Entity_Destroy(g, e, -1, gt);
                return;
            }
            st = G_Entity_GetCurrentStatePtr(g, e);
        }
    }
    if g.m.r8(st + 0x342) != 0 {
        *result = true;
    }
    G_Entity_Animate(g, e, gt);
    if g.m.ri32(st + 0x24) > 0 {
        G_Entity_ProcessRules(g, e, gt, flag_a, flag_b);
        if g.m.r8(flag_a) != 0 {
            mark_deleted(g, e);
            return;
        }
        if g.m.r8(flag_b) != 0 {
            G_Entity_Destroy(g, e, -1, gt);
            return;
        }
        st = G_Entity_GetCurrentStatePtr(g, e);
    }
    // Appearance from the state.
    g.m.wf32(e + 0x66, g.m.ri32(st + 0x3be) as f32);
    g.m.wf32(e + 0x6a, g.m.ri32(st + 0x3c2) as f32);
    let v = g.m.r8(st + 0x349);
    g.m.w8(e + 0x52, v);
    g.m.wf32(e + 0x58, g.m.ri32(st + 0x3c6) as f32);
    g.m.wf32(e + 0x5c, g.m.ri32(st + 0x3ca) as f32);
    let v = g.m.r16(st + 0x32e);
    g.m.w16(e + 0x60, v);
    G_GameObject_AdjustVisibilityAndTinting(g, e);
    g.m.w8(e + 0xa2, 1);
    if (g.m.rf32(e + 0x62) as f64) < g.m.rf64(0x4e34b0) && g.m.r8(d16 + 0x109) == 0 {
        g.m.w8(e + 0xa2, 0);
    }
    let s1 = scale_percentage(g, g.m.ri32(st + 0x3b6));
    g.m.wf32(e + 0x7e, s1 as f32);
    let s2 = scale_percentage(g, g.m.ri32(st + 0x3ba));
    g.m.wf32(e + 0x82, s2 as f32);
    G_GameObject_DoScaling(g, e);
    G_GameObject_CalculateDimensions(g, e);
    G_GameObject_Glow_Process(g, e);
    if g.m.r8(st + 0x323) != 0 || g.m.r8(st + 0x324) != 0 || g.m.r8(st + 0x328) != 0 {
        if sub_41b700(g, e + 0x138) {
            let o = g.m.r32(e + 0x138);
            let (c, d, ee) = (g.m.r8(st + 0x323), g.m.r8(st + 0x324), g.m.r8(st + 0x328));
            sub_41b5d0(g, e, o, c, d, ee);
        }
    }
    if g.m.r8(st + 0x34e) != 0 && !G_Bgnd_IsVerticalScrollingPaused(g) {
        G_Entity_Destroy(g, e, -1, gt);
        return;
    }
    G_Entity_DoMovementAI(g, e, gt, flag_a, flag_b);
    if g.m.r8(flag_a) != 0 {
        mark_deleted(g, e);
        return;
    }
    if g.m.r8(flag_b) != 0 {
        G_Entity_Destroy(g, e, -1, gt);
        return;
    }
    if !G_GameObject_MoveAndCheckPosition(g, e, 0x80, true) {
        mark_deleted(g, e);
        return;
    }
    if g.m.r8(st + 0x32a) != 0 {
        sub_41bd90(g, e);
    }
    if g.m.r8(st + 0x32b) != 0 {
        sub_41be80(g, e);
    }
    if g.m.r8(st + 0x32c) != 0 {
        sub_41bf70(g, e);
    }
    G_Entity_SpawnControl(g, e, gt);
    if g.m.r8(e + 0xbf) != 0 {
        return;
    }
    G_GameObject_GetBounds_424330(g, e, b, b + 4, b + 8, b + 12);
    let (bl, bt_, br, bb) = (g.m.ri32(b), g.m.ri32(b + 4), g.m.ri32(b + 8), g.m.ri32(b + 12));
    // Collision with the players.
    if pl.count > 0
        && g.m.r8(st + 0x343) != 0
        && g.m.r8(d16 + 0x102) == 0
        && g.m.r8(st + 0x34b) != 0
        && br >= -0x20
        && bl <= left_bound
        && bb >= 0
        && bt_ <= top_bound
    {
        let sp = g.m.sp;
        let myloc = g.m.alloca(8);
        G_GameObject_GetLoc_4240e0(g, e, myloc);
        let mut i = 0;
        while i < 2usize {
            if g.m.r8(e + 0xbf) != 0 {
                break;
            }
            if pl.active[i] {
                let rc = pl.rect + 16 * i as u32;
                let (r0, r1, r2, r3) =
                    (g.m.ri32(rc), g.m.ri32(rc + 4), g.m.ri32(rc + 8), g.m.ri32(rc + 12));
                if bb >= r0 && bt_ <= r2 && br >= r1 && bl <= r3 {
                    let h = bb.wrapping_sub(bt_);
                    let h = h.wrapping_add((h < 0) as i32) >> 1;
                    let ploc = pl.loc + 8 * i as u32;
                    let hit = umath::circle_collision(
                        (g.m.rf32(ploc), g.m.rf32(ploc + 4)),
                        pl.radius[i],
                        (g.m.rf32(myloc), g.m.rf32(myloc + 4)),
                        h as f32,
                    );
                    if hit {
                        let p = pl.player[i];
                        if g.m.r32(d16 + 0x4b6) != NONE {
                            if sub_41c1b0(g, p, e) {
                                let num = G_Player_GetNumber(g, p);
                                G_Entity_Destroy(g, e, num, gt);
                                g.m.w8(e + 0xbe, 1);
                            }
                        } else {
                            let mut done = false;
                            if g.m.r8(st + 0x327) != 0 && sub_41b700(g, e + 0x138) {
                                let num = G_Player_GetNumber(g, p);
                                let dmg = G_Res_GetPermFloat(g, 0xa1);
                                let o = g.m.r32(e + 0x138);
                                G_Entity_Hit(g, o, dmg, num, gt);
                                done = true;
                            }
                            if !done {
                                let num = G_Player_GetNumber(g, p);
                                let dmg = G_Res_GetPermFloat(g, 0xa1);
                                G_Entity_Hit(g, e, dmg, num, gt);
                            }
                            let dmg = f32::from_bits(g.m.r32(d16 + 0x25a));
                            G_Player_Hit(g, p, dmg, gt);
                            if G_Player_GetState(g, p) != 4 {
                                pl.active[i] = false;
                                pl.count -= 1;
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        g.m.pop_to(sp);
    }
    if g.m.r8(e + 0xbf) != 0 {
        return;
    }
    // Motion blur trail.
    if g.m.r8(st + 0x2ea) != 0 && g.m.r32(e + 0x1c) != NONE {
        let lo = g.m.ri32(st + 0x2ec);
        let hi = g.m.ri32(st + 0x2f0);
        let r = g.random_int(lo, hi);
        if gt > g.m.ri32(e + 0xe4).wrapping_add(r) {
            g.m.wi32(e + 0xe4, gt);
            let sp = g.m.sp;
            let req = g.m.alloca(16);
            g.m.copy(req, 0x4e0d5d, 14);
            g.m.w32(req, e);
            let v = g.m.r32(st + 0x2f4);
            g.m.w32(req + 4, v);
            let v = g.m.r32(st + 0x2f8);
            g.m.w32(req + 8, v);
            let v = g.m.r8(st + 0x2eb);
            g.m.w8(req + 0xc, v);
            G_MotionBlur_New(g, req);
            g.m.pop_to(sp);
        }
    }
    // Crosshair highlight for ground targets.
    if g.m.r8(d16 + 0x102) == 0
        && g.m.r32(g.m.r32(e + 0x8a) + 8) == GRND
        && g.m.r8(d16 + 0x104) != 0
        && g.m.r8(st + 0x34a) != 0
    {
        let mut i = 0;
        while i < 2usize {
            if pl.active[i] && !pl.hilited[i] {
                let cx = g.m.rf32(pl.cross + 8 * i as u32) as f64;
                let cy = g.m.rf32(pl.cross + 8 * i as u32 + 4) as f64;
                if (bl as f64) <= cx && (br as f64) > cx && (bt_ as f64) <= cy && (bb as f64) > cy {
                    G_WeaponHandler_Crosshair_Hilite(g, pl.player[i] + 0x235, true);
                    pl.hilited[i] = true;
                }
            }
            i += 1;
        }
    }
    // Debris.
    if g.m.r8(e + 0x134) == 0 && g.m.r8(e + 0x19) == 0 && g.m.r8(d16 + 0x110) != 0 {
        let sp = g.m.sp;
        let rect = g.m.alloca(16);
        G_GameObject_GetBounds_424210(g, e, rect);
        if G_Debris_CheckCollision(g, rect) {
            let v = g.m.r32(0x4e0640);
            g.m.w32(e + 0x10, v);
            let v = g.m.r32(0x4e0644);
            g.m.w32(e + 0x14, v);
            g.m.w8(e + 0x134, 1);
            if g.m.r8(d16 + 0x497) != 0 {
                let rect2 = g.m.alloca(16);
                G_GameObject_GetBounds_424210(g, e, rect2);
                G_Debris_New(g, rect2);
            }
        }
        g.m.pop_to(sp);
    }
    if g.m.r8(e + 0xbf) == 0 && g.m.r8(st + 0x343) != 0 {
        sub_41b920(g, e, gt);
    }
}
