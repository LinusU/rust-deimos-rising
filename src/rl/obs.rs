//! What the bot sees: built only from what the game draws on screen and
//! what the score bar shows, never from hidden state (hit points, spawn
//! tables, timers).
//!
//! Per player the observation is
//! - a set of up to [`K`] on-screen objects (one token per game object that
//!   drew something this frame), nearest to the player's ship first, each
//!   with its sprite id (for a learned embedding: the bot learns what a
//!   sprite means the way a person does, by seeing what it does) and
//!   [`F`] floats: position relative to the ship and on the screen, motion
//!   since the previous observation (an object followed from frame to
//!   frame, as the eye does), facing (animation frame), size, layer,
//!   translucency and hit flash;
//! - [`S`] floats about the player itself, its partner and the level, as
//!   the score bar and the screen show them, plus its own previous action;
//! - the three air weapon slots on the score bar (current, next, the one
//!   after), each embedded by its picture (sprite and frame).

use std::collections::HashMap;

use crate::game::og::session::Session;
use crate::game::og::view::DrawKind;

/// Objects per observation.
pub const K: usize = 64;
/// Floats per object.
pub const F: usize = 18;
/// Floats about the player.
pub const S: usize = 50;
/// Ids per player besides the objects (the air weapon slots).
pub const SELF_IDS: usize = 3;

/// The game area in front-buffer pixels.
pub const VIEW_W: f32 = 416.0;
pub const VIEW_H: f32 = 480.0;

/// The id buckets a sprite id is embedded with: two independent hashes
/// into tables of [`ID_BUCKETS`] (an id collides only when it collides in
/// both). 0 means "no object".
pub const ID_BUCKETS: u32 = 1024;

fn mix(mut x: u32, k: u32) -> u32 {
    x ^= k;
    x = x.wrapping_mul(0x9e37_79b1);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85eb_ca77);
    x ^= x >> 13;
    x
}

/// Both buckets of a sprite id, each in `1..ID_BUCKETS`; `(0, 0)` for none.
pub fn id_buckets(sprite: u32) -> (i32, i32) {
    if sprite == 0 || sprite == crate::game::og::view::NONE {
        return (0, 0);
    }
    let a = mix(sprite, 0x1234_5678) % (ID_BUCKETS - 1) + 1;
    let b = mix(sprite, 0x8765_4321) % (ID_BUCKETS - 1) + 1;
    (a as i32, b as i32)
}

/// One player's action: a direction (0 none, 1 up, then clockwise in
/// eighths to 8 up-left), three buttons and `charge`: hold air fire until
/// the charge is full, then let go (see [`Latch`]).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Action {
    pub dir: u8,
    pub air: bool,
    pub ground: bool,
    pub cycle: bool,
    pub charge: bool,
}

impl Action {
    /// From the packed byte: bits 0..3 direction, 4 air, 5 ground, 6 cycle,
    /// 7 charge.
    pub fn from_byte(b: u8) -> Action {
        Action { dir: (b & 15).min(8), air: b & 16 != 0, ground: b & 32 != 0, cycle: b & 64 != 0, charge: b & 128 != 0 }
    }
    pub fn to_byte(self) -> u8 {
        self.dir.min(8) | (self.air as u8) << 4 | (self.ground as u8) << 5 | (self.cycle as u8) << 6 | (self.charge as u8) << 7
    }
    /// The port's input order: up, right, down, left, ground, air, cycle.
    pub fn og(self) -> [bool; 7] {
        let (up, right, down, left) = match self.dir {
            1 => (true, false, false, false),
            2 => (true, true, false, false),
            3 => (false, true, false, false),
            4 => (false, true, true, false),
            5 => (false, false, true, false),
            6 => (false, false, true, true),
            7 => (false, false, false, true),
            8 => (true, false, false, true),
            _ => (false, false, false, false),
        };
        [up, right, down, left, self.ground, self.air, self.cycle]
    }
    /// Direction from held keys (opposite keys cancel), for recording a
    /// person's play in the same terms.
    pub fn from_og(i: [bool; 7]) -> Action {
        let dy = i[2] as i32 - i[0] as i32;
        let dx = i[1] as i32 - i[3] as i32;
        let dir = match (dx, dy) {
            (0, -1) => 1,
            (1, -1) => 2,
            (1, 0) => 3,
            (1, 1) => 4,
            (0, 1) => 5,
            (-1, 1) => 6,
            (-1, 0) => 7,
            (-1, -1) => 8,
            _ => 0,
        };
        Action { dir, ground: i[4], air: i[5], cycle: i[6], charge: false }
    }
}

/// The charge order: air fire held from the decision that gives it until
/// the charge is full (the power bar on the score bar reads 100%), then
/// let go so the charge goes off. A person does the same, watching the
/// bar; a decision every 100 ms could hardly hold a button for the ~2 s a
/// charge takes by chance. Holding past full would overload the ship.
#[derive(Clone, Copy, Default, Debug)]
pub struct Latch {
    pub on: bool,
    ticks: u32,
    /// Charges let go when full (for the trainer's statistics).
    pub released: u32,
}

impl Latch {
    /// At a decision: a charge order starts the hold. While one runs the
    /// decision's air choices are ignored.
    pub fn decide(&mut self, a: &mut Action) {
        if self.on {
            a.air = false;
            a.charge = false;
        } else if a.charge {
            self.on = true;
            self.ticks = 0;
            a.air = false;
        }
    }

    /// Per tick: whether air fire is held, or `None` when no charge runs.
    pub fn tick(&mut self, s: &Session, me: usize) -> Option<bool> {
        if !self.on {
            return None;
        }
        let pobj = s.g.m.r32(crate::game::og::game::PLAYERS + me as u32 * 4);
        // `G_Player` +0xba: the ship's state (4 = flying).
        if s.g.m.ri32(pobj + 0xba) != 4 {
            self.on = false;
            return Some(false);
        }
        let h = pobj + 0x235;
        let state = s.g.m.r32(h + 0x11);
        let pct = s.g.m.rf32(h + 0x25);
        self.ticks += 1;
        if state == 1 && pct >= 100.0 {
            self.on = false;
            self.released += 1;
            return Some(false);
        }
        if state == 2 || self.ticks > 90 {
            self.on = false;
            return Some(false);
        }
        Some(true)
    }
}

/// The player's own knowledge, known at once rather than seen late: its
/// previous action, whether a charge order is running, and its role
/// (0 support: the co-pilot; 1 lead).
pub fn own_knowledge(v: &mut [f32], prev: Action, latch_on: bool, role: f32) {
    for x in v[34..46].iter_mut() {
        *x = 0.0;
    }
    v[34 + prev.dir.min(8) as usize] = 1.0;
    v[43] = (prev.air || prev.charge) as i32 as f32;
    v[44] = prev.ground as i32 as f32;
    v[45] = prev.cycle as i32 as f32;
    v[48] = latch_on as i32 as f32;
    v[49] = role;
}

/// One player's observation.
#[derive(Clone)]
pub struct Obs {
    pub ent: Vec<f32>,     // K * F
    pub ent_id: Vec<i32>,  // K * 2
    pub selfv: Vec<f32>,   // S
    pub self_id: Vec<i32>, // SELF_IDS * 2
    /// The weapon slots as observation version 1 embedded them (the
    /// sprite alone: the same for every weapon), for older policies.
    pub self_id_v1: Vec<i32>,
}

impl Default for Obs {
    fn default() -> Self {
        Obs {
            ent: vec![0.0; K * F],
            ent_id: vec![0; K * 2],
            selfv: vec![0.0; S],
            self_id: vec![0; SELF_IDS * 2],
            self_id_v1: vec![0; SELF_IDS * 2],
        }
    }
}

/// An object as drawn this frame.
#[derive(Clone, Copy)]
struct Seen {
    obj: u32,
    sprite: u32,
    frame: i32,
    x: f32,
    y: f32,
    layer: i32,
    scale: f32,
    vis: f32,
    flash: f32,
    w: f32,
    h: f32,
    nframes: i32,
}

/// Follows objects between observations to give them a motion.
#[derive(Default)]
pub struct Tracker {
    prev: HashMap<u32, (u32, f32, f32)>,
    prev_time: i32,
    seen: Vec<Seen>,
    vel: Vec<(f32, f32, bool)>,
}

impl Tracker {
    pub fn reset(&mut self) {
        self.prev.clear();
        self.prev_time = 0;
    }

    /// Collects this frame's objects and their motion; call once per
    /// observation, before [`Tracker::observe`] for each player.
    pub fn update(&mut self, s: &Session) {
        let f = s.frame();
        let now = s.game_time();
        let dt = (now - self.prev_time).max(1) as f32;
        self.seen.clear();
        let mut index: HashMap<u32, usize> = HashMap::new();
        for it in &f.items {
            if it.to_terrain || it.layer <= 1 || it.kind == DrawKind::Shadow || it.obj == 0 {
                continue;
            }
            let x = it.x as f32;
            let y = it.y as f32;
            if x < -48.0 || x > VIEW_W + 48.0 || y < -48.0 || y > VIEW_H + 48.0 {
                continue;
            }
            let flash = if it.kind == DrawKind::Flash { (it.tint_pct / 100.0).clamp(0.0, 1.0) } else { 0.0 };
            if let Some(&i) = index.get(&it.obj) {
                let e = &mut self.seen[i];
                e.flash = e.flash.max(flash);
                if it.kind == DrawKind::Body {
                    e.sprite = it.sprite;
                    e.frame = it.frame;
                    e.x = x;
                    e.y = y;
                    e.layer = it.layer;
                }
                continue;
            }
            let (w, h) = s.g.host.sprite_frame_size(it.sprite, it.frame).unwrap_or((16, 16));
            let nframes = s.g.host.sprite_num_frames(it.sprite).max(1);
            index.insert(it.obj, self.seen.len());
            self.seen.push(Seen {
                obj: it.obj,
                sprite: it.sprite,
                frame: it.frame,
                x,
                y,
                layer: it.layer,
                scale: it.scale,
                vis: it.visibility_pct / 100.0,
                flash,
                w: w as f32 * it.scale,
                h: h as f32 * it.scale,
                nframes,
            });
        }
        self.vel.clear();
        for e in &self.seen {
            match self.prev.get(&e.obj) {
                Some(&(sp, px, py)) if sp == e.sprite && (e.x - px).abs() < 96.0 && (e.y - py).abs() < 96.0 => {
                    self.vel.push(((e.x - px) / dt, (e.y - py) / dt, false));
                }
                _ => self.vel.push((0.0, 0.0, true)),
            }
        }
        self.prev.clear();
        for e in &self.seen {
            self.prev.insert(e.obj, (e.sprite, e.x, e.y));
        }
        self.prev_time = now;
    }

    /// Player `me`'s observation of the frame collected by `update`.
    pub fn observe(&self, s: &mut Session, me: usize, prev_action: Action, out: &mut Obs) {
        let other = 1 - me;
        let pa = s.player(me);
        let pb = s.player(other);
        let (bars, background) = {
            let f = s.frame();
            (f.score_bars.clone(), f.background.clone())
        };
        let pobj = s.g.m.r32(crate::game::og::game::PLAYERS + me as u32 * 4);
        let qobj = s.g.m.r32(crate::game::og::game::PLAYERS + other as u32 * 4);
        // The ship's position; while it is not on screen, where it comes
        // back in.
        let (sx, sy) = if pa.state == 4 { (pa.x, pa.y) } else { (if me == 0 { 104.0 } else { 312.0 }, 330.0) };

        let mut order: Vec<(f32, usize)> = Vec::with_capacity(self.seen.len());
        for (i, e) in self.seen.iter().enumerate() {
            if e.obj == pobj {
                continue;
            }
            let dx = e.x - sx;
            let dy = e.y - sy;
            order.push((dx * dx + dy * dy, i));
        }
        order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        out.ent.iter_mut().for_each(|v| *v = 0.0);
        out.ent_id.iter_mut().for_each(|v| *v = 0);
        let n = order.len().min(K);
        for (k, &(d2, i)) in order.iter().take(K).enumerate() {
            let e = &self.seen[i];
            let (vx, vy, new) = self.vel[i];
            let o = &mut out.ent[k * F..(k + 1) * F];
            let ang = std::f32::consts::TAU * e.frame as f32 / e.nframes as f32;
            o[0] = (e.x - sx) / 208.0;
            o[1] = (e.y - sy) / 240.0;
            o[2] = e.x / VIEW_W;
            o[3] = e.y / VIEW_H;
            o[4] = (vx / 8.0).clamp(-4.0, 4.0);
            o[5] = (vy / 8.0).clamp(-4.0, 4.0);
            o[6] = new as i32 as f32;
            o[7] = d2.sqrt() / 300.0;
            o[8] = ang.sin();
            o[9] = ang.cos();
            o[10] = (e.nframes as f32 / 64.0).min(2.0);
            o[11] = e.layer as f32 / 15.0;
            o[12] = e.vis;
            o[13] = e.flash;
            o[14] = (e.obj == qobj) as i32 as f32;
            o[15] = (e.w / 64.0).min(4.0);
            o[16] = (e.h / 64.0).min(4.0);
            o[17] = 1.0; // present
            let (a, b) = id_buckets(e.sprite);
            out.ent_id[k * 2] = a;
            out.ent_id[k * 2 + 1] = b;
        }

        let v = &mut out.selfv;
        v.iter_mut().for_each(|x| *x = 0.0);
        let bar = &bars[me];
        let obar = &bars[other];
        v[0] = sx / VIEW_W;
        v[1] = sy / VIEW_H;
        v[2 + pa.state.clamp(0, 4) as usize] = 1.0;
        v[7] = bar.shield_pct / 100.0;
        v[8] = bar.power_pct / 100.0;
        v[9] = bar.lives.max(0) as f32 / 5.0;
        v[10] = pa.invulnerable as i32 as f32;
        v[11] = pa.in_game as i32 as f32;
        for w in 0..3 {
            // Every air weapon shares one score bar sprite; the frame
            // tells them apart.
            let (id, frame) = bar.weapons[w];
            let present = id != 0 && id != crate::game::og::view::NONE;
            v[12 + w] = present as i32 as f32;
            v[15 + w] = if present { (frame as f32 / 4.0).clamp(-1.0, 4.0) } else { 0.0 };
            let (a, b) = if present { id_buckets(id ^ (frame as u32 + 1).wrapping_mul(0x9e37_79b9)) } else { (0, 0) };
            out.self_id[w * 2] = a;
            out.self_id[w * 2 + 1] = b;
            let (a, b) = if present { id_buckets(id) } else { (0, 0) };
            out.self_id_v1[w * 2] = a;
            out.self_id_v1[w * 2 + 1] = b;
        }
        v[18] = pb.in_game as i32 as f32;
        v[19] = (pb.x - sx) / 208.0;
        v[20] = (pb.y - sy) / 240.0;
        v[21 + pb.state.clamp(0, 4) as usize] = 1.0;
        v[26] = obar.shield_pct / 100.0;
        v[27] = obar.lives.max(0) as f32 / 5.0;
        v[28] = pa.multiplier as f32 / 10.0;
        let nl = 9.0;
        v[29] = s.level_number() as f32 / nl;
        v[30] = if background.height > 0 { 1.0 - background.scroll_y as f32 / background.height as f32 } else { 0.0 };
        v[31] = s.is_level_complete() as i32 as f32;
        v[32] = me as f32;
        v[33] = pa.money_counter_active as i32 as f32;
        own_knowledge(v, prev_action, false, 0.0);
        v[46] = n as f32 / K as f32;
        v[47] = (s.game_time() as f32 / 6000.0).min(3.0);
    }
}
