//! `U_Token` (0x40ea10..): the `#key_TYPE <value>` scanner every data file
//! goes through. Each getter finds `key` at or after the cursor with
//! `strstr`, takes the text between the next `<` and `>`, and leaves the
//! cursor on that `>`. A missing key leaves both the destination and the
//! cursor untouched, which is why parsers preset defaults.

/// Locate `key` from `*cursor`, then the `<...>` value after it
/// (`U_Token_GetPtrAndLength`). Returns the value's byte range.
fn ptr_and_len(text: &[u8], cursor: &mut i32, key: &str) -> Option<(usize, usize)> {
    let from = (*cursor).max(0) as usize;
    let found = find(text, from, key.as_bytes())?;
    let start = find(text, found, b"<")?;
    let end = find(text, start, b">")?;
    *cursor = end as i32;
    // length = end - start - 1, value starts after the one-byte marker.
    Some((start + 1, end - start - 1))
}

/// `strstr` on a NUL-terminated buffer: stops at the first NUL.
fn find(text: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    let hay = &text[from.min(text.len())..];
    let hay = &hay[..hay.iter().position(|&b| b == 0).unwrap_or(hay.len())];
    if pat.is_empty() {
        return Some(from);
    }
    hay.windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

/// The value copied into a 32-byte local, as the numeric getters do.
fn local31(text: &[u8], start: usize, len: usize) -> &[u8] {
    &text[start..start + len.min(31)]
}

/// `sscanf("%i")`: optional sign, then `0x` hex, leading-`0` octal or
/// decimal. None when no digits parse (the destination is left alone).
pub fn scan_i(s: &[u8]) -> Option<i32> {
    let mut i = 0;
    while i < s.len() && (s[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    let mut neg = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        neg = s[i] == b'-';
        i += 1;
    }
    let (radix, mut j) = if s.len() > i + 1 && s[i] == b'0' && (s[i + 1] | 0x20) == b'x' {
        (16, i + 2)
    } else if i < s.len() && s[i] == b'0' {
        (8, i)
    } else {
        (10, i)
    };
    let digits_start = j;
    let mut v: u32 = 0;
    while j < s.len() {
        let d = match (s[j] as char).to_digit(radix) {
            Some(d) => d,
            None => break,
        };
        v = v.wrapping_mul(radix).wrapping_add(d);
        j += 1;
    }
    if j == digits_start {
        // "0x" with no hex digits still reads the 0.
        return if radix == 16 { Some(0) } else { None };
    }
    let v = v as i32;
    Some(if neg { v.wrapping_neg() } else { v })
}

/// `sscanf("%f")` into a float: the longest decimal prefix, via double.
pub fn scan_f(s: &[u8]) -> Option<f32> {
    let t = std::str::from_utf8(s).ok()?.trim_start();
    let b = t.as_bytes();
    let mut end = 0;
    let mut best = None;
    while end < b.len() {
        end += 1;
        if let Ok(v) = t[..end].parse::<f64>() {
            best = Some(v);
        } else if !matches!(b[end - 1], b'+' | b'-' | b'.' | b'e' | b'E' | b'0'..=b'9') {
            break;
        }
    }
    best.map(|v| v as f32)
}

pub fn get_int(text: &[u8], cursor: &mut i32, key: &str, out: &mut [u8]) {
    if let Some((start, len)) = ptr_and_len(text, cursor, key) {
        if len > 0 {
            if let Some(v) = scan_i(local31(text, start, len)) {
                out[..4].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
}

/// Integer getter into a plain variable (counts like `#numStates_INT`).
pub fn get_int_var(text: &[u8], cursor: &mut i32, key: &str, out: &mut i32) {
    let mut b = out.to_le_bytes();
    get_int(text, cursor, key, &mut b);
    *out = i32::from_le_bytes(b);
}

pub fn get_float(text: &[u8], cursor: &mut i32, key: &str, out: &mut [u8]) {
    if let Some((start, len)) = ptr_and_len(text, cursor, key) {
        if len > 0 {
            if let Some(v) = scan_f(local31(text, start, len)) {
                out[..4].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
}

pub fn get_bool(text: &[u8], cursor: &mut i32, key: &str, out: &mut [u8]) {
    if let Some((start, len)) = ptr_and_len(text, cursor, key) {
        if len > 0 {
            let v = local31(text, start, len);
            // strcmp against "TRUE" on the NUL-terminated copy.
            let v = &v[..v.iter().position(|&b| b == 0).unwrap_or(v.len())];
            out[0] = (v == b"TRUE") as u8;
        }
    }
}

/// `U_Token_GetID`: exactly four bytes, copied raw.
pub fn get_id(text: &[u8], cursor: &mut i32, key: &str, out: &mut [u8]) {
    if let Some((start, len)) = ptr_and_len(text, cursor, key) {
        if len == 4 {
            out[..4].copy_from_slice(&text[start..start + 4]);
        }
    }
}

/// `U_Token_GetString`: clears the destination, then copies at most
/// `size - 1` bytes (`U_Utils_String_Safe_strncpy` stops at a NUL).
pub fn get_string(text: &[u8], cursor: &mut i32, key: &str, out: &mut [u8], size: usize) {
    if let Some((start, len)) = ptr_and_len(text, cursor, key) {
        out[..size].fill(0);
        if len > 0 {
            let n = len.min(size - 1);
            for (i, &b) in text[start..start + n].iter().enumerate() {
                if b == 0 {
                    break;
                }
                out[i] = b;
            }
        }
    }
}

/// `U_Token_GetColor`: `RRGGBB` → 16-bit 1555 pixel. A missing key or a
/// malformed value leaves the destination alone.
pub fn get_color(text: &[u8], cursor: &mut i32, key: &str, out: &mut [u8]) {
    let Some((start, len)) = ptr_and_len(text, cursor, key) else { return };
    if len == 0 {
        return;
    }
    let v = local31(text, start, len);
    let v = &v[..v.iter().position(|&b| b == 0).unwrap_or(v.len())];
    if let Some(px) = html_to_pixel16(v) {
        out[..2].copy_from_slice(&px.to_le_bytes());
    }
}

/// `U_Pixel16_HTMLStringToPixel16` (0x44dce0).
pub fn html_to_pixel16(s: &[u8]) -> Option<u16> {
    if s.len() != 6 {
        return None;
    }
    let hex = |p: &[u8]| -> i32 {
        let t = std::str::from_utf8(p).unwrap_or("");
        let n = t.chars().take_while(|c| c.is_ascii_hexdigit()).count();
        i32::from_str_radix(&t[..n], 16).unwrap_or(0)
    };
    let (r, g, b) = (hex(&s[0..2]), hex(&s[2..4]), hex(&s[4..6]));
    // f = (float)c / 255.0f; f *= 65535.0f; c = (int)f  (x87 at double
    // precision, f spilled as a float between the steps).
    let r = ((r as f64 / 255.0) as f32 as f64 * 65535.0) as f32 as i32;
    let g = ((g as f64 / 255.0) as f32 as f64 * 65535.0) as f32 as i32;
    let b = ((b as f64 / 255.0) * 65535.0) as i32;
    Some((((r >> 1) & 0x7c00) | ((g >> 6) & 0x3e0) | ((b >> 11) & 0x1f)) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_like_sscanf() {
        assert_eq!(scan_i(b"42"), Some(42));
        assert_eq!(scan_i(b" -7"), Some(-7));
        assert_eq!(scan_i(b"0x1f"), Some(31));
        assert_eq!(scan_i(b"010"), Some(8));
        assert_eq!(scan_i(b"12.5"), Some(12));
        assert_eq!(scan_i(b"abc"), None);
        assert_eq!(scan_f(b"1.25"), Some(1.25));
        assert_eq!(scan_f(b"-3"), Some(-3.0));
        assert_eq!(scan_f(b"x"), None);
    }

    #[test]
    fn cursor_lands_on_the_closing_marker() {
        let t = b"#a_INT <5> #a_INT <6>\0";
        let mut c = 0;
        let mut v = 0;
        get_int_var(t, &mut c, "#a_INT", &mut v);
        assert_eq!((v, c), (5, 9));
        get_int_var(t, &mut c, "#a_INT", &mut v);
        assert_eq!(v, 6);
    }
}
