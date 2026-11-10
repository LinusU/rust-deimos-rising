//! G_Background (0x40fbe0..0x410650): vertical scrolling, the side-scroll
//! offset, map spawns by row and the media mask. Drawing stays in the
//! renderer.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::eg::G_EG_CheckForMapSpawnsAtRow;
use super::g::G;
use super::pixbuf;
use super::resource::G_Res_GetPermFloat;
use super::umath::ftol;

// Globals.
pub const NUM_ROWS: u32 = 0x4de6ec; // g_bgndNumRows
pub const SCROLL_OFFSET: u32 = 0x4de6f4; // g_bgndScrollOffset
pub const UPDATE_RECT_BOTTOM: u32 = 0x4de6fc; // g_bgndUpdateRectBottom
pub const CURRENT_ROW: u32 = 0x4de704; // g_bgndCurrentRow
pub const VERTICAL_SCROLL: u32 = 0x4de708; // g_bgndVerticalScroll
pub const LEVEL_COMPLETE: u32 = 0x4de70c; // g_bgndLevelComplete (byte)
pub const SIDE_SCROLL: u32 = 0x4de70d; // g_bgndSideScrolling
pub const SIDE_SCROLL_MAXED: u32 = 0x4de711;
pub const MEDIA_MASK: u32 = 0x4de716; // U_Image* of the media mask
pub const TILE_SIZE: u32 = 0x4de71a; // g_bgndTileSize
pub const SCROLLED_THIS_FRAME: u32 = 0x4de71e; // g_bgndVerticalScrollingThisFrame
pub const NO_MAP_SPAWNS: u32 = 0x4de722; // g_bgndInitialMapSpawnsDone (byte)
/// The display's background pixel buffer (`[0x50102b]`).
pub const BG_BUFFER: u32 = 0x50102b;

/// `(int)G_Res_GetPermFloat(i)` as the original converts it.
fn pf_int(g: &mut G, i: i32) -> i32 {
    ftol(G_Res_GetPermFloat(g, i) as f64)
}

/// `G_Bgnd_DoInitialMapSpawns` (0x40fc20): every row from the update
/// rect's bottom up to `scrollOffset - 0x40`, bottom first.
pub fn G_Bgnd_DoInitialMapSpawns(g: &mut G) {
    if g.m.rb(NO_MAP_SPAWNS) {
        return;
    }
    let count = g.m.ri32(UPDATE_RECT_BOTTOM).wrapping_sub(g.m.ri32(SCROLL_OFFSET).wrapping_sub(0x41));
    let mut i = 0;
    while i < count {
        let row = g.m.ri32(UPDATE_RECT_BOTTOM).wrapping_sub(i);
        G_EG_CheckForMapSpawnsAtRow(g, row);
        i += 1;
    }
}

/// `G_Bgnd_ResetAtLevelStart` (0x40fc70).
pub fn G_Bgnd_ResetAtLevelStart(g: &mut G, level: u32) {
    g.m.w8(0x4de715, 0);
    g.m.w32(VERTICAL_SCROLL, 1);
    g.m.w8(LEVEL_COMPLETE, 0);
    g.m.w32(SCROLLED_THIS_FRAME, 0);
    g.m.w32(SIDE_SCROLL, 0);
    g.m.w32(SIDE_SCROLL_MAXED, 0);
    G_Bgnd_Load(g, level);
    let v = g.m.r32(level + 0x60);
    g.m.w32(0x4de6e4, v);
    let v = g.m.r32(level + 0x64);
    g.m.w32(0x4de6e8, v);
    let v = g.m.r32(level + 0x68);
    g.m.w32(NUM_ROWS, v);
    let v = g.m.r32(level + 0x6c);
    g.m.w32(0x4de6f0, v);
    g.m.w32(0x4de6f8, 0x20);
    if g.m.ri32(0x4de6f8) < 0 {
        g.m.w32(0x4de6f8, 0);
    }
    let h = pf_int(g, 0x37);
    let v = g.m.ri32(NUM_ROWS).wrapping_sub(h);
    g.m.wi32(SCROLL_OFFSET, v);
    let w = pf_int(g, 0x36);
    let v = w.wrapping_add(g.m.ri32(0x4de6f8));
    g.m.wi32(0x4de700, v);
    let h = pf_int(g, 0x37);
    let v = h.wrapping_add(g.m.ri32(SCROLL_OFFSET));
    g.m.wi32(UPDATE_RECT_BOTTOM, v);
    let h = pf_int(g, 0x37);
    g.m.wi32(CURRENT_ROW, h.wrapping_add(1));
}

/// `G_Bgnd_Load` (0x40fe40), minus the drawing: sizes the display's
/// background buffer to the level image and loads the media mask, whose
/// tiles cover `bgWidth / maskWidth` pixels.
pub fn G_Bgnd_Load(g: &mut G, level: u32) {
    let bg_id = g.m.r32(level + 0x70);
    let (bw, bh) = g.host.image_size(bg_id).unwrap_or((0, 0));
    let buf = match g.m.r32(BG_BUFFER) {
        0 => {
            let b = pixbuf::new(&mut g.m);
            g.m.w32(BG_BUFFER, b);
            b
        }
        b => b,
    };
    pixbuf::create(&mut g.m, buf, bw, bh);
    // Media mask: the original loads image [level+0x280] (or 'none' when
    // it is missing) into a U_Image at 0x4de716.
    let mut mask_id = g.m.r32(level + 0x280);
    // 0x40ff82: U_Pak_Tag_IsInIndex('im16', id); `has_image` would test 'im08'.
    if !g.host.has_tag(0x36316d69, mask_id) {
        mask_id = super::unitdef::NONE;
    }
    let mask = pixbuf::new(&mut g.m);
    match g.host.image_pixels16(mask_id) {
        Some((w, h, px)) => {
            pixbuf::create(&mut g.m, mask, w, h);
            pixbuf::fill16(&mut g.m, mask, &px);
        }
        None => pixbuf::create(&mut g.m, mask, 0, 0),
    }
    g.m.w32(MEDIA_MASK, mask);
    let (mw, _) = pixbuf::dimensions(&g.m, mask);
    let tile = if mw != 0 { bw / mw } else { 0 };
    g.m.wi32(TILE_SIZE, tile);
}

/// `G_Bgnd_GetUpdateRectTop` (0x410100).
pub fn G_Bgnd_GetUpdateRectTop(g: &mut G) -> i32 {
    g.m.ri32(SCROLL_OFFSET)
}

/// `G_Bgnd_GetVerticalScrollingThisFrame` (0x410110).
pub fn G_Bgnd_GetVerticalScrollingThisFrame(g: &mut G) -> i32 {
    g.m.ri32(SCROLLED_THIS_FRAME)
}

/// `G_Bgnd_MediaMask_GetSurfaceAtLoc` (0x410120): the media class under a
/// map location (`U_Utils_IntVector*`), from the mask pixel at
/// location / tileSize through the jump table at 0x4bf46f (pixel values
/// 0x1f.. map to surfaces). Returns 0 off the mask.
pub fn G_Bgnd_MediaMask_GetSurfaceAtLoc(g: &mut G, loc: u32) -> i32 {
    let (x, y) = (g.m.ri32(loc), g.m.ri32(loc + 4));
    let mask = g.m.r32(MEDIA_MASK);
    let (w, h) = pixbuf::dimensions(&g.m, mask);
    let tile = g.m.ri32(TILE_SIZE);
    if tile == 0 {
        return 0;
    }
    let tx = x / tile;
    let ty = y / tile;
    if tx < 0 || tx >= w || ty < 0 || ty >= h {
        return 0;
    }
    let (base, row) = pixbuf::base_and_rowbytes(&g.m, mask);
    let px = g.m.r16(base.wrapping_add((ty * row) as u32).wrapping_add((tx * 2) as u32)) as i32;
    media_surface_for_pixel(g, px)
}

/// The switch at 0x4101c0: `jmp [edx*4 + 0x4bf46f]` with
/// `edx = pixel - 0x1f`, unsigned-bounded by 0 (so only pixel 0x1f is in
/// range and anything else returns 0). Each case sets the result.
fn media_surface_for_pixel(g: &mut G, px: i32) -> i32 {
    let idx = px.wrapping_sub(0x1f) as u32;
    if idx > 0 {
        return 0;
    }
    let target = g.m.r32(0x4bf46f + idx * 4);
    // The only case lands on 0x4101c7: `mov esi, 1`.
    if target == 0x4101c7 {
        1
    } else {
        0
    }
}

/// `G_Bgnd_ResumeVerticalScrolling` (0x4101f0).
pub fn G_Bgnd_ResumeVerticalScrolling(g: &mut G) {
    if !g.m.rb(LEVEL_COMPLETE) {
        g.m.w32(VERTICAL_SCROLL, 1);
    }
}

/// `G_Bgnd_StopVerticalScrolling` (0x410210).
pub fn G_Bgnd_StopVerticalScrolling(g: &mut G) {
    g.m.w32(VERTICAL_SCROLL, 0);
}

/// `G_Bgnd_IsVerticalScrollingPaused` (0x410220).
pub fn G_Bgnd_IsVerticalScrollingPaused(g: &mut G) -> bool {
    g.m.r32(VERTICAL_SCROLL) == 0
}

/// `G_Bgnd_Process` (0x410230): scroll, then 1 when the level's last row
/// has been reached, else spawn the map objects of row
/// `scrollOffset - 0x40`.
pub fn G_Bgnd_Process(g: &mut G) -> i32 {
    let mut result = 0;
    sub_410490(g);
    if g.m.r32(VERTICAL_SCROLL) == 0 {
        if g.m.rb(LEVEL_COMPLETE) {
            result = 1;
        }
    } else {
        let rows = g.m.ri32(NUM_ROWS);
        if g.m.ri32(CURRENT_ROW) >= rows {
            g.m.wi32(CURRENT_ROW, rows);
            g.m.w8(LEVEL_COMPLETE, 1);
            result = 1;
            g.m.w32(VERTICAL_SCROLL, 0);
        } else if !g.m.rb(NO_MAP_SPAWNS) {
            let row = g.m.ri32(SCROLL_OFFSET).wrapping_add(-0x40);
            G_EG_CheckForMapSpawnsAtRow(g, row);
        }
    }
    result
}

/// `G_Bgnd_GetSideScrolling` (0x4102b0).
pub fn G_Bgnd_GetSideScrolling(g: &mut G) -> i32 {
    g.m.ri32(SIDE_SCROLL)
}

/// `G_Bgnd_AdjustSideScroll` (0x4102c0): 0 = left, else right; the offset
/// stays within -0x20..0x1f, and the "maxed" flag records the direction
/// of a move that was not clamped.
pub fn G_Bgnd_AdjustSideScroll(g: &mut G, dir: i32) {
    g.m.w32(SIDE_SCROLL_MAXED, 0);
    let delta: i32 = if dir == 0 { -1 } else { 1 };
    let v = g.m.ri32(SIDE_SCROLL).wrapping_add(delta);
    g.m.wi32(SIDE_SCROLL, v);
    if delta < 0 {
        if g.m.ri32(SIDE_SCROLL) < -0x20 {
            g.m.wi32(SIDE_SCROLL, -0x20);
        } else {
            g.m.wi32(SIDE_SCROLL_MAXED, -1);
        }
    } else if g.m.ri32(SIDE_SCROLL) > 0x1f {
        g.m.wi32(SIDE_SCROLL, 0x1f);
    } else {
        g.m.wi32(SIDE_SCROLL_MAXED, 1);
    }
}

/// `sub_410490`: one tick of vertical scrolling. The offset and update
/// rect move up by the speed, the current row advances (clamped to the
/// map), the offset stops at 0 and the update rect at the background's
/// bottom; the distance actually moved becomes
/// `g_bgndVerticalScrollingThisFrame`.
pub fn sub_410490(g: &mut G) {
    let speed = g.m.ri32(VERTICAL_SCROLL);
    if speed == 0 {
        g.m.w32(SCROLLED_THIS_FRAME, 0);
        return;
    }
    let before = g.m.ri32(SCROLL_OFFSET);
    g.m.wi32(SCROLL_OFFSET, before.wrapping_sub(speed));
    let v = g.m.ri32(UPDATE_RECT_BOTTOM).wrapping_sub(speed);
    g.m.wi32(UPDATE_RECT_BOTTOM, v);
    let v = g.m.ri32(CURRENT_ROW).wrapping_add(speed);
    g.m.wi32(CURRENT_ROW, v);
    let rows = g.m.ri32(NUM_ROWS);
    if g.m.ri32(CURRENT_ROW) > rows {
        g.m.wi32(CURRENT_ROW, rows);
    } else if g.m.ri32(CURRENT_ROW) < 0 {
        g.m.wi32(CURRENT_ROW, 0);
    }
    if g.m.ri32(SCROLL_OFFSET) <= 0 {
        g.m.wi32(SCROLL_OFFSET, 0);
        let h = pf_int(g, 0x37);
        g.m.wi32(UPDATE_RECT_BOTTOM, h);
    }
    let buf = g.m.r32(BG_BUFFER);
    let bottom = pixbuf::rect(&g.m, buf).2;
    if g.m.ri32(UPDATE_RECT_BOTTOM) > bottom {
        g.m.wi32(UPDATE_RECT_BOTTOM, bottom);
        let h = pf_int(g, 0x37);
        g.m.wi32(SCROLL_OFFSET, bottom.wrapping_sub(h));
    }
    let after = g.m.ri32(SCROLL_OFFSET);
    g.m.wi32(SCROLLED_THIS_FRAME, before.wrapping_sub(after));
}
