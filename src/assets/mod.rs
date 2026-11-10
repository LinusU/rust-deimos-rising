//! Decoders for the original asset formats, read live from the retail paks:
//! - `im08`: animated GIF89a sprite sheets (each frame = one direction)
//! - `im16`: TGA backgrounds/media
//! - `soun`: AIFF with Apple `ima4` IMA-ADPCM compression

use crate::pak::{PakSet, ResId, Tag};
use std::collections::HashMap;
use std::path::Path;

/// A decoded sprite frame: tightly-cropped RGBA pixels.
#[derive(Clone)]
pub struct GifFrame {
    pub width: u32,
    pub height: u32,
    /// RGBA bytes, `width * height * 4`.
    pub rgba: Vec<u8>,
    /// Frame delay in seconds (0 for plate-extracted sprite frames).
    pub delay: f32,
}

/// A decoded sprite: all frames of one `im08` resource, plus the logical
/// pixel size reported by the GIF screen descriptor.
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    /// Frames in the original's scan order (direction-major).
    pub frames: Vec<GifFrame>,
}

/// Decoded PCM audio.
pub struct Sound {
    /// Interleaved signed 16-bit samples.
    pub samples: Vec<i16>,
    pub channels: usize,
    pub sample_rate: u32,
}

/// Level media mask (`im16` 16-bit TGA): a tile map where pixel value
/// `0x1f` marks "media" surface (G_Bgnd_MediaMask_GetSurfaceAtLoc).
/// Rows are stored top-down, matching the U_Image/QuickTime pixel order.
#[derive(Clone)]
pub struct MediaMask {
    pub w: u32,
    pub h: u32,
    /// Row-major u16 pixels, `w * h` entries.
    pub px: Vec<u16>,
}

/// Decode an uncompressed/RLE 16bpp TGA into a top-down u16 pixel map.
/// Returns None for other pixel formats (the retail masks are all
/// type-2, 16bpp, bottom-up).
fn decode_media_mask_tga(data: &[u8]) -> Option<MediaMask> {
    if data.len() < 18 {
        return None;
    }
    let idlen = data[0] as usize;
    let cmap_type = data[1];
    let img_type = data[2];
    if cmap_type != 0 || (img_type != 2 && img_type != 10) {
        return None;
    }
    let cmap_len = u16::from_le_bytes([data[5], data[6]]) as usize;
    let cmap_depth = data[7];
    let w = u16::from_le_bytes([data[12], data[13]]) as u32;
    let h = u16::from_le_bytes([data[14], data[15]]) as u32;
    let bpp = data[16];
    let desc = data[17];
    if bpp != 16 || w == 0 || h == 0 {
        return None;
    }
    let mut off = 18 + idlen + cmap_len * (cmap_depth as usize / 8);
    let n = (w * h) as usize;
    let mut file_rows: Vec<u16> = Vec::with_capacity(n);
    if img_type == 2 {
        if data.len() < off + n * 2 {
            return None;
        }
        for c in data[off..off + n * 2].chunks_exact(2) {
            file_rows.push(u16::from_le_bytes([c[0], c[1]]));
        }
    } else {
        // RLE: packet header high bit = run, count = (hdr & 0x7f) + 1.
        while file_rows.len() < n {
            if off >= data.len() {
                return None;
            }
            let hdr = data[off];
            off += 1;
            let count = (hdr & 0x7f) as usize + 1;
            if hdr & 0x80 != 0 {
                if off + 2 > data.len() {
                    return None;
                }
                let v = u16::from_le_bytes([data[off], data[off + 1]]);
                off += 2;
                file_rows.extend(std::iter::repeat(v).take(count));
            } else {
                if off + count * 2 > data.len() {
                    return None;
                }
                for c in data[off..off + count * 2].chunks_exact(2) {
                    file_rows.push(u16::from_le_bytes([c[0], c[1]]));
                }
                off += count * 2;
            }
        }
        file_rows.truncate(n);
    }
    // TGA descriptor bit 5 = top-down storage; retail masks are bottom-up.
    let top_down = desc & 0x20 != 0;
    let w_us = w as usize;
    let mut px = vec![0u16; n];
    for y in 0..h as usize {
        let src_y = if top_down { y } else { h as usize - 1 - y };
        px[y * w_us..(y + 1) * w_us]
            .copy_from_slice(&file_rows[src_y * w_us..(src_y + 1) * w_us]);
    }
    Some(MediaMask { w, h, px })
}

impl Sound {
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / (self.channels as f32 * self.sample_rate as f32)
    }
}

/// All game data, loaded once from the retail paks and cached.
#[derive(bevy::prelude::Resource)]
pub struct GameAssets {
    pub set: PakSet,
    sprites: HashMap<(u32, ResId), Sprite>,
    images16: HashMap<ResId, (u32, u32, Vec<u8>)>, // TGA -> (w,h,rgba)
    media_masks: HashMap<ResId, Option<MediaMask>>,
    sounds: HashMap<ResId, Sound>,
    // Parsed text resources, cached by (tag, id).
    pub units: HashMap<ResId, crate::data::UnitDef>,
    pub weapons: HashMap<ResId, crate::data::WeaponDef>,
    /// Weapon IDs in master-list (pak) order — the original iterates the
    /// weapon master list for type/level-filtered "next weapon" lookups.
    pub weapon_order: Vec<ResId>,
    pub players: HashMap<ResId, crate::data::PlayerDef>,
    pub levels: HashMap<ResId, crate::data::LevelDef>,
    pub fill_list: Option<crate::data::FillList>,
    pub rect_list: Option<crate::data::RectList>,
    pub color_list: Option<crate::data::ColorList>,
    pub object_ids: crate::data::IdList,
    pub sprite_ids: crate::data::IdList,
    /// `Sounds[gaso]` perm sound list — positional via `G_Res_GetPermSoundID`.
    pub sound_ids: crate::data::IdList,
    /// `Formats[gatf]`: the permanent text format ids, positional
    /// (`G_Text_GetPermTextSetting`).
    pub format_ids: crate::data::IdList,
    pub strings: HashMap<ResId, crate::data::StillText>,
    pub text_formats: HashMap<ResId, crate::data::TextFormat>,
    /// Per-sprite frame (width, height) table for collision bounds —
    /// populated by `warm_sprite_dims`.
    pub sprite_dims: HashMap<ResId, Vec<(u32, u32)>>,
}

impl GameAssets {
    pub fn open(data_dir: &Path) -> std::io::Result<GameAssets> {
        let set = PakSet::open_dir(data_dir)?;
        Ok(GameAssets {
            set,
            sprites: HashMap::new(),
            images16: HashMap::new(),
            media_masks: HashMap::new(),
            sounds: HashMap::new(),
            units: HashMap::new(),
            weapons: HashMap::new(),
            weapon_order: Vec::new(),
            players: HashMap::new(),
            levels: HashMap::new(),
            fill_list: None,
            rect_list: None,
            color_list: None,
            object_ids: crate::data::IdList { entries: Vec::new() },
            sprite_ids: crate::data::IdList { entries: Vec::new() },
            sound_ids: crate::data::IdList { entries: Vec::new() },
            format_ids: crate::data::IdList { entries: Vec::new() },
            strings: HashMap::new(),
            text_formats: HashMap::new(),
            sprite_dims: HashMap::new(),
        })
    }

    /// Load and parse every text resource. Call once at startup.
    pub fn load_all_data(&mut self) {
        // Permanent lists first (flli/reli/coli/idli are tiny). Collect the
        // (rid, display) pairs first so `read_text` can mutably borrow `set`.
        let pairs = |set: &PakSet, tag: Tag| -> Vec<(ResId, String)> {
            set.entries_of_tag(tag)
                .iter()
                .map(|e| (e.rid, e.display.clone()))
                .collect()
        };

        for (rid, _) in pairs(&self.set, Tag::Flli) {
            if let Some(t) = self.set.read_text(Tag::Flli, rid) {
                self.fill_list = Some(crate::data::FillList::parse(&t));
            }
        }
        for (rid, _) in pairs(&self.set, Tag::Reli) {
            if let Some(t) = self.set.read_text(Tag::Reli, rid) {
                self.rect_list = Some(crate::data::RectList::parse(&t));
            }
        }
        for (rid, _) in pairs(&self.set, Tag::Coli) {
            if let Some(t) = self.set.read_text(Tag::Coli, rid) {
                self.color_list = Some(crate::data::ColorList::parse(&t));
            }
        }
        // idli files: the original loads them positionally; Objects[gaob]
        // and Sprites[gasp] feed g_permObjectIDs / g_permSpriteIDs.
        for (rid, display) in pairs(&self.set, Tag::Edli) {
            let Some(t) = self.set.read_text(Tag::Edli, rid) else { continue };
            let list = crate::data::IdList::parse(&t);
            let d = display.to_lowercase();
            if d.contains("object") {
                self.object_ids = list;
            } else if d.contains("sprite") {
                self.sprite_ids = list;
            } else if d.contains("format") {
                self.format_ids = list;
            } else if d.contains("sound") {
                self.sound_ids = list;
            }
        }

        for (rid, _) in pairs(&self.set, Tag::Unde) {
            if let Some(t) = self.set.read_text(Tag::Unde, rid) {
                self.units.insert(rid, crate::data::UnitDef::parse(rid, &t));
            }
        }
        for (rid, _) in pairs(&self.set, Tag::Wede) {
            if let Some(t) = self.set.read_text(Tag::Wede, rid) {
                self.weapons.insert(rid, crate::data::WeaponDef::parse(rid, &t));
                self.weapon_order.push(rid);
            }
        }
        for (rid, _) in pairs(&self.set, Tag::Plde) {
            if let Some(t) = self.set.read_text(Tag::Plde, rid) {
                self.players.insert(rid, crate::data::PlayerDef::parse(&t));
            }
        }
        for (rid, _) in pairs(&self.set, Tag::Leve) {
            if let Some(t) = self.set.read_text(Tag::Leve, rid) {
                self.levels.insert(rid, crate::data::LevelDef::parse(&t));
            }
        }
        for (rid, _) in pairs(&self.set, Tag::Stli) {
            if let Some(t) = self.set.read_text(Tag::Stli, rid) {
                self.strings.insert(rid, crate::data::StillText::parse(&t));
            }
        }
        for (rid, _) in pairs(&self.set, Tag::Tefo) {
            if let Some(t) = self.set.read_text(Tag::Tefo, rid) {
                self.text_formats.insert(rid, crate::data::TextFormat::parse(&t));
            }
        }

        // Priv_Spawn_Ground's m_field24 velocity scale, per ground weapon:
        // the crosshair icon is drawn at player + (xoff, yoff) minus half
        // the icon frame, so diffY = iconH/2 - yoff and the scale is
        // max(0,(int)diffY)/|yoff|. The player position cancels out, making
        // it a per-weapon constant (the y>=shieldOffset clamp can't bind at
        // real player altitudes). Air weapons keep the 1.0 default.
        let ground_ids: Vec<ResId> = self
            .weapons
            .iter()
            .filter(|(_, w)| w.weapon_type == crate::pak::id("PEAG"))
            .map(|(&rid, _)| rid)
            .collect();
        for rid in ground_ids {
            let (face, frame, yoff) = {
                let w = &self.weapons[&rid];
                (w.crosshair_face, w.crosshair_frame, w.crosshair_y_offset)
            };
            if yoff == 0 {
                continue;
            }
            let icon_h = self
                .sprite(face)
                .and_then(|s| {
                    s.frames
                        .get(frame.max(0) as usize)
                        .map(|f| f.height as i64)
                })
                .unwrap_or(0);
            let diff_y = icon_h / 2 - yoff;
            let scale = if diff_y > 0 { diff_y as f64 / yoff.unsigned_abs() as f64 } else { 0.0 };
            let w = self.weapons.get_mut(&rid).unwrap();
            w.ballistic_scale = scale;
            w.ballistic_half_h = (icon_h / 2) as f64;
        }
    }

    // -- sprites ---------------------------------------------------------

    /// Decode an `im08` sprite into frames (cached).
    ///
    /// The original keeps two plates per sprite ID: the original-case GIF is
    /// the colour plate and the UPPERCASE GIF is the "alpha plate" whose
    /// non-key regions define each frame's rectangle (see
    /// `U_SpritePlate_GetBitmapRectListFromPlate`). Frames are cropped from
    /// the colour plate at those rectangles, in scan order.
    pub fn sprite(&mut self, rid: ResId) -> Option<&Sprite> {
        if !self.sprites.contains_key(&(Tag::Im08.code(), rid)) {
            let upper = to_upper_id(rid);
            let color_raw = self.set.read_raw(Tag::Im08, rid)?;
            let alpha_raw = self.set.read_raw(Tag::Im08, upper)?;
            let sprite = decode_sprite(&color_raw, &alpha_raw)?;
            self.sprites.insert((Tag::Im08.code(), rid), sprite);
        }
        self.sprites.get(&(Tag::Im08.code(), rid))
    }

    /// The frames `U_Sprite_Load` finds for sprite `rid`: `U_SpritePlate`
    /// run on its uppercase (alpha) plate as `plate_pixels` models it.
    pub fn sprite_frame_rects(&mut self, rid: ResId) -> Option<Vec<crate::game::og::spriteplate::Rect>> {
        let alpha_raw = self.set.read_raw(Tag::Im08, to_upper_id(rid))?;
        plate_frame_rects(&alpha_raw)
    }

    /// Decode `rid`'s sprite and cache its per-frame (w,h) table for the
    /// sim's collision bounds. Safe to call for missing/non-im08 rids —
    /// they are skipped.
    pub fn warm_sprite_dims(&mut self, rid: ResId) {
        if self.sprite_dims.contains_key(&rid) {
            return;
        }
        let dims = self
            .sprite(rid)
            .map(|s| s.frames.iter().map(|f| (f.width, f.height)).collect());
        if let Some(dims) = dims {
            self.sprite_dims.insert(rid, dims);
        }
    }

    /// Decode an `im16` TGA into RGBA (cached).
    pub fn image16(&mut self, rid: ResId) -> Option<&(u32, u32, Vec<u8>)> {
        if !self.images16.contains_key(&rid) {
            let raw = self.set.read_raw(Tag::Im16, rid)?;
            let img = decode_tga(&raw)?;
            self.images16.insert(rid, img);
        }
        self.images16.get(&rid)
    }

    /// Decode a level media-mask `im16` TGA into u16 tiles (cached).
    /// Returns None when the resource is missing or not a 16bpp TGA.
    pub fn media_mask(&mut self, rid: ResId) -> Option<&MediaMask> {
        if !self.media_masks.contains_key(&rid) {
            let mask = self
                .set
                .read_raw(Tag::Im16, rid)
                .and_then(|raw| decode_media_mask_tga(&raw));
            self.media_masks.insert(rid, mask);
        }
        self.media_masks.get(&rid).and_then(|m| m.as_ref())
    }

    // -- audio -----------------------------------------------------------

    /// Decode an AIFF `ima4` sound (cached).
    pub fn sound(&mut self, rid: ResId) -> Option<&Sound> {
        if !self.sounds.contains_key(&rid) {
            let raw = self.set.read_raw(Tag::Soun, rid)?;
            let snd = decode_aiff_ima4(&raw)?;
            self.sounds.insert(rid, snd);
        }
        self.sounds.get(&rid)
    }
}

// ---------------------------------------------------------------------------
// GIF decoding (via `gif` crate; handles LZW, transparency, frame disposal)
// ---------------------------------------------------------------------------

/// Uppercase a four-character resource ID, matching the original's
/// `U_Utils_String_ToUpper` on the sprite plate lookup.
fn to_upper_id(rid: ResId) -> ResId {
    let b = rid.to_le_bytes();
    let mut out = [0u8; 4];
    for i in 0..4 {
        out[i] = b[i].to_ascii_uppercase();
    }
    u32::from_le_bytes(out)
}

/// Decode the first GIF frame to composited RGBA.
fn decode_gif_rgba(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use gif::{ColorOutput, DisposalMethod};
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(ColorOutput::RGBA);
    let mut decoder = opts.read_info(data).ok()?;
    let w = decoder.width() as usize;
    let h = decoder.height() as usize;
    let mut canvas = vec![0u8; w * h * 4];
    let mut prev_canvas = canvas.clone();
    while let Some(frame) = decoder.read_next_frame().ok()? {
        let fw = frame.width as usize;
        let fh = frame.height as usize;
        let left = frame.left as usize;
        let top = frame.top as usize;
        let dispose = frame.dispose;
        let buf: &[u8] = &frame.buffer;

        if dispose == DisposalMethod::Previous {
            prev_canvas.copy_from_slice(&canvas);
        }
        if buf.len() >= fw * fh * 4 {
            for y in 0..fh {
                let cy = top + y;
                if cy >= h {
                    break;
                }
                for x in 0..fw {
                    let cx = left + x;
                    if cx >= w {
                        break;
                    }
                    let si = (y * fw + x) * 4;
                    if buf[si + 3] != 0 {
                        let di = (cy * w + cx) * 4;
                        canvas[di..di + 4].copy_from_slice(&buf[si..si + 4]);
                    }
                }
            }
        }
        match dispose {
            DisposalMethod::Background => {
                for y in 0..fh {
                    let cy = top + y;
                    if cy >= h {
                        break;
                    }
                    for x in 0..fw {
                        let cx = left + x;
                        if cx >= w {
                            break;
                        }
                        let di = (cy * w + cx) * 4;
                        canvas[di..di + 4].fill(0);
                    }
                }
            }
            DisposalMethod::Previous => canvas.copy_from_slice(&prev_canvas),
            _ => {}
        }
    }
    Some((w as u32, h as u32, canvas))
}

/// Decode the first GIF frame to raw palette indices (for the alpha plate).
fn decode_gif_indexed(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = opts.read_info(data).ok()?;
    let w = decoder.width() as usize;
    let h = decoder.height() as usize;
    let frame = decoder.read_next_frame().ok()??;
    let fw = frame.width as usize;
    let fh = frame.height as usize;
    let buf: &[u8] = &frame.buffer;
    if fw == w && fh == h && buf.len() >= w * h {
        return Some((w as u32, h as u32, buf[..w * h].to_vec()));
    }
    // Composite a sub-frame onto an index canvas (rare for these plates).
    let mut canvas = vec![0u8; w * h];
    for y in 0..fh.min(h - frame.top as usize) {
        for x in 0..fw.min(w - frame.left as usize) {
            canvas[(frame.top as usize + y) * w + frame.left as usize + x] =
                buf[y * fw + x];
        }
    }
    Some((w as u32, h as u32, canvas))
}

/// An 8-bit plate as the original scans it. `U_Image_GetPixelBuffer`
/// has QuickTime draw the GIF into an 8-bit GWorld, whose rows are padded
/// to 4 bytes and whose pixels are indices found through an inverse table:
/// colours agreeing in their top 4 bits per channel get the same index.
/// Each pixel is that 4-4-4 colour; padding never equals a pixel. Returns
/// (width, height, stride, pixels). Checked against every sprite plate
/// the original scans for Demo 01's level (tools/oracle/plates.py,
/// spritedims.py).
fn plate_pixels(data: &[u8]) -> Option<(usize, usize, usize, Vec<u16>)> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = opts.read_info(data).ok()?;
    let global = decoder.global_palette().map(|p| p.to_vec());
    let (w, h) = (decoder.width() as usize, decoder.height() as usize);
    let frame = decoder.read_next_frame().ok()??;
    let pal = frame.palette.clone().or(global)?;
    let rgb444 = |i: u8| -> u16 {
        let c = |k: usize| pal.get(i as usize * 3 + k).copied().unwrap_or(0) as u16 >> 4;
        (c(0) << 8) | (c(1) << 4) | c(2)
    };
    let stride = (w + 3) & !3;
    let mut px = vec![u16::MAX; stride * h];
    let (fw, fh, fl, ft) = (frame.width as usize, frame.height as usize, frame.left as usize, frame.top as usize);
    let bg = rgb444(0);
    for y in 0..h {
        for x in 0..w {
            px[y * stride + x] = bg;
        }
    }
    for y in 0..fh.min(h.saturating_sub(ft)) {
        for x in 0..fw.min(w.saturating_sub(fl)) {
            px[(ft + y) * stride + fl + x] = rgb444(frame.buffer[y * fw + x]);
        }
    }
    Some((w, h, stride, px))
}

/// `U_SpritePlate` on the plate GIF `data` (see `plate_pixels`).
fn plate_frame_rects(data: &[u8]) -> Option<Vec<crate::game::og::spriteplate::Rect>> {
    let (w, h, stride, px) = plate_pixels(data)?;
    Some(crate::game::og::spriteplate::bitmap_rects(&px, stride as i32, w as i32, h as i32))
}

/// Decode a sprite: colour plate (original case) + alpha plate (uppercase).
fn decode_sprite(color: &[u8], alpha: &[u8]) -> Option<Sprite> {
    let (cw, ch, color_px) = decode_gif_rgba(color)?;
    let (aw, ah, _) = decode_gif_indexed(alpha)?;
    if cw != aw || ch != ah {
        return None;
    }
    let (w, h) = (cw as usize, ch as usize);
    // The frames drawn are the rectangles `U_SpritePlate` finds on the
    // alpha plate.
    let rects: Vec<(usize, usize, usize, usize)> = plate_frame_rects(alpha)?
        .into_iter()
        .map(|r| (r.0.max(0) as usize, r.1.max(0) as usize, r.2.max(1) as usize, r.3.max(1) as usize))
        .collect();
    if rects.is_empty() {
        return None;
    }
    let (_, _, mut alpha_px) = decode_gif_rgba(alpha)?;
    let mut color_px = color_px;
    // Both plates go through QuickTime, so the colours and the mask's
    // transparency levels are brightened alike.
    quicktime_brighten(&mut color_px);
    quicktime_brighten(&mut alpha_px);
    // Colour key: the colour plate's third pixel (`U_Sprite.cc` reads the
    // 16-bit word at offset 4 of the plate's pixel data).
    let key = color_px.get(8..12).map(rgb555).unwrap_or(0);
    let frames = rects
        .iter()
        .map(|&r| {
            let (fw, fh) = (r.2 as u32, r.3 as u32);
            let mut rgba = crop_frame(&color_px, w, h, r);
            let mask = crop_frame(&alpha_px, w, h, r);
            apply_plate_alpha(&mut rgba, &mask, key);
            GifFrame {
                width: fw,
                height: fh,
                rgba,
                delay: 0.0,
            }
        })
        .collect();
    Some(Sprite {
        width: cw,
        height: ch,
        frames,
    })
}

/// RGBA8 pixel → the original's 15-bit 555 colour word.
fn rgb555(px: &[u8]) -> u16 {
    ((px[0] as u16 >> 3) << 10) | ((px[1] as u16 >> 3) << 5) | (px[2] as u16 >> 3)
}

/// Fill a frame's alpha channel from its alpha-plate crop, as the original
/// does when encoding a sprite frame (mask builder at VA 0x40df00, blended
/// by `U_SpriteBlit_DrawNormal`).
///
/// Each mask pixel's 5-bit red channel is a transparency level: 0 is
/// opaque, 1..=30 blends `level/32` of the background through, and 31 (or
/// a pixel equal to the colour key) is fully transparent. A frame whose
/// mask has no visible pixel at all is drawn colour-keyed instead.
fn apply_plate_alpha(rgba: &mut [u8], mask: &[u8], key: u16) {
    let level = |m: &[u8]| -> u32 {
        let red = (m[0] >> 3) as u32;
        if rgb555(m) == key || red >= 31 { 32 } else { red }
    };
    let any_visible = mask.chunks_exact(4).any(|m| level(m) < 32);
    for (px, m) in rgba.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
        px[3] = if any_visible {
            ((32 - level(m)) * 255 / 32) as u8
        } else if rgb555(px) == key {
            0
        } else {
            255
        };
    }
}

fn crop_frame(img: &[u8], iw: usize, ih: usize, rect: (usize, usize, usize, usize)) -> Vec<u8> {
    let (x, y, w, h) = rect;
    let mut out = vec![0u8; w * h * 4];
    for row in 0..h {
        let sy = y + row;
        if sy >= ih {
            break;
        }
        let src = (sy * iw + x) * 4;
        let dst = row * w * 4;
        let n = w * 4;
        if src + n <= img.len() && dst + n <= out.len() {
            out[dst..dst + n].copy_from_slice(&img[src..src + n]);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// TGA decoding
// ---------------------------------------------------------------------------

fn decode_tga(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    // TGA has no magic bytes, so decode with the format forced.
    let img = image::load_from_memory_with_format(data, image::ImageFormat::Tga).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let mut raw = rgba.into_raw();
    quicktime_brighten(&mut raw);
    Some((w, h, raw))
}

/// Brighten RGBA pixels the way QuickTime's importers do for every image
/// the original loads: backdrops, terrain, the score bar, sprite plates
/// and their alpha plates.
///
/// Measured against the original running under Drydock: a channel is cut
/// to 5 bits, and level `v` comes out as `round(31 * (v / 31) ^ 0.7455)`.
/// The game then works on those 5-bit levels, so a sprite's transparency
/// levels shift along with its colours.
fn quicktime_brighten(rgba: &mut [u8]) {
    let mut lut = [0u8; 32];
    for (v, out) in lut.iter_mut().enumerate() {
        let level = (31.0 * (v as f64 / 31.0).powf(0.7455)).round() as u8;
        *out = (level << 3) | (level >> 2);
    }
    for px in rgba.chunks_exact_mut(4) {
        for c in &mut px[..3] {
            *c = lut[(*c >> 3) as usize];
        }
    }
}

// ---------------------------------------------------------------------------
// AIFF / Apple IMA ADPCM ('ima4') decoding
// ---------------------------------------------------------------------------

const IMA_STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55,
    60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307,
    337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411,
    1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358,
    5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500,
    20350, 22385, 24623, 27086, 29794, 32767,
];

const IMA_INDEX_TABLE: [i32; 16] = [
    -1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8,
];

struct ImaState {
    predictor: i32,
    step_index: i32,
}

fn ima_expand_nibble(st: &mut ImaState, nibble: u8) -> i16 {
    let nibble = nibble & 0xF;
    let step = IMA_STEP_TABLE[st.step_index.clamp(0, 88) as usize];
    st.step_index += IMA_INDEX_TABLE[nibble as usize];
    st.step_index = st.step_index.clamp(0, 88);

    let mut diff = step >> 3;
    if nibble & 4 != 0 {
        diff += step;
    }
    if nibble & 2 != 0 {
        diff += step >> 1;
    }
    if nibble & 1 != 0 {
        diff += step >> 2;
    }
    if nibble & 8 != 0 {
        diff = -diff;
    }
    st.predictor += diff;
    st.predictor = st.predictor.clamp(-32768, 32767);
    st.predictor as i16
}

/// Decode one 34-byte `ima4` block into 64 samples for one channel.
fn decode_ima4_block(block: &[u8], out: &mut [i16]) {
    debug_assert!(block.len() >= 34 && out.len() >= 64);
    // Header: `ppppppppp iiiiiii` — bits 15..7 are the top 9 bits of the
    // signed 16-bit predictor (low 7 bits always zero), bits 6..0 are the
    // step index.
    let preamble = u16::from_be_bytes([block[0], block[1]]);
    let mut st = ImaState {
        predictor: ((preamble & 0xFF80) as i16) as i32,
        step_index: ((preamble & 0x7F) as i32).clamp(0, 88),
    };

    for k in 0..64 {
        let byte = block[2 + k / 2];
        let nibble = if k % 2 == 0 { byte & 0xF } else { byte >> 4 };
        out[k] = ima_expand_nibble(&mut st, nibble);
    }
}

/// Parse an AIFF file and decode `ima4` (or PCM) audio to interleaved i16.
pub fn decode_aiff_ima4(data: &[u8]) -> Option<Sound> {
    // Retail files are AIFF-C ('AIFC' FORM type); plain 'AIFF' also occurs.
    if data.len() < 12
        || &data[0..4] != b"FORM"
        || (&data[8..12] != b"AIFF" && &data[8..12] != b"AIFC")
    {
        return None;
    }
    let mut pos = 12usize;
    let mut channels = 0u16;
    let mut sample_rate = 0f64;
    let mut compression = [0u8; 4];
    let mut ssnd: Option<&[u8]> = None;

    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;
        let body_start = pos + 8;
        let body_end = (body_start + size).min(data.len());
        match id {
            b"COMM" => {
                if body_end - body_start >= 18 {
                    channels = u16::from_be_bytes([data[body_start], data[body_start + 1]]);
                    // 80-bit extended float sample rate at offset 8.
                    sample_rate = read_ieee80(&data[body_start + 8..body_start + 18]);
                    compression = [
                        data[body_start + 18],
                        data[body_start + 19],
                        data[body_start + 20],
                        data[body_start + 21],
                    ];
                }
            }
            b"SSND" => {
                // body: offset(4) blocksize(4) then sound data.
                if body_end - body_start >= 8 {
                    ssnd = Some(&data[body_start + 8..body_end]);
                }
            }
            _ => {}
        }
        pos = body_start + size + (size & 1); // chunks are word-aligned
    }

    let ssnd = ssnd?;
    let channels = channels.max(1) as usize;
    if compression != *b"ima4" {
        // Plain PCM AIFF ('NONE'/'sowt') — not present in retail, but cheap
        // to support: 16-bit big-endian samples.
        if compression == *b"NONE" {
            let mut out = Vec::with_capacity(ssnd.len() / 2);
            for c in ssnd.chunks_exact(2) {
                out.push(i16::from_be_bytes([c[0], c[1]]));
            }
            return Some(Sound {
                samples: out,
                channels,
                sample_rate: sample_rate as u32,
            });
        }
        return None;
    }

    // ima4: blocks of 34 bytes per channel, interleaved per 64-sample frame.
    let block = 34usize;
    let frames_per_round = 64usize * channels;
    let total_blocks = ssnd.len() / block;
    let usable = total_blocks - (total_blocks % channels);
    let mut out = Vec::with_capacity(usable / channels * frames_per_round);
    let mut chan_buf = vec![0i16; 64];
    let mut block_idx = 0usize;
    while block_idx + channels <= usable {
        // Decode one round: `channels` consecutive 34-byte blocks, which
        // together make 64 interleaved frames.
        let base = out.len();
        out.resize(base + 64 * channels, 0);
        for ch in 0..channels {
            let off = (block_idx + ch) * block;
            decode_ima4_block(&ssnd[off..off + block], &mut chan_buf);
            // Scatter this channel's 64 samples into the interleaved output.
            for k in 0..64 {
                out[base + k * channels + ch] = chan_buf[k];
            }
        }
        block_idx += channels;
    }
    Some(Sound {
        samples: out,
        channels,
        sample_rate: sample_rate as u32,
    })
}

/// Read a big-endian 80-bit IEEE extended float (used by AIFF sample rates).
fn read_ieee80(b: &[u8]) -> f64 {
    if b.len() < 10 {
        return 0.0;
    }
    let sign = (b[0] & 0x80) != 0;
    let exp = (((b[0] & 0x7F) as u32) << 8) | b[1] as u32;
    if exp == 0 {
        return 0.0;
    }
    // Mantissa is 64 bits; value = mantissa * 2^(exp - 16383 - 63).
    let mut mant: u64 = 0;
    for &byte in &b[2..10] {
        mant = (mant << 8) | byte as u64;
    }
    let v = (mant as f64) * 2f64.powi(exp as i32 - 16383 - 63);
    if sign {
        -v
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal AIFF-C `ima4` file with `packets` 34-byte blocks per channel.
    fn ima4_aifc(channels: u16, packets: u32) -> Vec<u8> {
        let mut comm = Vec::new();
        comm.extend_from_slice(&channels.to_be_bytes());
        comm.extend_from_slice(&packets.to_be_bytes());
        comm.extend_from_slice(&16u16.to_be_bytes());
        // 22050 Hz as an 80-bit extended float.
        comm.extend_from_slice(&[0x40, 0x0D, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        comm.extend_from_slice(b"ima4");
        let mut ssnd = vec![0u8; 8];
        for block in 0..packets * channels as u32 {
            // Preamble predictor differs per channel so they can be told apart.
            let predictor: i16 = if block % channels as u32 == 0 { 0x1000 } else { -0x1000 };
            ssnd.extend_from_slice(&(predictor as u16).to_be_bytes());
            ssnd.extend_from_slice(&[0u8; 32]);
        }
        let mut body = b"AIFC".to_vec();
        for (id, chunk) in [(b"COMM", &comm), (b"SSND", &ssnd)] {
            body.extend_from_slice(id);
            body.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
            body.extend_from_slice(chunk);
        }
        let mut file = b"FORM".to_vec();
        file.extend_from_slice(&(body.len() as u32).to_be_bytes());
        file.extend_from_slice(&body);
        file
    }

    #[test]
    fn stereo_ima4_interleaves_64_frames_per_packet() {
        let sound = decode_aiff_ima4(&ima4_aifc(2, 3)).expect("decode");
        assert_eq!(sound.channels, 2);
        assert_eq!(sound.sample_rate, 22050);
        // 3 packets of 64 frames, two samples per frame.
        assert_eq!(sound.samples.len(), 3 * 64 * 2);
        // Every frame carries both channels: left positive, right negative.
        for frame in sound.samples.chunks_exact(2) {
            assert!(frame[0] > 0 && frame[1] < 0, "frame {:?}", frame);
        }
    }

    #[test]
    fn mono_ima4_decodes_64_samples_per_packet() {
        let sound = decode_aiff_ima4(&ima4_aifc(1, 2)).expect("decode");
        assert_eq!(sound.samples.len(), 2 * 64);
    }
}
