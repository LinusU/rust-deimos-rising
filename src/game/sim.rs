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
/// Left edge of the 416-px playfield on screen: the original copies its
/// game buffer to the window at `LeftBorderWidth` (perm 0x3b = 32), see
/// `U_Display` in W_Display.cpp.
pub const PLAY_LEFT: f32 = 32.0;
/// Right edge of the playfield on screen (PLAY_LEFT + VisibleGameWidth 416).
pub const PLAY_RIGHT: f32 = PLAY_LEFT + 416.0;
/// Horizontal centre of the playfield on screen.
pub const PLAY_CENTER_X: f32 = PLAY_LEFT + 208.0;
/// World→screen x offset. Entity x is the original's game-area x (`m_x`,
/// 0..416 visible at rest): `G_GameObject` draws at `m_x - sideScroll` in
/// the game buffer, which sits `PLAY_LEFT` px into the window. The terrain
/// is blitted from image column `sideScroll + 32`, so `m_x` lies over
/// column `m_x + 32`; ground level objects are stored as image columns and
/// get `x -= 32` on load (sub_41a5b0).
pub const WORLD_TO_SCREEN_X: f32 = PLAY_LEFT;
/// Player top game-area limit (perm float 0xb7 = Player_TopGameAreaLimit).
pub const PLAYER_TOP_LIMIT: f32 = 13.0;
/// Side-scroll clamp (G_Bgnd_AdjustSideScroll: -0x20..=0x1f).
pub const SIDE_SCROLL_MIN: f32 = -32.0;
pub const SIDE_SCROLL_MAX: f32 = 31.0;
/// Max live entities (G_EG_RequestSpawn caps at 0x3e8).
pub const MAX_ENTITIES: usize = 1000;
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
    #[track_caller]
    pub fn rand(&mut self) -> u32 {
        if rand_log_enabled() {
            let l = std::panic::Location::caller();
            eprintln!("R {}:{} {:08x}", l.file().rsplit('/').next().unwrap_or(""), l.line(),
                self.state.wrapping_mul(1103515245).wrapping_add(12345));
        }
        self.state = self
            .state
            .wrapping_mul(1103515245)
            .wrapping_add(12345);
        (self.state >> 16) & 0x7fff
    }
}

/// `DEIMOS_RANDLOG=1` prints every `rand()` draw with its call site and
/// the new generator state, to line up against the original's stream.
pub fn rand_log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("DEIMOS_RANDLOG").is_some())
}

/// A retail film being replayed (G_Film playback): each player's pad
/// bytes, read one per Active tick by that player's own cursor.
#[derive(Clone)]
pub struct FilmPlayback {
    pub pads: [Vec<u8>; 2],
    pub total: [usize; 2],
    pub cursor: [usize; 2],
}

impl FilmPlayback {
    pub fn new(pads: [Vec<u8>; 2], total: [usize; 2]) -> Self {
        Self { pads, total, cursor: [0, 0] }
    }

    /// G_Film::GetInputs: decode the next pad byte (bit0 left, bit1 right,
    /// bit2 up, bit3 down, bit4 ground weapon, bit5 air weapon, bit6
    /// cycle). Past the recorded frames the inputs stay clear.
    pub fn next(&mut self, slot: usize) -> SlotInput {
        let c = self.cursor[slot];
        if c > self.total[slot] {
            return SlotInput::default();
        }
        self.cursor[slot] += 1;
        let b = self.pads[slot].get(c).copied().unwrap_or(0);
        SlotInput {
            left: b & 0x01 != 0,
            right: b & 0x02 != 0,
            up: b & 0x04 != 0,
            down: b & 0x08 != 0,
            bomb: b & 0x10 != 0,
            fire: b & 0x20 != 0,
            cycle: b & 0x40 != 0,
        }
    }

    /// G_Film::IsFinished: player 1's cursor has run past its frames.
    pub fn is_finished(&self) -> bool {
        self.cursor[0] > self.total[0]
    }
}

/// `U_Utils_RandomInt(min, max)` = `min + rand() % (max - min + 1)`.
/// The original early-returns `min` with NO rand() draw when min == max
/// (0x40f7ee jne skips the rand call) — matching that is essential for
/// stream parity since many state fields share equal min/max bounds.
#[track_caller]
fn rand_int(rng: &mut Crand, min: i64, max: i64) -> i64 {
    if min == max {
        return min;
    }
    min + (rng.rand() % (max - min + 1) as u32) as i64
}

/// `U_Utils_RandomFloat(low, high)` = `rand() * (high - low) / 32767 + min`.
/// Equal bounds early-return with no draw; reversed bounds roll downward
/// from the smaller value, matching the original's signed-range math.
#[track_caller]
fn rand_float(rng: &mut Crand, low: f64, high: f64) -> f64 {
    if low == high {
        return low;
    }
    let min = if low > high { high } else { low };
    rng.rand() as f64 * (high - low) / 32767.0 + min
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
    /// `G_Entity::Destroy` ran (original m_field_0xd2): gates the reap
    /// sweep's deletionSpawn path, kill-count, and coin release.
    pub destroy_ran: bool,
    /// Player that killed this entity (original m_field_0xce), -1 = none.
    pub killer: i32,
    /// Killed by direct player-ship body contact (original m_field_0xbe):
    /// suppresses coin release in sub_01ae10. Set by the ram path.
    pub ram_kill: bool,
    /// Reap sweep (sub_01b2d0) has processed this entity; `retain` only
    /// removes dead entities once reaped, mirroring the original's
    /// DeleteLink timing.
    pub reaped: bool,
    /// Entity is homing/rotating toward a player target (rule condition
    /// "Is Tracking Player" — the original's `m_field_0xb5`).
    pub tracking: bool,
    /// rotate_to_target sweep state: current direction index being
    /// animated toward the target (-1 = derive from heading), and the
    /// game-time of the last step (Priv_RotateToTargetLoc port).
    pub rot_dir: i64,
    pub rot_last: u32,
    /// m_field_0xb8: ticks of rotation pause armed by spawn-set volleys
    /// (pauseRotationWhileSpawning / timeToPauseRotation).
    pub rot_pause: u32,
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
    /// State-sound bookkeeping: m_fielde0 counts process passes since
    /// the state began, m_fielddc is the game time of the last play.
    pub sound_play_count: u32,
    pub sound_last_play: u32,
    /// Per-state entry counts (entity +0x144 array) — drives
    /// #stateOnCounter chains inside ChangeState.
    pub state_entry_count: Vec<u32>,
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

/// `G_Entity::SpawnControl` runtime state per spawn set — mirrors the
/// original's zeroed 0x16-byte `Priv_SpawnInfoView`: delay until the next
/// volley arm, game_time of the last arm, shots left this volley, and the
/// tick period between shots.
#[derive(Clone, Copy, Default)]
pub struct SpawnSetRt {
    pub delay: u32,
    pub last_arm: u32,
    pub remaining: u32,
    pub period: u32,
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
    /// Weapon selected while the air power-up machine was busy
    /// (G_WeaponHandler m_airCurrentWeapon); applied when the machine
    /// returns to idle after the release drains.
    pub air_weapon_queued: ResId,
    /// Current ground weapon def id.
    pub ground_weapon: ResId,
    /// Ticks until next shot allowed.
    pub fire_timer: u32,
    /// Ticks of invulnerability remaining.
    pub invuln: u32,
    /// Game time the current phase began (G_Player m_stateData).
    pub state_data: u32,
    /// Ground-weapon auto-burst shots remaining (G_WeaponHandler
    /// m_field_0x87). Armed by a bomb press after the recharge interval,
    /// drained one shot per delayBetweenLoadLaunches ticks.
    pub bomb_burst: i64,
    /// Game time of last ground-weapon activity (m_field_0x7b/0x7f — the
    /// original keeps two fields that are always written together).
    pub bomb_last: u32,
    /// Previous-tick bomb input for edge detection (pad1a).
    pub prev_bomb: bool,
    /// Previous-tick air-fire input for the autoRepeat semi-auto gate
    /// (m_field_0xa).
    pub prev_fire: bool,
    /// Previous-tick cycle input for the edge-triggered weapon switch
    /// (m_field_0xb).
    pub prev_cycle: bool,
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
    pub stat_kills_player: u64,
    pub stat_shots: u64,
    /// Pickup-type entities spawned (diagnostic).
    pub stat_pickups: u64,
    /// Coin-type entities spawned (diagnostic).
    pub stat_coins: u64,
    /// Raw def.score points awarded on player kills (diagnostic).
    pub stat_score_points: i64,
    /// Total entities created via spawn_one (diagnostic).
    pub stat_spawns: u64,
    /// Global game tick counter.
    pub game_time: u32,
    /// 1-based level number (weapon level-availability filtering).
    pub level_number: i64,
    pub players: [Option<Player>; 2],
    pub entities: Vec<Entity>,
    /// Indices into `level.objects` not yet spawned (sorted by world y).
    pending_objects: Vec<usize>,
    /// Level objects inside the initial update rect (y <= screen height),
    /// spawned once at level start in DESCENDING y order — G_Bgnd_
    /// DoInitialMapSpawns iterates rows from g_bgndUpdateRectBottom down to
    /// the scroll offset before the first frame runs.
    initial_objects: Vec<usize>,
    pub phase: Phase,
    /// Fixed-timestep accumulator (seconds).
    pub acc: f32,
    next_unique: u32,
    next_group_id: u32,
    /// Per-spawn-group reap bookkeeping — the original's group object's
    /// fields: [0x9a] expected children, [0x9e] remaining un-reaped,
    /// [0xa2] Destroy-killed children already reaped (vMatch counter).
    /// Keyed by `Entity::group_id`; entries are never removed (the
    /// original deletes the group object at remaining<=0, but keeping
    /// the counters is harmless and avoids dangling-state edge cases).
    groups: std::collections::HashMap<u32, GroupBook>,
    limit_message_shown: bool,
    pub rng: Crand,
    /// Retail film being replayed; overrides live input while set.
    pub film: Option<FilmPlayback>,
    /// Level-start notice banner deferred until the initial level objects
    /// have been queued, matching sub_41fc80's order (G_Bgnd_
    /// DoInitialMapSpawns runs before the Notice_Level_XX RequestSpawn).
    /// Tuple: (unit id, world pos, screen-lock pos).
    pub pending_notice: Option<(ResId, Vec2, Vec2)>,
    /// Deferred one-shot actions (e.g. spawn at pos next tick).
    pub events: Vec<SimEvent>,
    /// Level media mask (im16 16-bit TGA), loaded at level start.
    /// `G_Bgnd_MediaMask_GetSurfaceAtLoc` reads pixel 0x1f as surface 1.
    pub media_mask: Option<crate::assets::MediaMask>,
    /// `g_bgndTileSize` = background width / mask width (5 for retail).
    pub media_tile_size: i64,
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
    /// Player world positions captured before this tick's movement —
    /// the original's playerPrevLocs used by the contact circle test.
    player_pos_prev: [Vec2; 2],
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

/// Spawn-group reap counters (original group object +0x9a/+0x9e/+0xa2).
#[derive(Clone, Copy, Default)]
struct GroupBook {
    /// Children the spawn request was expected to create.
    expected: u32,
    /// Children not yet through sub_01ae10 (decremented every call,
    /// destroyed or deleted alike).
    remaining: u32,
    /// Children reaped with inFlag1 (Destroy ran) — vMatch fires when
    /// this reaches `expected`.
    killed: u32,
}

/// Internal spawn request.
#[derive(Clone)]
pub struct SpawnReq {
    pub unit: ResId,
    pub pos: Vec2,
    pub heading: Option<f32>,
    /// SpawnRequest f13 != 0 path: the request carries its own heading
    /// (weapon spawn-set angle) so arg4=1 and the tolerance roll always
    /// applies. Level objects leave this false (f13==0 → arg4 =
    /// initialHeadingSetInEditor, arg5 = f16 headingDegrees).
    pub heading_forced: bool,
    pub owner: usize,
    pub spawner_player: i32,
    pub stationary: bool,
    /// Force a specific state index (rare).
    pub state: Option<usize>,
}

impl Sim {
    pub fn new(level: LevelDef, level_id: ResId, seed: u64) -> Sim {
        // Required-group spawn order (sub_41a5b0 + sub_417900): the map
        // spawn frontier sweeps the level's file-y from the map bottom
        // (numRows) toward the top, so objects spawn in DESCENDING file-y
        // order; the stable sort keeps .leve file order for same-row ties,
        // matching the required-groups list scan.
        let level_h = (level.background[3] - level.background[1]).max(1) as f32;
        let mut pending: Vec<usize> = (0..level.objects.len()).collect();
        pending.sort_by(|&a, &b| level.objects[b].y.cmp(&level.objects[a].y));
        // G_Bgnd_DoInitialMapSpawns covers rows [numRows-544, numRows]
        // before frame 0; everything below is scroll-gated.
        let split = pending
            .iter()
            .position(|&i| (level.objects[i].y as f32) < level_h - 544.0)
            .unwrap_or(pending.len());
        let initial: Vec<usize> = pending[..split].to_vec();
        let rest = pending[split..].to_vec();
        let mut sim = Sim {
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
            stat_kills_player: 0,
            stat_shots: 0,
            stat_pickups: 0,
            stat_coins: 0,
            stat_score_points: 0,
            stat_spawns: 0,
            game_time: 0,
            level_number: 1,
            players: [None, None],
            entities: Vec::new(),
            pending_objects: rest,
            initial_objects: initial,
            phase: Phase::Intro,
            acc: 0.0,
            next_unique: 1,
            next_group_id: 1,
            groups: std::collections::HashMap::new(),
            limit_message_shown: false,
            rng: Crand::new(seed as u32),
            film: None,
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
            player_pos_prev: [Vec2::splat(-9999.0); 2],
            pending_notice: None,
            media_mask: None,
            media_tile_size: 0,
        };
        sim
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
        // G_Player::ResetAtLevelStart rolls U_Utils_RandomInt(400, 2000)
        // once per in-game player for the license field — consumed between
        // srand() and the first spawn, in slot order. gameType==1 keeps
        // only player 1 in game, so 1P films/levels draw exactly once.
        rand_int(&mut self.rng, 400, 2000);
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
            air_weapon_queued: NONE_ID,
            ground_weapon: id("plbo"), // Plasma Bomb
            fire_timer: 0,
            invuln: def.entry_invulnerability_time as u32,
            state_data: 0,
            bomb_burst: 0,
            bomb_last: 0,
            prev_bomb: false,
            prev_fire: false,
            prev_cycle: false,
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
    }

    /// `sub_41b650`: spawn-count roll + per-member appearsPercent cull.
    /// Runs at G_EG_RequestSpawn time — before any gates — so the rand
    /// draws happen in request order ahead of the spawn_one chains.
    fn roll_spawn_count(&mut self, assets: &GameAssets, unit: ResId) -> i64 {
        let Some(def) = assets.units.get(&unit) else {
            return 0;
        };
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
        count
    }

    /// Queue a unit spawn — G_EG_RequestSpawn is synchronous: count/cull
    /// rolls, gates, and the spawn_one chain all happen inline, in
    /// request order.
    pub fn request_spawn(&mut self, assets: &GameAssets, req: SpawnReq) {
        if rand_log_enabled() {
            eprintln!("S {} at=({:.2},{:.2})", rid_str(req.unit), req.pos.x, req.pos.y);
        }
        let count = self.roll_spawn_count(assets, req.unit);
        if std::env::var("DEIMOS_COINLOG").is_ok() && req.unit == id("cass") {
            eprintln!("t={} reqspawn cass count={} pos={:?}", self.game_time, count, req.pos);
        }
        if count <= 0 {
            return;
        }
        self.spawn_group(assets, &req, count);
    }

    /// Spawn pinned to a fixed screen position (notice banners).
    pub fn request_spawn_screen(&mut self, assets: &GameAssets, req: SpawnReq, screen: Vec2) {
        if rand_log_enabled() {
            eprintln!("S {} at=({:.2},{:.2})", rid_str(req.unit), req.pos.x, req.pos.y);
        }
        let count = self.roll_spawn_count(assets, req.unit);
        if count <= 0 {
            return;
        }
        let before = self.entities.len();
        self.spawn_group(assets, &req, count);
        for e in &mut self.entities[before..] {
            e.screen_lock = Some(screen);
        }
    }

    /// `G_EG_RequestSpawn` group path: gates then spawns `count` members
    /// (count/appears rolls already consumed at request time) with
    /// cumulative groupDelay activation delay.
    fn spawn_group(&mut self, assets: &GameAssets, req: &SpawnReq, count: i64) {
        let coinlog = std::env::var("DEIMOS_COINLOG").is_ok() && req.unit == id("cass");
        let Some(def) = assets.units.get(&req.unit).cloned() else {
            if coinlog { eprintln!("t={} spawn_group {}: no def", self.game_time, req.unit); }
            return;
        };
        if def.states.is_empty() {
            if coinlog { eprintln!("t={} spawn_group {}: no states", self.game_time, req.unit); }
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
        // deleteExistingEntitiesOfThisTypeOwnedByPlayer gate.
        if def.delete_existing_owned_by_player && req.spawner_player >= 0 {
            for e in self.entities.iter_mut() {
                if !e.dead && e.unit == req.unit && e.spawner_player == req.spawner_player {
                    e.dead = true;
                }
            }
        }
        if count <= 0 {
            return;
        }
        let gid = self.next_group_id;
        self.next_group_id = self.next_group_id.wrapping_add(1).max(1);
        // Group object bookkeeping (RequestSpawn tail): expected and
        // remaining both start at the rolled count.
        self.groups.insert(
            gid,
            GroupBook {
                expected: count.max(0) as u32,
                remaining: count.max(0) as u32,
                killed: 0,
            },
        );
        // The group-delay roll happens inside each entity's creation tail
        // (sub_41a890), after its ChangeState rolls — accumulate here so
        // member k activates at the sum of rolls 1..=k.
        let mut acc = 0i64;
        for _ in 0..count {
            let r = self.spawn_one(assets, req, gid, &mut acc);
            if coinlog {
                eprintln!("t={} spawn_one {} -> {:?}", self.game_time, req.unit, r);
            }
        }
    }

    fn spawn_one(&mut self, assets: &GameAssets, req: &SpawnReq, group_id: u32, group_acc: &mut i64) -> Option<usize> {
        let def = assets.units.get(&req.unit)?;
        if def.states.is_empty() {
            return None;
        }
        let state_idx = req.state.unwrap_or(0).min(def.states.len() - 1);
        let state = &def.states[state_idx];
        // sub_41a990 heading block — runs BEFORE sub_41c540 scatter.
        // Facing (+0x130): a forced spawn uses the request heading plus a
        // tolerance roll when initialHeadingTolerance != 0; everything
        // else faces the unit def's initialHeading with no roll.
        // G_EG_RequestSpawn (0x417e59) forces the spawn when the request
        // sets a heading, or when the unit has initialHeadingSetInEditor,
        // in which case the heading is the one the level editor stored
        // for the object.
        let forced = req.heading_forced || def.initial_heading_set_in_editor;
        let tol = def.initial_heading_tolerance;
        // Original half-range idiom (sbb/sar): (tol>=0 ? tol : tol+1) >> 1.
        let half_tol = (if tol >= 0 { tol } else { tol + 1 }) >> 1;
        let mut facing = if forced {
            let mut h = req.heading.unwrap_or(0.0);
            if tol != 0 {
                h += rand_int(&mut self.rng, -half_tol, half_tol) as f32;
                if h < 0.0 {
                    h += 360.0;
                } else if h > 359.0 {
                    h -= 360.0;
                }
                if h < 0.0 || h > 359.0 {
                    h = 0.0;
                }
            }
            h
        } else {
            def.initial_heading as f32
        };
        // sub_41c540 position scatter. When BOTH offset axes carry a range
        // the original takes the polar branch: a RandomInt(0,359) angle
        // always draws, then randomiseInitialLoc chooses between a
        // RandomFloat(0,|xMax|) radius on a circle (second draw) and a
        // fixed |xMax|,|yMax| ellipse (no second draw). Single-axis ranges
        // roll only that axis; equal bounds are a fixed offset, no draw.
        let xr = def.x_offset_min != def.x_offset_max;
        let yr = def.y_offset_min != def.y_offset_max;
        let pos = if xr && yr {
            let angle = rand_int(&mut self.rng, 0, 0x167) as f32;
            let ar = angle.to_radians();
            // GetVectorFromAngle yields screen-space (y-down) offsets;
            // world y is up-positive, so the y component negates.
            let (sx, sy) = (ar.sin(), -ar.cos());
            if def.randomise_initial_loc {
                let r = rand_float(&mut self.rng, 0.0, def.x_offset_max.abs()) as f32;
                req.pos + Vec2::new(sx * r, sy * r)
            } else {
                req.pos
                    + Vec2::new(
                        sx * def.x_offset_max.abs() as f32,
                        sy * def.y_offset_max.abs() as f32,
                    )
            }
        } else if xr || yr {
            let jx = if xr {
                let (lo, hi) = if def.x_offset_min <= def.x_offset_max {
                    (def.x_offset_min, def.x_offset_max)
                } else {
                    (def.x_offset_max, def.x_offset_min)
                };
                rand_int(&mut self.rng, lo.round() as i64, hi.round() as i64) as f32
            } else {
                def.x_offset_min as f32
            };
            let jy = if yr {
                let (lo, hi) = if def.y_offset_min <= def.y_offset_max {
                    (def.y_offset_min, def.y_offset_max)
                } else {
                    (def.y_offset_max, def.y_offset_min)
                };
                rand_int(&mut self.rng, lo.round() as i64, hi.round() as i64) as f32
            } else {
                def.y_offset_min as f32
            };
            // y offsets are screen-space (down-positive) in the original;
            // negate for world coords like the spawn-set path does.
            req.pos + Vec2::new(jx, -jy)
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
        // sub_01c840 velocity init. Stationary entities early-return
        // before the speed roll, leaving every velocity field zero.
        // Otherwise speed = RandomFloat(initialSpeedMin, initialSpeedMax)
        // (always draws), then the heading branch picks a direction:
        //   forced || initialHeadingSetInEditor → raw request heading
        //   initiallyHuntsClosestPlayer        → unit vector toward the
        //     closest active player (screen-centre reference), no roll
        //   otherwise                          → initialHeading plus a
        //     tolerance roll when != 0; the rolled value overwrites the
        //     facing stored at +0x130.
        // doBurst/doImplode/useOwnerHeading have no retail units.
        // Velocity uses U_Math_InvertAngle(heading) like the original.
        let vel = if req.stationary {
            Vec2::ZERO
        } else if req.heading_forced || def.initial_heading_set_in_editor {
            // sub_01c840's forced branch reuses arg4 = inArg7, the facing
            // stored at entity+0x130: the rolled request heading for forced
            // spawns, initialHeading for editor-heading units.
            let a = invert_angle(facing).to_radians();
            // GetVectorFromAngleAndSpeed is screen-space (y-down); negate y.
            Vec2::new(a.sin() * speed, -a.cos() * speed)
        } else if def.initially_hunts_closest_player {
            let reference = Vec2::new(pf(assets, pf::VISIBLE_GAME_WIDTH) as f32 / 2.0, 0.0);
            let mut best: Option<Vec2> = None;
            let mut best_d = f32::MAX;
            for p in self.players.iter().flatten() {
                if matches!(p.phase, PlayerPhase::Dead | PlayerPhase::Dying) {
                    continue;
                }
                let d = (p.screen - reference).length_squared();
                if d < best_d {
                    best_d = d;
                    best = Some(Vec2::new(
                        p.screen.x + self.cam_x - WORLD_TO_SCREEN_X,
                        self.scroll + SCREEN_H - p.screen.y,
                    ));
                }
            }
            let target = best.unwrap_or(Vec2::new(reference.x, -100.0));
            let d = target - pos;
            let l = d.length();
            if l > 0.0001 {
                d / l * speed
            } else {
                Vec2::ZERO
            }
        } else {
            // sub_01c840 heading branch: the heading source is
            // initialHeading unless useOwnerHeading selects the owner's
            // sprite angle (bl=0 suppresses the roll). The tolerance roll
            // therefore runs for every unit except useOwnerHeading+owner —
            // and no retail unit sets useOwnerHeading, so all tolerance!=0
            // units roll here.
            let mut h = def.initial_heading as f32;
            let sprite_angle = def.use_owner_heading && req.owner != usize::MAX;
            if !sprite_angle && tol != 0 {
                h += rand_int(&mut self.rng, -half_tol, half_tol) as f32;
                if h < 0.0 {
                    h += 360.0;
                } else if h > 359.0 {
                    h -= 360.0;
                }
                if h < 0.0 || h > 359.0 {
                    h = 0.0;
                }
            }
            facing = h;
            let a = invert_angle(h).to_radians();
            // GetVectorFromAngleAndSpeed is screen-space (y-down); negate y.
            Vec2::new(a.sin() * speed, -a.cos() * speed)
        };
        // sub_41a990 shield init: entity+0x12c = shields_BaseAmount, and
        // when LevelIncrement is non-zero it becomes
        // base + (levelNum-1)*inc capped at MaxAmount. Retail increments
        // are all >= 0, so the original's sign check must admit positives.
        let shields = {
            let mut s = def.shields_base;
            if def.shields_level_increment != 0.0 {
                s = (self.level_number as f64 - 1.0) * def.shields_level_increment + def.shields_base;
                if def.shields_max < s {
                    s = def.shields_max;
                }
            }
            s
        };
        let idx = self.entities.len();
        // req.owner is an entity index; store the stable unique_num.
        let owner_uid = if req.owner != usize::MAX && req.owner < self.entities.len() {
            self.entities[req.owner].unique_num
        } else {
            u32::MAX
        };
        self.entities.push(Entity {
            unit: req.unit,
            pos,
            vel,
            heading: facing,
            state: state_idx,
            state_time: 0,
            state_timer_target: 0,
            age: 0,
            // Placeholder — ChangeState(true) below rolls the initial
            // frame (or derives it from heading for editor-heading units).
            frame: state.sprite_frame_min,
            frame_timer: 0,
            dir_index: -1,
            shields,
            owner: owner_uid,
            spawner_player: req.spawner_player,
            spawn_timers: Vec::new(),
            hit_cooldown: 0,
            glow: GlowRt::default(),
            hit_change_time: 0,
            hit_fx_time: 0,
            hit_count: 0,
            dead: false,
            destroyed: false,
            destroy_ran: false,
            killer: -1,
            ram_kill: false,
            reaped: false,
            tracking: false,
            rot_dir: -1,
            rot_last: 0,
            rot_pause: 0,
            anim_stopped: false,
            screen_lock: None,
            activate_in: 0,
            group_id,
            owner_off: Vec2::ZERO,
            owner_prev: Vec2::ZERO,
            orbit_angle: 0.0,
            orbit_radius: f32::NAN,
            tvel: vel,
            vdelta: Vec2::ZERO,
            fleeing: state.flee_id != NONE_ID,
            stationary: req.stationary,
            flee_target: None,
            // Entities start at the unit's own visibility; effect
            // spawners such as `bsdt` are defined at 0% and never drawn.
            vis: def.initial_visibility_percent as f32,
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
            state_entry_count: vec![0; def.states.len()],
            blur_last: 0,
            entry_pos: req.pos,
            debris_hit: false,
            unique_num: self.next_unique,
            carried_weapon: carried_weapon(assets, def, self.level_number),
        });
        self.next_unique = self.next_unique.wrapping_add(1);
        self.stat_spawns += 1;
        if def.pickup_type != NONE_ID {
            self.stat_pickups += 1;
            if std::env::var("DEIMOS_COINLOG").is_ok() && req.unit == id("cass") {
                eprintln!(
                    "t={} cass spawned ptype={:08x} coin={} stat_p={} stat_c={}",
                    self.game_time, def.pickup_type, id("coin"),
                    self.stat_pickups, self.stat_coins
                );
            }
            if def.pickup_type == id("coin") {
                self.stat_coins += 1;
            }
            if std::env::var("DEIMOS_PICKLOG").is_ok() && def.pickup_type != id("coin") {
                eprintln!(
                    "t={} PICKUP spawned unit={:08x} ptype={:08x} pos=({:.0},{:.0})",
                    self.game_time, req.unit, def.pickup_type, pos.x, pos.y
                );
            }
        }
        // G_Game_GroundAccuracy_AddTarget on entity creation.
        if def.include_in_ground_accuracy {
            self.accuracy_targets += 1;
        }
        // sub_41a990 calls ChangeState(true, initialState) — every
        // state-entry roll (on_timer, frame, scale, flee, spawn-set init)
        // happens inside it, in the original's order.
        self.change_state(idx, state_idx, def, assets, true);
        // sub_41a890 tail: the group-delay roll accumulates per entity
        // AFTER its ChangeState rolls; cyclic scatter follows.
        let dmin = def.group_delay_min as i64;
        let dmax = def.group_delay_max.max(dmin);
        *group_acc += if dmin != dmax {
            rand_int(&mut self.rng, dmin, dmax)
        } else {
            dmin
        };
        self.entities[idx].activate_in = (*group_acc).max(0) as u32;
        self.scatter_cyclic_spawn(assets, idx);
        // Stationary destruct-obstacle units leave a debris zone from
        // spawn (sub_41a990 tail: m_field134 && destructCreateObstacle).
        if req.stationary && def.destruct_create_obstacle {
            self.add_debris(idx, assets);
        }
        // stateUseParentDirection: start on the frame matching the angle
        // the owner's sprite shows (GetFrameForAngle(GetAngleFromSpriteInfo)).
        if state.use_parent_direction {
            if let Some(owner_idx) = self
                .entities
                .iter()
                .position(|o| o.unique_num == self.entities[idx].owner && !o.dead)
            {
                let owner = &self.entities[owner_idx];
                let angle = assets
                    .units
                    .get(&owner.unit)
                    .and_then(|d| d.states.get(owner.state))
                    .map(|st| sprite_angle(owner, st))
                    .unwrap_or(owner.heading as i32);
                self.entities[idx].heading = angle as f32;
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
        self.fire_weapon_id(assets, slot, weapon_id, false);
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
        self.fire_weapon_id(assets, slot, weapon_id, true);
    }

    fn fire_weapon_id(&mut self, assets: &GameAssets, slot: usize, weapon_id: ResId, ground: bool) {
        let pos = match self.players[slot].as_ref() {
            Some(p) if p.phase == PlayerPhase::Active => p.world_pos(self),
            _ => return,
        };
        let wdef = match assets.weapons.get(&weapon_id) {
            Some(w) => w.clone(),
            None => return,
        };
        // Priv_Spawn_Ground stamps req.m_field24 — a velocity scale derived
        // from the crosshair icon position — on every ground-weapon spawn
        // request; Priv_Spawn_Air leaves it at the 1.0 default.
        let vel_scale = if ground {
            self.ground_ballistic_scale(assets, slot, &wdef)
        } else {
            1.0
        };
        let before = self.entities.len();
        let mut spawned = 0usize;
        for sp in &wdef.spawns {
            if sp.unit_id == NONE_ID {
                continue;
            }
            // req.f4/f8 = spawn-set offset + player loc added directly in
            // the original's screen-space y (down-positive); our world y is
            // up-positive altitude, so the file YLoc negates.
            let mut pos = pos;
            pos.x += sp.x_loc as f32;
            pos.y += (-sp.y_loc) as f32;
            let heading = if sp.set_heading { Some(sp.angle as f32) } else { None };
            self.request_spawn(assets, SpawnReq {
                unit: sp.unit_id,
                pos,
                heading,
                heading_forced: sp.set_heading,
                owner: usize::MAX,
                spawner_player: slot as i32,
                stationary: false,
                state: None,
            });
            spawned += 1;
        }
        self.stat_shots += spawned as u64;
        if vel_scale != 1.0 {
            // sub_01c840's tail: m_field24 scales entity+0x10/0x14 (m_prevX/Y
            // = integrated velocity) before the +0xf8 snapshot and the
            // tvel/vdelta init; ChangeState then rewrites tvel/vdelta from
            // state params, so only the momentum `vel` keeps the scale.
            for e in &mut self.entities[before..] {
                e.vel *= vel_scale;
            }
        }
    }

    /// Priv_Spawn_Ground's `req.m_field24`, recomputed per shot. The
    /// crosshair icon is drawn player-relative (G_Player.cpp: weaponLoc =
    /// ship + crosshairOffset, centered by iconH/2, clamped to
    /// g_shieldOffset = 0), so diffY = iconH/2 - crosshairYOffset is the
    /// constant ballistic_scale in normal play; it only goes dynamic when
    /// the clamp pins the icon at screen top (player high enough that
    /// weaponLoc.y - iconH/2 < 0), where diffY collapses to m_y.
    fn ground_ballistic_scale(
        &self,
        _assets: &GameAssets,
        slot: usize,
        wdef: &crate::data::WeaponDef,
    ) -> f32 {
        let off_y = wdef.crosshair_y_offset as f32;
        if off_y == 0.0 || wdef.ballistic_half_h == 0.0 {
            return wdef.ballistic_scale as f32;
        }
        let Some(p) = self.players[slot].as_ref() else {
            return 1.0;
        };
        // drawLoc.y = clamp(m_y + offY - iconH/2, g_shieldOffset=0, ..).
        let draw_loc_y = (p.screen.y + off_y - wdef.ballistic_half_h as f32).max(0.0);
        // diffY = m_y - temp.y; (int) truncates toward zero, then clamps >=0.
        let diff_y = p.screen.y - draw_loc_y;
        let rounded = (diff_y as i32).max(0);
        rounded as f32 / off_y.abs()
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
                    self.request_spawn(assets, SpawnReq {
                        unit: act_spawn,
                        pos,
                        heading: None,
                        heading_forced: false,
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
                            self.consume_sound_rand(pd.powerup_overload_sound.min_pitch, pd.powerup_overload_sound.max_pitch);
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
                                    self.consume_sound_rand(pd.powerup_overload_sound.min_pitch, pd.powerup_overload_sound.max_pitch);
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
                    self.request_spawn(assets, SpawnReq {
                        unit: rel_spawn,
                        pos,
                        heading: None,
                        heading_forced: false,
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
                        // Priv_AirPowerup_Process state-3→0 applies the
                        // weapon queued by ChangeWeapon while charging.
                        if !ground && p.air_weapon_queued != NONE_ID {
                            p.air_weapon = p.air_weapon_queued;
                            p.air_weapon_queued = NONE_ID;
                        }
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
            self.change_state(idx, si, &def, assets, false);
        }
    }

    /// Queue a level object for spawning (shared by the initial update-rect
    /// batch and the scroll-gated pending list).
    fn queue_level_object(&mut self, assets: &GameAssets, oi: usize) {
        let obj = &self.level.objects[oi];
        if obj.unit_id == NONE_ID {
            return;
        }
        // World position: the original entity y IS the file yLoc (screen
        // pos = y - scrollOffset); our pos.y measures height above the map
        // bottom, so pos.y = level_height - file_y. 'grnd'-class objects
        // get x -= 32.0 (sub_41a5b0: xLoc field -= dat_4e34b8 = 32.0f).
        let ground = assets
            .units
            .get(&obj.unit_id)
            .map(|d| d.is_ground_based)
            .unwrap_or(false);
        let pos = Vec2::new(
            obj.x as f32 - if ground { 32.0 } else { 0.0 },
            self.level_height() - obj.y as f32,
        );
        self.request_spawn(assets, SpawnReq {
            unit: obj.unit_id,
            pos,
            heading: Some(obj.heading_degrees as f32),
            heading_forced: false,
            owner: usize::MAX,
            spawner_player: -1,
            stationary: obj.stationary,
            state: None,
        });
    }

    /// Advance the simulation by one 60 Hz tick.
    pub fn step(&mut self, assets: &GameAssets, input: &PlayerInput) {
        self.step_tick(assets, input);
        // G_Game_Play bumps the game time after the tick, so the first
        // tick runs at time 0.
        self.game_time += 1;
    }

    fn step_tick(&mut self, assets: &GameAssets, input: &PlayerInput) {

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
                    if std::env::var("DEIMOS_ENDLOG").is_ok() {
                        eprintln!("ENDLOG P{} took_damage={} def_obj={:08x} money={}", slot, took_damage, def_obj, money);
                    }
                    if !took_damage {
                        if let Some(p) = self.players[slot].as_mut() {
                            p.damage_taken = true;
                            p.invuln = u32::MAX;
                        }
                        if def_obj != NONE_ID {
                            self.request_spawn(assets, SpawnReq {
                                unit: def_obj,
                                pos: wpos,
                                heading: None,
                                heading_forced: false,
                                owner: usize::MAX,
                                spawner_player: spawner,
                                stationary: true,
                                state: None,
                            });
                            let pts = self.level_number * pf(assets, 0xb8) as i64;
                            if std::env::var("DEIMOS_ENDLOG").is_ok() {
                                eprintln!("ENDLOG P{} defence bonus +{} (lvl={} pfb8={})", slot, pts, self.level_number, pf(assets, 0xb8));
                            }
                            self.add_score(slot, pts, false, assets);
                        } else if std::env::var("DEIMOS_ENDLOG").is_ok() {
                            eprintln!("ENDLOG P{} def_obj=NONE (no defence bonus)", slot);
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
                if std::env::var("DEIMOS_ENDLOG").is_ok() {
                    eprintln!("ENDLOG accuracy pct={:.1} tier={} total={} destroyed={}/{} lvl={}", pct, tier, total, self.accuracy_destroyed, self.accuracy_targets, self.level_number);
                }
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
                        self.request_spawn_screen(assets, SpawnReq {
                            unit: rid,
                            pos,
                            heading: None,
                            heading_forced: false,
                            owner: usize::MAX,
                            spawner_player: -1,
                            stationary: true,
                            state: None,
                        }, Vec2::new(PLAY_CENTER_X, 56.0));
                    }
                }
            }
        }

        // --- level object spawning ---
        // Initial update-rect objects spawn once, descending y (the
        // original's DoInitialMapSpawns row order) before any scrolling.
        for oi in std::mem::take(&mut self.initial_objects) {
            self.queue_level_object(assets, oi);
        }
        // Level-start notice banner: sub_41fc80 runs DoInitialMapSpawns,
        // then this RequestSpawn, before the first G_Bgnd_Process row
        // check — so it queues after the initial batch but before any
        // scroll-gated object.
        if let Some((unit, pos, screen)) = self.pending_notice.take() {
            self.request_spawn_screen(assets, SpawnReq {
                unit,
                pos,
                heading: None,
                heading_forced: false,
                owner: usize::MAX,
                spawner_player: -1,
                stationary: true,
                state: None,
            }, screen);
        }

        // Scroll-gated spawns (sub_410230 → sub_417900): each frame checks
        // row scrollOffset-0x40 = level_height-544-scroll; objects at that
        // file y spawn. In descending file-y order this is equivalent to
        // spawning every object whose file y has been reached by the
        // frontier.
        let frontier = self.level_height() - 544.0 - self.scroll;
        while let Some(&oi) = self.pending_objects.first() {
            if (self.level.objects[oi].y as f32) >= frontier {
                self.pending_objects.remove(0);
                self.queue_level_object(assets, oi);
            } else {
                break;
            }
        }

        // Contact tests run against where the ship WAS before this tick's
        // movement (retail's playerPrevLocs).
        for slot in 0..2 {
            self.player_pos_prev[slot] = self.players[slot]
                .as_ref()
                .map(|p| p.world_pos(self))
                .unwrap_or(Vec2::splat(-9999.0));
        }

        // --- players ---
        for slot in 0..2 {
            self.step_player(assets, slot, input);
        }

        // --- entities ---
        self.step_entities(assets);

        // --- collisions ---
        self.collide(assets);

        // --- reap sweep (sub_01b2d0 + sub_01ae10) ---
        // Every entity flagged dead (m_field_bf) is processed once, in
        // entity-list order: owner cascade, deletionSpawn media gate,
        // then group accounting/coin release. Entities killed during the
        // sweep at a later index are handled in the same pass; earlier
        // indices wait for the next tick's sweep — matching the original's
        // live child-list walk. Runs before `retain`.
        let mut ri = 0usize;
        while ri < self.entities.len() {
            if self.entities[ri].dead && !self.entities[ri].reaped {
                self.reap_entity(assets, ri);
            }
            ri += 1;
        }

        // --- cleanup ---
        // Only dead-and-reaped entities leave; dead-but-unreaped ones
        // (killed after the sweep point, or deferred by list order)
        // survive to the next tick's sweep like the original's links.
        self.entities.retain(|e| !e.dead || !e.reaped);

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
        let now = self.game_time;
        if slot == 0 && rand_log_enabled() {
            // Same spot as a breakpoint on G_Player::Process in the original.
            if let Some(p) = self.players[0].as_ref() {
                eprintln!("T {} ({:.4},{:.4}) score={}", now, p.screen.x - WORLD_TO_SCREEN_X + self.cam_x, p.screen.y, p.score);
            }
        }
        let def: PlayerDef = match self.players[slot].as_ref() {
            Some(p) => assets.players.get(&p.def_id).cloned().unwrap_or_default(),
            None => return,
        };
        let live_inp = input.for_slot(slot);
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
                // Priv_ProcessState state 2: appear once the game time
                // passes the entry delay.
                if now as i64 > def.entry_initial_delay + p.state_data as i64 {
                    p.phase = PlayerPhase::Active;
                    p.state_data = now;
                    // Entry effect ("plen") is requested when the player
                    // appears — after the initial object batch, like the
                    // original's first-frame Priv_Appear.
                    if def.entry_spawn != NONE_ID {
                        let w = Vec2::new(
                            p.screen.x + self.cam_x - WORLD_TO_SCREEN_X,
                            self.scroll + SCREEN_H - p.screen.y,
                        );
                        self.request_spawn(assets, SpawnReq {
                            unit: def.entry_spawn,
                            pos: w,
                            heading: None,
                            heading_forced: false,
                            owner: usize::MAX,
                            spawner_player: slot as i32,
                            stationary: false,
                            state: None,
                        });
                    }
                } else {
                    return;
                }
            }
            PlayerPhase::Dying => {
                // Priv_ProcessState state 3: finalDyingTime on the last
                // life, dyingTime otherwise. The life is consumed when the
                // death anim completes, not at the hit.
                let dur = if p.lives == 1 {
                    def.final_dying_time
                } else {
                    def.dying_time
                };
                if now as i64 > dur + p.state_data as i64 {
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
                        p.state_data = now;
                        p.air_power = PowerupRt::default();
                        p.gnd_power = PowerupRt::default();
                        p.power = 0.0;
                        p.gpower = 0.0;
                        p.prev_fire = false;
                        p.prev_bomb = false;
                        if def.entry_spawn != NONE_ID {
                            let w = Vec2::new(
                                p.screen.x + self.cam_x - WORLD_TO_SCREEN_X,
                                self.scroll + SCREEN_H - p.screen.y,
                            );
                            self.request_spawn(assets, SpawnReq {
                                unit: def.entry_spawn,
                                pos: w,
                                heading: None,
                                heading_forced: false,
                                owner: usize::MAX,
                                spawner_player: slot as i32,
                                stationary: false,
                                state: None,
                            });
                        }
                    } else {
                        p.phase = PlayerPhase::Dead;
                        p.state_data = now;
                        self.events.push(SimEvent::PlayerEvent(slot, "dead"));
                        return;
                    }
                } else {
                    return;
                }
            }
            PlayerPhase::Dead => return,
            PlayerPhase::Active => {}
        }
        // Priv_GetInputs runs after the state update, so a player that
        // appears this tick already reads input. A film frame is consumed
        // only on Active ticks.
        let inp = match self.film.as_mut() {
            Some(f) => f.next(slot),
            None => live_inp,
        };
        let p = self.players[slot].as_mut().unwrap();

        if p.invuln > 0 { p.invuln -= 1; }
        if p.fire_timer > 0 { p.fire_timer -= 1; }
        // Shields do not passively regenerate in the original — they
        // recover via "shie" pickups and respawn resets only. The power
        // meters are driven by the weapon power-up state machine.

        // Movement (G_Player::Process Result_0): each axis is independent —
        // a held direction adds ±delta clamped to ±max_speed, a released
        // axis decays toward 0 by delta. Diagonals therefore reach
        // max_speed on both axes at once.
        let max_speed = def.active_default_max_speed as f32;
        let step = def.active_velocity_delta as f32;
        if inp.up { p.vel.y -= step; }
        if inp.down { p.vel.y += step; }
        if inp.left { p.vel.x -= step; }
        if inp.right { p.vel.x += step; }
        if !inp.left && !inp.right {
            if p.vel.x < 0.0 { p.vel.x = (p.vel.x + step).min(0.0); }
            else if p.vel.x > 0.0 { p.vel.x = (p.vel.x - step).max(0.0); }
        }
        if !inp.up && !inp.down {
            if p.vel.y < 0.0 { p.vel.y = (p.vel.y + step).min(0.0); }
            else if p.vel.y > 0.0 { p.vel.y = (p.vel.y - step).max(0.0); }
        }
        p.vel.x = p.vel.x.clamp(-max_speed, max_speed);
        p.vel.y = p.vel.y.clamp(-max_speed, max_speed);
        // The original keeps the ship's x in game-area space (m_x) and
        // draws it at m_x - sideScroll, so a camera pan slides the ship
        // across the screen. Move in that space, then convert back.
        let mut gx = p.screen.x - WORLD_TO_SCREEN_X + self.cam_x + p.vel.x;
        p.screen.y += p.vel.y;

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

        // Clamp (G_Player::Process, 0x4349be): the sprite's edges stay
        // within game x [-32, 448], the full width of the terrain, with
        // its top at pf(0xb7) or lower, its bottom on screen.
        let (hw, hh) = {
            let (w, h) = player_half_extents(assets, p);
            (w.floor(), h.floor())
        };
        let (x_min, x_max) = (hw - 32.0, pf(assets, pf::VISIBLE_GAME_WIDTH) + 32.0 - hw);
        if gx < x_min {
            gx = x_min;
            p.vel.x = 0.0;
        } else if gx > x_max {
            gx = x_max;
            p.vel.x = 0.0;
        }
        p.screen.x = gx - self.cam_x + WORLD_TO_SCREEN_X;
        let (y_min, y_max) = (PLAYER_TOP_LIMIT + hh, SCREEN_H - hh);
        if p.screen.y < y_min {
            p.screen.y = y_min;
            p.vel.y = 0.0;
        } else if p.screen.y > y_max {
            p.screen.y = y_max;
            p.vel.y = 0.0;
        }

        // Ground weapon (G_WeaponHandler Process, m_field_0x87/0x7b/0x7f):
        // a fresh bomb press after the recharge interval arms an
        // auto-burst of min(pf(0x97)+level-1, pf(0x98)) shots; while the
        // burst counter is positive it fires one shot every
        // delayBetweenLoadLaunches ticks, even after release. Blocked
        // while the ground power-up machine is not idle (m_field_0x35).
        // The intervals come from m_airPermanentWeapon — the DEAG
        // (default pack/ground) weapon def, i.e. the ground weapon.
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
        // Priv_CheckSpawning_Air: normal fire only while the air
        // power-up machine is idle (m_field_0x11 == 0), and non
        // autoRepeat weapons need a fresh press each shot (m_field_0xa).
        let auto_repeat = assets
            .weapons
            .get(&p.air_weapon)
            .map(|w| w.auto_repeat)
            .unwrap_or(true);
        let fire_air = inp.fire
            && p.fire_timer == 0
            && p.air_power.state == 0
            && (auto_repeat || !p.prev_fire);
        p.prev_fire = inp.fire;
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
        // Select Air Weapon key: edge-triggered cycle to the next
        // available PEAA weapon (G_WeaponHandler inC → GetNextRefPtrByType
        // → ChangeWeapon). ChangeWeapon swaps immediately only while the
        // air power-up machine is idle (m_field_0x11 == 0); while charging
        // or releasing the pick is queued in m_airCurrentWeapon and
        // applied when the machine returns to state 0.
        if inp.cycle {
            if let Some(p) = self.players[slot].as_mut() {
                let edge = !p.prev_cycle;
                p.prev_cycle = inp.cycle;
                if edge {
                    if let Some(w) = next_weapon_after(assets, id("PEAA"), p.air_weapon, self.level_number) {
                        if w != p.air_weapon {
                            if p.air_power.state == 0 {
                                p.air_weapon = w;
                            } else {
                                p.air_weapon_queued = w;
                            }
                            p.weapon_popup = 60;
                            self.events.push(SimEvent::PlayerEvent(slot, "weapon_cycle"));
                        }
                    }
                }
            }
        } else if let Some(p) = self.players[slot].as_mut() {
            p.prev_cycle = false;
        }
    }

    fn two_player_active(&self) -> bool {
        self.players[1].is_some()
    }

    fn step_entities(&mut self, assets: &GameAssets) {
        self.entity_scroll_pause = false;
        // Retail's G_EG_Process walks a live child linked-list: an entity
        // spawned mid-pass is appended at the tail and reached later in the
        // SAME pass. Process in waves so same-tick spawns get their first
        // full update this tick — required for one-shot spawn-then-delete
        // states (rb## bonus pods) whose on_timer would otherwise fire
        // before their spawn-set volley runs.
        //
        // KNOWN DIVERGENCE (film/RNG parity): retail iterates
        // g_entityGroupList outer / m_childList inner — entities process
        // GROUP-BY-GROUP, not in flat spawn order. This Vec processes in
        // index order, so interleaved spawns across groups reorder rand()
        // draws and desync the stream from retail's. Mechanics are faithful;
        // only deterministic replay ordering differs.
        let mut start = 0usize;
        loop {
            let end = self.entities.len();
            if start >= end {
                break;
            }
            for i in start..end {
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
            let (def, mut state) = match assets.units.get(&unit_id) {
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

            // --- state repeat particles (G_EntityGroup stateParticles_*) ---
            // Runs before state transitions in the original (G_Particle_
            // NewGroup precedes the on_timer ChangeState), using the
            // pre-change state — matching that order keeps rand() aligned.
            // Original gate: MaxNumBursts != 0 && bursts < MaxNumBursts,
            // throttled to one burst per RepeatDelay ticks.
            if state.particles_id != NONE_ID && state.particles_max_bursts > 0 {
                let under_cap =
                    self.entities[i].particle_bursts < state.particles_max_bursts as u32;
                let due = self.game_time
                    >= self.entities[i].particle_last
                        + state.particles_repeat_delay.max(0) as u32;
                if under_cap && due {
                    let unit_scroll = assets
                        .units
                        .get(&self.entities[i].unit)
                        .map(|d| d.is_ground_based)
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

            // --- state sound (G_EG_Process, 0x418595) ---
            // A looping sound replays every loop delay; otherwise it plays
            // on the first pass after the state's first-ever entry, or on
            // the first pass of every entry when it repeats on state
            // change. Any pass that decides to play bumps the pass count
            // and stamps the time, even past the play cap.
            {
                let now = self.game_time;
                let e = &self.entities[i];
                let snd = &state.entry_sound;
                let mut play = false;
                if snd.id != NONE_ID {
                    if state.sound_loop {
                        let last = e.sound_last_play;
                        play = last == 0 || now as i64 >= last as i64 + state.sound_loop_delay;
                    } else if e.state_entry_count.get(e.state).copied().unwrap_or(0) == 1 {
                        play = e.sound_play_count == 0;
                    } else {
                        play = state.sound_repeat_on_state_change && e.sound_play_count == 0;
                    }
                }
                if play {
                    // TODO: sound_allow_only_one_instance skips the play
                    // while U_Sound_IsPlaying, which depends on audio.
                    let cap = state.sound_max_num_to_play;
                    if cap == 0 || (e.sound_play_count as i64) < cap {
                        self.consume_sound_rand(snd.min_pitch, snd.max_pitch);
                        self.events.push(SimEvent::Sound(snd.id));
                    }
                    let e = &mut self.entities[i];
                    e.sound_play_count += 1;
                    e.sound_last_play = now;
                }
            }

            // Timers.
            {
                let e = &mut self.entities[i];
                e.age += 1;
                e.state_time += 1;
                if e.hit_cooldown > 0 { e.hit_cooldown -= 1; }
                if e.rot_pause > 0 { e.rot_pause -= 1; }
                e.glow.process();
            }

            // --- state transitions ---
            // on_timer → change to named state (or delete). In the
            // original a normal ChangeState does NOT skip the rest of the
            // pass — it refreshes state and falls through to Animate /
            // ProcessRules / movement / SpawnControl this same tick; only
            // Delete/Destroy (dead) short-circuit via nextChild.
            if state.on_timer_target_reached(&self.entities[i]) {
                self.apply_state_change(i, &state.on_timer_change_to, assets);
                if self.entities[i].dead {
                    continue;
                }
                if let Some(d) = assets.units.get(&self.entities[i].unit) {
                    if !d.states.is_empty() {
                        state = d
                            .states[self.entities[i].state.min(d.states.len() - 1)]
                            .clone();
                    }
                }
            }
            // on_counter chains inside ChangeState (entry count), not here.

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
            // m_field_0xb8 (rot_pause) suppresses rotation while a spawn-set
            // volley is winding down (Priv_DoRotationAsRequired gate).
            {
                let epos = self.entities[i].pos;
                let has_target = self.nearest_player_dist(epos) != f32::MAX;
                self.entities[i].tracking = state.rotate_to_target
                    && has_target
                    && self.entities[i].rot_pause == 0;
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
                let owner_alive = owner_uid != u32::MAX
                    && self
                        .entities
                        .iter()
                        .any(|o| o.unique_num == owner_uid && !o.dead);
                // DoMovementAI's no-target flags (state +0x34c/+0x34d):
                // delete → [0xbf]=1 pure delete; destruct → Destroy(-1).
                if !any_active {
                    if state.delete_on_no_active_players {
                        self.entities[i].dead = true;
                        self.entities[i].killer = -1;
                        continue;
                    }
                    if state.destruct_on_no_active_players {
                        self.destroy_entity(assets, i, -1);
                        continue;
                    }
                }
                // state +0x34e: owner-relative states (0x323/0x324/0x328)
                // Destroy(-1) when vertical scrolling isn't paused —
                // gated on a LIVE owner (sub_41b700 weak-link check).
                let scroll_hold = self.scroll_paused || self.entity_scroll_pause;
                if state.destruct_if_scrolling_not_paused
                    && (state.use_owners_visibility
                        || state.use_owners_scale
                        || state.visually_reflect_owner_hits)
                    && owner_alive
                    && !scroll_hold
                {
                    self.destroy_entity(assets, i, -1);
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
                    // No active players: unit flee flags trigger Priv_Flee
                    // with the north/south flee ids, then skip motion AI.
                    // (The delete/destruct no-active flags already ran in
                    // the lifecycle block above.)
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

            // --- motion blur afterimages (G_MotionBlur_New trigger) ---
            // The original rolls the interval every tick the state's blur
            // flag is set AND the sprite face isn't 'none' — the roll is
            // inside that gate, so the guard must match or rand desyncs.
            if state.motion_blur_required && state.sprite_face != NONE_ID {
                let interval = rand_int(
                    &mut self.rng,
                    state.motion_blur_min_time,
                    state.motion_blur_max_time.max(state.motion_blur_min_time),
                ) as u32;
                if self.game_time > self.entities[i].blur_last + interval {
                    let (unit, state_i, frame, sx, sy, scale) = {
                        let e = &self.entities[i];
                        // Where the entity is drawn, screen-locked or not.
                        let sp = e.screen_pos(self);
                        (e.unit, e.state, e.frame, sp.x, sp.y, e.scale)
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
            self.step_spawn_sets(i, &state, assets);

            // --- cull ---
            let e = &self.entities[i];
            let sy = self.scroll + SCREEN_H - e.pos.y;
            let sx = e.pos.x - self.cam_x + WORLD_TO_SCREEN_X;
            let off = e.pos.y < -80.0
                || e.pos.y > self.level_height() + 80.0
                || sx < PLAY_LEFT - 160.0 || sx > PLAY_RIGHT + 224.0
                || sy < -320.0 || sy > SCREEN_H + 320.0;
            if off {
                self.entities[i].dead = true;
            }
            }
            if self.entities.len() == end {
                break;
            }
            start = end;
        }
    }

    fn apply_state_change(&mut self, idx: usize, name: &str, assets: &GameAssets) {
        let lname = name.trim().to_lowercase();
        if lname.is_empty() || lname == "none" {
            return;
        }
        if lname == "delete" {
            self.entities[idx].dead = true;
            return;
        }
        if lname == "destroy" {
            // ChangeState's "Destroy" out-flag → Destroy(-1) at the
            // caller, same as rule-driven destroys.
            self.destroy_entity(assets, idx, -1);
            return;
        }
        let unit_id = self.entities[idx].unit;
        if let Some(def) = assets.units.get(&unit_id) {
            if let Some(ns) = find_state(def, name) {
                self.change_state(idx, ns, def, assets, false);
            }
        }
    }

    fn change_state(&mut self, idx: usize, new_state: usize, def: &UnitDef, assets: &GameAssets, force: bool) {
        let ns = new_state.min(def.states.len() - 1);
        // Frame-roll gate: entity +0x1c mirrors state +0x300 (the sprite
        // face resId); the roll runs on forced entries or when it changes.
        let old_face = {
            let e = &self.entities[idx];
            def.states[e.state.min(def.states.len() - 1)].sprite_face
        };
        let epos = self.entities[idx].pos;
        let e = &mut self.entities[idx];
        e.state = ns;
        let state_idx = e.state;
        // entity +0x144[state]: cumulative entry count for on_counter.
        if e.state_entry_count.len() < def.states.len() {
            e.state_entry_count.resize(def.states.len(), 0);
        }
        e.state_entry_count[state_idx] += 1;
        // Priv_Flee on state entry: flee_id != "none" sets m_field_0xc0 and
        // computes the destination; any other state clears the flag.
        e.fleeing = def.states[ns].flee_id != NONE_ID;
        e.state_time = 0;
        e.dir_index = -1;
        let st = &def.states[e.state];
        // ChangeState rand order: on_timer roll (unconditional —
        // RandomInt(0,0) still draws), then the sprite-frame roll when
        // forced or the frame field changed (initialHeadingSetInEditor
        // units derive the frame from heading instead — no draw), then
        // the initial-scale roll on forced entries only, then Priv_Flee,
        // then the spawn-set init rolls.
        e.state_timer_target = rand_int(
            &mut self.rng,
            st.on_timer_min,
            st.on_timer_max.max(st.on_timer_min),
        ) as u32;
        if force || st.sprite_face != old_face {
            e.frame = if def.initial_heading_set_in_editor {
                // GetFrameForAngle(facing): direction-major frame layout,
                // no rand draw.
                let nd = st.num_directions.max(1);
                let span = (st.sprite_frame_max.max(st.sprite_frame_min)
                    - st.sprite_frame_min
                    + 1)
                .max(1);
                let per_dir = (span / nd).max(1);
                st.sprite_frame_min
                    + heading_to_dir(e.heading, nd as usize) * per_dir
            } else {
                rand_int(
                    &mut self.rng,
                    st.sprite_frame_min,
                    st.sprite_frame_max.max(st.sprite_frame_min),
                )
            };
        }
        if force {
            // inFlag1 scale block: roll only when the tolerance range is
            // non-zero; the negative clamp sits inside that same branch.
            let mut base = def.initial_scale_percent;
            let range = def.initial_scale_percent_tolerance;
            if range != 0 {
                let half = range / 2;
                base += rand_int(&mut self.rng, -half, half);
                if base < 0 {
                    base = 0;
                }
            }
            e.scale = base as f32 / 100.0;
        }
        // Priv_Flee rolls run after the scale block, before spawn sets.
        e.flee_target = if st.flee_id != NONE_ID {
            Self::compute_flee_target(&mut self.rng, self.scroll, assets, st.flee_id, epos)
        } else {
            None
        };
        // Priv_CheckSpawningAbilityAtStateChange: every non-'none' spawn
        // set rolls delay/total/period (in that order) and arms its first
        // volley immediately; 'none' sets reset to inert zeros.
        let now = self.game_time;
        e.spawn_timers = st
            .spawn_sets
            .iter()
            .map(|ss| {
                if ss.spawn_id == NONE_ID {
                    SpawnSetRt::default()
                } else {
                    SpawnSetRt {
                        delay: rand_int(
                            &mut self.rng,
                            ss.delay_between_min,
                            ss.delay_between_max.max(ss.delay_between_min),
                        ) as u32,
                        last_arm: now,
                        remaining: rand_int(
                            &mut self.rng,
                            ss.volley_min,
                            ss.volley_max.max(ss.volley_min),
                        ) as u32,
                        period: rand_int(
                            &mut self.rng,
                            ss.rate_min,
                            ss.rate_max.max(ss.rate_min),
                        ) as u32,
                    }
                }
            })
            .collect();
        e.anim_stopped = false;
        e.tracking = false;
        e.rot_dir = -1;
        // particle_bursts/particle_last are entity-lifetime counters
        // (m_field_ec/m_field_e8) — they persist across state changes.
        // ChangeState clears the pass count but keeps the last-play time.
        e.sound_play_count = 0;
        let has_link = st.lock_to_owner_loc || st.link_to_owner_loc || st.orbit_owner;
        let orbit = st.orbit_owner;
        self.cache_owner_link(idx, has_link, orbit);
        // ChangeState's velocity block: forced entries aim along the
        // (inverted) facing heading; transitions preserve the velocity
        // direction and ease the magnitude toward max_speed.
        self.state_velocity_init(idx, st, force);
        self.set_visual_targets(idx, st);
        // The state sound itself is played by the next G_EG_Process pass.
        // ChangeState tail: #stateOnCounter chains after N entries into
        // this state — the count resets, then ChangeState(false) recurses
        // into the named target (or Delete/Destroy flags).
        if st.on_counter > 0 {
            let reached = {
                let e = &mut self.entities[idx];
                let c = e.state_entry_count.get(state_idx).copied().unwrap_or(0);
                if c as i64 == st.on_counter {
                    e.state_entry_count[state_idx] = 0;
                    true
                } else {
                    false
                }
            };
            if reached {
                let target = st.on_counter_change_to.clone();
                self.apply_state_change(idx, &target, assets);
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
    fn compute_flee_target(rng: &mut Crand, scroll: f32, assets: &GameAssets, flee_id: ResId, pos: Vec2) -> Option<Vec2> {
        let scroll = scroll as f64;
        let p = |i: usize| pf(assets, i) as f64;
        let wy = |v: f64| (scroll + v) as f32;
        let rx = |rng: &mut Crand, hi: f64| rand_float(rng, 0.0, hi) as f32;
        let ry = |rng: &mut Crand, hi: f64| (scroll + rand_float(rng, 0.0, hi)) as f32;
        let coin = |rng: &mut Crand| rand_int(rng, 0, 1) != 0;
        let t = if flee_id == id("cega") {
            Some(Vec2::new(p(0x36) as f32, wy(p(0x37))))
        } else if flee_id == id("eara") {
            Some(Vec2::new(rx(rng, p(0x36)), wy(p(0xe))))
        } else if flee_id == id("wera") {
            Some(Vec2::new(rx(rng, p(0x36)), wy(p(0xf))))
        } else if flee_id == id("nora") {
            Some(Vec2::new(p(0x10) as f32, ry(rng, p(0x37))))
        } else if flee_id == id("sora") {
            Some(Vec2::new(p(0x11) as f32, ry(rng, p(0x37))))
        } else if flee_id == id("eace") {
            Some(Vec2::new(p(0x36) as f32, wy(p(0xe))))
        } else if flee_id == id("wece") {
            Some(Vec2::new(p(0x36) as f32, wy(p(0xf))))
        } else if flee_id == id("noce") {
            Some(Vec2::new(p(0x10) as f32, wy(p(0x37))))
        } else if flee_id == id("soce") {
            Some(Vec2::new(p(0x11) as f32, wy(p(0x37))))
        } else if flee_id == id("rave") {
            let y = if coin(rng) { p(0xe) } else { p(0xf) };
            Some(Vec2::new(rx(rng, p(0x36)), wy(y)))
        } else if flee_id == id("opve") {
            let y = if p(0x37) < pos.y as f64 { p(0xe) } else { p(0xf) };
            Some(Vec2::new(rx(rng, p(0x36)), wy(y)))
        } else if flee_id == id("raho") {
            let x = if coin(rng) { p(0x11) } else { p(0x10) };
            Some(Vec2::new(x as f32, ry(rng, p(0x37))))
        } else if flee_id == id("opho") {
            let x = if p(0x36) < pos.x as f64 { p(0x10) } else { p(0x11) };
            Some(Vec2::new(x as f32, ry(rng, p(0x37))))
        } else {
            // Unknown flee-id: fall back to a random point in the visible
            // play area (the pre-table behaviour).
            Some(Vec2::new(
                rand_float(rng, 32.0, 448.0) as f32,
                rand_float(
                    rng,
                    scroll,
                    (scroll + SCREEN_H as f64) as f64,
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

    /// `U_Sound_Play(U_Sound_Settings*)` (0x44f5f0) rolls the pitch with
    /// RandomFloat(minPitch, maxPitch), which draws unless the bounds are
    /// equal; its volume roll RandomInt(v, v) never draws. The pitch only
    /// matters to audio, so only the draw is kept.
    #[track_caller]
    fn consume_sound_rand(&mut self, min_pitch: f64, max_pitch: f64) {
        let _ = rand_float(&mut self.rng, min_pitch as f32 as f64, max_pitch as f32 as f64);
    }

    /// Emit a spark burst at an entity's screen position. `scroll` marks
    /// ground-class units whose particles track the terrain.
    fn emit_particles(&mut self, idx: usize, kind: ResId, color: u32, scroll: bool, circular: bool) {
        if kind == NONE_ID {
            return;
        }
        // G_Particle_NewGroup consumes one U_Utils_RandomInt(0,4) per
        // active particle — the draw count is fixed per type fourcc.
        let draws = match kind {
            k if k == id("med ") || k == id("laci") => 20,
            k if k == id("larg") || k == id("meci") => 40,
            k if k == id("tici") || k == id("tiny") => 5,
            k if k == id("smci") || k == id("smal") => 10,
            _ => 0,
        };
        for _ in 0..draws {
            rand_int(&mut self.rng, 0, 4);
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
        let (hw, hh) = entity_half_extents(assets, &self.entities[idx]);
        let e = &self.entities[idx];
        self.debris.push([
            e.pos.x - hw,
            e.pos.y - hh,
            e.pos.x + hw,
            e.pos.y + hh,
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
    /// collidesWithGroundObstacles that touch a debris zone are parked at
    /// g_entityGroupDefaultLoc — always (0,0), off the playfield, so the
    /// normal off-screen cull removes them next pass (m_field134 latches
    /// the one-shot). If they also create obstacles their bounds become
    /// new debris.
    fn step_debris(&mut self, idx: usize, assets: &GameAssets) {
        let (collides, creates, hit) = {
            let e = &self.entities[idx];
            let def = match assets.units.get(&e.unit) {
                Some(d) => d,
                None => return,
            };
            (
                def.collides_with_ground_obstacles,
                def.destruct_create_obstacle,
                e.debris_hit,
            )
        };
        if !collides || hit || self.debris.is_empty() {
            return;
        }
        let (hw, hh) = entity_half_extents(assets, &self.entities[idx]);
        let e = &self.entities[idx];
        let (x0, y0, x1, y1) = (e.pos.x - hw, e.pos.y - hh, e.pos.x + hw, e.pos.y + hh);
        let hit_debris = self.debris.iter().any(|d| {
            x1 >= d[0] && x0 <= d[2] && y1 >= d[1] && y0 <= d[3]
        });
        if hit_debris {
            let e = &mut self.entities[idx];
            e.pos = Vec2::ZERO;
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
        // The original compares locY in screen space (y-down); ours is
        // world altitude (y-up) — convert before the threshold check.
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
            self.scroll + SCREEN_H - e.pos.y
        };
        let scale = if rand_int(&mut self.rng, 0, 1) != 0 { 1.0f32 } else { 1.4 };
        // Retail draw order: X int roll, shared /100 fraction roll, X sign,
        // Y int roll (reusing the fraction), conditional Y sign.
        let mut dx = rand_int(&mut self.rng, 1, 4) as f32;
        let div = rand_int(&mut self.rng, 1, 100) as f32 / 100.0;
        dx += div;
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
        // dir is screen-space (y-down); negate y for world coords.
        e.vel = Vec2::new(ux * scale, -uy * scale);
        e.tvel = e.vel;
        e.vdelta = Vec2::new(ux * 0.2, -uy * 0.2);
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
        let (hw, hh) = entity_half_extents(assets, &self.entities[idx]);
        let e = &mut self.entities[idx];
        let r = hw;
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
        let min_y = self.scroll + hh;
        let max_y = self.scroll + h - hh;
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
        let target = Self::compute_flee_target(&mut self.rng, self.scroll, assets, flee_id, pos);
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
                // Orbit offset is screen-space (y-down) like
                // GetVectorFromAngle; negate y for world coords.
                e.pos = owner_pos + Vec2::new(hr.sin(), -hr.cos()) * e.orbit_radius;
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

    /// `G_Entity::SpawnControl`: per-tick spawn-set state machine.
    /// remaining > 0 → count down period; at 0 fire one shot and re-roll
    /// period. remaining == 0 → repeat sets re-arm once
    /// game_time >= last_arm + delay, re-rolling period/total/delay in
    /// that order; non-repeat sets stay inert until the next ChangeState.
    fn step_spawn_sets(&mut self, idx: usize, state: &UnitState, assets: &GameAssets) {
        if state.spawn_sets.is_empty() {
            return;
        }
        let mut timers = self.entities[idx].spawn_timers.clone();
        let (pos, now, spawner_player) = {
            let e = &self.entities[idx];
            (e.pos, self.game_time, e.spawner_player)
        };
        // spawnIfFleeing (spawnDef+0x4e): while the parent is fleeing
        // (m_field_0xc0), only flagged spawn sets may fire.
        let fleeing = state.flee_id != NONE_ID;
        let mut fires: Vec<usize> = Vec::new();
        for (si, ss) in state.spawn_sets.iter().enumerate() {
            if si >= timers.len() || ss.spawn_id == NONE_ID {
                continue;
            }
            if fleeing && !ss.spawn_if_fleeing {
                continue;
            }
            let rt = &mut timers[si];
            if rt.remaining > 0 {
                if rt.period > 0 {
                    rt.period -= 1;
                }
                if rt.period == 0 {
                    rt.remaining = rt.remaining.saturating_sub(1);
                    rt.period = rand_int(
                        &mut self.rng,
                        ss.rate_min,
                        ss.rate_max.max(ss.rate_min),
                    ) as u32;
                    fires.push(si);
                }
            } else if ss.repeat_spawns
                && now.wrapping_sub(rt.last_arm) >= rt.delay
            {
                rt.last_arm = now;
                rt.period = rand_int(
                    &mut self.rng,
                    ss.rate_min,
                    ss.rate_max.max(ss.rate_min),
                ) as u32;
                rt.remaining = rand_int(
                    &mut self.rng,
                    ss.volley_min,
                    ss.volley_max.max(ss.volley_min),
                ) as u32;
                rt.delay = rand_int(
                    &mut self.rng,
                    ss.delay_between_min,
                    ss.delay_between_max.max(ss.delay_between_min),
                ) as u32;
                // spawnDef+0x48/+0x4a → m_field_0xb8 rotation pause.
                if ss.pause_rotation_while_spawning {
                    let e = &mut self.entities[idx];
                    if ss.time_to_pause_rotation as u32 > e.rot_pause {
                        e.rot_pause = ss.time_to_pause_rotation as u32;
                    }
                }
            }
        }
        self.entities[idx].spawn_timers = timers;
        if !fires.is_empty() && std::env::var("DEIMOS_SPAWNSETLOG").is_ok() {
            eprintln!(
                "t={} spawnset parent={:08x} state='{}' fires={:?} pos=({:.0},{:.0})",
                self.game_time, self.entities[idx].unit, state.name, fires, pos.x, pos.y
            );
        }
        for si in fires {
            let ss = &state.spawn_sets[si];
            // Position: owner-relative offset or absolute. Offsets are in
            // screen space (+y down); our world y points up.
            let local = Vec2::new(ss.x_offset as f32, -ss.y_offset as f32);
            let mut req_heading = ss.heading_degrees as f32;
            let sp = if ss.adjust_offset_for_rotation {
                // adjustOffsetForUnitRotation (spawnDef+0x44): rotate the
                // offset by the angle the parent's sprite currently shows.
                // With setHeading that angle, plus the set's own heading,
                // is also what the spawned unit is sent off along.
                let mut angle = sprite_angle(&self.entities[idx], state);
                if ss.set_heading {
                    angle = (angle + ss.heading_degrees as i32).rem_euclid(360);
                    req_heading = angle as f32;
                }
                let (sin, cos) = (angle as f32).to_radians().sin_cos();
                let (ox, oy) = (ss.x_offset as f32, ss.y_offset as f32);
                pos + Vec2::new((ox * cos - oy * sin).trunc(), -(ox * sin + oy * cos).trunc())
            } else if ss.absolute_coordinates {
                Vec2::new(ss.x_offset as f32, ss.y_offset as f32)
            } else {
                pos + local
            };
            self.request_spawn(assets, SpawnReq {
                unit: ss.spawn_id,
                pos: sp,
                // Spawn-set requests only carry a heading when setHeading
                // is set (req+0xd/+0xe); otherwise f16 stays 0 so arg5=0.
                heading: if ss.set_heading { Some(req_heading) } else { None },
                heading_forced: ss.set_heading,
                owner: idx,
                spawner_player,
                stationary: ss.stationary_option,
                state: None,
            });
        }
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
            // Rule/movement-driven Destroy calls Destroy(-1) in the
            // original — no killer, so no reap-time coins.
            self.destroy_entity(assets, idx, -1);
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
                self.stat_score_points += def.score;
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
            self.consume_sound_rand(snd.min_pitch, snd.max_pitch);
            self.events.push(SimEvent::Sound(snd.id));
        }
        // hitParticles burst on non-lethal hits (G_Entity::Hit).
        self.emit_particles(
            idx,
            def.hit_particles,
            def.hit_particles_color,
            def.is_ground_based,
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
                self.request_spawn(assets, SpawnReq {
                    unit: cur_state.collision_spawn_id,
                    pos,
                    heading: None,
                    heading_forced: false,
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
                        heading_forced: false,
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
            self.request_spawn(assets, req);
        }
    }

    /// `G_Entity::CanSpawnOnMedia` — for ground-based units that don't
    /// death-spawn on any media, samples the level media mask at the
    /// entity's position. On a mask-0x1f tile it spawns the
    /// `mediaImpactSize` water-splash perm object and returns false, so
    /// the caller skips `destructSpawn`. Mask lookup mirrors
    /// `G_Bgnd_MediaMask_GetSurfaceAtLoc`: only pixel value 0x1f maps to
    /// surface 1; everything else (incl. out-of-bounds) is surface 0.
    fn can_spawn_on_media(&mut self, assets: &GameAssets, idx: usize) -> bool {
        let (is_ground, any_media, mis, pos, spawner) = {
            let e = &self.entities[idx];
            let Some(def) = assets.units.get(&e.unit) else { return true };
            (
                def.is_ground_based,
                def.do_death_spawn_on_any_media,
                def.media_impact_size,
                e.pos,
                e.spawner_player,
            )
        };
        if !is_ground || any_media {
            return true;
        }
        // loc = ((int)m_x + 0x20, scrollOffset + (int)m_y). m_x/m_y are
        // the entity's world/screen fields: pos.x IS m_x (ground units
        // store xLoc-32, so +0x20 restores the file xLoc for the mask);
        // pos.y = level_height - (scrollOffset + m_y) gives
        // loc.y = level_height - pos.y. The (int) truncations are kept
        // for faithfulness at sub-tile edges.
        let on_media = {
            let Some(mask) = &self.media_mask else { return true };
            if mask.w == 0 || self.media_tile_size <= 0 {
                return true;
            }
            let loc_x = pos.x as i64 + 32;
            let scroll_off = (self.max_scroll() - self.scroll) as i64;
            let m_y = self.scroll + SCREEN_H - pos.y;
            let loc_y = scroll_off + m_y as i64;
            let tx = loc_x / self.media_tile_size;
            let ty = loc_y / self.media_tile_size;
            tx >= 0
                && ty >= 0
                && tx < mask.w as i64
                && ty < mask.h as i64
                && mask.at(tx as u32, ty as u32) == 0x1f
        };
        if !on_media {
            return true;
        }
        // mediaImpactSize fourcc → perm water-splash object (obj[6..9]).
        // "smra"/"lara"/"mera" roll RandomInt; retail only uses "smra"
        // (Plasma Bomb, Bomb Crater) and "smal" (Tank - Tracks).
        let obj_slot = if mis == id("med ") {
            Some(8)
        } else if mis == id("lara") {
            Some(if rand_int(&mut self.rng, 0, 1) != 0 { 8 } else { 9 })
        } else if mis == id("mera") {
            Some(match rand_int(&mut self.rng, 0, 2) {
                0 => 6,
                1 => 7,
                _ => 8,
            })
        } else if mis == id("smra") {
            Some(if rand_int(&mut self.rng, 0, 1) != 0 { 6 } else { 7 })
        } else if mis == id("larg") {
            Some(9)
        } else if mis == id("smal") {
            Some(7)
        } else if mis == id("tiny") {
            Some(6)
        } else {
            None
        };
        if let Some(rid) = obj_slot.and_then(|s| assets.object_ids.entries.get(s)).map(|e| e.1) {
            if rid != NONE_ID {
                self.request_spawn(
                    assets,
                    SpawnReq {
                        unit: rid,
                        pos,
                        heading: None,
                        heading_forced: false,
                        owner: idx,
                        spawner_player: spawner,
                        stationary: false,
                        state: None,
                    },
                );
            }
        }
        false
    }

    /// Reap sweep per-entity prefix (sub_01b2d0 child handling): the
    /// owner cascade for Destroy'd entities, then the deletionSpawn
    /// media gate for entities that never ran Destroy. The sub_01ae10
    /// body runs in `group_reap`; the entity is marked `reaped` (the
    /// original's DeleteLink + free) afterwards.
    fn reap_entity(&mut self, assets: &GameAssets, idx: usize) {
        if idx >= self.entities.len()
            || !self.entities[idx].dead
            || self.entities[idx].reaped
        {
            return;
        }
        let Some(def) = assets.units.get(&self.entities[idx].unit).cloned() else {
            self.entities[idx].reaped = true;
            return;
        };
        let epos = self.entities[idx].pos;
        let owner_uid = self.entities[idx].owner;
        let destroy_ran = self.entities[idx].destroy_ran;
        let killer = self.entities[idx].killer;
        let spawner = self.entities[idx].spawner_player;
        let state = self
            .entities[idx]
            .state
            .min(def.states.len().saturating_sub(1));
        let destroy_owner = def
            .states
            .get(state)
            .map(|s| s.destroy_owner_on_destruction)
            .unwrap_or(false);

        // Owner cascade (sub_01b2d0): Destroy'd entity whose CURRENT
        // state sets destroyOwnerOnDestruction kills its live owner —
        // with the CHILD's killer ([0xce] of the reaped entity).
        // sub_41b700 weak-link check ≈ owner exists and isn't dead.
        if destroy_ran && destroy_owner && owner_uid != u32::MAX {
            let oidx = self
                .entities
                .iter()
                .position(|o| o.unique_num == owner_uid && !o.dead);
            if let Some(oidx) = oidx {
                self.destroy_entity(assets, oidx, killer);
            }
        }

        // deletionSpawn (fileData 0x2c2): only for entities removed
        // WITHOUT Destroy ([0xd2]==0), gated by CanSpawnOnMedia exactly
        // like destructSpawn. No retail unit sets deletionSpawn — the
        // path is engine-faithful but data-dead.
        if !destroy_ran
            && def.deletion_spawn != NONE_ID
            && self.can_spawn_on_media(assets, idx)
        {
            self.request_spawn(assets, SpawnReq {
                unit: def.deletion_spawn,
                pos: epos,
                heading: None,
                heading_forced: false,
                owner: idx,
                spawner_player: spawner,
                stationary: false,
                state: None,
            });
        }

        let in_flag1 = self.entities[idx].destroy_ran;
        let in_flag2 = self.entities[idx].killer != -1;
        self.group_reap(assets, idx, in_flag1, in_flag2);
        self.entities[idx].reaped = true;
    }

    /// sub_01ae10 — the universal "entity leaves the world" processor,
    /// also invoked recursively by the child cascades. `in_flag1` is the
    /// original's inFlag1 (Destroy ran / forced destroy-reap),
    /// `in_flag2` its has-killer arg. Order: child cascades → group
    /// [0xa2]/vMatch → coin release → Destroy call → dead flag →
    /// group [0x9e]--. Does NOT set `reaped`: cascaded entities remain
    /// sweep-eligible like the original's still-linked children (the
    /// sweep re-processes them with their own flags — the original's
    /// counters genuinely double-count in that case).
    fn group_reap(&mut self, assets: &GameAssets, idx: usize, in_flag1: bool, in_flag2: bool) {
        if idx >= self.entities.len() || self.entities[idx].reaped {
            return;
        }
        let Some(def) = assets.units.get(&self.entities[idx].unit).cloned() else {
            return;
        };
        let epos = self.entities[idx].pos;
        let gid = self.entities[idx].group_id;
        let ram_kill = self.entities[idx].ram_kill;
        // m_field_0x136: entity has spawn-set capacity (any state carries
        // spawn sets) — gates both child cascades in sub_01ae10.
        let has_spawn_sets = def.states.iter().any(|s| !s.spawn_sets.is_empty());

        if has_spawn_sets {
            // sub_41b090 (destructDestroyChildren): needs inFlag1;
            // children match on owner uid + their CURRENT state's
            // canBeDestroyedOnOwnerDestruction, reaped forced-destroy.
            if in_flag1 && def.destruct_destroy_children {
                let kids = self.cascade_children(assets, idx, true);
                for k in kids {
                    let f2 = self.entities[k].killer != -1;
                    self.group_reap(assets, k, true, f2);
                }
            }
            // sub_41b1b0 (destructDeleteChildren): runs for ANY reap;
            // children match owner uid + canBeDeletedOnOwnerDeletion,
            // reaped forced-delete (inFlag1=0, inFlag2=0).
            if def.destruct_delete_children {
                let kids = self.cascade_children(assets, idx, false);
                for k in kids {
                    self.group_reap(assets, k, false, false);
                }
            }
        }

        // Group [0xa2]++/vMatch — only for inFlag1 reaps.
        let mut vmatch = false;
        if in_flag1 {
            let g = self.groups.entry(gid).or_default();
            g.killed += 1;
            vmatch = g.killed == g.expected;
        }

        // Coin release: inFlag2 && inFlag1 && !ram_kill ([0xbe]==0)
        // && destructCoin != none && count > 0; then the group-kill
        // coin when vMatch (the PERM-group exclusion can't occur —
        // no retail entity joins the permanent group).
        if in_flag2 && in_flag1 && !ram_kill {
            if std::env::var("DEIMOS_COINLOG").is_ok()
                && (def.destruct_coin != NONE_ID || def.destruct_coin_on_group_kill != NONE_ID)
            {
                eprintln!(
                    "t={} coinrel unit={:08x} ncoj={} vmatch={} gid={} killed={}/{}",
                    self.game_time, self.entities[idx].unit, def.destruct_num_coins,
                    vmatch, gid,
                    self.groups.get(&gid).map(|g| g.killed).unwrap_or(0),
                    self.groups.get(&gid).map(|g| g.expected).unwrap_or(0),
                );
            }
            if def.destruct_coin != NONE_ID && def.destruct_num_coins > 0 {
                for _ in 0..def.destruct_num_coins.min(64) {
                    self.request_spawn(assets, SpawnReq {
                        unit: def.destruct_coin,
                        pos: epos,
                        heading: None,
                        heading_forced: false,
                        owner: idx,
                        spawner_player: -1,
                        stationary: false,
                        state: None,
                    });
                }
            }
            if vmatch && def.destruct_coin_on_group_kill != NONE_ID {
                self.request_spawn(assets, SpawnReq {
                    unit: def.destruct_coin_on_group_kill,
                    pos: epos,
                    heading: None,
                    heading_forced: false,
                    owner: idx,
                    spawner_player: -1,
                    stationary: false,
                    state: None,
                });
            }
        }

        // Tail Destroy call: the original invokes Destroy(killer)
        // whenever inFlag1 — a no-op for already-destroyed entities
        // (the [0xbf] guard), the real destruct path for live children
        // caught by a cascade.
        if in_flag1 {
            let k = self.entities[idx].killer;
            self.destroy_entity(assets, idx, k);
        }
        self.entities[idx].dead = true;
        // group->[0x9e]-- unconditional per sub_01ae10 call.
        if let Some(g) = self.groups.get_mut(&gid) {
            g.remaining = g.remaining.saturating_sub(1);
        }
    }

    /// sub_41b090/sub_41b1b0 child matcher: entities owned by `idx`
    /// (child owner-uid == parent unique_num) whose CURRENT state sets
    /// the requested owner-cascade flag. Already-reaped (freed)
    /// entities can't match — the original's links are gone.
    fn cascade_children(&self, assets: &GameAssets, idx: usize, want_destroy: bool) -> Vec<usize> {
        let my_uid = self.entities[idx].unique_num;
        self.entities
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                if *i == idx || e.reaped || e.owner != my_uid {
                    return false;
                }
                let Some(d) = assets.units.get(&e.unit) else {
                    return false;
                };
                if d.states.is_empty() {
                    return false;
                }
                let s = &d.states[e.state.min(d.states.len() - 1)];
                if want_destroy {
                    s.destroy_on_owner_destruction
                } else {
                    s.delete_on_owner_deletion
                }
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// `G_Entity::Destroy` — the destruct path: debris/particle/spawn/
    /// sound effects, flags, ground-accuracy, random bonus drop. Coin
    /// release and child cascades happen later in the reap sweep
    /// (sub_01ae10). No score (the original awards score in `Hit`,
    /// before `Destroy` is reached).
    pub fn destroy_entity(&mut self, assets: &GameAssets, idx: usize, from_player: i32) {
        if idx >= self.entities.len() || self.entities[idx].dead {
            return;
        }
        let def = match assets.units.get(&self.entities[idx].unit) {
            Some(d) => d.clone(),
            None => return,
        };
        let epos = self.entities[idx].pos;

        // --- G_Entity::Destroy body ---
        // G_Debris_New on the entity bounds, then the destructParticle
        // burst (its rand draws land before destructSpawn's rolls).
        self.maybe_debris_on_destroy(idx, assets);
        self.emit_particles(
            idx,
            def.destruct_particle,
            def.destruct_particle_color,
            def.is_ground_based,
            false,
        );
        // destructSpawn (0x45e): gated by CanSpawnOnMedia — ground units
        // dying on a media-mask tile spawn the water splash instead.
        // f18 = the entity's spawner player (m_field_0xca), owner = self.
        // destruct_notice: floating text — TODO notice system (unused in
        // retail data).
        if def.destruct_spawn != NONE_ID && self.can_spawn_on_media(assets, idx) {
            let spawner = self.entities[idx].spawner_player;
            self.request_spawn(assets, SpawnReq {
                unit: def.destruct_spawn,
                pos: epos,
                heading: None,
                heading_forced: false,
                owner: idx,
                spawner_player: spawner,
                stationary: false,
                state: None,
            });
        }
        if def.destruct_sound.id != NONE_ID {
            self.consume_sound_rand(def.destruct_sound.min_pitch, def.destruct_sound.max_pitch);
            self.events.push(SimEvent::Sound(def.destruct_sound.id));
        }

        self.entities[idx].dead = true;
        self.entities[idx].destroyed = true;
        self.entities[idx].destroy_ran = true;
        self.entities[idx].killer = from_player;
        self.stat_kills += 1;
        if from_player >= 0 {
            self.stat_kills_player += 1;
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
                if std::env::var("DEIMOS_BONUSLOG").is_ok() {
                    eprintln!("t={} bonusdrop unit={:08x} -> {:08x}", self.game_time, self.entities[idx].unit, bonus);
                }
                self.request_spawn(assets, SpawnReq {
                    unit: bonus,
                    pos: epos,
                    heading: None,
                    heading_forced: false,
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
            if std::env::var("DEIMOS_HITLOG").is_ok() {
                eprintln!("t={} P{} hit amt={:.1} dmg={:.1} sh={:.1} inv={}", self.game_time, slot, amount, dmg, p.shields, p.invuln);
            }
            if dmg > 0.0 {
                // pad7[0]: any real shield damage latches the no-damage
                // defence bonus off for the rest of the run.
                p.damage_taken = true;
                self.events.push(SimEvent::PlayerEvent(slot, "hit"));
            }
        }
        // Glow_Start(hitGlowColor, hitGlowSpeed) — ship flash on any
        // non-lethal hit (the original fires it even for invulnerable
        // hits). Sawtooth period ≈ 56/speed ticks.
        if p.shields >= 0.0 {
            let spd = pdef.hit_glow_speed.max(1) as u32;
            p.glow_ticks = (56 / spd).clamp(1, 255) as u8;
        }
        // Hit-effect spawn (active_SpawnOnHit), throttled by pf(0xa2) —
        // only on non-lethal hits with real damage, per G_Player::Hit.
        if p.shields >= 0.0
            && amount > 0.0
            && pdef.active_spawn_on_hit != NONE_ID
            && self.game_time as i64
                >= p.last_hit_fx as i64 + pf(assets, 0xa2) as i64
        {
            p.last_hit_fx = self.game_time;
            let pos = Vec2::new(p.screen.x + self.cam_x - WORLD_TO_SCREEN_X, self.scroll + SCREEN_H - p.screen.y);
            self.request_spawn(assets, SpawnReq {
                unit: pdef.active_spawn_on_hit,
                pos,
                heading: None,
                heading_forced: false,
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
        let mut player_died = false;
        let p = self.players[slot].as_mut().unwrap();
        if p.shields < 0.0 {
            p.shields = 0.0;
            player_died = true;
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
            p.state_data = self.game_time;
        } else if amount > 0.0 && !p.shield_warned
            && (p.shields as i64) <= pdef.shield_warning_percentage
            && pdef.active_shield_warning_object != NONE_ID
        {
            // Low-shield warning object, latched until shields recover.
            p.shield_warned = true;
            let pos = Vec2::new(p.screen.x + self.cam_x - WORLD_TO_SCREEN_X, self.scroll + SCREEN_H - p.screen.y);
            self.request_spawn(assets, SpawnReq {
                unit: pdef.active_shield_warning_object,
                pos,
                heading: None,
                heading_forced: false,
                owner: usize::MAX,
                spawner_player: slot as i32,
                stationary: true,
                state: None,
            });
        }
        // G_Player::Destroy head: sub_4198d0(playerNum) reaps every
        // entity this player spawned whose CURRENT state sets
        // canBeDestroyedOnOwnerDestruction (forced destroy-reap) or
        // canBeDeletedOnOwnerDeletion (forced delete-reap) — e.g. the
        // player's deployed ordnance vanishing with the ship. Runs
        // before the death-spawn object, matching the original order.
        if player_died {
            let spawner = slot as i32;
            let mut doomed: Vec<(usize, bool)> = Vec::new();
            for (i, e) in self.entities.iter().enumerate() {
                if e.reaped || e.spawner_player != spawner {
                    continue;
                }
                let Some(d) = assets.units.get(&e.unit) else { continue };
                if d.states.is_empty() {
                    continue;
                }
                let s = &d.states[e.state.min(d.states.len() - 1)];
                if s.destroy_on_owner_destruction {
                    doomed.push((i, true));
                } else if s.delete_on_owner_deletion {
                    doomed.push((i, false));
                }
            }
            for (i, destroy) in doomed {
                if destroy {
                    self.group_reap(assets, i, true, false);
                } else {
                    self.group_reap(assets, i, false, false);
                }
            }
        }
        // Death-spawn object (m_field0xba, spawned by G_Player::Destroy).
        if let Some((unit, pos)) = death_spawn {
            if unit != NONE_ID {
                self.request_spawn(assets, SpawnReq {
                    unit,
                    pos,
                    heading: None,
                    heading_forced: false,
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
                self.request_spawn(assets, SpawnReq {
                    unit: u,
                    pos: pos + off,
                    heading: None,
                    heading_forced: false,
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
            // Sprite-bounds collision (G_EntityGroup::Process): AABB
            // overlap gate, then a circle test between the entity and the
            // player's PREVIOUS position. Both radii are half sprite
            // heights, per the original's (bottom-top)/2.
            let (ehw, ehh) = entity_half_extents(assets, e);
            for slot in 0..2 {
                let Some(p) = self.players[slot].as_ref() else { continue };
                if p.phase != PlayerPhase::Active {
                    continue;
                }
                let (phw, phh) = player_half_extents(assets, p);
                if (e.pos.x - ppos[slot].x).abs() > ehw + phw
                    || (e.pos.y - ppos[slot].y).abs() > ehh + phh
                {
                    continue;
                }
                if self.player_pos_prev[slot].distance(e.pos) >= phh + ehh {
                    continue;
                }
                // Pickup overlap (original checks pickup_type != none first).
                if def.pickup_type != NONE_ID {
                    pickups.push((i, slot));
                    continue;
                }
                // Entity vs player (G_EG_Process, 0x418c80..0x418d44). Any
                // entity past the gate above hurts on contact: the entity
                // (or its owner, with passHitsToOwner) takes pf(0xa1), then
                // the player takes the unit's damage, unconditionally.
                player_hits.push((i, slot, def.damage));
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
            // Entity-vs-entity collision (sub_41b920) only runs for an
            // entity whose own state collides. A plasma bomb's flight
            // state does not: it can only hit once it reaches its target.
            let pstate = self.entities[i].state;
            if !pdef.states.get(pstate).map(|s| s.collides).unwrap_or(false) {
                continue;
            }
            let (pew, peh) = entity_half_extents(assets, &self.entities[i]);
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
                // Both must be the same class ('grnd' or 'air ' type code,
                // set from isGroundBased): bombs hit ground units only,
                // air weapons air units only. They must also be on opposite
                // sides (harmlessToPlayers set on exactly one).
                if tdef.is_ground_based != pdef.is_ground_based
                    || tdef.harmless_to_players == pdef.harmless_to_players
                {
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
                let (tew, teh) = entity_half_extents(assets, &self.entities[j]);
                let tpos = self.entities[j].pos;
                // AABB gate then half-height circle test, matching the
                // original's bounds-overlap + DoCircleCollisionDetection.
                if (epos.x - tpos.x).abs() > pew + tew || (epos.y - tpos.y).abs() > peh + teh {
                    continue;
                }
                if epos.distance(tpos) >= peh + teh {
                    continue;
                }
                hits.push((j, pdef.damage, espawner));
                shots.push(i);
                self.stat_hits += 1;
                break; // one target per projectile per tick
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
        // Pickups: apply effect, then Destroy directly — the original calls
        // entity->Destroy (not Hit/damage), bypassing shield/invulnerable
        // gates. Routing through damage_entity let pickups with
        // collision-immunity flags survive and re-collect every frame.
        // m_field_be=1 after Destroy is the "collected" flag that suppresses
        // reap-time coin release (ram_kill here).
        for (i, slot) in pickups {
            if self.entities[i].dead {
                continue;
            }
            if self.apply_pickup(assets, slot, i) {
                let owner = slot as i32;
                self.destroy_entity(assets, i, owner);
                self.entities[i].ram_kill = true;
            }
        }
        for (j, dmg, pl) in hits {
            self.damage_entity(assets, j, dmg, pl);
        }
        for (eidx, slot, dmg) in player_hits {
            // G_EG_Process contact block (0x418c80-0x418d44): the contacting
            // entity — or its owner when pass_hits_to_owner (state+0x327)
            // resolves — takes pf(0xa1)=100 damage credited to the player
            // FIRST; the player then takes fileData->damage unconditionally.
            // This is why enemy projectiles detonate on touch instead of
            // persisting to re-hit every frame.
            let pass = self.entities.get(eidx).and_then(|e| {
                assets
                    .units
                    .get(&e.unit)
                    .and_then(|d| d.states.get(e.state))
                    .map(|st| (st.pass_hits_to_owner, e.owner))
            });
            let mut owner_hit = false;
            if let Some((true, owner_uid)) = pass {
                if owner_uid != u32::MAX {
                    if let Some(oidx) = self
                        .entities
                        .iter()
                        .position(|o| o.unique_num == owner_uid && !o.dead)
                    {
                        self.damage_entity(assets, oidx, pf(assets, pf::PASS_HITS_TO_OWNER) as f64, slot as i32);
                        owner_hit = true;
                    }
                }
            }
            if !owner_hit {
                self.damage_entity(assets, eidx, pf(assets, pf::PASS_HITS_TO_OWNER) as f64, slot as i32);
            }
            if std::env::var("DEIMOS_HITLOG").is_ok() {
                let u = self.entities[eidx].unit;
                eprintln!("t={} P{} contact-hit unit={:08x}({}) dmg={:.2}", self.game_time, slot, u, rid_str(u), dmg);
            }
            self.damage_player(assets, slot, dmg);
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

/// `G_Entity::GetAngleFromSpriteInfo`: the angle (degrees, 0 = north,
/// clockwise) that the entity's current sprite frame depicts.
fn sprite_angle(e: &Entity, st: &UnitState) -> i32 {
    let dirs = st.num_directions.max(1) as i32;
    let per_dir = st.frames_per_direction.max(1) as i32;
    let frame = e.frame as i32;
    if dirs == 1 {
        return (360 / per_dir) * frame;
    }
    (360 / dirs) * (frame / per_dir).max(0)
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

/// World-space velocity for a game angle: `U_Math_GetVectorFromAngleAndSpeed`
/// is screen-space (y-down); negate y for our up-positive world coords.
fn vec_from_angle_speed(angle: f32, speed: f32) -> Vec2 {
    let r = angle.to_radians();
    Vec2::new(r.sin() * speed, -r.cos() * speed)
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

/// Sprite rid an entity currently draws — mirrors render.rs's
/// `entity_sprite` selection (state sprite_face → editor preview → unit).
fn entity_sprite_rid(def: &UnitDef, e: &Entity) -> ResId {
    if let Some(st) = def.states.get(e.state) {
        if st.sprite_face != NONE_ID {
            return st.sprite_face;
        }
    }
    if def.editor_preview_sprite_face != NONE_ID {
        return def.editor_preview_sprite_face;
    }
    e.unit
}

/// (half-width, half-height) of the entity's current sprite frame in
/// world units — the original's GetBounds extents (m_halfWidth/Height)
/// used by both the AABB pre-check and the circle radii. Falls back to
/// the old fixed radii when sprite dims were never warmed.
fn entity_half_extents(assets: &GameAssets, e: &Entity) -> (f32, f32) {
    let fallback = if assets
        .units
        .get(&e.unit)
        .map(|d| d.player_projectile)
        .unwrap_or(false)
    {
        (4.0, 4.0)
    } else {
        (14.0, 14.0)
    };
    let Some(def) = assets.units.get(&e.unit) else {
        return fallback;
    };
    let rid = entity_sprite_rid(def, e);
    let Some(dims) = assets.sprite_dims.get(&rid) else {
        return fallback;
    };
    let (w, h) = dims
        .get(e.frame.max(0) as usize)
        .or_else(|| dims.first())
        .copied()
        .unwrap_or((0, 0));
    if w == 0 || h == 0 {
        return fallback;
    }
    let s = e.scale.max(0.01);
    (w as f32 * 0.5 * s, h as f32 * 0.5 * s)
}

/// Player ship half-extents from the current air weapon's appearance
/// sprite at the tilt frame (G_Player::GetBounds equivalent).
fn player_half_extents(assets: &GameAssets, p: &Player) -> (f32, f32) {
    let fallback = (16.0, 16.0);
    let Some(wdef) = assets.weapons.get(&p.air_weapon) else {
        return fallback;
    };
    let rid = if p.slot == 0 {
        wdef.player1_appearance_face
    } else {
        wdef.player2_appearance_face
    };
    if rid == NONE_ID {
        return fallback;
    }
    let Some(dims) = assets.sprite_dims.get(&rid) else {
        return fallback;
    };
    let (w, h) = dims
        .get(p.tilt_frame)
        .or_else(|| dims.first())
        .copied()
        .unwrap_or((0, 0));
    if w == 0 || h == 0 {
        return fallback;
    }
    (w as f32 * 0.5, h as f32 * 0.5)
}

impl UnitState {
    /// Whether the rolled on_timer delay has been reached.
    ///
    /// Retail fires on `inArg == entryTime + duration` — i.e. `duration`
    /// frames AFTER the state-entry frame, so the entry frame itself is a
    /// free processing pass (its spawn-set volley runs before any delete).
    /// `state_time` counts processed ticks from 0, so `> target` lands on
    /// processed-frame #(target+1), matching that absolute-time check.
    fn on_timer_target_reached(&self, e: &Entity) -> bool {
        (self.on_timer_min > 0 || self.on_timer_max > 0)
            && e.state_timer_target > 0
            && e.state_time > e.state_timer_target
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
