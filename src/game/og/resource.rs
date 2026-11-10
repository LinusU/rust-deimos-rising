//! Port of G_Res (G_Resource.cc, 0x437000..0x438d20): the resource list
//! of loaded sprites and sounds and the permanent tables (object, sprite
//! and sound ids, rects, floats, colour, game strings) that
//! `G_Res_LoadPermData` fills at startup (see load.rs).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::mem::list;

// Globals (the module's .data block starts at 0x4ee118).
/// `g_resourceInitialised` (byte).
pub const INITIALISED: u32 = 0x4ee685;
/// `g_resourceList`: U_LinkedList of `G_Res_Entry` {magic, type, useage, id}.
pub const RES_LIST: u32 = 0x4ee686;
/// `g_permDataLoaded` (byte).
pub const PERM_DATA_LOADED: u32 = 0x4ee68a;
/// `g_permObjectIDs[0x28]`.
pub const PERM_OBJECT_IDS: u32 = 0x4ee68b;
/// `g_permSpriteIDs[8]`.
pub const PERM_SPRITE_IDS: u32 = 0x4ee72b;
/// `g_permSoundIDs[0x18]`.
pub const PERM_SOUND_IDS: u32 = 0x4ee74b;
/// `g_permGameStrings[0x25][0x80]`.
pub const PERM_GAME_STRINGS: u32 = 0x4ee7ab;
/// `g_permRects[0x16]` (U_Rect {top, left, bottom, right}).
pub const PERM_RECTS: u32 = 0x4efa2b;
/// `g_permFloats[0xdc]`.
pub const PERM_FLOATS: u32 = 0x4efb8b;
/// `g_permColor` (u16).
pub const PERM_COLOR: u32 = 0x4efefb;
/// The tag ids `sub_038d20` stores at startup (`sPriv_ResID` = 'inte' at
/// 0x4ee173, `sPriv_StringsResID` = 'pgsl' at 0x4ee17b, ...).
pub const TAG_ID_OBJECTS: u32 = 0x4ee183; // 'gaob'
pub const TAG_ID_SPRITES: u32 = 0x4ee187; // 'gasp'
pub const TAG_ID_SOUNDS: u32 = 0x4ee18b; // 'gaso'
pub const TAG_ID_RECTS: u32 = 0x4ee19f; // 'inre'
pub const TAG_ID_FLOATS: u32 = 0x4ee1a3; // 'gafl'
pub const TAG_ID_COLORS: u32 = 0x4ee1a7; // 'gaco'
pub const TAG_ID_INTERFACE_STRINGS: u32 = 0x4ee173; // 'inte'
pub const TAG_ID_GAME_STRINGS: u32 = 0x4ee17b; // 'pgsl'

pub const RES_TYPE_SOUND: i32 = 0;
pub const RES_TYPE_IMAGE: i32 = 1;

const SOUN: u32 = 0x6e756f73;
const IM08: u32 = 0x38306d69;
const NONE: u32 = 0x656e6f6e;

/// `G_Res_Init` (0x4375d0), minus the console commands: a fresh, empty
/// resource list.
pub fn G_Res_Init(g: &mut G) {
    g.m.w8(INITIALISED, 1);
    let old = g.m.r32(RES_LIST);
    if old != 0 {
        free_list(g, old);
        g.m.w32(RES_LIST, 0);
    }
    let l = list::new(&mut g.m);
    g.m.w32(RES_LIST, l);
}

/// `delete list` for a list whose objects are not owned.
fn free_list(g: &mut G, l: u32) {
    list::delete_all(&mut g.m, l);
    g.m.free(l);
}

/// `bool __cdecl G_Res_CanBeLoaded(enum G_Res_Type, long)` — 0x437a40.
pub fn G_Res_CanBeLoaded(g: &mut G, ty: i32, id: i32) -> bool {
    let tag = match ty {
        0 => SOUN,
        _ => IM08,
    };
    g.host.has_tag(tag, id as u32)
}

/// `bool __cdecl G_Res_Load(enum G_Res_Type, long, enum G_Res_Useage)` —
/// 0x4377c0. `U_Sound_Load`/`U_Sprite_Load` become "the pak has it"; the
/// log line on failure is dropped.
pub fn G_Res_Load(g: &mut G, ty: i32, id: i32, useage: i32) -> bool {
    let mut loaded = false;
    if id as u32 == NONE {
        return false;
    }
    let existing = G_Res_FindEntry(g, ty, id);
    if existing == 0 {
        match ty {
            0 => loaded = g.host.has_tag(SOUN, id as u32),
            1 => loaded = g.host.has_image(id as u32),
            _ => return false,
        }
        if loaded {
            let e = g.m.alloc(0x10);
            g.m.w32(e, 0x499602d2);
            let l = g.m.r32(RES_LIST);
            list::add(&mut g.m, l, e);
            g.m.wi32(e + 4, ty);
            g.m.wi32(e + 8, useage);
            g.m.wi32(e + 0xc, id);
        }
    } else {
        if useage < g.m.ri32(existing + 8) {
            g.m.wi32(existing + 8, useage);
        }
        loaded = true;
    }
    loaded
}

/// `bool __cdecl G_Res_IsLoaded(enum G_Res_Type, long)` — 0x437a80, 144 bytes, decomp: complete.
pub fn G_Res_IsLoaded(g: &mut G, a0: i32, a1: i32) -> bool {
    // Same walk as FindEntry, answering found / not found.
    let l = g.m.r32(RES_LIST);
    for e in list::objects(&g.m, l) {
        if e != 0 && g.m.ri32(e + 4) == a0 && g.m.ri32(e + 0xc) == a1 {
            return true;
        }
    }
    false
}

/// `long __cdecl G_Res_GetPermObjectID(int)` — 0x438140, 16 bytes, decomp: complete.
pub fn G_Res_GetPermObjectID(g: &mut G, a0: i32) -> i32 {
    g.m.ri32(PERM_OBJECT_IDS.wrapping_add((a0 as u32).wrapping_mul(4)))
}

/// `long __cdecl G_Res_GetPermSpriteID(int)` — 0x438150.
pub fn G_Res_GetPermSpriteID(g: &mut G, a0: i32) -> i32 {
    g.m.ri32(PERM_SPRITE_IDS.wrapping_add((a0 as u32).wrapping_mul(4)))
}

/// `long __cdecl G_Res_GetPermSoundID(int)` — 0x438160, 16 bytes, decomp: complete.
pub fn G_Res_GetPermSoundID(g: &mut G, a0: i32) -> i32 {
    g.m.ri32(PERM_SOUND_IDS.wrapping_add((a0 as u32).wrapping_mul(4)))
}

/// `void __cdecl G_Res_GetPermRect(int, struct U_Rect *)` — 0x438170:
/// copies the 16-byte rect {top, left, bottom, right} to `out`.
pub fn G_Res_GetPermRect(g: &mut G, a0: i32, out: u32) {
    let src = PERM_RECTS.wrapping_add((a0 as u32) << 4);
    g.m.copy(out, src, 16);
}

/// `float __cdecl G_Res_GetPermFloat(int)` — 0x4381b0, 16 bytes, decomp: complete.
pub fn G_Res_GetPermFloat(g: &mut G, a0: i32) -> f32 {
    g.m.rf32(PERM_FLOATS.wrapping_add((a0 as u32).wrapping_mul(4)))
}

/// `char * __cdecl G_Res_GetPermGameString(enum G_Res_PermGameStringIndex)` — 0x4381c0, 16 bytes, decomp: complete.
pub fn G_Res_GetPermGameString(g: &mut G, a0: i32) -> u32 {
    PERM_GAME_STRINGS.wrapping_add((a0 as u32) << 7)
}

/// `struct G_Res_Entry * __cdecl G_Res_FindEntry(enum G_Res_Type, long)` — 0x438320, 144 bytes, decomp: complete.
pub fn G_Res_FindEntry(g: &mut G, a0: i32, a1: i32) -> u32 {
    let l = g.m.r32(RES_LIST);
    for e in list::objects(&g.m, l) {
        if e != 0 && g.m.ri32(e + 4) == a0 && g.m.ri32(e + 0xc) == a1 {
            return e;
        }
    }
    0
}
