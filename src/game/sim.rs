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

pub const TICK_HZ: f32 = 60.0;
pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 480.0;
/// Terrain width in world pixels.
pub const WORLD_W: f32 = 480.0;
/// Left edge of the 416-px playfield on screen.
pub const PLAY_LEFT: f32 = 112.0;
/// Right edge of the playfield on screen (PLAY_LEFT + VisibleGameWidth 416).
pub const PLAY_RIGHT: f32 = PLAY_LEFT + 416.0;
/// World→screen x offset: 480-wide world centered in the 640 window.
pub const WORLD_TO_SCREEN_X: f32 = 80.0;
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
    /// Damage dealt to the owner entity on pass-hits contact (pf 0xa1 = 100).
    pub const PASS_HITS_TO_OWNER: usize = 0xa1;
}

pub fn pf(assets: &GameAssets, i: usize) -> f32 {
    assets.fill_list.as_ref().map(|f| f.get(i) as f32).unwrap_or(0.0)
}

/// Convert a packed ResId to its four-character string for debugging.
pub fn rid_str(rid: ResId) -> String {
    let b = rid.to_le_bytes();
    b.iter().map(|&c| c as char).collect()
}

/// MSL `rand()`/`srand()` from the original executable (rand.obj):
/// `state = state*1103515245 + 12345; return (state >> 16) & 0x7fff`.
/// `srand` stores the seed verbatim. Using the same generator lets retail
/// film replays consume an identical random stream.
#[derive(Clone)]
pub struct Crand {
    state: u32,
}

impl Crand {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }
    pub fn srand(&mut self, seed: u32) {
        self.state = seed;
    }
    pub fn rand(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(1103515245)
            .wrapping_add(12345);
        (self.state >> 16) & 0x7fff
    }
}

/// `U_Utils_RandomInt(min, max)` = `min + rand() % (max - min + 1)`.
fn rand_int(rng: &mut Crand, min: i64, max: i64) -> i64 {
    if min == max {
        min
    } else {
        min + (rng.rand() % (max - min + 1) as u32) as i64
    }
}

/// `U_Utils_RandomFloat(low, high)` = `rand() * (high - low) / 32767 + min`.
fn rand_float(rng: &mut Crand, low: f64, high: f64) -> f64 {
    if low == high {
        low
    } else {
        let min = if low > high { high } else { low };
        rng.rand() as f64 * (high - low) / 32767.0 + min
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
    /// Hit-glow state (G_GameObject m_glow*): blink timer 4..32 ±6/tick.
    pub glow: GlowRt,
    /// game_time of the last on-hit state change (rate limit).
    pub hit_change_time: u32,
    /// Last collision-spawn time (original m_field_0xc2).
    pub hit_fx_time: u32,
    /// Total hits taken (original m_field_0xc6); gates one-shot collision spawns.
    pub hit_count: u32,
    /// Marks entities to be removed.
    pub dead: bool,
    /// Removed via the Destroy (destruct) path rather than Delete —
    /// distinguishes the two for owner-deletion propagation.
    pub destroyed: bool,
    /// Entity is homing/rotating toward a player target (rule condition
    /// "Is Tracking Player" — the original's `m_field_0xb5`).
    pub tracking: bool,
    /// rotate_to_target sweep state: current direction index being
    /// animated toward the target (-1 = derive from heading), and the
    /// game-time of the last step (Priv_RotateToTargetLoc port).
    pub rot_dir: i64,
    pub rot_last: u32,
    /// Non-looping animation reached its last frame (rule condition
    /// "This Entity's Animation Has Stopped" — the original's `m_field_0xb6`).
    pub anim_stopped: bool,
    /// Pinned screen position (notice banners); set at spawn.
    pub screen_lock: Option<Vec2>,
    /// Ticks until this entity activates (original `m_fielda4`, fed by
    /// cumulative groupDelay rolls). While >0 it skips all processing.
    pub activate_in: u32,
    /// Group id shared by members of one spawn request (original group
    /// entity); used for group-kill bonus coins.
    pub group_id: u32,
    /// Cached owner-relative offset (original m_field11c/m_field120).
    owner_off: Vec2,
    /// Owner location at last link update (original m_field124/m_field128).
    owner_prev: Vec2,
    /// Orbit angle in degrees (original m_fieldd8).
    orbit_angle: f32,
    /// Orbit radius; NaN = "use fixed offset" sentinel (original dat_4e34c0).
    orbit_radius: f32,
    /// Target velocity (original m_field_0x100/0x104): the velocity
    /// Priv_AdjustToRequiredVelocity eases toward; for cyclic states it
    /// mirrors the walk value.
    pub tvel: Vec2,
    /// Per-tick velocity delta (original m_field_0x108/0x10c): the
    /// acceleration applied to vel each tick (walk delta when cyclic).
    vdelta: Vec2,
    /// Fleeing flag (original m_field_0xc0): set by Priv_Flee on flee-state
    /// entry or by the no-active-players flee paths; cleared on state
    /// change into a non-flee state.
    pub fleeing: bool,
    /// Stationary spawn flag (original m_field_0x134): zeroes the velocity
    /// fields in Priv_AdjustToRequiredVelocity.
    pub stationary: bool,
    /// Priv_Flee target point (original m_field_0x114/0x118), computed once
    /// per flee-state entry.
    pub flee_target: Option<Vec2>,
    /// Visibility fade state (G_GameObject m_some62/66/6a): current %,
    /// required %, per-tick delta %.
    pub vis: f32,
    pub vis_req: f32,
    pub vis_delta: f32,
    /// Tint fade state (m_some54/58/5C).
    pub tint: f32,
    pub tint_req: f32,
    pub tint_delta: f32,
    /// Scale fade state (m_scale/m_targetScale/m_scaleDelta), 1.0 = native.
    pub scale: f32,
    pub scale_tgt: f32,
    pub scale_delta: f32,
    /// State particle burst count / last-burst tick (G_EntityGroup
    /// m_field_ec/m_field_e8 for stateParticles_* repeats).
    pub particle_bursts: u32,
    pub particle_last: u32,
    /// State-sound bookkeeping (m_fielde0 play count, m_fielddc last-play
    /// tick, and the per-state-index "already played" flags at +0x144).
    pub sound_play_count: u32,
    pub sound_last_play: u32,
    pub state_sound_played: Vec<bool>,
    /// Last motion-blur spawn tick (m_field_e4).
    pub blur_last: u32,
    /// Position where this entity was requested to spawn — the debris
    /// collision teleport target (g_entityGroupDefaultLocX/Y).
    pub entry_pos: Vec2,
    /// Set once the entity has collided with a debris zone (m_field134).
    pub debris_hit: bool,
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
    /// Screen-space position for rendering. Screen-locked entities
    /// (notice banners) keep the position captured at spawn.
    pub fn screen_pos(&self, sim: &Sim) -> Vec2 {
        if let Some(sp) = self.screen_lock {
            return sp;
        }
        Vec2::new(self.pos.x - sim.cam_x + WORLD_TO_SCREEN_X, sim.scroll + SCREEN_H - self.pos.y)
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

/// Entity hit-glow (`G_GameObject::Glow_Start/Process`): timer starts at
/// 32 and oscillates 4..32 by ±step; glow ends after the up-swing.
#[derive(Clone, Copy, Default)]
pub struct GlowRt {
    pub active: bool,
    /// m_glowTimer (4..=32 while active).
    pub timer: i32,
    /// m_glowFlag — true while counting down.
    pub flag: bool,
}

impl GlowRt {
    /// Glow_Start(color ignored — original passes 0x7fff,6,0 for hits).
    pub fn start(&mut self) {
        if !self.active {
            self.active = true;
            self.timer = 32;
            self.flag = true;
        }
    }

    /// Glow_Process — one tick of the sawtooth.
    pub fn process(&mut self) {
        if !self.active {
            return;
        }
        if self.flag {
            self.timer -= 6;
            if self.timer < 4 {
                self.timer = 4;
                self.flag = false;
            }
        } else {
            self.timer += 6;
            if self.timer > 32 {
                self.timer = 32;
                self.flag = true;
                self.active = false;
            }
        }
    }
}

/// Level-end money tally state — `G_Player::MoneyCounter` phases 1..8.
/// The original stages a text sequence then drains the purse one coin per
/// `MoneyCounter_Delay` ticks, awarding `chunk` score each drain until the
/// bonus (`money * factor`) is fully converted.
#[derive(Clone, Copy, Default)]
pub struct MoneyTally {
    /// 0 = inactive; otherwise the original's m_moneyCounter phase (1-8).
    pub phase: u8,
    /// Phase start time (m_moneyCounter0/m_moneyCounter3).
    pub t0: u32,
    /// Last drain tick (m_moneyCounter3 during phase 6).
    pub drain_t: u32,
    /// Bonus score left to award (m_moneyCounter5).
    pub remaining: i64,
    /// Score awarded per drain tick (m_moneyCounter6).
    pub chunk: i64,
    /// Purse captured at tally start (m_moneyCounter4) — the displayed
    /// "Coin Bonus: N" amount, constant while the purse drains.
    pub purse: i64,
    /// MoneyCounter_Start ran this level (m_moneyCounter != 0): the tally
    /// is staged and will arm when the bonus sequence completes.
    pub staged: bool,
}

/// Per-player state that survives a level transition (G_Player fields the
/// original does NOT reset in ResetAtLevelStart: score, lives, multiplier,
/// extra-life thresholds, and the pad7[0] damage latch).
#[derive(Clone, Copy, Default)]
pub struct PlayerCarry {
    pub score: i64,
    pub lives: i64,
    pub multiplier: i64,
    pub next_life_score: i64,
    pub life_bonus_acc: i64,
    pub damage_taken: bool,
}

impl PlayerCarry {
    pub fn capture(p: &Player) -> PlayerCarry {
        PlayerCarry {
            score: p.score,
            lives: p.lives,
            multiplier: p.multiplier,
            next_life_score: p.next_life_score,
            life_bonus_acc: p.life_bonus_acc,
            damage_taken: p.damage_taken,
        }
    }

    pub fn apply(self, p: &mut Player) {
        p.score = self.score;
        p.lives = self.lives;
        p.multiplier = self.multiplier;
        p.next_life_score = self.next_life_score;
        p.life_bonus_acc = self.life_bonus_acc;
        p.damage_taken = self.damage_taken;
    }
}

/// Run-scoped state carried between levels: per-player carries plus the
/// campaign perfect counter and the pending accuracy reward.
#[derive(Clone, Default)]
pub struct RunCarry {
    pub players: [Option<PlayerCarry>; 2],
    pub perfect_count: u32,
    /// exe 0x4e4827 → becomes `accuracy_reward_pending` next level.
    pub accuracy_reward: bool,
}

impl RunCarry {
    pub fn capture(sim: &Sim) -> RunCarry {
        RunCarry {
            players: [
                sim.players[0].as_ref().map(PlayerCarry::capture),
                sim.players[1].as_ref().map(PlayerCarry::capture),
            ],
            perfect_count: sim.perfect_count,
            accuracy_reward: sim.accuracy_reward_carry,
        }
    }

    pub fn apply(self, sim: &mut Sim) {
        sim.perfect_count = self.perfect_count;
        // exe level-start transfer: pending reward arms this level.
        sim.accuracy_reward_pending = self.accuracy_reward;
        sim.accuracy_reward_carry = false;
        for slot in 0..2 {
            if let (Some(c), Some(p)) = (self.players[slot], sim.players[slot].as_mut()) {
                c.apply(p);
            }
        }
    }
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
    /// Ground-weapon auto-burst shots remaining (G_WeaponHandler
    /// m_field_0x87). Armed by a bomb press after the recharge interval,
    /// drained one shot per delayBetweenLoadLaunches ticks.
    pub bomb_burst: i64,
    /// Game time of last ground-weapon activity (m_field_0x7b/0x7f — the
    /// original keeps two fields that are always written together).
    pub bomb_last: u32,
    /// Previous-tick bomb input for edge detection (pad1a).
    pub prev_bomb: bool,
    /// Shield recharge delay after a hit.
    pub shield_recharge: u32,
    /// Score multiplier.
    pub multiplier: i64,
    /// Next score threshold granting an extra life (m_pad3a).
    pub next_life_score: i64,
    /// Accumulated extra-life bonus offset (m_pad3b).
    pub life_bonus_acc: i64,
    /// Game time of last accepted hit (m_shieldReset1 hit-delay gate).
    pub last_hit_time: u32,
    /// Game time of last hit-effect spawn (m_shieldReset2 throttle).
    pub last_hit_fx: u32,
    /// Level-end money→score tally (G_Player::MoneyCounter port).
    pub money_tally: MoneyTally,
    /// Pickup glow ticks remaining (G_Player::Glow_Start on coin collect).
    pub glow_ticks: u8,
    /// Weapon-selector popup ticks remaining (WepSelectorAppearanceTime=60).
    pub weapon_popup: u32,
    /// Low-shield warning already shown (pad7[1] latch).
    pub shield_warned: bool,
    /// Took-damage latch (pad7[0]): set on any shield damage and after the
    /// once-per-run no-damage defence bonus is awarded. Persists across
    /// levels for the whole run.
    pub damage_taken: bool,
    /// Age in ticks.
    pub age: u32,
    /// Entry animation progress (spawn entity index if any).
    pub entry_entity: usize,
    /// Air-weapon charge/release power-up state.
    pub air_power: PowerupRt,
    /// Ground-weapon charge/release power-up state.
    pub gnd_power: PowerupRt,
    /// Ground-weapon power meter 0..100.
    pub gpower: f64,
}

/// Runtime state for a weapon's charge/release power-up — the air bank
/// (G_WeaponHandler fields 0x11..0x2d) and ground bank (0x35..0x51).
#[derive(Clone, Default)]
pub struct PowerupRt {
    /// 0 idle, 1 charging, 2 overload, 3 releasing.
    pub state: u8,
    /// Consecutive fire-hold ticks (m_field_0x2d / 0x51).
    pub hold: u32,
    /// Current power level (m_field_0x21 / 0x45).
    pub level: i64,
    /// Tick the current state began (m_field_0x15 / 0x39).
    pub state_start: u32,
    /// Tick of last power-level change (m_field_0x1d / 0x41).
    pub last_change: u32,
    /// Tick of next release spawn (m_field_0x29 / 0x4d).
    pub next_release: u32,
    /// Unit id of the activation spawn (stands in for the tracked uid).
    pub act_unit: ResId,
    /// Overload warning phase (m_overloadB): true while waiting to refill.
    pub ovl_phase: bool,
    /// Ticks between overload warnings (m_overloadE), shrunk each warning.
    pub ovl_interval: i64,
    /// Completed overload warnings (m_overloadF).
    pub ovl_count: i64,
    /// Tick the last warning fired (m_overloadD).
    pub ovl_last: u32,
    /// Overload meter, 0..100 (m_some54) — drives the power-bar display.
    pub ovl_meter: f64,
}

impl Player {
    /// World position used for collisions and spawning.
    pub fn world_pos(&self, sim: &Sim) -> Vec2 {
        Vec2::new(
            self.screen.x + sim.cam_x - WORLD_TO_SCREEN_X,
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
    next_group_id: u32,
    limit_message_shown: bool,
    pub rng: Crand,
    spawn_queue: Vec<SpawnReq>,
    /// Parallel to spawn_queue: pinned screen pos for UI entities.
    spawn_locks: Vec<Option<Vec2>>,
    /// Deferred one-shot actions (e.g. spawn at pos next tick).
    pub events: Vec<SimEvent>,
    /// Debris zones (G_Debris): world-space rects [min_x, min_y, max_x,
    /// max_y] left by destroyed `destructCreateObstacle` units. Fixed in
    /// world space = scroll-locked like the original's screen rects.
    pub debris: Vec<[f32; 4]>,
    /// Ground-accuracy counters (G_Game_GroundAccuracy_*): units flagged
    /// `includeInGroundAccuracyCount` spawned / destroyed this level.
    pub accuracy_targets: u32,
    pub accuracy_destroyed: u32,
    /// Active accuracy reward this level (exe 0x4e4828 /
    /// `CheckForRewardThisLevel`): armed at level start when the previous
    /// level finished at 100% ground accuracy; consumed by the
    /// RandomBonus_6 (rb06) substitution in the bonus roll.
    pub accuracy_reward_pending: bool,
    /// Pending accuracy reward for the NEXT level (exe 0x4e4827): set at
    /// level end when accuracy hits 100%, transferred at next level start.
    pub accuracy_reward_carry: bool,
    /// Level-end bonus breakdown (sub_020930 + sub_020d90 port): the
    /// accuracy-tier bonus awarded to both players ahead of the coin tally.
    pub end_bonus_percent: i64,
    /// tier(0xbd..0xc2) x levelNumber.
    pub end_bonus_total: i64,
    /// Remaining bonus score left to award (counts down during drain).
    pub end_bonus_remaining: i64,
    /// Fixed per-drain-tick award = max(total*pf(0xc8), pf(0xc7)).
    pub end_bonus_step: i64,
    /// sub_020d90 state machine phase (1..=10, 0 = not started/done).
    pub end_seq_state: u8,
    end_seq_t0: u32,
    end_seq_drain_t: u32,
    /// Perfect-campaign bonus tick counter (state 10).
    end_seq_counter: u32,
    /// Levels at 100% accuracy this run (exe 0x4e485e).
    pub perfect_count: u32,
    /// Levels started this run (exe 0x4e482a) — gates the final-level
    /// banner delay together with level_number == 12.
    pub levels_played: u32,
    /// Whole level-end sequence finished (bonus drain + coin tallies) —
    /// the flow layer advances to the next sector on this.
    pub level_end_complete: bool,
}

#[derive(Clone, Debug)]
pub enum SimEvent {
    /// Play a sound (ResId of `soun`).
    Sound(ResId),
    /// Play a perm sound by gaso-list index (level-end sequence cues).
    PermSound(u8),
    /// A player died / extra life / etc. (slot, kind).
    PlayerEvent(usize, &'static str),
    /// Level finished.
    LevelComplete,
    /// Entity cap reached (original "Reached Entity Limit" notice).
    EntityLimit,
    /// Spark burst request (G_Particle_NewGroup). Screen-space pos.
    Particles {
        x: f32,
        y: f32,
        kind: ResId,
        color: u32,
        /// Scroll with terrain (ground-class units).
        scroll: bool,
        /// Circular burst (unit-vector table).
        circular: bool,
    },
    /// Frozen-sprite afterimage request (G_MotionBlur_New). Carries the
    /// visual snapshot; screen-space pos.
    MotionBlur {
        unit: ResId,
        state: usize,
        frame: i64,
        x: f32,
        y: f32,
        scale: f32,
        /// Initial visibility percent and per-tick delta percent.
        vis0: f32,
        vis_d: f32,
    },
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
            next_group_id: 1,
            limit_message_shown: false,
            rng: Crand::new(seed as u32),
            spawn_queue: Vec::new(),
            spawn_locks: Vec::new(),
            events: Vec::new(),
            debris: Vec::new(),
            accuracy_targets: 0,
            accuracy_destroyed: 0,
            accuracy_reward_pending: false,
            accuracy_reward_carry: false,
            end_bonus_percent: 0,
            end_bonus_total: 0,
            end_bonus_remaining: 0,
            end_bonus_step: 0,
            end_seq_state: 0,
            end_seq_t0: 0,
            end_seq_drain_t: 0,
            end_seq_counter: 0,
            perfect_count: 0,
            levels_played: 0,
            level_end_complete: false,
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
        // Entry coords are play-area relative (0..416) → screen space.
        let (ex, ey) = if two_player {
            (def.entry_multi_start_x as f32, def.entry_multi_start_y as f32)
        } else {
            (def.entry_solo_start_x as f32, def.entry_solo_start_y as f32)
        };
        let (sx, sy) = (PLAY_LEFT + ex, ey);
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
            // Shields_Reset(true) at level start: m_shieldStart =
            // defaultShieldPercentage.
            shields: def.default_shield_percentage as f64,
            power: 0.0,
            air_weapon: id("aiic"),  // Ion Cannon
            ground_weapon: id("plbo"), // Plasma Bomb
            fire_timer: 0,
            invuln: def.entry_invulnerability_time as u32,
            phase_time: 0,
            bomb_burst: 0,
            bomb_last: 0,
            prev_bomb: false,
            shield_recharge: 0,
            multiplier: 1,
            next_life_score: def.life_initial_required_score,
            life_bonus_acc: 0,
            last_hit_time: 0,
            last_hit_fx: 0,
            money_tally: MoneyTally::default(),
            glow_ticks: 0,
            weapon_popup: 0,
            shield_warned: false,
            damage_taken: false,
            age: 0,
            entry_entity: usize::MAX,
            air_power: PowerupRt::default(),
            gnd_power: PowerupRt::default(),
            gpower: 0.0,
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
        self.spawn_locks.push(None);
    }

    /// Queue a spawn pinned to a fixed screen position (notice banners).
    pub fn request_spawn_screen(&mut self, req: SpawnReq, screen: Vec2) {
        if self.entities.len() + self.spawn_queue.len() >= MAX_ENTITIES {
            return;
        }
        self.spawn_queue.push(req);
        self.spawn_locks.push(Some(screen));
    }

    /// `G_EG_RequestSpawn` group path (`sub_41b650` + spawn loop): rolls
    /// num_in_group count, culls per-member by appearsPercent, then
    /// spawns each member with cumulative groupDelay activation delay.
    fn spawn_group(&mut self, assets: &GameAssets, req: &SpawnReq) {
        let Some(def) = assets.units.get(&req.unit).cloned() else {
            return;
        };
        if def.states.is_empty() {
            return;
        }
        // canBeSpawnedOnlyWhenPlayersActive gate (also blocks while the
        // level is waiting to end, per G_EG_RequestSpawn).
        if def.spawn_only_when_players_active
            && (self.phase != Phase::Playing
                || !self
                    .players
                    .iter()
                    .flatten()
                    .any(|p| p.phase == PlayerPhase::Active))
        {
            return;
        }
        // doNotSpawnIfTypeAlreadyExists gate.
        if def.do_not_spawn_if_type_exists
            && self.entities.iter().any(|e| !e.dead && e.unit == req.unit)
        {
            return;
        }
        // deleteExistingEntitiesOfThisTypeOwnedByPlayer gate.
        if def.delete_existing_owned_by_player && req.spawner_player >= 0 {
            for e in self.entities.iter_mut() {
                if !e.dead && e.unit == req.unit && e.spawner_player == req.spawner_player {
                    e.dead = true;
                }
            }
        }
        // Count roll: clamp(min,1)..=max, min>max → max.
        let mut lo = def.num_in_group_min.max(1);
        let hi = def.num_in_group_max;
        if lo > hi {
            lo = hi;
        }
        let mut count = if lo != hi {
            rand_int(&mut self.rng, lo, hi.max(lo))
        } else {
            lo
        };
        // appearsPercent per-member cull.
        if count > 0 {
            let mut culled = 0i64;
            for _ in 0..count {
                let chance = def.appears_percent;
                if chance != 100
                    && (chance == 0 || rand_int(&mut self.rng, 0, 100) > chance)
                {
                    culled += 1;
                }
            }
            count -= culled;
        }
        if count <= 0 {
            return;
        }
        // Original entity cap: active + requested > 1000 → reject once.
        let active = self
            .entities
            .iter()
            .filter(|e| !e.dead && e.activate_in == 0)
            .count();
        if active + count as usize > MAX_ENTITIES {
            if !self.limit_message_shown {
                self.limit_message_shown = true;
                self.events.push(SimEvent::EntityLimit);
            }
            return;
        }
        let gid = self.next_group_id;
        self.next_group_id = self.next_group_id.wrapping_add(1).max(1);
        let mut acc = 0u32;
        for _ in 0..count {
            acc = acc.saturating_add(if def.group_delay_min != def.group_delay_max {
                rand_int(
                    &mut self.rng,
                    def.group_delay_min as i64,
                    def.group_delay_max.max(def.group_delay_min) as i64,
                ) as u32
            } else {
                def.group_delay_min.max(0) as u32
            });
            self.spawn_one(assets, req, acc, gid);
        }
    }

    fn spawn_one(&mut self, assets: &GameAssets, req: &SpawnReq, activate_in: u32, group_id: u32) -> Option<usize> {
        let def = assets.units.get(&req.unit)?;
        if def.states.is_empty() {
            return None;
        }
        let state_idx = req.state.unwrap_or(0).min(def.states.len() - 1);
        let state = &def.states[state_idx];
        // sub_01c540 runs before sub_01c840: the unit def's xOffset/yOffset
        // min/max jitter each spawned entity around the request point; equal
        // bounds are a fixed offset, and the original's polar mode only
        // applies when both axes carry a range.
        let pos = if def.x_offset_min != def.x_offset_max
            || def.y_offset_min != def.y_offset_max
        {
            let jx = if def.x_offset_min != def.x_offset_max {
                let (lo, hi) = if def.x_offset_min <= def.x_offset_max {
                    (def.x_offset_min, def.x_offset_max)
                } else {
                    (def.x_offset_max, def.x_offset_min)
                };
                rand_int(&mut self.rng, lo.round() as i64, hi.round() as i64) as f32
            } else {
                def.x_offset_min as f32
            };
            let jy = if def.y_offset_min != def.y_offset_max {
                let (lo, hi) = if def.y_offset_min <= def.y_offset_max {
                    (def.y_offset_min, def.y_offset_max)
                } else {
                    (def.y_offset_max, def.y_offset_min)
                };
                rand_int(&mut self.rng, lo.round() as i64, hi.round() as i64) as f32
            } else {
                def.y_offset_min as f32
            };
            req.pos + Vec2::new(jx, jy)
        } else {
            req.pos
        };
        // sub_01c840: stationary entities (request b1a -> m_field134) return
        // before the initial-speed roll, so they consume no rand() here.
        let speed = if req.stationary {
            0.0
        } else {
            rand_float(&mut self.rng, def.initial_speed_min as f64, def.initial_speed_max as f64) as f32
        };
        let heading_base = req.heading.unwrap_or(def.initial_heading as f32);
        let tol = def.initial_heading_tolerance;
        // The tolerance roll only runs on the useOwnerHeading path
        // (useRandomAngle in sub_01c840); no retail unit sets
        // use_owner_heading, so the roll never fires.
        let heading = if !req.stationary
            && req.heading.is_none()
            && def.use_owner_heading
            && tol != 0
        {
            let half = ((tol + 1) >> 1) as i64;
            heading_base + rand_int(&mut self.rng, -half, half) as f32
        } else {
            heading_base
        };
        // Spawn velocity uses the INVERTED heading (sub_01c840 calls
        // U_Math_InvertAngle before GetVectorFromAngleAndSpeed); the stored
        // heading (entity+0x130) keeps the uninverted facing. The
        // initiallyHuntsClosestPlayer branch never writes the velocity
        // fields, so those entities spawn with zero velocity and let
        // DoMovementAI acquire a target.
        let hr = invert_angle(heading).to_radians();
        let vel = if req.stationary || def.initially_hunts_closest_player {
            Vec2::ZERO
        } else {
            Vec2::new(hr.sin() * speed, hr.cos() * speed)
        };
        let shields = if def.shields_max > 0.0 { def.shields_max } else { def.shields_base };
        // Roll the on_timer target once at spawn.
        let timer_target = if state.on_timer_max > 0 || state.on_timer_min > 0 {
            rand_int(&mut self.rng, state.on_timer_min as i64, state.on_timer_max.max(state.on_timer_min) as i64) as u32
        } else { 0 };
        let idx = self.entities.len();
        // req.owner is an entity index; store the stable unique_num.
        let owner_uid = if req.owner != usize::MAX && req.owner < self.entities.len() {
            self.entities[req.owner].unique_num
        } else {
            u32::MAX
        };
        let flee_target = if state.flee_id != NONE_ID {
            self.compute_flee_target(assets, state.flee_id, pos)
        } else {
            None
        };
        self.entities.push(Entity {
            unit: req.unit,
            pos,
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
            glow: GlowRt::default(),
            hit_change_time: 0,
            hit_fx_time: 0,
            hit_count: 0,
            dead: false,
            destroyed: false,
            tracking: false,
            rot_dir: -1,
            rot_last: 0,
            anim_stopped: false,
            screen_lock: None,
            activate_in,
            group_id,
            owner_off: Vec2::ZERO,
            owner_prev: Vec2::ZERO,
            orbit_angle: 0.0,
            orbit_radius: f32::NAN,
            tvel: vel,
            vdelta: Vec2::ZERO,
            fleeing: state.flee_id != NONE_ID,
            stationary: req.stationary,
            flee_target,
            vis: 100.0,
            vis_req: 100.0,
            vis_delta: 0.0,
            tint: 0.0,
            tint_req: 0.0,
            tint_delta: 0.0,
            scale: 1.0,
            scale_tgt: 1.0,
            scale_delta: 0.0,
            particle_bursts: 0,
            particle_last: 0,
            sound_play_count: 0,
            sound_last_play: 0,
            state_sound_played: vec![false; def.states.len()],
            blur_last: 0,
            entry_pos: req.pos,
            debris_hit: false,
            unique_num: self.next_unique,
            carried_weapon: carried_weapon(assets, def, self.level_number),
        });
        self.next_unique = self.next_unique.wrapping_add(1);
        if def.pickup_type != NONE_ID {
            self.stat_pickups += 1;
        }
        // G_Game_GroundAccuracy_AddTarget on entity creation.
        if def.include_in_ground_accuracy {
            self.accuracy_targets += 1;
        }
        let has_link = state.lock_to_owner_loc || state.link_to_owner_loc || state.orbit_owner;
        self.cache_owner_link(idx, has_link, state.orbit_owner);
        self.state_velocity_init(idx, state, true);
        self.scatter_cyclic_spawn(assets, idx);
        self.set_visual_targets(idx, state);
        // Initial state entry sound.
        if state.entry_sound.id != NONE_ID {
            self.events.push(SimEvent::Sound(state.entry_sound.id));
        }
        // stateUseParentDirection: adopt the owner's facing at spawn.
        if state.use_parent_direction {
            if let Some(owner_idx) = self
                .entities
                .iter()
                .position(|o| o.unique_num == self.entities[idx].owner && !o.dead)
            {
                let oh = self.entities[owner_idx].heading;
                self.entities[idx].heading = oh;
                self.entities[idx].dir_index = -1; // force direction re-pick
            }
        }
        Some(idx)
    }

    /// Fire a weapon's projectile pattern for a player.
    fn fire_weapon(&mut self, assets: &GameAssets, slot: usize) {
        let weapon_id = {
            let p = match self.players[slot].as_ref() {
                Some(p) if p.phase == PlayerPhase::Active => p,
                _ => return,
            };
            p.air_weapon
        };
        self.fire_weapon_id(assets, slot, weapon_id);
        if let Some(p) = self.players[slot].as_mut() {
            if let Some(w) = assets.weapons.get(&weapon_id) {
                p.fire_timer = w.delay_between_launches.max(1) as u32;
            }
        }
    }

    /// Fire the ground (plasma bomb) weapon — consumes one bomb per shot.
    fn fire_ground_weapon(&mut self, assets: &GameAssets, slot: usize) {
        let weapon_id = match self.players[slot].as_ref() {
            Some(p) if p.phase == PlayerPhase::Active => p.ground_weapon,
            _ => return,
        };
        self.fire_weapon_id(assets, slot, weapon_id);
    }

    fn fire_weapon_id(&mut self, assets: &GameAssets, slot: usize, weapon_id: ResId) {
        let pos = match self.players[slot].as_ref() {
            Some(p) if p.phase == PlayerPhase::Active => p.world_pos(self),
            _ => return,
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
    }

    /// `Priv_AirPowerup_Process` / ground-bank mirror: charge while the fire
    /// button is held, overload or auto-release at max level, then drain
    /// power level into release-spawn salvos until empty.
    fn step_powerup(&mut self, assets: &GameAssets, slot: usize, ground: bool, held: bool) {
        let wdef = {
            let Some(p) = self.players[slot].as_ref() else { return };
            if p.phase != PlayerPhase::Active {
                return;
            }
            let wid = if ground { p.ground_weapon } else { p.air_weapon };
            match assets.weapons.get(&wid) {
                Some(w) => w.clone(),
                None => return,
            }
        };
        let pu = &wdef.powerup;
        let (until_act, act_spawn, tbc, max_lvl, over, rel_spawn, tbr, do_rel) = if ground {
            (
                pu.ground_time_until_activation,
                pu.ground_activation_spawn,
                pu.ground_time_between_power_changes,
                pu.ground_max_power_level,
                pu.ground_overload_time,
                pu.ground_release_spawn,
                pu.ground_time_between_release_spawns,
                pu.ground_do_release_on_max,
            )
        } else {
            (
                pu.air_time_until_activation,
                pu.air_activation_spawn,
                pu.air_time_between_power_changes,
                pu.air_max_power_level,
                pu.air_overload_time,
                pu.air_release_spawn,
                pu.air_time_between_release_spawns,
                pu.air_do_release_on_max,
            )
        };
        if act_spawn == NONE_ID && rel_spawn == NONE_ID {
            return;
        }
        let now = self.game_time;
        let mut rt = {
            let p = self.players[slot].as_ref().unwrap();
            if ground { p.gnd_power.clone() } else { p.air_power.clone() }
        };

        // Consecutive-hold counter and release-fire early out.
        rt.hold = if held { rt.hold.saturating_add(1) } else { 0 };
        if !held && (rt.state == 1 || rt.state == 2) {
            rt.state = 3;
            rt.state_start = now;
            self.release_activation(assets, slot, rt.act_unit);
        }

        match rt.state {
            0 => {
                if rt.hold as i64 >= until_act {
                    rt.hold = 0;
                    let pos = self.players[slot].as_ref().unwrap().world_pos(self);
                    self.request_spawn(SpawnReq {
                        unit: act_spawn,
                        pos,
                        heading: None,
                        owner: usize::MAX,
                        spawner_player: slot as i32,
                        stationary: false,
                        state: None,
                    });
                    rt.act_unit = act_spawn;
                    rt.state = 1;
                    rt.state_start = now;
                    rt.last_change = now;
                    rt.next_release = now;
                    rt.level = 0;
                }
            }
            1 => {
                // Overload: charge stalls after overload_time ticks.
                if over >= 1 && rt.state_start as i64 + over < now as i64 {
                    rt.state = 2;
                    rt.state_start = now;
                    let def_id = self.players[slot].as_ref().map(|p| p.def_id).unwrap_or(NONE_ID);
                    if let Some(pd) = assets.players.get(&def_id) {
                        rt.ovl_phase = true;
                        rt.ovl_interval = pd.powerup_overload_initial_time_between_warnings;
                        rt.ovl_count = 0;
                        rt.ovl_last = now;
                        rt.ovl_meter = if ground {
                            self.players[slot].as_ref().map(|p| p.gpower).unwrap_or(0.0)
                        } else {
                            self.players[slot].as_ref().map(|p| p.power).unwrap_or(0.0)
                        };
                        if pd.powerup_overload_sound.id != NONE_ID {
                            self.events.push(SimEvent::Sound(pd.powerup_overload_sound.id));
                        }
                    }
                } else if now as i64 > rt.last_change as i64 + tbc {
                    rt.level += 1;
                    rt.last_change = now;
                    let pct = if max_lvl >= 1 {
                        ((rt.level as f64 / max_lvl as f64) * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
                    };
                    if let Some(p) = self.players[slot].as_mut() {
                        if ground { p.gpower = pct } else { p.power = pct }
                    }
                    if rt.level > max_lvl {
                        rt.level = max_lvl;
                        if let Some(p) = self.players[slot].as_mut() {
                            if ground { p.gpower = 100.0 } else { p.power = 100.0 }
                        }
                        if do_rel {
                            rt.state = 3;
                            rt.state_start = now;
                            self.release_activation(assets, slot, rt.act_unit);
                        }
                    }
                }
            }
            2 => {
                // PowerupOverload_Process: meter drains by
                // warning_fade_percent per tick; every ovl_interval ticks it
                // refills and counts a warning (the interval shrinks toward
                // the minimum each cycle); at num_warnings the ship is
                // destroyed. Releasing fire exits via the state-3 path above.
                let pd = self.players[slot]
                    .as_ref()
                    .and_then(|p| assets.players.get(&p.def_id))
                    .cloned();
                if let Some(pd) = pd {
                    if rt.ovl_phase {
                        if now as i64 > rt.ovl_last as i64 + rt.ovl_interval {
                            rt.ovl_last = now;
                            rt.ovl_meter += 100.0;
                            if rt.ovl_meter >= 100.0 {
                                rt.ovl_meter = 100.0;
                                rt.ovl_phase = false;
                                rt.ovl_interval -= 1;
                                if rt.ovl_interval
                                    < pd.powerup_overload_min_time_between_warnings
                                {
                                    rt.ovl_interval =
                                        pd.powerup_overload_min_time_between_warnings;
                                }
                                rt.ovl_count += 1;
                                if rt.ovl_count >= pd.powerup_overload_num_warnings {
                                    self.destroy_player(assets, slot);
                                } else if pd.powerup_overload_sound.id != NONE_ID {
                                    self.events
                                        .push(SimEvent::Sound(pd.powerup_overload_sound.id));
                                }
                            }
                        }
                    } else {
                        rt.ovl_meter -= pd.powerup_overload_warning_fade_percent;
                        if rt.ovl_meter <= 0.0 {
                            rt.ovl_meter = 0.0;
                            rt.ovl_phase = true;
                        }
                    }
                    if let Some(p) = self.players[slot].as_mut() {
                        if ground {
                            p.gpower = rt.ovl_meter;
                        } else {
                            p.power = rt.ovl_meter;
                        }
                    }
                }
            }
            3 => {
                if rt.level > 0 && now as i64 > rt.next_release as i64 + tbr {
                    let pos = self.players[slot].as_ref().unwrap().world_pos(self);
                    self.request_spawn(SpawnReq {
                        unit: rel_spawn,
                        pos,
                        heading: None,
                        owner: usize::MAX,
                        spawner_player: slot as i32,
                        stationary: false,
                        state: None,
                    });
                    rt.next_release = now;
                    rt.level -= 1;
                    let pct = if max_lvl >= 1 {
                        ((rt.level as f64 / max_lvl as f64) * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
                    };
                    if let Some(p) = self.players[slot].as_mut() {
                        if ground { p.gpower = pct } else { p.power = pct }
                    }
                }
                if rt.level <= 0 {
                    rt.state = 0;
                    rt.level = 0;
                    rt.hold = 0;
                    rt.act_unit = NONE_ID;
                    if let Some(p) = self.players[slot].as_mut() {
                        if ground { p.gpower = 0.0 } else { p.power = 0.0 }
                    }
                }
            }
            _ => {}
        }

        if let Some(p) = self.players[slot].as_mut() {
            if ground {
                p.gnd_power = rt;
            } else {
                p.air_power = rt;
            }
        }
    }

    /// `G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum`: the
    /// activation entity switches to its `use_on_weapon_powerup_release` state.
    fn release_activation(&mut self, assets: &GameAssets, slot: usize, act_unit: ResId) {
        if act_unit == NONE_ID {
            return;
        }
        let Some(idx) = self
            .entities
            .iter()
            .position(|e| !e.dead && e.unit == act_unit && e.spawner_player == slot as i32)
        else {
            return;
        };
        let Some(def) = assets.units.get(&act_unit).cloned() else {
            return;
        };
        if let Some(si) = def
            .states
            .iter()
            .position(|s| s.use_on_weapon_powerup_release)
        {
            self.change_state(idx, si, &def, assets);
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
                // Level-cleared sequence (sub_020930 + G_Player level-end):
                // players go invulnerable, the no-damage defence bonus
                // (levelNumber * pf(0xb8)) is awarded once per run per player,
                // the accuracy-tier bonus is computed for the breakdown
                // display, and the end-sequence state machine starts. The
                // per-player MoneyCounter coin tallies arm when it finishes.
                for slot in 0..2 {
                    let info = self.players[slot].as_ref().and_then(|p| {
                        if p.phase == PlayerPhase::Dead {
                            return None;
                        }
                        let def = assets.players.get(&p.def_id)?;
                        let wx = p.screen.x + self.cam_x - WORLD_TO_SCREEN_X;
                        let wy = self.scroll + SCREEN_H - p.screen.y;
                        Some((
                            def.active_defence_bonus_object,
                            p.money,
                            Vec2::new(wx, wy),
                            slot as i32,
                            p.damage_taken,
                        ))
                    });
                    let Some((def_obj, money, wpos, spawner, took_damage)) = info else { continue };
                    // pad7[0]: the defence/shield bonus fires only while the
                    // player has taken no damage this run; awarding latches
                    // the flag so it can happen at most once per run. The
                    // whole award (spawn + score) requires the player def's
                    // defence-bonus object (field0xce) to be defined.
                    if !took_damage {
                        if let Some(p) = self.players[slot].as_mut() {
                            p.damage_taken = true;
                            p.invuln = u32::MAX;
                        }
                        if def_obj != NONE_ID {
                            self.request_spawn(SpawnReq {
                                unit: def_obj,
                                pos: wpos,
                                heading: None,
                                owner: usize::MAX,
                                spawner_player: spawner,
                                stationary: true,
                                state: None,
                            });
                            self.add_score(
                                slot,
                                self.level_number * pf(assets, 0xb8) as i64,
                                false,
                                assets,
                            );
                        }
                    }
                    // MoneyCounter_Start: stage the tally inactive — it arms
                    // when the bonus-breakdown sequence completes so the
                    // purse drains after the "Bonus:" display, like the
                    // original ordering. Every in-game player gets a counter,
                    // even with an empty purse.
                    if let Some(p) = self.players[slot].as_mut() {
                        let factor = if pf(assets, 0xaa) != 0.0 {
                            pf(assets, 0xab) * self.level_number as f32
                        } else {
                            pf(assets, 0xab)
                        };
                        let remaining = (money as f32 * factor) as i64;
                        // moneyCounter6 = max(remaining * pf(0xc8),
                        // pf(0xc7)) — a floor, not a cap.
                        let mut chunk = (remaining as f32 * pf(assets, 0xc8)) as i64;
                        let floor = pf(assets, 0xc7) as i64;
                        if floor > chunk {
                            chunk = floor;
                        }
                        p.money_tally = MoneyTally {
                            phase: 0,
                            t0: self.game_time,
                            drain_t: self.game_time,
                            remaining,
                            chunk: chunk.max(1),
                            purse: money,
                            staged: true,
                        };
                    }
                }
                // sub_020930: accuracy percent + tiered bonus.
                let pct = if self.accuracy_targets > 0 {
                    self.accuracy_destroyed as f32 / self.accuracy_targets as f32 * 100.0
                } else {
                    0.0
                };
                self.end_bonus_percent = pct as i64;
                let cutoff = pf(assets, 0xbc) as f32;
                let tier = if pct >= 100.0 {
                    pf(assets, 0xbd)
                } else if pct >= 100.0 - cutoff {
                    pf(assets, 0xbe)
                } else if pct >= 100.0 - 2.0 * cutoff {
                    pf(assets, 0xbf)
                } else if pct >= 100.0 - 3.0 * cutoff {
                    pf(assets, 0xc0)
                } else if pct >= 100.0 - 4.0 * cutoff {
                    pf(assets, 0xc1)
                } else {
                    pf(assets, 0xc2)
                };
                let total = (tier as i64) * self.level_number;
                self.end_bonus_total = total;
                self.end_bonus_remaining = total;
                self.end_bonus_step = if total > 0 {
                    ((total as f32) * pf(assets, 0xc8)).max(pf(assets, 0xc7) as f32) as i64
                } else {
                    0
                };
                // 100% accuracy arms the RandomBonus_6 reward for the NEXT
                // level (exe 0x4e4827 → 0x4e4828 at level start).
                self.accuracy_reward_carry = pct >= 100.0;
                // Remaining players go invulnerable for the sequence.
                for p in self.players.iter_mut().flatten() {
                    if p.phase != PlayerPhase::Dead {
                        p.invuln = u32::MAX;
                    }
                }
                self.end_seq_state = 1;
                self.end_seq_t0 = self.game_time;
                self.end_seq_drain_t = self.game_time;
                self.end_seq_counter = 0;
                self.level_end_complete = false;
                // Level-end notice object: perm 0x16 (Notice_LevelEnd) or
                // 0x17 (Notice_AllLevelsCompleted) on a full-campaign
                // finale, spawned screen-centered.
                let finale = self.level_number == 12 && self.levels_played == 12;
                let nidx = if finale { 0x17 } else { 0x16 };
                if let Some(rid) = assets.object_ids.entries.get(nidx).map(|e| e.1) {
                    if rid != NONE_ID {
                        let pos = Vec2::new(self.cam_x + 240.0, self.scroll + 240.0);
                        self.request_spawn_screen(SpawnReq {
                            unit: rid,
                            pos,
                            heading: None,
                            owner: usize::MAX,
                            spawner_player: -1,
                            stationary: true,
                            state: None,
                        }, Vec2::new(320.0, 56.0));
                    }
                }
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
                // Ground-class level objects spawn 32px higher
                // (sub_41a5b0: typeCode "grnd" → y -= dat_4e34b8 = 32.0f).
                let mut pos = Vec2::new(obj.x as f32, obj.y as f32);
                if assets
                    .units
                    .get(&obj.unit_id)
                    .map(|d| d.is_ground_based)
                    .unwrap_or(false)
                {
                    pos.y -= 32.0;
                }
                self.request_spawn(SpawnReq {
                    unit: obj.unit_id,
                    pos,
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
        let locks = std::mem::take(&mut self.spawn_locks);
        for (i, req) in queue.iter().enumerate() {
            let before = self.entities.len();
            self.spawn_group(assets, req);
            if let Some(sp) = locks.get(i).copied().flatten() {
                for e in &mut self.entities[before..] {
                    e.screen_lock = Some(sp);
                }
            }
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

        // --- level-end money tallies (G_Player::MoneyCounter_Process) ---
        if self.phase == Phase::LevelEnd {
            self.step_end_sequence(assets);
            self.step_money_tallies(assets);
            // Whole sequence done: bonus breakdown finished and every
            // MoneyCounter tally drained (phase 0 = done or never staged).
            if self.end_seq_state == 0 && !self.level_end_complete {
                let all_done = self
                    .players
                    .iter()
                    .flatten()
                    .all(|p| p.money_tally.phase == 0);
                if all_done {
                    self.level_end_complete = true;
                }
            }
        }
    }

    /// sub_020d90 — the level-end bonus-breakdown state machine. Times the
    /// "All Mission Targets Destroyed" → "Ground Accuracy" → "Bonus"
    /// display, drains the accuracy-tier bonus to both players, then runs
    /// the perfect-campaign bonus when every level hit 100% accuracy. On
    /// completion it arms each player's MoneyCounter tally.
    fn step_end_sequence(&mut self, assets: &GameAssets) {
        if self.phase != Phase::LevelEnd || self.end_seq_state == 0 {
            return;
        }
        let now = self.game_time;
        let elapsed = now.saturating_sub(self.end_seq_t0) as i64;
        match self.end_seq_state {
            // Banner delay: pf(0xc3), or pf(0xc4) on the finale of a full
            // 12-level run (exe flag: levelNumber==12 && levelsPlayed==12).
            1 => {
                let is_last = self.level_number == 12 && self.levels_played == 12;
                let delay = if is_last {
                    pf(assets, 0xc4)
                } else {
                    pf(assets, 0xc3)
                };
                if elapsed > delay as i64 {
                    self.end_seq_state = 2;
                    self.end_seq_t0 = now;
                }
            }
            // Accuracy% → Bonus label → sound/perfect latch → Bonus value.
            // Leaving states 2/3 plays perm sound 0x14; leaving state 4
            // plays 0x16 (perfect), 0x15 (bonus pending) or 0x14 (none).
            2 | 3 | 4 | 5 => {
                if elapsed > pf(assets, 0xc5) as i64 {
                    let leaving = self.end_seq_state;
                    if leaving == 4 && self.end_bonus_percent == 100 {
                        self.perfect_count += 1;
                    }
                    self.end_seq_state += 1;
                    self.end_seq_t0 = now;
                    let snd = match leaving {
                        2 | 3 => Some(0x14),
                        4 => {
                            if self.end_bonus_percent == 100 {
                                Some(0x16)
                            } else if self.end_bonus_total != 0 {
                                Some(0x15)
                            } else {
                                Some(0x14)
                            }
                        }
                        _ => None,
                    };
                    if let Some(s) = snd {
                        self.events.push(SimEvent::PermSound(s));
                    }
                }
            }
            // Bonus drain: every pf(0xc6) ticks award end_bonus_step to each
            // active player until the total is paid out.
            6 => {
                if self.end_bonus_remaining <= 0 {
                    self.end_seq_state = 7;
                    self.end_seq_t0 = now;
                    return;
                }
                if now.saturating_sub(self.end_seq_drain_t) > pf(assets, 0xc6) as u32 {
                    self.end_seq_drain_t = now;
                    self.end_bonus_remaining -= self.end_bonus_step;
                    if self.end_bonus_remaining < 0 {
                        self.end_bonus_remaining = 0;
                    }
                    for slot in 0..2 {
                        self.add_score(slot, self.end_bonus_step, false, assets);
                    }
                    self.events.push(SimEvent::PermSound(0x14));
                }
            }
            7 => {
                if elapsed > pf(assets, 0xc9) as i64 {
                    self.end_seq_state = 8;
                    self.end_seq_t0 = now;
                }
            }
            // Campaign check: G_Level_GetNumInLevelList() is always 12 —
            // the perfect bonus only runs when perfectCount == 12 exactly.
            8 => {
                if elapsed > pf(assets, 0xca) as i64 {
                    if self.perfect_count != 12 {
                        self.end_seq_state = 0;
                    } else {
                        self.end_seq_state = 9;
                        self.end_seq_t0 = now;
                        self.end_seq_counter = 0;
                        self.events.push(SimEvent::PermSound(0x17));
                    }
                }
            }
            // Perfect-campaign bonus: pf(0xcd) raw score to both players,
            // pf(0xd0) times, alternating 9/10 waits.
            9 => {
                if elapsed > pf(assets, 0xce) as i64 {
                    self.end_seq_state = 10;
                    self.end_seq_t0 = now;
                }
            }
            10 => {
                if elapsed > pf(assets, 0xcf) as i64 {
                    self.end_seq_counter += 1;
                    self.end_seq_t0 = now;
                    for slot in 0..2 {
                        self.add_score(slot, pf(assets, 0xcd) as i64, true, assets);
                    }
                    if self.end_seq_counter < pf(assets, 0xd0) as u32 {
                        self.end_seq_state = 9;
                    } else {
                        self.end_seq_state = 0;
                    }
                }
            }
            _ => self.end_seq_state = 0,
        }
        // Sequence finished: arm the staged MoneyCounter tallies (every
        // in-game player got one at level end, purse or not).
        if self.end_seq_state == 0 {
            let now = self.game_time;
            for p in self.players.iter_mut().flatten() {
                if p.money_tally.phase == 0 && p.money_tally.staged {
                    p.money_tally.phase = 1;
                    p.money_tally.t0 = now;
                    p.money_tally.drain_t = now;
                }
            }
        }
    }

    /// Advance each player's money tally through the original's 8 phases.
    /// Phase timing uses the same perm floats: appear 10, then 20/20/20/30
    /// staged waits, a 2-tick drain cadence, then 30/20 fade-out waits.
    fn step_money_tallies(&mut self, assets: &GameAssets) {
        for slot in 0..2 {
            let mut tally = match self.players[slot].as_ref() {
                Some(p) if p.money_tally.phase != 0 => p.money_tally,
                _ => continue,
            };
            let now = self.game_time;
            let elapsed = now.saturating_sub(tally.t0) as i64;
            match tally.phase {
                // Staged waits: 1→2 (pf 0xac), 2→5 (pf 0xad each), then
                // phase 6 drains, 7→8 (pf 0xae), 8→done (pf 0xaf). The
                // original plays perm sound 0x14 on each transition, or
                // 0x15/0x14 at the 4→5 reveal depending on the remainder.
                1 | 2 | 3 | 4 | 5 => {
                    let need = if tally.phase == 1 {
                        pf(assets, 0xac) as i64
                    } else {
                        pf(assets, 0xad) as i64
                    };
                    if elapsed > need {
                        let leaving = tally.phase;
                        tally.phase += 1;
                        tally.t0 = now;
                        if tally.phase == 6 {
                            tally.drain_t = now;
                        }
                        let snd = if leaving == 4 {
                            if tally.remaining <= 0 { 0x15 } else { 0x14 }
                        } else {
                            0x14
                        };
                        self.events.push(SimEvent::PermSound(snd));
                    }
                }
                6 => {
                    if tally.remaining > 0 {
                        let delay = pf(assets, 0xb0) as u32;
                        if now.saturating_sub(tally.drain_t) >= delay {
                            tally.drain_t = now;
                            if let Some(p) = self.players[slot].as_mut() {
                                if p.money > 0 {
                                    p.money -= 1;
                                }
                            }
                            self.add_score(slot, tally.chunk, false, assets);
                            tally.remaining -= tally.chunk;
                            self.events
                                .push(SimEvent::PlayerEvent(slot, "money_tick"));
                        }
                    } else {
                        tally.phase = 7;
                        tally.t0 = now;
                    }
                }
                7 => {
                    if elapsed > pf(assets, 0xae) as i64 {
                        tally.phase = 8;
                        tally.t0 = now;
                    }
                }
                8 => {
                    if elapsed > pf(assets, 0xaf) as i64 {
                        tally.phase = 0;
                    }
                }
                _ => tally.phase = 0,
            }
            if let Some(p) = self.players[slot].as_mut() {
                p.money_tally = tally;
            }
        }
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
        if p.glow_ticks > 0 {
            p.glow_ticks -= 1;
        }
        if p.weapon_popup > 0 {
            p.weapon_popup -= 1;
        }

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
                // Priv_ProcessState state 3: finalDyingTime on the last
                // life, dyingTime otherwise. The life is consumed when the
                // death anim completes, not at the hit.
                let dur = if p.lives == 1 {
                    def.final_dying_time
                } else {
                    def.dying_time
                };
                if p.phase_time >= dur.max(1) as u32 {
                    p.lives = (p.lives - 1).max(0);
                    if p.lives > 0 {
                        // Priv_Appear: reposition, reset shields and weapon
                        // charge state, replay the entry effect.
                        p.phase = PlayerPhase::Active;
                        let (ex, ey) = if two_p {
                            (def.entry_multi_start_x as f32, def.entry_multi_start_y as f32)
                        } else {
                            (def.entry_solo_start_x as f32, def.entry_solo_start_y as f32)
                        };
                        p.screen = Vec2::new(PLAY_LEFT + ex, ey);
                        p.vel = Vec2::ZERO;
                        p.shields = def.default_shield_percentage as f64;
                        p.invuln = def.entry_invulnerability_time as u32;
                        p.phase_time = 0;
                        p.air_power = PowerupRt::default();
                        p.gnd_power = PowerupRt::default();
                        p.power = 0.0;
                        p.gpower = 0.0;
                        if def.entry_spawn != NONE_ID {
                            let w = Vec2::new(
                                p.screen.x + self.cam_x - WORLD_TO_SCREEN_X,
                                self.scroll + SCREEN_H - p.screen.y,
                            );
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
                    } else {
                        p.phase = PlayerPhase::Dead;
                        p.phase_time = 0;
                        self.events.push(SimEvent::PlayerEvent(slot, "dead"));
                    }
                }
                return;
            }
            PlayerPhase::Dead => return,
            PlayerPhase::Active => {}
        }

        if p.invuln > 0 { p.invuln -= 1; }
        if p.fire_timer > 0 { p.fire_timer -= 1; }
        // Shields do not passively regenerate in the original — they
        // recover via "shie" pickups and respawn resets only. The power
        // meters are driven by the weapon power-up state machine.

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

        // Ground weapon (G_WeaponHandler Process, m_field_0x87/0x7b/0x7f):
        // a fresh bomb press after the recharge interval arms an
        // auto-burst of min(pf(0x97)+level-1, pf(0x98)) shots; while the
        // burst counter is positive it fires one shot every
        // delayBetweenLoadLaunches ticks, even after release. Blocked
        // while the ground power-up machine is not idle (m_field_0x35).
        let now = self.game_time;
        let level_num = self.level_number;
        let mut fire_gnd = false;
        {
            let gw = assets.weapons.get(&p.ground_weapon).cloned();
            let recharge = gw.as_ref().map(|w| w.delay_between_launches).unwrap_or(0).max(0) as u32;
            let spacing = gw.as_ref().map(|w| w.delay_between_load_launches).unwrap_or(0).max(0) as u32;
            if p.bomb_burst > 0 {
                if now > p.bomb_last.saturating_add(spacing) {
                    p.bomb_burst -= 1;
                    fire_gnd = true;
                    p.bomb_last = now;
                }
            } else if inp.bomb && !p.prev_bomb && p.gnd_power.state == 0 {
                // Priv_CheckSpawning_Ground.
                if now > p.bomb_last.saturating_add(recharge) {
                    let base = pf(assets, pf::WEAPON_BASE_BOMBS) as i64;
                    let cap = pf(assets, pf::WEAPON_MAX_BOMBS) as i64;
                    let burst = (base + (level_num - 1).max(0)).min(cap) - 1;
                    p.bomb_burst = burst.max(0);
                    p.bomb_last = now;
                    fire_gnd = true;
                }
            }
            p.prev_bomb = inp.bomb;
        }
        let fire_air = inp.fire && p.fire_timer == 0;
        // Firing.
        if fire_gnd {
            self.fire_ground_weapon(assets, slot);
        }
        if fire_air {
            self.fire_weapon(assets, slot);
        }
        // Weapon charge/release power-up machines (air = fire held,
        // ground = bomb held).
        self.step_powerup(assets, slot, false, inp.fire);
        self.step_powerup(assets, slot, true, inp.bomb);
        // Select Air Weapon key: cycle to the next available PEAA weapon
        // (G_WeaponHandler inC → GetNextRefPtrByType).
        if inp.cycle {
            if let Some(p) = self.players[slot].as_mut() {
                if let Some(w) = next_weapon_after(assets, id("PEAA"), p.air_weapon, self.level_number) {
                    if w != p.air_weapon {
                        p.air_weapon = w;
                        p.weapon_popup = 60;
                        self.events.push(SimEvent::PlayerEvent(slot, "weapon_cycle"));
                    }
                }
            }
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
            // m_fielda4: activation delay — decrement, skip while >0.
            let e = &mut self.entities[i];
            e.activate_in = e.activate_in.saturating_sub(1);
            if e.activate_in > 0 {
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
                e.glow.process();
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

            // --- rotate_to_target sweep (Priv_RotateToTargetLoc): step the
            // direction frame toward the tracked player, one direction per
            // frame_delay ticks, in the direction of the wrapped angle.
            if self.entities[i].tracking {
                let nd = state.num_directions.max(1);
                if nd > 1 {
                    let epos = self.entities[i].pos;
                    if let Some((tp, _)) = self.nearest_player(epos) {
                        let target_dir = heading_to_dir(
                            vector_to_heading(tp - epos),
                            nd as usize,
                        );
                        let cur = if self.entities[i].rot_dir >= 0 {
                            self.entities[i].rot_dir
                        } else {
                            heading_to_dir(self.entities[i].heading, nd as usize)
                        };
                        let delay = state.frame_delay.max(1) as u32;
                        if target_dir != cur
                            && self.game_time
                                > self.entities[i].rot_last + delay
                        {
                            let mut diff = (target_dir - cur).rem_euclid(nd);
                            if diff > nd / 2 {
                                diff -= nd;
                            }
                            self.entities[i].rot_dir =
                                (cur + diff.signum()).rem_euclid(nd);
                            self.entities[i].rot_last = self.game_time;
                            self.entities[i].frame_timer = 0;
                        } else if self.entities[i].rot_dir < 0 {
                            self.entities[i].rot_dir = cur;
                        }
                    }
                }
            } else {
                self.entities[i].rot_dir = -1;
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

            // --- DoMovementAI ---
            let epos0 = self.entities[i].pos;
            let fleeing = self.entities[i].fleeing;
            let mut pending_change: Option<String> = None;

            if fleeing {
                // m_field_0xc0: MoveToTargetLoc toward the flee destination
                // (Priv_RotateToTargetLoc runs via the rot_dir sweep).
                if let Some(t) = self.entities[i].flee_target {
                    self.move_to_target_loc(i, &state, t);
                }
            } else {
                let target = self.nearest_player(epos0);
                let mut fled = false;
                if target.is_none() {
                    if state.delete_on_no_active_players {
                        self.entities[i].dead = true;
                        continue;
                    }
                    if state.destruct_on_no_active_players {
                        let sp = self.entities[i].spawner_player;
                        self.destroy_entity(assets, i, sp);
                        continue;
                    }
                    // No active players: unit flee flags trigger Priv_Flee
                    // with the north/south flee ids, then skip motion AI.
                    if let Some(d) = assets.units.get(&self.entities[i].unit) {
                        let fid = if d.flees_north_on_no_players {
                            Some(id("nora"))
                        } else if d.flees_south_on_no_players {
                            Some(id("sora"))
                        } else {
                            None
                        };
                        if let Some(fid) = fid {
                            self.begin_flee(i, assets, fid);
                            fled = true;
                        }
                    }
                }
                if !fled {
                    // Cyclic walk + game-area constrain run before the
                    // target-move decision (original DoMovementAI order).
                    if state.cyclic_motion {
                        self.step_cyclic(i, &state);
                    }
                    let constrain = assets
                        .units
                        .get(&self.entities[i].unit)
                        .map(|d| d.constrain_in_game_area)
                        .unwrap_or(false);
                    if constrain {
                        self.constrain_in_game_area(i, assets);
                    }
                    let mut hold = false;
                    let mut reverse: Option<(Vec2, f32)> = None;
                    let mut target_pos: Option<Vec2> = None;
                    if let Some((tp, td)) = target {
                        target_pos = Some(tp);
                        let in_range = state.on_range != 0.0 && td < state.on_range as f32;
                        if in_range {
                            let oc = state.on_range_change_to.trim();
                            if !oc.is_empty() && !oc.eq_ignore_ascii_case("none") {
                                pending_change = Some(state.on_range_change_to.clone());
                            }
                            hold = state.hold_position_to_target;
                            if state.reverse_direction_on_reaction {
                                reverse = Some((tp, td));
                            }
                        }
                    }
                    if let Some(name) = pending_change {
                        self.apply_state_change(i, &name, assets);
                        continue;
                    }
                    if let Some(tp) = target_pos {
                        if let Some((rtp, rtd)) = reverse {
                            self.reverse_from_target(i, &state, rtp, rtd);
                        }
                        if hold {
                            self.hold_to_target(i, &state, tp);
                        } else if state.hunts {
                            self.move_to_target_loc(i, &state, tp);
                        } else {
                            self.adjust_to_required_velocity(i, &state);
                        }
                    } else {
                        self.adjust_to_required_velocity(i, &state);
                    }
                }
            }
            // Position integration (G_GameObject::MoveAndCheckPosition):
            // m_x += m_prevX; m_y += m_prevY.
            {
                let e = &mut self.entities[i];
                e.pos += e.vel;
                if e.vel.length_squared() > 0.0001 {
                    e.heading = vector_to_heading(e.vel);
                }
            }

            // --- visibility/tint/scale fades (AdjustVisibilityAndTinting) ---
            self.adjust_visuals(i);

            // --- owner linkage (lock/link/orbit) ---
            self.step_owner_link(i, assets);

            // --- state repeat particles (G_EntityGroup stateParticles_*) ---
            if state.particles_id != NONE_ID {
                let max_bursts = if state.particles_repeat {
                    state.particles_max_bursts.max(0) as u32
                } else {
                    1
                };
                let under_cap = max_bursts == 0 || self.entities[i].particle_bursts < max_bursts;
                let due = self.game_time
                    >= self.entities[i].particle_last
                        + state.particles_repeat_delay.max(0) as u32;
                if under_cap && due {
                    let unit_scroll = assets
                        .units
                        .get(&self.entities[i].unit)
                        .map(|d| d.terrain_effect)
                        .unwrap_or(false);
                    self.emit_particles(
                        i,
                        state.particles_id,
                        state.particles_color,
                        unit_scroll,
                        false,
                    );
                    self.entities[i].particle_bursts += 1;
                    self.entities[i].particle_last = self.game_time;
                }
            }

            // --- state sound loop/repeat (G_EntityGroup flag3c block) ---
            {
                let (play, under_cap) = {
                    let e = &self.entities[i];
                    let mut play = false;
                    if state.entry_sound.id != NONE_ID {
                        if state.sound_loop {
                            // Repeat every sound_loop_delay ticks.
                            if e.sound_last_play == 0
                                || self.game_time
                                    >= e.sound_last_play + state.sound_loop_delay.max(0) as u32
                            {
                                play = true;
                            }
                        } else if e
                            .state_sound_played
                            .get(e.state)
                            .copied()
                            .unwrap_or(false)
                            && e.sound_play_count == 0
                        {
                            play = true;
                        } else if state.sound_repeat_on_state_change && e.sound_play_count == 0 {
                            play = true;
                        }
                    }
                    let under_cap = state.sound_max_num_to_play <= 0
                        || e.sound_play_count < state.sound_max_num_to_play as u32;
                    (play, under_cap)
                };
                if play && under_cap {
                    // max_num_to_play caps total plays; 0 = uncapped here
                    // (the original gates the whole block on it — entry
                    // sounds still fire once via change_state).
                    self.events.push(SimEvent::Sound(state.entry_sound.id));
                    let e = &mut self.entities[i];
                    e.sound_play_count += 1;
                    e.sound_last_play = self.game_time;
                }
            }

            // --- motion blur afterimages (G_MotionBlur_New trigger) ---
            if state.motion_blur_required {
                let interval = rand_int(
                    &mut self.rng,
                    state.motion_blur_min_time,
                    state.motion_blur_max_time.max(state.motion_blur_min_time),
                ) as u32;
                if self.game_time > self.entities[i].blur_last + interval {
                    let (unit, state_i, frame, sx, sy, scale) = {
                        let e = &self.entities[i];
                        (
                            e.unit,
                            e.state,
                            e.frame,
                            e.pos.x - self.cam_x + WORLD_TO_SCREEN_X,
                            self.scroll + SCREEN_H - e.pos.y,
                            e.scale,
                        )
                    };
                    self.events.push(SimEvent::MotionBlur {
                        unit,
                        state: state_i,
                        frame,
                        x: sx,
                        y: sy,
                        scale,
                        vis0: state.motion_blur_initial_visibility as f32,
                        vis_d: state.motion_blur_visibility_delta as f32,
                    });
                    self.entities[i].blur_last = self.game_time;
                }
            }

            // --- debris / ground-obstacle collision ---
            self.step_debris(i, assets);

            // --- spawn sets ---
            self.step_spawn_sets(i, &state);

            // --- cull ---
            let e = &self.entities[i];
            let sy = self.scroll + SCREEN_H - e.pos.y;
            let sx = e.pos.x - self.cam_x + WORLD_TO_SCREEN_X;
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
                self.change_state(idx, ns, def, assets);
            }
        }
    }

    fn change_state(&mut self, idx: usize, new_state: usize, def: &UnitDef, assets: &GameAssets) {
        let ns = new_state.min(def.states.len() - 1);
        let epos = self.entities[idx].pos;
        let flee_target = if def.states[ns].flee_id != NONE_ID {
            self.compute_flee_target(assets, def.states[ns].flee_id, epos)
        } else {
            None
        };
        let e = &mut self.entities[idx];
        e.state = ns;
        let state_idx = e.state;
        // Priv_Flee on state entry: flee_id != "none" sets m_field_0xc0 and
        // computes the destination; any other state clears the flag.
        e.fleeing = def.states[ns].flee_id != NONE_ID;
        e.flee_target = flee_target;
        e.state_time = 0;
        e.dir_index = -1;
        let st = &def.states[e.state];
        e.state_timer_target = if st.on_timer_max > 0 || st.on_timer_min > 0 {
            rand_int(&mut self.rng, st.on_timer_min, st.on_timer_max.max(st.on_timer_min)) as u32
        } else { 0 };
        e.spawn_timers = vec![SpawnSetRt::default(); st.spawn_sets.len()];
        e.frame = st.sprite_frame_min;
        e.anim_stopped = false;
        e.tracking = false;
        e.rot_dir = -1;
        e.particle_bursts = 0;
        e.particle_last = 0;
        e.sound_play_count = 0;
        e.sound_last_play = 0;
        if e.state_sound_played.len() < def.states.len() {
            e.state_sound_played.resize(def.states.len(), false);
        }
        let has_link = st.lock_to_owner_loc || st.link_to_owner_loc || st.orbit_owner;
        let orbit = st.orbit_owner;
        self.cache_owner_link(idx, has_link, orbit);
        self.state_velocity_init(idx, st, false);
        self.set_visual_targets(idx, st);
        // stateEntrySound plays on every state entry; the per-state-index
        // flag (entity +0x144 array) records it has played once.
        let entry_sound = st.entry_sound.id;
        if entry_sound != NONE_ID {
            self.events.push(SimEvent::Sound(entry_sound));
            let e = &mut self.entities[idx];
            if e.state_sound_played.len() > state_idx {
                e.state_sound_played[state_idx] = true;
            }
        }
    }

    /// `G_Entity::Priv_Flee`: compute the flee destination (m_field_0x114/
    /// 0x118) once per flee-state entry. The original dispatches on 14
    /// fourCC flee-ids; each arm picks an off-screen/edge target from perm
    /// floats (pf 0xe=-1000, 0xf=2000, 0x10=-1000, 0x11=2000, 0x36=416,
    /// 0x37=480) with occasional RandomFloat/RandomInt rolls. Perm-float y
    /// values are screen-band offsets, so world y = scroll + value. The
    /// dat_004dfda8 constants at +0x738/+0x744 are runtime-initialised and
    /// unrecovered; 0.0/1.0 reproduce the observed retail behaviour.
    fn compute_flee_target(&mut self, assets: &GameAssets, flee_id: ResId, pos: Vec2) -> Option<Vec2> {
        let scroll = self.scroll as f64;
        let p = |i: usize| pf(assets, i) as f64;
        let wy = |v: f64| (scroll + v) as f32;
        let rx = |rng: &mut Crand, hi: f64| rand_float(rng, 0.0, hi) as f32;
        let ry = |rng: &mut Crand, hi: f64| (scroll + rand_float(rng, 0.0, hi)) as f32;
        let coin = |rng: &mut Crand| rand_int(rng, 0, 1) != 0;
        let t = if flee_id == id("cega") {
            Some(Vec2::new(p(0x36) as f32, wy(p(0x37))))
        } else if flee_id == id("eara") {
            Some(Vec2::new(rx(&mut self.rng, p(0x36)), wy(p(0xe))))
        } else if flee_id == id("wera") {
            Some(Vec2::new(rx(&mut self.rng, p(0x36)), wy(p(0xf))))
        } else if flee_id == id("nora") {
            Some(Vec2::new(p(0x10) as f32, ry(&mut self.rng, p(0x37))))
        } else if flee_id == id("sora") {
            Some(Vec2::new(p(0x11) as f32, ry(&mut self.rng, p(0x37))))
        } else if flee_id == id("eace") {
            Some(Vec2::new(p(0x36) as f32, wy(p(0xe))))
        } else if flee_id == id("wece") {
            Some(Vec2::new(p(0x36) as f32, wy(p(0xf))))
        } else if flee_id == id("noce") {
            Some(Vec2::new(p(0x10) as f32, wy(p(0x37))))
        } else if flee_id == id("soce") {
            Some(Vec2::new(p(0x11) as f32, wy(p(0x37))))
        } else if flee_id == id("rave") {
            let y = if coin(&mut self.rng) { p(0xe) } else { p(0xf) };
            Some(Vec2::new(rx(&mut self.rng, p(0x36)), wy(y)))
        } else if flee_id == id("opve") {
            let y = if p(0x37) < pos.y as f64 { p(0xe) } else { p(0xf) };
            Some(Vec2::new(rx(&mut self.rng, p(0x36)), wy(y)))
        } else if flee_id == id("raho") {
            let x = if coin(&mut self.rng) { p(0x11) } else { p(0x10) };
            Some(Vec2::new(x as f32, ry(&mut self.rng, p(0x37))))
        } else if flee_id == id("opho") {
            let x = if p(0x36) < pos.x as f64 { p(0x10) } else { p(0x11) };
            Some(Vec2::new(x as f32, ry(&mut self.rng, p(0x37))))
        } else {
            // Unknown flee-id: fall back to a random point in the visible
            // play area (the pre-table behaviour).
            Some(Vec2::new(
                rand_float(&mut self.rng, 32.0, 448.0) as f32,
                rand_float(
                    &mut self.rng,
                    self.scroll as f64,
                    (self.scroll + SCREEN_H) as f64,
                ) as f32,
            ))
        };
        t
    }

    /// Copy a state's visibility/tint/scale requirements onto the entity
    /// (G_EntityGroup state-change block: m_some66/6a, m_some58/5C, scale).
    fn set_visual_targets(&mut self, idx: usize, st: &UnitState) {
        let e = &mut self.entities[idx];
        e.vis_req = st.required_visibility_percent as f32;
        e.vis_delta = st.visibility_delta_percent as f32;
        e.tint_req = st.tint_percent as f32;
        e.tint_delta = st.tint_delta_percent as f32;
        // Scale percentages → factor (U_Sprite_ConvertScalePercentage = /100).
        // A zero requirement means "no scale constraint" → hold 1.0.
        if st.required_scale_percent > 0 {
            e.scale_tgt = st.required_scale_percent as f32 / 100.0;
        }
        e.scale_delta = (st.scale_delta_percent as f32 / 100.0).abs();
    }

    /// Emit a spark burst at an entity's screen position. `scroll` marks
    /// ground-class units whose particles track the terrain.
    fn emit_particles(&mut self, idx: usize, kind: ResId, color: u32, scroll: bool, circular: bool) {
        if kind == NONE_ID {
            return;
        }
        let e = &self.entities[idx];
        let sx = e.pos.x - self.cam_x + WORLD_TO_SCREEN_X;
        let sy = self.scroll + SCREEN_H - e.pos.y;
        self.events.push(SimEvent::Particles {
            x: sx,
            y: sy,
            kind,
            color,
            scroll,
            circular,
        });
    }

    /// Leave a debris zone at an entity's bounds (G_Debris_New).
    fn add_debris(&mut self, idx: usize, assets: &GameAssets) {
        let e = &self.entities[idx];
        let r = assets
            .units
            .get(&e.unit)
            .map(entity_radius)
            .unwrap_or(8.0);
        self.debris.push([
            e.pos.x - r,
            e.pos.y - r,
            e.pos.x + r,
            e.pos.y + r,
        ]);
        if self.debris.len() > 512 {
            self.debris.remove(0);
        }
    }

    /// `G_Entity::Destroy`-adjacent debris: units flagged
    /// destructCreateObstacle leave a collision zone where they die.
    fn maybe_debris_on_destroy(&mut self, idx: usize, assets: &GameAssets) {
        let flagged = assets
            .units
            .get(&self.entities[idx].unit)
            .map(|d| d.destruct_create_obstacle)
            .unwrap_or(false);
        if flagged {
            self.add_debris(idx, assets);
        }
    }

    /// Debris collision (G_EntityGroup tick block): entities flagged
    /// collidesWithGroundObstacles that touch a debris zone are reset to
    /// their entry position once (m_field134), and if they also create
    /// obstacles their bounds become new debris.
    fn step_debris(&mut self, idx: usize, assets: &GameAssets) {
        let (collides, creates, hit, entry) = {
            let e = &self.entities[idx];
            let def = match assets.units.get(&e.unit) {
                Some(d) => d,
                None => return,
            };
            (
                def.collides_with_ground_obstacles,
                def.destruct_create_obstacle,
                e.debris_hit,
                e.entry_pos,
            )
        };
        if !collides || hit || self.debris.is_empty() {
            return;
        }
        let e = &self.entities[idx];
        let r = assets
            .units
            .get(&e.unit)
            .map(entity_radius)
            .unwrap_or(8.0);
        let (x0, y0, x1, y1) = (e.pos.x - r, e.pos.y - r, e.pos.x + r, e.pos.y + r);
        let hit_debris = self.debris.iter().any(|d| {
            x1 >= d[0] && x0 <= d[2] && y1 >= d[1] && y0 <= d[3]
        });
        if hit_debris {
            let e = &mut self.entities[idx];
            e.pos = entry;
            e.debris_hit = true;
            if creates {
                self.add_debris(idx, assets);
            }
        }
    }

    /// `G_GameObject::AdjustVisibilityAndTinting` + `DoScaling`: nudge the
    /// current values toward their per-state targets each tick.
    fn adjust_visuals(&mut self, idx: usize) {
        let e = &mut self.entities[idx];
        if e.vis_req < e.vis {
            e.vis = (e.vis - e.vis_delta).max(e.vis_req).max(0.0);
        } else if e.vis < e.vis_req {
            e.vis = (e.vis + e.vis_delta).min(e.vis_req);
        }
        if e.tint_req < e.tint {
            e.tint = (e.tint - e.tint_delta).max(e.tint_req).max(0.0);
        } else if e.tint < e.tint_req {
            e.tint = (e.tint + e.tint_delta).min(e.tint_req);
        }
        if e.scale > e.scale_tgt {
            e.scale = (e.scale - e.scale_delta).max(e.scale_tgt);
        } else if e.scale < e.scale_tgt {
            e.scale = (e.scale + e.scale_delta).min(e.scale_tgt);
        }
    }

    /// `G_Entity::ChangeState` velocity block: on state entry the entity's
    /// velocity is retargeted — `vdelta` (0x108/0x10c) becomes the per-tick
    /// step toward a speed `mag` one `delta`-sized move closer to
    /// `max_speed`, and `tvel` (0x100/0x104) the full target velocity along
    /// the (inverted) heading. `stateLockToOwnerLoc` (0x32a) zeroes all
    /// velocity fields. `spawn_like` selects the inFlag1 path (spawn
    /// heading at entity+0x130); later changes derive the angle from the
    /// current velocity.
    fn state_velocity_init(&mut self, idx: usize, st: &UnitState, spawn_like: bool) {
        let e = &mut self.entities[idx];
        if st.lock_to_owner_loc {
            e.vel = Vec2::ZERO;
            e.tvel = Vec2::ZERO;
            e.vdelta = Vec2::ZERO;
            return;
        }
        let speed = e.vel.length();
        let heading = if spawn_like || e.vel.length_squared() <= 0.0001 {
            e.heading
        } else {
            vector_to_heading(e.vel)
        };
        let angle = invert_angle(heading);
        let threshold = st.max_speed as f32;
        let margin = st.delta as f32;
        let diff = (threshold - speed).abs();
        let step = margin.min(diff);
        let mag = if threshold > speed { speed + step } else { speed - step };
        let tmp = vec_from_angle_speed(angle, mag);
        e.vdelta = tmp - e.vel;
        e.tvel = vec_from_angle_speed(angle, threshold);
    }

    /// `sub_01cbc0`: entities spawned into a cyclic-motion state get a
    /// randomized initial velocity — scale 1.0 or 1.4, direction components
    /// `RandomInt(1,4) + RandomInt(1,100)/100` sharing one fraction roll,
    /// independent sign flips (the y-flip roll only happens when
    /// `pf(0x37)/4 < locY`). Normalized dir * scale becomes the velocity;
    /// the cyclic walk delta is dir * 0.2 (dat_4e34d0).
    fn scatter_cyclic_spawn(&mut self, assets: &GameAssets, idx: usize) {
        let loc_y = {
            let e = &self.entities[idx];
            let is_cyclic = assets
                .units
                .get(&e.unit)
                .and_then(|d| d.states.get(e.state))
                .map(|st| st.cyclic_motion)
                .unwrap_or(false);
            if !is_cyclic {
                return;
            }
            e.pos.y
        };
        let scale = if rand_int(&mut self.rng, 0, 1) != 0 { 1.0f32 } else { 1.4 };
        let div = rand_int(&mut self.rng, 1, 100) as f32 / 100.0;
        let mut dx = rand_int(&mut self.rng, 1, 4) as f32 + div;
        if rand_int(&mut self.rng, 0, 1) != 0 {
            dx = -dx;
        }
        let mut dy = rand_int(&mut self.rng, 1, 4) as f32 + div;
        if (((pf(assets, 0x37) as i64) / 4) as f32) < loc_y
            && rand_int(&mut self.rng, 0, 1) != 0
        {
            dy = -dy;
        }
        let len = (dx * dx + dy * dy).sqrt();
        let (ux, uy) = (dx / len, dy / len);
        let e = &mut self.entities[idx];
        e.vel = Vec2::new(ux * scale, uy * scale);
        e.tvel = e.vel;
        e.vdelta = Vec2::new(ux * 0.2, uy * 0.2);
    }

    /// `G_EG_CacheOwnerLocOffsetsAndAngles`: snapshot the owner-relative
    /// offset (and orbit radius/angle for orbit states) on state entry.
    fn cache_owner_link(&mut self, idx: usize, has_link: bool, orbit: bool) {
        if !has_link {
            return;
        }
        let self_pos = self.entities[idx].pos;
        let Some(owner_pos) = self.owner_pos(idx) else { return };
        let e = &mut self.entities[idx];
        e.owner_prev = owner_pos;
        e.owner_off = self_pos - owner_pos;
        if orbit {
            // radius = current distance, angle = bearing owner→self.
            let dist = e.owner_off.length();
            e.orbit_radius = if dist > 0.001 { dist } else { f32::NAN };
            e.orbit_angle = if dist > 0.001 {
                vector_to_heading(e.owner_off)
            } else {
                0.0
            };
        }
    }

    /// Resolve the position of an entity's owner: the owner entity by
    /// unique_num, falling back to the spawning player (original m_fieldca).
    fn owner_pos(&self, idx: usize) -> Option<Vec2> {
        let e = &self.entities[idx];
        if e.owner != u32::MAX {
            if let Some(o) = self.entities.iter().find(|o| o.unique_num == e.owner && !o.dead) {
                return Some(o.pos);
            }
        }
        if e.spawner_player >= 0 {
            if let Some(p) = self.players[e.spawner_player as usize].as_ref() {
                if p.phase == PlayerPhase::Active {
                    return Some(p.world_pos(self));
                }
            }
        }
        None
    }

    /// `G_Entity::Priv_DoCyclicMotion`: the velocity itself random-walks —
    /// per-tick step is `RandomInt(amp/2, amp) + RandomInt(1, 100) / 100`
    /// with amp = stateMaxSpeed; vel clamps to ±step flipping the walk
    /// delta sign, then vel += delta and tvel mirrors vel.
    fn step_cyclic(&mut self, idx: usize, state: &UnitState) {
        let amp = (state.max_speed as i64).abs().max(2);
        let step = rand_int(&mut self.rng, amp / 2, amp) as f32
            + rand_int(&mut self.rng, 1, 100) as f32 / 100.0;
        let e = &mut self.entities[idx];
        if e.vel.x > step {
            e.vel.x = step;
            e.vdelta.x = -e.vdelta.x;
        }
        if e.vel.x < -step {
            e.vel.x = -step;
            e.vdelta.x = -e.vdelta.x;
        }
        if e.vel.y > step {
            e.vel.y = step;
            e.vdelta.y = -e.vdelta.y;
        }
        if e.vel.y < -step {
            e.vel.y = -step;
            e.vdelta.y = -e.vdelta.y;
        }
        e.vel.x += e.vdelta.x;
        e.vel.y += e.vdelta.y;
        e.tvel = e.vel;
    }

    /// `G_Entity::Priv_MoveToTargetLoc`: accelerate vel toward `target_pt`
    /// per-axis — vdelta = ±margin by position sign, vel += vdelta clamped
    /// to ±target per axis. Fleeing entities use fleeSpeed/fleeDelta;
    /// others maxSpeed/delta.
    fn move_to_target_loc(&mut self, idx: usize, state: &UnitState, target_pt: Vec2) {
        let fleeing = self.entities[idx].fleeing;
        let (tmax, margin) = if fleeing {
            (state.flee_speed as f32, state.flee_delta as f32)
        } else {
            (state.max_speed as f32, state.delta as f32)
        };
        let e = &mut self.entities[idx];
        e.vdelta.x = if e.pos.x < target_pt.x { margin } else { -margin };
        e.vdelta.y = if e.pos.y < target_pt.y { margin } else { -margin };
        e.vel.x += e.vdelta.x;
        e.vel.x = e.vel.x.clamp(-tmax, tmax);
        e.vel.y += e.vdelta.y;
        e.vel.y = e.vel.y.clamp(-tmax, tmax);
    }

    /// `G_Entity::Priv_AdjustToRequiredVelocity`: ease vel toward tvel by
    /// vdelta per axis without overshoot. Stationary entities and
    /// orbit-owner states get the original's degenerate paths.
    fn adjust_to_required_velocity(&mut self, idx: usize, state: &UnitState) {
        let e = &mut self.entities[idx];
        if e.stationary {
            e.vel = Vec2::ZERO;
            e.tvel = Vec2::ZERO;
            e.vdelta = Vec2::ZERO;
            return;
        }
        if state.orbit_owner {
            // Scalar path (stateOrbitOwner, 0x32c): ease vel.x toward
            // maxSpeed at delta rate.
            let target = state.max_speed as f32;
            let margin = state.delta as f32;
            if e.vel.x < target {
                e.vel.x = (e.vel.x + margin).min(target);
            } else if e.vel.x > target {
                e.vel.x = (e.vel.x - margin).max(target);
            }
            return;
        }
        if e.vel.x < e.tvel.x {
            e.vel.x += e.vdelta.x;
            if e.vel.x > e.tvel.x {
                e.vel.x = e.tvel.x;
            }
        } else if e.vel.x > e.tvel.x {
            e.vel.x += e.vdelta.x;
            if e.vel.x < e.tvel.x {
                e.vel.x = e.tvel.x;
            }
        }
        if e.vel.y < e.tvel.y {
            e.vel.y += e.vdelta.y;
            if e.vel.y > e.tvel.y {
                e.vel.y = e.tvel.y;
            }
        } else if e.vel.y > e.tvel.y {
            e.vel.y += e.vdelta.y;
            if e.vel.y < e.tvel.y {
                e.vel.y = e.tvel.y;
            }
        }
    }

    /// `G_Entity::Priv_HoldToTarget`: drift AWAY from the target point —
    /// vdelta = ∓holdDelta per axis, vel += vdelta clamped ±holdMaxSpeed.
    fn hold_to_target(&mut self, idx: usize, state: &UnitState, target_pt: Vec2) {
        let cap = state.hold_max_speed as f32;
        let margin = state.hold_delta as f32;
        let e = &mut self.entities[idx];
        e.vdelta.x = if e.pos.x < target_pt.x { -margin } else { margin };
        e.vdelta.y = if e.pos.y < target_pt.y { -margin } else { margin };
        e.vel.x = (e.vel.x + e.vdelta.x).clamp(-cap, cap);
        e.vel.y = (e.vel.y + e.vdelta.y).clamp(-cap, cap);
    }

    /// `G_Entity::Priv_ReverseFromTarget`: retarget tvel to maxSpeed along
    /// the direction AWAY from the target (used with
    /// stateReverseDirectionOnReaction).
    fn reverse_from_target(&mut self, idx: usize, state: &UnitState, target_pt: Vec2, dist: f32) {
        let e = &mut self.entities[idx];
        if dist > 0.001 {
            let dx = (target_pt.x - e.pos.x) / dist;
            let dy = (target_pt.y - e.pos.y) / dist;
            let scale = state.max_speed as f32;
            e.tvel = Vec2::new(-dx * scale, -dy * scale);
        }
    }

    /// `G_Entity::Priv_ConstrainInGameArea`: bounce-clamp position to the
    /// playfield, flipping vel, vdelta and tvel on each bounce. The
    /// original bounds are screen-space (x ∈ [-32, pf36+32],
    /// y ∈ [0, pf37]); expressed in world coords that is the visible band.
    fn constrain_in_game_area(&mut self, idx: usize, assets: &GameAssets) {
        let w = self.level_width();
        let h = pf(assets, pf::VISIBLE_GAME_HEIGHT) as f32;
        let e = &mut self.entities[idx];
        let r = entity_radius(assets.units.get(&e.unit).unwrap()) * 0.5;
        if e.pos.x < r {
            e.pos.x = r;
            e.vel.x = -e.vel.x;
            e.vdelta.x = -e.vdelta.x;
            e.tvel.x = -e.tvel.x;
        } else if e.pos.x > w - r {
            e.pos.x = w - r;
            e.vel.x = -e.vel.x;
            e.vdelta.x = -e.vdelta.x;
            e.tvel.x = -e.tvel.x;
        }
        let min_y = self.scroll;
        let max_y = self.scroll + h;
        if e.pos.y < min_y {
            e.pos.y = min_y;
            e.vel.y = -e.vel.y;
            e.vdelta.y = -e.vdelta.y;
            e.tvel.y = -e.tvel.y;
        } else if e.pos.y > max_y {
            e.pos.y = max_y;
            e.vel.y = -e.vel.y;
            e.vdelta.y = -e.vdelta.y;
            e.tvel.y = -e.tvel.y;
        }
    }

    /// Begin a dynamic flee (no-active-players paths): sets m_field_0xc0
    /// and computes the destination through the flee-id arm table.
    fn begin_flee(&mut self, idx: usize, assets: &GameAssets, flee_id: ResId) {
        let pos = self.entities[idx].pos;
        let target = self.compute_flee_target(assets, flee_id, pos);
        let e = &mut self.entities[idx];
        e.fleeing = true;
        e.flee_target = target;
    }

    /// Per-tick owner linkage (G_EG_Process tail): lock → link → orbit.
    fn step_owner_link(&mut self, idx: usize, assets: &GameAssets) {
        let (lock, link, orbit, reflect) = {
            let e = &self.entities[idx];
            let Some(def) = assets.units.get(&e.unit) else { return };
            let Some(st) = def.states.get(e.state) else { return };
            (
                st.lock_to_owner_loc,
                st.link_to_owner_loc,
                st.orbit_owner,
                st.visually_reflect_owner_hits,
            )
        };
        if !(lock || link || orbit || reflect) {
            return;
        }
        let Some(owner_pos) = self.owner_pos(idx) else { return };
        let e = &mut self.entities[idx];
        if lock {
            e.pos = owner_pos + e.owner_off;
        }
        if link {
            // Rigid translation: move by the owner's delta since last tick.
            let delta = owner_pos - e.owner_prev;
            e.pos += delta;
            e.owner_prev = owner_pos;
        }
        if orbit {
            // angleStep ≈ original (int)m_prevX — smoothed x-velocity.
            let angle_step = e.vel.x as i32;
            if !e.orbit_radius.is_nan() && angle_step != 0 {
                e.orbit_angle = (e.orbit_angle + angle_step as f32).rem_euclid(360.0);
                let hr = e.orbit_angle.to_radians();
                e.pos = owner_pos + Vec2::new(hr.sin(), hr.cos()) * e.orbit_radius;
            } else {
                e.pos = owner_pos + e.owner_off;
            }
            // Recompute the cached offset from the new position.
            e.owner_off = e.pos - owner_pos;
        }
        // visuallyReflectOwnerHits: mirror the owner's hit-flash glow
        // (sub_41b5d0 copies the owner's glow fields each tick).
        if reflect {
            let owner_uid = self.entities[idx].owner;
            if owner_uid != u32::MAX {
                let glow = self
                    .entities
                    .iter()
                    .find(|o| o.unique_num == owner_uid && !o.dead)
                    .map(|o| o.glow);
                if let Some(glow) = glow {
                    self.entities[idx].glow = glow;
                }
            }
        }
    }

    fn step_animation(&mut self, idx: usize, state: &UnitState) {
        let e = &mut self.entities[idx];
        let num_dirs = state.num_directions.max(1) as usize;
        let frame_min = state.sprite_frame_min;
        let frame_max = state.sprite_frame_max.max(frame_min);
        let span = (frame_max - frame_min + 1).max(1) as usize;
        let per_dir = (span / num_dirs).max(1) as i64;
        // rotate_to_target entities sweep rot_dir toward the player;
        // everyone else derives direction from heading.
        let dir = if e.tracking && e.rot_dir >= 0 {
            e.rot_dir.rem_euclid(num_dirs as i64)
        } else {
            heading_to_dir(e.heading, num_dirs)
        };
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
            let r = rand_int(&mut self.rng, 0, per_dir - 1);
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
        // spawnIfFleeing (spawnDef+0x4e): while the parent is fleeing
        // (m_field_0xc0), only flagged spawn sets may fire.
        let fleeing = state.flee_id != NONE_ID;
        for (si, ss) in state.spawn_sets.iter().enumerate() {
            if si >= timers.len() || ss.spawn_id == NONE_ID {
                continue;
            }
            if fleeing && !ss.spawn_if_fleeing {
                continue;
            }
            let rt = &mut timers[si];
            if rt.done {
                continue;
            }
            if !rt.started {
                rt.started = true;
                rt.next_volley = rand_int(&mut self.rng, ss.delay_between_min, ss.delay_between_max.max(ss.delay_between_min)) as u32;
                rt.shots_left = rand_int(&mut self.rng, ss.volley_min, ss.volley_max.max(ss.volley_min)) as u32;
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
                    rt.shots_left = rand_int(&mut self.rng, ss.volley_min, ss.volley_max.max(ss.volley_min)) as u32;
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
            rt.shot_timer = rand_int(&mut self.rng, ss.rate_min, ss.rate_max.max(ss.rate_min)) as u32;
            if rt.shots_left == 0 {
                rt.next_volley = rand_int(&mut self.rng, ss.delay_between_min, ss.delay_between_max.max(ss.delay_between_min)) as u32;
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
            if e.dead || e.activate_in > 0 || e.unit != unit_id {
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
        self.entities.iter().filter(|e| !e.dead && e.activate_in == 0 && e.unit == unit_id).count()
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
        let px = pos.x - self.cam_x + WORLD_TO_SCREEN_X - PLAY_LEFT;
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
            "this entity's visibility is at required level" => {
                (self.entities[idx].vis - self.entities[idx].vis_req).abs() < 0.5
            }
            "this entity's tint is at required level" => {
                (self.entities[idx].tint - self.entities[idx].tint_req).abs() < 0.5
            }
            "this entity's scale is at required level" => {
                (self.entities[idx].scale - self.entities[idx].scale_tgt).abs() < 0.01
            }
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
        // State-level invulnerability gates (0x321/0x322).
        let cur_state = def
            .states
            .get(self.entities[idx].state)
            .cloned()
            .unwrap_or_default();
        if cur_state.invulnerable_until_owner_destroyed {
            let owner_uid = self.entities[idx].owner;
            if owner_uid != u32::MAX
                && self
                    .entities
                    .iter()
                    .any(|o| o.unique_num == owner_uid && !o.dead)
            {
                return;
            }
        }
        if cur_state.invulnerable_until_children_destroyed {
            let my_uid = self.entities[idx].unique_num;
            if self.entities.iter().any(|c| !c.dead && c.owner == my_uid) {
                return;
            }
        }
        // Already-shieldless entities ignore further hits (shieldNonPos).
        if self.entities[idx].shields <= 0.0 {
            return;
        }
        let old_shields = self.entities[idx].shields;
        {
            let e = &mut self.entities[idx];
            e.shields = (e.shields - amount).max(0.0);
            e.hit_cooldown = 2;
            e.hit_count += 1;
        }
        // stateCollides... 0x344: shields do not deplete on collision —
        // the subtraction is rolled back entirely.
        if cur_state.shields_do_not_deplete_on_collision {
            self.entities[idx].shields = old_shields;
        }
        // On-hit state change runs BEFORE the death check (0x3b2/0x596).
        let oc = cur_state.on_hit_change_to.trim();
        if !oc.is_empty()
            && !oc.eq_ignore_ascii_case("none")
            && self.game_time
                > self.entities[idx].hit_change_time
                    + cur_state.on_hit_change_state_delay.max(0) as u32
        {
            self.entities[idx].hit_change_time = self.game_time;
            self.apply_state_change(idx, &cur_state.on_hit_change_to, assets);
        }
        if self.entities[idx].shields <= 0.0 {
            // Score is awarded only on shield depletion (Score_Adjust,
            // non-raw → multiplied).
            if from_player >= 0 {
                self.add_score(from_player as usize, def.score as i64, false, assets);
            }
            self.destroy_entity(assets, idx, from_player);
            return;
        }
        // Survived: Glow_Start(0x7fff,6,0) unless the state opts out
        // (stateDoNotGlowOnCollision, st+0x350).
        if !cur_state.do_not_glow_on_collision {
            self.entities[idx].glow.start();
        }
        // Survived: shield vs unshielded sound — 0x344 picks unshielded.
        let snd = if cur_state.shields_do_not_deplete_on_collision {
            def.unshielded_sound
        } else {
            def.shield_sound
        };
        if snd.id != NONE_ID {
            self.events.push(SimEvent::Sound(snd.id));
        }
        // hitParticles burst on non-lethal hits (G_Entity::Hit).
        self.emit_particles(
            idx,
            def.hit_particles,
            def.hit_particles_color,
            def.terrain_effect,
            def.hit_particle_circular_burst,
        );
        // Collision spawn (state+0x2e0): throttled, repeat-gated, owned
        // by the hit entity itself.
        if cur_state.collision_spawn_id != NONE_ID {
            let can_spawn = if cur_state.collision_repeat_spawns {
                self.game_time as i64
                    >= self.entities[idx].hit_fx_time as i64
                        + cur_state.collision_spawn_delay.max(0) as i64
            } else {
                self.entities[idx].hit_count <= 1
                    && self.game_time as i64
                        >= self.entities[idx].hit_fx_time as i64
                            + cur_state.collision_spawn_delay.max(0) as i64
            };
            if can_spawn {
                let pos = self.entities[idx].pos;
                let owner_idx = idx;
                self.entities[idx].hit_fx_time = self.game_time;
                self.request_spawn(SpawnReq {
                    unit: cur_state.collision_spawn_id,
                    pos,
                    heading: None,
                    owner: owner_idx,
                    spawner_player: from_player,
                    stationary: false,
                    state: None,
                });
            }
        }
    }

    /// `G_Player::Score_Adjust`: score += points (*multiplier unless raw),
    /// then the extra-life threshold check (m_pad3a/m_pad3b port).
    fn add_score(&mut self, slot: usize, points: i64, raw: bool, assets: &GameAssets) {
        let mut life_spawn_req = None;
        let Some(p) = self.players[slot].as_mut() else { return };
        if p.phase == PlayerPhase::Dead {
            return;
        }
        let mult = if raw { 1 } else { p.multiplier.max(1) };
        p.score += points * mult;
        if points <= 0 {
            return;
        }
        if raw {
            // Raw adds set m_pad3b = score + ExtraLifeScoreAdjustment.
            p.life_bonus_acc = p.score + pf(assets, 0xb6) as i64;
            return;
        }
        if p.score > p.next_life_score {
            let (max_lives, step, life_spawn) = assets
                .players
                .get(&p.def_id)
                .map(|d| (d.life_max_num, d.life_additional_required_score, d.life_spawn))
                .unwrap_or((i64::MAX, 30000, NONE_ID));
            if p.lives < max_lives {
                p.lives += 1;
                self.events.push(SimEvent::PlayerEvent(slot, "extra_life"));
                if life_spawn != NONE_ID {
                    // Lives_Add spawns the player's life_Spawn object
                    // (Notice - Extra Life) at the ship.
                    let pos = Vec2::new(
                        p.screen.x + self.cam_x - WORLD_TO_SCREEN_X,
                        self.scroll + SCREEN_H - p.screen.y,
                    );
                    life_spawn_req = Some(SpawnReq {
                        unit: life_spawn,
                        pos,
                        heading: None,
                        owner: usize::MAX,
                        spawner_player: slot as i32,
                        stationary: false,
                        state: None,
                    });
                }
            }
            // m_pad3a += lifeAdditionalRequiredScore + m_pad3b;
            // m_pad3b += ExtraLifeScoreAdjustment.
            p.next_life_score += step + p.life_bonus_acc;
            p.life_bonus_acc += pf(assets, 0xb6) as i64;
        }
        if let Some(req) = life_spawn_req {
            self.request_spawn(req);
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
        let my_gid = self.entities[idx].group_id;

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
        // destructParticle burst (G_Entity::Destroy); ground-class units
        // scroll with the terrain ("grnd" check → terrain_effect proxy).
        self.emit_particles(
            idx,
            def.destruct_particle,
            def.destruct_particle_color,
            def.terrain_effect,
            false,
        );
        // destruct_notice: floating text — TODO notice system (unused in
        // retail data).
        self.maybe_debris_on_destroy(idx, assets);

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

        // Group-kill bonus coin: when the last member of this spawn
        // group goes down, spawn destruct_coin_on_group_kill once.
        if def.destruct_coin_on_group_kill != NONE_ID && my_gid != 0 {
            let any_left = self
                .entities
                .iter()
                .any(|e| !e.dead && e.group_id == my_gid);
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

        // G_Game_GroundAccuracy_AddDestroyed. The rb06 reward is NOT armed
        // here: the original arms it at the NEXT level's start when this
        // level ends at 100% (exe 0x4e4827 → 0x4e4828 transfer).
        if def.include_in_ground_accuracy {
            self.accuracy_destroyed += 1;
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
        let roll = rand_int(&mut self.rng, 0, 100);
        let slot = if roll < pf(assets, 0xd1) as i64 {
            // CheckForRewardThisLevel: at 100% ground accuracy a
            // pf(0xda)-weighted sub-roll swaps RandomBonus_1 for
            // RandomBonus_6 and consumes the reward (SetRewardDone).
            if self.accuracy_reward_pending && roll < pf(assets, 0xda) as i64 {
                self.accuracy_reward_pending = false;
                0x1e
            } else {
                0x19
            }
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

    /// `G_Player::Destroy` — the wreck path shared by fatal hits and weapon
    /// overload. Runs the same death sequence as a lethal damage_player call
    /// without hit FX, hit sound, or the damage latch.
    pub fn destroy_player(&mut self, assets: &GameAssets, slot: usize) {
        let Some(p) = self.players[slot].as_mut() else { return };
        if p.phase != PlayerPhase::Active {
            return;
        }
        p.shields = -1.0;
        p.last_hit_time = 0;
        self.damage_player(assets, slot, 0.0);
    }

    /// `G_Player::Hit` — shield damage = shieldBaseHitPercentage × incoming,
    /// gated by shieldHitDelay game-time cooldown and invulnerability.
    pub fn damage_player(&mut self, assets: &GameAssets, slot: usize, amount: f64) {
        let pdef_id = {
            let Some(p) = self.players[slot].as_ref() else { return };
            if p.phase != PlayerPhase::Active {
                return;
            }
            p.def_id
        };
        let pdef = match assets.players.get(&pdef_id) {
            Some(d) => d.clone(),
            None => return,
        };
        // Hit-delay gate: inB >= m_shieldReset1 + shieldHitDelay.
        let Some(p) = self.players[slot].as_mut() else { return };
        if (self.game_time as i64) < p.last_hit_time as i64 + pdef.shield_hit_delay {
            return;
        }
        p.last_hit_time = self.game_time;
        p.shield_recharge = 30;
        if p.invuln == 0 {
            let dmg = pdef.shield_base_hit_percentage as f64 * amount;
            p.shields -= dmg;
            if dmg > 0.0 {
                // pad7[0]: any real shield damage latches the no-damage
                // defence bonus off for the rest of the run.
                p.damage_taken = true;
                self.events.push(SimEvent::PlayerEvent(slot, "hit"));
            }
        }
        // Hit-effect spawn (active_SpawnOnHit), throttled by pf(0xa2).
        if pdef.active_spawn_on_hit != NONE_ID
            && self.game_time as i64
                >= p.last_hit_fx as i64 + pf(assets, 0xa2) as i64
        {
            p.last_hit_fx = self.game_time;
            let pos = Vec2::new(p.screen.x + self.cam_x - WORLD_TO_SCREEN_X, self.scroll + SCREEN_H - p.screen.y);
            self.request_spawn(SpawnReq {
                unit: pdef.active_spawn_on_hit,
                pos,
                heading: None,
                owner: usize::MAX,
                spawner_player: slot as i32,
                stationary: true,
                state: None,
            });
        }
        // Money burst + death-spawn object on ship destruction
        // (G_Player::Destroy): remaining money converts to coin pickups
        // scattered at the wreck, then the purse resets. Collected after
        // the `p` borrow ends.
        let mut money_burst: Option<(i64, Vec2)> = None;
        let mut death_spawn: Option<(ResId, Vec2)> = None;
        let p = self.players[slot].as_mut().unwrap();
        if p.shields < 0.0 {
            p.shields = 0.0;
            let pos = Vec2::new(
                p.screen.x + self.cam_x - WORLD_TO_SCREEN_X,
                self.scroll + SCREEN_H - p.screen.y,
            );
            death_spawn = Some((pdef.death_spawn, pos));
            if p.money > 0 {
                money_burst = Some((p.money, pos));
                p.money = 0;
            }
            self.events.push(SimEvent::PlayerEvent(slot, "shield_break"));
            // State 3 (dying); the life is consumed when the anim ends.
            p.phase = PlayerPhase::Dying;
            p.phase_time = 0;
        } else if amount > 0.0 && !p.shield_warned
            && (p.shields as i64) <= pdef.shield_warning_percentage
            && pdef.active_shield_warning_object != NONE_ID
        {
            // Low-shield warning object, latched until shields recover.
            p.shield_warned = true;
            let pos = Vec2::new(p.screen.x + self.cam_x - WORLD_TO_SCREEN_X, self.scroll + SCREEN_H - p.screen.y);
            self.request_spawn(SpawnReq {
                unit: pdef.active_shield_warning_object,
                pos,
                heading: None,
                owner: usize::MAX,
                spawner_player: slot as i32,
                stationary: true,
                state: None,
            });
        }
        // Death-spawn object (m_field0xba, spawned by G_Player::Destroy).
        if let Some((unit, pos)) = death_spawn {
            if unit != NONE_ID {
                self.request_spawn(SpawnReq {
                    unit,
                    pos,
                    heading: None,
                    owner: usize::MAX,
                    spawner_player: slot as i32,
                    stationary: true,
                    state: None,
                });
            }
        }
        // Burst the purse into coin pickups (50/10/5/1 denominations).
        if let Some((mut money, pos)) = money_burst {
            const COINS: [(i64, ResId); 4] = [
                (50, id("clac")),
                (10, id("slac")),
                (5, id("gsac")),
                (1, id("ssac")),
            ];
            // Slight scatter so the pile doesn't stack pixel-perfect.
            let mut n = 0i64;
            while money > 0 {
                let (v, u) = COINS.iter().find(|(v, _)| *v <= money).copied().unwrap();
                money -= v;
                n += 1;
                let off = Vec2::new(
                    ((n * 37) % 24) as f32 - 12.0,
                    ((n * 53) % 24) as f32 - 12.0,
                );
                self.request_spawn(SpawnReq {
                    unit: u,
                    pos: pos + off,
                    heading: None,
                    owner: usize::MAX,
                    spawner_player: slot as i32,
                    stationary: false,
                    state: None,
                });
            }
        }
    }

    fn collide(&mut self, assets: &GameAssets) {
        // Pass A: entities vs players (enemy projectiles / ramming bodies).
        // Pass B: player-owned projectiles vs hittable entities.
        let mut hits: Vec<(usize, f64, i32)> = Vec::new(); // (target idx, dmg, owner)
        let mut shots: Vec<usize> = Vec::new();            // projectile idx that hit
        let mut player_hits: Vec<(usize, usize, f64)> = Vec::new();
        let mut pickups: Vec<(usize, usize)> = Vec::new();  // (entity idx, player slot)

        let ppos: Vec<Vec2> = self.players.iter()
            .map(|p| p.as_ref().map(|p| p.world_pos(self)).unwrap_or(Vec2::splat(-9999.0)))
            .collect();

        for (i, e) in self.entities.iter().enumerate() {
            if e.dead || e.activate_in > 0 {
                continue;
            }
            let Some(def) = assets.units.get(&e.unit) else { continue };
            // Fully faded entities don't interact unless hittableWhenInvisible.
            if e.vis <= 0.0 && !def.hittable_when_invisible {
                continue;
            }
            // G_EG_Process player-contact gate: stateCollides &&
            // stateCollidesWithPlayers && !harmlessToPlayers.
            let st = def.states.get(e.state);
            let contacts_players = st
                .map(|s| s.collides && s.collides_with_players)
                .unwrap_or(false);
            if !contacts_players || def.harmless_to_players {
                continue;
            }
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
                    && !def.terrain_effect
                    && def.damage > 0.0
                {
                    player_hits.push((i, slot, def.damage));
                }
            }
        }

        for i in 0..self.entities.len() {
            if self.entities[i].dead || self.entities[i].activate_in > 0 {
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
                if i == j || self.entities[j].dead || self.entities[j].activate_in > 0 {
                    continue;
                }
                if self.entities[j].spawner_player == espawner {
                    continue; // same side
                }
                let Some(tdef) = assets.units.get(&self.entities[j].unit) else { continue };
                if self.entities[j].vis <= 0.0 && !tdef.hittable_when_invisible {
                    continue;
                }
                if !tdef.can_be_hit_by_player_projectile {
                    continue;
                }
                // Entity-vs-entity collision requires stateCollides (0x343).
                let tst = self.entities[j].state;
                if !tdef
                    .states
                    .get(tst)
                    .map(|s| s.collides)
                    .unwrap_or(false)
                {
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
        for (eidx, slot, dmg) in player_hits {
            self.damage_player(assets, slot, dmg);
            // pass_hits_to_owner (state 0x327): the owner entity soaks
            // pf(0xa1) damage credited to the contacting player.
            let pass = self.entities.get(eidx).and_then(|e| {
                assets
                    .units
                    .get(&e.unit)
                    .and_then(|d| d.states.get(e.state))
                    .map(|st| (st.pass_hits_to_owner, e.owner))
            });
            if let Some((true, owner_uid)) = pass {
                if owner_uid != u32::MAX {
                    if let Some(oidx) = self
                        .entities
                        .iter()
                        .position(|o| o.unique_num == owner_uid && !o.dead)
                    {
                        self.damage_entity(assets, oidx, pf(assets, pf::PASS_HITS_TO_OWNER) as f64, slot as i32);
                    }
                }
            }
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

/// `U_Math_GetVectorFromAngleAndSpeed`: heading degrees → velocity vector
/// (x = sin, y = cos, matching the original's convention).
fn vec_from_angle_speed(angle: f32, speed: f32) -> Vec2 {
    let r = angle.to_radians();
    Vec2::new(r.sin() * speed, r.cos() * speed)
}

/// `U_Math_InvertAngle`: reflect the heading (the original's spawn-heading
/// → velocity-angle transform).
fn invert_angle(a: f32) -> f32 {
    if a <= 180.0 {
        (a - 180.0).abs()
    } else {
        540.0 - a
    }
}

// ---------------------------------------------------------------------------
// Weapons / pickups
// ---------------------------------------------------------------------------

/// `G_WepDef_MasterList_GetNextRefPtrByType`: weapons of `wtype` available
/// at `level`, in master-list order.
pub fn weapon_chain(assets: &GameAssets, wtype: ResId, level: i64) -> Vec<ResId> {
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
            // Glow_Start(0x7fff, 6, false) — brief ship flash on collect.
            p.glow_ticks = 6;
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
    /// Select/cycle air weapon (original input[6], inC).
    pub cycle: bool,
}

impl PlayerInput {
    pub fn for_slot(&self, slot: usize) -> SlotInput {
        if slot == 0 { self.p1 } else { self.p2 }
    }
}
