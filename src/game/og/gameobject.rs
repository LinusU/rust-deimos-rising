//! Port of the original's gameobject module (G_GameObject, 0x423e30..0x424710).
//! The drawing functions (`Draw`, `Priv_Draw`, `Priv_DrawShadow`) are not
//! ported: the renderer draws from the object's fields.
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

// Draw, Priv_Draw and Priv_DrawShadow (0x424710..) are rendering and are not
// ported: the renderer draws from the object's fields.
