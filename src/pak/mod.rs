//! Reader for the original game's `.pak` archives.
//!
//! The paks are ordinary ZIP files in which every entry uses the *stored*
//! (uncompressed) method — the same restriction the original `unzip.c`
//! enforced. Each entry name has the form `Folder/Display Name[abcd].ext`;
//! the four characters inside the brackets are the resource ID and the
//! extension selects the resource tag (see `Tag`).
//!
//! Most non-media payloads (`unde`, `leve`, `plde`, `wede`, `flli`, `reli`,
//! `edli`, `coli`, `tefo`, `stli`, `pref`) are obfuscated with a per-byte
//! nibble-swap + XOR cipher, reproduced by [`decrypt`]. GIF/TGA/AIFF payloads
//! are stored verbatim.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Four-character resource ID packed little-endian, e.g. `id("bagb")`.
pub type ResId = u32;

pub const fn id(s: &str) -> ResId {
    let b = s.as_bytes();
    debug_assert!(b.len() == 4);
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub const NONE_ID: ResId = id("none");

/// Resource tag, identified by its four-character code.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tag {
    Im08, // 8-bit GIF sprites
    Im16, // 16-bit TGA images
    Soun, // sound effects (AIFF/IMA)
    Stli, // still-image / text pages
    Flli, // float/game-constant definitions
    Wede, // weapon definitions
    Reli, // rect lists
    Edli, // ID lists
    Coli, // colour lists
    Tefo, // text formats
    Plde, // player definitions
    Pref, // preferences
    Film, // demo/film recordings
    Unde, // unit definitions
    Leve, // level definitions
}

impl Tag {
    pub fn from_code(code: &[u8]) -> Option<Tag> {
        Some(match code {
            b"im08" => Tag::Im08,
            b"im16" => Tag::Im16,
            b"soun" => Tag::Soun,
            b"stli" => Tag::Stli,
            b"flli" => Tag::Flli,
            b"wede" => Tag::Wede,
            b"reli" => Tag::Reli,
            b"edli" => Tag::Edli,
            b"coli" => Tag::Coli,
            b"tefo" => Tag::Tefo,
            b"plde" => Tag::Plde,
            b"pref" => Tag::Pref,
            b"film" => Tag::Film,
            b"unde" => Tag::Unde,
            b"leve" => Tag::Leve,
            _ => return None,
        })
    }

    pub fn code(self) -> ResId {
        match self {
            Tag::Im08 => id("im08"),
            Tag::Im16 => id("im16"),
            Tag::Soun => id("soun"),
            Tag::Stli => id("stli"),
            Tag::Flli => id("flli"),
            Tag::Wede => id("wede"),
            Tag::Reli => id("reli"),
            Tag::Edli => id("edli"),
            Tag::Coli => id("coli"),
            Tag::Tefo => id("tefo"),
            Tag::Plde => id("plde"),
            Tag::Pref => id("pref"),
            Tag::Film => id("film"),
            Tag::Unde => id("unde"),
            Tag::Leve => id("leve"),
        }
    }

    /// Whether payloads of this tag are stored encrypted.
    pub fn is_encrypted(self) -> bool {
        !matches!(self, Tag::Im08 | Tag::Im16 | Tag::Soun | Tag::Film)
    }
}

/// Extension → tag mapping, recovered from the original's own tables
/// (`sub_405210` in `U_Pak.cpp`). Matching is case-insensitive here; the
/// original accepted only the listed casings, which only ever appear in
/// canonical form inside the shipped paks.
fn tag_for_extension(ext: &str) -> Option<Tag> {
    Some(match ext.to_ascii_lowercase().as_str() {
        ".im08" | ".gif" | ".giff" => Tag::Im08,
        ".im16" | ".tga" | ".targa" => Tag::Im16,
        ".snd" | ".sound" | ".aif" | ".aiff" | ".ima" => Tag::Soun,
        ".txt" | ".text" | ".string" | ".stli" => Tag::Stli,
        ".flt" | ".float" | ".flli" => Tag::Flli,
        ".we" | ".wep" | ".wede" => Tag::Wede,
        ".rect" | ".reli" | ".rectlist" => Tag::Reli,
        ".id" | ".idli" | ".idlist" => Tag::Edli,
        ".co" | ".coli" | ".color" | ".colorlist" => Tag::Coli,
        ".tefo" | ".textformat" => Tag::Tefo,
        ".plde" | ".player" => Tag::Plde,
        ".pref" => Tag::Pref,
        ".film" => Tag::Film,
        ".unde" | ".unitdef" | ".unit" => Tag::Unde,
        ".lvl" | ".leve" | ".level" => Tag::Leve,
        _ => return None,
    })
}

/// Parsed pak entry name: `Display Name[abcd].ext` under an optional folder.
#[derive(Clone, Debug)]
pub struct EntryName {
    /// Display name without the `[id]` suffix and extension.
    pub display: String,
    /// Resource ID from the `[...]` bracket.
    pub rid: ResId,
    /// Tag selected by extension.
    pub tag: Tag,
    /// Folder the entry lived in (e.g. `im08`, `unde`).
    pub folder: String,
}

pub fn parse_entry_name(name: &str) -> Option<EntryName> {
    let (folder, file) = match name.rsplit_once('/') {
        Some((f, file)) => (f.to_string(), file),
        None => (String::new(), name),
    };
    let dot = file.find('.')?;
    let (stem, ext_with_dot) = file.split_at(dot);
    let ext = &ext_with_dot[..ext_with_dot.len().min(16)];
    let open = stem.rfind('[')?;
    if !stem.ends_with(']') {
        return None;
    }
    let idpart = &stem[open + 1..stem.len() - 1];
    if idpart.len() != 4 {
        return None;
    }
    let tag = tag_for_extension(ext)?;
    Some(EntryName {
        display: stem[..open].to_string(),
        rid: id(idpart),
        tag,
        folder,
    })
}

/// The original's per-byte obfuscation: swap nibbles, then XOR 0xff.
/// It is an involution — the same routine encrypts and decrypts.
pub fn decrypt(data: &mut [u8]) {
    for b in data.iter_mut() {
        *b = ((*b >> 4) | (*b << 4)) ^ 0xff;
    }
}

#[derive(Clone, Copy)]
struct ZipEntry {
    name_off_data: u64, // offset of local file header
    data_off: u64,      // offset of file data (local header + name + extra)
    size: u64,
}

/// One opened `.pak` archive.
pub struct Pak {
    file: File,
    by_tag_id: HashMap<(u32, u32), ZipEntry>,
    // Keyed by (tag code, rid): the same four-character ID legitimately
    // appears under different tags (e.g. `gaco` is both a `.coli` and a
    // `.tefo` file).
    pub names: HashMap<(u32, u32), EntryName>,
}

impl Pak {
    /// Open a pak file and index its central directory.
    pub fn open(path: &Path) -> std::io::Result<Pak> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        // Locate End Of Central Directory (PK\x05\x06) scanning back from EOF.
        let mut eocd = None;
        let mut window_start = len.saturating_sub(0x10000);
        loop {
            let window_len = (len - window_start) as usize;
            let mut buf = vec![0u8; window_len];
            file.seek(SeekFrom::Start(window_start))?;
            file.read_exact(&mut buf)?;
            if let Some(pos) = buf.windows(4).rposition(|w| w == b"PK\x05\x06") {
                eocd = Some(window_start + pos as u64);
                break;
            }
            if window_start == 0 {
                break;
            }
            window_start = window_start.saturating_sub(0x10000 - 22);
        }
        let eocd = eocd.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "no EOCD record")
        })?;
        file.seek(SeekFrom::Start(eocd))?;
        let mut rec = [0u8; 22];
        file.read_exact(&mut rec)?;
        let cd_count = u16::from_le_bytes([rec[10], rec[11]]) as usize;
        let cd_size = u32::from_le_bytes([rec[12], rec[13], rec[14], rec[15]]) as u64;
        let cd_off = u32::from_le_bytes([rec[16], rec[17], rec[18], rec[19]]) as u64;

        file.seek(SeekFrom::Start(cd_off))?;
        let mut cd = vec![0u8; cd_size as usize];
        file.read_exact(&mut cd)?;

        let mut by_tag_id = HashMap::new();
        let mut names = HashMap::new();
        let mut pos = 0usize;
        for _ in 0..cd_count {
            if pos + 46 > cd.len() || &cd[pos..pos + 4] != b"PK\x01\x02" {
                break;
            }
            let method = u16::from_le_bytes([cd[pos + 10], cd[pos + 11]]);
            let _csize = u32::from_le_bytes([cd[pos + 20], cd[pos + 21], cd[pos + 22], cd[pos + 23]]) as u64;
            let usize_ = u32::from_le_bytes([cd[pos + 24], cd[pos + 25], cd[pos + 26], cd[pos + 27]]) as u64;
            let name_len = u16::from_le_bytes([cd[pos + 28], cd[pos + 29]]) as usize;
            let extra_len = u16::from_le_bytes([cd[pos + 30], cd[pos + 31]]) as usize;
            let comment_len = u16::from_le_bytes([cd[pos + 32], cd[pos + 33]]) as usize;
            let local_off = u32::from_le_bytes([cd[pos + 42], cd[pos + 43], cd[pos + 44], cd[pos + 45]]) as u64;
            let name_start = pos + 46;
            let name_end = name_start + name_len;
            let raw_name = std::str::from_utf8(&cd[name_start..name_end])
                .unwrap_or("")
                .to_string();
            pos = name_end + extra_len + comment_len;

            // Directories have zero size and no parseable tag.
            if usize_ == 0 && raw_name.ends_with('/') {
                continue;
            }
            if method != 0 {
                // The original asserts on compressed entries; ours are all stored.
                continue;
            }
            let Some(parsed) = parse_entry_name(&raw_name) else {
                continue;
            };

            // Resolve the data offset through the local header, whose name and
            // extra-field lengths can differ from the central directory's.
            let data_off = Self::local_data_offset(&mut file, local_off)?;
            let entry = ZipEntry {
                name_off_data: local_off,
                data_off,
                size: usize_,
            };
            by_tag_id.insert((parsed.tag.code(), parsed.rid), entry);
            names.insert((parsed.tag.code(), parsed.rid), parsed);
        }
        Ok(Pak {
            file,
            by_tag_id,
            names,
        })
    }

    fn local_data_offset(file: &mut File, local_off: u64) -> std::io::Result<u64> {
        file.seek(SeekFrom::Start(local_off))?;
        let mut hdr = [0u8; 30];
        file.read_exact(&mut hdr)?;
        if &hdr[0..4] != b"PK\x03\x04" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "bad local header",
            ));
        }
        let name_len = u16::from_le_bytes([hdr[26], hdr[27]]) as u64;
        let extra_len = u16::from_le_bytes([hdr[28], hdr[29]]) as u64;
        Ok(local_off + 30 + name_len + extra_len)
    }

    /// Look up an entry by tag + ID.
    pub fn find(&self, tag: Tag, rid: ResId) -> Option<&EntryName> {
        self.names.get(&(tag.code(), rid))
    }

    /// Read an entry's raw bytes (still encrypted if applicable).
    pub fn read_raw(&mut self, tag: Tag, rid: ResId) -> Option<Vec<u8>> {
        let entry = *self.by_tag_id.get(&(tag.code(), rid))?;
        let mut buf = vec![0u8; entry.size as usize];
        self.file.seek(SeekFrom::Start(entry.data_off)).ok()?;
        self.file.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    /// Read and decrypt a text-format entry.
    pub fn read_text(&mut self, tag: Tag, rid: ResId) -> Option<String> {
        let mut buf = self.read_raw(tag, rid)?;
        if tag.is_encrypted() {
            decrypt(&mut buf);
        }
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    /// All parsed entry names of a given tag, sorted by display name for
    /// deterministic iteration (the original iterated the zip order).
    pub fn entries_of_tag(&self, tag: Tag) -> Vec<&EntryName> {
        let mut v: Vec<&EntryName> = self
            .names
            .values()
            .filter(|n| n.tag == tag)
            .collect();
        v.sort_by(|a, b| a.display.cmp(&b.display));
        v
    }
}

/// Convenience: open every pak in a directory (e.g. `retail/ Data/Paks`).
pub struct PakSet {
    pub paks: Vec<(PathBuf, Pak)>,
}

impl PakSet {
    /// Open all `*.pak` files in `dir`.
    pub fn open_dir(dir: &Path) -> std::io::Result<PakSet> {
        let mut paks = Vec::new();
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension()
                    .map(|e| e.eq_ignore_ascii_case("pak"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for p in paths {
            match Pak::open(&p) {
                Ok(pak) => paks.push((p.clone(), pak)),
                Err(e) => eprintln!("deimos: failed to open {:?}: {}", p, e),
            }
        }
        Ok(PakSet { paks })
    }

    /// Find a resource across all paks. Media paks and the game pak have
    /// disjoint tag spaces in practice, but search everything anyway.
    pub fn find(&self, tag: Tag, rid: ResId) -> Option<(usize, &EntryName)> {
        for (i, (_, pak)) in self.paks.iter().enumerate() {
            if let Some(name) = pak.find(tag, rid) {
                return Some((i, name));
            }
        }
        None
    }

    /// Read raw bytes for a resource.
    pub fn read_raw(&mut self, tag: Tag, rid: ResId) -> Option<Vec<u8>> {
        for (_, pak) in self.paks.iter_mut() {
            if let Some(data) = pak.read_raw(tag, rid) {
                return Some(data);
            }
        }
        None
    }

    /// Read and decrypt a text resource.
    pub fn read_text(&mut self, tag: Tag, rid: ResId) -> Option<String> {
        for (_, pak) in self.paks.iter_mut() {
            if let Some(data) = pak.read_text(tag, rid) {
                return Some(data);
            }
        }
        None
    }

    /// List every entry of a tag across all paks.
    pub fn entries_of_tag(&self, tag: Tag) -> Vec<&EntryName> {
        let mut v = Vec::new();
        for (_, pak) in &self.paks {
            v.extend(pak.entries_of_tag(tag));
        }
        v
    }
}
