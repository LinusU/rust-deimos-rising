//! Drives one ship of a live game with the trained policy, the way the
//! training environment does: a decision every `REPEAT` ticks, from what
//! the screen showed `DELAY` decisions earlier.

use std::collections::VecDeque;

use super::obs::{Action, Obs, Tracker};
use super::policy::Policy;
use crate::game::og::session::Session;

/// Ticks per decision (tools/rl trains with 3: ten decisions a second).
pub const REPEAT: u32 = 3;
/// Decisions the observation lags behind (the server's `--obs-delay`).
pub const DELAY: usize = 1;

pub struct CoPilot {
    pub policy: Policy,
    /// The ship it flies (0 = player 1, 1 = player 2).
    pub slot: usize,
    /// Sampling temperature: 1 plays as trained, lower is steadier, higher
    /// sloppier.
    pub temperature: f32,
    tracker: Tracker,
    hist: VecDeque<Obs>,
    ticks: u32,
    action: Action,
    rng: u64,
}

impl CoPilot {
    pub fn new(policy: Policy, slot: usize, temperature: f32, seed: u64) -> CoPilot {
        CoPilot {
            policy,
            slot,
            temperature,
            tracker: Tracker::default(),
            hist: VecDeque::new(),
            ticks: 0,
            action: Action::default(),
            rng: seed | 1,
        }
    }

    /// A new game starts.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.hist.clear();
        self.ticks = 0;
        self.action = Action::default();
    }

    /// The ship's controls for the next tick, in the port's order.
    pub fn input(&mut self, s: &mut Session) -> [bool; 7] {
        if self.ticks % REPEAT == 0 {
            self.tracker.update(s);
            let mut o = if self.hist.len() > DELAY { self.hist.pop_front().unwrap() } else { Obs::default() };
            self.tracker.observe(s, self.slot, self.action, &mut o);
            self.hist.push_back(o);
            let mut seen = self.hist[0].clone();
            // Its own last action is known at once (as in training).
            let v = &mut seen.selfv;
            for i in 34..46 {
                v[i] = 0.0;
            }
            v[34 + self.action.dir.min(8) as usize] = 1.0;
            v[43] = self.action.air as i32 as f32;
            v[44] = self.action.ground as i32 as f32;
            v[45] = self.action.cycle as i32 as f32;
            let mut rng = self.rng;
            let mut r = || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                (rng >> 40) as f32 / (1u64 << 24) as f32
            };
            self.action = self.policy.act(&seen, self.temperature, &mut r);
            self.rng = rng;
        }
        self.ticks += 1;
        self.action.og()
    }
}
