//! Bevy rendering layer: turns `Sim` state into sprites each frame.
//!
//! World→screen mapping (top-left origin, y down):
//!   screen_x = world_x - cam_x          (cam_x = side-scroll offset)
//!   screen_y = scroll + 480 - world_y
//! Bevy world→NDC: camera centered, so bevy_pos = (screen_x - 320, 240 - screen_y).

use crate::assets::GameAssets;
use crate::game::ogplay::{OgGame, OgMode};
use crate::game::sim::{PlayerInput, SlotInput, Sim, SCREEN_H, SCREEN_W};
use crate::pak::ResId;
use bevy::prelude::*;
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
    /// A retail film: replayed inside the sim per Active tick, not per
    /// frame, so `p1`/`p2` stay empty.
    pub film: Option<crate::game::sim::FilmPlayback>,
}

impl DemoRec {
    /// Parse an input script: one `<from>-<to> <keys...>` line per span of
    /// frames (inclusive; a lone `<frame>` is a single frame). Keys are
    /// `up down left right fire bomb cycle`, for player 1 unless written
    /// `p2:<key>`. Spans may overlap; `#` starts a comment.
    pub fn from_script(text: &str, level: ResId, seed: u64) -> Result<DemoRec, String> {
        let mut rec = DemoRec { level: Some(level), seed, p1: Vec::new(), p2: Vec::new(), film: None };
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

    pub fn pack(s: &SlotInput) -> u8 {
        (s.up as u8)
            | ((s.down as u8) << 1)
            | ((s.left as u8) << 2)
            | ((s.right as u8) << 3)
            | ((s.fire as u8) << 4)
            | ((s.bomb as u8) << 5)
            | ((s.cycle as u8) << 6)
    }
    pub fn unpack(b: u8) -> SlotInput {
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
        if let Some(f) = &self.film {
            return f.total[0].max(f.total[1]);
        }
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
        Some(DemoRec { level: Some(lvl), seed, p1, p2, film: None })
    }

    /// Build a playback stream from a retail `film` resource, translating
    /// each packed input byte through the recovered G_Film bit map
    /// (bit0=L bit1=R bit2=U bit3=D bit4=bomb bit5=air-fire bit6=cycle)
    /// into our SlotInput packing. This is what the original's attract
    /// mode replays; the seed/level come straight from the film header.
    pub fn from_film(film: &crate::assets::FilmData) -> Self {
        DemoRec {
            level: Some(film.level_id),
            seed: film.seed as u64,
            p1: Vec::new(),
            p2: Vec::new(),
            film: Some(crate::game::sim::FilmPlayback::new(film.inputs.clone(), film.frames)),
        }
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
// Setup
// ---------------------------------------------------------------------------

pub fn setup(mut commands: Commands) {
    // The original blends translucent sprites on the stored colour values,
    // so the camera composites in sRGB rather than in linear light.
    commands.spawn((Camera2d, bevy::camera::CompositingSpace::Srgb));
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
        px[3] = (opacity * 255.0) as u8;
    }
    let handle = images.add(rgba_image(f.width, f.height, &rgba));
    book.blends.insert(key, handle.clone());
    Some(handle)
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

/// Put every unscaled sprite on whole pixels, where the original draws it.
///
/// `U_Sprite_Draw` is handed a sprite's centre in whole pixels and blits
/// with the top-left at `(x - w/2, y - h/2)`, halving in integers. A
/// sprite of odd size therefore covers the pixels around `x + 0.5`, half
/// a pixel off its nominal centre; left on the nominal centre it would
/// straddle pixel edges and shift by a pixel from one glyph to the next.
pub fn snap_sprites(images: Res<Assets<Image>>, mut q: Query<(&Sprite, &mut Transform)>) {
    for (sprite, mut tf) in q.iter_mut() {
        if tf.scale != Vec3::ONE || tf.rotation != Quat::IDENTITY {
            continue;
        }
        let size = sprite
            .custom_size
            .or(sprite.rect.map(|r| r.size()))
            .or(images.get(&sprite.image).map(|i| i.size_f32()));
        let Some(size) = size else { continue };
        // Top-left in screen pixels, then back to the centre Bevy wants.
        let left = (tf.translation.x + SCREEN_W * 0.5).floor() - (size.x * 0.5).floor();
        let top = (SCREEN_H * 0.5 - tf.translation.y).floor() - (size.y * 0.5).floor();
        let snapped = to_world_z(left + size.x * 0.5, top + size.y * 0.5, tf.translation.z);
        if tf.translation != snapped {
            tf.translation = snapped;
        }
    }
}

// ---------------------------------------------------------------------------
// App wiring
// ---------------------------------------------------------------------------

use bevy::window::{Window, WindowPlugin, WindowResolution};
use crate::game::input::KeyBindings;
use crate::game::menu::{MenuPointer, SectorHit, TitleItem, TITLE_ITEMS};

/// Menu/flow state machine. Drives title → sectors → briefing → play →
/// cleared/game-over → credits using the retail `lgsp`/`cred` strings.
pub fn flow_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut flow: ResMut<Flow>,
    mut og: NonSendMut<OgGame>,
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
    if og.mode == OgMode::Film && flow.state == FlowState::Playing && shot.is_none() {
        if keys.get_just_pressed().next().is_some() || pointer.clicked {
            og.stop();
            flow.state = FlowState::Title;
            flow.ticks = 0;
        }
    }
    // Title idle ~20s: start attract playback. The original plays its
    // pre-recorded demo films here — cycle through them; fall back to the
    // locally-recorded demo only when no film resource loads.
    if flow.state == FlowState::Title && og.mode == OgMode::Idle && flow.ticks > ATTRACT_IDLE_TICKS {
        for _ in 0..DEMO_FILMS.len() {
            let rid = DEMO_FILMS[demo.film_idx % DEMO_FILMS.len()];
            demo.film_idx += 1;
            let Some(bytes) = assets.set.read_raw(crate::pak::Tag::Film, rid) else { continue };
            match og.start_film(rid, &bytes) {
                Ok(()) => {
                    flow.carry = None;
                    flow.state = FlowState::Playing;
                    flow.ticks = 0;
                    flow.paused = false;
                    break;
                }
                Err(e) => eprintln!("attract film failed: {e}"),
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
                // The port plays the campaign on from the chosen sector by
                // itself; the seed is the tick count in the original.
                let seed = if shot.is_some() {
                    0xC0FFEE
                } else {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u32)
                        .unwrap_or(1)
                };
                match og.start_live(flow.players, flow.sector + 1, seed) {
                    Ok(()) => {
                        // Starting above sector 1 restricts the run: no
                        // hi-score and no campaign progress credit.
                        flow.restricted = flow.sector > 0;
                        flow.levels_played = 1;
                        flow.state = FlowState::Playing;
                        flow.ticks = 0;
                        flow.paused = false;
                        flow.game_over_at = None;
                    }
                    Err(e) => {
                        eprintln!("cannot start the game: {e}");
                        flow.state = FlowState::Title;
                        flow.ticks = 0;
                    }
                }
            }
        }
        FlowState::Playing => {
            // Esc leaves the game for the title.
            if esc && shot.is_none() {
                og.stop();
                flow.state = FlowState::Title;
                flow.ticks = 0;
                return;
            }
            if og.mode == OgMode::Film {
                // Attract playback: back to the title when the film (or
                // its level) is over.
                if og.ended() && shot.is_none() {
                    og.stop();
                    flow.state = FlowState::Title;
                    flow.ticks = 0;
                }
                return;
            }
            // The session moves on to the next level by itself.
            let n = og.level_number();
            if n >= 1 && (n - 1) as usize != flow.sector && !og.ended() {
                if !flow.restricted && (n - 1) as u32 > flow.progress {
                    flow.progress = (n - 1) as u32;
                    save_progress(flow.progress);
                }
                flow.sector = (n - 1) as usize;
                flow.levels_played += 1;
            }
            if og.ended() {
                use crate::game::og::session::EndReason;
                let (reason, results) = {
                    let s = og.session.as_mut().unwrap();
                    let reason = s.end_reason();
                    let mut results: Vec<PendingScore> = Vec::new();
                    if !flow.restricted {
                        for i in 0..flow.players.min(2) {
                            let sc = s.player(i).score as i64;
                            if sc > 0 {
                                results.push(PendingScore {
                                    score: sc,
                                    level: assets
                                        .levels
                                        .get(flow.level_ids.get(flow.sector).unwrap_or(&crate::pak::NONE_ID))
                                        .map(|l| l.name.clone())
                                        .unwrap_or_default(),
                                });
                            }
                        }
                    }
                    (reason, results)
                };
                og.stop();
                let mut results = results;
                results.sort_by(|a, b| b.score.cmp(&a.score));
                flow.pending_scores = results.into_iter().filter(|ps| scores.qualifies(ps.score)).collect();
                flow.name_text.clear();
                flow.score_hilite = None;
                flow.game_over_at = None;
                flow.ticks = 0;
                if reason == Some(EndReason::Completed) {
                    if !flow.restricted {
                        flow.progress = flow.progress.max(flow.level_ids.len() as u32);
                        save_progress(flow.progress);
                    }
                    flow.state = FlowState::Credits;
                } else {
                    flow.state = if flow.pending_scores.is_empty() {
                        FlowState::Scores
                    } else {
                        FlowState::NameEntry
                    };
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
                flow.state = if flow.pending_scores.is_empty() {
                    FlowState::Title
                } else {
                    FlowState::NameEntry
                };
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

pub fn run(assets: GameAssets, data_dir: std::path::PathBuf, level_id: ResId, players: usize, shot: Option<ShotConfig>) {
    let mut assets = assets;
    let mut level_ids = mission_sorted_level_ids(&assets);
    if level_ids.is_empty() {
        level_ids.push(level_id);
    }
    let mut og = OgGame::new(data_dir).expect("port game");
    og.warm().expect("port game data");
    let mut demo = Demo::default();
    let mut initial_state = FlowState::Title;
    let mut shot = shot;
    if let Some(cfg) = shot.as_mut().filter(|c| c.screen.is_none()) {
        let last = cfg.frames.iter().copied().max().unwrap_or(0) as usize;
        match (cfg.script.take(), cfg.film) {
            (None, Some(rid)) => {
                let bytes = assets.set.read_raw(crate::pak::Tag::Film, rid).expect("film not found");
                og.start_film(rid, &bytes).expect("film");
            }
            (script, None) => {
                let script = script.unwrap_or_else(|| {
                    // Tap fire: holding it charges the weapon until it overloads.
                    let fire = DemoRec::pack(&crate::game::sim::SlotInput { fire: true, ..default() });
                    let len = last + SHOT_EXIT_DELAY as usize + 60;
                    DemoRec {
                        level: Some(level_id),
                        seed: 0xC0FFEE,
                        p1: (0..len).map(|f| if f % 12 < 4 { fire } else { 0 }).collect(),
                        p2: Vec::new(),
                        film: None,
                    }
                });
                let lid = script.level.unwrap_or(level_id);
                let n = level_ids.iter().position(|&r| r == lid).unwrap_or(0) + 1;
                og.start_live(players, n, script.seed as u32).expect("game");
                og.script = Some(script);
            }
            (Some(_), Some(rid)) => {
                let bytes = assets.set.read_raw(crate::pak::Tag::Film, rid).expect("film not found");
                og.start_film(rid, &bytes).expect("film");
            }
        }
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
        .insert_resource(TextureBook::default())
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.05)))
        .insert_resource(flow)
        .insert_resource(HighScores::load())
        .insert_resource(crate::game::input::KeyBindings::load())
        .insert_resource(MenuPointer::default())
        .insert_resource(demo)
;
    app.insert_non_send(og);
    if !shot_mode {
        if let Some(engine) = crate::game::audio::AudioEngine::new() {
            app.insert_resource(engine);
        }
    }

    if shot_mode {
        app.add_systems(PreUpdate, shot_mute_input.after(bevy::input::InputSystems));
    }
    app.add_systems(Startup, setup).add_systems(
        Update,
        (
            crate::game::menu::gather_pointer,
            flow_system,
            crate::game::ogplay::og_system,
            crate::game::menu::sync_menu,
            snap_sprites,
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
    // G_Text_Draw finds the widest digit by walking the ten frames from
    // the one of '1' but names it `'0' + index`, so it is the width of the
    // character that many past '0' (the quirk makes '2' the stand-in when
    // the third frame is the first widest one).
    let widest_digit = {
        let mut max_w = 0;
        let mut widest = '1';
        for (i, c) in "1234567890".chars().enumerate() {
            let w = dims(c).0;
            if w > max_w {
                max_w = w;
                widest = (b'0' + i as u8) as char;
            }
        }
        dims(widest).0
    };
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
                center: Vec2::new((x + w / 2) as f32, (y + h / 2) as f32),
            });
        }
        x += advance(c);
    }
    out
}

