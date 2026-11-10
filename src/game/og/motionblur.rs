//! Port of the original's motionblur module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::gameobject::*;
use super::mem::list;

// Module state: pool of 1000 `G_MotionBlurObject`s (0x8a bytes each) plus
// the list of the live ones.
pub const LIST: u32 = 0x4e8eea;
pub const IN_MODULE: u32 = 0x4e8ee9;
pub const FREE_HINT: u32 = 0x4e8eee; // index of the last freed slot, -1 = none
pub const COUNT: u32 = 0x4e8ef2;
pub const WARNED: u32 = 0x4e8ef6;
pub const POOL: u32 = 0x4e8ef8; // 1000 x { byte used, i32 index, ptr object }
pub const REF_DEFAULT: u32 = 0x4e8a19;
pub const POOL_SIZE: i32 = 0x3e8;

fn slot(i: i32) -> u32 {
    POOL.wrapping_add((i as u32).wrapping_mul(10))
}

fn cp(g: &mut G, dst: u32, src: u32, off: u32, size: u32) {
    match size {
        1 => {
            let v = g.m.r8(src + off);
            g.m.w8(dst + off, v);
        }
        2 => {
            let v = g.m.r16(src + off);
            g.m.w16(dst + off, v);
        }
        _ => {
            let v = g.m.r32(src + off);
            g.m.w32(dst + off, v);
        }
    }
}

fn default_ref(g: &mut G, r: u32) {
    let v = g.m.r32(REF_DEFAULT);
    g.m.w32(r, v);
    let v = g.m.r32(REF_DEFAULT + 4);
    g.m.w32(r + 4, v);
}

/// `sub_42d9d0` (called from the module's init, not by the tick): builds
/// the pool of 1000 objects.
pub fn sub_42d9d0(g: &mut G) {
    g.m.clear(FREE_HINT, 0x271a);
    g.m.wi32(FREE_HINT, -1);
    g.m.w32(COUNT, 0);
    g.m.w8(WARNED, 0);
    let mut i = 0;
    while i < POOL_SIZE {
        let s = slot(i);
        g.m.w8(s, 0);
        g.m.wi32(s + 2, i);
        let o = g.m.alloc(0x8a);
        G_GameObject_G_GameObject(g, o);
        g.m.w32(s + 6, o);
        if o == 0 {
            panic!("G_MotionBlur.cc:0x19a pool allocation failed");
        }
        g.m.wi32(o + 0xc, i);
        i += 1;
    }
}

/// `void __cdecl G_MotionBlur_ResetAtLevelStart(void)` — 0x42d5b0, 112 bytes, decomp: complete. 
pub fn G_MotionBlur_ResetAtLevelStart(g: &mut G) {
    G_MotionBlur_sub_2d910(g);
    let l = list::new(&mut g.m);
    g.m.w32(LIST, l);
    if l == 0 {
        panic!("G_MotionBlur.cc:0x81 list allocation failed");
    }
    G_MotionBlur_sub_2dab0(g);
}

/// `void __cdecl G_MotionBlur_New(struct G_MotionBlur_Request *)` — 0x42d620, 400 bytes, decomp: complete. 
pub fn G_MotionBlur_New(g: &mut G, a0: u32) {
    let o = G_MotionBlur_Alloc(g);
    if o == 0 {
        return;
    }
    let l = g.m.r32(LIST);
    list::add(&mut g.m, l, o);
    let t = g.m.r32(a0);
    cp(g, o, t, 0x18, 1);
    cp(g, o, t, 0x19, 1);
    g.m.w8(o + 0x1a, 0);
    cp(g, o, t, 0x1c, 4);
    cp(g, o, t, 0x20, 4);
    cp(g, o, t, 0x24, 4);
    cp(g, o, t, 0x28, 4);
    cp(g, o, t, 0x2c, 4);
    cp(g, o, t, 0x30, 4);
    for off in 0x34..=0x39 {
        cp(g, o, t, off, 1);
    }
    cp(g, o, t, 0x3a, 2);
    cp(g, o, t, 0x3c, 4);
    cp(g, o, t, 0x40, 4);
    cp(g, o, t, 0x44, 4);
    cp(g, o, t, 0x48, 2);
    cp(g, o, t, 0x4a, 4);
    cp(g, o, t, 0x4e, 4);
    g.m.w8(o + 0x38, 0);
    cp(g, o, t, 0x52, 1);
    cp(g, o, t, 0x53, 1);
    cp(g, o, t, 0x54, 4);
    cp(g, o, t, 0x58, 4);
    cp(g, o, t, 0x5c, 4);
    cp(g, o, t, 0x60, 2);
    cp(g, o, t, 0x62, 4);
    cp(g, o, t, 0x66, 4);
    cp(g, o, t, 0x6a, 4);
    if g.m.r8(a0 + 0xc) != 0 {
        cp(g, o, t, 0x6e, 1);
        cp(g, o, t, 0x6f, 1);
        cp(g, o, t, 0x70, 4);
        cp(g, o, t, 0x74, 4);
        cp(g, o, t, 0x78, 2);
    }
    cp(g, o, t, 0x7a, 4);
    cp(g, o, t, 0x7e, 4);
    cp(g, o, t, 0x82, 4);
    let v = g.m.r32(a0 + 4);
    g.m.w32(o + 0x62, v);
    g.m.w32(o + 0x66, 0);
    let v = g.m.r32(a0 + 8);
    g.m.w32(o + 0x6a, v);
    let sp = g.m.sp;
    let loc = g.m.alloca(8);
    G_GameObject_GetLoc_4240e0(g, t, loc);
    G_GameObject_SetLoc_424120(g, o, loc);
    g.m.pop_to(sp);
}

/// `void __cdecl G_MotionBlur_Process(void)` — 0x42d7b0, 192 bytes, decomp: complete. 
pub fn G_MotionBlur_Process(g: &mut G) {
    let l = g.m.r32(LIST);
    let count = list::num(&g.m, l);
    if count < 1 {
        return;
    }
    let sp = g.m.sp;
    let r = g.m.alloca(8);
    default_ref(g, r);
    let mut i = 0;
    while i < count {
        list::next(&mut g.m, l, r);
        let o = g.m.r32(r + 4);
        if o == 0 {
            break;
        }
        let v = (g.m.rf32(o + 0x62) as f64 - g.m.rf32(o + 0x6a) as f64) as f32;
        g.m.wf32(o + 0x62, v);
        // fcomp [o+0x66]; and ah,5; cmp ah,1: below (not unordered)
        if (v as f64) < g.m.rf32(o + 0x66) as f64 {
            let link = g.m.r32(r);
            list::delete_link_ref(&mut g.m, l, link, r);
            G_MotionBlur_Free(g, o);
        }
        i += 1;
    }
    g.m.pop_to(sp);
}

/// `void __cdecl G_MotionBlur_sub_2d910(void)` — 0x42d910, 192 bytes, decomp: compiling. 
pub fn G_MotionBlur_sub_2d910(g: &mut G) {
    if !g.m.rb(IN_MODULE) || g.m.r32(LIST) == 0 {
        return;
    }
    let l = g.m.r32(LIST);
    let count = list::num(&g.m, l);
    if count > 0 {
        let sp = g.m.sp;
        let r = g.m.alloca(8);
        default_ref(g, r);
        let mut i = 0;
        while i < count {
            list::next(&mut g.m, l, r);
            if g.m.r32(r + 4) == 0 {
                break;
            }
            let link = g.m.r32(r);
            list::delete_link_ref(&mut g.m, l, link, r);
            i += 1;
        }
        g.m.pop_to(sp);
    }
    list::delete_all(&mut g.m, l);
    g.m.free(l);
    g.m.w32(LIST, 0);
}

/// `void __cdecl G_MotionBlur_sub_2dab0(void)` — 0x42dab0, 64 bytes, decomp: complete. 
pub fn G_MotionBlur_sub_2dab0(g: &mut G) {
    g.m.w32(FREE_HINT, 0);
    g.m.w32(COUNT, 0);
    g.m.w8(WARNED, 0);
    let mut i = 0;
    while i < POOL_SIZE {
        g.m.w8(slot(i), 0);
        i += 1;
    }
}

/// `void __cdecl G_MotionBlur_sub_2daf0(void)` — 0x42daf0, 160 bytes, decomp: compiling. 
pub fn G_MotionBlur_sub_2daf0(g: &mut G) {
    let mut i = 0;
    while i < POOL_SIZE {
        let s = slot(i);
        g.m.w8(s, 0);
        g.m.wi32(s + 2, -1);
        let o = g.m.r32(s + 6);
        if o != 0 {
            g.m.free(o);
            g.m.w32(s + 6, 0);
        }
        i += 1;
    }
    g.m.w8(0x4eb608, 0);
    g.m.wi32(FREE_HINT, -1);
    g.m.w32(COUNT, 0);
    g.m.w8(WARNED, 0);
}

/// `class G_MotionBlurObject * __cdecl G_MotionBlur_Alloc(void)` — 0x42db90, 272 bytes, decomp: compiling. 
pub fn G_MotionBlur_Alloc(g: &mut G) -> u32 {
    let mut result = 0;
    let count = g.m.ri32(COUNT);
    if count >= POOL_SIZE {
        if !g.m.rb(WARNED) {
            // U_App_Log + a console warning (0x4eb6b4 / 0x4eb6fe)
            g.m.w8(WARNED, 1);
        }
        return result;
    }
    let mut idx: i32 = -1;
    if g.m.ri32(FREE_HINT) == -1 {
        let mut i = 0;
        while i < POOL_SIZE {
            if g.m.r8(slot(i)) == 0 {
                idx = i;
                break;
            }
            i += 1;
        }
        if idx == -1 {
            panic!("G_MotionBlur.cc:0x1fe no free slot");
        }
    } else {
        idx = g.m.ri32(FREE_HINT);
    }
    if idx != -1 {
        g.m.wi32(COUNT, count.wrapping_add(1));
        let s = slot(idx);
        g.m.w8(s, 1);
        result = g.m.r32(s + 6);
        g.m.wi32(result + 0xc, idx);
        g.m.wi32(FREE_HINT, -1);
    }
    result
}

/// `void __cdecl G_MotionBlur_Free(class G_MotionBlurObject *)` — 0x42dca0, 48 bytes, decomp: complete. 
pub fn G_MotionBlur_Free(g: &mut G, a0: u32) {
    let idx = g.m.ri32(a0 + 0xc);
    g.m.w8(slot(idx), 0);
    let c = g.m.ri32(COUNT).wrapping_sub(1);
    g.m.wi32(COUNT, c);
    g.m.wi32(FREE_HINT, idx);
}
