//! Drives one ship of a live game with the trained policy, the way the
//! training environment does: a decision every `REPEAT` ticks, from what
//! the screen showed `DELAY` decisions earlier.

use std::collections::VecDeque;

use super::obs::{own_knowledge, Action, Latch, Obs, Tracker};
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
    latch: Latch,
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
            latch: Latch::default(),
            rng: seed | 1,
        }
    }

    /// A new game starts.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.hist.clear();
        self.ticks = 0;
        self.action = Action::default();
        self.latch = Latch::default();
    }

    /// The ship's controls for the next tick, in the port's order.
    pub fn input(&mut self, s: &mut Session) -> [bool; 7] {
        if self.ticks % REPEAT == 0 {
            self.tracker.update(s);
            let mut o = if self.hist.len() > DELAY { self.hist.pop_front().unwrap() } else { Obs::default() };
            self.tracker.observe(s, self.slot, self.action, &mut o);
            self.hist.push_back(o);
            let mut seen = self.hist[0].clone();
            // Its own last action is known at once (as in training); it
            // plays the support role.
            own_knowledge(&mut seen.selfv, self.action, self.latch.on, 0.0);
            let mut rng = self.rng;
            let mut r = || {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                (rng >> 40) as f32 / (1u64 << 24) as f32
            };
            let mut a = self.policy.act(&seen, self.temperature, &mut r);
            self.rng = rng;
            self.latch.decide(&mut a);
            self.action = a;
        }
        self.ticks += 1;
        let mut i = self.action.og();
        if let Some(hold) = self.latch.tick(s, self.slot) {
            i[5] = hold;
        }
        i
    }
}
