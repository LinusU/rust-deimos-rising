//! Stand-in for the platform `U_PixelBuffer`/`U_Image` objects the game
//! logic queries (W_PixelBuffer.obj): only dimensions, rect and pixel
//! access, which is all the ported code uses. Object layout (ours):
//! {width, height, base, rowBytes}.

use super::mem::Mem;

pub fn new(m: &mut Mem) -> u32 {
    m.alloc(16)
}

/// Size the buffer and allocate 16-bit pixels.
pub fn create(m: &mut Mem, b: u32, w: i32, h: i32) {
    // Re-creating a buffer (a new level's background) releases the old pixels.
    let old = m.r32(b + 8);
    if old != 0 {
        m.free(old);
    }
    m.wi32(b, w);
    m.wi32(b + 4, h);
    let row = w.max(0) * 2;
    let base = if w > 0 && h > 0 { m.alloc((row * h) as u32) } else { 0 };
    m.w32(b + 8, base);
    m.wi32(b + 12, row);
}

pub fn fill16(m: &mut Mem, b: u32, px: &[u16]) {
    let base = m.r32(b + 8);
    for (i, &p) in px.iter().enumerate() {
        m.w16(base + i as u32 * 2, p);
    }
}

/// `U_PixelBuffer::GetDimensions` (0x44ea30).
pub fn dimensions(m: &Mem, b: u32) -> (i32, i32) {
    if b == 0 {
        return (0, 0);
    }
    (m.ri32(b), m.ri32(b + 4))
}

/// `U_PixelBuffer::GetBaseAddrAndRowBytes` (0x44ea50).
pub fn base_and_rowbytes(m: &Mem, b: u32) -> (u32, i32) {
    (m.r32(b + 8), m.ri32(b + 12))
}

/// `U_PixelBuffer::GetRect` (0x44eac0) as U_Rect {top, left, bottom, right}.
pub fn rect(m: &Mem, b: u32) -> (i32, i32, i32, i32) {
    let (w, h) = dimensions(m, b);
    (0, 0, h, w)
}
