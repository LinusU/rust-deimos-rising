//! Port of the original's debris module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::mem::list;

/// Debris module globals.
pub const IN_MODULE: u32 = 0x4dfd19; // bool
pub const LIST: u32 = 0x4dfd1a; // U_LinkedList*
/// The default `U_LinkRef` {link, object} the loops start from.
pub const REF_DEFAULT: u32 = 0x4dfcdb;

fn bgnd_scrolled(g: &mut G) -> i32 {
    super::bgnd::G_Bgnd_GetVerticalScrollingThisFrame(g)
}

/// `void __cdecl G_Debris_ResetAtLevelStart(void)` — 0x412b50, 112 bytes, decomp: complete. 
pub fn G_Debris_ResetAtLevelStart(g: &mut G) {
    sub_412d90(g);
    let l = list::new(&mut g.m);
    g.m.w32(LIST, l);
}

/// `void __cdecl G_Debris_New(struct U_Rect *)` — 0x412bc0, 128 bytes, decomp: complete. 
pub fn G_Debris_New(g: &mut G, a0: u32) {
    let d = g.m.alloc(0x10);
    let l = g.m.r32(LIST);
    list::add(&mut g.m, l, d);
    for k in 0..4 {
        let v = g.m.r32(a0 + k * 4);
        g.m.w32(d + k * 4, v);
    }
}

/// `void __cdecl G_Debris_Process(void)` — 0x412c40, 144 bytes, decomp: complete. 
pub fn G_Debris_Process(g: &mut G) {
    let l = g.m.r32(LIST);
    let count = list::num(&g.m, l);
    if count < 1 {
        return;
    }
    let delta = bgnd_scrolled(g);
    let sp = g.m.sp;
    let r = g.m.alloca(8);
    let v = g.m.r32(REF_DEFAULT);
    g.m.w32(r, v);
    let v = g.m.r32(REF_DEFAULT + 4);
    g.m.w32(r + 4, v);
    let mut i = 0;
    while i < count {
        list::next(&mut g.m, l, r);
        let d = g.m.r32(r + 4);
        if d == 0 {
            break;
        }
        let v = g.m.ri32(d).wrapping_add(delta);
        g.m.wi32(d, v);
        let v = g.m.ri32(d + 8).wrapping_add(delta);
        g.m.wi32(d + 8, v);
        i += 1;
    }
    g.m.pop_to(sp);
}

/// `bool __cdecl G_Debris_CheckCollision(struct U_Rect *)` — 0x412cd0, 160 bytes, decomp: complete. 
pub fn G_Debris_CheckCollision(g: &mut G, a0: u32) -> bool {
    let mut hit = false;
    let l = g.m.r32(LIST);
    let count = list::num(&g.m, l);
    if count >= 1 {
        let sp = g.m.sp;
        let r = g.m.alloca(8);
        let v = g.m.r32(REF_DEFAULT);
        g.m.w32(r, v);
        let v = g.m.r32(REF_DEFAULT + 4);
        g.m.w32(r + 4, v);
        let mut i = 0;
        while i < count {
            list::next(&mut g.m, l, r);
            let d = g.m.r32(r + 4);
            if d == 0 {
                break;
            }
            if g.m.ri32(a0 + 8) >= g.m.ri32(d)
                && g.m.ri32(a0) <= g.m.ri32(d + 8)
                && g.m.ri32(a0 + 0xc) >= g.m.ri32(d + 4)
                && g.m.ri32(a0 + 4) <= g.m.ri32(d + 0xc)
            {
                hit = true;
                break;
            }
            i += 1;
        }
        g.m.pop_to(sp);
    }
    hit
}

/// `_sub_412d90` — 0x412d90, 224 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_412d90(g: &mut G) {
    // G_Debris_DeleteAll
    if !g.m.rb(IN_MODULE) || g.m.r32(LIST) == 0 {
        return;
    }
    let l = g.m.r32(LIST);
    let count = list::num(&g.m, l);
    if count > 0 {
        let sp = g.m.sp;
        let r = g.m.alloca(8);
        let v = g.m.r32(REF_DEFAULT);
        g.m.w32(r, v);
        let v = g.m.r32(REF_DEFAULT + 4);
        g.m.w32(r + 4, v);
        let mut i = 0;
        while i < count {
            list::next(&mut g.m, l, r);
            let d = g.m.r32(r + 4);
            if d == 0 {
                break;
            }
            let link = g.m.r32(r);
            list::delete_link_ref(&mut g.m, l, link, r);
            g.m.free(d);
            i += 1;
        }
        g.m.pop_to(sp);
    }
    list::delete_all(&mut g.m, l);
    g.m.free(l);
    g.m.w32(LIST, 0);
}
