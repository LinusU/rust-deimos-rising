//! Port of the original's wepdefs module (G_WeaponDefinitions.cc,
//! 0x444d60..0x447000): the master list of weapon definitions (each a
//! 0x200-byte `G_WepDef`, see wepdef.rs) and the lookups over it. The
//! definitions themselves are put into memory by load.rs.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::mem::list;
use super::resource::{G_Res_Load, RES_TYPE_IMAGE, RES_TYPE_SOUND};
use super::unitdefs::G_UnitDef_LoadResourcesForUnitDefID;

// Globals.
/// `g_wepDefInitialised` (byte).
pub const INITIALISED: u32 = 0x4f943b;
/// `g_wepDefMasterList`: U_LinkedList of `G_WepDef *`.
pub const MASTER_LIST: u32 = 0x4f943c;
/// Offset of a definition's spawn-set list (0x32-byte blocks).
pub const SPAWN_LIST: u32 = 0x1c0;

const NONE: u32 = 0x656e6f6e;
/// Weapon types: 'PEAA', 'PEAG' (primary air and ground), 'SPEC'.
pub const TYPE_PEAA: i32 = 0x41414550;
pub const TYPE_PEAG: i32 = 0x47414550;
pub const TYPE_SPEC: i32 = 0x43455053;

/// `struct G_WepDef * __cdecl G_WepDef_MasterList_GetRefPtrForIndex(int)` — 0x445030, 128 bytes, decomp: complete.
pub fn G_WepDef_MasterList_GetRefPtrForIndex(g: &mut G, a0: i32) -> u32 {
    let l = g.m.r32(MASTER_LIST);
    // The walk finds the a0th link; out-of-range indices find none.
    list::nth(&g.m, l, a0)
}

/// `struct G_WepDef * __cdecl G_WepDef_MasterList_GetNextRefPtrByType(enum G_WepDef_Type, long, int)` — 0x4450b0, 192 bytes.
///
/// Walks the definitions of type `a0` whose level range contains `a2`
/// (+0x13c min, +0x140 max). With id 'none' the first such one is
/// returned; otherwise the one after the definition with id `a1`, or the
/// first such one when there is none after it.
pub fn G_WepDef_MasterList_GetNextRefPtrByType(g: &mut G, a0: i32, a1: i32, a2: i32) -> u32 {
    let l = g.m.r32(MASTER_LIST);
    let mut first = 0u32;
    let mut found_exact = false;
    for wep in list::objects(&g.m, l) {
        if g.m.ri32(wep + 8) != a0 {
            continue;
        }
        if a2 < g.m.ri32(wep + 0x13c) {
            continue;
        }
        if a2 > g.m.ri32(wep + 0x140) {
            continue;
        }
        if a1 as u32 == NONE {
            return wep;
        }
        if first == 0 {
            first = wep;
        }
        if a1 == g.m.ri32(wep + 4) {
            found_exact = true;
            continue;
        }
        if found_exact {
            return wep;
        }
    }
    first
}

/// `void __cdecl sub_0459a0(long type, int level)` — 0x4459a0: loads the
/// resources of every weapon of `type` available at `level`.
pub fn sub_4459a0(g: &mut G, a0: i32, a1: i32) {
    let l = g.m.r32(MASTER_LIST);
    for wep in list::objects(&g.m, l) {
        if g.m.ri32(wep + 8) == a0 && a1 >= g.m.ri32(wep + 0x13c) && a1 <= g.m.ri32(wep + 0x140) {
            sub_445a30(g, wep);
        }
    }
}

/// `G_WepDef_LoadResourcesForLevel` (0x4456b0).
pub fn G_WepDef_LoadResourcesForLevel(g: &mut G, level: i32) {
    sub_4459a0(g, TYPE_PEAA, level);
    sub_4459a0(g, TYPE_PEAG, level);
    sub_4459a0(g, TYPE_SPEC, level);
}

/// `void __cdecl sub_045a30(struct G_WepDef *)` — 0x445a30: loads a
/// weapon's selection sound and sprites, then the units it can spawn
/// (crosshair spawn, powerup spawns, spawn sets).
pub fn sub_445a30(g: &mut G, a0: u32) {
    let id = g.m.ri32(a0 + 0x14e);
    G_Res_Load(g, RES_TYPE_SOUND, id, 1);
    for off in [0x130, 0x166, 0x16e, 0x144, 0x148] {
        let id = g.m.ri32(a0 + off);
        G_Res_Load(g, RES_TYPE_IMAGE, id, 1);
    }
    for off in [0x17e, 0x1c8, 0x1d8, 0x1e6, 0x1f6] {
        let id = g.m.ri32(a0 + off);
        G_UnitDef_LoadResourcesForUnitDefID(g, id);
    }
    let sets = g.m.r32(a0 + SPAWN_LIST);
    if sets != 0 {
        let mut i = 0;
        while i < list::num(&g.m, sets) {
            let set = list::nth(&g.m, sets, i);
            let id = g.m.ri32(set + 0x20);
            G_UnitDef_LoadResourcesForUnitDefID(g, id);
            i += 1;
        }
    }
}

/// `G_WepDef_ShutDownInternal` (0x445870): disposes the master list, each
/// definition (those carrying the 0x499602d2 signature) with its spawn-set
/// list.
pub fn G_WepDef_ShutDownInternal(g: &mut G) {
    let master = g.m.r32(MASTER_LIST);
    if g.m.r8(INITIALISED) == 0 || master == 0 {
        return;
    }
    for wep in list::objects(&g.m, master) {
        if wep != 0 && g.m.r32(wep) == 0x499602d2 {
            let sets = g.m.r32(wep + SPAWN_LIST);
            if sets != 0 {
                loop {
                    let o = list::delete_first(&mut g.m, sets);
                    if o == 0 {
                        break;
                    }
                    g.m.free(o);
                }
                g.m.free(sets);
            }
            g.m.free(wep);
        }
    }
    list::delete_all(&mut g.m, master);
    g.m.free(master);
    g.m.w32(MASTER_LIST, 0);
}
