//! Bevy rendering layer: turns `Sim` state into sprites each frame.
//!
//! World→screen mapping (top-left origin, y down):
//!   screen_x = world_x - cam_x          (cam_x = side-scroll offset)
//!   screen_y = scroll + 480 - world_y
//! Bevy world→NDC: camera centered, so bevy_pos = (screen_x - 320, 240 - screen_y).

use crate::assets::GameAssets;
use crate::game::sim::{
    Entity as SimEntity, PlayerPhase, Sim, SpawnReq, PLAY_LEFT, PLAY_RIGHT, SCREEN_H, SCREEN_W,
};
use crate::pak::ResId;
use bevy::prelude::*;
use rand::Rng;
use std::collections::HashMap;

/// Cache of decoded sprite frames → Bevy images.
#[derive(Resource, Default)]
pub struct TextureBook {
    /// (sprite rid, frame index) → image handle.
    frames: HashMap<(ResId, i64), Handle<Image>>,
    /// (sprite rid, frame index, blend amount, blit) → image with the
    /// blend baked into its alpha (see `frame_image_blend`).
    blends: HashMap<(ResId, i64, u32, Blit), Handle<Image>>,
    /// im16 rid → handle.
    images16: HashMap<ResId, Handle<Image>>,
}

/// Marks a Bevy sprite as one drawn part of a sim entity (keyed by
/// unique_num): the base sprite, its tint overlay, its glow overlay or
/// its shadow.
#[derive(Component)]
pub struct SimSprite {
    pub key: u32,
    pub part: u8,
}

/// Marks the terrain background sprite.
#[derive(Component)]
pub struct TerrainSprite;

/// Marks a player ship sprite, or its shadow.
#[derive(Component)]
pub struct PlayerSprite {
    pub slot: usize,
    pub shadow: bool,
}

/// The running game sim.
#[derive(Resource)]
pub struct GameSim(pub Sim);

// ---------------------------------------------------------------------------
// Game flow: title → sector select → briefing → play → cleared/game over
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlowState {
    Title,
    Sectors,
    Briefing,
    Playing,
    /// High-score table, shown from the title screen and after a game.
    Scores,
    /// Typing a name for a qualifying score, on the score table.
    NameEntry,
    Credits,
    /// Key-binding screen.
    Controls,
}

#[derive(Resource)]
pub struct Flow {
    pub state: FlowState,
    /// Selected sector index into `level_ids`.
    pub sector: usize,
    /// Chosen player count for the next run.
    pub players: usize,
    /// Ticks spent in the current state.
    pub ticks: u32,
    /// Fraction of a 60 Hz tick carried between frames.
    pub tick_acc: f32,
    /// Level rids in mission order (sorted le01..le12).
    pub level_ids: Vec<ResId>,
    /// Pause toggle (original: Caps Lock).
    pub paused: bool,
    /// Pending hi-score name entries (one per qualifying player).
    pub pending_scores: Vec<PendingScore>,
    /// Name typed so far in FlowState::NameEntry.
    pub name_text: String,
    /// Row of the score table to highlight (the entry just added).
    pub score_hilite: Option<usize>,
    /// Selected button on the title screen.
    pub menu_sel: usize,
    /// Selected (action row, player column) on the Controls screen.
    pub controls_sel: (usize, usize),
    /// Controls screen is waiting for the key to bind.
    pub rebinding: bool,
    /// Page of the credits being shown.
    pub credits_page: usize,
    /// `ticks` value at which every player was found dead, while the
    /// game-over notice plays out.
    pub game_over_at: Option<u32>,
    /// Run started above sector 1: 1 life, no hi-score, no progress credit
    /// (original level-select restriction).
    pub restricted: bool,
    /// Sequentially completed level count; sectors 0..=progress are selectable.
    pub progress: u32,
    /// Run-scoped state carried into the next level (score/lives/etc.).
    pub carry: Option<crate::game::sim::RunCarry>,
    /// Levels started this run (exe 0x4e482a equivalent).
    pub levels_played: u32,
}

/// A run result waiting for a player name.
#[derive(Clone)]
pub struct PendingScore {
    pub score: i64,
    pub level: String,
}

/// One row of the hi-score table (top 15 like the original prefs image).
#[derive(Clone)]
pub struct ScoreEntry {
    pub name: String,
    pub score: i64,
    pub level: String,
}

const SCORES_PATH: &str = "deimos-hi-scores.txt";
const PROGRESS_PATH: &str = "deimos-progress.txt";
const MAX_SCORES: usize = 15;

/// Number of sequentially completed levels (sector-select unlock count).
/// The original stores this in the prefs blob; the remake uses a plain
/// runtime file (never committed).
fn load_progress() -> u32 {
    std::fs::read_to_string(PROGRESS_PATH)
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

fn save_progress(progress: u32) {
    let _ = std::fs::write(PROGRESS_PATH, format!("{}\n", progress));
}

/// Local hi-score table. The original persists an encrypted prefs blob;
/// the remake keeps a plain runtime file (never committed).
#[derive(Resource, Default)]
pub struct HighScores {
    pub entries: Vec<ScoreEntry>,
}

impl HighScores {
    pub fn load() -> Self {
        let mut entries = Vec::new();
        if let Ok(text) = std::fs::read_to_string(SCORES_PATH) {
            for line in text.lines() {
                let mut it = line.splitn(3, '|');
                if let (Some(score), Some(level), Some(name)) =
                    (it.next(), it.next(), it.next())
                {
                    if let Ok(s) = score.parse::<i64>() {
                        entries.push(ScoreEntry {
                            name: name.trim().to_string(),
                            score: s,
                            level: level.trim().to_string(),
                        });
                    }
                }
            }
        }
        entries.sort_by(|a, b| b.score.cmp(&a.score));
        entries.truncate(MAX_SCORES);
        HighScores { entries }
    }

    pub fn save(&self) {
        let mut s = String::new();
        for e in &self.entries {
            s.push_str(&format!("{}|{}|{}\n", e.score, e.level, e.name));
        }
        let _ = std::fs::write(SCORES_PATH, s);
    }

    /// Would `score` make the table?
    pub fn qualifies(&self, score: i64) -> bool {
        score > 0
            && (self.entries.len() < MAX_SCORES
                || self.entries.last().map(|e| score > e.score).unwrap_or(true))
    }

    /// Insert a named score, keep the top MAX_SCORES sorted, and return
    /// the row it landed on.
    pub fn insert(&mut self, name: String, score: i64, level: String) -> usize {
        // After every entry scoring at least as much, like the preview row.
        let row = self.entries.iter().position(|e| score > e.score).unwrap_or(self.entries.len());
        self.entries.insert(row, ScoreEntry { name, score, level });
        self.entries.truncate(MAX_SCORES);
        self.save();
        row
    }
}

// ---------------------------------------------------------------------------
// Demo/attract mode (G_Film port): record per-player input bitmasks during
// play, replay them on the title screen after idle. Local file only.
// ---------------------------------------------------------------------------

const DEMO_PATH: &str = "deimos-demo.bin";
const DEMO_MAX_FRAMES: usize = 20000; // original pad is 0x4e20 bytes

/// Retail demo films (`film` resources) the original's attract mode plays
/// on the title screen, cycled each time idle playback triggers. The local
/// `deimos-demo.bin` recording is only a fallback when films can't load.
const DEMO_FILMS: [u32; 4] = [0x31306564, 0x32306564, 0x33306564, 0x34306564];

/// Recorded input stream for one player: 7 bits/frame
/// (up,down,left,right,fire,bomb,cycle).
#[derive(Default)]
pub struct DemoRec {
    pub level: Option<ResId>,
    pub seed: u64,
    pub p1: Vec<u8>,
    pub p2: Vec<u8>,
}

impl DemoRec {
    /// Parse an input script: one `<from>-<to> <keys...>` line per span of
    /// frames (inclusive; a lone `<frame>` is a single frame). Keys are
    /// `up down left right fire bomb cycle`, for player 1 unless written
    /// `p2:<key>`. Spans may overlap; `#` starts a comment.
    pub fn from_script(text: &str, level: ResId, seed: u64) -> Result<DemoRec, String> {
        let mut rec = DemoRec { level: Some(level), seed, p1: Vec::new(), p2: Vec::new() };
        for (ln, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let err = |what: &str| format!("line {}: {}", ln + 1, what);
            let mut words = line.split_whitespace();
            let span = words.next().unwrap();
            let (from, to) = span.split_once('-').unwrap_or((span, span));
            let from: usize = from.parse().map_err(|_| err("bad start frame"))?;
            let to: usize = to.parse().map_err(|_| err("bad end frame"))?;
            if to < from || to >= DEMO_MAX_FRAMES {
                return Err(err("bad frame span"));
            }
            let (mut m1, mut m2) = (0u8, 0u8);
            for word in words {
                let (mask, key) = match word.strip_prefix("p2:") {
                    Some(k) => (&mut m2, k),
                    None => (&mut m1, word),
                };
                let bit = ["up", "down", "left", "right", "fire", "bomb", "cycle"]
                    .iter()
                    .position(|k| *k == key)
                    .ok_or_else(|| err("unknown key"))?;
                *mask |= 1 << bit;
            }
            for (stream, mask) in [(&mut rec.p1, m1), (&mut rec.p2, m2)] {
                if stream.len() <= to {
                    stream.resize(to + 1, 0);
                }
                for b in &mut stream[from..=to] {
                    *b |= mask;
                }
            }
        }
        Ok(rec)
    }

    /// Both players' inputs at frame `f` (nothing pressed past the end).
    pub fn input_at(&self, f: usize) -> PlayerInput {
        PlayerInput {
            p1: self.p1.get(f).map(|b| DemoRec::unpack(*b)).unwrap_or_default(),
            p2: self.p2.get(f).map(|b| DemoRec::unpack(*b)).unwrap_or_default(),
        }
    }

    fn pack(s: &SlotInput) -> u8 {
        (s.up as u8)
            | ((s.down as u8) << 1)
            | ((s.left as u8) << 2)
            | ((s.right as u8) << 3)
            | ((s.fire as u8) << 4)
            | ((s.bomb as u8) << 5)
            | ((s.cycle as u8) << 6)
    }
    fn unpack(b: u8) -> SlotInput {
        SlotInput {
            up: b & 1 != 0,
            down: b & 2 != 0,
            left: b & 4 != 0,
            right: b & 8 != 0,
            fire: b & 16 != 0,
            bomb: b & 32 != 0,
            cycle: b & 64 != 0,
        }
    }
    pub fn frames(&self) -> usize {
        self.p1.len().max(self.p2.len())
    }
    fn save(&self) {
        if let Some(lvl) = self.level {
            let mut buf = Vec::with_capacity(16 + self.p1.len() + self.p2.len());
            buf.extend_from_slice(&lvl.to_le_bytes());
            buf.extend_from_slice(&self.seed.to_le_bytes());
            buf.extend_from_slice(&(self.p1.len() as u32).to_le_bytes());
            buf.extend_from_slice(&self.p1);
            buf.extend_from_slice(&(self.p2.len() as u32).to_le_bytes());
            buf.extend_from_slice(&self.p2);
            let _ = std::fs::write(DEMO_PATH, buf);
        }
    }
    fn load() -> Option<Self> {
        let buf = std::fs::read(DEMO_PATH).ok()?;
        if buf.len() < 24 {
            return None;
        }
        let lvl = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        let seed = u64::from_le_bytes(buf[4..12].try_into().ok()?);
        let n1 = u32::from_le_bytes(buf[12..16].try_into().ok()?) as usize;
        if buf.len() < 16 + n1 + 4 {
            return None;
        }
        let p1 = buf[16..16 + n1].to_vec();
        let n2 = u32::from_le_bytes(buf[16 + n1..20 + n1].try_into().ok()?) as usize;
        if buf.len() < 20 + n1 + n2 {
            return None;
        }
        let p2 = buf[20 + n1..20 + n1 + n2].to_vec();
        Some(DemoRec { level: Some(lvl), seed, p1, p2 })
    }

    /// Build a playback stream from a retail `film` resource, translating
    /// each packed input byte through the recovered G_Film bit map
    /// (bit0=L bit1=R bit2=U bit3=D bit4=bomb bit5=air-fire bit6=cycle)
    /// into our SlotInput packing. This is what the original's attract
    /// mode replays; the seed/level come straight from the film header.
    pub fn from_film(film: &crate::assets::FilmData) -> Self {
        let tr = |b: u8| -> u8 {
            let mut s = SlotInput::default();
            if b & 0x01 != 0 { s.left = true; }
            if b & 0x02 != 0 { s.right = true; }
            if b & 0x04 != 0 { s.up = true; }
            if b & 0x08 != 0 { s.down = true; }
            if b & 0x10 != 0 { s.bomb = true; }
            if b & 0x20 != 0 { s.fire = true; }
            if b & 0x40 != 0 { s.cycle = true; }
            DemoRec::pack(&s)
        };
        let n = film.frames[0].max(film.frames[1]);
        let p1 = (0..n)
            .map(|t| film.inputs[0].get(t).map(|&b| tr(b)).unwrap_or(0))
            .collect();
        let p2 = (0..n)
            .map(|t| film.inputs[1].get(t).map(|&b| tr(b)).unwrap_or(0))
            .collect();
        DemoRec { level: Some(film.level_id), seed: film.seed as u64, p1, p2 }
    }
}

/// Demo recorder + playback cursor.
#[derive(Resource, Default)]
pub struct Demo {
    /// Inputs recorded during the current run.
    pub rec: DemoRec,
    /// Playback state: demo being replayed + frame cursor.
    pub playback: Option<DemoRec>,
    pub frame: usize,
    /// Cursor into DEMO_FILMS so successive attract triggers cycle demos.
    pub film_idx: usize,
}


// ---------------------------------------------------------------------------
// Particles (G_Particle port): spark bursts in screen space
// ---------------------------------------------------------------------------

/// One live particle.
#[derive(Clone)]
struct PPart {
    pos: Vec2,
    vel: Vec2,
    /// Growth/life counter 0..32 (original m_field6).
    grow: f32,
    active: bool,
    color: Color,
}

struct PGroup {
    /// Scroll with the terrain (original m_field414, ground-class units).
    scroll: bool,
    parts: Vec<PPart>,
}

/// Velocity lookup tables: 100 entries each, generated once like
/// G_Particle_LoadHeader (random unit vectors; table1 gets a random
/// magnitude class {1, 32, 65535}, table2 stays unit length).
struct ParticleTables {
    t1: Vec<Vec2>,
    t2: Vec<Vec2>,
    idx1: usize,
    idx2: usize,
}

#[derive(Resource, Default)]
pub struct Particles {
    groups: Vec<PGroup>,
    tables: Option<ParticleTables>,
    last_scroll: f32,
    /// Sim tick the particles were last advanced at.
    last_tick: u32,
    white: Option<Handle<Image>>,
    /// Particle randomness is seeded so a replay renders identically every
    /// run (screenshots can then be compared).
    rng: Option<rand::rngs::StdRng>,
}

/// Fixed seed for the visual-only particle RNG.
const PARTICLE_SEED: u64 = 0xDE1305;

impl Particles {
    fn tables(&mut self) -> &mut ParticleTables {
        if self.tables.is_none() {
            use rand::SeedableRng;
            let mut rng = self
                .rng
                .take()
                .unwrap_or_else(|| rand::rngs::StdRng::seed_from_u64(PARTICLE_SEED));
            let mut t1 = Vec::with_capacity(100);
            let mut t2 = Vec::with_capacity(100);
            for _ in 0..100 {
                // Random point in the 416x480 view → normalized vector
                // toward the center-ish target (invRange - rand).
                let rx = rng.gen_range(0.0..416.0f32);
                let ry = rng.gen_range(0.0..480.0f32);
                let dx = 208.0 - rx;
                let dy = 240.0 - ry;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let v = Vec2::new(dx / len, dy / len);
                t2.push(v);
                let mult = match rng.gen_range(0..4) {
                    1 => 32.0,
                    2 => 65535.0,
                    _ => 1.0,
                };
                t1.push(v * mult);
            }
            self.tables = Some(ParticleTables { t1, t2, idx1: 0, idx2: 0 });
            self.rng = Some(rng);
        }
        self.tables.as_mut().unwrap()
    }

    /// `G_Particle_NewGroup`: spawn a spark burst. `circular` selects the
    /// unit-vector table (hitParticleDoCircularBurst / "icem" type).
    pub fn burst(
        &mut self,
        x: f32,
        y: f32,
        kind: ResId,
        color: u32,
        scroll: bool,
        circular: bool,
    ) {
        // (count, fast?, unit_table?) per particle type.
        let (count, fast, unit_tbl) = match kind {
            k if k == crate::pak::id("med ") => (20usize, false, false),
            k if k == crate::pak::id("larg") => (40, false, false),
            k if k == crate::pak::id("laci") => (20, false, false),
            k if k == crate::pak::id("icem") => (40, false, true),
            k if k == crate::pak::id("tici") => (5, true, false),
            k if k == crate::pak::id("smci") => (10, true, false),
            k if k == crate::pak::id("smal") => (10, true, false),
            k if k == crate::pak::id("tiny") => (5, true, false),
            _ => return,
        };
        let use_t2 = unit_tbl || circular;
        let speed_mul = if fast { 5.0 } else { -32.0 };
        // Shade variants: base * (1 - i*0.12), secondary set *0.6.
        let base = [
            ((color >> 16) & 0xff) as f32 / 255.0,
            ((color >> 8) & 0xff) as f32 / 255.0,
            (color & 0xff) as f32 / 255.0,
        ];
        let shades: Vec<Color> = (0..5)
            .flat_map(|i| {
                let f = 1.0 - i as f32 * 0.12;
                [f, f * 0.6].map(move |s| {
                    Color::srgba(
                        (base[0] * s).min(1.0),
                        (base[1] * s).min(1.0),
                        (base[2] * s).min(1.0),
                        1.0,
                    )
                })
            })
            .collect();
        let mut parts = Vec::with_capacity(count);
        // Building the tables also seeds the RNG; borrow it for this burst.
        self.tables();
        let mut rng = self.rng.take().expect("particle rng seeded by tables()");
        for _ in 0..count {
            let (vx, vy) = {
                let t = self.tables();
                if use_t2 {
                    let i = t.idx2;
                    t.idx2 = (t.idx2 + 1) % 99;
                    let v = t.t2[i];
                    (v.x, v.y)
                } else {
                    let i = t.idx1;
                    t.idx1 = (t.idx1 + 1) % 99;
                    let v = t.t1[i];
                    (v.x, v.y)
                }
            };
            parts.push(PPart {
                pos: Vec2::new(x, y),
                vel: Vec2::new(vx * speed_mul, vy * speed_mul),
                grow: 0.0,
                active: true,
                color: shades[rng.gen_range(0..shades.len())],
            });
        }
        self.rng = Some(rng);
        self.groups.push(PGroup { scroll, parts });
        if self.groups.len() > 64 {
            self.groups.remove(0);
        }
    }
}

/// Advance particles one 60 Hz tick (G_Particle_Process).
fn particle_tick(p: &mut Particles, scroll_delta: f32) {
    // Bounds (G_Particle: game-area x in [-32, 448], with a 32-px margin
    // above and below), in window space.
    const BX0: f32 = PLAY_LEFT - 32.0;
    const BX1: f32 = PLAY_LEFT + 448.0;
    const BY0: f32 = -32.0;
    const BY1: f32 = 512.0;
    p.groups.retain_mut(|g| {
        let mut any = false;
        for pt in g.parts.iter_mut() {
            if !pt.active {
                continue;
            }
            if g.scroll {
                pt.pos.y += scroll_delta;
            }
            pt.vel *= 0.96; // pf(0x90)
            pt.pos += pt.vel;
            if pt.pos.x < BX0 || pt.pos.x + 7.0 > BX1 || pt.pos.y < BY0 || pt.pos.y + 7.0 > BY1 {
                pt.active = false;
                continue;
            }
            pt.grow += 1.0; // pf(0x94)
            if pt.grow > 32.0 {
                pt.active = false;
            } else {
                any = true;
            }
        }
        any
    });
}

/// Spawn/despawn particle sprites each frame.
pub fn sync_particles(
    mut commands: Commands,
    mut parts: ResMut<Particles>,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(Entity, &ParticleSprite, &mut Sprite, &mut Transform)>,
) {
    if flow.state != FlowState::Playing {
        for (e, _, _, _) in q.iter() {
            commands.entity(e).despawn();
        }
        parts.groups.clear();
        parts.last_tick = 0;
        return;
    }
    // One particle tick per sim tick.
    let now = sim_res.0.game_time;
    let ticks = now.saturating_sub(parts.last_tick).min(8);
    parts.last_tick = now;
    if ticks > 0 {
        // Spread the terrain scroll since the last tick over this frame's ticks.
        let scroll_delta = (sim_res.0.scroll - parts.last_scroll) / ticks as f32;
        parts.last_scroll = sim_res.0.scroll;
        for _ in 0..ticks {
            particle_tick(&mut parts, scroll_delta);
        }
    }
    // White 2x2 tex for particle quads.
    if parts.white.is_none() {
        let px = [255u8, 255, 255, 255].repeat(4);
        parts.white = Some(images.add(rgba_image(2, 2, &px)));
    }
    let white = parts.white.clone().unwrap();

    // Desired: (group, part) -> (pos, color, grow)
    let mut desired: Vec<(u32, Vec2, Color, f32)> = Vec::new();
    for (gi, g) in parts.groups.iter().enumerate() {
        for (pi, p) in g.parts.iter().enumerate() {
            if p.active {
                desired.push(((gi * 64 + pi) as u32, p.pos, p.color, p.grow));
            }
        }
    }
    // Despawn missing.
    for (bevy_e, ps, _, _) in q.iter() {
        if !desired.iter().any(|d| d.0 == ps.key) {
            commands.entity(bevy_e).despawn();
        }
    }
    for (key, pos, color, grow) in desired {
        // Unique depth per particle: equal-z sprites have no stable order.
        let wpos = to_world_z(pos.x, pos.y, Z_PARTICLES + (key % 4096) as f32 / 4096.0 * 4.0);
        // Quick fade-in over first 8 ticks of growth.
        let alpha = (grow / 8.0).min(1.0);
        let col = Color::srgba(color.to_srgba().red, color.to_srgba().green, color.to_srgba().blue, alpha);
        let mut found = None;
        for (_bevy_e, ps, sprite, transform) in q.iter_mut() {
            if ps.key == key {
                found = Some((sprite, transform));
                break;
            }
        }
        if let Some((mut sprite, mut transform)) = found {
            transform.translation = wpos;
            sprite.color = col;
        } else {
            let mut sprite = Sprite::from_image(white.clone());
            sprite.color = col;
            commands.spawn((
                sprite,
                Transform::from_translation(wpos),
                ParticleSprite { key },
            ));
        }
    }
}

#[derive(Component)]
pub struct ParticleSprite {
    key: u32,
}

// ---------------------------------------------------------------------------
// Motion blur (G_MotionBlur port): frozen sprite afterimages that fade out
// ---------------------------------------------------------------------------

/// One fading afterimage (G_MotionBlurObject snapshot).
pub struct BlurFx {
    /// Sprite rid resolved from unit+state (entity_sprite rules).
    pub rid: ResId,
    pub frame: i64,
    pub pos: Vec2, // screen space
    pub scale: f32,
    /// Current visibility 0..1; fades by vis_d per tick.
    pub vis: f32,
    pub vis_d: f32,
}

#[derive(Resource, Default)]
pub struct Blurs {
    pub list: Vec<BlurFx>,
    /// Sim tick the afterimages were last faded at.
    last_tick: u32,
}

/// Advance blurs one tick and sync their sprites.
pub fn sync_blurs(
    mut commands: Commands,
    mut blurs: ResMut<Blurs>,
    flow: Res<Flow>,
    sim_res: Res<GameSim>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &BlurSprite, &mut Sprite, &mut Transform)>,
) {
    if flow.state != FlowState::Playing {
        for (e, _, _, _) in q.iter() {
            commands.entity(e).despawn();
        }
        blurs.list.clear();
        blurs.last_tick = 0;
        return;
    }
    // Fade once per sim tick, like the original's per-tick process.
    let now = sim_res.0.game_time;
    let ticks = now.saturating_sub(blurs.last_tick);
    blurs.last_tick = now;
    for _ in 0..ticks.min(8) {
        blurs.list.retain_mut(|b| {
            b.vis -= b.vis_d / 100.0;
            b.vis > 0.0
        });
    }
    // Cap the list like the original's motion-blur limit.
    if blurs.list.len() > 256 {
        let excess = blurs.list.len() - 256;
        blurs.list.drain(0..excess);
    }

    let mut desired: Vec<(u32, Vec2, ResId, i64, f32, f32)> = Vec::new();
    for (i, b) in blurs.list.iter().enumerate() {
        desired.push((i as u32, b.pos, b.rid, b.frame, b.vis, b.scale));
    }
    for (bevy_e, bs, _, _) in q.iter() {
        if !desired.iter().any(|d| d.0 == bs.key) {
            commands.entity(bevy_e).despawn();
        }
    }
    for (key, pos, rid, frame, vis, scale) in desired {
        let handle = match frame_image(&mut *assets, &mut *images, &mut *book, rid, frame) {
            Some(h) => h,
            None => continue,
        };
        let wpos = to_world_z(pos.x, pos.y, Z_BLUR + (key % 4096) as f32 / 4096.0 * 4.0);
        let mut found = None;
        for (_be, bs, sprite, transform) in q.iter_mut() {
            if bs.key == key {
                found = Some((sprite, transform));
                break;
            }
        }
        if let Some((mut sprite, mut transform)) = found {
            transform.translation = wpos;
            transform.scale = Vec3::splat(scale.max(0.01));
            sprite.color.set_alpha(vis.clamp(0.0, 1.0));
        } else {
            let mut sprite = Sprite::from_image(handle);
            sprite.color.set_alpha(vis.clamp(0.0, 1.0));
            commands.spawn((
                sprite,
                Transform::from_translation(wpos).with_scale(Vec3::splat(scale.max(0.01))),
                BlurSprite { key },
            ));
        }
    }
}

#[derive(Component)]
pub struct BlurSprite {
    key: u32,
}

// ---------------------------------------------------------------------------
// Weapon crosshair (G_WeaponHandler draw list): per-player reticle from the
// ground weapon def; locked frame when an enemy is under the ship.
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct Crosshair {
    pub slot: usize,
}

/// Weapon-selector popup marker (world-space text near the ship).
#[derive(Component)]
pub struct WeaponPopup {
    pub slot: usize,
}

/// Sync the weapon-name popup shown for ~60 ticks after cycling
/// (WepSelectorAppearanceTime).
pub fn sync_weapon_popup(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    assets: Res<GameAssets>,
    mut q: Query<(Entity, &WeaponPopup, &mut Text2d, &mut TextColor, &mut Transform)>,
) {
    let sim = &sim_res.0;
    let mut seen = [false; 2];
    if flow.state == FlowState::Playing {
        for slot in 0..2 {
            let Some(p) = sim.players[slot].as_ref() else { continue };
            if p.weapon_popup == 0 || p.phase != crate::game::sim::PlayerPhase::Active {
                continue;
            }
            seen[slot] = true;
            let name = assets
                .weapons
                .get(&p.air_weapon)
                .map(|w| w.name.clone())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            // Fade out over the last 20 ticks.
            let alpha = (p.weapon_popup.min(20) as f32 / 20.0).clamp(0.0, 1.0);
            let pos = to_world_z(p.screen.x, p.screen.y - 34.0, layer_z(13));
            let mut found = None;
            for (be, wp, text, col, tr) in q.iter_mut() {
                if wp.slot == slot {
                    found = Some((be, text, col, tr));
                    break;
                }
            }
            if let Some((_, mut text, mut col, mut tr)) = found {
                if text.0 != name {
                    text.0 = name;
                }
                tr.translation = pos;
                col.0 = Color::srgba(0.7, 0.9, 1.0, alpha);
            } else {
                commands.spawn((
                    Text2d::new(name.clone()),
                    TextFont {
                        font_size: bevy::text::FontSize::Px(11.0),
                        ..default()
                    },
                    TextColor(Color::srgba(0.7, 0.9, 1.0, alpha)),
                    Transform::from_translation(pos),
                    WeaponPopup { slot },
                ));
            }
        }
    }
    for (be, wp, _, _, _) in q.iter() {
        if !seen[wp.slot] {
            commands.entity(be).despawn();
        }
    }
}

/// Sync the per-player weapon crosshair sprites.
pub fn sync_crosshair(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &Crosshair, &mut Sprite, &mut Transform)>,
) {
    let sim = &sim_res.0;
    if flow.state != FlowState::Playing {
        for (e, _, _, _) in q.iter() {
            commands.entity(e).despawn();
        }
        return;
    }
    for slot in 0..2 {
        let Some(p) = sim.players[slot].as_ref() else { continue };
        if p.phase != PlayerPhase::Active {
            continue;
        }
        let Some(wdef) = assets.weapons.get(&p.ground_weapon).cloned() else { continue };
        if wdef.crosshair_face == crate::pak::NONE_ID {
            continue;
        }
        // Locked frame when a ground-class, state-targetable enemy sits
        // under the player (original: unit class "grnd" + stateIsTargetable
        // + player loc inside entity bounds → Crosshair_Hilite).
        let ps = Vec2::new(p.screen.x, p.screen.y);
        let locked = sim.entities.iter().any(|e| {
            if e.dead {
                return false;
            }
            let Some(d) = assets.units.get(&e.unit) else { return false };
            // Original also requires canBeHitByPlayerProjectile (fileData+0x104).
            if !d.is_ground_based || !d.can_be_hit_by_player_projectile {
                return false;
            }
            let targetable = d
                .states
                .get(e.state)
                .map(|st| st.is_targetable)
                .unwrap_or(false);
            if !targetable {
                return false;
            }
            let es = e.screen_pos(sim);
            (es.x - ps.x).abs() < 40.0 && es.y > ps.y - 10.0 && es.y < ps.y + 160.0
        });
        let (rid, frame) = if locked
            && wdef.crosshair_locked_face != crate::pak::NONE_ID
        {
            (wdef.crosshair_locked_face, wdef.crosshair_locked_frame)
        } else {
            (wdef.crosshair_face, wdef.crosshair_frame)
        };
        let Some(handle) = frame_image(&mut *assets, &mut *images, &mut *book, rid, frame) else { continue };
        // Crosshair offsets are screen-space; +y is down-screen.
        let pos = Vec2::new(
            ps.x + wdef.crosshair_x_offset as f32,
            ps.y + wdef.crosshair_y_offset as f32,
        );
        let wpos = to_world_z(pos.x, pos.y, layer_z(13));
        let mut found = None;
        for (be, ch, sprite, transform) in q.iter_mut() {
            if ch.slot == slot {
                found = Some((sprite, transform));
                break;
            }
        }
        if let Some((mut sprite, mut transform)) = found {
            transform.translation = wpos;
            sprite.image = handle;
        } else {
            commands.spawn((
                Sprite::from_image(handle),
                Transform::from_translation(wpos),
                Crosshair { slot },
            ));
        }
    }
    // Despawn crosshairs for inactive/missing players.
    for (be, ch, _, _) in q.iter() {
        let active = sim.players[ch.slot]
            .as_ref()
            .map(|p| p.phase == PlayerPhase::Active)
            .unwrap_or(false);
        if !active {
            commands.entity(be).despawn();
        }
    }
}

/// Input state gathered from Bevy keys.
#[derive(Resource, Default)]
pub struct FrameInput {
    pub p1: crate::game::sim::SlotInput,
    pub p2: crate::game::sim::SlotInput,
    /// Number of players in this run.
    pub players: usize,
    /// Level list index for next level / flow control.
    pub level_index: usize,
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

pub fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Convert RGBA pixels to a Bevy image with nearest-neighbour sampling.
pub fn rgba_image(w: u32, h: u32, rgba: &[u8]) -> Image {
    let mut img = Image::new(
        bevy::render::render_resource::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        rgba.to_vec(),
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = bevy::image::ImageSampler::nearest();
    img
}

/// Get-or-create the Bevy image for one sprite frame.
fn frame_image(
    assets: &mut GameAssets,
    images: &mut Assets<Image>,
    book: &mut TextureBook,
    rid: ResId,
    frame: i64,
) -> Option<Handle<Image>> {
    let key = (rid, frame);
    if let Some(h) = book.frames.get(&key) {
        return Some(h.clone());
    }
    let sprite = assets.sprite(rid)?;
    let f = sprite.frames.get(frame.max(0) as usize).or_else(|| sprite.frames.first())?;
    let img = rgba_image(f.width, f.height, &f.rgba);
    let handle = images.add(img);
    book.frames.insert(key, handle.clone());
    Some(handle)
}

/// Which of the original's sprite blits a baked texture stands in for.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Blit {
    /// `DrawNormal` / `DrawTranslucent`: the sprite's own colours.
    Sprite,
    /// `DrawColorised`: white, for `Sprite::color` to supply the tint.
    Flat,
    /// `DrawShadow`: black.
    Shadow,
}

/// Sprite frame as the original's translucent, colourised and shadow blits
/// draw it: `blend` (0..=32) is added to each pixel's own transparency
/// level, and anything reaching 32 vanishes.
pub(crate) fn frame_image_blend(
    assets: &mut GameAssets,
    images: &mut Assets<Image>,
    book: &mut TextureBook,
    rid: ResId,
    frame: i64,
    blend: u32,
    blit: Blit,
) -> Option<Handle<Image>> {
    if blend == 0 && blit == Blit::Sprite {
        return frame_image(assets, images, book, rid, frame);
    }
    let key = (rid, frame, blend, blit);
    if let Some(h) = book.blends.get(&key) {
        return Some(h.clone());
    }
    let sprite = assets.sprite(rid)?;
    let f = sprite.frames.get(frame.max(0) as usize).or_else(|| sprite.frames.first())?;
    let mut rgba = f.rgba.clone();
    for px in rgba.chunks_exact_mut(4) {
        // Alpha was stored as (32 - level) * 255 / 32; recover the level.
        let level = 32 - (px[3] as u32 * 32 + 127) / 255;
        let weight = if level >= 32 { 32 } else { (level + blend).min(32) };
        let opacity = (32 - weight) as f32 / 32.0;
        match blit {
            Blit::Sprite => {}
            Blit::Flat => px[..3].fill(255),
            Blit::Shadow => px[..3].fill(0),
        }
        px[3] = if blit == Blit::Shadow {
            // The original darkens the stored (gamma-encoded) pixel values;
            // the GPU blends in linear light. For black the two agree when
            // the opacity is raised to match: 1 - (1 - a)^2.2.
            ((1.0 - (1.0 - opacity).powf(2.2)) * 255.0).round() as u8
        } else {
            (opacity * 255.0) as u8
        };
    }
    let handle = images.add(rgba_image(f.width, f.height, &rgba));
    book.blends.insert(key, handle.clone());
    Some(handle)
}

/// A shadow as `G_GameObject::Priv_DrawShadow` (VA 0x424c40) draws it.
struct Shadow {
    /// Render layer: 6 for airborne objects, 4 or 2 for ground ones.
    layer: u32,
    /// Offset from the object, in screen pixels (+y down).
    offset: Vec2,
    /// Scale relative to the object: airborne shadows are half size.
    scale: f32,
}

fn shadow_for(assets: &GameAssets, layer_id: ResId, air: bool, obj_scale: f32) -> Shadow {
    use crate::game::sim::pf;
    let airborne = match &layer_id.to_le_bytes() {
        b"grhi" | b"grou" => false,
        b"hud " | b"plwe" | b"plef" | b"plsh" | b"aihi" | b"plui" | b"ailo" | b"atmo"
        | b"play" => true,
        _ => air,
    };
    if airborne {
        // Shadow_XOffset/YOffset (pf 0x30/0x31) at the fixed 0.5 factor.
        Shadow {
            layer: 6,
            offset: Vec2::new((pf(assets, 0x30) * 0.5).round(), (pf(assets, 0x31) * 0.5).round()),
            scale: 0.5,
        }
    } else {
        // Shadow_GroundXOffset/YOffset (pf 0x32/0x33) times the object's scale.
        Shadow {
            layer: if &layer_id.to_le_bytes() == b"grhi" { 4 } else { 2 },
            offset: Vec2::new(
                (pf(assets, 0x32) * obj_scale).round(),
                (pf(assets, 0x33) * obj_scale).round(),
            ),
            scale: 1.0,
        }
    }
}

/// Blend amount of a shadow: 20 of 32, or more for a faded object.
const SHADOW_BLEND: u32 = 20;

/// `U_Pixel16_ConvertVisibilityPercentToBlendAmount`: 100% → 0, 0% → 32.
fn visibility_blend(vis: f32) -> u32 {
    ((vis / 100.0 * 32.0 - 32.0).abs() as u32).min(32)
}

pub(crate) fn image16_handle(
    assets: &mut GameAssets,
    images: &mut Assets<Image>,
    book: &mut TextureBook,
    rid: ResId,
) -> Option<Handle<Image>> {
    if let Some(h) = book.images16.get(&rid) {
        return Some(h.clone());
    }
    let (w, h, rgba) = assets.image16(rid)?.clone();
    let img = rgba_image(w, h, &rgba);
    let handle = images.add(img);
    book.images16.insert(rid, handle.clone());
    Some(handle)
}

// ---------------------------------------------------------------------------
// Frame sync
// ---------------------------------------------------------------------------

/// screen coords → bevy world coords (camera at origin, y up).
pub fn to_world(sx: f32, sy: f32) -> Vec3 {
    Vec3::new(sx - SCREEN_W * 0.5, SCREEN_H * 0.5 - sy, 0.0)
}

/// `to_world` at an explicit depth (higher z draws on top).
pub(crate) fn to_world_z(sx: f32, sy: f32, z: f32) -> Vec3 {
    Vec3::new(sx - SCREEN_W * 0.5, SCREEN_H * 0.5 - sy, z)
}

// Draw order. The original sorts sprites into 16 render layers
// (`U_Sprite_RenderLists_Add`); each maps to a z band here.
const Z_TERRAIN: f32 = 0.0;
/// stateDrawToTerrain sprites: painted onto the background, under layer 1.
const Z_TERRAIN_PASS: f32 = 5.0;
/// Motion-blur afterimages: just under the low air layer.
const Z_BLUR: f32 = 65.0;
/// Particles are plotted after the sprite layers, under the HUD layer.
const Z_PARTICLES: f32 = 145.0;
/// Side borders and score bar: cover anything leaving the playfield, the
/// way the original's 416-px game buffer clips it.
const Z_FRAME: f32 = 200.0;
const Z_SCOREBAR: f32 = 210.0;

/// z for one of the original's render layers (0..=15).
fn layer_z(layer: u32) -> f32 {
    layer as f32 * 10.0
}

/// Render layer for a unit's `#drawLayer_ID` (`G_GameObject` draw, layer
/// switch at G_GameObject.cpp:361). "defa"/"none" fall back on whether the
/// unit is airborne.
fn draw_layer(id: ResId, air: bool) -> u32 {
    match &id.to_le_bytes() {
        b"grou" => 3,
        b"grhi" => 5,
        b"ailo" => 7,
        b"aihi" => 8,
        b"plwe" => 9,
        b"play" => 10,
        b"plsh" => 11,
        b"plef" => 12,
        b"plui" => 13,
        b"atmo" => 14,
        b"hud " => 15,
        _ => if air { 7 } else { 3 },
    }
}

/// Sync all sim entities to Bevy sprites (spawn/despawn/update).
pub fn sync_entities(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &SimSprite, &mut Sprite, &mut Transform)>,
) {
    let sim = &sim_res.0;

    // One entry per sprite to draw: (key, part) → image, position, tint.
    struct Draw {
        key: (u32, u8),
        handle: Handle<Image>,
        pos: Vec3,
        scale: f32,
        color: Color,
    }
    let mut draws: Vec<Draw> = Vec::new();
    if flow.state == FlowState::Playing {
        for e in &sim.entities {
            if e.dead || e.activate_in > 0 || e.vis <= 0.0 {
                continue;
            }
            let (rid, frame) = entity_sprite(&*assets, e);
            let sp = e.screen_pos(sim);
            let udef = assets.units.get(&e.unit);
            let state_idx = e.state.min(
                udef.map(|d| d.states.len().saturating_sub(1))
                    .unwrap_or(0),
            );
            let state_def = udef.and_then(|d| d.states.get(state_idx));
            let tint_color = state_def.map(|s| s.tint_color).unwrap_or(0);
            let terrain = state_def.map(|s| s.draw_to_terrain).unwrap_or(false);
            let colorise = state_def.map(|s| s.do_colorise).unwrap_or(false);
            let layer = udef
                .map(|d| draw_layer(d.draw_layer, !d.is_ground_based))
                .unwrap_or(7);
            // Within a layer, later-spawned entities draw on top.
            let order = (e.unique_num % 4096) as f32 / 4096.0 * 4.0;
            let z = if terrain {
                // stateDrawToTerrain: painted onto the background.
                Z_TERRAIN_PASS + order
            } else {
                layer_z(layer) + order
            };

            // G_GameObject::Priv_Draw draws up to three sprites with the
            // same frame: the base (hidden by stateDoColorise), a flat
            // overlay in the state's tint colour, then the glow overlay.
            let vis_blend = visibility_blend(e.vis);
            // castsShadows: the sprite's shape in black on a shadow layer.
            if !terrain && udef.map(|d| d.casts_shadows).unwrap_or(false) {
                let d = udef.unwrap();
                let sh = shadow_for(&assets, d.draw_layer, !d.is_ground_based, e.scale);
                if let Some(handle) = frame_image_blend(
                    &mut *assets, &mut *images, &mut *book, rid, frame,
                    vis_blend.max(SHADOW_BLEND), Blit::Shadow,
                ) {
                    draws.push(Draw {
                        key: (e.unique_num, 3),
                        handle,
                        pos: to_world_z(sp.x + sh.offset.x, sp.y + sh.offset.y, layer_z(sh.layer) + order),
                        scale: e.scale * sh.scale,
                        color: Color::WHITE,
                    });
                }
            }
            let mut parts: Vec<(u8, u32, Blit, Color)> = Vec::new();
            if !colorise {
                parts.push((0, vis_blend, Blit::Sprite, Color::WHITE));
            }
            if e.tint > 0.0 {
                let t = (e.tint / 100.0) * 32.0 * (1.0 - vis_blend as f32 / 32.0);
                let blend = ((t - 32.0).abs() as u32).min(32);
                let c = Color::srgb_u8(
                    (tint_color >> 16) as u8,
                    (tint_color >> 8) as u8,
                    tint_color as u8,
                );
                parts.push((1, blend, Blit::Flat, c));
            }
            if e.glow.active {
                // Hit glows are white (Glow_Start is passed 0x7fff).
                parts.push((2, (e.glow.timer.clamp(0, 32)) as u32, Blit::Flat, Color::WHITE));
            }
            for (part, blend, blit, color) in parts {
                if blend >= 32 {
                    continue;
                }
                let Some(handle) = frame_image_blend(
                    &mut *assets, &mut *images, &mut *book, rid, frame, blend, blit,
                ) else {
                    continue;
                };
                draws.push(Draw {
                    key: (e.unique_num, part),
                    handle,
                    pos: to_world_z(sp.x, sp.y, z + part as f32 * 0.0003),
                    scale: e.scale,
                    color,
                });
            }
        }
    }

    // Reconcile with the existing sprites.
    let mut existing: HashMap<(u32, u8), Entity> =
        q.iter().map(|(be, ss, _, _)| ((ss.key, ss.part), be)).collect();
    for d in draws {
        let scl = Vec3::splat(d.scale);
        match existing.remove(&d.key).and_then(|be| q.get_mut(be).ok()) {
            Some((_, _, mut sprite, mut transform)) => {
                sprite.image = d.handle;
                sprite.color = d.color;
                transform.translation = d.pos;
                transform.scale = scl;
            }
            None => {
                let mut sprite = Sprite::from_image(d.handle);
                sprite.color = d.color;
                commands.spawn((
                    sprite,
                    Transform::from_translation(d.pos).with_scale(scl),
                    SimSprite { key: d.key.0, part: d.key.1 },
                ));
            }
        }
    }
    for (_, be) in existing {
        commands.entity(be).despawn();
    }
}

/// Pick the sprite rid + frame index for an entity's current state.
fn entity_sprite(assets: &GameAssets, e: &SimEntity) -> (ResId, i64) {
    if let Some(def) = assets.units.get(&e.unit) {
        if !def.states.is_empty() {
            let st = &def.states[e.state.min(def.states.len() - 1)];
            if st.sprite_face != crate::pak::NONE_ID {
                return (st.sprite_face, e.frame);
            }
        }
        // Fallback: editor preview sprite.
        if def.editor_preview_sprite_face != crate::pak::NONE_ID {
            return (def.editor_preview_sprite_face, e.frame);
        }
    }
    (e.unit, e.frame)
}

/// Sync terrain background.
pub fn sync_terrain(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &TerrainSprite, &mut Sprite, &mut Transform), Without<PlayerSprite>>,
) {
    if flow.state != FlowState::Playing {
        for (e, _, _, _) in q.iter() {
            commands.entity(e).despawn();
        }
        return;
    }
    let sim = &sim_res.0;
    let rid = sim.level.background_image;
    let Some(handle) = image16_handle(&mut *assets, &mut *images, &mut *book, rid) else {
        return;
    };
    // Visible slice (`G_Bgnd_CopyToFrontBuffer`): image rows [top, top+480]
    // where top = levelH - 480 - scroll, columns [cam_x+32, cam_x+32+416],
    // drawn into the playfield at screen x 32..448.
    let level_h = sim.level_height();
    let img_top = (level_h - SCREEN_H - sim.scroll).max(0.0).floor();
    let play_w = PLAY_RIGHT - PLAY_LEFT;
    let img_left = (sim.cam_x + 32.0).max(0.0).floor();
    let rect = bevy::math::Rect::new(img_left, img_top, img_left + play_w, img_top + SCREEN_H);
    let center = to_world_z(PLAY_LEFT + play_w * 0.5, SCREEN_H * 0.5, Z_TERRAIN);
    if q.is_empty() {
        commands.spawn((
            Sprite {
                image: handle,
                rect: Some(rect),
                ..default()
            },
            Transform::from_translation(center),
            TerrainSprite,
        ));
    } else {
        for (_, _, mut sprite, mut transform) in q.iter_mut() {
            sprite.image = handle.clone();
            sprite.rect = Some(rect);
            transform.translation = center;
        }
    }
}

/// Marks the side borders and score-bar panel framing the playfield.
#[derive(Component)]
pub struct FrameSprite;

/// Frame around the playfield while playing: 32-px black borders at both
/// screen edges (`U_Display::DrawBlackBorders`) and the 160x480 score-bar
/// panel image `scor` right of the playfield (`G_ScoreBar_ResetAtLevelStart`).
pub fn sync_frame(
    mut commands: Commands,
    flow: Res<Flow>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    q: Query<Entity, With<FrameSprite>>,
) {
    let playing = flow.state == FlowState::Playing;
    if !playing {
        for e in q.iter() {
            commands.entity(e).despawn();
        }
        return;
    }
    if !q.is_empty() {
        return;
    }
    let border_w = PLAY_LEFT;
    for cx in [border_w * 0.5, SCREEN_W - border_w * 0.5] {
        commands.spawn((
            Sprite::from_color(Color::BLACK, Vec2::new(border_w, SCREEN_H)),
            Transform::from_translation(to_world_z(cx, SCREEN_H * 0.5, Z_FRAME)),
            FrameSprite,
        ));
    }
    if let Some(h) = image16_handle(&mut *assets, &mut *images, &mut *book, crate::pak::id("scor")) {
        let bar_w = SCREEN_W - border_w - SCOREBAR_X;
        commands.spawn((
            Sprite::from_image(h),
            Transform::from_translation(to_world_z(SCOREBAR_X + bar_w * 0.5, SCREEN_H * 0.5, Z_FRAME)),
            FrameSprite,
        ));
    }
}

/// Sync player ships.
pub fn sync_players(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &PlayerSprite, &mut Sprite, &mut Transform)>,
) {
    let sim = &sim_res.0;
    // (slot, is shadow) → image, position, scale, colour.
    let mut draws: Vec<((usize, bool), Handle<Image>, Vec3, f32, Color)> = Vec::new();
    if flow.state == FlowState::Playing {
        for slot in 0..2 {
            let p = match sim.players[slot].as_ref() {
                Some(p) if p.phase != PlayerPhase::Dead => p,
                _ => continue,
            };
            // Ship sprite = current air weapon's appearance face for this player.
            let wdef = assets.weapons.get(&p.air_weapon);
            let face = wdef.map(|w| {
                if slot == 0 { w.player1_appearance_face } else { w.player2_appearance_face }
            }).unwrap_or_else(|| crate::pak::id("pl1o"));
            let frame_idx = p.tilt_frame as i64;
            // The ship is on the "play" layer, so its shadow is an airborne one.
            let sh = shadow_for(&assets, crate::pak::id("play"), true, 1.0);
            if let Some(handle) = frame_image_blend(
                &mut *assets, &mut *images, &mut *book, face, frame_idx, SHADOW_BLEND, Blit::Shadow,
            ) {
                let pos = to_world_z(
                    p.screen.x + sh.offset.x,
                    p.screen.y + sh.offset.y,
                    layer_z(sh.layer) + 4.5 + slot as f32 * 0.1,
                );
                draws.push(((slot, true), handle, pos, sh.scale, Color::WHITE));
            }
            let Some(handle) = frame_image(&mut *assets, &mut *images, &mut *book, face, frame_idx) else {
                continue;
            };
            let wpos = to_world_z(p.screen.x, p.screen.y, layer_z(10) + slot as f32);
            // Blink while invulnerable. Pickup glow flash
            // (G_Player::Glow_Start) is a brief overbright tint while
            // glow_ticks counts down; the blink takes priority.
            let blink = p.invuln > 0 && (sim.game_time / 4) % 2 == 0;
            let color = if blink {
                Color::srgba(1.0, 1.0, 1.0, 0.4)
            } else if p.glow_ticks > 0 {
                Color::srgba(2.5, 2.5, 2.5, 1.0)
            } else {
                Color::WHITE
            };
            draws.push(((slot, false), handle, wpos, 1.0, color));
        }
    }
    let mut existing: HashMap<(usize, bool), Entity> =
        q.iter().map(|(be, ps, _, _)| ((ps.slot, ps.shadow), be)).collect();
    for (key, handle, pos, scale, color) in draws {
        let scl = Vec3::splat(scale);
        match existing.remove(&key).and_then(|be| q.get_mut(be).ok()) {
            Some((_, _, mut sprite, mut transform)) => {
                sprite.image = handle;
                sprite.color = color;
                transform.translation = pos;
                transform.scale = scl;
            }
            None => {
                let mut sprite = Sprite::from_image(handle);
                sprite.color = color;
                commands.spawn((
                    sprite,
                    Transform::from_translation(pos).with_scale(scl),
                    PlayerSprite { slot: key.0, shadow: key.1 },
                ));
            }
        }
    }
    // Despawn dead/absent players.
    for (_, be) in existing {
        commands.entity(be).despawn();
    }
}

// ---------------------------------------------------------------------------
// App wiring
// ---------------------------------------------------------------------------

use crate::game::sim::{PlayerInput, SlotInput};
use bevy::window::{Window, WindowPlugin, WindowResolution};
use crate::game::input::KeyBindings;
use crate::game::menu::{MenuPointer, SectorHit, TitleItem, TITLE_ITEMS};

/// Build the input state from keyboard state.
fn gather_input(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<crate::game::input::KeyBindings>,
    mut fi: ResMut<FrameInput>,
) {
    use crate::game::input::{BOMB, CYCLE, DOWN, FIRE, LEFT, RIGHT, UP};
    let slot = |k: &[KeyCode; 7]| SlotInput {
        up: keys.pressed(k[UP]),
        down: keys.pressed(k[DOWN]),
        left: keys.pressed(k[LEFT]),
        right: keys.pressed(k[RIGHT]),
        fire: keys.pressed(k[FIRE]),
        bomb: keys.pressed(k[BOMB]),
        cycle: keys.just_pressed(k[CYCLE]),
    };
    fi.p1 = slot(&bindings.keys[0]);
    fi.p2 = slot(&bindings.keys[1]);
}

fn sim_system(
    time: Res<Time>,
    fi: Res<FrameInput>,
    flow: Res<Flow>,
    mut sim: ResMut<GameSim>,
    mut demo: ResMut<Demo>,
    assets: Res<GameAssets>,
    shot: Option<Res<ShotPlan>>,
) {
    if flow.state != FlowState::Playing || flow.paused {
        return;
    }
    let input = if let Some(pb) = &demo.playback {
        // Attract playback: feed recorded inputs at the frame cursor.
        let f = demo.frame;
        PlayerInput {
            p1: pb.p1.get(f).map(|b| DemoRec::unpack(*b)).unwrap_or_default(),
            p2: pb.p2.get(f).map(|b| DemoRec::unpack(*b)).unwrap_or_default(),
        }
    } else {
        PlayerInput { p1: fi.p1, p2: fi.p2 }
    };
    // Record real (non-playback) input, one byte per player per frame.
    if demo.playback.is_none() {
        if demo.rec.p1.len() < DEMO_MAX_FRAMES {
            demo.rec.p1.push(DemoRec::pack(&fi.p1));
            demo.rec.p2.push(DemoRec::pack(&fi.p2));
        }
    } else {
        demo.frame += 1;
    }
    // Screenshot mode steps exactly one tick per rendered frame so a given
    // frame number always shows the same sim state.
    let dt = if shot.is_some() {
        1.0 / crate::game::sim::TICK_HZ
    } else {
        time.delta_secs()
    };
    sim.0.advance(&*assets, &input, dt);
}

/// Menu/flow state machine. Drives title → sectors → briefing → play →
/// cleared/game-over → credits using the retail `lgsp`/`cred` strings.
pub fn flow_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut flow: ResMut<Flow>,
    mut sim: ResMut<GameSim>,
    mut assets: ResMut<GameAssets>,
    mut scores: ResMut<HighScores>,
    mut demo: ResMut<Demo>,
    mut audio: Option<ResMut<crate::game::audio::AudioEngine>>,
    pointer: Res<MenuPointer>,
    mut bindings: ResMut<KeyBindings>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time>,
    shot: Option<Res<ShotPlan>>,
) {
    // Flow timers are in 60 Hz ticks whatever the display's refresh rate;
    // screenshot mode counts one per frame.
    let ticks = if shot.is_some() {
        1
    } else {
        flow.tick_acc += time.delta_secs() * crate::game::sim::TICK_HZ;
        let n = flow.tick_acc.floor().min(8.0);
        flow.tick_acc -= n;
        n as u32
    };
    flow.ticks += ticks;
    // Any key aborts attract playback back to the title.
    if demo.playback.is_some() && flow.state == FlowState::Playing {
        if keys.get_just_pressed().next().is_some() || pointer.clicked {
            demo.playback = None;
            demo.frame = 0;
            flow.state = FlowState::Title;
            flow.ticks = 0;
        }
    }
    // Title idle ~20s: start attract playback. The original plays its
    // pre-recorded demo films here — cycle through them; fall back to the
    // locally-recorded demo only when no film resource loads.
    if flow.state == FlowState::Title
        && demo.playback.is_none()
        && flow.ticks > ATTRACT_IDLE_TICKS
    {
        let mut pb_opt: Option<DemoRec> = None;
        for _ in 0..DEMO_FILMS.len() {
            let rid = DEMO_FILMS[demo.film_idx % DEMO_FILMS.len()];
            demo.film_idx += 1;
            if let Some(film) = assets.film(rid) {
                pb_opt = Some(DemoRec::from_film(&film));
                break;
            }
        }
        let pb = pb_opt.or_else(DemoRec::load);
        if let Some(pb) = pb {
            if let Some(lid) = pb.level {
                if let Some(new_sim) = start_level(&mut *assets, lid, 1, pb.seed) {
                    sim.0 = new_sim;
                    demo.playback = Some(pb);
                    demo.frame = 0;
                    // Sync flow to the recorded level so the post-level
                    // transition advances to the next campaign sector.
                    if let Some(idx) = flow.level_ids.iter().position(|&r| r == lid) {
                        flow.sector = idx;
                        flow.levels_played = idx as u32 + 1;
                        sim.0.levels_played = flow.levels_played;
                    }
                    flow.carry = None;
                    if let Some(audio) = audio.as_deref_mut() {
                        let mr = sim.0.level.music;
                        if mr != crate::pak::NONE_ID {
                            audio.ensure_music(&mut *assets, mr);
                        }
                    }
                    flow.state = FlowState::Playing;
                    flow.ticks = 0;
                    flow.game_over_at = None;
                }
            }
        }
        // Nothing to play: don't retry every frame.
        if flow.state == FlowState::Title {
            flow.ticks = 0;
        }
    }
    // Caps Lock toggles pause during play (documented default).
    if keys.just_pressed(KeyCode::CapsLock) && flow.state == FlowState::Playing {
        flow.paused = !flow.paused;
    }
    // F3/F4 volume down/up (documented defaults), any flow state.
    if keys.just_pressed(KeyCode::F3) {
        if let Some(a) = audio.as_deref_mut() {
            a.adjust_volume(-0.1);
        }
    }
    if keys.just_pressed(KeyCode::F4) {
        if let Some(a) = audio.as_deref_mut() {
            a.adjust_volume(0.1);
        }
    }
    let enter = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    let space = keys.just_pressed(KeyCode::Space);
    let any = enter || space || pointer.clicked;
    let up = keys.just_pressed(KeyCode::ArrowUp);
    let down = keys.just_pressed(KeyCode::ArrowDown);
    let left = keys.just_pressed(KeyCode::ArrowLeft);
    let right = keys.just_pressed(KeyCode::ArrowRight);
    let esc = keys.just_pressed(KeyCode::Escape);
    // Set when a menu choice should play the button sound (perm sound 2).
    let mut click = false;

    match flow.state {
        FlowState::Title => {
            let n = TITLE_ITEMS.len();
            if up {
                flow.menu_sel = (flow.menu_sel + n - 1) % n;
                flow.ticks = 0;
            }
            if down {
                flow.menu_sel = (flow.menu_sel + 1) % n;
                flow.ticks = 0;
            }
            let hover = pointer.pos.and_then(|pos| crate::game::menu::title_hit(&mut assets, pos));
            if pointer.moved {
                if let Some(i) = hover {
                    flow.menu_sel = i;
                }
            }
            let mut chosen = None;
            if enter {
                chosen = Some(TITLE_ITEMS[flow.menu_sel].1);
            }
            if pointer.clicked {
                if let Some(i) = hover {
                    flow.menu_sel = i;
                    chosen = Some(TITLE_ITEMS[i].1);
                }
            }
            // The original's keyboard shortcuts (G_Interface, key handler
            // at VA 0x426d20): 1/N, 2, S/H, D, Q. C is ours, for Controls.
            for (key, item) in [
                (KeyCode::Digit1, TitleItem::OnePlayer),
                (KeyCode::KeyN, TitleItem::OnePlayer),
                (KeyCode::Digit2, TitleItem::TwoPlayers),
                (KeyCode::KeyC, TitleItem::Controls),
                (KeyCode::KeyS, TitleItem::Scores),
                (KeyCode::KeyH, TitleItem::Scores),
                (KeyCode::KeyD, TitleItem::Demos),
                (KeyCode::KeyQ, TitleItem::Quit),
            ] {
                if keys.just_pressed(key) {
                    chosen = Some(item);
                }
            }
            if let Some(item) = chosen {
                click = true;
                if let Some(i) = TITLE_ITEMS.iter().position(|t| t.1 == item) {
                    flow.menu_sel = i;
                }
                flow.ticks = 0;
                match item {
                    TitleItem::OnePlayer | TitleItem::TwoPlayers => {
                        flow.players = if item == TitleItem::OnePlayer { 1 } else { 2 };
                        // Open on the furthest sector reached.
                        flow.sector = (flow.progress as usize).min(flow.level_ids.len().saturating_sub(1));
                        flow.state = FlowState::Sectors;
                    }
                    TitleItem::Controls => {
                        flow.controls_sel = (0, 0);
                        flow.rebinding = false;
                        flow.state = FlowState::Controls;
                    }
                    TitleItem::Scores => {
                        flow.score_hilite = None;
                        flow.state = FlowState::Scores;
                    }
                    // Run the idle timer out: attract mode starts next frame.
                    TitleItem::Demos => flow.ticks = ATTRACT_IDLE_TICKS + 1,
                    TitleItem::Quit => {
                        exit.write(AppExit::Success);
                    }
                }
            }
        }
        FlowState::Sectors => {
            // Every sector can be looked at; only those reached can be
            // started (sequential campaign).
            let n = flow.level_ids.len();
            let hit = pointer
                .pos
                .filter(|_| pointer.clicked)
                .and_then(|pos| crate::game::menu::sector_hit(&assets, pos));
            if (left || up || hit == Some(SectorHit::Previous)) && flow.sector > 0 {
                flow.sector -= 1;
                click = true;
            }
            if (right || down || hit == Some(SectorHit::Next)) && flow.sector + 1 < n {
                flow.sector += 1;
                click = true;
            }
            // "Press any button to start": either fire key works too.
            let fire = [crate::game::input::FIRE, crate::game::input::BOMB]
                .iter()
                .any(|&a| keys.just_pressed(bindings.keys[0][a]));
            if enter || space || fire || hit == Some(SectorHit::Current) {
                if flow.sector as u32 <= flow.progress {
                    // New run: drop any carried state from a previous run.
                    flow.carry = None;
                    flow.levels_played = 0;
                    flow.state = FlowState::Briefing;
                    flow.ticks = 0;
                    click = true;
                }
            } else if esc {
                flow.state = FlowState::Title;
                flow.ticks = 0;
            }
        }
        FlowState::Briefing => {
            if flow.ticks > BRIEFING_TICKS || any {
                let lid = flow.level_ids.get(flow.sector).copied();
                if let Some(lid) = lid {
                    if let Some(new_sim) =
                        start_level(&mut *assets, lid, flow.players, 0xC0FFEE)
                    {
                        sim.0 = new_sim;
                        flow.levels_played += 1;
                        sim.0.levels_played = flow.levels_played;
                        // Carry run-scoped state (score/lives/perfect count/
                        // accuracy reward) from the previous level.
                        if let Some(carry) = flow.carry.take() {
                            carry.apply(&mut sim.0);
                        }
                        if let Some(audio) = audio.as_deref_mut() {
                            let mr = sim.0.level.music;
                            if mr != crate::pak::NONE_ID {
                                audio.ensure_music(&mut *assets, mr);
                            }
                        }
                        // Start a fresh input recording for this run.
                        demo.rec = DemoRec {
                            level: Some(lid),
                            seed: 0xC0FFEE,
                            p1: Vec::new(),
                            p2: Vec::new(),
                        };
                        demo.playback = None;
                        demo.frame = 0;
                        // Starting above sector 1 restricts the run: one
                        // life, no hi-score, no campaign progress credit.
                        flow.restricted = flow.sector > 0;
                        if flow.restricted {
                            for p in sim.0.players.iter_mut().flatten() {
                                p.lives = 1;
                            }
                        }
                        flow.state = FlowState::Playing;
                        flow.ticks = 0;
                        flow.paused = false;
                        flow.game_over_at = None;
                    }
                }
            }
        }
        FlowState::Playing => {
            // Esc exits to title (original quits the app; we keep the run's
            // demo recording instead of losing it).
            if esc && demo.playback.is_none() {
                if !demo.rec.p1.is_empty() {
                    demo.rec.save();
                }
                demo.rec = DemoRec::default();
                flow.state = FlowState::Title;
                flow.ticks = 0;
                return;
            }
            let in_playback = demo.playback.is_some();
            // Attract playback ends on: recorded frames exhausted, the demo
            // level completing, or every player dead — then return to the
            // title. Returning early keeps the level-end/game-over flow
            // below from overriding the transition (and skips it entirely
            // while a demo is running).
            if in_playback {
                let frames_done = demo.frame >= demo.playback.as_ref().unwrap().frames();
                let level_done = sim.0.level_end_complete;
                let any_alive = sim
                    .0
                    .players
                    .iter()
                    .flatten()
                    .any(|p| p.phase != PlayerPhase::Dead);
                let any_slot = sim.0.players.iter().any(|p| p.is_some());
                if frames_done || level_done || (any_slot && !any_alive) {
                    demo.playback = None;
                    demo.frame = 0;
                    flow.state = FlowState::Title;
                    flow.ticks = 0;
                    return;
                }
            }
            if !in_playback && sim.0.phase == crate::game::sim::Phase::LevelEnd {
                // The original's level-end sequence (bonus breakdown +
                // coin tally) plays out timed inside the sim; advance only
                // once it fully completes.
                if sim.0.level_end_complete {
                    // Campaign progress only counts for unrestricted runs
                    // that completed levels sequentially from sector 1.
                    if !flow.restricted
                        && demo.playback.is_none()
                        && flow.sector as u32 == flow.progress
                    {
                        flow.progress += 1;
                        save_progress(flow.progress);
                    }
                    // Stash run-scoped state for the next level.
                    flow.carry = Some(crate::game::sim::RunCarry::capture(&sim.0));
                    if flow.sector + 1 < flow.level_ids.len() {
                        flow.sector += 1;
                        flow.state = FlowState::Briefing;
                    } else {
                        flow.state = FlowState::Credits;
                    }
                    flow.ticks = 0;
                }
            } else if !in_playback {
                let any_alive = sim
                    .0
                    .players
                    .iter()
                    .flatten()
                    .any(|p| p.phase != PlayerPhase::Dead);
                let any_slot = sim.0.players.iter().any(|p| p.is_some());
                if any_slot && !any_alive {
                    match flow.game_over_at {
                        None => {
                            flow.game_over_at = Some(flow.ticks);
                            // Notice_GameOver object (perm 0x18), once.
                            if let Some(rid) = assets.object_ids.entries.get(0x18).map(|e| e.1) {
                                if rid != crate::pak::NONE_ID {
                                    let pos = Vec2::new(sim.0.cam_x + 240.0, sim.0.scroll + 240.0);
                                    sim.0.request_spawn_screen(&assets, SpawnReq {
                                        unit: rid,
                                        pos,
                                        heading: None,
                                        heading_forced: false,
                                        owner: usize::MAX,
                                        spawner_player: -1,
                                        stationary: true,
                                        state: None,
                                    }, Vec2::new(crate::game::sim::PLAY_CENTER_X, 56.0));
                                }
                            }
                        }
                        // Let the notice play, then go to the score table.
                        Some(at) if enter || flow.ticks > at + GAME_OVER_TICKS => {
                            flow.game_over_at = None;
                            // Queue hi-score entries for qualifying players,
                            // best first. Restricted runs (started above
                            // sector 1) never count.
                            let mut results: Vec<PendingScore> = if flow.restricted {
                                Vec::new()
                            } else {
                                sim.0
                                    .players
                                    .iter()
                                    .flatten()
                                    .filter(|p| p.score > 0)
                                    .map(|p| PendingScore {
                                        score: p.score,
                                        level: sim.0.level.name.clone(),
                                    })
                                    .collect()
                            };
                            results.sort_by(|a, b| b.score.cmp(&a.score));
                            flow.pending_scores = results
                                .into_iter()
                                .filter(|ps| scores.qualifies(ps.score))
                                .collect();
                            flow.name_text.clear();
                            flow.score_hilite = None;
                            flow.state = if flow.pending_scores.is_empty() {
                                FlowState::Scores
                            } else {
                                FlowState::NameEntry
                            };
                            // Persist the recorded run as the attract demo
                            // (G_Film_Save "last").
                            if !demo.rec.p1.is_empty() {
                                demo.rec.save();
                            }
                            flow.ticks = 0;
                        }
                        Some(_) => {}
                    }
                }
            }
        }
        FlowState::NameEntry => {
            // Type a name on the score table; Enter confirms it.
            let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            for key in keys.get_just_pressed() {
                if *key == KeyCode::Backspace {
                    flow.name_text.pop();
                } else if let Some(c) = typed_char(*key, shift) {
                    if flow.name_text.chars().count() < NAME_MAX_CHARS {
                        flow.name_text.push(c);
                    }
                }
            }
            if enter {
                if let Some(ps) = flow.pending_scores.first().cloned() {
                    let typed = flow.name_text.trim();
                    let name = if typed.is_empty() { "Player" } else { typed }.to_string();
                    flow.score_hilite = Some(scores.insert(name, ps.score, ps.level));
                    flow.pending_scores.remove(0);
                    click = true;
                }
                flow.name_text.clear();
                if flow.pending_scores.is_empty() {
                    flow.state = FlowState::Scores;
                }
                flow.ticks = 0;
            }
        }
        FlowState::Scores => {
            // Scores_Duration (pf 0x4e), or any key.
            if any || esc || flow.ticks > crate::game::sim::pf(&assets, 0x4e) as u32 {
                flow.state = FlowState::Title;
                flow.ticks = 0;
            }
        }
        FlowState::Credits => {
            let pages = crate::game::menu::credits_pages(&assets);
            let page_ticks = pages.get(flow.credits_page).map(|p| p.ticks).unwrap_or(0);
            if esc {
                flow.credits_page = pages.len();
            } else if any || flow.ticks > page_ticks {
                flow.credits_page += 1;
                flow.ticks = 0;
            }
            if flow.credits_page >= pages.len() {
                flow.credits_page = 0;
                flow.state = FlowState::Title;
                flow.ticks = 0;
            }
        }
        FlowState::Controls => {
            let (mut row, mut col) = flow.controls_sel;
            if flow.rebinding {
                if esc {
                    flow.rebinding = false;
                } else if let Some(key) = keys
                    .get_just_pressed()
                    .copied()
                    .find(|k| crate::game::input::is_bindable(*k))
                {
                    bindings.bind(col, row, key);
                    bindings.save();
                    flow.rebinding = false;
                    click = true;
                }
            } else {
                let rows = crate::game::menu::CONTROLS_ROWS;
                if up {
                    row = (row + rows - 1) % rows;
                }
                if down {
                    row = (row + 1) % rows;
                }
                if left || right {
                    col = 1 - col;
                }
                let hover = pointer.pos.and_then(crate::game::menu::controls_hit);
                if pointer.moved || pointer.clicked {
                    if let Some(cell) = hover {
                        (row, col) = cell;
                    }
                }
                flow.controls_sel = (row, col);
                if enter || (pointer.clicked && hover.is_some()) {
                    flow.rebinding = true;
                    click = true;
                } else if esc {
                    flow.state = FlowState::Title;
                    flow.ticks = 0;
                }
            }
        }
    }

    if click {
        if let Some(audio) = audio.as_deref_mut() {
            let rid = assets.sound_ids.entries.get(2).map(|(_, r)| *r);
            if let Some(sound) = rid.and_then(|r| assets.sound(r)) {
                audio.play_sfx(&crate::assets::Sound {
                    samples: sound.samples.clone(),
                    channels: sound.channels,
                    sample_rate: sound.sample_rate,
                });
            }
        }
    }
}

/// Idle ticks on the title screen before attract mode starts.
const ATTRACT_IDLE_TICKS: u32 = 60 * 20;
/// Ticks the briefing stays up without a key press.
const BRIEFING_TICKS: u32 = 180;
/// Ticks the game-over notice plays before the score table.
const GAME_OVER_TICKS: u32 = 240;
/// Longest name the score table accepts.
const NAME_MAX_CHARS: usize = 16;

/// The character a key types into the name field, if it types one.
fn typed_char(key: KeyCode, shift: bool) -> Option<char> {
    let name = format!("{:?}", key);
    if let Some(letter) = name.strip_prefix("Key").filter(|l| l.len() == 1) {
        let c = letter.chars().next()?;
        return Some(if shift { c } else { c.to_ascii_lowercase() });
    }
    if let Some(digit) = name.strip_prefix("Digit").filter(|d| d.len() == 1) {
        return digit.chars().next();
    }
    match key {
        KeyCode::Space => Some(' '),
        KeyCode::Minus => Some('-'),
        KeyCode::Period => Some('.'),
        _ => None,
    }
}

/// Mission order recovered from the retail exe's `g_demoLevelNames`
/// table: 12×64-byte slots of `#indentifier_STR` values, nibble-swap +
/// XOR-0xFF encrypted (the same `U_Utils_String_Encrypt` cipher used for
/// pak strings). Slot order is campaign order — Lucena (Mariner
/// Valley) is sector 1, matching the player guide.
const MISSION_ORDER: [&str; 12] = [
    "Lucena", "Yippe", "Vista", "Swoop", "Conrad", "Delos", "Sparta",
    "Saratoga", "Hannibal", "Leonidas", "Thebes", "Yamato",
];

/// Campaign position (0-based) of a level rid; unknown ids sort last.
fn mission_rank(assets: &GameAssets, rid: ResId) -> usize {
    assets
        .levels
        .get(&rid)
        .and_then(|l| MISSION_ORDER.iter().position(|id| l.identifier == *id))
        .unwrap_or(usize::MAX)
}

fn mission_sorted_level_ids(assets: &GameAssets) -> Vec<ResId> {
    let mut ids: Vec<ResId> = assets.levels.keys().copied().collect();
    ids.sort_by_key(|&r| mission_rank(assets, r));
    ids
}

/// Start a level: build Sim + spawn players.
pub fn start_level(
    assets: &mut GameAssets,
    level_id: ResId,
    players: usize,
    seed: u64,
) -> Option<Sim> {
    // Warm per-frame sprite dimensions for the sim's collision bounds
    // (entity/player half-extents = current frame's half height/width).
    let mut rids: Vec<ResId> = Vec::new();
    for (rid, u) in &assets.units {
        rids.push(*rid);
        rids.push(u.editor_preview_sprite_face);
        for st in &u.states {
            rids.push(st.sprite_face);
        }
    }
    for w in assets.weapons.values() {
        rids.push(w.player1_appearance_face);
        rids.push(w.player2_appearance_face);
    }
    for rid in rids {
        if rid != crate::pak::NONE_ID {
            assets.warm_sprite_dims(rid);
        }
    }
    let level = assets.levels.get(&level_id)?.clone();
    let mut sim = Sim::new(level, level_id, seed);
    // Media mask + g_bgndTileSize (bg width / mask width, int division).
    sim.media_mask = assets.media_mask(sim.level.media_mask).cloned();
    if let Some(m) = &sim.media_mask {
        let bg_w = (sim.level.background[2] - sim.level.background[0]).max(1) as i64;
        sim.media_tile_size = bg_w / m.w as i64;
    }
    // 1-based mission number for weapon level-availability filtering.
    let level_ids = mission_sorted_level_ids(assets);
    sim.level_number = level_ids.iter().position(|&r| r == level_id).map(|i| i as i64 + 1).unwrap_or(1);
    sim.phase = crate::game::sim::Phase::Playing;
    for slot in 0..players.min(2) {
        sim.add_player(assets, slot, players > 1);
    }
    // Level-start notice banner: perm object levelNum + 9
    // (Notice_Level_01..12), spawned screen-centered like sub_41fc80.
    // Deferred so the initial level objects spawn first — the original
    // runs G_Bgnd_DoInitialMapSpawns before this RequestSpawn.
    let nidx = sim.level_number as usize + 9;
    if let Some(rid) = assets.object_ids.entries.get(nidx).map(|e| e.1) {
        if rid != crate::pak::NONE_ID {
            let pos = Vec2::new(sim.cam_x + 240.0, sim.scroll + 240.0);
            sim.pending_notice = Some((rid, pos, Vec2::new(crate::game::sim::PLAY_CENTER_X, 56.0)));
        }
    }
    Some(sim)
}

/// Run the game. `assets` must already be opened + `load_all_data()` called.
/// Screenshot mode (`--shot`): run deterministically, save PNGs of chosen
/// frames, then exit. Lets a change be checked by looking at its output.
pub struct ShotConfig {
    /// Directory the `frameNNNNN.png` files are written to.
    pub out_dir: std::path::PathBuf,
    /// Frame numbers to capture (one sim tick per frame while playing).
    pub frames: Vec<u32>,
    /// Retail film to replay.
    pub film: Option<ResId>,
    /// Input script to replay; with neither the ship sits still tapping fire.
    pub script: Option<DemoRec>,
    /// Capture a menu screen instead of gameplay: `title`, `sectors`,
    /// `briefing`, `scores`, `name`, `credits` or `controls`.
    pub screen: Option<String>,
    /// Sector (0-based) shown on the sector and briefing screens.
    pub sector: usize,
}

/// Live screenshot-mode state: frame counter and capture cursor.
#[derive(Resource)]
pub struct ShotPlan {
    out_dir: std::path::PathBuf,
    frames: Vec<u32>,
    next: usize,
    frame: u32,
    last_request: u32,
}

/// Frames to keep running after the last capture so its PNG gets written.
const SHOT_EXIT_DELAY: u32 = 20;

/// Screenshot mode ignores the keyboard, so a stray key press cannot abort
/// the replay being captured.
fn shot_mute_input(mut keys: ResMut<ButtonInput<KeyCode>>) {
    keys.reset_all();
}

fn shot_system(
    mut commands: Commands,
    plan: Option<ResMut<ShotPlan>>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let Some(mut plan) = plan else { return };
    plan.frame += 1;
    if plan.next < plan.frames.len() && plan.frames[plan.next] <= plan.frame {
        let path = plan.out_dir.join(format!("frame{:05}.png", plan.frames[plan.next]));
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        plan.next += 1;
        plan.last_request = plan.frame;
    }
    if plan.next == plan.frames.len() && plan.frame > plan.last_request + SHOT_EXIT_DELAY {
        exit.write(AppExit::Success);
    }
}

pub fn run(assets: GameAssets, level_id: ResId, players: usize, shot: Option<ShotConfig>) {
    let mut assets = assets;
    let mut level_ids = mission_sorted_level_ids(&assets);
    if level_ids.is_empty() {
        level_ids.push(level_id);
    }
    let mut sim = start_level(&mut assets, level_id, players, 0xC0FFEE)
        .expect("level not found");
    let mut demo = Demo::default();
    let mut initial_state = FlowState::Title;
    let mut shot = shot;
    if let Some(cfg) = shot.as_mut().filter(|c| c.screen.is_none()) {
        let last = cfg.frames.iter().copied().max().unwrap_or(0) as usize;
        let pb = match (cfg.script.take(), cfg.film) {
            (Some(mut script), _) => {
                // Keep playing (idle) until the last capture is written.
                let len = last + SHOT_EXIT_DELAY as usize + 60;
                if script.p1.len() < len {
                    script.p1.resize(len, 0);
                }
                script
            }
            (None, Some(rid)) => DemoRec::from_film(&assets.film(rid).expect("film not found")),
            (None, None) => {
                // Tap fire: holding it charges the weapon until it overloads.
                let fire = DemoRec::pack(&SlotInput { fire: true, ..default() });
                let len = last + SHOT_EXIT_DELAY as usize + 60;
                DemoRec {
                    level: Some(level_id),
                    seed: 0xC0FFEE,
                    p1: (0..len).map(|f| if f % 12 < 4 { fire } else { 0 }).collect(),
                    p2: Vec::new(),
                }
            }
        };
        let lid = pb.level.unwrap_or(level_id);
        sim = start_level(&mut assets, lid, players, pb.seed).expect("level not found");
        demo.playback = Some(pb);
        initial_state = FlowState::Playing;
    }

    // Screenshot mode renders 1:1 (no HiDPI scaling) and as fast as it can.
    let window = if shot.is_some() {
        Window {
            resolution: WindowResolution::new(640, 480).with_scale_factor_override(1.0),
            present_mode: bevy::window::PresentMode::AutoNoVsync,
            title: "Deimos Rising".into(),
            ..default()
        }
    } else {
        Window {
            resolution: WindowResolution::new(640, 480),
            title: "Deimos Rising".into(),
            ..default()
        }
    };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(bevy::image::ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(window),
                ..default()
            }),
    );
    let shot_mode = shot.is_some();
    let shot_screen = shot.as_ref().and_then(|c| c.screen.clone());
    let shot_sector = shot.as_ref().map(|c| c.sector).unwrap_or(0);
    if let Some(cfg) = shot {
        std::fs::create_dir_all(&cfg.out_dir).expect("create screenshot dir");
        let mut frames = cfg.frames;
        frames.sort_unstable();
        frames.dedup();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 60.0),
        ))
        .insert_resource(ShotPlan {
            out_dir: cfg.out_dir,
            frames,
            next: 0,
            frame: 0,
            last_request: 0,
        });
    }
    let mut flow = Flow {
        state: initial_state,
        sector: 0,
        players: players.clamp(1, 2),
        ticks: 0,
        tick_acc: 0.0,
        level_ids,
        paused: false,
        pending_scores: Vec::new(),
        name_text: String::new(),
        score_hilite: None,
        menu_sel: 0,
        controls_sel: (0, 0),
        rebinding: false,
        credits_page: 0,
        game_over_at: None,
        restricted: false,
        progress: load_progress(),
        carry: None,
        levels_played: 0,
    };
    // Screenshot mode can open straight on a menu screen.
    match shot_screen.as_deref() {
        Some("sectors") => flow.state = FlowState::Sectors,
        Some("briefing") => flow.state = FlowState::Briefing,
        Some("scores") => flow.state = FlowState::Scores,
        Some("credits") => flow.state = FlowState::Credits,
        Some("controls") => flow.state = FlowState::Controls,
        Some("name") => {
            flow.state = FlowState::NameEntry;
            flow.name_text = "Linus".into();
            flow.pending_scores = vec![PendingScore { score: 23150, level: "Mariner Valley".into() }];
        }
        _ => {}
    }
    if shot_screen.is_some() {
        flow.sector = shot_sector.min(flow.level_ids.len().saturating_sub(1));
    }
    app.insert_resource(assets)
        .insert_resource(GameSim(sim))
        .insert_resource(TextureBook::default())
        .insert_resource(FrameInput {
            p1: SlotInput::default(),
            p2: SlotInput::default(),
            players,
            level_index: 0,
        })
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.05)))
        .insert_resource(flow)
        .insert_resource(HighScores::load())
        .insert_resource(crate::game::input::KeyBindings::load())
        .insert_resource(MenuPointer::default())
        .insert_resource(demo)
        .insert_resource(ScoreBars::default())
        .insert_resource(Particles::default())
        .insert_resource(Blurs::default());
    if !shot_mode {
        if let Some(engine) = crate::game::audio::AudioEngine::new() {
            app.insert_resource(engine);
        }
    }

    if shot_mode {
        app.add_systems(PreUpdate, shot_mute_input.after(bevy::input::InputSystems));
    }
    app.add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                gather_input,
                crate::game::menu::gather_pointer,
                flow_system,
                sim_system,
                crate::game::audio::drain_events,
                crate::game::menu::sync_menu,
                sync_terrain,
                sync_frame,
                sync_particles,
                sync_blurs,
                sync_crosshair,
                sync_weapon_popup,
                sync_entities,
                sync_players,
                sync_scorebar,
                sync_hud_text,
                shot_system,
            )
                .chain(),
        );
    app.run();
}

// ---------------------------------------------------------------------------
// HUD
// ---------------------------------------------------------------------------

// Bitmap text (G_Text port). Every string is drawn from the `tesm` font
// sprite, one frame per character, laid out by a `tefo` text format.

/// Font sprite used for all text (`g_textSpriteIDs[0]`).
pub(crate) const FONT: &str = "tesm";

/// Font frame for a character: the jump table at VA 0x43eb70. Anything
/// outside `!`..=`~`, the space included, gets the blank frame 90.
fn font_frame(c: char) -> i64 {
    match c {
        'A'..='Z' => c as i64 - 'A' as i64,
        'a'..='z' => 26 + c as i64 - 'a' as i64,
        '1'..='9' => 52 + c as i64 - '1' as i64,
        '0' => 61,
        '!'..='/' => 62 + c as i64 - '!' as i64,
        ':'..='@' => 77 + c as i64 - ':' as i64,
        '[' | '{' => 69, // drawn as '('
        ']' | '}' => 70, // drawn as ')'
        '\\' => 84,
        '^' => 85,
        '_' => 86,
        '`' => 87,
        '|' => 88,
        '~' => 89,
        _ => 90,
    }
}

/// One character placed by `layout_text`.
pub(crate) struct Glyph {
    pub(crate) frame: i64,
    /// Sprite centre, in the coordinate space of the format's location.
    pub(crate) center: Vec2,
}

/// Lay a string out the way `G_Text_Priv_0x43e4e0` does. `pos` is the
/// text's top-left for `LEFT`; `RIGH` ends there, `CENT` centres on it,
/// `CEGA` centres in the 416-px game area, `CEBU` in the 640-px screen.
/// Monospaced text gives every character the widest digit's width.
pub(crate) fn layout_text(
    assets: &mut GameAssets,
    text: &str,
    pos: Vec2,
    align: &str,
    monospaced: bool,
    spacing: i32,
) -> Vec<Glyph> {
    let Some(font) = assets.sprite(crate::pak::id(FONT)) else {
        return Vec::new();
    };
    let dims = |c: char| -> (i32, i32) {
        font.frames
            .get(font_frame(c) as usize)
            .map(|f| (f.width as i32, f.height as i32))
            .unwrap_or((0, 0))
    };
    let widest_digit = "1234567890".chars().map(|c| dims(c).0).max().unwrap_or(0);
    let advance = |c: char| if monospaced { widest_digit } else { dims(c).0 };
    let total: i32 = text.chars().map(|c| spacing + advance(c)).sum();
    let mut x = match align {
        "RIGH" => pos.x as i32 - total,
        "CENT" => (pos.x - total as f32 / 2.0) as i32,
        "CEGA" => ((416.0 - total as f32) / 2.0) as i32,
        "CEBU" => ((640.0 - total as f32) / 2.0) as i32,
        _ => pos.x as i32,
    };
    let y = pos.y as i32;
    let mut out = Vec::new();
    for c in text.chars() {
        x += spacing;
        if c != ' ' {
            // sub_43e8c0: the glyph's centre is half its own size in from
            // the pen position, so its top-left lands on the pen.
            let (w, h) = dims(c);
            out.push(Glyph {
                frame: font_frame(c),
                center: Vec2::new((x + (w + 1) / 2) as f32, (y + (h + 1) / 2) as f32),
            });
        }
        x += advance(c);
    }
    out
}

/// Marks one glyph sprite of the score bar's text.
#[derive(Component)]
pub struct HudGlyph {
    key: (usize, u8, usize),
}

/// Draw the score-bar numbers with the game font (`G_ScoreBar_Draw`):
/// each player's score as seven digits (text formats `sbs1`/`sbs2`), and
/// the lives in reserve (`sbl1`/`sbl2`, or `sll1`/`sll2` on the last life).
pub fn sync_hud_text(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &HudGlyph, &mut Sprite, &mut Transform)>,
) {
    let sim = &sim_res.0;
    let mut draws: Vec<((usize, u8, usize), Handle<Image>, Vec3, Color)> = Vec::new();
    if flow.state == FlowState::Playing {
        let max_lives = crate::game::sim::pf(&assets, 0x8f) as i64; // ScoreBar_Lives_MaxNumDisplayed
        for slot in 0..2 {
            let player = sim.players[slot]
                .as_ref()
                .filter(|p| p.phase != PlayerPhase::Dead);
            let in_game = player.is_some();
            let score = player.map(|p| p.score).unwrap_or(0);
            let mut lives = player.map(|p| (p.lives as i64 - 1).max(0)).unwrap_or(0);
            if max_lives > 0 {
                lives = lives.min(max_lives);
            }
            let n = slot + 1;
            let lives_fmt = if in_game && lives == 0 { format!("sll{n}") } else { format!("sbl{n}") };
            let texts = [
                (0u8, format!("sbs{n}"), format!("{:07}", score)),
                (1u8, lives_fmt, format!("{}", lives)),
            ];
            for (kind, fmt_id, text) in texts {
                let Some(fmt) = assets.text_formats.get(&crate::pak::id(&fmt_id)).cloned() else {
                    continue;
                };
                // Locations are in front-buffer space; the screen is
                // PLAY_LEFT further right.
                let pos = Vec2::new(
                    fmt.get_int("Loc_X") as f32 + PLAY_LEFT,
                    fmt.get_int("Loc_Y") as f32,
                );
                let glyphs = layout_text(
                    &mut assets,
                    &text,
                    pos,
                    fmt.get("Format").unwrap_or("LEFT").trim(),
                    fmt.get("Monospaced").map(|v| v.trim() == "TRUE").unwrap_or(false),
                    fmt.get_int("SpaceBetweenChars") as i32,
                );
                // A player who is not in the game has their numbers
                // blended halfway to nothing.
                let mut blend = fmt.get_int("BlendAmount_0To32") as u32;
                if !in_game {
                    blend += (32 - blend) / 2;
                }
                let colorise = fmt.get("Colorise_Do").map(|v| v.trim() == "TRUE").unwrap_or(false);
                let rgb = u32::from_str_radix(fmt.get("ColoriseColor").unwrap_or("ffffff").trim(), 16)
                    .unwrap_or(0xffffff);
                let (blit, color) = if colorise {
                    (Blit::Flat, Color::srgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
                } else {
                    (Blit::Sprite, Color::WHITE)
                };
                for (i, g) in glyphs.iter().enumerate() {
                    let Some(handle) = frame_image_blend(
                        &mut assets, &mut images, &mut book, crate::pak::id(FONT), g.frame, blend.min(32), blit,
                    ) else {
                        continue;
                    };
                    draws.push((
                        (slot, kind, i),
                        handle,
                        to_world_z(g.center.x, g.center.y, Z_SCOREBAR + 2.0),
                        color,
                    ));
                }
            }
        }
    }
    let mut existing: HashMap<(usize, u8, usize), Entity> =
        q.iter().map(|(be, g, _, _)| (g.key, be)).collect();
    for (key, handle, pos, color) in draws {
        match existing.remove(&key).and_then(|be| q.get_mut(be).ok()) {
            Some((_, _, mut sprite, mut transform)) => {
                sprite.image = handle;
                sprite.color = color;
                transform.translation = pos;
            }
            None => {
                let mut sprite = Sprite::from_image(handle);
                sprite.color = color;
                commands.spawn((sprite, Transform::from_translation(pos), HudGlyph { key }));
            }
        }
    }
    for (_, be) in existing {
        commands.entity(be).despawn();
    }
}

/// The level-end breakdown, one line per entry, while the sim runs its
/// end sequence (sub_020d90 states + MoneyCounter tally). Retail strings:
/// "All Mission Targets Destroyed!!!", "Ground Accuracy:", "   Bonus:",
/// "Coin Bonus:", "  x", "  =". Empty outside the level-end phase.
pub(crate) fn end_text_lines(sim: &Sim, assets: &GameAssets) -> Vec<String> {
    if sim.phase != crate::game::sim::Phase::LevelEnd {
        return Vec::new();
    }
    let mut s = String::new();
    match sim.end_seq_state {
        // Banner hold: only the destroyed banner (string 0xa).
        1 => s.push_str("    All Mission Targets Destroyed!!!"),
        // Perfect-campaign sequence: the original shows the banner on
        // state 9 and blanks the buffer on state 10 (format 0x4e4de7
        // is an empty string) — a blinking banner while awarding.
        9 => s.push_str("    All Mission Targets Destroyed!!!"),
        10 => {}
        _ => {
            s.push_str("    All Mission Targets Destroyed!!!\n\n");
            s.push_str(&format!("Ground Accuracy: {}%\n", sim.end_bonus_percent));
            if sim.end_seq_state >= 3 {
                if sim.end_bonus_total > 0 {
                    // During the drain the original counts the bonus
                    // down live; afterwards it shows the full amount.
                    let shown = if sim.end_seq_state == 6 {
                        sim.end_bonus_remaining
                    } else {
                        sim.end_bonus_total
                    };
                    s.push_str(&format!("   Bonus: {}\n", shown));
                } else {
                    s.push_str("   Bonus:None!\n");
                }
            } else {
                s.push_str("   Bonus:\n");
            }
            // Coin Bonus lines appear once the MoneyCounter tallies arm.
            let factor = crate::game::sim::pf(&assets, 0xab) as i64;
            for p in sim.players.iter().flatten() {
                let purse = if p.money_tally.purse > 0 {
                    p.money_tally.purse
                } else {
                    p.money
                };
                if purse <= 0 && p.money_tally.remaining <= 0 {
                    continue;
                }
                // "= N" counts down live during the drain; before the
                // tally arms it shows the full purse x factor total.
                let remaining = if p.money_tally.phase != 0 {
                    p.money_tally.remaining
                } else {
                    purse * factor
                };
                s.push_str(&format!(
                    "Coin Bonus: {}  x  {}  =  {}\n",
                    purse, factor, remaining
                ));
            }
        }
    }
    s.lines().map(|l| l.trim().to_string()).collect()
}

// ---------------------------------------------------------------------------
// Score bar (G_ScoreBar port): 160x480 strip right of the playfield. Perm
// rects from the `reli` Rects file are bar-local; sprite positions are perm
// floats in front-buffer space (bar at x=416), i.e. screen x - PLAY_LEFT.
// ---------------------------------------------------------------------------

/// Left edge of the 160-px score bar on screen: directly right of the
/// playfield (`m_screenScorebarRect.left = innerRect->right`).
const SCOREBAR_X: f32 = PLAY_RIGHT;

/// Animated meter percentages, eased toward the live values like
/// `G_ScoreBar_Process` (perm floats Shield/Power Meter Increase/DecreaseRate).
#[derive(Resource, Default)]
pub struct ScoreBars {
    pub shield: [f32; 2],
    pub power: [f32; 2],
}

/// Which score-bar element a sprite entity represents.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
enum BarElem {
    LifeIcon,
    ShieldTrack,
    ShieldFill,
    PowerTrack,
    PowerFill,
    Weapon(usize),
}

#[derive(Component)]
struct ScoreBarSprite {
    player: usize,
    elem: BarElem,
}

fn bar_rect(assets: &GameAssets, player: usize, name: &str) -> Option<crate::data::Rect> {
    let full = format!("Scorebar Player {} {}", player + 1, name);
    assets.rect_list.as_ref()?.get(&full)
}

/// Sync score-bar sprites to sim state (Playing only).
#[allow(clippy::too_many_arguments)]
fn sync_scorebar(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    flow: Res<Flow>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut bars: ResMut<ScoreBars>,
    time: Res<Time>,
    mut q: Query<(Entity, &ScoreBarSprite, &mut Sprite, &mut Transform)>,
) {
    let dt = time.delta_secs() * 60.0; // meter rates are per-frame at 60fps
    let active = flow.state == FlowState::Playing;

    // Desired elements; `center` is the sprite centre on screen.
    struct Want {
        player: usize,
        elem: BarElem,
        rid: ResId,
        frame: i64,
        center: Vec2,
        /// Screen x at which the sprite is cut off on the right (meter fill).
        clip_right: Option<f32>,
        alpha: f32,
        scale: f32,
    }
    let mut wants: Vec<Want> = Vec::new();

    if active {
        use crate::game::sim::pf;
        let sim = &sim_res.0;
        let pdef_rid = [crate::pak::id("pl01"), crate::pak::id("pl02")];
        // Perm floats place sprite centres in front-buffer space, where the
        // playfield starts at x=0; on screen it starts at PLAY_LEFT.
        let at = |assets: &GameAssets, xi: usize, yi: usize| {
            Vec2::new(pf(assets, xi) + PLAY_LEFT, pf(assets, yi))
        };
        // Depleted meter section: the original overlays a black colour
        // strip letting `ColorStrip_BlendAmount`/32 of the meter through
        // (text formats `sbsh`/`sbpm`).
        let depleted_alpha = |assets: &GameAssets, fmt: &str| {
            assets
                .text_formats
                .get(&crate::pak::id(fmt))
                .map(|t| t.get_int("ColorStrip_BlendAmount_0To32") as f32 / 32.0)
                .unwrap_or(0.25)
        };
        let weapon_alpha = [
            1.0 - pf(&assets, 0x8d) / 32.0, // ScoreBar_Weapons_BlendAmount_Selected
            1.0 - pf(&assets, 0x8c) / 32.0, // ..._NonSelected
        ];
        let weapon_scale = pf(&assets, 0x8e); // NonSelectedScaleFactor
        for slot in 0..2 {
            let player = sim.players[slot]
                .as_ref()
                .filter(|p| p.phase != crate::game::sim::PlayerPhase::Dead);
            let in_game = player.is_some();
            // Ease meter fill toward the live 0..100 values.
            let (shields, power) = player
                .map(|p| (p.shields as f32, p.power as f32))
                .unwrap_or((0.0, 0.0));
            ease(&mut bars.shield[slot], shields, dt, 2.0, 3.0);
            ease(&mut bars.power[slot], power, dt, 2.0, 4.0);

            let Some(def) = assets.players.get(&pdef_rid[slot]) else { continue };
            let life = (def.sprite_score_bar, def.sprite_score_bar_frame);
            let meters = [
                (def.sprite_score_bar_shield, def.sprite_score_bar_shield_frame, bars.shield[slot], "Shields", "sbsh", 0x74, BarElem::ShieldTrack, BarElem::ShieldFill),
                (def.sprite_score_bar_power, def.sprite_score_bar_power_frame, bars.power[slot], "Power", "sbpm", 0x7a, BarElem::PowerTrack, BarElem::PowerFill),
            ];
            // P2's perm floats follow P1's as the next (x, y) pair.
            let o = slot * 2;

            if life.0 != crate::pak::NONE_ID {
                // A player not in the game gets a half-blended life symbol.
                wants.push(Want { player: slot, elem: BarElem::LifeIcon, rid: life.0, frame: life.1, center: at(&assets, 0x70 + o, 0x71 + o), clip_right: None, alpha: if in_game { 1.0 } else { 0.5 }, scale: 1.0 });
            }
            for (rid, frame, pct, rect, fmt, xi, track, fill) in meters {
                if rid == crate::pak::NONE_ID {
                    continue;
                }
                let center = at(&assets, xi + o, xi + 1 + o);
                wants.push(Want { player: slot, elem: track, rid, frame, center, clip_right: None, alpha: depleted_alpha(&assets, fmt), scale: 1.0 });
                // The filled part ends `pct` of the way across the meter's rect.
                if let Some(r) = bar_rect(&assets, slot, rect) {
                    let edge = SCOREBAR_X + r.left as f32 + (pct / 100.0).clamp(0.0, 1.0) * (r.right - r.left) as f32;
                    wants.push(Want { player: slot, elem: fill, rid, frame, center, clip_right: Some(edge), alpha: 1.0, scale: 1.0 });
                }
            }

            // Weapon preview chain: current + next two PEAA weapons
            // (AirWeapon_GetScoreBarInfo). Slots whose preview duplicates an
            // earlier slot are blanked, like the original.
            let Some(p) = player else { continue };
            let chain = crate::game::sim::weapon_chain(&assets, crate::pak::id("PEAA"), sim.level_number);
            let mut previews: [Option<ResId>; 3] = [None; 3];
            if let Some(c) = chain.iter().position(|&w| w == p.air_weapon) {
                let mut seen: Vec<(ResId, i64)> = Vec::new();
                for i in 0..3 {
                    let w = chain[(c + i) % chain.len()];
                    let Some(wd) = assets.weapons.get(&w) else { continue };
                    let key = (wd.score_bar_preview_face, wd.score_bar_preview_frame);
                    if wd.score_bar_preview_face == crate::pak::NONE_ID || seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    previews[i] = Some(w);
                }
            } else {
                previews[0] = Some(p.air_weapon);
            }
            // ScoreBar_P1Weapons_1..3 at 0x80.., P2's at 0x86..
            let wbase = 0x80 + slot * 6;
            for (i, w) in previews.iter().enumerate() {
                let Some(w) = w else { continue };
                let Some(wd) = assets.weapons.get(w) else { continue };
                if wd.score_bar_preview_face == crate::pak::NONE_ID {
                    continue;
                }
                let selected = i == 0;
                wants.push(Want {
                    player: slot,
                    elem: BarElem::Weapon(i),
                    rid: wd.score_bar_preview_face,
                    frame: wd.score_bar_preview_frame,
                    center: at(&assets, wbase + i * 2, wbase + i * 2 + 1),
                    clip_right: None,
                    alpha: weapon_alpha[if selected { 0 } else { 1 }],
                    scale: if selected { 1.0 } else { weapon_scale },
                });
            }
        }
    }

    // Reconcile entities keyed by (player, elem).
    let mut existing: std::collections::HashMap<(usize, BarElem), Entity> =
        q.iter().map(|(e, s, _, _)| ((s.player, s.elem), e)).collect();

    for w in &wants {
        let key = (w.player, w.elem);
        let Some(handle) = frame_image(&mut assets, &mut images, &mut book, w.rid, w.frame)
        else {
            continue;
        };
        let size = images
            .get(&handle)
            .map(|i| Vec2::new(i.width() as f32, i.height() as f32))
            .unwrap_or(Vec2::ZERO);
        // Meter fill: keep only the part of the image left of `clip_right`.
        let left = w.center.x - size.x * 0.5;
        let shown = match w.clip_right {
            Some(edge) => (edge - left).clamp(0.0, size.x).floor(),
            None => size.x,
        };
        let rect = w.clip_right.map(|_| bevy::math::Rect::new(0.0, 0.0, shown, size.y));
        // Tracks sit under fills.
        let z = Z_SCOREBAR + if w.clip_right.is_some() { 1.0 } else { 0.0 };
        let pos = to_world_z(left + shown * 0.5, w.center.y, z);
        let color = Color::srgba(1.0, 1.0, 1.0, if shown > 0.0 { w.alpha } else { 0.0 });
        let scale = Vec3::splat(w.scale);
        match existing.remove(&key) {
            Some(e) => {
                if let Ok((_, _, mut sprite, mut tf)) = q.get_mut(e) {
                    sprite.image = handle;
                    sprite.color = color;
                    sprite.rect = rect;
                    tf.translation = pos;
                    tf.scale = scale;
                }
            }
            None => {
                commands.spawn((
                    Sprite {
                        image: handle,
                        color,
                        rect,
                        ..default()
                    },
                    Transform::from_translation(pos).with_scale(scale),
                    Visibility::default(),
                    ScoreBarSprite { player: w.player, elem: w.elem },
                ));
            }
        }
    }
    // Despawn leftovers (everything when !active).
    for (_, e) in existing {
        commands.entity(e).despawn();
    }
}

fn ease(cur: &mut f32, target: f32, dt: f32, inc: f32, dec: f32) {
    if *cur > target {
        *cur = (*cur - dec * dt).max(target);
    } else if *cur < target {
        *cur = (*cur + inc * dt).min(target);
    }
}

