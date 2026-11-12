//! Live play on the ported game: the non-replay path of `G_Game_Play`
//! (0x41e690) split into `start_game` (everything before the loop) and
//! `tick` (one pass of the loop: `sub_420280`, `sub_4207f0`, the time
//! increment, `sub_420740`), for a renderer and the input devices to drive.
//!
//! What differs from a film replay (`play.rs`), all as in the original:
//! `[0x4e483a]` (game mode) is 0 so `G_Player::Priv_GetInputs` reads
//! `G_Input_GetCachedPlayerInputs` (filled by `G_Input_CachePlayerInputs`
//! from [`G::live_inputs`]) and records them into the film object with
//! `G_Film::SetInputs`; `[0x4e483e]` (game type) is the number of players;
//! the seed is the caller's (the original passes the tick count to `srand`);
//! when the end-of-level money counters finish with a player still in the
//! game `sub_4207f0` starts the next level (`sub_41fc80`) instead of ending.
//!
//! Not ported (as in `play.rs`): music, the display, the console, pausing,
//! `G_GameInterface`, the recording of the film to disk and the results the
//! original copies out of the players when `G_Game_Play` returns (read them
//! through the queries instead, before the next `start_game`).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use std::path::Path;

use super::game::*;
use super::init::*;
use super::input::G_Input_AssumeControl;
use super::level::{self, LEVEL_SIZE};
use super::player::*;
use super::resource::{G_Res_GetPermFloat, G_Res_GetPermSoundID};
use super::umath::ftol;
use super::unitdefs::G_UnitDef_ResetResourceLoadingList;
use super::view::{self, FrameView};
use super::g::G;

const NONE: u32 = 0x656e6f6e;

/// How a game that is no longer running ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// Every player ran out of lives (the "game over" object has run its
    /// course).
    GameOver,
    /// The last level was completed and the money counters finished.
    Completed,
}

/// One player's state, as the original's accessors report it.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerInfo {
    /// `G_Player::InGame` (`+0xb8`): takes part in this game.
    pub in_game: bool,
    /// `G_Player::GetState`: 1 leaving, 2 entering, 3 dying, 4 active
    /// (0 = never started).
    pub state: i32,
    /// Position (`+0`, `+4`): x in game-area pixels, y in screen pixels.
    pub x: f32,
    pub y: f32,
    pub score: i32,
    pub lives: i32,
    /// `Shields_GetPercentage`, 0..=100.
    pub shields_pct: f32,
    pub money: i32,
    /// `Multiplier_Get`: 1, 2, 3, 4, 5 or 10.
    pub multiplier: i32,
    pub invulnerable: bool,
    /// `MoneyCounter_IsActive`: the end-of-level money tally is running.
    pub money_counter_active: bool,
}

/// A running (or finished) game on the port.
pub struct Session {
    pub g: G,
    /// The `G_Film` the original records the inputs into.
    film: u32,
    /// The `G_Level` of the current level (`sub_41fc80`'s argument).
    level: u32,
    /// procA (first-active flag, byte 0) and procC (game-over spawned,
    /// byte 2) of `G_Game_Play`.
    flags: u32,
    started: bool,
    /// A film replay (`start_film`): the film supplies the inputs.
    replay: bool,
    frame: FrameView,
}

fn pf_int(g: &mut G, i: i32) -> i32 {
    ftol(G_Res_GetPermFloat(g, i) as f64)
}

impl Session {
    /// Loads the game data from the retail executable image and the paks,
    /// runs the module inits (`play::startup`).
    pub fn new(exe: &[u8], paks: &Path) -> Result<Session, String> {
        let mut g = super::play::startup(exe, paks)?;
        let flags = g.m.alloc(4);
        Ok(Session { g, film: 0, level: 0, flags, started: false, replay: false, frame: FrameView::default() })
    }

    /// The part of `G_Game_Play` before the film and level are chosen.
    fn reset_state(&mut self) {
        let g = &mut self.g;
        // startTime = U_Utils_RandomInt(0x190, 0x7d0) (recording path only;
        // still a draw), gRegistration.Is().
        let _start_time = g.random_int(0x190, 0x7d0);
        let reg = registration_is(g);
        g.m.w8(0x4e4999, reg as u8);
        if g.m.r8(SHADOWS_VALID) == 0 {
            g.m.w8(SHADOWS_VALID, 1);
            g.m.clear(PLAYERS, 0x17c);
            g.m.w8(SHADOWS, 1);
        } else {
            for a in [PLAYING, MONEY_DONE, NO_PLAYER_IN_GAME, ACCURACY_REWARD_PENDING, ACCURACY_REWARD, WAITING_FOR_LEVEL_END] {
                g.m.w8(a, 0);
            }
            g.m.w32(LEVEL_ID, NONE);
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
        g.m.clear(self.flags, 4);
        self.film = g.m.alloc(FILM_SIZE);
        G_Film_G_Film(g, self.film);
    }

    /// `G_Game_Play` up to the loop, for a new live game: `players` is the
    /// game type (1 or 2), `first_level` the 1-based position in the level
    /// list, `seed` what the original takes from the tick count.
    pub fn start_game(&mut self, players: i32, first_level: i32, seed: u32) -> Result<(), String> {
        if !(1..=2).contains(&players) {
            return Err(format!("players must be 1 or 2, not {players}"));
        }
        let n = level::G_Level_GetNumInLevelList(&mut self.g);
        if first_level < 1 || first_level > n {
            return Err(format!("level {first_level} is not in the level list (1..={n})"));
        }
        self.replay = false;
        self.reset_state();
        let g = &mut self.g;
        // The film id 0 of the start struct: not a replay, game mode 0.
        g.m.w32(GAME_MODE, 0);
        // The struct's level id / game type; recording starts here in the
        // original (0x41e260) and srand(tick count).
        let level_num = first_level;
        let level_id = level::G_Level_GetLevelIDFor1BasedLevelListNumber(g, level_num);
        g.m.wi32(0x4e483e, players);
        let level1 = g.m.alloc(LEVEL_SIZE);
        level::G_Level_LoadByID(g, level_id, level1, 0);
        g.srand(seed);

        g.m.w8(PLAYING, 1);
        g.m.w8(NO_PLAYER_IN_GAME, 0);
        g.m.w8(MONEY_DONE, 0);
        g.m.w8(WAITING_FOR_LEVEL_END, 0);
        g.m.wi32(LEVEL_ID, level_id);
        let num = level::G_Level_Get1BasedNumForLevelID(g, level_id);
        if num < 1 {
            return Err(format!("level id {level_id:#x} is not in the level list"));
        }
        g.m.wi32(CURRENT_LEVEL, num);
        let level2 = g.m.alloc(LEVEL_SIZE);
        level::G_Level_LoadByLevelListNumber(g, num, level2, 0);
        for i in 0..2u32 {
            let p = g.m.r32(PLAYERS + i * 4);
            let ty = g.m.ri32(0x4e483e);
            let time = g.m.ri32(GAME_TIME);
            G_Player_SetUpAtNewGameStart(g, p, i as i32, ty, time, num);
        }
        G_Input_AssumeControl(g, true);
        sub_41fc80(g, false, level2 as i32);
        self.level = level2;
        self.started = true;
        g.live_inputs = [0; 14];
        g.draw_sink = Some(Vec::new());
        self.frame = view::build_frame(g, level2);
        Ok(())
    }

    /// `G_Game_Play` up to the loop for a replay of film `film_id` (the
    /// resource's bytes in `film`), as `play::play_film` sets it up. Ticks
    /// then take their inputs from the film; `tick`'s argument is ignored.
    pub fn start_film(&mut self, film_id: u32, film: &[u8]) -> Result<(), String> {
        self.replay = true;
        self.started = false;
        self.reset_state();
        let g = &mut self.g;
        let sp = g.m.sp;
        let out_level = g.m.alloca(4);
        g.m.w32(out_level, NONE);
        g.m.w32(GAME_MODE, 1);
        let film_obj = self.film;
        if !G_Film_Load(g, film_obj, film_id as i32, Some(film)) {
            g.m.pop_to(sp);
            return Err("film failed to load".into());
        }
        let out_seed = g.m.alloca(4);
        G_Film_StartPlayback(g, film_obj, out_level, 0x4e483e, out_seed);
        let seed = g.m.r32(out_seed);
        g.srand(seed);
        let level_id = g.m.ri32(out_level);
        g.m.pop_to(sp);
        let level1 = g.m.alloc(LEVEL_SIZE);
        level::G_Level_LoadByID(g, level_id, level1, 0);

        g.m.w8(PLAYING, 1);
        g.m.w8(NO_PLAYER_IN_GAME, 0);
        g.m.w8(MONEY_DONE, 0);
        g.m.w8(WAITING_FOR_LEVEL_END, 0);
        g.m.wi32(LEVEL_ID, level_id);
        let level_num = level::G_Level_Get1BasedNumForLevelID(g, level_id);
        if level_num < 1 {
            return Err(format!("film level {level_id:#x} is not in the level list"));
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
        sub_41fc80(g, true, level2 as i32);
        self.level = level2;
        self.started = true;
        g.live_inputs = [0; 14];
        g.draw_sink = Some(Vec::new());
        self.frame = view::build_frame(g, level2);
        Ok(())
    }

    /// The running game is a film replay.
    pub fn is_replay(&self) -> bool {
        self.replay
    }

    /// Scale each player's top speed along x and y for the ticks that
    /// follow (see [`G::live_move_scale`]). Film replays ignore it.
    pub fn set_move_scale(&mut self, scale: [[f32; 2]; 2]) {
        self.g.live_move_scale = scale;
    }

    /// One pass of `G_Game_Play`'s loop. `inputs[player]` is [up, right,
    /// down, left, ground weapon, air weapon, cycle]. Does nothing when the
    /// game is not running.
    pub fn tick(&mut self, inputs: [[bool; 7]; 2]) {
        if !self.started || !self.g.m.rb(PLAYING) {
            return;
        }
        let g = &mut self.g;
        if self.replay {
            g.live_move_scale = [[1.0; 2]; 2];
        }
        for p in 0..2 {
            for k in 0..7 {
                g.live_inputs[p * 7 + k] = inputs[p][k] as u8;
            }
        }
        // 0x4230f0 clears the render lists at the start of the frame.
        g.draw_sink = Some(Vec::new());
        // The music start (replays and live play alike set the flag).
        if g.m.r8(FLAG_4854) == 0 {
            let start = pf_int(g, 0x12);
            if g.m.ri32(GAME_TIME) == start {
                g.m.w8(FLAG_4854, 1);
            }
        }
        let proc_a = self.flags;
        let proc_c = self.flags + 2;
        if self.replay {
            // `play_film`'s loop body: a replay stops when the money
            // counters are done or the film is.
            sub_420280(g, proc_a, 1, proc_c, self.film);
            if g.m.r8(MONEY_DONE) != 0 {
                g.m.w8(PLAYING, 0);
            }
            let t = g.m.ri32(GAME_TIME).wrapping_add(1);
            g.m.wi32(GAME_TIME, t);
            if G_Film_IsFinished(g, self.film) {
                g.m.w8(PLAYING, 0);
            }
        } else {
            sub_420280(g, proc_a, 0, proc_c, self.film);
            sub_4207f0_live(g, self.level);
            let t = g.m.ri32(GAME_TIME).wrapping_add(1);
            g.m.wi32(GAME_TIME, t);
        }
        self.frame = view::build_frame(g, self.level);
    }

    /// What the original would draw for the last tick (or for the start of
    /// the level before the first tick).
    pub fn frame(&self) -> &FrameView {
        &self.frame
    }

    // --- state queries -------------------------------------------------

    /// The game is running (`[0x4e4824]`); false before `start_game` and
    /// after the game ended.
    pub fn is_playing(&self) -> bool {
        self.started && self.g.m.rb(PLAYING)
    }
    /// A game was started and has stopped running.
    pub fn is_ended(&self) -> bool {
        self.started && !self.g.m.rb(PLAYING)
    }
    pub fn end_reason(&self) -> Option<EndReason> {
        if !self.is_ended() {
            return None;
        }
        Some(if self.g.m.rb(NO_PLAYER_IN_GAME) { EndReason::GameOver } else { EndReason::Completed })
    }
    /// `G_Game_GameTime_Get`: ticks since the level start (the next tick
    /// runs at this time).
    pub fn game_time(&self) -> i32 {
        self.g.m.ri32(GAME_TIME)
    }
    /// The level's end has been reached (`G_Game_IsWaitingForLevelToEnd`):
    /// the map is scrolled to the end; the end-of-level sequence, tally and
    /// money counters run until the next level starts.
    pub fn is_level_complete(&self) -> bool {
        self.g.m.rb(WAITING_FOR_LEVEL_END)
    }
    /// The end-of-level objects are spawned and the tally has begun.
    pub fn end_sequence_started(&self) -> bool {
        self.g.m.rb(END_SEQUENCE_STARTED)
    }
    /// The money counters finished (next level / end of game follows).
    pub fn money_done(&self) -> bool {
        self.g.m.rb(MONEY_DONE)
    }
    /// All players are out of the game (`[0x4e4826]`): game over.
    pub fn is_game_over(&self) -> bool {
        self.g.m.rb(NO_PLAYER_IN_GAME)
    }
    pub fn is_final_level(&self) -> bool {
        self.g.m.rb(FINAL_LEVEL)
    }
    /// `G_Game_GetLevelID`: the four-character level id as a little-endian
    /// u32 (`'none'` once the last level is done).
    pub fn level_id(&self) -> u32 {
        self.g.m.r32(LEVEL_ID)
    }
    /// `G_Game_GetCurrentLevelNumber`: 1-based position in the level list.
    pub fn level_number(&self) -> i32 {
        self.g.m.ri32(CURRENT_LEVEL)
    }
    /// How many levels are played in this game so far (including the
    /// current one).
    pub fn levels_played(&self) -> i32 {
        self.g.m.ri32(LEVELS_PLAYED)
    }
    pub fn num_levels(&mut self) -> i32 {
        level::G_Level_GetNumInLevelList(&mut self.g)
    }
    /// The `G_Level` of the current level in memory (`+0x70` background
    /// image id, ...).
    pub fn level_addr(&self) -> u32 {
        self.level
    }
    /// The end-of-level tally text (`sub_420d90`'s buffer), empty when
    /// there is none.
    pub fn end_of_level_text(&self) -> String {
        String::from_utf8_lossy(self.g.m.cstr(EOL_TEXT)).into_owned()
    }

    pub fn player(&mut self, i: usize) -> PlayerInfo {
        let g = &mut self.g;
        let p = g.m.r32(PLAYERS + (i as u32) * 4);
        if p == 0 {
            return PlayerInfo::default();
        }
        PlayerInfo {
            in_game: G_Player_InGame(g, p),
            state: G_Player_GetState(g, p),
            x: g.m.rf32(p),
            y: g.m.rf32(p + 4),
            score: G_Player_Score_Get(g, p),
            lives: G_Player_Lives_Get(g, p),
            shields_pct: G_Player_Shields_GetPercentage(g, p),
            money: G_Player_Money_Get(g, p),
            multiplier: G_Player_Multiplier_Get(g, p),
            invulnerable: G_Player_IsInvulnerable(g, p),
            money_counter_active: G_Player_MoneyCounter_IsActive(g, p),
        }
    }
}

/// `sub_4207f0` (0x4207f0) for a live game: once the money counters are
/// done (`[0x4e4825]`), start the next level while a player is still in
/// the game, else stop. The music and `G_GameInterface::ResetAtLevelStart`
/// (0x423090) are not ported.
fn sub_4207f0_live(g: &mut G, level: u32) {
    if !g.m.rb(MONEY_DONE) {
        return;
    }
    let mut any = false;
    for i in 0..2u32 {
        let p = g.m.r32(PLAYERS + i * 4);
        if G_Player_InGame(g, p) {
            any = true;
            break;
        }
    }
    if !any {
        g.m.w8(PLAYING, 0);
        return;
    }
    let id = G_Res_GetPermSoundID(g, 3);
    sound_default(g, id, 0x4b, 0x64);
    g.m.w8(MONEY_DONE, 0);
    g.m.w8(WAITING_FOR_LEVEL_END, 0);
    sub_41fc80(g, false, level as i32);
}
