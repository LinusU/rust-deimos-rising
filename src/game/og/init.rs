//! Startup (`*_Init`) functions of the modules the gameplay code relies on
//! and the per-game setup of `G_Game_Play`, ported from the disassembly.
//! Only state is ported: manager and console registration, logging and
//! drawing are skipped.
//!
//! Startup order of the original (0x4525a3..0x4525d5, after the data
//! loading of load.rs): `G_ScoreBar_Init`, `G_EG_Init`, `G_Particle_Init`,
//! `G_Debris_Init`, `G_Bgnd_Init`, `G_MotionBlur_Init`; `G_Input_Init` and
//! `G_Notice_Init` run earlier (0x45209a, 0x4520a9). See [`init_modules`].
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::eg::G_EG_Init;
use super::g::G;
use super::game::registration_is;
use super::gameobject::G_GameObject_G_GameObject;
use super::input::G_Input_Clear;
use super::level::G_Level_GetNumInLevelList;
use super::load::{G_PlayerDef_MasterList_GetIndexForID, G_PlayerDef_MasterList_GetRefPtrForIndex};
use super::mem::list;
use super::motionblur::sub_42d9d0;
use super::player::*;
use super::resource::*;
use super::umath::{ftol, sqrt};
use super::unitdefs::{
    G_UnitDef_LoadResourcesForUnit, G_UnitDef_LoadResourcesForUnitDefID, G_UnitDef_MasterList_GetRefPtrForID,
};
use super::weaponhandler::G_WeaponHandler_SetUpForAppearance;
use super::wepdefs::G_WepDef_MasterList_GetRefPtrForIndex;

const NONE: u32 = 0x656e6f6e;

/// `sizeof(G_Player)` (the operator new at 0x41eca7 asks for 0x35a).
pub const PLAYER_SIZE: u32 = 0x35a;
/// Offset of the embedded `G_WeaponHandler` in a `G_Player`.
pub const WEAPON_HANDLER: u32 = 0x235;
/// `sizeof(G_Film)`.
pub const FILM_SIZE: u32 = 0x9d7a;

/// Runs the module inits in the order the original does.
pub fn init_modules(g: &mut G) {
    G_Input_Init(g);
    G_Notice_Init(g);
    G_ScoreBar_Init(g);
    G_EG_Init(g);
    G_Particle_Init(g);
    G_Debris_Init(g);
    G_Bgnd_Init(g);
    G_MotionBlur_Init(g);
}

// ---------------------------------------------------------------------
// Module inits
// ---------------------------------------------------------------------

/// `G_Input_Init` (0x425af0): flag, then `G_Input_Clear`.
pub fn G_Input_Init(g: &mut G) {
    g.m.w8(0x4e5a89, 1);
    G_Input_Clear(g);
}

/// `G_Notice_Init` (0x42dee0): flag, then the state reset of 0x42df90
/// (`G_Notice_ResetAtLevelStart`).
pub fn G_Notice_Init(g: &mut G) {
    g.m.w8(0x4eba05, 1); // g_noticeInitialised
    super::notice::G_Notice_ResetAtLevelStart(g);
}

/// `G_Debris_Init` (0x412ab0): flag only (plus console command).
pub fn G_Debris_Init(g: &mut G) {
    g.m.w8(super::debris::IN_MODULE, 1);
}

/// `G_Bgnd_Init` (0x40f900): flag, no media mask, tile size 0, no
/// scrolling this frame (plus console commands).
pub fn G_Bgnd_Init(g: &mut G) {
    g.m.w8(0x4de6e3, 1);
    g.m.w32(super::bgnd::MEDIA_MASK, 0);
    g.m.w32(super::bgnd::TILE_SIZE, 0);
    g.m.w32(super::bgnd::SCROLLED_THIS_FRAME, 0);
}

/// `G_MotionBlur_Init` (0x42d4e0): flag; builds the pool once (0x4eb608).
pub fn G_MotionBlur_Init(g: &mut G) {
    g.m.w8(super::motionblur::IN_MODULE, 1);
    if g.m.r8(0x4eb608) == 0 {
        sub_42d9d0(g);
        g.m.w8(0x4eb608, 1);
    }
}

/// `G_Particle_Init` (0x42e7b0): flag, the velocity tables, and two random
/// starting indices (`RandomInt(0, 99)`, twice).
pub fn G_Particle_Init(g: &mut G) {
    g.m.w8(super::particle::IN_MODULE, 1);
    G_Particle_LoadHeader(g);
    let a = g.random_int(0, 0x63);
    g.m.wi32(super::particle::INDEX1, a);
    let b = g.random_int(0, 0x63);
    g.m.wi32(super::particle::INDEX2, b);
}

/// `G_Particle_LoadHeader` (0x4300b0): fills the two tables of 100
/// velocity vectors: a random point in a box (perm floats 0x36, 0x37)
/// relative to its centre, normalised with the table square root, and
/// every vector scaled by one of three factors (or none) at random.
pub fn G_Particle_LoadHeader(g: &mut G) {
    let a = ftol(G_Res_GetPermFloat(g, 0x36) as f64);
    let b = ftol(G_Res_GetPermFloat(g, 0x37) as f64);
    let div = g.m.rf32(0x4ec2a4) as f64;
    let fx = (a as f64 / div) as f32;
    let fy = (b as f64 / div) as f32;
    let mut i = 0u32;
    while i < 100 {
        let ent = super::particle::TABLE1 + i * 8;
        let rx = g.random_int(0, a);
        let ry = g.random_int(0, b);
        // dx is stored as a float; dy is stored as a float but the sum of
        // squares uses the unrounded x87 value.
        let dx = (fx as f64 - rx as f64) as f32;
        let dy_d = fy as f64 - ry as f64;
        let dy = dy_d as f32;
        let sum = dy_d * dy_d + dx as f64 * dx as f64;
        let len = sqrt(ftol(sum));
        let nx = (dx as f64 / len as f64) as f32;
        let ny = (dy as f64 / len as f64) as f32;
        g.m.wf32(super::particle::TABLE2 + i * 8, nx);
        g.m.wf32(super::particle::TABLE2 + i * 8 + 4, ny);
        g.m.wf32(ent, nx);
        g.m.wf32(ent + 4, ny);
        let r = g.random_int(0, 3);
        let scale_at = match r {
            1 => Some(0x4ec2a8),
            2 => Some(0x4ec2b0),
            3 => Some(0x4ec2b8),
            _ => None,
        };
        if let Some(c) = scale_at {
            let f = g.m.rf64(c);
            let x = (g.m.rf32(ent) as f64 * f) as f32;
            g.m.wf32(ent, x);
            let y = (g.m.rf32(ent + 4) as f64 * f) as f32;
            g.m.wf32(ent + 4, y);
        }
        i += 1;
    }
}

/// `G_ScoreBar_Init` (0x438f60): flag; for each of the two bars the score,
/// lives and gauges are zero, the eight element rects come from the perm
/// rects (0..7 for player 1, 8..15 for player 2) and a copy of each is
/// shifted by the display's window offset (taken as 0 here), the redraw
/// flags are set and the weapon info is 'none'.
pub fn G_ScoreBar_Init(g: &mut G) {
    use super::scorebar::{BARS, BAR_SIZE};
    g.m.w8(0x4f0a1f, 1);
    let sp = g.m.sp;
    // Display window rect (0x44c300 on the display): the host has no
    // window, so the offset is zero.
    let off_a = 0i32;
    let off_b = 0i32;
    let mut i = 0u32;
    while i < 2 {
        let bar = BARS + i * BAR_SIZE;
        g.m.w32(bar + 0x18, 0);
        g.m.w32(bar + 0x1c, 0);
        g.m.w32(bar + 0x20, 0);
        g.m.w32(bar + 0x24, 0);
        let mut k = 0u32;
        while k < 8 {
            G_Res_GetPermRect(g, (i * 8 + k) as i32, bar + 0x28 + k * 0x10);
            k += 1;
        }
        let mut k = 0u32;
        while k < 8 {
            let src = bar + 0x28 + k * 0x10;
            let dst = bar + 0xa8 + k * 0x10;
            let v0 = g.m.ri32(src).wrapping_add(off_a);
            let v1 = g.m.ri32(src + 4).wrapping_add(off_b);
            let v2 = g.m.ri32(src + 8).wrapping_add(off_a);
            let v3 = g.m.ri32(src + 12).wrapping_add(off_b);
            g.m.wi32(dst, v0);
            g.m.wi32(dst + 4, v1);
            g.m.wi32(dst + 8, v2);
            g.m.wi32(dst + 12, v3);
            k += 1;
        }
        for f in 0x128..=0x12fu32 {
            g.m.w8(bar + f, 1);
        }
        g.m.w32(bar + 0x131, NONE);
        g.m.w32(bar + 0x135, 0);
        g.m.w32(bar + 0x139, NONE);
        g.m.w32(bar + 0x13d, 0);
        g.m.w32(bar + 0x141, NONE);
        g.m.w32(bar + 0x145, 0);
        i += 1;
    }
    g.m.pop_to(sp);
}

// ---------------------------------------------------------------------
// G_WeaponHandler
// ---------------------------------------------------------------------

/// `G_WeaponHandler::G_WeaponHandler` (0x446c80); `this` is the handler.
pub fn G_WeaponHandler_G_WeaponHandler(g: &mut G, this: u32) {
    let obj = this + 0x8d;
    G_GameObject_G_GameObject(g, obj);
    g.m.w8(obj + 0x8a, 0);
    g.m.w8(obj + 0x8b, 0);
    g.m.w32(obj + 0x8c, 0xffff_ffff);
    g.m.w32(this + 0x73, 0);
    g.m.w32(this + 0x11, 0);
    g.m.w32(this + 0x15, 0);
    g.m.w32(this + 0x2d, 0);
    g.m.w32(this + 0x35, 0);
    g.m.w32(this + 0x39, 0);
    g.m.w32(this + 0x51, 0);
}

/// `sub_448830` (0x448830): the first weapon whose default tag is 'DEAA'
/// (`flag` false) or 'DEAG' (`flag` true); 0 if none.
pub fn sub_448830(g: &mut G, flag: bool) -> u32 {
    let mut i = 0;
    loop {
        let w = G_WepDef_MasterList_GetRefPtrForIndex(g, i);
        i += 1;
        if w == 0 {
            return 0;
        }
        let tag = g.m.r32(w + 0xc);
        if tag == 0x41414544 && !flag {
            return w;
        }
        if tag == 0x47414544 && flag {
            return w;
        }
    }
}

/// `sub_4488e0` (0x4488e0): the air ('PEAA') weapon whose level range
/// contains `level` with the highest minimum level; 0 if none.
pub fn sub_4488e0(g: &mut G, level: i32) -> u32 {
    let mut i = 0;
    let mut best = 0u32;
    loop {
        let w = G_WepDef_MasterList_GetRefPtrForIndex(g, i);
        i += 1;
        if w == 0 {
            return best;
        }
        if g.m.r32(w + 8) != 0x41414550 {
            continue;
        }
        if g.m.ri32(w + 0x140) < level {
            continue;
        }
        let min = g.m.ri32(w + 0x13c);
        if min > level {
            continue;
        }
        if best == 0 || g.m.ri32(best + 0x13c) < min {
            best = w;
        }
    }
}

/// `G_WeaponHandler::Priv_WeaponInfoList_RemoveObjects` (0x448720): frees
/// every object of the list and its links.
fn weapon_info_list_remove_objects(g: &mut G, l: u32) {
    for obj in list::objects(&g.m, l) {
        g.m.free(obj);
    }
    list::delete_all(&mut g.m, l);
}

/// `G_WeaponHandler::SetUpAtNewGameStart` (0x446d60).
pub fn G_WeaponHandler_SetUpAtNewGameStart(g: &mut G, this: u32, player_num: i32, a: i32, level: i32) {
    g.m.w32(this + 0x55, 0);
    g.m.w32(this + 0x59, 0);
    G_WeaponHandler_SetUpForAppearance(g, this, false);
    g.m.w8(this + 8, 0);
    g.m.wi32(this + 0x119, player_num);
    let v = G_Res_GetPermFloat(g, 0x95);
    g.m.wf32(this + 0x11d, v);
    let v = G_Res_GetPermFloat(g, 0x96);
    g.m.wf32(this + 0x121, v);
    g.m.w32(this + 0xd7, 0x69756c70);
    let l = g.m.r32(this + 0x73);
    if l == 0 {
        let l = list::new(&mut g.m);
        g.m.w32(this + 0x73, l);
    } else {
        weapon_info_list_remove_objects(g, l);
    }
    let w = sub_448830(g, true);
    if w != 0 {
        g.m.w32(this + 0x77, w);
        g.m.wi32(this + 0x7b, a);
        g.m.wi32(this + 0x7f, a);
        g.m.w32(this + 0x83, 0);
        g.m.w32(this + 0x87, 0);
        g.m.w8(this + 0x8b, 0);
        g.m.w8(this + 0x8c, 1);
    } else {
        panic!("GAME DATA INCORRECT. No default Ground Weapon for Level {level} was found.");
    }
    let w = sub_4488e0(g, level);
    if w != 0 {
        g.m.w32(this + 0x5d, w);
        g.m.wi32(this + 0x61, a);
        g.m.wi32(this + 0x65, a);
        g.m.w32(this + 0x69, 0);
        g.m.w32(this + 0x6d, 0);
        g.m.w8(this + 0x71, 0);
        g.m.w8(this + 0x72, 1);
    } else {
        panic!("GAME DATA INCORRECT. No suitable Air Weapon for Level {level} was found.");
    }
}

// ---------------------------------------------------------------------
// G_Player
// ---------------------------------------------------------------------

/// `G_Player::Lives_Set` (0x431280).
pub fn G_Player_Lives_Set(g: &mut G, this: u32, n: i32) {
    g.m.wi32(this + 0x8e, n.wrapping_add(0x1524dcef));
}

/// `G_Player::Score_Set` (0x434f20).
pub fn G_Player_Score_Set(g: &mut G, this: u32, n: i32) {
    g.m.wi32(this + 0xa6, n.wrapping_add(0x5532a3e));
}

/// `G_Player::Score_Reset` (0x434ef0).
pub fn G_Player_Score_Reset(g: &mut G, this: u32) {
    G_Player_Score_Set(g, this, 0);
}

/// `G_Player::Speed_Reset` (0x4311d0): the speed limit from the definition.
pub fn G_Player_Speed_Reset(g: &mut G, this: u32) {
    let def = g.m.r32(this + 0x8a);
    let v = g.m.r32(def + 0xd2);
    g.m.w32(this + 0x9a, v);
}

/// `G_Player::Cheating_Set` (0x435120).
pub fn G_Player_Cheating_Set(g: &mut G, this: u32, v: bool) {
    g.m.w8(this + 0xb3, v as u8);
}

/// `G_Player::Lives_ResetAtNewGameStart` (0x4311f0).
pub fn G_Player_Lives_ResetAtNewGameStart(g: &mut G, this: u32, first_level: bool) {
    if g.m.rb(this + 0xb8) {
        let def = g.m.r32(this + 0x8a);
        let n = if first_level { g.m.ri32(def + 0x62) } else { 1 };
        G_Player_Lives_Set(g, this, n);
        let v = g.m.r32(def + 0x66);
        g.m.w32(this + 0x92, v);
    } else {
        G_Player_Lives_Set(g, this, 0);
        g.m.w32(this + 0x92, 0);
    }
    g.m.w32(this + 0x96, 0);
}

/// `G_Player::G_Player` (0x430710), on a fresh 0x35a-byte object.
pub fn G_Player_G_Player(g: &mut G, this: u32) {
    G_GameObject_G_GameObject(g, this);
    G_WeaponHandler_G_WeaponHandler(g, this + WEAPON_HANDLER);
    g.m.w32(this + 0x8a, 0);
    g.m.w32(this + 0x92, 0);
    g.m.w32(this + 0x96, 0);
    g.m.w32(this + 0x9a, 0);
    g.m.w32(this + 0xaa, 1);
    g.m.w32(this + 0xae, 0xffff_ffff);
    g.m.w8(this + 0xb3, 0);
    g.m.w32(this + 0xb4, NONE);
    g.m.w8(this + 0xb8, 0);
    g.m.w8(this + 0xb9, 0);
    g.m.w32(this + 0xba, 1);
    g.m.w32(this + 0xbe, 0);
    g.m.w32(this + 0xc2, 0xffff_ffff);
    g.m.w32(this + 0xc6, 1);
    g.m.w8(this + 0xca, 0);
    g.m.w8(this + 0xcb, 0);
    g.m.w8(this + 0xcd, 0);
    g.m.w8(this + 0xcc, 0);
    g.m.w32(this + 0xce, 0);
    let zero = f32::from_bits(g.m.r32(0x4eca57));
    G_Player_Shields_SetPercentage(g, this, zero);
    G_Player_Score_Reset(g, this);
    G_Player_Lives_Set(g, this, 0);
    G_Player_Money_Set(g, this, 0);
}

/// `new G_Player`.
pub fn new_player(g: &mut G) -> u32 {
    let p = g.m.alloc(PLAYER_SIZE);
    G_Player_G_Player(g, p);
    p
}

/// `G_PlayerDef_LoadResourcesForDef` (0x4363b0): the definition's sprites
/// (images 0x28, 0x30, 0x38, 0x40), sound 0xec and the units 0x9e, 0x6e,
/// 0xba, 0xc2, 0xc6, 0xca, 0xce.
pub fn G_PlayerDef_LoadResourcesForDef(g: &mut G, def: u32) {
    for off in [0x28u32, 0x30, 0x38, 0x40] {
        let id = g.m.ri32(def + off);
        G_Res_Load(g, RES_TYPE_IMAGE, id, 1);
    }
    let id = g.m.ri32(def + 0xec);
    G_Res_Load(g, RES_TYPE_SOUND, id, 1);
    for off in [0x9eu32, 0x6e, 0xba, 0xc2, 0xc6, 0xca, 0xce] {
        let id = g.m.ri32(def + off);
        G_UnitDef_LoadResourcesForUnitDefID(g, id);
    }
}

/// `G_Player::sub_035890` (0x435890): loads the resources of a unit named
/// by a perm object id (an unknown id is a data failure).
fn load_unit_resources(g: &mut G, id: i32, name: &str) {
    if id as u32 == NONE {
        return;
    }
    let def = G_UnitDef_MasterList_GetRefPtrForID(g, id);
    if def != 0 {
        G_UnitDef_LoadResourcesForUnit(g, def);
    } else {
        eprintln!("G_Player: unit '{}' ({name}) is not defined", String::from_utf8_lossy(&(id as u32).to_le_bytes()));
    }
}

/// The player definitions' resources named by perm object 0 / 1.
fn load_perm_player_def(g: &mut G, which: i32) {
    let id = G_Res_GetPermObjectID(g, which);
    if id as u32 == NONE {
        return;
    }
    let idx = G_PlayerDef_MasterList_GetIndexForID(g, id);
    if idx == -1 {
        panic!("G_Player.cc:{}: player definition missing", if which == 0 { 0xaa } else { 0xb7 });
    }
    let def = G_PlayerDef_MasterList_GetRefPtrForIndex(g, idx);
    if def != 0 {
        G_PlayerDef_LoadResourcesForDef(g, def);
    }
}

/// `G_Player::SetUpAtNewGameStart` (0x430850): `player_num`, game type,
/// the game time (`a`) and the 1-based level number (`level`).
pub fn G_Player_SetUpAtNewGameStart(g: &mut G, this: u32, player_num: i32, game_type: i32, a: i32, level: i32) {
    g.m.wi32(this + 0xc6, game_type);
    g.m.wi32(this + 0xc2, player_num);
    let which = match player_num {
        0 => 0,
        1 => 1,
        _ => panic!("G_Player::SetUpAtNewGameStart: player number {player_num}"),
    };
    let id = G_Res_GetPermObjectID(g, which);
    let idx = G_PlayerDef_MasterList_GetIndexForID(g, id);
    if idx != -1 {
        let def = G_PlayerDef_MasterList_GetRefPtrForIndex(g, idx);
        g.m.w32(this + 0x8a, def);
        if def == 0 {
            panic!("G_Player.cc:0x96: no player definition for player {player_num}");
        }
    }
    let def = g.m.r32(this + 0x8a);
    G_PlayerDef_LoadResourcesForDef(g, def);
    g.m.w8(this + 0xb8, 0);
    load_perm_player_def(g, 0);
    load_perm_player_def(g, 1);
    for i in 2..=9 {
        let id = G_Res_GetPermObjectID(g, i);
        load_unit_resources(g, id, "perm object");
    }
    let n = G_Level_GetNumInLevelList(g);
    for i in 0..n {
        let id = G_Res_GetPermObjectID(g, 0xa + i);
        load_unit_resources(g, id, "level object");
    }
    for i in 0x16..=0x27 {
        let id = G_Res_GetPermObjectID(g, i);
        load_unit_resources(g, id, "perm object");
    }
    // One player game: only player 1 is in the game.
    if game_type == 1 {
        if player_num == 0 {
            g.m.w8(this + 0xb8, 1);
            G_Player_Lives_ResetAtNewGameStart(g, this, level == 1);
        }
    } else {
        g.m.w8(this + 0xb8, 1);
        G_Player_Lives_ResetAtNewGameStart(g, this, level == 1);
    }
    G_Player_Money_Reset(g, this);
    G_Player_Score_Reset(g, this);
    G_Player_Priv_Multiplier_Reset(g, this);
    G_Player_Shields_Reset(g, this, true);
    let pn = g.m.ri32(this + 0xc2);
    G_WeaponHandler_SetUpAtNewGameStart(g, this + WEAPON_HANDLER, pn, a, level);
    G_Player_Speed_Reset(g, this);
    G_Player_Shields_Reset(g, this, false);
    G_Player_Cheating_Set(g, this, false);
    G_Player_MoneyCounter_Reset(g, this);
    G_Player_PowerupOverload_Reset(g, this);
    if g.m.rb(this + 0xb8) {
        G_Player_SetState(g, this, 2, a);
        g.m.clear(this + 0x1f6, 7);
    } else {
        G_Player_SetState(g, this, 1, a);
    }
    g.m.w8(this + 0x233, 0);
    if g.m.ri32(this + 0xc2) == 0 {
        let reg = registration_is(g);
        g.m.w8(this + 0x233, reg as u8);
    }
}

// ---------------------------------------------------------------------
// G_Film
// ---------------------------------------------------------------------

/// `SwapULong`.
fn swap(v: u32) -> u32 {
    v.swap_bytes()
}

/// `G_Film::G_Film` (0x41df10) on a fresh 0x9d7a-byte object.
pub fn G_Film_G_Film(g: &mut G, this: u32) {
    g.m.w8(this, 1);
    g.m.clear(this + 0x12, 0x9d68);
    g.m.w32(this + 0x12, 0x2715);
    G_Film_Priv_ResetCurrentFrameNumbers(g, this);
    g.m.w32(this + 0xa, 0);
    g.m.w32(this + 0xe, 0);
}

/// `G_Film::Priv_ResetCurrentFrameNumbers` (0x41e4a0).
pub fn G_Film_Priv_ResetCurrentFrameNumbers(g: &mut G, this: u32) {
    g.m.w32(this + 2, 0);
    g.m.w32(this + 6, 0);
}

/// `G_Film::Load` (0x41dfc0), the resource bytes (`U_Pak_GetPtrToTagData`
/// of tag 'film') given as `data`; 0x9d68 bytes are copied to `+0x12`,
/// then a byte-swapped (Macintosh) film header is fixed up.
pub fn G_Film_Load(g: &mut G, this: u32, id: i32, data: Option<&[u8]>) -> bool {
    let mut result = false;
    if id as u32 == NONE {
        return false;
    }
    for i in 0..2u32 {
        g.m.w32(this + 0xa + i * 4, 0);
    }
    let Some(data) = data else {
        eprintln!("G_Film: film resource missing");
        return false;
    };
    let n = data.len().min(0x9d68);
    g.m.write(this + 0x12, &data[..n]);
    let version = g.m.r32(this + 0x12);
    if version != 0x2715 && swap(version) == 0x2715 {
        let v = swap(g.m.r32(this + 0x12));
        g.m.w32(this + 0x12, v);
        let v = swap(g.m.r32(this + 0x16));
        g.m.w32(this + 0x16, v);
        for i in 0..2u32 {
            let rec = this + i * 0x4eac;
            let v = swap(g.m.r32(rec + 0x22));
            g.m.w32(rec + 0x22, v);
            let v = swap(g.m.r32(rec + 0x26));
            g.m.w32(rec + 0x26, v);
        }
    }
    if g.m.r32(this + 0x12) != 0x2715 {
        eprintln!("G_Film: bad film version {:#x}", g.m.r32(this + 0x12));
    } else {
        result = true;
    }
    result
}

/// `G_Film::StartPlayback` (0x41e210): level id, game type and random seed
/// out, frame numbers back to 0.
pub fn G_Film_StartPlayback(g: &mut G, this: u32, out_level: u32, out_type: u32, out_seed: u32) {
    let v = g.m.r32(this + 0x1a);
    g.m.w32(out_level, v);
    let v = g.m.r32(this + 0x1e);
    g.m.w32(out_type, v);
    let v = g.m.r32(this + 0x16);
    g.m.w32(out_seed, v);
    G_Film_Priv_ResetCurrentFrameNumbers(g, this);
}

/// `G_Film::IsFinished` (0x41e2c0): the frame of player 1 is past its
/// recorded length.
pub fn G_Film_IsFinished(g: &mut G, this: u32) -> bool {
    g.m.ri32(this + 2) > g.m.ri32(this + 0x22)
}
