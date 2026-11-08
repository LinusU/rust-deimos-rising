//! Core game simulation — a fixed-timestep (60 Hz) port of the original's
//! entity/unit model. Pure Rust, no Bevy types, so it can be tested headless.
//!
//! Coordinate model (reverse-engineered from G_Background/G_Entity/G_Player):
//! - World X: terrain pixels, 0..480 (the background image is 480 wide).
//! - World Y: distance north from the level's bottom edge, 0..level_height.
//!   `screen_y = scroll + 480 - world_y`.
//! - `scroll` advances ~1 px/tick while not paused; level ends at
//!   level_height - 480.
//! - Camera X (`cam_x`): side-scroll offset in -32..=31; `screen_x = world_x - cam_x`.
//! - Angles are compass degrees: 0 = north, 90 = east, 180 = south, 270 = west.
//!   Velocity: `vx = sin(a)*speed`, `vy = cos(a)*speed` (world space, +y north).

use crate::assets::GameAssets;
use crate::data::{LevelDef, PlayerDef, StateRule, UnitDef, UnitState};
use crate::pak::{id, ResId, NONE_ID};
use glam::Vec2;
use rand::Rng;
use rand::SeedableRng;

pub const TICK_HZ: f32 = 60.0;
pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 480.0;
/// Terrain width in world pixels.
pub const WORLD_W: f32 = 480.0;
/// Left edge of the 416-px playfield on screen.
pub const PLAY_LEFT: f32 = 32.0;
/// Right edge of the playfield on screen (PLAY_LEFT + VisibleGameWidth 416).
pub const PLAY_RIGHT: f32 = PLAY_LEFT + 416.0;
/// Player top game-area limit (perm float 0xb7 = Player_TopGameAreaLimit).
pub const PLAYER_TOP_LIMIT: f32 = 13.0;
/// Player bottom limit.
pub const PLAYER_BOTTOM_LIMIT: f32 = SCREEN_H - PLAYER_TOP_LIMIT;
/// Side-scroll clamp (G_Bgnd_AdjustSideScroll: -0x20..=0x1f).
pub const SIDE_SCROLL_MIN: f32 = -32.0;
pub const SIDE_SCROLL_MAX: f32 = 31.0;
/// Max live entities (G_EG_RequestSpawn caps at 0x3e8).
pub const MAX_ENTITIES: usize = 1000;
/// Ticks of scroll look-ahead for level-object spawning (original uses 0x40 px
/// of row lookahead; we spawn one screen ahead so they never pop in).
const SPAWN_AHEAD: f32 = SCREEN_H;

/// Perm-float indices used by the sim (positional into `fill_list`).
pub mod pf {
    pub const VISIBLE_GAME_WIDTH: usize = 0x36; // 416
    pub const VISIBLE_GAME_HEIGHT: usize = 0x37; // 480
    pub const PLAYER_TOP_LIMIT: usize = 0xb7; // 13
    pub const WEAPON_BASE_BOMBS: usize = 0x97;
    pub const WEAPON_MAX_BOMBS: usize = 0x98;
}

pub fn pf(assets: &GameAssets, i: usize) -> f32 {
    assets.fill_list.as_ref().map(|f| f.get(i) as f32).unwrap_or(0.0)
}

/// Convert a packed ResId to its four-character string for debugging.
pub fn rid_str(rid: ResId) -> String {
    let b = rid.to_le_bytes();
    b.iter().map(|&c| c as char).collect()
}

fn rand_range(rng: &mut impl Rng, min: f64, max: f64) -> f64 {
    if max <= min {
        min
    } else {
        rng.gen_range(min..=max)
    }
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

/// Runtime state for one spawned unit instance.
#[derive(Clone)]
pub struct Entity {
    /// Unit definition id (`unde` resource).
    pub unit: ResId,
    /// World position: x in 0..480, y = distance north from level bottom.
    pub pos: Vec2,
    /// World velocity, px/tick.
    pub vel: Vec2,
    /// Heading in degrees (0 = north).
    pub heading: f32,
    /// Index into `UnitDef::states`.
    pub state: usize,
    /// Ticks spent in the current state.
    pub state_time: u32,
    /// Rolled on_timer delay for the current state (ticks).
    pub state_timer_target: u32,
    /// Ticks since the entity spawned.
    pub age: u32,
    /// Current animation frame.
    pub frame: i64,
    /// Ticks until next animation frame advance.
    pub frame_timer: u32,
    /// Direction index last used for frame selection.
    pub dir_index: i64,
    /// Remaining shield hit points.
    pub shields: f64,
    /// Owning entity's `unique_num` (spawn sets / linkage), u32::MAX = none.
    /// Stored as a serial rather than an index because `entities` is
    /// compacted by `retain` every tick.
    pub owner: u32,
    /// Which player slot spawned this (for kill credit / friendly checks), -1 = none.
    pub spawner_player: i32,
    /// Per-spawn-set runtime timers for the current state.
    pub spawn_timers: Vec<SpawnSetRt>,
    /// Ticks remaining before this entity may be hit again.
    pub hit_cooldown: u32,
    /// Ticks of post-hit glow remaining.
    pub hit_flash: u32,
    /// Marks entities to be removed.
    pub dead: bool,
    /// Removed via the Destroy (destruct) path rather than Delete —
    /// distinguishes the two for owner-deletion propagation.
    pub destroyed: bool,
    /// Entity is homing/rotating toward a player target (rule condition
    /// "Is Tracking Player" — the original's `m_field_0xb5`).
    pub tracking: bool,
    /// Non-looping animation reached its last frame (rule condition
    /// "This Entity's Animation Has Stopped" — the original's `m_field_0xb6`).
    pub anim_stopped: bool,
    /// Unique serial for weapon-powerup release targeting.
    pub unique_num: u32,
    /// For weapon-class pickup units (pickup_type "air "/"grnd"/"spec"):
    /// the weapon def this pickup grants, resolved at spawn the way the
    /// original does (`GetNextRefPtrByType(type, NONE, level)` → first
    /// available weapon of the class at the current level).
    pub carried_weapon: ResId,
}

/// Per-spawn-set runtime state.
#[derive(Clone, Default)]
pub struct SpawnSetRt {
    pub next_volley: u32,
    pub volleys_left: u32,
    pub shots_left: u32,
    pub shot_timer: u32,
    pub started: bool,
    pub done: bool,
}

impl Entity {
    /// Screen-space position for rendering.
    pub fn screen_pos(&self, sim: &Sim) -> Vec2 {
        Vec2::new(self.pos.x - sim.cam_x, sim.scroll + SCREEN_H - self.pos.y)
    }
}

// ---------------------------------------------------------------------------
// Players
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerPhase {
    /// Waiting to enter.
    Entering,
    /// Normal active play.
    Active,
    /// Death animation playing.
    Dying,
    /// Out of lives.
    Dead,
}

pub struct Player {
    pub slot: usize,
    pub def_id: ResId,
    pub phase: PlayerPhase,
    /// Screen-space ship position (playfield coords).
    pub screen: Vec2,
    /// Screen-space velocity px/tick.
    pub vel: Vec2,
    /// Current tilt frame.
    pub tilt_frame: usize,
    pub lives: i64,
    pub score: i64,
    pub money: i64,
    /// Shield integrity 0..100.
    pub shields: f64,
    /// Weapon power meter 0..100.
    pub power: f64,
    /// Current air weapon def id.
    pub air_weapon: ResId,
    /// Current ground weapon def id.
    pub ground_weapon: ResId,
    /// Ticks until next shot allowed.
    pub fire_timer: u32,
    /// Ticks of invulnerability remaining.
    pub invuln: u32,
    /// Ticks in current phase.
    pub phase_time: u32,
    /// Bombs available.
    pub bombs: i64,
    /// Shield recharge delay after a hit.
    pub shield_recharge: u32,
    /// Score multiplier.
    pub multiplier: i64,
    /// Age in ticks.
    pub age: u32,
    /// Entry animation progress (spawn entity index if any).
    pub entry_entity: usize,
}

impl Player {
    /// World position used for collisions and spawning.
    pub fn world_pos(&self, sim: &Sim) -> Vec2 {
        Vec2::new(
            self.screen.x + sim.cam_x,
            sim.scroll + SCREEN_H - self.screen.y,
        )
    }
}

// ---------------------------------------------------------------------------
// Simulation
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Intro,
    Playing,
    LevelEnd,
    GameOver,
}

pub struct Sim {
    pub level: LevelDef,
    pub level_id: ResId,
    /// Scroll distance: world-y at screen bottom minus 480... see model docs.
    pub scroll: f32,
    /// Side-scroll camera offset.
    pub cam_x: f32,
    /// Vertical scroll paused.
    pub scroll_paused: bool,
    /// Set each tick when any live entity's state requests a scroll pause.
    entity_scroll_pause: bool,
    /// Pixels per tick the world scrolls.
    pub scroll_speed: f32,
    /// Level finished scrolling.
    pub level_complete: bool,
    /// Debug counters (hits landed / entities destroyed).
    pub stat_hits: u64,
    pub stat_kills: u64,
    /// Pickup-type entities spawned (diagnostic).
    pub stat_pickups: u64,
    /// Global game tick counter.
    pub game_time: u32,
    /// 1-based level number (weapon level-availability filtering).
    pub level_number: i64,
    pub players: [Option<Player>; 2],
    pub entities: Vec<Entity>,
    /// Indices into `level.objects` not yet spawned (sorted by world y).
    pending_objects: Vec<usize>,
    pub phase: Phase,
    /// Fixed-timestep accumulator (seconds).
    pub acc: f32,
    next_unique: u32,
    pub rng: rand::rngs::StdRng,
    spawn_queue: Vec<SpawnReq>,
    /// Deferred one-shot actions (e.g. spawn at pos next tick).
    pub events: Vec<SimEvent>,
}

#[derive(Clone, Debug)]
pub enum SimEvent {
    /// Play a sound (ResId of `soun`).
    Sound(ResId),
    /// A player died / extra life / etc. (slot, kind).
    PlayerEvent(usize, &'static str),
    /// Level finished.
    LevelComplete,
}

/// Internal spawn request.
#[derive(Clone)]
pub struct SpawnReq {
    pub unit: ResId,
    pub pos: Vec2,
    pub heading: Option<f32>,
    pub owner: usize,
    pub spawner_player: i32,
    pub stationary: bool,
    /// Force a specific state index (rare).
    pub state: Option<usize>,
}

impl Sim {
    pub fn new(level: LevelDef, level_id: ResId, seed: u64) -> Sim {
        let mut pending: Vec<usize> = (0..level.objects.len()).collect();
        pending.sort_by_key(|&i| level.objects[i].y);
        Sim {
            level,
            level_id,
            scroll: 0.0,
            cam_x: 0.0,
            scroll_paused: false,
            entity_scroll_pause: false,
            scroll_speed: 1.0,
            level_complete: false,
            stat_hits: 0,
            stat_kills: 0,
            stat_pickups: 0,
            game_time: 0,
            level_number: 1,
            players: [None, None],
            entities: Vec::new(),
            pending_objects: pending,
            phase: Phase::Intro,
            acc: 0.0,
            next_unique: 1,
            rng: rand::rngs::StdRng::seed_from_u64(seed),
            spawn_queue: Vec::new(),
            events: Vec::new(),
        }
    }

    /// World-space width/height of the level terrain.
    pub fn level_width(&self) -> f32 {
        (self.level.background[2] - self.level.background[0]).max(1) as f32
    }
    pub fn level_height(&self) -> f32 {
        (self.level.background[3] - self.level.background[1]).max(1) as f32
    }
    /// Max scroll value (camera reaches top of level).
    pub fn max_scroll(&self) -> f32 {
        (self.level_height() - SCREEN_H).max(0.0)
    }

    /// Add a player slot at level start.
    pub fn add_player(&mut self, assets: &GameAssets, slot: usize, two_player: bool) {
        let def_id = if slot == 0 { id("pl01") } else { id("pl02") };
        let def = assets.players.get(&def_id).cloned().unwrap_or_default();
        let (sx, sy) = if two_player {
            (def.entry_multi_start_x as f32, def.entry_multi_start_y as f32)
        } else {
            (def.entry_solo_start_x as f32, def.entry_solo_start_y as f32)
        };
        let max_bombs = pf(assets, pf::WEAPON_MAX_BOMBS) as i64;
        let base_bombs = pf(assets, pf::WEAPON_BASE_BOMBS) as i64;
        self.players[slot] = Some(Player {
            slot,
            def_id,
            phase: PlayerPhase::Entering,
            screen: Vec2::new(sx, sy),
            vel: Vec2::ZERO,
            tilt_frame: 0,
            lives: def.life_num_initial,
            score: 0,
            money: 0,
            shields: 0.0,
            power: 0.0,
            air_weapon: id("aiic"),  // Ion Cannon
            ground_weapon: id("plbo"), // Plasma Bomb
            fire_timer: 0,
            invuln: def.entry_invulnerability_time as u32,
            phase_time: 0,
            bombs: base_bombs.max(1).min(max_bombs.max(1)),
            shield_recharge: 0,
            multiplier: 1,
            age: 0,
            entry_entity: usize::MAX,
        });
        // Spawn the entry effect unit ("plen").
        if def.entry_spawn != NONE_ID {
            let p = self.players[slot].as_ref().unwrap();
            let w = p.world_pos(self);
            self.request_spawn(SpawnReq {
                unit: def.entry_spawn,
                pos: w,
                heading: None,
                owner: usize::MAX,
                spawner_player: slot as i32,
                stationary: false,
                state: None,
            });
        }
    }

    /// Queue a unit spawn (processed during step).
    pub fn request_spawn(&mut self, req: SpawnReq) {
        if self.entities.len() + self.spawn_queue.len() >= MAX_ENTITIES {
            return;
        }
        self.spawn_queue.push(req);
    }

    fn spawn_entity(&mut self, assets: &GameAssets, req: &SpawnReq) -> Option<usize> {
        let def = assets.units.get(&req.unit)?;
        if def.states.is_empty() {
            return None;
        }
        let state_idx = req.state.unwrap_or(0).min(def.states.len() - 1);
        let state = &def.states[state_idx];
        let speed = rand_range(&mut self.rng, def.initial_speed_min, def.initial_speed_max) as f32;
        let heading_base = req.heading.unwrap_or(def.initial_heading as f32);
        let tol = def.initial_heading_tolerance as f32;
        let heading = heading_base + rand_range(&mut self.rng, -(tol as f64), tol as f64) as f32;
        let hr = heading.to_radians();
        let vel = if req.stationary {
            Vec2::ZERO
        } else {
            Vec2::new(hr.sin() * speed, hr.cos() * speed)
        };
        let shields = if def.shields_max > 0.0 { def.shields_max } else { def.shields_base };
        // Roll the on_timer target once at spawn.
        let timer_target = if state.on_timer_max > 0 || state.on_timer_min > 0 {
            rand_range(&mut self.rng, state.on_timer_min as f64, state.on_timer_max.max(state.on_timer_min) as f64) as u32
        } else { 0 };
        let idx = self.entities.len();
        // req.owner is an entity index; store the stable unique_num.
        let owner_uid = if req.owner != usize::MAX && req.owner < self.entities.len() {
            self.entities[req.owner].unique_num
        } else {
            u32::MAX
        };
        self.entities.push(Entity {
            unit: req.unit,
            pos: req.pos,
            vel,
            heading,
            state: state_idx,
            state_time: 0,
            state_timer_target: timer_target,
            age: 0,
            frame: state.sprite_frame_min,
            frame_timer: 0,
            dir_index: -1,
            shields,
            owner: owner_uid,
            spawner_player: req.spawner_player,
            spawn_timers: vec![SpawnSetRt::default(); state.spawn_sets.len()],
            hit_cooldown: 0,
            hit_flash: 0,
            dead: false,
            destroyed: false,
            tracking: false,
            anim_stopped: false,
            unique_num: self.next_unique,
            carried_weapon: carried_weapon(assets, def, self.level_number),
        });
        self.next_unique = self.next_unique.wrapping_add(1);
        if def.pickup_type != NONE_ID {
            self.stat_pickups += 1;
        }
        Some(idx)
    }

    /// Fire a weapon's projectile pattern for a player.
    fn fire_weapon(&mut self, assets: &GameAssets, slot: usize) {
        let (weapon_id, pos) = {
            let p = match self.players[slot].as_ref() {
                Some(p) if p.phase == PlayerPhase::Active => p,
                _ => return,
            };
            (p.air_weapon, p.world_pos(self))
        };
        let wdef = match assets.weapons.get(&weapon_id) {
            Some(w) => w.clone(),
            None => return,
        };
        for sp in &wdef.spawns {
            if sp.unit_id == NONE_ID {
                continue;
            }
            // Weapon spawn offsets: x right, y up-screen = +world y.
            let mut pos = pos;
            pos.x += sp.x_loc as f32;
            pos.y += (-sp.y_loc) as f32;
            let heading = if sp.set_heading { Some(sp.angle as f32) } else { None };
            self.request_spawn(SpawnReq {
                unit: sp.unit_id,
                pos,
                heading,
                owner: usize::MAX,
                spawner_player: slot as i32,
                stationary: false,
                state: None,
            });
        }
        if let Some(p) = self.players[slot].as_mut() {
            p.fire_timer = wdef.delay_between_launches.max(1) as u32;
        }
    }

    /// Advance the simulation by one 60 Hz tick.
    pub fn step(&mut self, assets: &GameAssets, input: &PlayerInput) {
        self.game_time += 1;

        // --- scrolling ---
        if !self.scroll_paused && !self.entity_scroll_pause && self.phase == Phase::Playing {
            self.scroll += self.scroll_speed;
            if self.scroll >= self.max_scroll() {
                self.scroll = self.max_scroll();
                self.level_complete = true;
                self.scroll_paused = true;
                self.phase = Phase::LevelEnd;
                self.events.push(SimEvent::LevelComplete);
            }
        }

        // --- level object spawning ---
        // Objects spawn when the scroll reaches their world y (they appear at
        // the top edge and scroll down into view).
        let spawn_horizon = self.scroll + SPAWN_AHEAD;
        while let Some(&oi) = self.pending_objects.first() {
            let obj = &self.level.objects[oi];
            if (obj.y as f32) <= spawn_horizon {
                self.pending_objects.remove(0);
                if obj.unit_id == NONE_ID {
                    continue;
                }
                self.request_spawn(SpawnReq {
                    unit: obj.unit_id,
                    pos: Vec2::new(obj.x as f32, obj.y as f32),
                    heading: Some(obj.heading_degrees as f32),
                    owner: usize::MAX,
                    spawner_player: -1,
                    stationary: obj.stationary,
                    state: None,
                });
            } else {
                break;
            }
        }

        // --- process queued spawns ---
        let queue = std::mem::take(&mut self.spawn_queue);
        for req in &queue {
            self.spawn_entity(assets, req);
        }

        // --- players ---
        for slot in 0..2 {
            self.step_player(assets, slot, input);
        }

        // --- entities ---
        self.step_entities(assets);

        // --- collisions ---
        self.collide(assets);

        // --- cleanup ---
        self.entities.retain(|e| !e.dead);
    }

    fn step_player(&mut self, assets: &GameAssets, slot: usize, input: &PlayerInput) {
        let def: PlayerDef = match self.players[slot].as_ref() {
            Some(p) => assets.players.get(&p.def_id).cloned().unwrap_or_default(),
            None => return,
        };
        let inp = input.for_slot(slot);
        let two_p = self.two_player_active();
        let p = self.players[slot].as_mut().unwrap();
        p.age += 1;

        match p.phase {
            PlayerPhase::Entering => {
                p.phase_time += 1;
                if p.phase_time >= def.entry_initial_delay as u32 {
                    p.phase = PlayerPhase::Active;
                    p.phase_time = 0;
                }
                return;
            }
            PlayerPhase::Dying => {
                p.phase_time += 1;
                if p.phase_time >= def.death_duration as u32 {
                    if p.lives > 0 {
                        // Respawn.
                        p.phase = PlayerPhase::Active;
                        p.screen = Vec2::new(
                            if slot == 0 && two_p { def.entry_multi_start_x as f32 } else { def.entry_solo_start_x as f32 },
                            def.entry_solo_start_y as f32,
                        );
                        p.vel = Vec2::ZERO;
                        p.shields = 100.0;
                        p.invuln = def.entry_invulnerability_time as u32;
                        p.phase_time = 0;
                    } else {
                        p.phase = PlayerPhase::Dead;
                        p.phase_time = 0;
                    }
                }
                return;
            }
            PlayerPhase::Dead => return,
            PlayerPhase::Active => {}
        }

        if p.invuln > 0 { p.invuln -= 1; }
        if p.fire_timer > 0 { p.fire_timer -= 1; }
        if p.shield_recharge > 0 {
            p.shield_recharge -= 1;
        } else if p.shields < 100.0 {
            p.shields = (p.shields + 0.05).min(100.0);
        }
        p.power = (p.power + 0.02).min(100.0);

        // Movement: velocity approaches ±max_speed by delta per tick,
        // decays toward 0 when input released.
        let max_speed = def.active_default_max_speed as f32;
        let step = def.active_velocity_delta as f32;
        let mut tv = Vec2::ZERO;
        if inp.up { tv.y -= 1.0; }
        if inp.down { tv.y += 1.0; }
        if inp.left { tv.x -= 1.0; }
        if inp.right { tv.x += 1.0; }
        if tv.length_squared() > 0.0 {
            tv = tv.normalize() * max_speed;
        }
        p.vel += (tv - p.vel).clamp_length_max(step);
        if tv == Vec2::ZERO {
            p.vel *= 0.85;
            if p.vel.length() < 0.05 {
                p.vel = Vec2::ZERO;
            }
        }
        p.screen += p.vel;

        p.tilt_frame = if inp.left { 4 }
            else if inp.right { 6 }
            else if inp.up { 1 }
            else if inp.down { 3 }
            else { 0 };

        // Side-scroll camera follows steering.
        if inp.left {
            self.cam_x = (self.cam_x - 1.0).max(SIDE_SCROLL_MIN);
        } else if inp.right {
            self.cam_x = (self.cam_x + 1.0).min(SIDE_SCROLL_MAX);
        }

        // Clamp to playfield.
        let hw = 14.0;
        p.screen.x = p.screen.x.clamp(PLAY_LEFT + hw, PLAY_RIGHT - hw);
        p.screen.y = p.screen.y.clamp(PLAYER_TOP_LIMIT, PLAYER_BOTTOM_LIMIT);
        if p.vel.x != 0.0 && (p.screen.x <= PLAY_LEFT + hw || p.screen.x >= PLAY_RIGHT - hw) {
            p.vel.x = 0.0;
        }
        if p.vel.y != 0.0 && (p.screen.y <= PLAYER_TOP_LIMIT || p.screen.y >= PLAYER_BOTTOM_LIMIT) {
            p.vel.y = 0.0;
        }

        // Bomb (ground weapon) — TODO full implementation.
        if inp.bomb && p.bombs > 0 {
            p.bombs -= 1;
        }
        // Firing.
        if inp.fire && p.fire_timer == 0 {
            self.fire_weapon(assets, slot);
        }
    }

    fn two_player_active(&self) -> bool {
        self.players[1].is_some()
    }

    fn step_entities(&mut self, assets: &GameAssets) {
        self.entity_scroll_pause = false;
        for i in 0..self.entities.len() {
            if self.entities[i].dead {
                continue;
            }
            let unit_id = self.entities[i].unit;
            // Clone the def+state to release the assets borrow.
            let (def, state) = match assets.units.get(&unit_id) {
                Some(d) if !d.states.is_empty() => {
                    let s = d.states[self.entities[i].state.min(d.states.len() - 1)].clone();
                    (d.clone(), s)
                }
                _ => {
                    self.entities[i].dead = true;
                    continue;
                }
            };
            let _ = def;

            // Timers.
            {
                let e = &mut self.entities[i];
                e.age += 1;
                e.state_time += 1;
                if e.hit_cooldown > 0 { e.hit_cooldown -= 1; }
                if e.hit_flash > 0 { e.hit_flash -= 1; }
            }

            // --- state transitions ---
            // on_timer → change to named state (or delete).
            if state.on_timer_target_reached(&self.entities[i]) {
                self.apply_state_change(i, &state.on_timer_change_to, assets);
                continue;
            }
            if state.on_counter > 0 && self.entities[i].state_time as i64 >= state.on_counter {
                self.apply_state_change(i, &state.on_counter_change_to, assets);
                continue;
            }

            // --- animation ---
            self.step_animation(i, &state);

            // --- rules (original runs ProcessRules right after Animate,
            //     before movement; a fired rule changes state for this tick)
            self.step_rules(i, &state, assets);
            if self.entities[i].dead {
                continue;
            }
            // State may have changed — refresh for movement/spawn sets.
            let state = match assets.units.get(&self.entities[i].unit) {
                Some(d) if !d.states.is_empty() => {
                    d.states[self.entities[i].state.min(d.states.len() - 1)].clone()
                }
                _ => continue,
            };

            // --- tracking flag (m_field_0xb5): rotating to a live target ---
            {
                let epos = self.entities[i].pos;
                let has_target = self.nearest_player_dist(epos) != f32::MAX;
                self.entities[i].tracking = state.rotate_to_target && has_target;
            }

            // --- lifecycle flags ---
            // pause_vertical_scrolling aggregates into entity_scroll_pause
            // for the NEXT tick's scroll decision (G_EG_Process result).
            if state.pause_vertical_scrolling {
                self.entity_scroll_pause = true;
            }
            {
                let any_active = self
                    .players
                    .iter()
                    .flatten()
                    .any(|p| p.phase == PlayerPhase::Active);
                let owner_uid = self.entities[i].owner;
                let owner_info = if owner_uid == u32::MAX {
                    None
                } else {
                    self.entities
                        .iter()
                        .find(|o| o.unique_num == owner_uid)
                        .map(|o| (o.dead, o.destroyed))
                };
                let mut do_destroy = false;
                let mut do_delete = false;
                if !any_active {
                    if state.delete_on_no_active_players {
                        self.entities[i].dead = true;
                        continue;
                    }
                    if state.destruct_on_no_active_players {
                        do_destroy = true;
                    }
                }
                let scroll_hold = self.scroll_paused || self.entity_scroll_pause;
                if state.destruct_if_scrolling_not_paused
                    && owner_uid != u32::MAX
                    && !scroll_hold
                {
                    do_destroy = true;
                }
                // Owner gone (dead or compacted away): propagate per flag.
                let owner_gone = owner_uid != u32::MAX
                    && owner_info.map(|(d, _)| d).unwrap_or(true);
                if owner_gone {
                    // Missing owner counts as destroyed (destruct path is
                    // the common removal for owner entities).
                    let owner_destroyed = owner_info.map(|(_, ds)| ds).unwrap_or(true);
                    if owner_destroyed && state.destroy_on_owner_destruction {
                        do_destroy = true;
                    }
                    if !owner_destroyed && state.delete_on_owner_deletion {
                        do_delete = true;
                    }
                }
                if do_destroy {
                    let sp = self.entities[i].spawner_player;
                    self.destroy_entity(assets, i, sp);
                    continue;
                }
                if do_delete {
                    self.entities[i].dead = true;
                    continue;
                }
            }

            // --- movement + hunt/on-range AI (DoMovementAI approximation) ---
            let epos0 = self.entities[i].pos;
            let target = self.nearest_player(epos0);
            let mut pending_change: Option<String> = None;
            let mut brake = false;
            let mut flee_from: Option<Vec2> = None;
            let mut hunt_to: Option<Vec2> = None;
            if let Some((tp, td)) = target {
                let in_range = state.on_range != 0.0 && td < state.on_range as f32;
                if in_range {
                    let oc = state.on_range_change_to.trim();
                    if !oc.is_empty() && !oc.eq_ignore_ascii_case("none") {
                        pending_change = Some(state.on_range_change_to.clone());
                    }
                    if state.hold_position_to_target {
                        brake = true;
                    }
                }
                if state.flee_id != NONE_ID {
                    flee_from = Some(tp);
                } else if state.hunts && !brake {
                    hunt_to = Some(tp);
                }
            } else {
                // No active players: unit-level flee flags pick a heading.
                let udef = assets.units.get(&self.entities[i].unit);
                if let Some(d) = udef {
                    if d.flees_north_on_no_players {
                        hunt_to = Some(epos0 + Vec2::new(0.0, 4096.0));
                    } else if d.flees_south_on_no_players {
                        hunt_to = Some(epos0 + Vec2::new(0.0, -4096.0));
                    }
                }
            }
            if let Some(name) = pending_change {
                self.apply_state_change(i, &name, assets);
                continue;
            }
            {
                let e = &mut self.entities[i];
                let target_speed = state.max_speed as f32;
                let cur_speed = e.vel.length();
                if brake {
                    e.vel *= 0.85;
                } else if let Some(tp) = flee_from.or(hunt_to) {
                    // Steer toward (hunt) or away from (flee) the target.
                    let dir = if flee_from.is_some() {
                        (epos0 - tp).normalize_or_zero()
                    } else {
                        (tp - epos0).normalize_or_zero()
                    };
                    if dir.length_squared() > 0.0001 {
                        let speed = if flee_from.is_some() {
                            state.flee_speed.max(state.max_speed) as f32
                        } else {
                            target_speed
                        };
                        let delta = state.delta.max(state.flee_delta) as f32;
                        let new_speed = (cur_speed + delta).min(speed.max(1.0));
                        e.vel = dir * new_speed;
                        e.heading = vector_to_heading(e.vel);
                    }
                } else if target_speed > 0.0 && cur_speed > 0.001 {
                    let delta = state.delta as f32;
                    let new_speed = (cur_speed + delta).min(target_speed);
                    e.vel = e.vel.normalize() * new_speed;
                }
                e.pos += e.vel;
                if e.vel.length_squared() > 0.0001 {
                    e.heading = vector_to_heading(e.vel);
                }
            }

            // --- spawn sets ---
            self.step_spawn_sets(i, &state);

            // --- cull ---
            let e = &self.entities[i];
            let sy = self.scroll + SCREEN_H - e.pos.y;
            let sx = e.pos.x - self.cam_x;
            let off = e.pos.y < -80.0
                || e.pos.y > self.level_height() + 80.0
                || sx < -80.0 || sx > SCREEN_W + 80.0
                || sy < -320.0 || sy > SCREEN_H + 320.0;
            if off {
                self.entities[i].dead = true;
            }
        }
    }

    fn apply_state_change(&mut self, idx: usize, name: &str, assets: &GameAssets) {
        let lname = name.trim().to_lowercase();
        if lname.is_empty() || lname == "none" {
            return;
        }
        if lname == "delete" || lname == "destroy" {
            self.entities[idx].dead = true;
            return;
        }
        let unit_id = self.entities[idx].unit;
        if let Some(def) = assets.units.get(&unit_id) {
            if let Some(ns) = find_state(def, name) {
                self.change_state(idx, ns, def);
            }
        }
    }

    fn change_state(&mut self, idx: usize, new_state: usize, def: &UnitDef) {
        let e = &mut self.entities[idx];
        e.state = new_state.min(def.states.len() - 1);
        e.state_time = 0;
        e.dir_index = -1;
        let st = &def.states[e.state];
        e.state_timer_target = if st.on_timer_max > 0 || st.on_timer_min > 0 {
            rand_range(&mut self.rng, st.on_timer_min as f64, st.on_timer_max.max(st.on_timer_min) as f64) as u32
        } else { 0 };
        e.spawn_timers = vec![SpawnSetRt::default(); st.spawn_sets.len()];
        e.frame = st.sprite_frame_min;
        e.anim_stopped = false;
        e.tracking = false;
    }

    fn step_animation(&mut self, idx: usize, state: &UnitState) {
        let e = &mut self.entities[idx];
        let num_dirs = state.num_directions.max(1) as usize;
        let frame_min = state.sprite_frame_min;
        let frame_max = state.sprite_frame_max.max(frame_min);
        let span = (frame_max - frame_min + 1).max(1) as usize;
        let per_dir = (span / num_dirs).max(1) as i64;
        let dir = heading_to_dir(e.heading, num_dirs);
        if dir != e.dir_index {
            e.dir_index = dir;
            // Direction-major layout: each direction owns per_dir frames.
            e.frame = frame_min + dir as i64 * per_dir;
            e.frame_timer = 0;
        }
        // Original latches m_field_0xb6 until the next ChangeState.
        if e.anim_stopped {
            return;
        }
        let delay = state.frame_delay.max(1) as u32;
        e.frame_timer += 1;
        if e.frame_timer < delay {
            return;
        }
        e.frame_timer = 0;
        let dir_base = frame_min + dir as i64 * per_dir;
        let local = (e.frame - dir_base).clamp(0, per_dir - 1);
        if state.continuous_frame_randomisation {
            let r = rand_range(&mut self.rng, 0.0, (per_dir - 1) as f64) as i64;
            e.frame = dir_base + r;
            return;
        }
        let step_dir = if state.animate_backwards { -1 } else { 1 } * state.frame_delta.signum() as i64;
        let mut next = local + if step_dir == 0 { 1 } else { step_dir };
        if next >= per_dir {
            if state.loop_animation {
                next = 0;
            } else {
                next = per_dir - 1;
                e.anim_stopped = true;
            }
        }
        if next < 0 {
            if state.loop_animation {
                next = per_dir - 1;
            } else {
                next = 0;
                e.anim_stopped = true;
            }
        }
        e.frame = dir_base + next;
    }

    fn step_spawn_sets(&mut self, idx: usize, state: &UnitState) {
        if state.spawn_sets.is_empty() {
            return;
        }
        let mut timers = self.entities[idx].spawn_timers.clone();
        let (pos, heading) = {
            let e = &self.entities[idx];
            (e.pos, e.heading)
        };
        for (si, ss) in state.spawn_sets.iter().enumerate() {
            if si >= timers.len() || ss.spawn_id == NONE_ID {
                continue;
            }
            let rt = &mut timers[si];
            if rt.done {
                continue;
            }
            if !rt.started {
                rt.started = true;
                rt.next_volley = rand_range(&mut self.rng, ss.delay_between_min as f64, ss.delay_between_max.max(ss.delay_between_min) as f64) as u32;
                rt.shots_left = rand_range(&mut self.rng, ss.volley_min as f64, ss.volley_max.max(ss.volley_min) as f64) as u32;
                rt.shot_timer = 0;
                continue;
            }
            if rt.shots_left == 0 {
                if rt.next_volley > 0 {
                    rt.next_volley -= 1;
                    if rt.next_volley == 0 && !ss.repeat_spawns {
                        rt.done = true;
                    }
                } else if ss.repeat_spawns {
                    rt.shots_left = rand_range(&mut self.rng, ss.volley_min as f64, ss.volley_max.max(ss.volley_min) as f64) as u32;
                    rt.shot_timer = 0;
                }
                continue;
            }
            if rt.shot_timer > 0 {
                rt.shot_timer -= 1;
                continue;
            }
            // Fire one shot.
            rt.shots_left = rt.shots_left.saturating_sub(1);
            rt.shot_timer = rand_range(&mut self.rng, ss.rate_min as f64, ss.rate_max.max(ss.rate_min) as f64) as u32;
            if rt.shots_left == 0 {
                rt.next_volley = rand_range(&mut self.rng, ss.delay_between_min as f64, ss.delay_between_max.max(ss.delay_between_min) as f64) as u32;
            }
            // Position: owner-relative offset or absolute.
            let local = Vec2::new(ss.x_offset as f32, -ss.y_offset as f32);
            let sp = if ss.absolute_coordinates {
                Vec2::new(ss.x_offset as f32, ss.y_offset as f32)
            } else if ss.adjust_offset_for_rotation {
                let r = heading.to_radians();
                pos + Vec2::new(
                    local.x * r.cos() - local.y * r.sin(),
                    local.x * r.sin() + local.y * r.cos(),
                )
            } else {
                pos + local
            };
            self.request_spawn(SpawnReq {
                unit: ss.spawn_id,
                pos: sp,
                heading: if ss.set_heading { Some(ss.heading_degrees as f32) } else { Some(heading) },
                owner: idx,
                spawner_player: self.entities[idx].spawner_player,
                stationary: ss.stationary_option,
                state: None,
            });
        }
        self.entities[idx].spawn_timers = timers;
    }

    /// `G_Entity::ProcessRules` — evaluates up to 5 rules in order; the
    /// first whose condition holds fires `ChangeState(rule.action)` and
    /// processing stops. Rules with `unit_id == none` or an empty
    /// condition are skipped entirely (matching the original).
    fn step_rules(&mut self, idx: usize, state: &UnitState, assets: &GameAssets) {
        for rule in state.rules.iter().take(5) {
            if rule.unit_id == NONE_ID || rule.condition.trim().is_empty() {
                continue;
            }
            if !self.rule_condition(rule, idx, state, assets) {
                continue;
            }
            self.apply_rule_action(idx, &rule.action, assets);
            return;
        }
    }

    /// `G_Entity::ChangeState` as reached from a rule: "Delete" removes,
    /// "Destroy" runs the destruct path, anything else is a state-name
    /// lookup (unknown names are a no-op).
    fn apply_rule_action(&mut self, idx: usize, action: &str, assets: &GameAssets) {
        if action == "Delete" {
            self.entities[idx].dead = true;
            return;
        }
        if action == "Destroy" {
            let owner = self.entities[idx].spawner_player;
            self.destroy_entity(assets, idx, owner);
            return;
        }
        self.apply_state_change(idx, action, assets);
    }

    /// Entity-of-type query shared by the tracking/active conditions.
    /// `param` is the rule's range: 0 = any distance, else the candidate
    /// must be within `param` px of the rule-triggering entity.
    fn any_entity_matching(&self, unit_id: ResId, from: Vec2, param: i64, tracking_only: bool) -> bool {
        if unit_id == NONE_ID {
            return false;
        }
        for e in &self.entities {
            if e.dead || e.unit != unit_id {
                continue;
            }
            if tracking_only && !e.tracking {
                continue;
            }
            if param != 0 && e.pos.distance(from) > param as f32 {
                continue;
            }
            return true;
        }
        false
    }

    fn count_active_of_type(&self, unit_id: ResId) -> usize {
        if unit_id == NONE_ID {
            return 0;
        }
        self.entities.iter().filter(|e| !e.dead && e.unit == unit_id).count()
    }

    /// "Destroyable air/ground entities" map to the unit's
    /// include-in-accuracy flags (the original reads the same bytes the
    /// accuracy stats and debug labels use).
    fn any_destroyable_air(&self, assets: &GameAssets) -> bool {
        self.entities.iter().any(|e| {
            !e.dead
                && assets
                    .units
                    .get(&e.unit)
                    .map(|d| d.include_in_air_accuracy)
                    .unwrap_or(false)
        })
    }

    fn any_destroyable_ground(&self, assets: &GameAssets) -> bool {
        self.entities.iter().any(|e| {
            if e.dead {
                return false;
            }
            let Some(d) = assets.units.get(&e.unit) else { return false };
            if !d.include_in_ground_accuracy {
                return false;
            }
            self.is_within_game_area(e.pos, assets)
        })
    }

    /// `G_Entity::IsWithinGameArea`: entity inside the visible playfield —
    /// playfield x 0..416 (world x 32..448), height above the scroll
    /// origin 0..480.
    fn is_within_game_area(&self, pos: Vec2, assets: &GameAssets) -> bool {
        let px = pos.x - PLAY_LEFT;
        let py = pos.y - self.scroll;
        px >= 0.0 && px <= pf(assets, pf::VISIBLE_GAME_WIDTH)
            && py >= 0.0 && py <= pf(assets, pf::VISIBLE_GAME_HEIGHT)
    }

    fn rule_condition(&self, rule: &StateRule, idx: usize, state: &UnitState, assets: &GameAssets) -> bool {
        let cond = rule.condition.trim().to_lowercase();
        let param = rule.range;
        let pos = self.entities[idx].pos;
        match cond.as_str() {
            "is tracking player" => self.any_entity_matching(rule.unit_id, pos, param, true),
            "is not tracking player" => !self.any_entity_matching(rule.unit_id, pos, param, true),
            "is active" => self.any_entity_matching(rule.unit_id, pos, param, false),
            "is not active" => !self.any_entity_matching(rule.unit_id, pos, param, false),
            "no destroyable air entities are active" => !self.any_destroyable_air(assets),
            "no destroyable ground entities are active" => !self.any_destroyable_ground(assets),
            "no destroyable air or ground entities are active" => {
                !self.any_destroyable_air(assets) && !self.any_destroyable_ground(assets)
            }
            "no players are active" => {
                !self.players.iter().flatten().any(|p| p.phase == PlayerPhase::Active)
            }
            "this entity is within range of a player" => {
                param != 0 && self.nearest_player_dist(pos) < param as f32
            }
            "this entity is not within range of a player" => {
                param != 0 && !(self.nearest_player_dist(pos) < param as f32)
            }
            "this entity's animation has stopped" => self.entities[idx].anim_stopped,
            // Tween-at-target conditions: the original compares the live
            // value against the state's required value. With no tween
            // simulation, a zero delta means already at target.
            "this entity's visibility is at required level" => state.visibility_delta_percent == 0,
            "this entity's tint is at required level" => state.tint_delta_percent == 0,
            "this entity's scale is at required level" => state.scale_delta_percent == 0,
            "number of this type of entity active" => {
                self.count_active_of_type(rule.unit_id) as i64 == param
            }
            "are fewer of these entities active" => {
                (self.count_active_of_type(rule.unit_id) as i64) < param
            }
            "are more of these entities active" => {
                (self.count_active_of_type(rule.unit_id) as i64) > param
            }
            _ => false,
        }
    }

    /// Closest active player: (world pos, distance).
    fn nearest_player(&self, pos: Vec2) -> Option<(Vec2, f32)> {
        let mut best: Option<(Vec2, f32)> = None;
        for p in self.players.iter().flatten() {
            if p.phase != PlayerPhase::Active {
                continue;
            }
            let pp = p.world_pos(self);
            let d = pp.distance(pos);
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((pp, d));
            }
        }
        best
    }

    fn nearest_player_dist(&self, pos: Vec2) -> f32 {
        let mut best = f32::MAX;
        for p in self.players.iter().flatten() {
            if p.phase != PlayerPhase::Active {
                continue;
            }
            best = best.min(p.world_pos(self).distance(pos));
        }
        best
    }

    /// Apply damage to an entity; handles shield depletion and death.
    pub fn damage_entity(&mut self, assets: &GameAssets, idx: usize, amount: f64, from_player: i32) {
        if idx >= self.entities.len() || self.entities[idx].dead {
            return;
        }
        if self.entities[idx].hit_cooldown > 0 {
            return;
        }
        let def = match assets.units.get(&self.entities[idx].unit) {
            Some(d) => d.clone(),
            None => return,
        };
        let e = &mut self.entities[idx];
        e.shields -= amount;
        e.hit_cooldown = 2;
        e.hit_flash = 4;
        let died = e.shields <= 0.0;
        if died && from_player >= 0 {
            if let Some(p) = self.players[from_player as usize].as_mut() {
                p.score += def.score as i64 * p.multiplier;
            }
        }
        if died {
            // Full destruct path: spawn/sound/coins/bonus/children.
            self.destroy_entity(assets, idx, from_player);
        } else {
            // Non-fatal hit: shield vs unshielded sound (hit particles
            // aren't ported yet).
            let shields_left = self.entities[idx].shields;
            let snd = if shields_left > 0.0 { def.shield_sound } else { def.unshielded_sound };
            if snd.id != NONE_ID {
                self.events.push(SimEvent::Sound(snd.id));
            }
        }
    }

    /// `G_Entity::Destroy` — the destruct path: spawn/sound/particle
    /// effects, coin release, random bonus drop, child handling. No score
    /// (the original awards score in `Hit`, before `Destroy` is reached).
    pub fn destroy_entity(&mut self, assets: &GameAssets, idx: usize, from_player: i32) {
        if idx >= self.entities.len() || self.entities[idx].dead {
            return;
        }
        let def = match assets.units.get(&self.entities[idx].unit) {
            Some(d) => d.clone(),
            None => return,
        };
        let epos = self.entities[idx].pos;
        let my_uid = self.entities[idx].unique_num;
        let owner_uid = self.entities[idx].owner;

        // Children first (destruct_destroy/delete_children).
        if def.destruct_destroy_children || def.destruct_delete_children {
            let kids: Vec<usize> = self
                .entities
                .iter()
                .enumerate()
                .filter(|(i, e)| *i != idx && !e.dead && e.owner == my_uid)
                .map(|(i, _)| i)
                .collect();
            for k in kids {
                if def.destruct_destroy_children {
                    self.destroy_entity(assets, k, from_player);
                } else {
                    self.entities[k].dead = true;
                }
            }
        }

        if def.destruct_spawn != NONE_ID {
            self.request_spawn(SpawnReq {
                unit: def.destruct_spawn,
                pos: epos,
                heading: None,
                owner: usize::MAX,
                spawner_player: from_player,
                stationary: false,
                state: None,
            });
        }
        if def.destruct_sound.id != NONE_ID {
            self.events.push(SimEvent::Sound(def.destruct_sound.id));
        }
        // destruct_particle / destruct_notice: particle & notice systems
        // aren't ported yet.

        // Coin release: N copies of destruct_coin at the kill location.
        if def.destruct_coin != NONE_ID && def.destruct_num_coins > 0 {
            for _ in 0..def.destruct_num_coins.min(64) {
                self.request_spawn(SpawnReq {
                    unit: def.destruct_coin,
                    pos: epos,
                    heading: None,
                    owner: usize::MAX,
                    spawner_player: from_player,
                    stationary: false,
                    state: None,
                });
            }
        }

        self.entities[idx].dead = true;
        self.entities[idx].destroyed = true;
        self.stat_kills += 1;

        // Group-kill bonus coin: when the last sibling under the same
        // owner goes down, spawn destruct_coin_on_group_kill once.
        if def.destruct_coin_on_group_kill != NONE_ID && owner_uid != u32::MAX {
            let any_left = self
                .entities
                .iter()
                .any(|e| !e.dead && e.owner == owner_uid);
            if !any_left {
                self.request_spawn(SpawnReq {
                    unit: def.destruct_coin_on_group_kill,
                    pos: epos,
                    heading: None,
                    owner: usize::MAX,
                    spawner_player: from_player,
                    stationary: false,
                    state: None,
                });
            }
        }

        // Random bonus drop (G_Entity::Destroy tail): weighted roll over
        // perm floats 0xd1..0xd9/0xda/0xdb → object_ids[0x19..0x22].
        if def.destruct_release_random_bonus {
            if let Some(bonus) = self.roll_random_bonus(assets) {
                self.request_spawn(SpawnReq {
                    unit: bonus,
                    pos: epos,
                    heading: None,
                    owner: usize::MAX,
                    spawner_player: from_player,
                    stationary: false,
                    state: None,
                });
            }
        }
    }

    /// Weighted random-bonus pick, mirroring `G_Entity::Destroy`'s roll.
    fn roll_random_bonus(&mut self, assets: &GameAssets) -> Option<ResId> {
        let roll = rand_range(&mut self.rng, 0.0, 100.0) as i64;
        let slot = if roll < pf(assets, 0xd1) as i64 {
            0x19
        } else if roll < pf(assets, 0xd2) as i64 {
            0x1a
        } else if roll < pf(assets, 0xd3) as i64 {
            0x1b
        } else if roll < pf(assets, 0xd4) as i64 {
            0x1c
        } else if roll < pf(assets, 0xd5) as i64 {
            0x1d
        } else if roll < pf(assets, 0xd6) as i64 {
            0x1e
        } else if roll < pf(assets, 0xd7) as i64 {
            0x1f
        } else if roll < pf(assets, 0xd8) as i64 {
            0x20
        } else if (self.level_number as f32) < pf(assets, 0xdb) {
            0x20
        } else if roll < pf(assets, 0xd9) as i64 {
            0x21
        } else {
            0x22
        };
        assets
            .object_ids
            .entries
            .get(slot)
            .map(|(_, r)| *r)
            .filter(|r| *r != NONE_ID)
    }

    /// Damage a player.
    pub fn damage_player(&mut self, slot: usize, amount: f64) {
        let p = match self.players[slot].as_mut() {
            Some(p) if p.phase == PlayerPhase::Active => p,
            _ => return,
        };
        if p.invuln > 0 {
            return;
        }
        p.shields -= amount;
        p.shield_recharge = 30;
        if p.shields <= 0.0 {
            p.lives -= 1;
            p.shields = 0.0;
            self.events.push(SimEvent::PlayerEvent(slot, "hit"));
            if p.lives > 0 {
                p.phase = PlayerPhase::Dying;
                p.phase_time = 0;
                p.invuln = 0;
            } else {
                p.phase = PlayerPhase::Dead;
                self.events.push(SimEvent::PlayerEvent(slot, "dead"));
            }
        }
    }

    fn collide(&mut self, assets: &GameAssets) {
        // Pass A: entities vs players (enemy projectiles / ramming bodies).
        // Pass B: player-owned projectiles vs hittable entities.
        let mut hits: Vec<(usize, f64, i32)> = Vec::new(); // (target idx, dmg, owner)
        let mut shots: Vec<usize> = Vec::new();            // projectile idx that hit
        let mut player_hits: Vec<(usize, f64)> = Vec::new();
        let mut pickups: Vec<(usize, usize)> = Vec::new();  // (entity idx, player slot)

        let ppos: Vec<Vec2> = self.players.iter()
            .map(|p| p.as_ref().map(|p| p.world_pos(self)).unwrap_or(Vec2::splat(-9999.0)))
            .collect();

        for (i, e) in self.entities.iter().enumerate() {
            if e.dead {
                continue;
            }
            let Some(def) = assets.units.get(&e.unit) else { continue };
            let r = entity_radius(def);
            for slot in 0..2 {
                if ppos[slot].x < -999.0 {
                    continue; // no active player in this slot
                }
                let dist = ppos[slot].distance(e.pos);
                if dist >= r + 12.0 {
                    continue;
                }
                // Pickup overlap (original checks pickup_type != none first).
                if def.pickup_type != NONE_ID {
                    pickups.push((i, slot));
                    continue;
                }
                // Dangerous entity vs player.
                if !def.player_projectile
                    && !def.harmless_to_players
                    && !def.terrain_effect
                    && def.damage > 0.0
                {
                    player_hits.push((slot, def.damage));
                }
            }
        }

        for i in 0..self.entities.len() {
            if self.entities[i].dead {
                continue;
            }
            let (epos, espawner) = {
                let e = &self.entities[i];
                (e.pos, e.spawner_player)
            };
            if espawner < 0 {
                continue;
            }
            let Some(pdef) = assets.units.get(&self.entities[i].unit) else { continue };
            if !pdef.player_projectile {
                continue;
            }
            let pr = entity_radius(pdef);
            for j in 0..self.entities.len() {
                if i == j || self.entities[j].dead {
                    continue;
                }
                if self.entities[j].spawner_player == espawner {
                    continue; // same side
                }
                let Some(tdef) = assets.units.get(&self.entities[j].unit) else { continue };
                if !tdef.can_be_hit_by_player_projectile {
                    continue;
                }
                let tr = entity_radius(tdef);
                if epos.distance(self.entities[j].pos) < pr + tr {
                    hits.push((j, pdef.damage, espawner));
                    shots.push(i);
                    self.stat_hits += 1;
                    break; // one target per projectile per tick
                }
            }
        }

        // Projectiles die on impact via damage_entity so destruct_spawn fires.
        for i in shots {
            let (unit, owner) = {
                let e = &self.entities[i];
                (e.unit, e.spawner_player)
            };
            let max_hp = assets.units.get(&unit).map(|d| d.shields_max).unwrap_or(1.0);
            self.damage_entity(assets, i, max_hp + 1.0, owner);
        }
        // Pickups: apply effect, destroy entity (full Destroy → destruct_spawn).
        for (i, slot) in pickups {
            if self.entities[i].dead {
                continue;
            }
            if self.apply_pickup(assets, slot, i) {
                let owner = slot as i32;
                self.damage_entity(assets, i, f64::MAX, owner);
            }
        }
        for (j, dmg, pl) in hits {
            self.damage_entity(assets, j, dmg, pl);
        }
        for (slot, dmg) in player_hits {
            self.damage_player(slot, dmg);
        }
    }

    /// Advance fixed timestep; call every frame with delta seconds.
    pub fn advance(&mut self, assets: &GameAssets, input: &PlayerInput, dt: f32) {
        self.acc += dt;
        let step = 1.0 / TICK_HZ;
        let mut n = 0;
        while self.acc >= step && n < 8 {
            self.step(assets, input);
            self.acc -= step;
            n += 1;
        }
        if n == 8 {
            self.acc = 0.0;
        }
    }
}

/// Find a state index by name (or 1-based index fallback).
fn find_state(def: &UnitDef, name: &str) -> Option<usize> {
    let n = name.trim();
    if n.is_empty() || n.eq_ignore_ascii_case("none") {
        return None;
    }
    for (i, s) in def.states.iter().enumerate() {
        if s.name.eq_ignore_ascii_case(n) {
            return Some(i);
        }
    }
    if let Ok(num) = n.parse::<usize>() {
        if num >= 1 && num <= def.states.len() {
            return Some(num - 1);
        }
    }
    None
}

/// Heading (deg, 0=N) → direction index for num_dirs sprites.
fn heading_to_dir(heading: f32, num_dirs: usize) -> i64 {
    if num_dirs <= 1 {
        return 0;
    }
    let step = 360.0 / num_dirs as f32;
    let h = heading.rem_euclid(360.0);
    ((h / step).round() as i64).rem_euclid(num_dirs as i64)
}

/// Velocity → compass heading degrees.
fn vector_to_heading(v: Vec2) -> f32 {
    if v.length_squared() < 0.0001 {
        return 0.0;
    }
    let deg = v.y.atan2(v.x).to_degrees();
    (90.0 - deg).rem_euclid(360.0)
}

// ---------------------------------------------------------------------------
// Weapons / pickups
// ---------------------------------------------------------------------------

/// `G_WepDef_MasterList_GetNextRefPtrByType`: weapons of `wtype` available
/// at `level`, in master-list order.
fn weapon_chain(assets: &GameAssets, wtype: ResId, level: i64) -> Vec<ResId> {
    assets
        .weapon_order
        .iter()
        .copied()
        .filter(|&w| {
            assets.weapons.get(&w).map_or(false, |d| {
                d.weapon_type == wtype
                    && level >= d.min_level_available
                    && level <= d.max_level_available
            })
        })
        .collect()
}

/// Next weapon after `after` in the type chain (wraps to first, like the
/// original's `first` fallback). `NONE_ID` → first available.
fn next_weapon_after(assets: &GameAssets, wtype: ResId, after: ResId, level: i64) -> Option<ResId> {
    let chain = weapon_chain(assets, wtype, level);
    if chain.is_empty() {
        return None;
    }
    if after == NONE_ID {
        return Some(chain[0]);
    }
    match chain.iter().position(|&w| w == after) {
        Some(i) => Some(chain[(i + 1) % chain.len()]),
        None => Some(chain[0]),
    }
}

/// Weapon class carried by a weapon-pickup unit, per `G_Entity` state init:
/// "air "→PEAA, "grnd"→PEAG, "spec"→PEAG (approximation; spec is rare).
fn pickup_weapon_type(pickup: ResId) -> Option<ResId> {
    if pickup == id("air ") {
        Some(id("PEAA"))
    } else if pickup == id("grnd") || pickup == id("spec") {
        Some(id("PEAG"))
    } else {
        None
    }
}

/// Resolve the weapon a pickup unit carries at spawn.
fn carried_weapon(assets: &GameAssets, def: &UnitDef, level: i64) -> ResId {
    match pickup_weapon_type(def.pickup_type) {
        Some(wt) => next_weapon_after(assets, wt, NONE_ID, level).unwrap_or(NONE_ID),
        None => NONE_ID,
    }
}

impl Sim {
    /// Apply a pickup effect for player `slot` touching entity `idx`.
    /// Returns true if the entity should be destroyed.
    fn apply_pickup(&mut self, assets: &GameAssets, slot: usize, idx: usize) -> bool {
        let def = match assets.units.get(&self.entities[idx].unit) {
            Some(d) => d.clone(),
            None => return true,
        };
        let pickup = def.pickup_type;
        let Some(p) = self.players[slot].as_mut() else { return true };
        if p.phase != PlayerPhase::Active {
            return false;
        }
        // Weapon pickups can't be grabbed while invulnerable (original
        // sub_41c1b0 returns false → entity is NOT destroyed).
        if pickup_weapon_type(pickup).is_some() && p.invuln > 0 {
            return false;
        }
        if pickup == id("air ") {
            if let Some(w) = next_weapon_after(assets, id("PEAA"), p.air_weapon, self.level_number) {
                p.air_weapon = w;
            }
            self.events.push(SimEvent::PlayerEvent(slot, "weapon_air"));
        } else if pickup == id("grnd") {
            if let Some(w) = next_weapon_after(assets, id("PEAG"), p.ground_weapon, self.level_number) {
                p.ground_weapon = w;
            }
            self.events.push(SimEvent::PlayerEvent(slot, "weapon_ground"));
        } else if pickup == id("coin") {
            p.money += def.pickup_value;
            self.events.push(SimEvent::PlayerEvent(slot, "coin"));
        } else if pickup == id("mult") {
            // 1→2→3→4→5→10, then stuck (G_Player::Multiplier_Advance).
            p.multiplier = match p.multiplier {
                1 => 2,
                2 => 3,
                3 => 4,
                4 => 5,
                5 => 10,
                _ => p.multiplier,
            };
            self.events.push(SimEvent::PlayerEvent(slot, "multiplier"));
        } else if pickup == id("exli") {
            let max_lives = assets
                .players
                .get(&p.def_id)
                .map(|d| d.life_max_num)
                .unwrap_or(i64::MAX);
            if p.lives < max_lives {
                p.lives += 1;
            }
            self.events.push(SimEvent::PlayerEvent(slot, "life"));
        } else if pickup == id("shie") {
            p.shields = (p.shields + def.pickup_value as f64).min(100.0);
            self.events.push(SimEvent::PlayerEvent(slot, "shield"));
        }
        // "spec" and unknown types: entity is still consumed (result=true).
        true
    }
}

fn entity_radius(def: &UnitDef) -> f32 {
    if def.player_projectile {
        4.0
    } else if def.harmless_to_players {
        8.0
    } else {
        14.0
    }
}

impl UnitState {
    /// Whether the rolled on_timer delay has been reached.
    fn on_timer_target_reached(&self, e: &Entity) -> bool {
        (self.on_timer_min > 0 || self.on_timer_max > 0)
            && e.state_timer_target > 0
            && e.state_time >= e.state_timer_target
    }
}

/// Per-slot input state, filled by the Bevy input layer.
#[derive(Clone, Copy, Default)]
pub struct PlayerInput {
    pub p1: SlotInput,
    pub p2: SlotInput,
}

#[derive(Clone, Copy, Default)]
pub struct SlotInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub fire: bool,
    pub bomb: bool,
}

impl PlayerInput {
    pub fn for_slot(&self, slot: usize) -> SlotInput {
        if slot == 0 { self.p1 } else { self.p2 }
    }
}
