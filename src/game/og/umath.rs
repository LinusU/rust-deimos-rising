//! `U_Math` (0x402170..0x402ad2), ported from the disassembly.
//!
//! Angles are whole degrees, 0 = down the screen (+y), increasing toward
//! +x; trig comes from 360-entry float tables, and square roots of
//! distances come from a table indexed by the truncated squared distance.
//! The tables here are the original's own, copied from its memory after
//! `U_Math_Init` (atan at 0x4c6aff, sqrt at 0x4c7aff, cos at 0x4d7aff,
//! sin at 0x4d809f).
//!
//! The x87 runs at double precision, so expressions are evaluated in f64
//! and rounded to f32 wherever the original stores a float.

use std::sync::OnceLock;

static TABLES: &[u8] = include_bytes!("umath_tables.bin");

struct Tables {
    atan: Vec<i32>,
    sqrt: Vec<f32>,
    cos: Vec<f32>,
    sin: Vec<f32>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let f = |o: usize| f32::from_le_bytes(TABLES[o..o + 4].try_into().unwrap());
        let i = |o: usize| i32::from_le_bytes(TABLES[o..o + 4].try_into().unwrap());
        Tables {
            atan: (0..1024).map(|k| i(k * 4)).collect(),
            sqrt: (0..0x4000).map(|k| f(0x1000 + k * 4)).collect(),
            cos: (0..360).map(|k| f(0x11000 + k * 4)).collect(),
            sin: (0..360).map(|k| f(0x115a0 + k * 4)).collect(),
        }
    })
}

/// CodeWarrior's float-to-int conversion (`fist` plus a fix-up): truncation
/// toward zero.
pub fn ftol(v: f64) -> i32 {
    if v.is_nan() {
        return i32::MIN;
    }
    v.trunc() as i32
}

/// MSL `sqrtf`: double sqrt of the float, rounded back to float.
pub fn sqrtf(v: f32) -> f32 {
    (v as f64).sqrt() as f32
}

/// MSL `atanf`.
pub fn atanf(v: f32) -> f32 {
    (v as f64).atan() as f32
}

/// `U_Math_Cos` (0x402750). Index 360 wraps to 0; anything else outside
/// 0..359 would read past the table in the original.
pub fn cos(angle: i32) -> f32 {
    let a = if angle == 360 { 0 } else { angle };
    tables().cos[a as usize]
}

/// `U_Math_Sin` (0x402770).
pub fn sin(angle: i32) -> f32 {
    let a = if angle == 360 { 0 } else { angle };
    tables().sin[a as usize]
}

/// `U_Math_Sqrt` (0x402790).
pub fn sqrt(v: i32) -> f32 {
    if v < 0x4000 {
        tables().sqrt[v as usize]
    } else {
        sqrtf(v as f32)
    }
}

/// `U_Math_GetVectorFromAngle` (0x402320): (sin, cos).
pub fn vector_from_angle(angle: i32) -> (f32, f32) {
    (sin(angle), cos(angle))
}

/// `U_Math_GetVectorFromAngleAndSpeed` (0x402360).
pub fn vector_from_angle_and_speed(angle: i32, speed: f32) -> (f32, f32) {
    (
        (sin(angle) as f64 * speed as f64) as f32,
        (cos(angle) as f64 * speed as f64) as f32,
    )
}

/// `U_Math_GetUnitVector` (0x4023a0): divides by the table root of the
/// truncated squared length.
pub fn unit_vector(x: f32, y: f32) -> (f32, f32) {
    let sq = y as f64 * y as f64 + x as f64 * x as f64;
    let len = sqrt(ftol(sq));
    ((x as f64 / len as f64) as f32, (y as f64 / len as f64) as f32)
}

/// `U_Math_GetSpeedFromVector` (0x402420).
pub fn speed_from_vector(x: f32, y: f32) -> f32 {
    sqrtf((y as f64 * y as f64 + x as f64 * x as f64) as f32)
}

/// `U_Math_GetDistanceToTarget` (0x4026e0).
pub fn distance_to_target(src: (f32, f32), dest: (f32, f32)) -> f32 {
    let dx = dest.0 as f64 - src.0 as f64;
    let dy = dest.1 as f64 - src.1 as f64;
    sqrt(ftol(dy * dy + dx * dx))
}

/// `U_Math_DoCircleCollisionDetection` (0x4027d0).
pub fn circle_collision(a: (f32, f32), ra: f32, b: (f32, f32), rb: f32) -> bool {
    let dx = a.0 as f64 - b.0 as f64;
    let dy = a.1 as f64 - b.1 as f64;
    let idx = ftol(dy * dy + dx * dx);
    let dist = if idx < 0x4000 { tables().sqrt[idx.max(0) as usize] } else { sqrtf(idx as f32) };
    ra as f64 + rb as f64 > dist as f64
}

/// `U_Math_InvertAngle` (0x402890).
pub fn invert_angle(a: i32) -> i32 {
    if a <= 180 {
        (a - 180).abs()
    } else {
        540 - a
    }
}

/// `U_Math_GetAngleFromVector(U_Utils_Vector*)` (0x402450): atanf-based,
/// truncated, 360 folds to 0.
pub fn angle_from_vector(x: f32, y: f32) -> i32 {
    const R2D: f64 = 57.29577951308232;
    let angle: f32 = if x == 0.0 && y == 0.0 {
        0.0
    } else if x == 0.0 && y > 0.0 {
        0.0
    } else if x == 0.0 && y < 0.0 {
        180.0
    } else if y == 0.0 && x > 0.0 {
        90.0
    } else if y == 0.0 && x < 0.0 {
        270.0
    } else if x > 0.0 && y > 0.0 {
        (atanf((x as f64 / y as f64) as f32) as f64 * R2D) as f32
    } else if x > 0.0 && y < 0.0 {
        (180.0 - atanf((x as f64 / y.abs() as f64) as f32) as f64 * R2D) as f32
    } else if x < 0.0 && y > 0.0 {
        (360.0 - atanf((x.abs() as f64 / y as f64) as f32) as f64 * R2D) as f32
    } else if x < 0.0 && y < 0.0 {
        (270.0 - atanf((y.abs() as f64 / x.abs() as f64) as f32) as f64 * R2D) as f32
    } else {
        0.0
    };
    let a = ftol(angle as f64);
    if a >= 360 {
        0
    } else {
        a
    }
}

/// `U_Math_GetAngleFromVector(double, double)` (0x4028e0): the atan-table
/// variant behind `U_Math_GetInterceptAngle`.
pub fn angle_from_vector_table(x: f64, y: f64) -> i32 {
    // |v| as the original takes it: v > 0 ? v : -v.
    let abs = |v: f64| if v > 0.0 { v } else { -v };
    let (ax, ay) = (abs(x), abs(y));
    let r = if ax < ay { abs(x / y) } else { abs(y / x) };
    let idx = ftol(r * 100.0).clamp(0, 0x3ff);
    let mut e = tables().atan[idx as usize].abs();
    if ax < ay {
        e = 90 - e;
    }
    if x < 0.0 && y >= 0.0 {
        e = 180 - e;
    }
    if x < 0.0 && y < 0.0 {
        e += 180;
    }
    if x >= 0.0 && y < 0.0 {
        e = -e;
    }
    e -= 90;
    if e < 0 {
        e += 360;
    }
    if e >= 360 {
        e -= 360;
    }
    e
}

/// `U_Math_GetInterceptAngle` (0x4022e0): angle of (src - dest).
pub fn intercept_angle(src_x: i32, src_y: i32, dest_x: i32, dest_y: i32) -> i32 {
    angle_from_vector_table((src_x - dest_x) as f64, (src_y - dest_y) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_their_definitions() {
        for i in 0..0x4000 {
            assert_eq!(sqrt(i), (i as f32).sqrt(), "sqrt {i}");
        }
        assert_eq!(cos(0), 1.0);
        assert_eq!(sin(90), 1.0);
        assert_eq!(sin(360), 0.0);
    }

    #[test]
    fn angles() {
        assert_eq!(angle_from_vector(0.0, 1.0), 0);
        assert_eq!(angle_from_vector(1.0, 0.0), 90);
        assert_eq!(angle_from_vector(0.0, -1.0), 180);
        assert_eq!(angle_from_vector(-1.0, 0.0), 270);
        assert_eq!(invert_angle(0), 180);
        assert_eq!(invert_angle(270), 270);
    }
}
