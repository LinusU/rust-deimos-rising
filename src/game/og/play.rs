//! The replay path of `G_Game_Play` (0x41e690) driving the ported tick
//! (`sub_420280`), headless: film load and start, the level start, then the
//! loop until the film or the level ends. Display, music, the menu and ad
//! screens, the console commands and `G_GameInterface` are not ported.
//!
//! Globals `G_Game_Play` sets for a replay (all in the `0x4e40f0` block):
//! `[0x4e4999]` = registration (`gRegistration.Is()`), `[0x4e481b]` = 1 and
//! `0x4e481c..0x4e4998` cleared on the first call, `[0x4e4846]` (shadows)
//! = 1, `[0x4e483a]` (game mode) = 1 for a replay, `[0x4e483e]` (game type)
//! from the film, `[0x4e4824]` (playing) = 1, `[0x4e4826]`, `[0x4e4825]`,
//! `[0x4e4855]` = 0, `[0x4e4832]` (level id) and `[0x4e482e]` (level
//! number) from the film's level, `[0x4e4836]` (game time) = 0.
//!
//! The original under test runs registered: `game::registration_is` reads
//! `[0x52b688] > 0` (`sub_4589c0`, whose result is `1 << [0x52b690]`, never
//! 0), so [`run_film`] writes 1 there.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use std::cell::RefCell;
use std::io::Write;
use std::path::Path;

use super::g::{Host, G};
use super::game::*;
use super::init::*;
use super::input::G_Input_AssumeControl;
use super::level::{self, LEVEL_SIZE};
use super::load::{load_game_data, PakHost};
use super::mem::Mem;
use super::player::G_Player_InGame;
use super::resource::G_Res_GetPermFloat;
use super::umath::ftol;
use super::unitdefs::G_UnitDef_ResetResourceLoadingList;
use crate::assets::GameAssets;

/// `sPriv_Info.playerPtrs[2]` lives at `PLAYERS`; the registration number
/// `sub_4589c0` tests.
const REGISTRATION: u32 = 0x52b688;

/// The inner host of the `PakHost`: sprite and image data from the
/// remake's asset loader.
pub struct AssetHost {
    assets: RefCell<GameAssets>,
}

impl AssetHost {
    pub fn open(paks: &Path) -> std::io::Result<AssetHost> {
        Ok(AssetHost { assets: RefCell::new(GameAssets::open(paks)?) })
    }
}

impl Host for AssetHost {
    fn sprite_frame_size(&self, id: u32, frame: i32) -> Option<(i32, i32)> {
        if frame < 0 {
            return None;
        }
        let mut a = self.assets.borrow_mut();
        a.warm_sprite_dims(id);
        a.sprite_dims.get(&id)?.get(frame as usize).map(|&(w, h)| (w as i32, h as i32))
    }
    fn sprite_num_frames(&self, id: u32) -> i32 {
        let mut a = self.assets.borrow_mut();
        a.warm_sprite_dims(id);
        a.sprite_dims.get(&id).map(|d| d.len() as i32).unwrap_or(0)
    }
    fn has_image(&self, id: u32) -> bool {
        self.sprite_num_frames(id) > 0
    }
    fn image_size(&self, id: u32) -> Option<(i32, i32)> {
        let mut a = self.assets.borrow_mut();
        a.image16(id).map(|(w, h, _)| (*w as i32, *h as i32))
    }
    fn image_pixels16(&self, id: u32) -> Option<(i32, i32, Vec<u16>)> {
        let mut a = self.assets.borrow_mut();
        a.media_mask(id).map(|m| (m.w as i32, m.h as i32, m.px.clone()))
    }
}

/// A `G` over the retail executable image and the paks in `paks`, with
/// the game data loaded and the module inits run, as at the original's
/// startup. Registered (see the module docs).
pub fn startup(exe: &[u8], paks: &Path) -> Result<G, String> {
    let mut m = Mem::new();
    m.map_pe(exe)?;
    let inner = AssetHost::open(paks).map_err(|e| format!("assets: {e}"))?;
    let host = PakHost::open(paks, Box::new(inner)).map_err(|e| format!("paks: {e}"))?;
    let mut g = G::new(m, Box::new(host));
    g.m.wi32(REGISTRATION, 1);
    load_game_data(&mut g)?;
    init_modules(&mut g);
    Ok(g)
}

/// Writes the `R` lines for the draws since the last call.
fn flush_rand(g: &mut G, out: &mut dyn Write) {
    if let Some(log) = g.rand_log.as_mut() {
        for (state, _) in log.drain(..) {
            let _ = writeln!(out, "R {state:08x}");
        }
    }
}

/// `G_Game_Play` for a replay of film `film_id` (`film` are the resource's
/// bytes), at most `max_ticks` ticks. One `T` line per tick (player 0's
/// position, score and state before the tick) and one `R` line per rand()
/// draw. Returns the number of ticks run.
pub fn play_film(
    g: &mut G,
    film_id: u32,
    film: &[u8],
    max_ticks: Option<u32>,
    out: &mut dyn Write,
) -> Result<u32, String> {
    // startTime = U_Utils_RandomInt(0x190, 0x7d0) (used by the recording
    // path only; still a draw), gRegistration.Is().
    let _start_time = g.random_int(0x190, 0x7d0);
    let reg = registration_is(g);
    g.m.w8(0x4e4999, reg as u8);

    if g.m.r8(SHADOWS_VALID) == 0 {
        // First call: the module's state block is cleared (the console
        // commands are not registered).
        g.m.w8(SHADOWS_VALID, 1);
        g.m.clear(PLAYERS, 0x17c);
        g.m.w8(SHADOWS, 1);
    } else {
        for a in [PLAYING, MONEY_DONE, NO_PLAYER_IN_GAME, ACCURACY_REWARD_PENDING, ACCURACY_REWARD, WAITING_FOR_LEVEL_END] {
            g.m.w8(a, 0);
        }
        g.m.w32(LEVEL_ID, 0x656e6f6e);
        g.m.w32(CURRENT_LEVEL, 0);
        g.m.w32(LEVELS_PLAYED, 0);
        g.m.w8(FINAL_LEVEL, 0);
        g.m.w32(GAME_TIME, 0);
        g.m.w32(GAME_MODE, 0);
        g.m.w32(0x4e483e, 1);
        g.m.w32(0x4e484c, 0);
        g.m.w32(GAME_OVER_TIME, 0);
        g.m.w8(END_SEQUENCE_STARTED, 0);
        g.m.w32(END_SEQUENCE_TIME, 0);
        g.m.w8(FLAG_4854, 0);
        sub_420790(g);
        sub_4207d0(g);
        sub_4208d0(g);
        g.m.w32(LEVELS_COMPLETED, 0);
        for i in 0..2u32 {
            g.m.w32(PLAYERS + i * 4, 0);
        }
    }
    G_UnitDef_ResetResourceLoadingList(g);
    for i in 0..2u32 {
        let p = new_player(g);
        g.m.w32(PLAYERS + i * 4, p);
    }

    let sp = g.m.sp;
    let proc_a = g.m.alloca(2); // procA at ebp-0x4f0, procC at ebp-0x4ee
    let proc_c = proc_a + 2;
    let film_obj = g.m.alloc(FILM_SIZE);
    G_Film_G_Film(g, film_obj);
    let out_level = g.m.alloca(4);
    g.m.w32(out_level, 0x656e6f6e);
    let is_replay = true;
    g.m.w32(GAME_MODE, 1);

    // sub_020120 (the film index selection is replaced by the id): Load,
    // StartPlayback, srand(seed).
    if !G_Film_Load(g, film_obj, film_id as i32, Some(film)) {
        g.m.pop_to(sp);
        return Err("film failed to load".into());
    }
    let out_seed = g.m.alloca(4);
    G_Film_StartPlayback(g, film_obj, out_level, 0x4e483e, out_seed);
    let seed = g.m.r32(out_seed);
    g.srand(seed);
    g.rand_log = Some(Vec::new());

    let level_id = g.m.ri32(out_level);
    let level1 = g.m.alloc(LEVEL_SIZE);
    level::G_Level_LoadByID(g, level_id, level1, 0);

    g.m.w8(PLAYING, 1);
    g.m.w8(NO_PLAYER_IN_GAME, 0);
    g.m.w8(MONEY_DONE, 0);
    g.m.w8(WAITING_FOR_LEVEL_END, 0);
    g.m.wi32(LEVEL_ID, level_id);
    let level_num = level::G_Level_Get1BasedNumForLevelID(g, level_id);
    if level_num < 1 {
        g.m.pop_to(sp);
        return Err(format!("film level '{}' is not in the level list", level_id));
    }
    g.m.wi32(CURRENT_LEVEL, level_num);
    let level2 = g.m.alloc(LEVEL_SIZE);
    level::G_Level_LoadByLevelListNumber(g, level_num, level2, 0);
    for i in 0..2u32 {
        let p = g.m.r32(PLAYERS + i * 4);
        let ty = g.m.ri32(0x4e483e);
        let time = g.m.ri32(GAME_TIME);
        G_Player_SetUpAtNewGameStart(g, p, i as i32, ty, time, level_num);
    }
    G_Input_AssumeControl(g, true);
    sub_41fc80(g, is_replay, level2 as i32);

    let mut ticks = 0u32;
    while g.m.rb(PLAYING) {
        if max_ticks.map_or(false, |n| ticks >= n) {
            break;
        }
        // Output: draws since the previous line, then player 0's state.
        flush_rand(g, out);
        let p0 = g.m.r32(PLAYERS);
        let x = g.m.rf32(p0);
        let y = g.m.rf32(p0 + 4);
        let score = g.m.ri32(p0 + 0xa6).wrapping_sub(0x5532a3e);
        let st = g.m.ri32(p0 + 0xba);
        let t = g.m.ri32(GAME_TIME);
        let _ = writeln!(out, "T {t} ({x:.4},{y:.4}) score={score} st={st}");

        // Process_StartFrame says: run a frame.
        if g.m.r8(FLAG_4854) == 0 {
            // The music start (replays do not play music).
            let start = ftol(G_Res_GetPermFloat(g, 0x12) as f64);
            if g.m.ri32(GAME_TIME) == start {
                g.m.w8(FLAG_4854, 1);
            }
        }
        sub_420280(g, proc_a, is_replay as u8, proc_c, film_obj);
        // sub_4207f0: once the money counters are done a replay stops.
        if g.m.r8(MONEY_DONE) != 0 {
            g.m.w8(PLAYING, 0);
        }
        let t = g.m.ri32(GAME_TIME).wrapping_add(1);
        g.m.wi32(GAME_TIME, t);
        ticks += 1;
        // (sub_020740: the draw lists are not built.)
        if G_Film_IsFinished(g, film_obj) {
            g.m.w8(PLAYING, 0);
        }
    }
    flush_rand(g, out);
    g.m.pop_to(sp);
    Ok(ticks)
}

/// Startup plus [`play_film`].
pub fn run_film(
    exe: &[u8],
    paks: &Path,
    film_id: u32,
    film: &[u8],
    max_ticks: Option<u32>,
    out: &mut dyn Write,
) -> Result<u32, String> {
    let mut g = startup(exe, paks)?;
    play_film(&mut g, film_id, film, max_ticks, out)
}
