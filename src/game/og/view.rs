//! What the original draws each frame, without drawing it: the ports of
//! `sub_420740` (0x420740, the draw list build that follows every tick) and
//! of what it calls, with the sprite blitters replaced by recording into a
//! `Vec<DrawItem>` in the order the original's render lists would draw.
//!
//! Frame structure in the original (`G_GameInterface::Draw`, 0x4238a0):
//!
//!   1. `U_Sprite_RenderLists_Clear` (start of the frame, 0x4230f0)
//!   2. the tick (terrain stamps of destroyed units are added here)
//!   3. `sub_420740`: `G_EG_BuildDrawList`, `G_MotionBlur_BuildDrawList`,
//!      `G_Player::BuildDrawList` x2, `sub_421730` (end-of-level text),
//!      `G_Notice_BuildDrawList`, `G_ScoreBar_Draw`
//!   4. render list group Back (layers 0, 1): stamped into the terrain
//!      (background) buffer, never into the screen
//!   5. `G_Bgnd_CopyToFrontBuffer`: the background window to the screen
//!   6. render list group Middle (layers 2..5)
//!   7. `G_Particle_Draw`
//!   8. render list group Front (layers 6..15)
//!
//! [`FrameView::items`] holds layers 0..15 already in draw order (by layer,
//! then in the order the draws were submitted); see [`DrawItem`] for the
//! coordinate spaces.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::bgnd;
use super::g::G;
use super::game::*;
use super::mem::list;
use super::pixbuf;
use super::player::*;
use super::resource::G_Res_GetPermFloat;
use super::scorebar;
use super::umath::ftol;

pub const NONE: u32 = 0x656e6f6e;

/// `U_Sprite_Settings` (0x4a bytes, 1-byte packed): everything a
/// `U_Sprite_RenderLists_Add` receives.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub frame_data: u32,
    pub x: i32,
    pub y: i32,
    pub sprite: u32,
    pub frame: i32,
    pub flags: i32,
    pub scale: f32,
    pub color: u32,
    /// {top, left, bottom, right}
    pub clip: [i32; 4],
    pub layer: i32,
    pub direct: bool,
    pub pad: u8,
    pub tint: u16,
    pub src_rect: [i32; 4],
    pub color2: u16,
}

/// The settings every sprite draw starts from (the 0x4a bytes at 0x4e5473,
/// `G_GameObject.cc`'s prototype).
pub const SETTINGS_TEMPLATE: u32 = 0x4e5473;

impl Settings {
    pub fn template(g: &G) -> Settings {
        let a = SETTINGS_TEMPLATE;
        let m = &g.m;
        Settings {
            frame_data: m.r32(a),
            x: m.ri32(a + 4),
            y: m.ri32(a + 8),
            sprite: m.r32(a + 0xc),
            frame: m.ri32(a + 0x10),
            flags: m.ri32(a + 0x14),
            scale: m.rf32(a + 0x18),
            color: m.r32(a + 0x1c),
            clip: [m.ri32(a + 0x20), m.ri32(a + 0x24), m.ri32(a + 0x28), m.ri32(a + 0x2c)],
            layer: m.ri32(a + 0x30),
            direct: m.r8(a + 0x34) != 0,
            pad: m.r8(a + 0x35),
            tint: m.r16(a + 0x36),
            src_rect: [m.ri32(a + 0x38), m.ri32(a + 0x3c), m.ri32(a + 0x40), m.ri32(a + 0x44)],
            color2: m.r16(a + 0x48),
        }
    }
}

/// Which of a game object's sprite submissions a [`DrawItem`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawKind {
    /// The object's sprite (`Priv_Draw`, first submission). Translucent
    /// when its visibility is not 100 percent.
    Body,
    /// The damage/hit flash: the same sprite again, colourised towards
    /// `tint` (`Priv_Draw`, second submission, while `+0x54 > 0`).
    Flash,
    /// The glow: the same sprite again, colourised (`Priv_Draw`, third
    /// submission, while `+0x6e` is set).
    Glow,
    /// The ground shadow (`Priv_DrawShadow`): the same sprite darkened,
    /// at its own offset position and layer.
    Shadow,
}

/// How `U_Sprite_Draw` composites the sprite (`flags & 1 / 2 / 4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawMode {
    /// Opaque (colour-key transparent pixels, or the per-pixel alpha map).
    Normal,
    /// `dst = (src * (32 - blend) + dst * blend) / 32`.
    Translucent,
    /// `dst = dst * blend / 32` (darkens the background under the sprite).
    Shadow,
    /// `dst = (tint * (32 - blend) + dst * blend) / 32` for every opaque
    /// pixel of the sprite (a silhouette in the `tint` colour).
    Colorised,
}

/// One `U_Sprite_RenderLists_Add` call, i.e. one sprite blit.
///
/// Coordinates: `(x, y)` is the CENTRE of the sprite frame (unscaled draws
/// use `x - w/2, y - h/2` as the top left; scaled draws scale about the
/// same centre). Normal items are in front-buffer pixels: the game-area
/// window, `perm_float(0x36)` wide and `perm_float(0x37)` high, whose left
/// edge is `x = 0` after the side-scroll has been subtracted already (the
/// original subtracts `g_bgndSideScrolling` for objects that scroll with the
/// background, so it is already applied in `x`). Items with `to_terrain` are
/// in background-buffer pixels instead: x already includes the +0x20 border,
/// y is the scroll offset (`bgnd` top of the window) + the object's y.
#[derive(Clone, Debug)]
pub struct DrawItem {
    pub kind: DrawKind,
    /// Sprite group id (the four characters as a little-endian u32).
    pub sprite: u32,
    pub frame: i32,
    pub x: i32,
    pub y: i32,
    /// 1.0 = unscaled. For shadows this is the scale the shadow's own
    /// offset computation left in the settings.
    pub scale: f32,
    pub mode: DrawMode,
    /// `m_color` (0..=31 drawn; 32 is never submitted): translucent and
    /// shadow modes use it as the weight of the DESTINATION (0 = opaque
    /// sprite / black shadow, 32 = nothing); colorised mode as the weight of
    /// the destination too (0 = pure tint colour). `255 - blend * 8` style
    /// conversions are the renderer's business.
    pub blend: u32,
    /// RGB555 (`0x7c1f | 0x3e0` masks) tint colour, used by `Colorised`.
    pub tint: u16,
    /// Raw `U_Sprite_Settings::m_flags` (1 translucent, 2 shadow,
    /// 4 colourised, 8 draw to the terrain buffer).
    pub flags: i32,
    /// Flag 8: stamped permanently into the background (terrain) buffer
    /// instead of the screen (destroyed-unit decals, `layer` 0 or 1).
    pub to_terrain: bool,
    /// Clip rectangle {top, left, bottom, right} in the target buffer's
    /// pixels.
    pub clip: [i32; 4],
    /// Render list layer 0..=15 (see the module docs for the groups).
    pub layer: i32,
    /// `m_direct`: the original blits these immediately instead of through
    /// the lists (no unit in the retail data uses it).
    pub direct: bool,
    /// The object's visibility percent (`+0x62`, 100 = opaque), tint flash
    /// percent (`+0x54`), flash colour (`+0x60`), for reference.
    pub visibility_pct: f32,
    pub tint_pct: f32,
    pub tint_color: u16,
}

impl DrawItem {
    /// `U_Sprite_Draw`'s `mode` from the flags.
    pub fn mode_of(flags: i32) -> DrawMode {
        if flags & 1 != 0 {
            DrawMode::Translucent
        } else if flags & 2 != 0 {
            DrawMode::Shadow
        } else if flags & 4 != 0 {
            DrawMode::Colorised
        } else {
            DrawMode::Normal
        }
    }
    /// For `Translucent`/`Colorised`: 0.0 (nothing) ..= 1.0 (full) weight of
    /// the sprite / tint colour.
    pub fn source_weight(&self) -> f32 {
        (32 - self.blend.min(32)) as f32 / 32.0
    }
}

/// What a game object contributes to a submission besides the settings.
#[derive(Clone, Copy)]
pub struct ObjInfo {
    pub visibility_pct: f32,
    pub tint_pct: f32,
    pub tint_color: u16,
}

pub fn obj_info(g: &G, obj: u32) -> ObjInfo {
    ObjInfo { visibility_pct: g.m.rf32(obj + 0x62), tint_pct: g.m.rf32(obj + 0x54), tint_color: g.m.r16(obj + 0x60) }
}

/// `U_Sprite_RenderLists_Add` (0x4094e0) / `U_Sprite_Draw` for direct
/// settings: records the draw instead of blitting. `g_spriteRenderListsEnabled`
/// (0x4dce92, the console's FX switch) off draws every sprite opaque and
/// unscaled.
pub fn render_lists_add(g: &mut G, s: &Settings, kind: DrawKind, info: ObjInfo) {
    if g.draw_sink.is_none() {
        return;
    }
    let mut s = s.clone();
    if g.m.r8(0x4dce92) != 0 {
        if s.sprite == NONE || s.color == 0x20 {
            return;
        }
    } else {
        s.color = 0;
        s.scale = 1.0;
    }
    if s.sprite == NONE {
        return;
    }
    let item = DrawItem {
        kind,
        sprite: s.sprite,
        frame: s.frame,
        x: s.x,
        y: s.y,
        scale: s.scale,
        mode: DrawItem::mode_of(s.flags),
        blend: s.color,
        tint: s.tint,
        flags: s.flags,
        to_terrain: s.flags & 8 != 0,
        clip: s.clip,
        layer: s.layer,
        direct: s.direct,
        visibility_pct: info.visibility_pct,
        tint_pct: info.tint_pct,
        tint_color: info.tint_color,
    };
    g.draw_sink.as_mut().unwrap().push(item);
}

/// The background (terrain) buffer's rectangle {top, left, bottom, right},
/// as `U_PixelBuffer::GetBounds` gives it for the clip of terrain stamps.
pub fn bg_buffer_rect(g: &G) -> [i32; 4] {
    let b = g.m.r32(bgnd::BG_BUFFER);
    let r = pixbuf::rect(&g.m, b);
    [r.0, r.1, r.2, r.3]
}

/// `U_Pixel16_ConvertVisibilityPercentToBlendAmount` (0x44e010).
pub fn visibility_to_blend(g: &mut G, pct: i32) -> u32 {
    let k1 = g.m.rf32(0x4ff30e) as f64;
    let k2 = g.m.rf32(0x4ff312) as f64;
    let k3 = g.m.rf32(0x4ff316) as f64;
    let mut v = (pct as f64 / k1) * k2 - k2;
    if v < k3 {
        v = -v;
    }
    let r = ftol(v) as u32;
    if r > 0x20 {
        0x20
    } else {
        r
    }
}

// ---------------------------------------------------------------------
// The draw list build
// ---------------------------------------------------------------------

/// A line of text the original draws with `G_Text_Draw` (the glyphs are
/// sprites; font, base position and alignment come from the permanent text
/// settings `perm_index`, `G_Text_GetPermTextSetting`, which are not ported).
#[derive(Clone, Debug)]
pub struct TextItem {
    pub source: TextSource,
    /// Index for `G_Text_GetPermTextSetting`.
    pub perm_index: i32,
    pub text: String,
    /// `G_Text_Settings::m_color`: used as the translucency blend (0..=32,
    /// 32 = invisible), as for sprites.
    pub blend: u32,
    /// `m_field10c`: the sprite layer.
    pub layer: i32,
    /// Added to the permanent setting's `m_pos.y`.
    pub y_add: i32,
    /// Overrides the permanent setting's `m_alignment` (a 4-character code).
    pub alignment: Option<u32>,
    /// Added to `m_field136` (the notice does this; the setting's
    /// `m_flag12c` is cleared when the result exceeds 0x20).
    pub field136_add: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSource {
    /// `G_Player::BuildDrawList`: the player's money counter text.
    PlayerMoney(i32),
    /// `sub_421730`: the end-of-level text.
    EndOfLevel,
    /// `G_Notice_BuildDrawList`.
    Notice,
}

/// A live particle (`G_Particle_Draw`): a 7x7 soft blob whose top left is
/// `(ix, iy)` in front-buffer pixels. Only particles that lie fully inside
/// the game area are listed, as the original skips the others.
#[derive(Clone, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub ix: i32,
    pub iy: i32,
    /// RGB555 colour of the centre.
    pub near_color: u16,
    /// RGB555 colour of the rim.
    pub far_color: u16,
    /// `+6`: 0..=32 age/fade weight `w` (fed to [`particle_blob`]).
    pub w: u32,
}

/// One pixel of the blob: which colour and the weight of the DESTINATION
/// (`dst = (dst * weight + colour * (32 - weight)) / 32`).
#[derive(Clone, Copy, Debug)]
pub struct BlobPixel {
    pub near: bool,
    pub weight: u32,
}

/// The 7x7 weights of `G_Particle_Draw` for fade value `w`, row by row.
/// Transcribed from the decompilation (0x42f200), not re-verified against
/// the disassembly.
pub fn particle_blob(w: u32) -> [[BlobPixel; 7]; 7] {
    let center = if w > 6 { w - 7 } else { w };
    let mid = (w + 10).min(31);
    let outer = (w + 22).min(31);
    let inner = (w + 6).min(31);
    let f = |weight| BlobPixel { near: false, weight };
    let n = |weight| BlobPixel { near: true, weight };
    let row0 = [f(outer), f(outer), f(mid), f(mid), f(mid), f(outer), f(outer)];
    let row1 = [f(outer), f(mid), f(inner), f(inner), f(inner), f(mid), f(outer)];
    let row2 = [f(mid), f(inner), f(w), n(w), f(w), f(inner), f(mid)];
    let row3 = [f(mid), f(inner), n(w), n(center), n(w), f(inner), f(mid)];
    let row4 = [f(mid), f(inner), n(w), n(w), n(w), f(inner), f(mid)];
    [row0, row1, row2, row3, row4, row1, row0]
}

/// `G_ScoreBar` state for one player (`G_ScoreBar_Draw` redraws the parts
/// whose flag is set; the renderer can simply draw all of it every frame).
#[derive(Clone, Debug, Default)]
pub struct ScoreBar {
    /// The player is in the game (the bar is shown).
    pub shown: bool,
    pub score: i32,
    pub lives: i32,
    /// Shield bar fill, 0..=100 (eased towards the player's shields).
    pub shield_pct: f32,
    /// Power bar fill, 0..=100.
    pub power_pct: f32,
    /// The player definition's six score bar resource ids (`+0x30..` of the
    /// player def).
    pub def_copy: [u32; 6],
    /// Air weapon slots' {id, ammo} as `G_WeaponHandler::AirWeapon_GetScoreBarInfo`
    /// leaves them at bar+0x131 (3 x 2 dwords); id = 'none' when empty.
    pub weapons: [(u32, i32); 3],
}

/// The background for this frame (`G_Bgnd_CopyToFrontBuffer`, 0x410330).
#[derive(Clone, Debug, Default)]
pub struct Background {
    /// Image id of the level's background (`level+0x70`).
    pub image: u32,
    /// Size of the background buffer.
    pub width: i32,
    pub height: i32,
    /// Top of the window in the background buffer (`g_bgndScrollOffset`;
    /// decreases as the level scrolls).
    pub scroll_y: i32,
    /// `g_bgndSideScrolling`, -0x20..=0x1f.
    pub side_scroll: i32,
    /// Left of the window in the background buffer: `max(0, side_scroll + 0x20)`.
    pub src_left: i32,
    /// The window size (the game area): `perm_float(0x36)` x `perm_float(0x37)`.
    pub view_w: i32,
    pub view_h: i32,
    /// Rows the vertical scroll moved this frame.
    pub scrolled_this_frame: i32,
}

/// Everything the original would draw for one frame.
#[derive(Clone, Debug, Default)]
pub struct FrameView {
    /// Sprite draws, layers 0..=15 in draw order.
    pub items: Vec<DrawItem>,
    pub texts: Vec<TextItem>,
    pub particles: Vec<Particle>,
    pub background: Background,
    pub score_bars: [ScoreBar; 2],
}

impl FrameView {
    /// Render list group Back (layers 0, 1): items with `to_terrain` are
    /// stamped permanently into the background buffer (once; they are only
    /// emitted in the frame they are submitted); any other item here is
    /// drawn to the screen buffer before the background is copied over it,
    /// i.e. never seen.
    pub fn back(&self) -> impl Iterator<Item = &DrawItem> {
        self.items.iter().filter(|i| i.layer <= 1)
    }
    /// Group Middle (layers 2..=5), drawn over the background.
    pub fn middle(&self) -> impl Iterator<Item = &DrawItem> {
        self.items.iter().filter(|i| (2..=5).contains(&i.layer))
    }
    /// Group Front (layers 6..=15), drawn over the particles.
    pub fn front(&self) -> impl Iterator<Item = &DrawItem> {
        self.items.iter().filter(|i| i.layer >= 6)
    }
}

/// Sorts into draw order: render list `layer`, submission order within.
fn in_draw_order(items: Vec<DrawItem>) -> Vec<DrawItem> {
    let mut v: Vec<(usize, DrawItem)> = items.into_iter().enumerate().collect();
    v.sort_by_key(|(i, it)| (it.layer.clamp(0, 15), *i));
    v.into_iter().map(|(_, it)| it).collect()
}

fn player(g: &G, i: u32) -> u32 {
    g.m.r32(PLAYERS + i * 4)
}

/// `G_EG_BuildDrawList` (0x419060): per group, first the shadows of the
/// units whose definition has `+0x11c` set, then the units themselves. The
/// console's debug labels (0x4e349e..0x4e34a5) are not drawn.
pub fn G_EG_BuildDrawList(g: &mut G) {
    let groups = list::objects(&g.m, g.m.r32(0x4e0d70));
    for grp in groups {
        let kids = list::objects(&g.m, g.m.r32(grp + 0xa6));
        for &e in &kids {
            let def = g.m.r32(e + 0x8a);
            if g.m.r32(e + 0x1c) != NONE && g.m.r8(def + 0x11c) != 0 && g.m.ri32(e + 0xa4) <= 0 {
                g.m.w8(e + 0x38, 1);
                g.m.w8(e + 0x37, 0);
                super::gameobject::G_GameObject_Draw(g, e);
                g.m.w8(e + 0x37, 1);
            }
        }
        for &e in &kids {
            if g.m.r32(e + 0x1c) != NONE && g.m.ri32(e + 0xa4) <= 0 {
                g.m.w8(e + 0x38, 0);
                g.m.w8(e + 0x37, 1);
                super::gameobject::G_GameObject_Draw(g, e);
                g.m.w8(e + 0x38, 1);
            }
        }
    }
}

/// `G_MotionBlur_BuildDrawList` (0x42d870).
pub fn G_MotionBlur_BuildDrawList(g: &mut G) {
    let l = g.m.r32(super::motionblur::LIST);
    if l == 0 {
        return;
    }
    for o in list::objects(&g.m, l) {
        if o != 0 {
            super::gameobject::G_GameObject_Draw(g, o);
        }
    }
}

/// `G_WeaponHandler::BuildDrawList` (0x447ad0): the air weapon's crosshair
/// object (at `handler + 0x8d`).
pub fn G_WeaponHandler_BuildDrawList(g: &mut G, this: u32) {
    if g.m.r8(this + 0x117) != 0 {
        super::gameobject::G_GameObject_Draw(g, this + 0x8d);
    }
}

fn cstring(g: &G, a: u32) -> String {
    String::from_utf8_lossy(g.m.cstr(a)).into_owned()
}

/// `G_Player::BuildDrawList` (0x434e10).
pub fn G_Player_BuildDrawList(g: &mut G, this: u32, texts: &mut Vec<TextItem>) {
    if g.m.ri32(this + 0xba) != 4 {
        return;
    }
    G_WeaponHandler_BuildDrawList(g, this + 0x235);
    g.m.w8(this + 0x38, 1);
    g.m.w8(this + 0x37, 0);
    super::gameobject::G_GameObject_Draw(g, this);
    g.m.w8(this + 0x37, 1);
    g.m.w8(this + 0x38, 0);
    g.m.w8(this + 0x37, 1);
    super::gameobject::G_GameObject_Draw(g, this);
    g.m.w8(this + 0x38, 1);
    if g.m.r32(this + 0xd2) != 0 && g.m.r32(this + 0xda) < 0x20 {
        texts.push(TextItem {
            source: TextSource::PlayerMoney(g.m.ri32(this + 0xc2)),
            perm_index: 0x1e,
            text: cstring(g, this + 0xe6),
            blend: g.m.r32(this + 0xda),
            layer: 0xf,
            y_add: g.m.ri32(this + 0x1f2),
            alignment: None,
            field136_add: None,
        });
    }
}

/// `sub_421730` (0x421730): the end-of-level tally text.
fn sub_421730(g: &mut G, texts: &mut Vec<TextItem>) {
    if g.m.r32(EOL_STATE) != 0 && g.m.r32(EOL_ALPHA) < 0x20 {
        texts.push(TextItem {
            source: TextSource::EndOfLevel,
            perm_index: 0x35,
            text: cstring(g, EOL_TEXT),
            blend: g.m.r32(EOL_ALPHA),
            layer: 0xf,
            y_add: 0,
            alignment: None,
            field136_add: None,
        });
    }
}

/// `G_Notice_BuildDrawList` (0x42e350).
fn G_Notice_BuildDrawList(g: &mut G, texts: &mut Vec<TextItem>) {
    use super::notice::*;
    if g.m.r8(ACTIVE) != 0 && g.m.r8(TEXT) != 0 && g.m.ri32(COUNTDOWN) <= 0 {
        texts.push(TextItem {
            source: TextSource::Notice,
            perm_index: 0x31,
            text: cstring(g, TEXT),
            blend: g.m.r32(ALPHA),
            layer: 0xf,
            y_add: 0,
            alignment: Some(g.m.r32(0x4eba6e)),
            field136_add: Some(g.m.r32(ALPHA) as i32),
        });
    }
}

/// `G_Particle_Draw` (0x42f200) minus the pixel plotting.
fn live_particles(g: &mut G) -> Vec<Particle> {
    use super::particle::LIST;
    let mut out = Vec::new();
    let l = g.m.r32(LIST);
    if l == 0 || list::num(&g.m, l) < 1 {
        return out;
    }
    let side = bgnd::G_Bgnd_GetSideScrolling(g);
    let bound_x = ftol(G_Res_GetPermFloat(g, 0x36) as f64);
    let bound_y = ftol(G_Res_GetPermFloat(g, 0x37) as f64);
    for grp in list::objects(&g.m, l) {
        if g.m.ri32(grp + 0x418) > 0 {
            continue;
        }
        let count = g.m.ri32(grp + 0x41c);
        for j in 0..count.max(0) as u32 {
            let p = grp + 4 + j * 0x1a;
            if g.m.r8(p) == 0 {
                continue;
            }
            // +0xa is x, +0xe is y (G_Particle_Process adds the vertical
            // scroll to +0xe).
            let x = (g.m.rf32(p + 0xa) as f64 - side as f64) as f32;
            let y = g.m.rf32(p + 0xe);
            if !(x >= 0.0 && (x as f64 + 7.0) < bound_x as f64 && y >= 0.0 && (y as f64 + 7.0) < bound_y as f64) {
                continue;
            }
            out.push(Particle {
                x,
                y,
                ix: ftol(x as f64),
                iy: ftol(y as f64),
                near_color: g.m.r16(p + 2),
                far_color: g.m.r16(p + 4),
                w: g.m.r32(p + 6),
            });
        }
    }
    out
}

fn score_bar_view(g: &mut G, i: u32) -> ScoreBar {
    let b = scorebar::BARS + i * scorebar::BAR_SIZE;
    let mut s = ScoreBar::default();
    s.shown = g.m.r8(b + 0x12f) != 0;
    s.score = g.m.ri32(b + 0x18);
    s.lives = g.m.ri32(b + 0x1c);
    s.shield_pct = g.m.rf32(b + 0x20);
    s.power_pct = g.m.rf32(b + 0x24);
    for k in 0..6 {
        s.def_copy[k as usize] = g.m.r32(b + k * 4);
    }
    for k in 0..3 {
        s.weapons[k as usize] = (g.m.r32(b + 0x131 + k * 8), g.m.ri32(b + 0x131 + k * 8 + 4));
    }
    s
}

/// `sub_420740` (0x420740) as a view: call after the tick (and the time
/// increment), with the sink holding what the tick submitted already
/// (terrain stamps).
pub fn build_frame(g: &mut G, level: u32) -> FrameView {
    let mut texts = Vec::new();
    G_EG_BuildDrawList(g);
    G_MotionBlur_BuildDrawList(g);
    for i in 0..2 {
        let p = player(g, i);
        G_Player_BuildDrawList(g, p, &mut texts);
    }
    sub_421730(g, &mut texts);
    G_Notice_BuildDrawList(g, &mut texts);
    let items = g.draw_sink.take().unwrap_or_default();
    let w = ftol(G_Res_GetPermFloat(g, 0x36) as f64);
    let h = ftol(G_Res_GetPermFloat(g, 0x37) as f64);
    let side = g.m.ri32(bgnd::SIDE_SCROLL);
    let buf = g.m.r32(bgnd::BG_BUFFER);
    let (bw, bh) = pixbuf::dimensions(&g.m, buf);
    let background = Background {
        image: g.m.r32(level + 0x70),
        width: bw,
        height: bh,
        scroll_y: g.m.ri32(bgnd::SCROLL_OFFSET),
        side_scroll: side,
        src_left: (side + 0x20).max(0),
        view_w: w,
        view_h: h,
        scrolled_this_frame: g.m.ri32(bgnd::SCROLLED_THIS_FRAME),
    };
    let particles = live_particles(g);
    let score_bars = [score_bar_view(g, 0), score_bar_view(g, 1)];
    FrameView { items: in_draw_order(items), texts, particles, background, score_bars }
}
