//! `U_SpritePlate` (0x40e300..0x40e940): finds the frames on a sprite
//! plate. Frames sit in bands marked in column 0 and are trimmed to the
//! pixels that differ from their cell's background; the trimmed size is
//! what `U_Sprite_GetDimensions` reports.
//!
//! The plate is generic over the pixel type: the original scans an 8-bit
//! QuickTime buffer, which `assets::plate_pixels` models as colours
//! compared at 4 bits per channel.

/// One frame: (left, top, width, height) of the trimmed bitmap.
pub type Rect = (i32, i32, i32, i32);

/// `U_SpritePlate_GetBitmapRectListFromPlate` on an 8-bit plate.
pub fn bitmap_rects<T: Copy + PartialEq>(px: &[T], stride: i32, width: i32, height: i32) -> Vec<Rect> {
    let mut out = Vec::new();
    if width < 3 || height < 2 || px.len() < 3 || px[0] == px[1] || px[1] == px[2] {
        return out;
    }
    let (mut row, mut col) = (0, 0);
    while let Some(r) = next_bitmap(px, stride, width, height, &mut row, &mut col) {
        out.push(r);
    }
    out
}

/// `sub_40e540`: the next frame after (row, col).
fn next_bitmap<T: Copy + PartialEq>(px: &[T], stride: i32, width: i32, height: i32, io_row: &mut i32, io_col: &mut i32) -> Option<Rect> {
    let mut row = *io_row;
    let mut col = *io_col;
    if row == 0 {
        row += 1;
    }
    while row < height {
        if let Some(row_run) = band_height(px, stride, height, row) {
            let band = (row * stride) as usize;
            while col < width {
                if let Some(col_run) = cell_width(&px[band..], stride, width, row_run, col) {
                    if let Some((bx, by, bw, bh)) = trim(&px[band + col as usize..], stride, col_run, row_run) {
                        *io_row = row;
                        *io_col = col + col_run + 1;
                        let left = col + 1 + bx - 1;
                        let _ = by;
                        let top = row_run - bh + row - 1;
                        return Some((left, top, bw, bh));
                    }
                    col += col_run;
                }
                col += 1;
            }
            row += row_run;
            col = 0;
        }
        row += 1;
    }
    None
}

/// `sub_40e6e0`: rows from `start` until column 0 shows the marker
/// colour `plate[1]`.
fn band_height<T: Copy + PartialEq>(px: &[T], stride: i32, limit: i32, start: i32) -> Option<i32> {
    let key = px[1];
    let mut run = 0;
    while start + run < limit && px[((start + run) * stride) as usize] != key {
        run += 1;
    }
    (run > 0).then_some(run)
}

/// `sub_40e750`: columns from `start` with no marker pixel (`band[1]`)
/// in the band's rows.
fn cell_width<T: Copy + PartialEq>(band: &[T], stride: i32, limit: i32, rows: i32, start: i32) -> Option<i32> {
    let key = band[1];
    let span = rows * limit;
    let mut run = 0;
    while start + run < limit {
        let col = (start + run) as usize;
        let mut off = 0;
        while off < span && band.get(col + off as usize).map_or(false, |&p| p != key) {
            off += stride;
        }
        if off < span {
            break;
        }
        run += 1;
    }
    (run > 0).then_some(run)
}

/// `sub_40e7d0`: trim a cell to the pixels that differ from its corner
/// colour; None when the cell is empty. Returns (x, y, w, h) inside it.
fn trim<T: Copy + PartialEq>(cell: &[T], stride: i32, width: i32, height: i32) -> Option<Rect> {
    let key = cell[0];
    let at = |r: i32, c: i32| cell[(r * stride + c) as usize];
    let row_empty = |r: i32| (0..width).all(|c| at(r, c) == key);
    let mut top = 0;
    while top < height && row_empty(top) {
        top += 1;
    }
    if top >= height {
        return None;
    }
    let mut bottom = 0;
    while bottom < height && row_empty(height - 1 - bottom) {
        bottom += 1;
    }
    let h = height - top - bottom;
    let col_empty = |c: i32| (top..top + h).all(|r| at(r, c) == key);
    let mut left = 0;
    while left < width && col_empty(left) {
        left += 1;
    }
    let mut right = 0;
    while right < width && col_empty(width - 1 - right) {
        right += 1;
    }
    Some((left, top, width - left - right, h))
}
