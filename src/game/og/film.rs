//! Port of the original's film module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `public: void __thiscall G_Film::GetInputs(enum G_Game_PlayerNum, struct G_Input_PlayerInputs *)` — 0x41e300, 208 bytes, decomp: complete. 
pub fn G_Film_GetInputs(g: &mut G, this: u32, a0: i32, a1: u32) {
    // Fields: currentFrame[2] at +2, records at +0x22 (stride 0x4eac):
    // {totalFrames, field1, field2, pad bytes...} with the pad at +0xc.
    let frame = g.m.ri32(this + (a0 as u32) * 4 + 2);
    let rec = this + (a0 as u32).wrapping_mul(0x4eac);
    if frame <= g.m.ri32(rec + 0x22) {
        let b = g.m.r8(rec + 0x2e + frame as u32) as u32;
        g.m.w8(a1 + 3, (b & 1 != 0) as u8);
        g.m.w8(a1 + 1, (b & 2 != 0) as u8);
        g.m.w8(a1, (b & 4 != 0) as u8);
        g.m.w8(a1 + 2, (b & 8 != 0) as u8);
        g.m.w8(a1 + 4, (b & 0x10 != 0) as u8);
        g.m.w8(a1 + 5, (b & 0x20 != 0) as u8);
        g.m.w8(a1 + 6, (b & 0x40 != 0) as u8);
        g.m.wi32(this + (a0 as u32) * 4 + 2, frame.wrapping_add(1));
    }
}

/// `public: void __thiscall G_Film::SetInputs(enum G_Game_PlayerNum, struct G_Input_PlayerInputs *, long)` — 0x41e3d0, 208 bytes, decomp: complete. 
pub fn G_Film_SetInputs(g: &mut G, this: u32, a0: i32, a1: u32, a2: i32) {
    let frame = g.m.ri32(this + (a0 as u32) * 4 + 2);
    if frame < 0x4e20 {
        let rec = this + (a0 as u32).wrapping_mul(0x4eac);
        let p = rec + 0x2e + frame as u32;
        g.m.w8(p, 0);
        let bits = [(3u32, 1u8), (1, 2), (0, 4), (2, 8), (4, 0x10), (5, 0x20), (6, 0x40)];
        for (off, bit) in bits {
            if g.m.r8(a1 + off) != 0 {
                let v = g.m.r8(p) | bit;
                g.m.w8(p, v);
            }
        }
        let f = frame.wrapping_add(1);
        g.m.wi32(this + (a0 as u32) * 4 + 2, f);
        g.m.wi32(rec + 0x22, f);
        g.m.wi32(rec + 0x26, a2.wrapping_add(0xb3ac2));
        let id = super::game::G_Game_GetLevelID(g);
        g.m.wi32(rec + 0x2a, id);
    }
}
