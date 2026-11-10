//! Port of the original's unitdefs module (G_UnitDefinitions.cc,
//! 0x43fe60..0x445000): the master list of unit definitions (each a
//! 0x79e0-byte `G_UnitDef`, see unitdef.rs), the family list built from
//! their family names, and the lookups over both. The definitions
//! themselves are put into memory by load.rs.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::mem::list;
use super::resource::{G_Res_Load, RES_TYPE_IMAGE, RES_TYPE_SOUND};

// Globals.
/// `g_unitDefInitialised` (byte).
pub const INITIALISED: u32 = 0x4f6569;
/// `g_unitDefMasterList`: U_LinkedList of `G_UnitDef *`.
pub const MASTER_LIST: u32 = 0x4f656a;
/// `g_unitDefFamilyList`: U_LinkedList of `G_UnitDef_Family *`.
pub const FAMILY_LIST: u32 = 0x4f656e;
/// `g_unitDefResourceLoadingList`: U_LinkedList of unit ids (4 bytes each).
pub const RES_LOADING_LIST: u32 = 0x4f6572;
/// Number of families (`sPriv_NumFamiliesInUnitList`).
pub const NUM_FAMILIES: u32 = 0x4f6576;
/// Number of units in the family lists (`sPriv_NumUnitsInFamilyList`).
pub const NUM_UNITS_IN_FAMILIES: u32 = 0x4f657a;
/// Pointer to the name of the family units without one go into (the exe
/// string " Misc").
pub const DEFAULT_FAMILY_NAME_PTR: u32 = 0x4f6565;
/// `sPriv_ResID` ('inte' string list id).
pub const RES_ID: u32 = 0x4f6053;

/// `G_UnitDef_Family` (0x4c bytes): name +0 (0x40), `U_LinkedList` of
/// `G_UnitDef *` (4-byte cells) embedded at +0x40.
pub const FAMILY_SIZE: u32 = 0x4c;
/// Offset of a state's spawn-set list inside a `G_UnitDef`
/// (`0x4d8 + state * 0x5da + 0x5d6`).
pub const STATE_BASE: u32 = 0x4d8;
pub const STATE_SIZE: u32 = 0x5da;
pub const STATE_SPAWN_LIST: u32 = 0x5d6;
pub const MAX_STATES: u32 = 0x14;

const NONE: u32 = 0x656e6f6e;

/// `struct G_UnitDef * __cdecl G_UnitDef_MasterList_GetRefPtrForIndex(int)`
/// — 0x4403c0.
pub fn G_UnitDef_MasterList_GetRefPtrForIndex(g: &mut G, index: i32) -> u32 {
    let l = g.m.r32(MASTER_LIST);
    if index < 0 {
        return 0;
    }
    list::nth(&g.m, l, index)
}

/// `struct G_UnitDef * __cdecl G_UnitDef_MasterList_GetRefPtrForID(long)` — 0x440340, 128 bytes, decomp: complete.
pub fn G_UnitDef_MasterList_GetRefPtrForID(g: &mut G, a0: i32) -> u32 {
    let l = g.m.r32(MASTER_LIST);
    for def in list::objects(&g.m, l) {
        if g.m.ri32(def + 4) == a0 {
            return def;
        }
    }
    0
}

/// `struct G_UnitDef_Family * __cdecl G_UnitDef_MasterList_GetFamilyPtrForUnitDef(struct G_UnitDef *)` — 0x440440, 224 bytes.
pub fn G_UnitDef_MasterList_GetFamilyPtrForUnitDef(g: &mut G, a0: u32) -> u32 {
    let mut name = a0 + 0x56;
    if g.m.r8(name) == 0 {
        name = g.m.r32(DEFAULT_FAMILY_NAME_PTR);
    }
    let l = g.m.r32(FAMILY_LIST);
    for fam in list::objects(&g.m, l) {
        if fam != 0 && g.m.strcmp_mem(name, fam) == 0 {
            return fam;
        }
    }
    sub_441dc0(g);
    // The original logs "Couldn't find a Family for a Unit Definition".
    0
}

/// `struct G_UnitDef * __cdecl G_UnitDef_MasterList_GetRefPtrForID_ByFamily(long, struct G_UnitDef_Family *)` — 0x440520, 192 bytes.
pub fn G_UnitDef_MasterList_GetRefPtrForID_ByFamily(g: &mut G, a0: i32, a1: u32) -> u32 {
    for cell in list::objects(&g.m, a1 + 0x40) {
        let def = g.m.r32(cell);
        if g.m.ri32(def + 4) == a0 {
            return def;
        }
    }
    let l = g.m.r32(MASTER_LIST);
    for def in list::objects(&g.m, l) {
        if g.m.ri32(def + 4) == a0 {
            return def;
        }
    }
    0
}

/// `void sub_441870(void)` — 0x441870: disposes the master list (every
/// definition with its spawn-set lists).
pub fn sub_441870(g: &mut G) {
    let master = g.m.r32(MASTER_LIST);
    if g.m.r8(INITIALISED) == 0 || master == 0 {
        return;
    }
    loop {
        let def = list::delete_first(&mut g.m, master);
        if def == 0 {
            break;
        }
        for i in 0..MAX_STATES {
            let at = def + STATE_BASE + i * STATE_SIZE + STATE_SPAWN_LIST;
            let sets = g.m.r32(at);
            sub_441930(g, sets);
            g.m.w32(at, 0);
        }
        g.m.free(def);
    }
    g.m.free(master);
    g.m.w32(MASTER_LIST, 0);
}

/// `void sub_441930(U_LinkedList *)` — 0x441930: deletes a list and the
/// blocks it owns.
pub fn sub_441930(g: &mut G, l: u32) {
    if l == 0 {
        return;
    }
    loop {
        let o = list::delete_first(&mut g.m, l);
        if o == 0 {
            break;
        }
        g.m.free(o);
    }
    g.m.free(l);
}

/// `void sub_441d30(void)` — 0x441d30: disposes the family list.
pub fn sub_441d30(g: &mut G) {
    g.m.w32(NUM_FAMILIES, 0);
    g.m.w32(NUM_UNITS_IN_FAMILIES, 0);
    let fams = g.m.r32(FAMILY_LIST);
    if g.m.r8(INITIALISED) == 0 || fams == 0 {
        return;
    }
    loop {
        let fam = list::delete_first(&mut g.m, fams);
        if fam == 0 {
            break;
        }
        sub_441f30(g, fam);
    }
    g.m.free(fams);
    g.m.w32(FAMILY_LIST, 0);
}

/// `void sub_441dc0(void)` — 0x441dc0: logs every family (diagnostics
/// only, nothing to do here).
pub fn sub_441dc0(g: &mut G) {}

/// `void sub_441f30(G_UnitDef_Family *)` — 0x441f30: deletes a family and
/// the 4-byte cells of its list.
pub fn sub_441f30(g: &mut G, fam: u32) {
    if fam == 0 {
        return;
    }
    loop {
        let cell = list::delete_first(&mut g.m, fam + 0x40);
        if cell == 0 {
            break;
        }
        g.m.free(cell);
    }
    g.m.free(fam);
}

/// `void sub_4419d0(G_UnitDef *)` — 0x4419d0: puts a definition into the
/// family named at +0x56 (creating it at the end of the family list), or
/// into the default family (created at the start of the list) when it has
/// no family name.
pub fn sub_4419d0(g: &mut G, def: u32) {
    let name = def + 0x56;
    let no_family = g.m.r8(name) == 0;
    let fams = g.m.r32(FAMILY_LIST);
    if !no_family {
        let mut found = false;
        for fam in list::objects(&g.m, fams) {
            if fam != 0 && g.m.strcmp_mem(name, fam) == 0 {
                found = true;
                add_cell(g, fam, def);
                g.m.wi32(NUM_UNITS_IN_FAMILIES, g.m.ri32(NUM_UNITS_IN_FAMILIES) + 1);
                break;
            }
        }
        if !found {
            let fam = g.m.alloc(FAMILY_SIZE);
            list::add(&mut g.m, fams, fam);
            let n = g.m.cstr(name).to_vec();
            g.m.strcpy(fam, &n);
            add_cell(g, fam, def);
            g.m.wi32(NUM_FAMILIES, g.m.ri32(NUM_FAMILIES) + 1);
            g.m.wi32(NUM_UNITS_IN_FAMILIES, g.m.ri32(NUM_UNITS_IN_FAMILIES) + 1);
        }
    }
    if no_family {
        let default_name = g.m.r32(DEFAULT_FAMILY_NAME_PTR);
        let mut found = false;
        for fam in list::objects(&g.m, fams) {
            if fam != 0 && g.m.strcmp_mem(default_name, fam) == 0 {
                found = true;
                add_cell(g, fam, def);
                g.m.wi32(NUM_UNITS_IN_FAMILIES, g.m.ri32(NUM_UNITS_IN_FAMILIES) + 1);
                break;
            }
        }
        if !found {
            let fam = g.m.alloc(FAMILY_SIZE);
            list::add_at_start(&mut g.m, fams, fam);
            let n = g.m.cstr(default_name).to_vec();
            g.m.strcpy(fam, &n);
            add_cell(g, fam, def);
            g.m.wi32(NUM_FAMILIES, g.m.ri32(NUM_FAMILIES) + 1);
            g.m.wi32(NUM_UNITS_IN_FAMILIES, g.m.ri32(NUM_UNITS_IN_FAMILIES) + 1);
        }
    }
}

/// Appends a 4-byte cell holding `def` to the family's list.
fn add_cell(g: &mut G, fam: u32, def: u32) {
    let cell = g.m.alloc(4);
    list::add(&mut g.m, fam + 0x40, cell);
    g.m.w32(cell, def);
}

/// `bool sub_442070(long id)` — 0x442070: whether a unit's resources were
/// already queued; if not, queues it.
pub fn sub_442070(g: &mut G, id: i32) -> bool {
    let l = g.m.r32(RES_LOADING_LIST);
    for e in list::objects(&g.m, l) {
        if e != 0 && g.m.ri32(e) == id {
            return true;
        }
    }
    let n = g.m.alloc(4);
    list::add(&mut g.m, l, n);
    g.m.wi32(n, id);
    false
}

/// `G_UnitDef_ResetResourceLoadingList` (0x4409c0).
pub fn G_UnitDef_ResetResourceLoadingList(g: &mut G) {
    G_UnitDef_ClearResourceLoadingList(g);
    let l = list::new(&mut g.m);
    g.m.w32(RES_LOADING_LIST, l);
}

/// `G_UnitDef_ClearResourceLoadingList` (0x441f90).
pub fn G_UnitDef_ClearResourceLoadingList(g: &mut G) {
    let l = g.m.r32(RES_LOADING_LIST);
    if g.m.r8(INITIALISED) == 0 || l == 0 {
        return;
    }
    for e in list::objects(&g.m, l) {
        if e != 0 {
            g.m.free(e);
        }
    }
    list::delete_all(&mut g.m, l);
    g.m.free(l);
    g.m.w32(RES_LOADING_LIST, 0);
}

/// `void sub_444520(long *field, G_UnitDef *, const char *name)` —
/// 0x444520: loads the sound a field names (`G_Res_Load`, useage 1); a
/// failure (the original also logs it) resets the field to 'none'.
pub fn sub_444520(g: &mut G, field: u32, def: u32, name: u32) {
    let id = g.m.r32(field);
    if id == NONE {
        return;
    }
    if !G_Res_Load(g, RES_TYPE_SOUND, id as i32, 1) {
        g.m.w32(field, NONE);
    }
}

/// `void sub_4445c0(long *field, G_UnitDef *, const char *name)` —
/// 0x4445c0: as `sub_444520`, for a sprite.
pub fn sub_4445c0(g: &mut G, field: u32, def: u32, name: u32) {
    let id = g.m.r32(field);
    if id == NONE {
        return;
    }
    if !G_Res_Load(g, RES_TYPE_IMAGE, id as i32, 1) {
        g.m.w32(field, NONE);
    }
}

/// `G_UnitDef_LoadResourcesForUnitDefID` (0x440a30): looks the unit up
/// and loads its resources (the original logs an unknown id).
pub fn G_UnitDef_LoadResourcesForUnitDefID(g: &mut G, id: i32) {
    if id as u32 == NONE {
        return;
    }
    let def = G_UnitDef_MasterList_GetRefPtrForID(g, id);
    if def != 0 {
        G_UnitDef_LoadResourcesForUnit(g, def);
    }
}

/// `G_UnitDef_LoadResourcesForUnit` (0x440af0): once per unit, loads the
/// sounds and sprites the definition names and recurses into the units it
/// can spawn (destruct, coin, collision and pickup spawns, each state's
/// collision spawn and spawn sets).
pub fn G_UnitDef_LoadResourcesForUnit(g: &mut G, def: u32) {
    let id = g.m.ri32(def + 4);
    if sub_442070(g, id) {
        return;
    }
    let fd = def + 0x16;
    sub_444520(g, fd + 0x49e, def, 0);
    sub_444520(g, fd + 0x42e, def, 0);
    sub_444520(g, fd + 0x446, def, 0);
    sub_444520(g, fd + 0x40a, def, 0);
    for off in [0x48c, 0x490, 0x2c2, 0x45e, 0x4ba] {
        let u = g.m.ri32(fd + off);
        G_UnitDef_LoadResourcesForUnitDefID(g, u);
    }
    let n = g.m.ri32(def + 0x12);
    let mut i = 0;
    while i < n {
        let st = def + STATE_BASE + i as u32 * STATE_SIZE;
        sub_444520(g, st, def, 0);
        sub_4445c0(g, st + 0x300, def, 0);
        let u = g.m.ri32(st + 0x2e0);
        G_UnitDef_LoadResourcesForUnitDefID(g, u);
        let sets = g.m.r32(st + STATE_SPAWN_LIST);
        if sets != 0 {
            let mut j = 0;
            while j < list::num(&g.m, sets) {
                let set = list::nth(&g.m, sets, j);
                let u = g.m.ri32(set + 0x20);
                G_UnitDef_LoadResourcesForUnitDefID(g, u);
                j += 1;
            }
        }
        i += 1;
    }
}
