//! Exporting the decoded textures as PNGs, and loading replacement PNGs
//! over them.
//!
//! Both trees share one layout, so an exported file can be copied into the
//! override tree, edited and picked up as it is:
//!
//! - `sprites/<id>/<frame>.png`: one `im08` sprite frame (frame numbered
//!   from 000, in the original's scan order)
//! - `images/<id>.png`: one `im16` image
//!
//! `<id>` is the four-character resource id. Ids that are not plain
//! letters and digits, or that only differ from another id by case (the
//! names must survive a case-insensitive file system), are written as
//! eight hex digits instead.
//!
//! An override is either the original's size or a whole multiple of it
//! (the same multiple across and down). The game keeps the original's size
//! for layout and collisions; a larger override is kept at full size for an
//! HD renderer (see [`Hd`]) and shrunk to the original's size for the
//! 640x480 one.

use super::GameAssets;
use crate::pak::{ResId, Tag};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Where overrides are read from unless `DEIMOS_TEXTURES` says otherwise
/// (set it empty to load none).
pub const DEFAULT_OVERRIDE_DIR: &str = "textures/override";
/// Where `--export-textures` writes unless given a directory.
pub const DEFAULT_EXPORT_DIR: &str = "textures/export";

/// An override's pixels at the size it was drawn: `scale` times the
/// original's width and height.
#[derive(Clone)]
pub struct Hd {
    pub scale: u32,
    pub rgba: Vec<u8>,
}

/// The override directory to use, if it exists.
pub fn override_dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("DEIMOS_TEXTURES") {
        Some(d) if d.is_empty() => return None,
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(DEFAULT_OVERRIDE_DIR),
    };
    dir.is_dir().then_some(dir)
}

/// File names for every resource of `tag`, keyed by id. For `im08` only
/// the colour plates are named: a sprite's alpha plate is its id in
/// uppercase, so an id without a lowercase letter is an alpha plate.
pub fn file_names(assets: &GameAssets, tag: Tag) -> HashMap<ResId, String> {
    let mut rids: Vec<ResId> = assets
        .set
        .entries_of_tag(tag)
        .iter()
        .map(|e| e.rid)
        .filter(|rid| tag != Tag::Im08 || rid.to_le_bytes().iter().any(|c| c.is_ascii_lowercase()))
        .collect();
    rids.sort();
    rids.dedup();
    let text = |rid: ResId| -> Option<String> {
        let b = rid.to_le_bytes();
        b.iter().all(|c| c.is_ascii_alphanumeric()).then(|| String::from_utf8_lossy(&b).into_owned())
    };
    let mut folds: HashMap<String, usize> = HashMap::new();
    for &rid in &rids {
        if let Some(t) = text(rid) {
            *folds.entry(t.to_ascii_lowercase()).or_default() += 1;
        }
    }
    rids.into_iter()
        .map(|rid| {
            let name = match text(rid) {
                Some(t) if folds[&t.to_ascii_lowercase()] == 1 => t,
                _ => format!("{rid:08x}"),
            };
            (rid, name)
        })
        .collect()
}

/// Read an override PNG for an image of `w` x `h`. None when there is no
/// file; a file that cannot be used is reported and skipped.
pub fn load_override(path: &Path, w: u32, h: u32) -> Option<Hd> {
    if !path.is_file() {
        return None;
    }
    let img = match image::open(path) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("texture override {}: {e}", path.display());
            return None;
        }
    };
    let (ow, oh) = img.dimensions();
    let scale = if w > 0 { ow / w } else { 0 };
    if scale == 0 || ow != w * scale || oh != h * scale {
        eprintln!(
            "texture override {}: {ow}x{oh} is not a whole multiple of the original {w}x{h}, skipped",
            path.display()
        );
        return None;
    }
    Some(Hd { scale, rgba: img.into_raw() })
}

/// `hd` shrunk to the original's size: each block of `scale` x `scale`
/// pixels averaged, colours weighted by their opacity so transparent
/// pixels do not darken the edges.
pub fn downscale(hd: &Hd, w: u32, h: u32) -> Vec<u8> {
    let k = hd.scale as usize;
    if k == 1 {
        return hd.rgba.clone();
    }
    let (w, h) = (w as usize, h as usize);
    let hw = w * k;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u64; 4];
            for sy in y * k..(y + 1) * k {
                for sx in x * k..(x + 1) * k {
                    let p = &hd.rgba[(sy * hw + sx) * 4..][..4];
                    let a = p[3] as u64;
                    for c in 0..3 {
                        sum[c] += p[c] as u64 * a;
                    }
                    sum[3] += a;
                }
            }
            let d = &mut out[(y * w + x) * 4..][..4];
            if sum[3] > 0 {
                for c in 0..3 {
                    d[c] = (sum[c] / sum[3]) as u8;
                }
            }
            d[3] = (sum[3] / (k * k) as u64) as u8;
        }
    }
    out
}

/// `--export-textures [dir]`: every sprite frame and image as a PNG, plus
/// `index.txt` naming each one.
pub fn export(assets: &mut GameAssets, out: &Path) -> Result<(), String> {
    let err = |p: &Path, e: &dyn std::fmt::Display| format!("{}: {e}", p.display());
    let mut index = String::from("# kind file id-hex size frames name\n");
    let displays = |assets: &GameAssets, tag: Tag| -> HashMap<ResId, String> {
        assets.set.entries_of_tag(tag).iter().map(|e| (e.rid, e.display.clone())).collect()
    };

    let sprite_names = file_names(assets, Tag::Im08);
    let sprite_display = displays(assets, Tag::Im08);
    let mut sprites: Vec<ResId> = sprite_names.keys().copied().collect();
    sprites.sort_by_key(|rid| sprite_names[rid].clone());
    let (mut n_sprites, mut n_frames) = (0, 0);
    for rid in sprites {
        let name = &sprite_names[&rid];
        let Some(sprite) = assets.sprite(rid) else { continue };
        let dir = out.join("sprites").join(name);
        std::fs::create_dir_all(&dir).map_err(|e| err(&dir, &e))?;
        for (i, f) in sprite.frames.iter().enumerate() {
            let p = dir.join(format!("{i:03}.png"));
            image::save_buffer(&p, &f.rgba, f.width, f.height, image::ColorType::Rgba8).map_err(|e| err(&p, &e))?;
        }
        let first = sprite.frames.first().map(|f| format!("{}x{}", f.width, f.height)).unwrap_or_default();
        index.push_str(&format!(
            "sprite sprites/{name} {rid:08x} {first} {} {}\n",
            sprite.frames.len(),
            sprite_display.get(&rid).map(String::as_str).unwrap_or("")
        ));
        n_sprites += 1;
        n_frames += sprite.frames.len();
    }

    let image_names = file_names(assets, Tag::Im16);
    let image_display = displays(assets, Tag::Im16);
    let mut images: Vec<ResId> = image_names.keys().copied().collect();
    images.sort_by_key(|rid| image_names[rid].clone());
    let dir = out.join("images");
    std::fs::create_dir_all(&dir).map_err(|e| err(&dir, &e))?;
    let mut n_images = 0;
    for rid in images {
        let name = &image_names[&rid];
        let Some((w, h, rgba)) = assets.image16(rid) else { continue };
        let p = dir.join(format!("{name}.png"));
        image::save_buffer(&p, rgba, *w, *h, image::ColorType::Rgba8).map_err(|e| err(&p, &e))?;
        index.push_str(&format!(
            "image images/{name}.png {rid:08x} {w}x{h} 1 {}\n",
            image_display.get(&rid).map(String::as_str).unwrap_or("")
        ));
        n_images += 1;
    }

    let p = out.join("index.txt");
    std::fs::write(&p, index).map_err(|e| err(&p, &e))?;
    println!("exported {n_sprites} sprites ({n_frames} frames) and {n_images} images to {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_weights_colour_by_opacity() {
        // One 2x2 block: an opaque red pixel and three clear black ones.
        let mut rgba = vec![0u8; 16];
        rgba[..4].copy_from_slice(&[255, 0, 0, 255]);
        let out = downscale(&Hd { scale: 2, rgba }, 1, 1);
        assert_eq!(out, vec![255, 0, 0, 63]);
    }

    #[test]
    fn overrides_must_be_whole_multiples() {
        let dir = std::env::temp_dir().join(format!("deimos-tex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, w: u32, h: u32| {
            let p = dir.join(name);
            image::save_buffer(&p, &vec![7u8; (w * h * 4) as usize], w, h, image::ColorType::Rgba8).unwrap();
            p
        };
        assert_eq!(load_override(&write("same.png", 4, 3), 4, 3).map(|h| h.scale), Some(1));
        assert_eq!(load_override(&write("double.png", 8, 6), 4, 3).map(|h| h.scale), Some(2));
        assert!(load_override(&write("uneven.png", 8, 3), 4, 3).is_none());
        assert!(load_override(&write("odd.png", 6, 4), 4, 3).is_none());
        assert!(load_override(&dir.join("missing.png"), 4, 3).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
