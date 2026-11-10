//! Port of the original's particle module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::bgnd::G_Bgnd_GetVerticalScrollingThisFrame;
use super::mem::list;
use super::resource::G_Res_GetPermFloat;
use super::umath::ftol;

// Globals.
pub const IN_MODULE: u32 = 0x4ebc33;
pub const LIST: u32 = 0x4ebc34; // U_LinkedList* of groups
pub const REF_DEFAULT: u32 = 0x4ebb83;
/// Velocity tables (99 x {f32 x, f32 y}) filled at start-up by
/// G_Particle_LoadHeader, and the rolling indices into them.
pub const TABLE1: u32 = 0x4ebc38;
pub const TABLE2: u32 = 0x4ebf58;
pub const INDEX1: u32 = 0x4ec278;
pub const INDEX2: u32 = 0x4ec27c;
// Constants in .rdata: 32.0, 65535.0, 1.0 (double), 3.0, 5.0, -32.0, 7.0, 0.0.
const C_COLOR_DIV: u32 = 0x4ec280;
const C_COLOR_MUL: u32 = 0x4ec284;
const C_ONE: u32 = 0x4ec288;
const C_SPEED_A: u32 = 0x4ec290;
const C_SPEED_B: u32 = 0x4ec294;
const C_MIN: u32 = 0x4ec298;
const C_SEVEN: u32 = 0x4ec29c;
const C_ZERO: u32 = 0x4ec2a0;

fn default_ref(g: &mut G, r: u32) {
    let v = g.m.r32(REF_DEFAULT);
    g.m.w32(r, v);
    let v = g.m.r32(REF_DEFAULT + 4);
    g.m.w32(r + 4, v);
}

/// `U_Pixel16_RGBToPixel16` (0x44dfb0) on three u16 channels at `p`.
fn rgb_to_pixel16(g: &G, p: u32) -> u16 {
    let r = g.m.r16(p) as u32;
    let gr = g.m.r16(p + 2) as u32;
    let b = g.m.r16(p + 4) as u32;
    (((r >> 1) & 0x7c00) | ((gr >> 6) & 0x3e0) | (b >> 11)) as u16
}

/// `sub_42ffd0` (called by ResetAtLevelStart and by the module's shutdown):
/// frees every group and the list.
fn delete_all_groups(g: &mut G) {
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
            let grp = g.m.r32(r + 4);
            let link = g.m.r32(r);
            list::delete_link_ref(&mut g.m, l, link, r);
            g.m.free(grp);
            i += 1;
        }
        g.m.pop_to(sp);
    }
    list::delete_all(&mut g.m, l);
    g.m.free(l);
    g.m.w32(LIST, 0);
}

/// `void __cdecl G_Particle_ResetAtLevelStart(void)` — 0x42e890, 112 bytes, decomp: complete. 
pub fn G_Particle_ResetAtLevelStart(g: &mut G) {
    delete_all_groups(g);
    let l = list::new(&mut g.m);
    g.m.w32(LIST, l);
}

/// `void __cdecl G_Particle_NewGroup(struct G_Particle_Settings *)` — 0x42e900, 1536 bytes, decomp: compiling. 
pub fn G_Particle_NewGroup(g: &mut G, a0: u32) {
    let ty = g.m.r32(a0 + 0x10);
    if ty == 0x656e6f6e || ty == 0 {
        return;
    }
    // (grow flag [+0x416], count, flag [+0x417]) per particle type.
    let (flag2, count, flag1): (u8, i32, u8) = match ty {
        0x2064656d => (0, 0x14, 0),
        0x6772616c => (0, 0x28, 0),
        0x6963616c => (0, 0x28, 1),
        0x6963656d => (0, 0x14, 1),
        0x69636974 => (1, 5, 1),
        0x69636d73 => (1, 0xa, 1),
        0x6c616d73 => (1, 0xa, 0),
        0x796e6974 => (1, 5, 0),
        _ => return,
    };
    let grp = g.m.alloc(0x420);
    g.m.w32(grp, 0x499602d2);
    let l = g.m.r32(LIST);
    list::add(&mut g.m, l, grp);
    let v = g.m.r32(a0 + 0xa);
    g.m.w32(grp + 0x418, v);
    let v = g.m.r8(a0 + 0xe);
    g.m.w8(grp + 0x414, v);
    g.m.w8(grp + 0x416, flag2);
    g.m.wi32(grp + 0x41c, count);
    g.m.w8(grp + 0x415, 0);
    g.m.w8(grp + 0x417, flag1);
    let scale1 = G_Res_GetPermFloat(g, 0x91);
    let scale2 = G_Res_GetPermFloat(g, 0x92);
    let div = g.m.rf32(C_COLOR_DIV) as f64;
    let mul = g.m.rf32(C_COLOR_MUL) as f64;
    let one = g.m.rf64(C_ONE);
    let color = g.m.r16(a0 + 8) as u32;
    let mut hash1 = [0u16; 5];
    let mut hash2 = [0u16; 5];
    let mut i = 0;
    while i < 5 {
        let comp: [u16; 3] = [
            ((color >> 10) & 0x1f) as u16,
            ((color >> 5) & 0x1f) as u16,
            (color & 0x1f) as u16,
        ];
        let mut c = [0u16; 3];
        for k in 0..3 {
            c[k] = ftol(comp[k] as f64 / div * mul) as u16;
        }
        // factor = 1.0 - i * scale1, rounded to a float
        let factor = (one - (i as f64) * scale1 as f64) as f32;
        for k in 0..3 {
            c[k] = ftol(c[k] as f64 * factor as f64) as u16;
        }
        let mut d = [0u16; 3];
        for k in 0..3 {
            d[k] = ftol(c[k] as f64 * scale2 as f64) as u16;
        }
        let sp = g.m.sp;
        let t = g.m.alloca(12);
        for k in 0..3 {
            g.m.w16(t + k as u32 * 2, c[k]);
            g.m.w16(t + 6 + k as u32 * 2, d[k]);
        }
        hash1[i] = rgb_to_pixel16(g, t);
        hash2[i] = rgb_to_pixel16(g, t + 6);
        g.m.pop_to(sp);
        i += 1;
    }
    let va = g.m.r32(a0);
    let ve = g.m.r32(a0 + 4);
    let mut j = 0;
    while j < 0x28 {
        let p = grp + 4 + (j as u32) * 0x1a;
        if j >= count {
            g.m.w8(p, 0);
            j += 1;
            continue;
        }
        g.m.w8(p, 1);
        g.m.w32(p + 0xa, va);
        g.m.w32(p + 0xe, ve);
        let ri = g.random_int(0, 4) as usize;
        g.m.w16(p + 2, hash1[ri]);
        g.m.w16(p + 4, hash2[ri]);
        g.m.w32(p + 6, 0);
        g.m.w8(p + 1, 0);
        let (tbl, idx_addr) = if flag1 != 0 { (TABLE2, INDEX2) } else { (TABLE1, INDEX1) };
        let idx = g.m.r32(idx_addr);
        let e = tbl.wrapping_add(idx.wrapping_mul(8));
        let vx = g.m.r32(e);
        let vy = g.m.r32(e + 4);
        g.m.w32(p + 0x12, vx);
        g.m.w32(p + 0x16, vy);
        let ni = idx.wrapping_add(1);
        g.m.w32(idx_addr, if (ni as i32) < 0x63 { ni } else { 0 });
        let k = if flag2 != 0 { g.m.rf32(C_SPEED_A) } else { g.m.rf32(C_SPEED_B) } as f64;
        let v = (g.m.rf32(p + 0x12) as f64 * k) as f32;
        g.m.wf32(p + 0x12, v);
        let v = (g.m.rf32(p + 0x16) as f64 * k) as f32;
        g.m.wf32(p + 0x16, v);
        j += 1;
    }
}

/// `void __cdecl G_Particle_Process(void)` — 0x42ef00, 768 bytes, decomp: compiling. 
pub fn G_Particle_Process(g: &mut G) {
    let l = g.m.r32(LIST);
    let num = list::num(&g.m, l);
    if num < 1 {
        return;
    }
    let vs = G_Bgnd_GetVerticalScrollingThisFrame(g);
    let bound_x = ftol(G_Res_GetPermFloat(g, 0x36) as f64).wrapping_add(0x20);
    let bound_y = ftol(G_Res_GetPermFloat(g, 0x37) as f64);
    let sp = g.m.sp;
    let r = g.m.alloca(8);
    default_ref(g, r);
    let scale = G_Res_GetPermFloat(g, 0x90);
    let growth = ftol(G_Res_GetPermFloat(g, 0x94) as f64) as u32;
    let c_min = g.m.rf32(C_MIN) as f64;
    let c_seven = g.m.rf32(C_SEVEN) as f64;
    let c_zero = g.m.rf32(C_ZERO) as f64;
    let mut gi = 0;
    while gi < num {
        list::next(&mut g.m, l, r);
        let grp = g.m.r32(r + 4);
        let d = g.m.ri32(grp + 0x418).wrapping_sub(1);
        g.m.wi32(grp + 0x418, d);
        if d > 0 {
            gi += 1;
            continue;
        }
        let mut any_active = false;
        let count = g.m.ri32(grp + 0x41c);
        let mut j = 0;
        while j < count {
            let p = grp + 4 + (j as u32).wrapping_mul(0x1a);
            j += 1;
            if g.m.r8(p) == 0 {
                continue;
            }
            if g.m.r8(grp + 0x414) != 0 {
                let v = (vs as f64 + g.m.rf32(p + 0xe) as f64) as f32;
                g.m.wf32(p + 0xe, v);
            }
            let v = (g.m.rf32(p + 0x12) as f64 * scale as f64) as f32;
            g.m.wf32(p + 0x12, v);
            let v = (g.m.rf32(p + 0x16) as f64 * scale as f64) as f32;
            g.m.wf32(p + 0x16, v);
            let v = (g.m.rf32(p + 0xa) as f64 + g.m.rf32(p + 0x12) as f64) as f32;
            g.m.wf32(p + 0xa, v);
            let v = (g.m.rf32(p + 0xe) as f64 + g.m.rf32(p + 0x16) as f64) as f32;
            g.m.wf32(p + 0xe, v);
            let fa = g.m.rf32(p + 0xa) as f64;
            let fe = g.m.rf32(p + 0xe) as f64;
            let out = fa < c_min
                || (bound_x as f64) < fa + c_seven
                || fe < c_zero
                || (bound_y as f64) < fe + c_seven;
            if out && g.m.r8(grp + 0x415) == 0 {
                g.m.w8(p, 0);
                continue;
            }
            if g.m.r8(grp + 0x415) == 0 {
                let f6 = g.m.r32(p + 6);
                if f6 < 0x20 {
                    let mut n = f6.wrapping_add(growth);
                    if n > 0x20 {
                        n = 0x20;
                    }
                    g.m.w32(p + 6, n);
                } else {
                    g.m.w8(p, 0);
                    continue;
                }
            } else {
                let f6 = g.m.r32(p + 6);
                if f6 != 0 {
                    if f6 > growth {
                        g.m.w32(p + 6, f6.wrapping_sub(growth));
                    } else {
                        g.m.w32(p + 6, 0);
                    }
                } else {
                    g.m.w8(p, 0);
                    continue;
                }
            }
            if g.m.r8(p) != 0 {
                any_active = true;
            }
        }
        if !any_active {
            let link = g.m.r32(r);
            list::delete_link_ref(&mut g.m, l, link, r);
            sub_4302f0(g, grp);
        }
        gi += 1;
    }
    g.m.pop_to(sp);
}

/// `void __cdecl sub_0302f0(struct G_Particle_Group *)` — 0x4302f0, 32 bytes, decomp: complete. 
pub fn sub_4302f0(g: &mut G, a0: u32) {
    if a0 != 0 {
        g.m.free(a0);
    }
}
