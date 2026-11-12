//! A two-player game on the port as a training environment: each step
//! holds both players' actions for `repeat` ticks (the bot decides at a
//! human-like rate, not every tick) and reports what happened.

use std::path::Path;

use super::obs::{own_knowledge, Action, Latch, Obs, Tracker};
use crate::game::og::session::{EndReason, Session};

/// Floats of per-env info: 8 per player then 9 for the game.
pub const INFO: usize = 25;

pub struct EnvConfig {
    /// Ticks per decision.
    pub repeat: u32,
    /// Episode length cap in ticks.
    pub max_ticks: u32,
    /// Levels an episode may complete before it ends.
    pub max_levels: i32,
    /// Starting levels to pick from (1-based list positions).
    pub levels: Vec<i32>,
    /// Decisions the observation lags behind the game (reaction time: with
    /// 3 ticks per decision at 30 ticks a second, 1 means the bot acts on
    /// what the screen showed 100 ms before).
    pub delay: usize,
}

pub struct Env {
    pub s: Session,
    exe: Vec<u8>,
    paks: std::path::PathBuf,
    tracker: Tracker,
    rng: u64,
    pub prev: [Action; 2],
    ticks: u32,
    start_level: i32,
    start_played: i32,
    pub obs: [Obs; 2],
    hist: std::collections::VecDeque<[Obs; 2]>,
    delay: usize,
    pub info: [f32; INFO],
    pub latch: [Latch; 2],
}

#[derive(Clone, Copy, Default)]
struct PStat {
    score: i32,
    lives: i32,
    shields: f32,
    state: i32,
    in_game: bool,
    money: i32,
}

impl Env {
    pub fn new(exe: &[u8], paks: &Path, seed: u64) -> Result<Env, String> {
        let s = Session::new(exe, paks)?;
        Ok(Env {
            s,
            exe: exe.to_vec(),
            paks: paks.to_path_buf(),
            tracker: Tracker::default(),
            rng: seed.wrapping_mul(0x9e3779b97f4a7c15) | 1,
            prev: [Action::default(); 2],
            ticks: 0,
            start_level: 1,
            start_played: 0,
            obs: [Obs::default(), Obs::default()],
            hist: Default::default(),
            delay: 0,
            info: [0.0; INFO],
            latch: [Latch::default(); 2],
        })
    }

    fn next_rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// Starts a new two-player game on a level drawn from `cfg.levels`.
    pub fn reset(&mut self, cfg: &EnvConfig) -> Result<(), String> {
        // The port's heap only grows when its free lists run dry; start over
        // with a fresh session long before it fills.
        if self.s.g.m.heap_used() > 40 << 20 {
            self.s = Session::new(&self.exe, &self.paks)?;
        }
        let r = self.next_rand();
        let level = cfg.levels[(r % cfg.levels.len() as u64) as usize];
        let seed = (self.next_rand() >> 16) as u32;
        self.s.start_game(2, level, seed)?;
        // A game started past the first level gets a single life (the
        // original's rule for level select); training gets the full
        // allowance everywhere so later levels are seen as often.
        if level != 1 {
            for i in 0..2u32 {
                let p = self.s.g.m.r32(crate::game::og::game::PLAYERS + i * 4);
                let def = self.s.g.m.r32(p + 0x8a);
                let n = self.s.g.m.ri32(def + 0x62);
                crate::game::og::init::G_Player_Lives_Set(&mut self.s.g, p, n);
            }
        }
        self.start_level = level;
        self.start_played = self.s.levels_played();
        self.ticks = 0;
        self.prev = [Action::default(); 2];
        self.tracker.reset();
        self.hist.clear();
        self.delay = cfg.delay;
        self.latch = [Latch::default(); 2];
        self.observe();
        self.info = [0.0; INFO];
        self.fill_info(&[PStat::default(); 2], [0; 2], [0.0; 2], 0.0);
        Ok(())
    }

    fn pstat(&mut self, i: usize) -> PStat {
        let p = self.s.player(i);
        PStat { score: p.score, lives: p.lives, shields: p.shields_pct, state: p.state, in_game: p.in_game, money: p.money }
    }

    pub fn observe(&mut self) {
        self.tracker.update(&self.s);
        let prev = self.prev;
        let mut pair = if self.hist.len() > self.delay {
            self.hist.pop_front().unwrap()
        } else {
            [Obs::default(), Obs::default()]
        };
        self.tracker.observe(&mut self.s, 0, prev[0], &mut pair[0]);
        self.tracker.observe(&mut self.s, 1, prev[1], &mut pair[1]);
        self.hist.push_back(pair);
        // The newest observation is at the back; the bot sees the one
        // `delay` decisions older (or the oldest there is).
        let front = &self.hist[0];
        self.obs[0].clone_from(&front[0]);
        self.obs[1].clone_from(&front[1]);
        // The previous-action features are the bot's own knowledge, not
        // something it sees late.
        for p in 0..2 {
            own_knowledge(&mut self.obs[p].selfv, prev[p], self.latch[p].on, 0.0);
        }
    }

    /// One decision: `actions` held for `cfg.repeat` ticks. `learners`
    /// says which players are driven by the policy being trained: the
    /// episode ends when none of them is left in the game. Returns whether
    /// the episode ended (the env is then already reset and `info` holds
    /// the final step's numbers with the done flag set).
    pub fn step(&mut self, cfg: &EnvConfig, actions: [Action; 2], learners: [bool; 2]) -> Result<bool, String> {
        let mut deaths = [0i32; 2];
        let mut damage = [0f32; 2];
        let before = [self.pstat(0), self.pstat(1)];
        let mut last = before;
        let mut actions = actions;
        for i in 0..2 {
            self.latch[i].decide(&mut actions[i]);
        }
        let base = [actions[0].og(), actions[1].og()];
        let mut level_done = 0.0;
        let was_complete = self.s.is_level_complete();
        for _ in 0..cfg.repeat {
            let mut inputs = base;
            for i in 0..2 {
                if let Some(hold) = self.latch[i].tick(&self.s, i) {
                    inputs[i][5] = hold;
                }
            }
            self.s.tick(inputs);
            self.ticks += 1;
            for i in 0..2 {
                let now = self.pstat(i);
                if now.state == 3 && last[i].state != 3 {
                    deaths[i] += 1;
                }
                if now.shields < last[i].shields && now.state == 4 {
                    damage[i] += last[i].shields - now.shields;
                }
                last[i] = now;
            }
            if self.s.is_ended() {
                break;
            }
        }
        if self.s.is_level_complete() && !was_complete {
            level_done = 1.0;
        }
        self.prev = actions;
        let levels_done = self.s.levels_played() - self.start_played;
        let mut end = 0.0;
        if self.s.is_ended() {
            end = match self.s.end_reason() {
                Some(EndReason::Completed) => 2.0,
                _ => 1.0,
            };
        } else if self.ticks >= cfg.max_ticks || levels_done >= cfg.max_levels {
            end = 3.0;
        } else if (learners[0] || learners[1]) && !((learners[0] && last[0].in_game) || (learners[1] && last[1].in_game)) {
            end = 1.0;
        }
        self.fill_info(&before, deaths, damage, level_done);
        if end > 0.0 {
            self.info[19] = end;
            self.reset_keep_info(cfg)?;
            return Ok(true);
        }
        self.observe();
        Ok(false)
    }

    fn reset_keep_info(&mut self, cfg: &EnvConfig) -> Result<(), String> {
        let info = self.info;
        self.reset(cfg)?;
        self.info = info;
        Ok(())
    }

    fn fill_info(&mut self, before: &[PStat; 2], deaths: [i32; 2], damage: [f32; 2], level_done: f32) {
        for i in 0..2 {
            let p = self.pstat(i);
            let o = i * 8;
            self.info[o] = (p.score - before[i].score) as f32;
            self.info[o + 1] = p.lives as f32;
            self.info[o + 2] = p.shields;
            self.info[o + 3] = p.state as f32;
            self.info[o + 4] = p.in_game as i32 as f32;
            self.info[o + 5] = deaths[i] as f32;
            self.info[o + 6] = damage[i];
            self.info[o + 7] = p.score as f32;
        }
        self.info[16] = self.s.level_number() as f32;
        self.info[17] = (self.s.levels_played() - self.start_played) as f32;
        self.info[18] = level_done;
        self.info[19] = 0.0;
        self.info[20] = self.s.game_time() as f32;
        self.info[21] = self.ticks as f32;
        self.info[22] = self.start_level as f32;
        let f = self.s.frame();
        let b = &f.background;
        self.info[23] = if b.height > 0 { 1.0 - b.scroll_y as f32 / b.height as f32 } else { 0.0 };
        self.info[24] = self.s.is_level_complete() as i32 as f32;
    }
}
