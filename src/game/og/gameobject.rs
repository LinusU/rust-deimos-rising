//! Port of the original's gameobject module (G_GameObject, 0x423e30..0x424710).
//! `Draw`, `Priv_Draw` and `Priv_DrawShadow` are ported down to the sprite
//! settings they hand to `U_Sprite_RenderLists_Add`, which records them
//! (view.rs); they do nothing unless `g.draw_sink` is set (live play).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::bgnd::G_Bgnd_GetVerticalScrollingThisFrame;
use super::g::G;
use super::resource::G_Res_GetPermFloat;
use super::umath::ftol;

const NONE: u32 = 0x656e6f6e;

/// `U_Sprite_GetPtrToSpriteFrame` (0x40a6b0). The sprite frame record it
/// returns is not modelled; the game logic only tests it for null and hands
/// it to `U_Sprite_GetDimensionsForSpriteFrameData` (0x40a7f0). It is stood
/// in for by a token holding the (clamped) frame number; the sprite id comes
/// from the object that stores the token (`+0x1c`). Null when the sprite
/// group is missing, as the original returns null for an unknown group.
pub fn sprite_get_ptr_to_sprite_frame(g: &mut G, id: u32, frame: i32) -> u32 {
    if id == NONE {
        return 0;
    }
    let n = g.host.sprite_num_frames(id);
    if n < 1 {
        return 0;
    }
    // A frame past the end is reported and reset to 0.
    let frame = if frame >= n { 0 } else { frame };
    0x8000_0000 | (frame as u32 & 0x7fff_ffff)
}

/// `U_Sprite_GetDimensions` (0x40a8c0) / `U_Sprite_GetDimensionsForSpriteFrameData`
/// (0x40a7f0): the frame's size, multiplied by the scale (truncating) unless
/// the scale is exactly 1.0. Frame sizes come from `g.host.sprite_frame_size`.
pub fn sprite_get_dimensions(g: &mut G, id: u32, frame: i32, scale: f32) -> (i32, i32) {
    if id == NONE {
        return (0, 0);
    }
    let frame = {
        let n = g.host.sprite_num_frames(id);
        if frame >= n {
            0
        } else {
            frame
        }
    };
    let (w, h) = match g.host.sprite_frame_size(id, frame) {
        Some(d) => d,
        None => return (0, 0),
    };
    if scale == 1.0 {
        (w, h)
    } else {
        (ftol(w as f64 * scale as f64), ftol(h as f64 * scale as f64))
    }
}

/// `public: __thiscall G_GameObject::G_GameObject(void)` — 0x423e30.
pub fn G_GameObject_G_GameObject(g: &mut G, this: u32) {
    g.m.w32(this + 8, 0x499602d2);
    G_GameObject_SetDefaults(g, this);
}

/// `public: __thiscall G_GameObject::~G_GameObject(void)` — 0x423e60.
pub fn G_GameObject_dtor_G_GameObject(g: &mut G, this: u32) {}

/// `public: void __thiscall G_GameObject::SetDefaults(void)` — 0x423e70.
pub fn G_GameObject_SetDefaults(g: &mut G, this: u32) {
    let m = &mut g.m;
    let v = m.r32(0x4e5420);
    m.w32(this, v);
    let v = m.r32(0x4e5424);
    m.w32(this + 4, v);
    let v = m.r32(0x4e5420);
    m.w32(this + 0x10, v);
    let v = m.r32(0x4e5424);
    m.w32(this + 0x14, v);
    m.w32(this + 0xc, 0);
    m.w8(this + 0x19, 1);
    m.w8(this + 0x36, 0);
    m.w8(this + 0x37, 1);
    m.w8(this + 0x38, 1);
    m.w32(this + 0x1c, NONE);
    m.w32(this + 0x20, 0);
    let v = m.r32(0x4e5438);
    m.w32(this + 0x24, v);
    let v = m.r32(0x4e543c);
    m.w32(this + 0x28, v);
    let v = m.r32(0x4e5438);
    m.w32(this + 0x2c, v);
    let v = m.r32(0x4e543c);
    m.w32(this + 0x30, v);
    m.w32(this + 0x4e, 0);
    m.w8(this + 0x18, 1);
    m.w8(this + 0x1a, 0);
    m.w8(this + 0x52, 0);
    m.w32(this + 0x54, 0);
    m.w32(this + 0x58, 0);
    m.w32(this + 0x5c, 0);
    m.w32(this + 0x62, 0x42c80000);
    m.w32(this + 0x66, 0x42c80000);
    m.w32(this + 0x6a, 0);
    m.w8(this + 0x34, 1);
    m.w8(this + 0x35, 0);
    let v = m.r32(0x4e5453);
    m.w32(this + 0x3a, v);
    let v = m.r32(0x4e5457);
    m.w32(this + 0x3e, v);
    let v = m.r32(0x4e545b);
    m.w32(this + 0x42, v);
    let v = m.r32(0x4e545f);
    m.w32(this + 0x46, v);
    m.w32(this + 0x4a, 0x61666564);
    m.w8(this + 0x6e, 0);
    m.w32(this + 0x7a, 0x3f800000);
    m.w32(this + 0x7e, 0x3f800000);
    m.w32(this + 0x82, 0);
    m.w32(this + 0x86, 0);
}

/// `public: void __thiscall G_GameObject::AdjustVisibilityAndTinting(void)`
/// — 0x423f70. Moves the visibility (`+0x62`, towards `+0x66` by `+0x6a`)
/// and the tint (`+0x54`, towards `+0x58` by `+0x5c`). The "below 0" test
/// uses the double the x87 still holds after the f32 store.
pub fn G_GameObject_AdjustVisibilityAndTinting(g: &mut G, this: u32) {
    // Visibility.
    let a = g.m.rf32(this + 0x62);
    let b = g.m.rf32(this + 0x66);
    if a > b {
        let n = g.m.rf32(this + 0x62) as f64 - g.m.rf32(this + 0x6a) as f64;
        g.m.wf32(this + 0x62, n as f32);
        if n < 0.0 {
            g.m.w32(this + 0x62, 0);
        }
        let t = g.m.rf32(this + 0x66);
        if g.m.rf32(this + 0x62) < t {
            g.m.wf32(this + 0x62, t);
        }
    } else if a < b {
        let n = g.m.rf32(this + 0x62) as f64 + g.m.rf32(this + 0x6a) as f64;
        g.m.wf32(this + 0x62, n as f32);
        let t = g.m.rf32(this + 0x66);
        if g.m.rf32(this + 0x62) > t {
            g.m.wf32(this + 0x62, t);
        }
    }
    // Tint.
    let a = g.m.rf32(this + 0x54);
    let b = g.m.rf32(this + 0x58);
    if a > b {
        let n = g.m.rf32(this + 0x54) as f64 - g.m.rf32(this + 0x5c) as f64;
        g.m.wf32(this + 0x54, n as f32);
        if n < 0.0 {
            g.m.w32(this + 0x54, 0);
        }
        let t = g.m.rf32(this + 0x58);
        if g.m.rf32(this + 0x54) < t {
            g.m.wf32(this + 0x54, t);
        }
    } else if a < b {
        let n = g.m.rf32(this + 0x54) as f64 + g.m.rf32(this + 0x5c) as f64;
        g.m.wf32(this + 0x54, n as f32);
        let t = g.m.rf32(this + 0x58);
        if g.m.rf32(this + 0x54) > t {
            g.m.wf32(this + 0x54, t);
        }
    }
}

/// `public: void __thiscall G_GameObject::DoScaling(void)` — 0x424060.
/// Scale `+0x7a` moves towards the target `+0x7e` by `+0x82`, flagging the
/// dimensions (`+0x34`) for recalculation.
pub fn G_GameObject_DoScaling(g: &mut G, this: u32) {
    let a = g.m.rf32(this + 0x7a);
    let b = g.m.rf32(this + 0x7e);
    if a > b {
        g.m.w8(this + 0x34, 1);
        let n = g.m.rf32(this + 0x7a) as f64 - g.m.rf32(this + 0x82) as f64;
        g.m.wf32(this + 0x7a, n as f32);
        let t = g.m.rf32(this + 0x7e);
        if g.m.rf32(this + 0x7a) < t {
            g.m.wf32(this + 0x7a, t);
        }
    } else if a < b {
        g.m.w8(this + 0x34, 1);
        let n = g.m.rf32(this + 0x7a) as f64 + g.m.rf32(this + 0x82) as f64;
        g.m.wf32(this + 0x7a, n as f32);
        let t = g.m.rf32(this + 0x7e);
        if g.m.rf32(this + 0x7a) > t {
            g.m.wf32(this + 0x7a, t);
        }
    }
}

/// `public: struct U_Utils_Vector * __thiscall G_GameObject::GetLoc(void)` — 0x4240d0.
pub fn G_GameObject_GetLoc_4240d0(g: &mut G, this: u32) -> u32 {
    this
}

/// `public: void __thiscall G_GameObject::GetLoc(struct U_Utils_Vector *) const` — 0x4240e0.
pub fn G_GameObject_GetLoc_4240e0(g: &mut G, this: u32, a0: u32) {
    let x = g.m.r32(this);
    g.m.w32(a0, x);
    let y = g.m.r32(this + 4);
    g.m.w32(a0 + 4, y);
}

/// `public: void __thiscall G_GameObject::GetLoc(float *, float *) const` — 0x424100.
pub fn G_GameObject_GetLoc_424100(g: &mut G, this: u32, a0: u32, a1: u32) {
    let x = g.m.r32(this);
    g.m.w32(a0, x);
    let y = g.m.r32(this + 4);
    g.m.w32(a1, y);
}

/// `public: void __thiscall G_GameObject::SetLoc(struct U_Utils_Vector *)` — 0x424120.
pub fn G_GameObject_SetLoc_424120(g: &mut G, this: u32, a0: u32) {
    let x = g.m.r32(a0);
    g.m.w32(this, x);
    let y = g.m.r32(a0 + 4);
    g.m.w32(this + 4, y);
}

/// `public: void __thiscall G_GameObject::SetLoc(float, float)` — 0x424140.
pub fn G_GameObject_SetLoc_424140(g: &mut G, this: u32, a0: f32, a1: f32) {
    g.m.wf32(this, a0);
    g.m.wf32(this + 4, a1);
}

/// `public: void __thiscall G_GameObject::CalculateDimensions(void)` — 0x424160.
/// The sprite size comes from `U_Sprite_GetDimensionsForSpriteFrameData`
/// (0x40a7f0, when a frame record is cached in `+0x4e`) or
/// `U_Sprite_GetDimensions` (0x40a8c0); both map to `g.host.sprite_frame_size`
/// (see `sprite_get_dimensions`).
pub fn G_GameObject_CalculateDimensions(g: &mut G, this: u32) {
    if g.m.r8(this + 0x34) == 0 {
        return;
    }
    let id = g.m.r32(this + 0x1c);
    if id != NONE {
        let fd = g.m.r32(this + 0x4e);
        let scale = g.m.rf32(this + 0x7a);
        let (w, h) = if fd != 0 {
            sprite_get_dimensions(g, id, (fd & 0x7fff_ffff) as i32, scale)
        } else {
            let frame = g.m.ri32(this + 0x20);
            sprite_get_dimensions(g, id, frame, scale)
        };
        g.m.wi32(this + 0x24, w);
        g.m.wi32(this + 0x28, h);
        // Signed division by two, truncating toward zero.
        g.m.wi32(this + 0x2c, w / 2);
        g.m.wi32(this + 0x30, h / 2);
    } else {
        g.m.w32(this + 0x2c, 0);
        g.m.w32(this + 0x30, 0);
    }
    g.m.w8(this + 0x34, 0);
}

/// `x -/+ half` truncated: `fild half; fsubr/fadd [x]; fist` + fix-up.
fn bound(g: &G, v: u32, half: u32, add: bool) -> i32 {
    let h = g.m.ri32(half) as f64;
    let c = g.m.rf32(v) as f64;
    ftol(if add { h + c } else { c - h })
}

/// `public: void __thiscall G_GameObject::GetBounds(struct U_Rect *) const` — 0x424210.
/// U_Rect order is {top, left, bottom, right}.
pub fn G_GameObject_GetBounds_424210(g: &mut G, this: u32, a0: u32) {
    let left = bound(g, this, this + 0x2c, false);
    g.m.wi32(a0 + 4, left);
    let top = bound(g, this + 4, this + 0x30, false);
    g.m.wi32(a0, top);
    let right = bound(g, this, this + 0x2c, true);
    g.m.wi32(a0 + 0xc, right);
    let bottom = bound(g, this + 4, this + 0x30, true);
    g.m.wi32(a0 + 8, bottom);
}

/// `public: void __thiscall G_GameObject::GetBounds(int *, int *, int *, int *) const` — 0x424330.
pub fn G_GameObject_GetBounds_424330(g: &mut G, this: u32, a0: u32, a1: u32, a2: u32, a3: u32) {
    let left = bound(g, this, this + 0x2c, false);
    g.m.wi32(a0, left);
    let top = bound(g, this + 4, this + 0x30, false);
    g.m.wi32(a1, top);
    let right = bound(g, this, this + 0x2c, true);
    g.m.wi32(a2, right);
    let bottom = bound(g, this + 4, this + 0x30, true);
    g.m.wi32(a3, bottom);
}

/// `public: void __thiscall G_GameObject::GetDimensions(int *, int *) const` — 0x424460.
pub fn G_GameObject_GetDimensions(g: &mut G, this: u32, a0: u32, a1: u32) {
    let w = g.m.r32(this + 0x24);
    g.m.w32(a0, w);
    let h = g.m.r32(this + 0x28);
    g.m.w32(a1, h);
}

/// `public: void __thiscall G_GameObject::Glow_Start(unsigned short, int, bool)` — 0x424480.
pub fn G_GameObject_Glow_Start(g: &mut G, this: u32, a0: u16, a1: i32, a2: bool) {
    let active = g.m.r8(this + 0x6e) != 0;
    if (active && a2) || !active {
        g.m.w8(this + 0x6e, 1);
        g.m.w32(this + 0x70, 0x20);
        g.m.w8(this + 0x6f, 1);
        g.m.wi32(this + 0x74, a1);
        g.m.w16(this + 0x78, a0);
    }
}

/// `public: void __thiscall G_GameObject::Glow_Stop(void)` — 0x4244c0.
pub fn G_GameObject_Glow_Stop(g: &mut G, this: u32) {
    g.m.w8(this + 0x6e, 0);
}

/// `public: void __thiscall G_GameObject::Glow_Process(void)` — 0x4244d0.
/// The timer (`+0x70`) is compared unsigned.
pub fn G_GameObject_Glow_Process(g: &mut G, this: u32) {
    if g.m.r8(this + 0x6e) == 0 {
        return;
    }
    if g.m.r8(this + 0x6f) != 0 {
        let t = g.m.r32(this + 0x70).wrapping_sub(g.m.r32(this + 0x74));
        g.m.w32(this + 0x70, t);
        if t < 4 {
            g.m.w32(this + 0x70, 4);
            let f = (g.m.r8(this + 0x6f) == 0) as u8;
            g.m.w8(this + 0x6f, f);
        }
    } else {
        let t = g.m.r32(this + 0x70).wrapping_add(g.m.r32(this + 0x74));
        g.m.w32(this + 0x70, t);
        if t > 0x20 {
            g.m.w32(this + 0x70, 0x20);
            let f = (g.m.r8(this + 0x6f) == 0) as u8;
            g.m.w8(this + 0x6f, f);
            g.m.w8(this + 0x6e, 0);
        }
    }
}

/// `public: bool __thiscall G_GameObject::MoveAndCheckPosition(int, bool)` — 0x424530.
/// Units without `+0x19` (the non-`air ` ones) drift with the scroll; then
/// the velocity is added. Returns whether the object is still within the
/// margin-extended screen (`a1`), or within the fixed bounds (not `a1`).
pub fn G_GameObject_MoveAndCheckPosition(g: &mut G, this: u32, a0: i32, a1: bool) -> bool {
    if g.m.r8(this + 0x19) == 0 {
        let s = G_Bgnd_GetVerticalScrollingThisFrame(g);
        let y = s as f64 + g.m.rf32(this + 4) as f64;
        g.m.wf32(this + 4, y as f32);
    }
    let x = g.m.rf32(this) as f64 + g.m.rf32(this + 0x10) as f64;
    g.m.wf32(this, x as f32);
    let y = g.m.rf32(this + 4) as f64 + g.m.rf32(this + 0x14) as f64;
    g.m.wf32(this + 4, y as f32);
    let sw = ftol(G_Res_GetPermFloat(g, 0x36) as f64);
    let sh = ftol(G_Res_GetPermFloat(g, 0x37) as f64);
    let mut ret = true;
    let hw = g.m.ri32(this + 0x2c) as f64;
    let hh = g.m.ri32(this + 0x30) as f64;
    let x = g.m.rf32(this) as f64;
    let y = g.m.rf32(this + 4) as f64;
    if a1 {
        let neg = a0.wrapping_neg() as f64;
        if neg > hw + x {
            ret = false;
        } else if x - hw > sw.wrapping_add(a0) as f64 {
            ret = false;
        } else if neg > y {
            ret = false;
        } else if y - hh > sh.wrapping_add(a0) as f64 {
            ret = false;
        }
    } else {
        let lo_x = g.m.rf32(0x4e59b8) as f64;
        let lo_y = g.m.rf32(0x4e59bc) as f64;
        if hw + x < lo_x {
            ret = false;
        } else if x - hw > sw.wrapping_add(0x20) as f64 {
            ret = false;
        } else if hh + y < lo_y {
            ret = false;
        } else if y - hh > sh as f64 {
            ret = false;
        }
    }
    ret
}

// ---------------------------------------------------------------------
// Drawing: the settings handed to U_Sprite_RenderLists_Add (view.rs).
// ---------------------------------------------------------------------

use super::bgnd::{G_Bgnd_GetSideScrolling, G_Bgnd_GetUpdateRectTop};
use super::game::{G_Game_GameTime_Get, G_Game_GetShadowsOption};
use super::view::{self, DrawKind, ObjInfo, Settings};

/// `G_GameObject::Draw` — 0x424710. Draws when the visibility is not below
/// or equal to 0: the shadow (if `+0x38` and the shadows option) then the
/// sprite (if `+0x37`).
pub fn G_GameObject_Draw(g: &mut G, this: u32) {
    if g.draw_sink.is_none() {
        return;
    }
    let vis = g.m.rf32(this + 0x62) as f64;
    let zero = g.m.rf64(0x4e59b0);
    if vis < zero || vis == zero {
        return;
    }
    if g.m.rb(this + 0x38) && G_Game_GetShadowsOption(g) {
        G_GameObject_Priv_DrawShadow(g, this);
    }
    if g.m.rb(this + 0x37) {
        G_GameObject_Priv_Draw(g, this);
    }
}

/// The render list layer for a material tag (`+0x4a`), as the switch at
/// 0x4248f0 maps it; None for tags it does not list.
fn layer_for_tag(tag: u32) -> Option<i32> {
    Some(match tag {
        0x20647568 => 0xf,         // 'hud '
        0x65776c70 => 9,           // 'plwe'
        0x66656c70 => 0xc,         // 'plef'
        0x68736c70 => 0xb,         // 'plsh'
        0x69686961 => 8,           // 'aihi'
        0x69687267 => 5,           // 'grhi'
        0x69756c70 => 0xd,         // 'plui'
        0x6f6c6961 => 7,           // 'ailo'
        0x6f6d7461 => 0xe,         // 'atmo'
        0x756f7267 => 3,           // 'grou'
        0x79616c70 => 0xa,         // 'play'
        _ => return None,
    })
}

/// `G_GameObject::Priv_Draw` — 0x424760: the sprite, the hit flash and the
/// glow.
pub fn G_GameObject_Priv_Draw(g: &mut G, this: u32) {
    let terrain = g.m.rb(this + 0x36);
    let mut hud = 0i32;
    let mut rect_top = 0i32;
    if terrain {
        hud = 0x20;
        rect_top = G_Bgnd_GetUpdateRectTop(g);
    }
    let mut side = 0i32;
    if g.m.rb(this + 0x18) && !terrain {
        side = G_Bgnd_GetSideScrolling(g);
    }
    let mut s = Settings::template(g);
    s.frame_data = g.m.r32(this + 0x4e);
    s.sprite = g.m.r32(this + 0x1c);
    s.frame = g.m.ri32(this + 0x20);
    s.x = ftol(g.m.rf32(this) as f64).wrapping_sub(side).wrapping_add(hud);
    s.y = ftol(g.m.rf32(this + 4) as f64).wrapping_add(rect_top);
    s.scale = g.m.rf32(this + 0x7a);
    s.clip = [g.m.ri32(this + 0x3a), g.m.ri32(this + 0x3e), g.m.ri32(this + 0x42), g.m.ri32(this + 0x46)];
    s.direct = g.m.r8(this + 0x35) != 0;
    if !terrain {
        let mut tag = g.m.r32(this + 0x4a);
        if tag == 0 || tag == NONE {
            g.m.w32(this + 0x4a, 0x61666564);
            tag = 0x61666564;
        }
        if tag == 0x61666564 {
            s.layer = if g.m.r8(this + 0x19) != 0 { 7 } else { 3 };
        } else if let Some(l) = layer_for_tag(tag) {
            s.layer = l;
        }
    } else {
        let time = G_Game_GameTime_Get(g);
        if time > g.m.ri32(this + 0x86) {
            s.layer = 1;
            s.flags |= 8;
            s.clip = view::bg_buffer_rect(g);
            g.m.wi32(this + 0x86, time);
        }
    }
    let vis = g.m.rf32(this + 0x62) as f64;
    if !(vis == g.m.rf64(0x4e59c0)) {
        s.flags |= 1;
        let pct = ftol(vis);
        s.color = view::visibility_to_blend(g, pct);
    }
    let info = view::obj_info(g, this);
    if g.m.r8(this + 0x52) == 0 {
        view::render_lists_add(g, &s, DrawKind::Body, info);
    }
    // The hit flash.
    let flash_pct = g.m.rf32(this + 0x54) as f64;
    if flash_pct > g.m.rf64(0x4e59b0) {
        let t1 = ((flash_pct / g.m.rf64(0x4e59c0)) * g.m.rf64(0x4e59c8)) as f32;
        let k32 = g.m.rf32(0x4e59d0) as f64;
        let t2 = (g.m.rf64(0x4e59d8) - (s.color as f64 / k32)) as f32;
        let mut flash = t1 as f64 * t2 as f64 - k32;
        if flash < g.m.rf64(0x4e59b0) {
            flash = -flash;
        }
        s.color = ftol(flash) as u32;
        s.flags = 0;
        if terrain {
            s.flags |= 8;
        }
        s.flags |= 4;
        s.tint = g.m.r16(this + 0x60);
        view::render_lists_add(g, &s, DrawKind::Flash, info);
    }
    // The glow.
    if g.m.r8(this + 0x6e) != 0 {
        s.flags = 0;
        if terrain {
            s.flags |= 8;
        }
        s.flags |= 4;
        s.color = g.m.r32(this + 0x70);
        s.tint = g.m.r16(this + 0x78);
        view::render_lists_add(g, &s, DrawKind::Glow, info);
    }
}

/// `G_GameObject::Priv_DrawShadow` — 0x424c40. The shadow's offset is the
/// permanent floats 0x30/0x31 (ground, scaled by half the object's scale)
/// or 0x32/0x33 (elevated, the object's scale), by material tag; the
/// settings' scale is whatever that computation left in its slot.
pub fn G_GameObject_Priv_DrawShadow(g: &mut G, this: u32) {
    let terrain = g.m.rb(this + 0x36);
    let mut hud = 0i32;
    let mut rect_top = 0i32;
    if terrain {
        hud = 0x20;
        rect_top = G_Bgnd_GetUpdateRectTop(g);
    }
    let mut side = 0i32;
    if g.m.rb(this + 0x18) && !terrain {
        side = G_Bgnd_GetSideScrolling(g);
    }
    let mut s = Settings::template(g);
    s.frame_data = g.m.r32(this + 0x4e);
    s.x = ftol(g.m.rf32(this) as f64);
    s.y = ftol(g.m.rf32(this + 4) as f64);
    s.sprite = g.m.r32(this + 0x1c);
    s.frame = g.m.ri32(this + 0x20);
    s.clip = [g.m.ri32(this + 0x3a), g.m.ri32(this + 0x3e), g.m.ri32(this + 0x42), g.m.ri32(this + 0x46)];
    s.direct = g.m.r8(this + 0x35) != 0;
    s.flags |= 2;
    let mut blend: u32 = 0x14;
    let vis = g.m.rf32(this + 0x62) as f64;
    if !(vis == g.m.rf64(0x4e59c0)) {
        let pct = ftol(vis);
        blend = view::visibility_to_blend(g, pct);
        if blend < 0x14 {
            blend = 0x14;
        }
    }
    s.color = blend;
    let mut tag = g.m.r32(this + 0x4a);
    if tag == 0 || tag == NONE {
        g.m.w32(this + 0x4a, 0x61666564);
    }
    tag = g.m.r32(this + 0x4a);
    let m_scale = g.m.rf32(this + 0x7a);
    let half = g.m.rf32(0x4e59e0) as f64;
    let some19 = g.m.r8(this + 0x19) != 0;
    let some1a = g.m.r8(this + 0x1a) != 0;
    // (dx, dy) with the scale slot the settings share.
    let ground = |g: &mut G, s: &mut Settings| -> (i32, i32) {
        let slot = (m_scale as f64 * half) as f32;
        s.scale = slot;
        if some1a {
            let dx = ftol(G_Res_GetPermFloat(g, 0x30) as f64 * slot as f64);
            let dy = ftol(G_Res_GetPermFloat(g, 0x31) as f64 * slot as f64);
            (dx, dy)
        } else {
            let dx = ftol(G_Res_GetPermFloat(g, 0x30) as f64 * half);
            let dy = ftol(G_Res_GetPermFloat(g, 0x31) as f64 * half);
            (dx, dy)
        }
    };
    let elevated = |g: &mut G, s: &mut Settings| -> (i32, i32) {
        s.scale = m_scale;
        let dx = ftol(G_Res_GetPermFloat(g, 0x32) as f64 * m_scale as f64);
        let dy = ftol(G_Res_GetPermFloat(g, 0x33) as f64 * m_scale as f64);
        (dx, dy)
    };
    let default_tag = tag == 0x61666564;
    let (mut dx, mut dy);
    if default_tag {
        if some19 {
            s.layer = 6;
            (dx, dy) = ground(g, &mut s);
        } else {
            s.layer = 2;
            (dx, dy) = elevated(g, &mut s);
        }
    } else if some19 {
        (dx, dy) = ground(g, &mut s);
    } else {
        (dx, dy) = elevated(g, &mut s);
    }
    if terrain {
        s.layer = 0;
        s.x = ftol(dx as f64 + g.m.rf32(this) as f64 - side as f64 + hud as f64);
        s.y = ftol(dy as f64 + g.m.rf32(this + 4) as f64 + rect_top as f64);
        s.flags |= 8;
        s.clip = view::bg_buffer_rect(g);
    } else {
        if !default_tag {
            match tag {
                0x756f7267 => {
                    s.layer = 2;
                    (dx, dy) = elevated(g, &mut s);
                }
                0x69687267 => {
                    s.layer = 4;
                    (dx, dy) = elevated(g, &mut s);
                }
                0x20647568 | 0x65776c70 | 0x66656c70 | 0x68736c70 | 0x69686961 | 0x69756c70 | 0x6f6c6961
                | 0x6f6d7461 | 0x79616c70 => {
                    s.layer = 6;
                    (dx, dy) = ground(g, &mut s);
                }
                _ => {}
            }
        }
        s.x = ftol(dx as f64 + g.m.rf32(this) as f64 - side as f64);
        s.y = ftol(dy as f64 + g.m.rf32(this + 4) as f64);
    }
    let info = view::obj_info(g, this);
    view::render_lists_add(g, &s, DrawKind::Shadow, info);
}
