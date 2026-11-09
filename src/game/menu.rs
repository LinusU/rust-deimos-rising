//! Menu screens (G_Interface, G_LevelSelection, G_Scores, G_Credits).
//!
//! The original draws its menus from three things: full-screen `im16`
//! backdrops, the `mebu`/`mebh` button sprites (one frame per caption, a
//! normal and a highlighted set), and strings in the `tesm` bitmap font
//! placed by `tefo` text formats. This module rebuilds the list of things
//! to draw every frame and keeps one Bevy sprite per item.
//!
//! The layout helpers (`title_hit`, `sector_hit`, ...) are shared with the
//! flow system in `render.rs`, which owns the menu state and its input.

use super::input::{key_label, KeyBindings, ACTIONS};
use super::render::{
    frame_image_blend, image16_handle, layout_text, to_world_z, Blit, Flow, FlowState, GameSim,
    HighScores, TextureBook, FONT,
};
use super::sim::{pf, PLAY_LEFT};
use crate::assets::GameAssets;
use crate::pak::{id, ResId, NONE_ID};
use bevy::prelude::*;
use std::collections::HashMap;

/// Menus draw above the game and its frame.
const Z_MENU: f32 = 300.0;

/// Screen centre x.
const CENTER_X: f32 = 320.0;

// ---------------------------------------------------------------------------
// Mouse
// ---------------------------------------------------------------------------

/// Mouse position in 640x480 screen coordinates, and whether the left
/// button was pressed this frame.
#[derive(Resource, Default)]
pub struct MenuPointer {
    pub pos: Option<Vec2>,
    pub clicked: bool,
    /// The mouse moved this frame (so hovering should take the selection).
    pub moved: bool,
}

pub fn gather_pointer(
    windows: Query<&Window>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut pointer: ResMut<MenuPointer>,
) {
    let pos = windows.iter().next().and_then(|w| {
        let p = w.cursor_position()?;
        // The window may be resized; the game is always 640x480.
        Some(Vec2::new(p.x * 640.0 / w.width().max(1.0), p.y * 480.0 / w.height().max(1.0)))
    });
    pointer.moved = pos.is_some() && pos != pointer.pos;
    pointer.pos = pos;
    pointer.clicked = buttons.just_pressed(MouseButton::Left);
}

// ---------------------------------------------------------------------------
// Layout shared with the flow system
// ---------------------------------------------------------------------------

/// What a title-screen button does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleItem {
    OnePlayer,
    TwoPlayers,
    Controls,
    Scores,
    Demos,
    Quit,
}

/// Title buttons, top to bottom: frame in the `mebu`/`mebh` button sprites
/// and what it does. The original's list (G_Interface, VA 0x4277e0) is the
/// same minus Controls, whose button frame the Windows build left unused
/// because it configured keys in a native dialog; Preferences and Register
/// are left out here.
pub const TITLE_ITEMS: [(i64, TitleItem); 6] = [
    (0, TitleItem::OnePlayer),
    (1, TitleItem::TwoPlayers),
    (3, TitleItem::Controls),
    (4, TitleItem::Scores),
    (5, TitleItem::Demos),
    (6, TitleItem::Quit),
];

/// Button sprites: normal and highlighted.
const BUTTONS: &str = "mebu";
const BUTTONS_HILITE: &str = "mebh";

/// Centre of title button `i`: `Interface_Btn_StartYLoc` (pf 0x3e) plus
/// `Interface_Btn_VerticalGap` (pf 0x3f) per list position. The logo is
/// element 0 of the original's list, so the first button is at position 1.
pub fn title_item_center(assets: &GameAssets, i: usize) -> Vec2 {
    Vec2::new(CENTER_X, pf(assets, 0x3e) + pf(assets, 0x3f) * (i + 1) as f32)
}

/// Screen rectangle of a sprite frame drawn centred on `center`.
fn sprite_rect(assets: &mut GameAssets, rid: ResId, frame: i64, center: Vec2) -> Rect {
    let (w, h) = assets
        .sprite(rid)
        .and_then(|s| s.frames.get(frame as usize))
        .map(|f| (f.width as f32, f.height as f32))
        .unwrap_or((0.0, 0.0));
    Rect::from_center_size(center, Vec2::new(w, h))
}

/// The title button under `pos`, if any.
pub fn title_hit(assets: &mut GameAssets, pos: Vec2) -> Option<usize> {
    (0..TITLE_ITEMS.len()).find(|&i| {
        let center = title_item_center(assets, i);
        sprite_rect(assets, id(BUTTONS), TITLE_ITEMS[i].0, center).contains(pos)
    })
}

/// The three parts of the sector-selection screen that can be clicked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SectorHit {
    Previous,
    Current,
    Next,
}

/// Rect names for the three preview slots (`reli` Rects list).
const SECTOR_RECTS: [(&str, SectorHit); 3] = [
    ("LevSel_Button_Previous", SectorHit::Previous),
    ("LevSel_Button_Current", SectorHit::Current),
    ("LevSel_Button_Next", SectorHit::Next),
];

fn named_rect(assets: &GameAssets, name: &str) -> Option<Rect> {
    let r = assets.rect_list.as_ref()?.get(name)?;
    Some(Rect::new(r.left as f32, r.top as f32, r.right as f32, r.bottom as f32))
}

/// The sector-selection slot under `pos`, if any.
pub fn sector_hit(assets: &GameAssets, pos: Vec2) -> Option<SectorHit> {
    SECTOR_RECTS
        .iter()
        .find(|(name, _)| named_rect(assets, name).map(|r| r.contains(pos)).unwrap_or(false))
        .map(|(_, hit)| *hit)
}

/// Rows and columns of the Controls screen: one row per action, one
/// column per player.
pub const CONTROLS_ROWS: usize = ACTIONS.len();
const CONTROLS_TOP: f32 = 150.0;
const CONTROLS_GAP: f32 = 22.0;
const CONTROLS_LABEL_X: f32 = 110.0;
const CONTROLS_KEY_X: [f32; 2] = [340.0, 460.0];

/// The Controls cell (row, player column) under `pos`, if any.
pub fn controls_hit(pos: Vec2) -> Option<(usize, usize)> {
    let row = ((pos.y - CONTROLS_TOP + 4.0) / CONTROLS_GAP).floor();
    if row < 0.0 || row >= CONTROLS_ROWS as f32 {
        return None;
    }
    let col = (0..2).find(|&c| pos.x >= CONTROLS_KEY_X[c] - 10.0 && pos.x < CONTROLS_KEY_X[c] + 110.0)?;
    Some((row as usize, col))
}

/// One page of the credits: title, body lines, and how long it stays up.
pub struct CreditsPage {
    pub title: String,
    pub lines: Vec<String>,
    pub ticks: u32,
}

/// Split the `cred` string list into pages. A `<title>` line opens a page
/// and `<page N>` closes it, N being its display time in ticks.
pub fn credits_pages(assets: &GameAssets) -> Vec<CreditsPage> {
    let mut pages = Vec::new();
    let mut page = CreditsPage { title: String::new(), lines: Vec::new(), ticks: 240 };
    let Some(list) = assets.strings.get(&id("cred")) else {
        return pages;
    };
    for line in &list.strings {
        if let Some(title) = line.strip_prefix("<title>") {
            page.title = title.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("<page") {
            page.ticks = rest.trim_end_matches('>').trim().parse().unwrap_or(240);
            pages.push(std::mem::replace(
                &mut page,
                CreditsPage { title: String::new(), lines: Vec::new(), ticks: 240 },
            ));
        } else {
            page.lines.push(line.trim_end().to_string());
        }
    }
    if !page.title.is_empty() || page.lines.iter().any(|l| !l.trim().is_empty()) {
        pages.push(page);
    }
    pages
}

// ---------------------------------------------------------------------------
// Text styles
// ---------------------------------------------------------------------------

/// A `tefo` text format, as far as the menus use one.
#[derive(Clone)]
pub struct TextStyle {
    /// Location; what it anchors depends on `align`.
    pub pos: Vec2,
    /// `LEFT`, `RIGH`, `CENT`, `CEBU` (centre on screen) or `CEGA`
    /// (centre in the game area).
    pub align: String,
    pub monospaced: bool,
    pub spacing: i32,
    /// 0..=32, how much of the background shows through.
    pub blend: u32,
    /// Flat colour to draw the glyphs in; `None` keeps the font's own.
    pub color: Option<Color>,
}

/// Read text format `fmt_id`.
pub fn text_style(assets: &GameAssets, fmt_id: &str) -> TextStyle {
    let Some(fmt) = assets.text_formats.get(&id(fmt_id)) else {
        return TextStyle {
            pos: Vec2::ZERO,
            align: "LEFT".into(),
            monospaced: false,
            spacing: 0,
            blend: 0,
            color: None,
        };
    };
    let flag = |key: &str| fmt.get(key).map(|v| v.trim() == "TRUE").unwrap_or(false);
    // Alignment is a four-character code; a few formats use the numeric
    // form the comments in the data explain (3 = centre in buffer,
    // 4 = centre in game area).
    let raw = fmt.get("Format").unwrap_or("LEFT").trim().to_uppercase();
    let align = match raw.as_str() {
        "3" => "CEBU".to_string(),
        "4" => "CEGA".to_string(),
        other => other.chars().take(4).collect(),
    };
    let color = flag("Colorise_Do").then(|| {
        let rgb = u32::from_str_radix(fmt.get("ColoriseColor").unwrap_or("ffffff").trim(), 16)
            .unwrap_or(0xffffff);
        Color::srgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    });
    TextStyle {
        pos: Vec2::new(fmt.get_int("Loc_X") as f32, fmt.get_int("Loc_Y") as f32),
        align,
        monospaced: flag("Monospaced"),
        spacing: fmt.get_int("SpaceBetweenChars") as i32,
        blend: (fmt.get_int("BlendAmount_0To32").max(0) as u32).min(32),
        color,
    }
}

impl TextStyle {
    fn at(&self, x: f32, y: f32) -> TextStyle {
        TextStyle { pos: Vec2::new(x, y), ..self.clone() }
    }
    fn at_y(&self, y: f32) -> TextStyle {
        TextStyle { pos: Vec2::new(self.pos.x, y), ..self.clone() }
    }
    fn colored(&self, color: Color) -> TextStyle {
        TextStyle { color: Some(color), ..self.clone() }
    }
    fn blended(&self, blend: u32) -> TextStyle {
        TextStyle { blend: blend.min(32), ..self.clone() }
    }
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

/// One thing to draw: an image centred at a screen position.
struct Draw {
    image: Handle<Image>,
    center: Vec2,
    /// Stretch to this size (solid rectangles); `None` draws at image size.
    size: Option<Vec2>,
    color: Color,
    scale: f32,
}

/// Collects the frame's draws, in back-to-front order.
struct Painter<'a> {
    assets: &'a mut GameAssets,
    images: &'a mut Assets<Image>,
    book: &'a mut TextureBook,
    out: Vec<Draw>,
}

impl Painter<'_> {
    /// Full-colour image centred on `center`.
    fn image(&mut self, rid: ResId, center: Vec2) {
        if let Some(image) = image16_handle(self.assets, self.images, self.book, rid) {
            self.out.push(Draw { image, center, size: None, color: Color::WHITE, scale: 1.0 });
        }
    }

    /// Full-colour image with its top-left corner at `corner`.
    fn image_at(&mut self, rid: ResId, corner: Vec2, color: Color) {
        let Some((w, h)) = self.assets.image16(rid).map(|i| (i.0 as f32, i.1 as f32)) else {
            return;
        };
        if let Some(image) = image16_handle(self.assets, self.images, self.book, rid) {
            let center = corner + Vec2::new(w, h) * 0.5;
            self.out.push(Draw { image, center, size: None, color, scale: 1.0 });
        }
    }

    /// One sprite frame centred on `center`.
    fn sprite(&mut self, rid: ResId, frame: i64, center: Vec2, scale: f32, blend: u32) {
        if let Some(image) =
            frame_image_blend(self.assets, self.images, self.book, rid, frame, blend, Blit::Sprite)
        {
            self.out.push(Draw { image, center, size: None, color: Color::WHITE, scale });
        }
    }

    /// Solid rectangle.
    fn rect(&mut self, rect: Rect, color: Color) {
        self.out.push(Draw {
            image: Handle::default(),
            center: rect.center(),
            size: Some(rect.size()),
            color,
            scale: 1.0,
        });
    }

    /// A string in the game font.
    fn text(&mut self, style: &TextStyle, text: &str) {
        let glyphs = layout_text(
            self.assets,
            text,
            style.pos,
            &style.align,
            style.monospaced,
            style.spacing,
        );
        let (blit, color) = match style.color {
            Some(c) => (Blit::Flat, c),
            None => (Blit::Sprite, Color::WHITE),
        };
        for g in glyphs {
            if let Some(image) = frame_image_blend(
                self.assets, self.images, self.book, id(FONT), g.frame, style.blend, blit,
            ) {
                self.out.push(Draw { image, center: g.center, size: None, color, scale: 1.0 });
            }
        }
    }

    /// Interface string `index` from string list `list`.
    fn string(&self, list: &str, index: usize) -> String {
        self.assets
            .strings
            .get(&id(list))
            .and_then(|s| s.strings.get(index))
            .cloned()
            .unwrap_or_default()
    }
}

/// Marks a sprite belonging to the menu draw list.
#[derive(Component)]
pub struct MenuSprite(usize);

/// Rebuild the current screen's draw list and sync it to sprites.
#[allow(clippy::too_many_arguments)]
pub fn sync_menu(
    mut commands: Commands,
    flow: Res<Flow>,
    sim_res: Res<GameSim>,
    scores: Res<HighScores>,
    bindings: Res<KeyBindings>,
    pointer: Res<MenuPointer>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &MenuSprite, &mut Sprite, &mut Transform)>,
) {
    let mut p = Painter { assets: &mut assets, images: &mut images, book: &mut book, out: Vec::new() };
    match flow.state {
        FlowState::Title => title(&mut p, &flow),
        FlowState::Sectors => sectors(&mut p, &flow, &pointer),
        FlowState::Briefing => briefing(&mut p, &flow),
        FlowState::Scores | FlowState::NameEntry => score_table(&mut p, &flow, &scores),
        FlowState::Credits => credits(&mut p, &flow),
        FlowState::Controls => controls(&mut p, &flow, &bindings),
        FlowState::Playing => overlay(&mut p, &flow, &sim_res),
    }
    let draws = p.out;

    let mut existing: HashMap<usize, Entity> = q.iter().map(|(e, m, _, _)| (m.0, e)).collect();
    for (i, d) in draws.into_iter().enumerate() {
        let pos = to_world_z(d.center.x, d.center.y, Z_MENU + i as f32 * 0.01);
        let scale = Vec3::splat(d.scale);
        match existing.remove(&i).and_then(|e| q.get_mut(e).ok()) {
            Some((_, _, mut sprite, mut transform)) => {
                sprite.image = d.image;
                sprite.color = d.color;
                sprite.custom_size = d.size;
                transform.translation = pos;
                transform.scale = scale;
            }
            None => {
                commands.spawn((
                    Sprite { image: d.image, color: d.color, custom_size: d.size, ..default() },
                    Transform::from_translation(pos).with_scale(scale),
                    MenuSprite(i),
                ));
            }
        }
    }
    for (_, e) in existing {
        commands.entity(e).despawn();
    }
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

const SCREEN_CENTER: Vec2 = Vec2::new(320.0, 240.0);

/// Title screen (G_Interface): the `menu` backdrop, the game logo, the
/// button column, then the web address and copyright lines.
fn title(p: &mut Painter, flow: &Flow) {
    p.image(id("back"), SCREEN_CENTER);
    let logo = text_style(p.assets, "ingl");
    p.sprite(id("galo"), 0, Vec2::new(CENTER_X, logo.pos.y), 1.0, 0);
    for (i, (frame, _)) in TITLE_ITEMS.iter().enumerate() {
        let face = if flow.menu_sel == i { BUTTONS_HILITE } else { BUTTONS };
        let center = title_item_center(p.assets, i);
        p.sprite(id(face), *frame, center, 1.0, 0);
    }
    let web = p.string("inte", 0);
    let style = text_style(p.assets, "inwe");
    p.text(&style, &web);
    let copyright = p.string("inte", 1);
    let style = text_style(p.assets, "inco");
    p.text(&style, &copyright);
}

/// Sector selection (G_LevelSelection): the `lese` frame with the
/// previous, current and next sectors' previews in its three slots.
fn sectors(p: &mut Painter, flow: &Flow, pointer: &MenuPointer) {
    p.image(id("lese"), SCREEN_CENTER);
    let count = flow.level_ids.len();
    let unlocked = |i: usize| i as u32 <= flow.progress;
    let hover = pointer.pos.and_then(|pos| sector_hit(p.assets, pos));

    // Previews: LevSel_FirstPreviewX/YLoc (pf 0x27/0x28), spaced by
    // LevSel_PreviewX/YSpacing (pf 0x29/0x2a).
    let first = Vec2::new(pf(p.assets, 0x27), pf(p.assets, 0x28));
    let spacing = Vec2::new(pf(p.assets, 0x29), pf(p.assets, 0x2a));
    for slot in 0..3usize {
        let Some(i) = (flow.sector + slot).checked_sub(1).filter(|&i| i < count) else {
            continue;
        };
        let preview = p.assets.levels.get(&flow.level_ids[i]).map(|l| l.preview_image);
        let Some(rid) = preview.filter(|r| *r != NONE_ID) else { continue };
        // Sectors not reached yet are shown darkened.
        let shade = if unlocked(i) { 1.0 } else { 0.35 };
        p.image_at(rid, first + spacing * slot as f32, Color::srgb(shade, shade, shade));
    }

    // Border around the current preview (text format `lsbo`: colour and
    // blend only, one pixel outside the slot).
    if let Some(r) = named_rect(p.assets, "LevSel_Button_Current") {
        let border = text_style(p.assets, "lsbo");
        let color = border
            .color
            .unwrap_or(Color::WHITE)
            .with_alpha(1.0 - border.blend as f32 / 32.0);
        let o = Rect::new(r.min.x - 1.0, r.min.y - 1.0, r.max.x + 1.0, r.max.y + 1.0);
        p.rect(Rect::new(o.min.x, o.min.y, o.max.x, o.min.y + 1.0), color);
        p.rect(Rect::new(o.min.x, o.max.y - 1.0, o.max.x, o.max.y), color);
        p.rect(Rect::new(o.min.x, o.min.y + 1.0, o.min.x + 1.0, o.max.y - 1.0), color);
        p.rect(Rect::new(o.max.x - 1.0, o.min.y + 1.0, o.max.x, o.max.y - 1.0), color);
    }

    // Buttons over the previews: LevelSelect_Notice_* positions (pf
    // 0x5a..0x5f) and LevelSelect_SpriteFrame_* frames (pf 0x60..0x63);
    // the one under the mouse is highlighted and scaled by pf 0x64.
    let hilite_scale = pf(p.assets, 0x64);
    let mut button = |p: &mut Painter, hit: SectorHit, frame_pf: usize, x_pf: usize| {
        let frame = pf(p.assets, frame_pf) as i64;
        let center = Vec2::new(pf(p.assets, x_pf), pf(p.assets, x_pf + 1));
        let hot = hover == Some(hit);
        let face = if hot { BUTTONS_HILITE } else { BUTTONS };
        p.sprite(id(face), frame, center, if hot { hilite_scale } else { 1.0 }, 0);
    };
    if flow.sector > 0 {
        button(p, SectorHit::Previous, 0x60, 0x5a);
    }
    let start_frame = if unlocked(flow.sector) { 0x61 } else { 0x62 };
    button(p, SectorHit::Current, start_frame, 0x5c);
    if flow.sector + 1 < count {
        button(p, SectorHit::Next, 0x63, 0x5e);
    }

    // Sector number in the circle, name in the bar underneath.
    let locked = !unlocked(flow.sector);
    let number = text_style(p.assets, if locked { "lsnn" } else { "lsnu" });
    p.text(&number, &format!("{:02}", flow.sector + 1));
    if locked {
        let style = text_style(p.assets, "lsna");
        let text = p.string("pgsl", 4); // "Sector Not Reached"
        p.text(&style, &text);
    } else {
        let name = flow
            .level_ids
            .get(flow.sector)
            .and_then(|r| p.assets.levels.get(r))
            .map(|l| l.name.clone())
            .unwrap_or_default();
        let style = text_style(p.assets, "lsde");
        p.text(&style, &name);
    }

    // The two instruction strings take turns along the bottom edge.
    let hint = p.string("pgsl", if (flow.ticks / 180) % 2 == 0 { 1 } else { 2 });
    let style = text_style(p.assets, "inco");
    p.text(&style, &hint);
}

/// Briefing before a sector: its name, description and map credit beside
/// its preview (text formats `brti`, `brno`, `brpr`).
fn briefing(p: &mut Painter, flow: &Flow) {
    p.image(id("back"), SCREEN_CENTER);
    let Some(level) = flow.level_ids.get(flow.sector).and_then(|r| p.assets.levels.get(r)).cloned()
    else {
        return;
    };
    let title = text_style(p.assets, "brti");
    p.text(&title, &format!("Sector {:02}:  {}", flow.sector + 1, level.name));
    let body = text_style(p.assets, "brno");
    let gap = pf(p.assets, 0x4b); // Credits_VerticalGap
    let mut y = body.pos.y + gap * 2.0;
    for line in wrap(&level.description, 36).into_iter().chain([String::new(), level.copyright]) {
        p.text(&body.at_y(y), &line);
        y += gap;
    }
    if level.preview_image != NONE_ID {
        let preview = text_style(p.assets, "brpr");
        p.image_at(level.preview_image, preview.pos, Color::WHITE);
    }
}

/// Break `text` into lines of at most `width` characters at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(word.to_string());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

/// High-score table (G_Scores): name, score and sector columns, one row
/// every `Scores_VerticalGap` (pf 0x4c) from `Scores_YLoc` (pf 0x4d). The
/// row being entered or just entered uses the highlighted formats.
fn score_table(p: &mut Painter, flow: &Flow, scores: &HighScores) {
    p.image(id("back"), SCREEN_CENTER);
    for (fmt, title) in [("scnt", "Name"), ("scst", "Score"), ("sset", "Sector")] {
        let style = text_style(p.assets, fmt);
        p.text(&style, title);
    }
    let top = pf(p.assets, 0x4d);
    let gap = pf(p.assets, 0x4c);
    let entering = flow.state == FlowState::NameEntry;
    // While a name is being typed its row is previewed where it will land.
    let pending = flow.pending_scores.first().filter(|_| entering);
    let insert_at = pending.map(|ps| {
        scores.entries.iter().position(|e| ps.score > e.score).unwrap_or(scores.entries.len())
    });
    // (name, score, sector, highlighted), top to bottom.
    let mut rows: Vec<(String, i64, String, bool)> = Vec::new();
    for (i, e) in scores.entries.iter().enumerate() {
        rows.push((e.name.clone(), e.score, e.level.clone(), flow.score_hilite == Some(i)));
    }
    if let (Some(at), Some(ps)) = (insert_at, pending) {
        // Flashing cursor after the typed name (game string 7 is "_").
        let cursor = if (flow.ticks / 20) % 2 == 0 { "_" } else { "" };
        rows.insert(at, (format!("{}{}", flow.name_text, cursor), ps.score, ps.level.clone(), true));
    }
    for (row, (name, score, level, hilite)) in rows.iter().take(15).enumerate() {
        let y = top + gap * row as f32;
        let fmts = if *hilite { ["scph", "scsh", "sseh"] } else { ["scpn", "scsc", "scse"] };
        let style = text_style(p.assets, fmts[0]);
        p.text(&style.at_y(y), name);
        let style = text_style(p.assets, fmts[1]);
        p.text(&style.at_y(y), &score.to_string());
        let style = text_style(p.assets, fmts[2]);
        p.text(&style.at_y(y), level);
    }
    if entering {
        let style = text_style(p.assets, "inco");
        p.text(&style, "Type your name, then press Enter");
    }
}

/// Credits (G_Credits): one page of the `cred` list at a time, its title
/// at `Credits_TitleYLoc` (pf 0x4a) and a line every `Credits_VerticalGap`
/// (pf 0x4b).
fn credits(p: &mut Painter, flow: &Flow) {
    p.image(id("back"), SCREEN_CENTER);
    let pages = credits_pages(p.assets);
    let Some(page) = pages.get(flow.credits_page) else { return };
    let top = pf(p.assets, 0x4a);
    let gap = pf(p.assets, 0x4b);
    let title = text_style(p.assets, "crti");
    p.text(&title.at_y(top), &page.title);
    let body = text_style(p.assets, "crno");
    for (i, line) in page.lines.iter().enumerate() {
        p.text(&body.at_y(top + gap * (i + 1) as f32), line);
    }
}

/// Controls: every action with each player's key. Not an original screen
/// (Windows used a native dialog), so it borrows the credits' formats.
fn controls(p: &mut Painter, flow: &Flow, bindings: &KeyBindings) {
    p.image(id("back"), SCREEN_CENTER);
    let title = text_style(p.assets, "crti");
    let body = text_style(p.assets, "crno");
    let hilite = text_style(p.assets, "lsnn").color.unwrap_or(Color::srgb(1.0, 0.0, 0.0));
    p.text(&title.at(CONTROLS_LABEL_X, CONTROLS_TOP - 50.0), "Controls");
    for (col, name) in ["Player 1", "Player 2"].iter().enumerate() {
        p.text(&title.at(CONTROLS_KEY_X[col], CONTROLS_TOP - 26.0), name);
    }
    for (row, (_, label)) in ACTIONS.iter().enumerate() {
        let y = CONTROLS_TOP + CONTROLS_GAP * row as f32;
        p.text(&body.at(CONTROLS_LABEL_X, y), label);
        for col in 0..2 {
            let selected = flow.controls_sel == (row, col);
            let text = if selected && flow.rebinding {
                "Press a key".to_string()
            } else {
                key_label(bindings.keys[col][row])
            };
            let style = body.at(CONTROLS_KEY_X[col], y);
            if selected {
                p.text(&style.colored(hilite), &text);
            } else {
                p.text(&style, &text);
            }
        }
    }
    let hint = text_style(p.assets, "inco");
    let text = if flow.rebinding {
        "Press the new key, or Esc to keep the old one"
    } else {
        "Arrows choose  -  Enter changes the key  -  Esc returns"
    };
    p.text(&hint, text);
}

/// Text drawn over the game: the level-end tally and the pause notice,
/// centred in the game area with the game-notice format (`gano`).
fn overlay(p: &mut Painter, flow: &Flow, sim_res: &GameSim) {
    let notice = text_style(p.assets, "gano");
    // Game-area text formats are in front-buffer space.
    let style = TextStyle { pos: notice.pos + Vec2::new(PLAY_LEFT, 0.0), ..notice };
    let gap = pf(p.assets, 0x4b);
    let lines = super::render::end_text_lines(&sim_res.0, p.assets);
    let top = style.pos.y - gap * lines.len().saturating_sub(1) as f32 * 0.5;
    for (i, line) in lines.iter().enumerate() {
        game_area_text(p, &style.at_y(top + gap * i as f32), line);
    }
    if flow.paused {
        // Game string 0: "Press Caps Lock".
        let text = p.string("pgsl", 0);
        game_area_text(p, &style.blended(8), &text);
    }
}

/// Draw text whose format centres it in the game area, shifting the
/// front-buffer x the layout produces onto the screen.
fn game_area_text(p: &mut Painter, style: &TextStyle, text: &str) {
    let first = p.out.len();
    p.text(style, text);
    if style.align == "CEGA" {
        for d in &mut p.out[first..] {
            d.center.x += PLAY_LEFT;
        }
    }
}
