//! Gameplay on the faithful port (`game::og`): owns the running `Session`,
//! steps it, and draws each `FrameView` the way the original's 16-bit
//! software renderer does (a CPU compositor into one 640x480 image).
//!
//! Frame structure (see `og::view`): the background window is copied from
//! the level's background buffer (which accumulates the terrain stamps),
//! then the middle sprite group, the particles, the front group and the
//! texts are drawn into the 416x480 game area, which sits at x = 32 of the
//! 640x480 screen, left of the 160-pixel score bar.
//!
//! Tick mapping in screenshot mode (`--shot ... --at N`): exactly one tick
//! per rendered frame, so the capture of frame N shows the game after N
//! ticks, i.e. with game time N (the original draws after the tick and the
//! time increment).

use crate::assets::{GameAssets, GifFrame};
use crate::game::input::{KeyBindings, BOMB, CYCLE, DOWN, FIRE, LEFT, RIGHT, UP};
use crate::game::og::session::Session;
use crate::game::og::view::{DrawItem, DrawMode, FrameView, TextItem};
use crate::game::render::{layout_text, Flow, FlowState, ShotPlan, FONT};
use crate::pak::{self, ResId};
use bevy::prelude::*;
use std::path::PathBuf;

pub const SCREEN_W: usize = 640;
pub const SCREEN_H: usize = 480;
/// Left edge of the game area on the screen.
pub const PLAY_LEFT: i32 = 32;
/// Left edge of the score bar on the screen.
pub const SCOREBAR_X: i32 = PLAY_LEFT + 416;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OgMode {
    Idle,
    Live,
    Film,
}

/// The running port game plus what is needed to draw it.
pub struct OgGame {
    exe: Vec<u8>,
    paks: PathBuf,
    pub session: Option<Session>,
    pub mode: OgMode,
    /// The level's background buffer (RGBA8, `bg_w` x `bg_h`) with the
    /// terrain stamps painted in, and what it was loaded for.
    bg: Vec<u8>,
    bg_w: i32,
    bg_h: i32,
    bg_key: (u32, i32, i32),
    /// Fraction of a tick carried between rendered frames.
    acc: f32,
    /// Ticks run since the game started.
    pub ticks: u32,
    cycle_latch: [bool; 2],
    handle: Option<Handle<Image>>,
    dirty: bool,
    /// Scripted inputs (screenshot mode without a film): per tick.
    pub script: Option<crate::game::render::DemoRec>,
    /// The level music last handed to the audio engine.
    music: u32,
}

impl OgGame {
    /// Loads the game data (takes a moment).
    pub fn new(paks: PathBuf) -> Result<OgGame, String> {
        let exe_path = [paks.join("../../DeimosRising.exe"), PathBuf::from("retail/DeimosRising.exe")]
            .into_iter()
            .find(|p| p.exists())
            .ok_or("DeimosRising.exe not found next to the data directory")?;
        let exe = std::fs::read(exe_path).map_err(|e| e.to_string())?;
        Ok(OgGame {
            exe,
            paks,
            session: None,
            mode: OgMode::Idle,
            bg: Vec::new(),
            bg_w: 0,
            bg_h: 0,
            bg_key: (0, 0, 0),
            acc: 0.0,
            ticks: 0,
            cycle_latch: [false; 2],
            handle: None,
            dirty: false,
            script: None,
            music: 0,
        })
    }

    fn session(&mut self) -> Result<&mut Session, String> {
        if self.session.is_none() {
            crate::game::og::play::SOUND_CAPTURE.store(true, std::sync::atomic::Ordering::Relaxed);
            self.session = Some(Session::new(&self.exe, &self.paks)?);
        }
        Ok(self.session.as_mut().unwrap())
    }

    fn reset(&mut self) {
        self.bg_key = (0, 0, 0);
        self.acc = 0.0;
        self.ticks = 0;
        self.cycle_latch = [false; 2];
        self.dirty = true;
        self.music = 0;
    }

    /// A new live game of `players` players from the 1-based level list
    /// position `level`.
    pub fn start_live(&mut self, players: usize, level: usize, seed: u32) -> Result<(), String> {
        self.session()?.start_game(players as i32, level as i32, seed)?;
        self.mode = OgMode::Live;
        self.reset();
        Ok(())
    }

    /// A replay of a retail film.
    pub fn start_film(&mut self, id: ResId, bytes: &[u8]) -> Result<(), String> {
        self.session()?.start_film(id, bytes)?;
        self.mode = OgMode::Film;
        self.reset();
        Ok(())
    }

    /// Loads the game data now rather than at the first game.
    pub fn warm(&mut self) -> Result<(), String> {
        self.session().map(|_| ())
    }

    /// Back to nothing running (menus).
    pub fn stop(&mut self) {
        self.mode = OgMode::Idle;
    }

    pub fn ended(&self) -> bool {
        self.session.as_ref().map(|s| s.is_ended()).unwrap_or(true)
    }

    /// Position (1-based) of the running level in the level list.
    pub fn level_number(&self) -> i32 {
        self.session.as_ref().map(|s| s.level_number()).unwrap_or(0)
    }

    /// Runs one tick and keeps the background buffer current.
    fn tick(&mut self, assets: &mut GameAssets, inputs: [[bool; 7]; 2]) {
        let Some(s) = self.session.as_mut() else { return };
        s.tick(inputs);
        self.ticks += 1;
        if let Some(t) = std::env::var("OG_ITEMS").ok().and_then(|v| v.parse::<i32>().ok()) {
            if s.game_time() == t {
                for it in &s.frame().items {
                    eprintln!(
                        "ITEM {:?} {} f{} ({},{}) sc={} {:?} blend={} tint={:04x} layer={} flags={} clip={:?}",
                        it.kind,
                        String::from_utf8_lossy(&it.sprite.to_le_bytes()),
                        it.frame,
                        it.x,
                        it.y,
                        it.scale,
                        it.mode,
                        it.blend,
                        it.tint,
                        it.layer,
                        it.flags,
                        it.clip
                    );
                }
            }
        }
        if std::env::var_os("OG_DEBUG").is_some() {
            let t = s.game_time();
            let b = &s.frame().background;
            let (sy, sl) = (b.scroll_y, b.side_scroll);
            let p = s.player(0);
            eprintln!("OGTICK t={t} scroll_y={sy} side={sl} p0=({:.1},{:.1}) st={} items={}", p.x, p.y, p.state, s.frame().items.len());
        }
        self.absorb(assets);
    }

    /// Loads the background buffer when the level changed and stamps the
    /// terrain items of the latest frame into it.
    fn absorb(&mut self, assets: &mut GameAssets) {
        let Some(s) = self.session.as_ref() else { return };
        let fv = s.frame();
        let key = (fv.background.image, s.level_number(), s.levels_played());
        if key != self.bg_key {
            self.bg_key = key;
            if let Some((w, h, px)) = assets.image16(fv.background.image) {
                self.bg_w = *w as i32;
                self.bg_h = *h as i32;
                self.bg = px.clone();
                quantize(&mut self.bg);
            } else {
                self.bg.clear();
                self.bg_w = 0;
                self.bg_h = 0;
            }
        }
        for it in fv.items.iter().filter(|i| i.to_terrain) {
            let mut t = Target { px: &mut self.bg, w: self.bg_w, h: self.bg_h };
            blit_item(assets, &mut t, it, it.clip);
        }
        self.dirty = true;
    }
}

// ---------------------------------------------------------------------------
// Blitting
// ---------------------------------------------------------------------------

struct Target<'a> {
    px: &'a mut [u8],
    w: i32,
    h: i32,
}

fn rgb555(c: u16) -> [u8; 3] {
    let x = |v: u16| {
        let v = (v & 31) as u8;
        (v << 3) | (v >> 2)
    };
    [x(c >> 10), x(c >> 5), x(c)]
}

/// The original's blits on 8-bit channels. `clip` is {top, left, bottom,
/// right}, right/bottom exclusive.
#[allow(clippy::too_many_arguments)]
fn blit(
    t: &mut Target,
    f: &GifFrame,
    cx: i32,
    cy: i32,
    scale: f32,
    mode: DrawMode,
    blend: u32,
    tint: [u8; 3],
    clip: [i32; 4],
) {
    let (w, h) = (f.width as i32, f.height as i32);
    // `U_Sprite_Draw`: an unscaled frame has its top left at (x - w/2,
    // y - h/2) in integer halves; a scaled one (`U_Sprite_DrawScaledClipped`)
    // is (int)(w * scale) wide, its left (int)(x - w * scale / 2).
    let (dw, dh, x0, y0) = if scale == 1.0 {
        (w, h, cx - w / 2, cy - h / 2)
    } else {
        let sw = w as f32 * scale;
        let sh = h as f32 * scale;
        (sw as i32, sh as i32, (cx as f32 - sw / 2.0) as i32, (cy as f32 - sh / 2.0) as i32)
    };
    if dw <= 0 || dh <= 0 {
        return;
    }
    let left = clip[1].max(0);
    let top = clip[0].max(0);
    let right = clip[3].min(t.w);
    let bottom = clip[2].min(t.h);
    for dy in 0..dh {
        let y = y0 + dy;
        if y < top || y >= bottom {
            continue;
        }
        let sy = if dh == h { dy } else { (dy * h / dh).min(h - 1) };
        for dx in 0..dw {
            let x = x0 + dx;
            if x < left || x >= right {
                continue;
            }
            let sx = if dw == w { dx } else { (dx * w / dw).min(w - 1) };
            let s = ((sy * w + sx) * 4) as usize;
            let a = f.rgba[s + 3] as u32;
            // Alpha was stored as (32 - level) * 255 / 32.
            let level = 32 - (a * 32 + 127) / 255;
            if level >= 32 {
                continue;
            }
            let weight = match mode {
                DrawMode::Normal => level,
                // The shadow blit's weight grows with the square of the
                // pixel's own level (a shadow is not drawn at 32 or more).
                DrawMode::Shadow if level > 0 => ((level as f32 * (level as f32 * 0.032)) + blend as f32) as u32,
                _ => (level + blend).min(32),
            };
            if weight >= 32 {
                continue;
            }
            let src = match mode {
                DrawMode::Shadow => [0, 0, 0],
                DrawMode::Colorised => tint,
                _ => [f.rgba[s], f.rgba[s + 1], f.rgba[s + 2]],
            };
            let d = ((y * t.w + x) * 4) as usize;
            for k in 0..3 {
                t.px[d + k] = mix5(src[k], t.px[d + k], weight);
            }
            t.px[d + 3] = 255;
        }
    }
}

/// The original's 16-bit pixels keep 5 bits per channel: blends `src`
/// (weight 32 - `w`) and `dst` (weight `w`) on the 5-bit values and gives
/// the result back as 8 bits the way a 555 pixel is shown.
fn mix5(src: u8, dst: u8, w: u32) -> u8 {
    let o = ((src >> 3) as u32 * (32 - w) + (dst >> 3) as u32 * w) / 32;
    ((o << 3) | (o >> 2)) as u8
}

/// A background or panel pixel as the original's 16-bit buffer holds it.
fn quantize(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        for k in 0..3 {
            let v = p[k] >> 3;
            p[k] = (v << 3) | (v >> 2);
        }
    }
}

/// A black strip over a rectangle: keeps `weight` / 32 of what is there.
fn darken(t: &mut Target, x0: i32, y0: i32, x1: i32, y1: i32, weight: u32) {
    for y in y0.max(0)..y1.min(t.h) {
        for x in x0.max(0)..x1.min(t.w) {
            let d = ((y * t.w + x) * 4) as usize;
            for k in 0..3 {
                t.px[d + k] = mix5(0, t.px[d + k], weight);
            }
        }
    }
}

fn blit_item(assets: &mut GameAssets, t: &mut Target, it: &DrawItem, clip: [i32; 4]) {
    let Some(sprite) = assets.sprite(it.sprite) else { return };
    let Some(f) = sprite.frames.get(it.frame.max(0) as usize) else { return };
    blit(t, f, it.x, it.y, it.scale, it.mode, it.blend, rgb555(it.tint), clip);
}

fn blit_sprite(assets: &mut GameAssets, t: &mut Target, id: ResId, frame: i64, cx: i32, cy: i32, mode: DrawMode, blend: u32, clip: [i32; 4]) {
    let Some(sprite) = assets.sprite(id) else { return };
    let Some(f) = sprite.frames.get(frame.max(0) as usize) else { return };
    blit(t, f, cx, cy, 1.0, mode, blend, [255; 3], clip);
}

// ---------------------------------------------------------------------------
// Frame composition
// ---------------------------------------------------------------------------

fn draw_particles(g: &mut Target, fv: &FrameView) {
    for p in &fv.particles {
        let blob = crate::game::og::view::particle_blob(p.w);
        let near = rgb555(p.near_color);
        let far = rgb555(p.far_color);
        for (j, row) in blob.iter().enumerate() {
            for (i, px) in row.iter().enumerate() {
                let (x, y) = (p.ix + i as i32, p.iy + j as i32);
                if x < 0 || y < 0 || x >= g.w || y >= g.h || px.weight >= 32 {
                    continue;
                }
                let c = if px.near { near } else { far };
                let d = ((y * g.w + x) * 4) as usize;
                for k in 0..3 {
                    g.px[d + k] = mix5(c[k], g.px[d + k], px.weight);
                }
            }
        }
    }
}

fn parse_hex(s: Option<&str>) -> u32 {
    u32::from_str_radix(s.unwrap_or("ffffff").trim(), 16).unwrap_or(0xffffff)
}

/// Draws `text` with text format `fmt_id` (placed by `pos`, blended by
/// `blend`); `align` overrides the format's.
#[allow(clippy::too_many_arguments)]
fn draw_text(
    assets: &mut GameAssets,
    t: &mut Target,
    fmt_id: ResId,
    text: &str,
    pos_add: (i32, i32),
    blend: Option<u32>,
    align: Option<String>,
    dim_half: bool,
) {
    let Some(fmt) = assets.text_formats.get(&fmt_id).cloned() else { return };
    let pos = Vec2::new((fmt.get_int("Loc_X") as i32 + pos_add.0) as f32, (fmt.get_int("Loc_Y") as i32 + pos_add.1) as f32);
    let raw = align.unwrap_or_else(|| fmt.get("Format").unwrap_or("LEFT").trim().to_uppercase());
    let align: String = match raw.as_str() {
        "3" => "CEBU".into(),
        "4" => "CEGA".into(),
        o => o.chars().take(4).collect(),
    };
    let glyphs = layout_text(
        assets,
        text,
        pos,
        &align,
        fmt.get("Monospaced").map(|v| v.trim() == "TRUE").unwrap_or(false),
        fmt.get_int("SpaceBetweenChars") as i32,
    );
    let mut b = blend.unwrap_or(fmt.get_int("BlendAmount_0To32") as u32);
    if dim_half {
        b += (32 - b.min(32)) / 2;
    }
    if b >= 32 {
        return;
    }
    let colorise = fmt.get("Colorise_Do").map(|v| v.trim() == "TRUE").unwrap_or(false);
    let rgb = parse_hex(fmt.get("ColoriseColor"));
    let tint = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
    let mode = if colorise { DrawMode::Colorised } else { DrawMode::Translucent };
    let (w, h) = (t.w, t.h);
    // The colour strip behind the text: its rectangle grown by the offsets
    // (at least `MinWidth` x `MinHeight`), a colour blended over it.
    if fmt.get("ColorStrip_Do").map(|v| v.trim() == "TRUE").unwrap_or(false) && !glyphs.is_empty() {
        let dims: Vec<(i32, i32)> = match assets.sprite(pak::id(FONT)) {
            Some(sp) => glyphs
                .iter()
                .map(|g| sp.frames.get(g.frame as usize).map(|f| (f.width as i32, f.height as i32)).unwrap_or((0, 0)))
                .collect(),
            None => vec![(0, 0); glyphs.len()],
        };
        let left = glyphs.iter().zip(&dims).map(|(g, d)| g.center.x as i32 - d.0 / 2).min().unwrap();
        let right = glyphs.iter().zip(&dims).map(|(g, d)| g.center.x as i32 - d.0 / 2 + d.0).max().unwrap() + 1;
        let top = pos.y as i32;
        let bottom = top + dims.iter().map(|d| d.1).max().unwrap_or(0);
        let (ho, vo) = (fmt.get_int("ColorStrip_HOffset") as i32, fmt.get_int("ColorStrip_VOffset") as i32);
        let mut r = (left - ho, top - vo, right + ho, bottom + vo);
        r.2 = r.2.max(r.0 + fmt.get_int("ColorStrip_MinWidth") as i32);
        r.3 = r.3.max(r.1 + fmt.get_int("ColorStrip_MinHeight") as i32);
        let c = parse_hex(fmt.get("ColorStrip_Color"));
        let cb = fmt.get_int("ColorStrip_BlendAmount_0To32") as u32;
        for y in r.1.max(0)..r.3.min(h) {
            for x in r.0.max(0)..r.2.min(w) {
                let d = ((y * w + x) * 4) as usize;
                for k in 0..3 {
                    t.px[d + k] = mix5((c >> (16 - 8 * k)) as u8, t.px[d + k], cb.min(32));
                }
            }
        }
    }
    for g in glyphs {
        blit_sprite_tint(assets, t, pak::id(FONT), g.frame, g.center.x as i32, g.center.y as i32, mode, b, tint, [0, 0, h, w]);
    }
}

fn blit_sprite_tint(assets: &mut GameAssets, t: &mut Target, id: ResId, frame: i64, cx: i32, cy: i32, mode: DrawMode, blend: u32, tint: [u8; 3], clip: [i32; 4]) {
    let Some(sprite) = assets.sprite(id) else { return };
    let Some(f) = sprite.frames.get(frame.max(0) as usize) else { return };
    blit(t, f, cx, cy, 1.0, mode, blend, tint, clip);
}

fn draw_game_text(assets: &mut GameAssets, t: &mut Target, it: &TextItem) {
    let Some(&(_, fmt)) = assets.format_ids.entries.get(it.perm_index as usize) else { return };
    let align = it.alignment.map(|a| String::from_utf8_lossy(&a.to_le_bytes()).into_owned());
    draw_text(assets, t, fmt, &it.text, (0, it.y_add), Some(it.blend), align, false);
}

/// The score bars (`G_ScoreBar_Draw`), over the `scor` panel, in screen
/// coordinates.
fn draw_score_bars(assets: &mut GameAssets, t: &mut Target, fv: &FrameView) {
    let pf = |a: &GameAssets, i: usize| crate::game::sim::pf(a, i);
    let full = [0, 0, t.h, t.w];
    let max_lives = pf(assets, 0x8f) as i64;
    for slot in 0..2usize {
        let bar = &fv.score_bars[slot];
        let in_game = bar.shown;
        let o = slot * 2;
        let at = |a: &GameAssets, xi: usize, yi: usize| (pf(a, xi) as i32 + PLAY_LEFT, pf(a, yi) as i32);
        let [life, life_frame, sh, sh_frame, pw, pw_frame] = bar.def_copy;
        if life != pak::NONE_ID {
            let (x, y) = at(assets, 0x70 + o, 0x71 + o);
            blit_sprite(assets, t, life, life_frame as i64, x, y, DrawMode::Translucent, if in_game { 0 } else { 16 }, full);
        }
        let depleted = |a: &GameAssets, fmt: &str| {
            a.text_formats.get(&pak::id(fmt)).map(|t| t.get_int("ColorStrip_BlendAmount_0To32") as u32).unwrap_or(24)
        };
        let meters = [
            (sh, sh_frame, bar.shield_pct, "Shields", "sbsh", 0x74usize),
            (pw, pw_frame, bar.power_pct, "Power", "sbpm", 0x7a),
        ];
        for (rid, frame, pct, rect, fmt, xi) in meters {
            if rid == pak::NONE_ID {
                continue;
            }
            let (x, y) = at(assets, xi + o, xi + 1 + o);
            // The whole meter, then a black colour strip over the depleted
            // part that lets `ColorStrip_BlendAmount` / 32 of it through.
            blit_sprite(assets, t, rid, frame as i64, x, y, DrawMode::Normal, 0, full);
            let b = depleted(assets, fmt);
            let name = format!("Scorebar Player {} {}", slot + 1, rect);
            let r = assets.rect_list.as_ref().and_then(|l| l.get(&name));
            if let Some(r) = r {
                let edge = SCOREBAR_X + r.left as i32 + ((pct / 100.0).clamp(0.0, 1.0) * (r.right - r.left) as f32) as i32;
                darken(t, edge, r.top as i32, SCOREBAR_X + r.right as i32, r.bottom as i32, b);
            }
        }
        // Air weapon preview chain: {sprite, frame} of this, the next and
        // the following air weapon.
        let wbase = 0x80 + slot * 6;
        let w_non = (pf(assets, 0x8c) as u32).min(32);
        let scale = pf(assets, 0x8e);
        for (i, &(face, frame)) in bar.weapons.iter().enumerate() {
            if face == pak::NONE_ID || !in_game {
                continue;
            }
            let (x, y) = at(assets, wbase + i * 2, wbase + i * 2 + 1);
            let selected = i == 0;
            let blend = if selected { (pf(assets, 0x8d) as u32).min(32) } else { w_non };
            let Some(sprite) = assets.sprite(face) else { continue };
            let Some(f) = sprite.frames.get(frame.max(0) as usize) else { continue };
            blit(t, f, x, y, if selected { 1.0 } else { scale }, DrawMode::Translucent, blend, [255; 3], full);
        }
        // Score and lives.
        let n = slot + 1;
        let mut lives = (bar.lives as i64 - 1).max(0);
        if max_lives > 0 {
            lives = lives.min(max_lives);
        }
        let lives_fmt = if in_game && lives == 0 { format!("sll{n}") } else { format!("sbl{n}") };
        draw_text(assets, t, pak::id(&format!("sbs{n}")), &format!("{:07}", bar.score.max(0)), (PLAY_LEFT, 0), None, None, !in_game);
        draw_text(assets, t, pak::id(&lives_fmt), &format!("{}", lives), (PLAY_LEFT, 0), None, None, !in_game);
    }
}

/// The whole screen for the session's current frame, RGBA8 640x480.
fn render(og: &mut OgGame, assets: &mut GameAssets, out: &mut Vec<u8>) {
    out.clear();
    out.resize(SCREEN_W * SCREEN_H * 4, 0);
    for px in out.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let Some(s) = og.session.as_ref() else { return };
    let fv = s.frame();
    let bgd = &fv.background;
    let (vw, vh) = (bgd.view_w.max(0), bgd.view_h.max(0));
    let mut gpx = vec![0u8; (vw * vh * 4) as usize];
    for px in gpx.chunks_exact_mut(4) {
        px[3] = 255;
    }
    // Background window.
    if og.bg_w > 0 {
        for y in 0..vh {
            let sy = bgd.scroll_y + y;
            if sy < 0 || sy >= og.bg_h {
                continue;
            }
            let x0 = bgd.src_left.max(0);
            let n = (vw.min(og.bg_w - x0)).max(0);
            let s0 = ((sy * og.bg_w + x0) * 4) as usize;
            let d0 = ((y * vw) * 4) as usize;
            gpx[d0..d0 + (n * 4) as usize].copy_from_slice(&og.bg[s0..s0 + (n * 4) as usize]);
        }
    }
    let clip_game = [0, 0, vh, vw];
    {
        let mut g = Target { px: &mut gpx, w: vw, h: vh };
        for it in fv.middle() {
            blit_item(assets, &mut g, it, it.clip);
        }
        draw_particles(&mut g, fv);
        for it in fv.front() {
            blit_item(assets, &mut g, it, it.clip);
        }
        for tx in &fv.texts {
            draw_game_text(assets, &mut g, tx);
        }
        // A replay shows its tag (format `gare`, "Game Replay").
        if og.mode == OgMode::Film {
            draw_text(assets, &mut g, pak::id("gare"), "REPLAY", (0, 0), None, None, false);
        }
        let _ = clip_game;
    }
    for y in 0..(vh as usize).min(SCREEN_H) {
        let d = (y * SCREEN_W + PLAY_LEFT as usize) * 4;
        let n = (vw as usize).min(SCREEN_W - PLAY_LEFT as usize) * 4;
        out[d..d + n].copy_from_slice(&gpx[y * vw as usize * 4..y * vw as usize * 4 + n]);
    }
    let mut scr = Target { px: out, w: SCREEN_W as i32, h: SCREEN_H as i32 };
    // The score bar panel.
    let panel = assets.image16(pak::id("scor")).cloned();
    if let Some((w, h, mut px)) = panel {
        quantize(&mut px);
        for y in 0..(h as usize).min(SCREEN_H) {
            for x in 0..(w as usize) {
                let dx = SCOREBAR_X as usize + x;
                if dx >= SCREEN_W {
                    break;
                }
                let d = (y * SCREEN_W + dx) * 4;
                let s = (y * w as usize + x) * 4;
                scr.px[d..d + 4].copy_from_slice(&px[s..s + 4]);
            }
        }
    }
    draw_score_bars(assets, &mut scr, fv);
}

// ---------------------------------------------------------------------------
// Bevy systems
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct OgScreen;

fn slot_inputs(keys: &ButtonInput<KeyCode>, b: &KeyBindings, latch: &mut [bool; 2], first: bool) -> [[bool; 7]; 2] {
    let mut out = [[false; 7]; 2];
    for p in 0..2 {
        let k = &b.keys[p];
        if keys.just_pressed(k[CYCLE]) {
            latch[p] = true;
        }
        out[p] = [
            keys.pressed(k[UP]),
            keys.pressed(k[RIGHT]),
            keys.pressed(k[DOWN]),
            keys.pressed(k[LEFT]),
            keys.pressed(k[BOMB]),
            keys.pressed(k[FIRE]),
            first && latch[p],
        ];
        if first {
            latch[p] = false;
        }
    }
    out
}

/// Steps the port (one tick per 1/60 s; exactly one per frame in
/// screenshot mode) and keeps the screen image current.
#[allow(clippy::too_many_arguments)]
pub fn og_system(
    mut commands: Commands,
    mut og: NonSendMut<OgGame>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<KeyBindings>,
    flow: Res<Flow>,
    shot: Option<Res<ShotPlan>>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut audio: Option<ResMut<crate::game::audio::AudioEngine>>,
    screen: Query<Entity, With<OgScreen>>,
) {
    let playing = flow.state == FlowState::Playing && og.mode != OgMode::Idle;
    if !playing {
        for e in screen.iter() {
            commands.entity(e).despawn();
        }
        return;
    }
    let og = &mut *og;
    if !flow.paused {
        let n = if shot.is_some() {
            1
        } else {
            og.acc += time.delta_secs() * 60.0;
            let n = og.acc.floor().min(8.0);
            og.acc -= n;
            n as u32
        };
        for i in 0..n {
            let inputs = if og.mode == OgMode::Film {
                [[false; 7]; 2]
            } else if let Some(sc) = &og.script {
                let t = og.ticks as usize;
                let unpack = |v: &Vec<u8>| crate::game::render::DemoRec::unpack(v.get(t).copied().unwrap_or(0));
                let (a, b) = (unpack(&sc.p1), unpack(&sc.p2));
                let m = |s: crate::game::sim::SlotInput| [s.up, s.right, s.down, s.left, s.bomb, s.fire, s.cycle];
                [m(a), m(b)]
            } else {
                slot_inputs(&keys, &bindings, &mut og.cycle_latch, i == 0)
            };
            if og.session.as_ref().map(|s| s.is_playing()).unwrap_or(false) {
                og.tick(&mut assets, inputs);
            }
        }
    }
    // Sounds and music.
    let sounds = crate::game::og::play::take_sounds();
    if let Some(a) = audio.as_deref_mut() {
        if let Some(s) = og.session.as_ref() {
            let m = s.g.m.r32(s.level_addr() + 0x27c);
            if m != og.music {
                og.music = m;
                if m != pak::NONE_ID {
                    a.ensure_music(&mut assets, m);
                }
            }
        }
        for (id, vol, pitch) in sounds {
            if let Some(snd) = assets.sound(id) {
                let pcm = crate::assets::Sound { samples: snd.samples.clone(), channels: snd.channels, sample_rate: snd.sample_rate };
                a.play_sfx_pitched(&pcm, vol as f32 / 100.0, pitch);
            }
        }
    }
    // The screen sprite.
    if og.handle.is_none() {
        let mut img = Image::new(
            bevy::render::render_resource::Extent3d { width: SCREEN_W as u32, height: SCREEN_H as u32, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            vec![0u8; SCREEN_W * SCREEN_H * 4],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        img.sampler = bevy::image::ImageSampler::nearest();
        og.handle = Some(images.add(img));
    }
    if screen.is_empty() {
        commands.spawn((
            Sprite::from_image(og.handle.clone().unwrap()),
            Transform::from_xyz(0.0, 0.0, 1.0),
            OgScreen,
        ));
        og.dirty = true;
    }
    if og.dirty {
        let mut buf = Vec::new();
        render(og, &mut assets, &mut buf);
        if let Some(mut img) = images.get_mut(og.handle.as_ref().unwrap()) {
            img.data = Some(buf);
        }
        og.dirty = false;
    }
}
