//! Everything the original does at startup (and at level load) to put
//! data into memory, so the ported modules find it where the original
//! keeps it: the permanent resource tables of G_Res, the master lists of
//! unit, weapon and player definitions, the unit families and the level
//! list. Also `PakHost`, the `Host` that reads the original's pak files.
//!
//! Startup order of the original (0x452090..0x4525bc): `G_Res_Init`,
//! `G_Res_LoadPermData`, ..., `G_Res_LoadPermResources`, `G_Level_Init`,
//! `G_UnitDef_Init`, `G_WepDef_Init`, `G_PlayerDef_Init`; see
//! [`load_game_data`]. A level is loaded with [`load_level`].
//!
//! Definitions are parsed by unitdef.rs / wepdef.rs / playerdef.rs into
//! byte images and copied into `g.m` at `alloc`ed addresses; spawn sets
//! become `U_LinkedList`s of 0x56-byte (units, hung at state +0x5d6) or
//! 0x32-byte (weapons, at +0x1c0) blocks, created only when a state or
//! weapon has any, like the parsers do.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use super::g::{Host, G};
use super::level::{self, G_Level_Init, G_Level_LoadByLevelListNumber, LEVEL_SIZE};
use super::mem::list;
use super::playerdef::{self, PlayerDef};
use super::resource::*;
use super::token::{html_to_pixel16, scan_f, scan_i};
use super::unitdef::UnitDef;
use super::unitdefs::{self, sub_4419d0, sub_441870, sub_441d30};
use super::wepdef::WepDef;
use super::wepdefs;
use crate::pak::{self, PakSet, Tag};

const NONE: u32 = 0x656e6f6e;
/// Pak tags as the original names them (`U_Pak_TagType`).
pub const TAG_IM08: u32 = 0x38306d69;
pub const TAG_SOUN: u32 = 0x6e756f73;
pub const TAG_STLI: u32 = 0x696c7473;
pub const TAG_FLLI: u32 = 0x696c6c66;
pub const TAG_WEDE: u32 = 0x65646577;
pub const TAG_RELI: u32 = 0x696c6572;
pub const TAG_EDLI: u32 = 0x696c6465;
pub const TAG_COLI: u32 = 0x696c6f63;
pub const TAG_PLDE: u32 = 0x65646c70;
pub const TAG_UNDE: u32 = 0x65646e75;
pub const TAG_LEVE: u32 = 0x6576656c;

// G_PlayerDef globals.
/// `g_playerDefInitialised` (byte).
pub const PLAYERDEF_INITIALISED: u32 = 0x4ed5f3;
/// `g_playerDefMasterList`: U_LinkedList of `G_PlayerDef *` (0x104 bytes).
pub const PLAYERDEF_MASTER_LIST: u32 = 0x4ed5f4;

/// The module's static tag ids and copied statics, as `sub_038d20`
/// (0x438d20, run before `main`) leaves them in G_Resource's data block
/// (0x4ee118): offset, id.
const RES_TAG_IDS: [(u32, &[u8; 4]); 26] = [
    (0x2b, b"pali"),
    (0x2f, b"pref"),
    (0x33, b"adve"),
    (0x37, b"back"),
    (0x3b, b"brie"),
    (0x3f, b"lese"),
    (0x43, b"decr"),
    (0x47, b"edpa"),
    (0x4b, b"pucr"),
    (0x4f, b"scor"),
    (0x53, b"publ"),
    (0x57, b"cred"),
    (0x5b, b"inte"),
    (0x5f, b"edit"),
    (0x63, b"pgsl"),
    (0x67, b"tesp"),
    (0x6b, b"gaob"),
    (0x6f, b"gasp"),
    (0x73, b"gaso"),
    (0x77, b"gate"),
    (0x7b, b"edit"),
    (0x7f, b"inmu"),
    (0x83, b"ammu"),
    (0x87, b"inre"),
    (0x8b, b"gafl"),
    (0x8f, b"gaco"),
];

// ---------------------------------------------------------------------
// PakHost
// ---------------------------------------------------------------------

/// A `Host` over the original's paks. Pak access (`read_text`,
/// `ids_of_tag`, `has_tag`, `has_image`) is answered here; sprite sizes,
/// image pixels and sound playback are forwarded to `inner`.
pub struct PakHost {
    set: RefCell<PakSet>,
    /// Entry ids per tag in the original's tag index order: pak files by
    /// name, entries in zip order (`U_Pak_BuildTagIndex`).
    order: HashMap<u32, Vec<u32>>,
    inner: Box<dyn Host>,
}

impl PakHost {
    /// Opens every `*.pak` in `dir` (e.g. `retail/ Data/Paks`).
    pub fn open(dir: &Path, inner: Box<dyn Host>) -> std::io::Result<PakHost> {
        Ok(PakHost::new(PakSet::open_dir(dir)?, inner))
    }

    pub fn new(set: PakSet, inner: Box<dyn Host>) -> PakHost {
        let mut order: HashMap<u32, Vec<u32>> = HashMap::new();
        for (path, _) in &set.paks {
            for (tag, id) in zip_entry_order(path) {
                order.entry(tag).or_default().push(id);
            }
        }
        PakHost { set: RefCell::new(set), order, inner }
    }
}

/// (tag code, resource id) of the entries of a stored-method zip in
/// central directory order, the order `Unzip_GetNextEntryInfo` yields.
/// Entries with size 0 or an unparseable name are skipped, like the
/// original's index build.
fn zip_entry_order(path: &Path) -> Vec<(u32, u32)> {
    let Ok(data) = std::fs::read(path) else { return Vec::new() };
    let le16 = |o: usize| u16::from_le_bytes([data[o], data[o + 1]]) as usize;
    let le32 = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap()) as usize;
    let Some(eocd) = (0..data.len().saturating_sub(21)).rev().find(|&i| &data[i..i + 4] == b"PK\x05\x06")
    else {
        return Vec::new();
    };
    let count = le16(eocd + 10);
    let mut pos = le32(eocd + 16);
    let mut out = Vec::new();
    for _ in 0..count {
        if pos + 46 > data.len() || &data[pos..pos + 4] != b"PK\x01\x02" {
            break;
        }
        let method = le16(pos + 10);
        let size = le32(pos + 24);
        let name_len = le16(pos + 28);
        let extra_len = le16(pos + 30);
        let comment_len = le16(pos + 32);
        let name = String::from_utf8_lossy(&data[pos + 46..pos + 46 + name_len]).into_owned();
        pos += 46 + name_len + extra_len + comment_len;
        if size == 0 || method != 0 {
            continue;
        }
        if let Some(e) = pak::parse_entry_name(&name) {
            out.push((e.tag.code(), e.rid));
        }
    }
    out
}

impl Host for PakHost {
    fn sprite_frame_size(&self, id: u32, frame: i32) -> Option<(i32, i32)> {
        self.inner.sprite_frame_size(id, frame)
    }
    fn sprite_num_frames(&self, id: u32) -> i32 {
        self.inner.sprite_num_frames(id)
    }
    fn has_image(&self, id: u32) -> bool {
        self.has_tag(TAG_IM08, id)
    }
    fn image_size(&self, id: u32) -> Option<(i32, i32)> {
        self.inner.image_size(id)
    }
    fn image_pixels16(&self, id: u32) -> Option<(i32, i32, Vec<u16>)> {
        self.inner.image_pixels16(id)
    }
    fn sound_play(&mut self, id: u32, volume: i32, pitch: f32) {
        self.inner.sound_play(id, volume, pitch)
    }
    fn sound_is_playing(&self, id: u32) -> bool {
        self.inner.sound_is_playing(id)
    }
    fn read_text(&self, tag: u32, id: u32) -> Option<Vec<u8>> {
        let t = Tag::from_code(&tag.to_le_bytes())?;
        let mut raw = self.set.borrow_mut().read_raw(t, id)?;
        if t.is_encrypted() {
            pak::decrypt(&mut raw);
        }
        Some(raw)
    }
    fn ids_of_tag(&self, tag: u32) -> Vec<u32> {
        self.order.get(&tag).cloned().unwrap_or_default()
    }
    fn has_tag(&self, tag: u32, id: u32) -> bool {
        match Tag::from_code(&tag.to_le_bytes()) {
            Some(t) => self.set.borrow().find(t, id).is_some(),
            None => false,
        }
    }
}

// ---------------------------------------------------------------------
// Pak text helpers (U_Pak_GetObjectID / GetRect / GetColor / GetString)
// ---------------------------------------------------------------------

/// The values `U_Token_Get(text, offset, "#", &offset, "<", ">")` yields
/// in turn: after each '#', the text between the next '<' and '>'
/// (`strtok` semantics: '>' directly after '<' is skipped over).
fn list_values(text: &[u8]) -> Vec<&[u8]> {
    let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(h) = text[pos..].iter().position(|&b| b == b'#') {
        let p = pos + h;
        let Some(lt) = text[p..].iter().position(|&b| b == b'<') else { break };
        let mut s = p + lt + 1;
        while s < text.len() && text[s] == b'>' {
            s += 1;
        }
        if s >= text.len() {
            break;
        }
        let e = text[s..].iter().position(|&b| b == b'>').map(|x| s + x).unwrap_or(text.len());
        out.push(&text[s..e]);
        pos = s;
    }
    out
}

fn tag_text(g: &G, tag: u32, id_addr: u32) -> Result<(Vec<u8>, u32), String> {
    let id = g.m.r32(id_addr);
    match g.host.read_text(tag, id) {
        Some(t) => Ok((t, id)),
        None => Err(format!("pak: no {} entry '{}'", fourcc(tag), fourcc(id))),
    }
}

fn fourcc(v: u32) -> String {
    String::from_utf8_lossy(&v.to_le_bytes()).into_owned()
}

/// `U_Pak_GetObjectID` (0x404c30): the `index`th id of an 'edli' list
/// whose tag id is stored at `id_addr`; 'none' past its end.
fn pak_get_object_id(g: &G, id_addr: u32, index: usize) -> Result<u32, String> {
    let (text, id) = tag_text(g, TAG_EDLI, id_addr)?;
    match list_values(&text).get(index) {
        Some(v) if v.len() == 4 => Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]])),
        Some(v) => Err(format!("edli '{}' entry {index}: ids are 4 characters", fourcc(id))),
        None => Ok(NONE),
    }
}

/// `U_Pak_GetRect` (0x4046e0): the `index`th rect of a 'reli' list as
/// {top, left, bottom, right}. File order is left, top, right, bottom.
fn pak_get_rect(g: &G, id_addr: u32, index: usize) -> Result<[i32; 4], String> {
    let (text, id) = tag_text(g, TAG_RELI, id_addr)?;
    let Some(v) = list_values(&text).get(index).copied() else {
        return Err(format!("reli '{}': no rect {index}", fourcc(id)));
    };
    let mut r = [0i32; 4]; // top, left, bottom, right
    if !v.is_empty() {
        let mut value = 0i32;
        for (n, tok) in v.split(|&b| b == b',').filter(|t| !t.is_empty()).take(4).enumerate() {
            if let Some(x) = scan_i(tok) {
                value = x;
            }
            match n {
                0 => r[1] = value,
                1 => r[0] = value,
                2 => r[3] = value,
                _ => r[2] = value,
            }
        }
    }
    Ok(r)
}

/// `U_Pak_GetColor` (0x404a10): the `index`th colour of a 'coli' list as
/// a 16-bit pixel (0 when unparseable).
fn pak_get_color(g: &G, id_addr: u32, index: usize) -> Result<u16, String> {
    let (text, id) = tag_text(g, TAG_COLI, id_addr)?;
    match list_values(&text).get(index) {
        Some(v) => Ok(html_to_pixel16(v).unwrap_or(0)),
        None => Err(format!("coli '{}': no colour {index}", fourcc(id))),
    }
}

/// `U_Pak_GetString` (0x404530): line `index` of a string list ('\r'
/// separated), copied into `out` (`size` bytes, zero filled, truncated).
/// None when the line does not exist or is empty.
fn pak_get_string(text: &[u8], index: usize, size: usize) -> Option<Vec<u8>> {
    let mut start = 0;
    if index != 0 {
        let mut count = 0;
        let mut ok = false;
        for (i, &b) in text.iter().enumerate() {
            if b == b'\r' {
                count += 1;
                if count == index {
                    start = i + 1;
                    ok = true;
                    break;
                }
            }
        }
        if !ok {
            return None;
        }
    }
    let len = text[start..].iter().position(|&b| b == b'\r').unwrap_or(text.len() - start);
    if len == 0 {
        return None;
    }
    let mut out = vec![0u8; size];
    let n = len.min(size - 1);
    out[..n].copy_from_slice(&text[start..start + n]);
    Some(out)
}

// ---------------------------------------------------------------------
// G_Res
// ---------------------------------------------------------------------

/// `sub_038d20`: the ids G_Resource keeps its tag lookups under.
pub fn init_res_statics(g: &mut G) {
    for (off, id) in RES_TAG_IDS {
        g.m.write(0x4ee118 + off, id);
    }
}

/// `G_Res_LoadPermData` (0x437b10): fills the permanent tables — sprite
/// ids (8, 'gasp'), sound ids (0x18, 'gaso'), rects (0x16, 'inre'),
/// floats (0xdc, 'gafl'), the colour ('gaco') and object ids (0x28,
/// 'gaob').
pub fn G_Res_LoadPermData(g: &mut G) -> Result<(), String> {
    g.m.w8(PERM_DATA_LOADED, 1);
    for i in 0..8 {
        let v = pak_get_object_id(g, TAG_ID_SPRITES, i)?;
        g.m.w32(PERM_SPRITE_IDS + i as u32 * 4, v);
    }
    for i in 0..0x18 {
        let v = pak_get_object_id(g, TAG_ID_SOUNDS, i)?;
        g.m.w32(PERM_SOUND_IDS + i as u32 * 4, v);
    }
    for i in 0..0x16 {
        let r = pak_get_rect(g, TAG_ID_RECTS, i)?;
        for k in 0..4 {
            g.m.wi32(PERM_RECTS + i as u32 * 16 + k as u32 * 4, r[k]);
        }
    }
    sub_4383b0(g)?;
    for i in 0..1 {
        let v = pak_get_color(g, TAG_ID_COLORS, i)?;
        g.m.w16(PERM_COLOR + i as u32 * 2, v);
    }
    for i in 0..0x28 {
        let v = pak_get_object_id(g, TAG_ID_OBJECTS, i)?;
        g.m.w32(PERM_OBJECT_IDS + i as u32 * 4, v);
    }
    Ok(())
}

/// `sub_4383b0` (0x4383b0): the permanent floats, 0xdc of them, in file
/// order, each read with `sscanf("%f")`.
pub fn sub_4383b0(g: &mut G) -> Result<(), String> {
    let (text, id) = tag_text(g, TAG_FLLI, TAG_ID_FLOATS)?;
    let mut count = 0usize;
    for v in list_values(&text) {
        if let Some(f) = scan_f(v) {
            g.m.wf32(PERM_FLOATS + count as u32 * 4, f);
        }
        count += 1;
        if count >= 0xdc {
            return Ok(());
        }
    }
    Err(format!("flli '{}' has {count} floats, expected 220", fourcc(id)))
}

/// `G_Res_LoadPermResources` (0x437c70): loads the permanent sprites
/// (all must exist) and sounds into the resource list and reads the 0x25
/// game strings ('pgsl') into `g_permGameStrings`.
pub fn G_Res_LoadPermResources(g: &mut G) -> Result<(), String> {
    for i in 0..8 {
        let id = g.m.ri32(PERM_SPRITE_IDS + i * 4);
        if !G_Res_Load(g, RES_TYPE_IMAGE, id, 0) {
            return Err(format!("permanent sprite '{}' cannot be loaded", fourcc(id as u32)));
        }
    }
    for i in 0..0x18 {
        let id = g.m.ri32(PERM_SOUND_IDS + i * 4);
        G_Res_Load(g, RES_TYPE_SOUND, id, 0);
    }
    let (text, id) = tag_text(g, TAG_STLI, TAG_ID_GAME_STRINGS)?;
    for i in 0..0x25usize {
        let Some(s) = pak_get_string(&text, i, 0x80) else {
            return Err(format!("stli '{}': missing string {i}", fourcc(id)));
        };
        g.m.write(PERM_GAME_STRINGS + i as u32 * 0x80, &s);
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------

/// Copies a parsed unit definition into a fresh 0x79e0-byte block and
/// hangs its spawn sets (U_LinkedList of 0x56-byte blocks) at each
/// state's +0x5d6; the original creates a state's list only when it has
/// sets. Returns the `G_UnitDef` address.
pub fn write_unit_def(g: &mut G, def: &UnitDef) -> u32 {
    let a = g.m.alloc(def.img.len() as u32);
    g.m.write(a, &def.img);
    for (i, sets) in def.spawn_sets.iter().enumerate() {
        if sets.is_empty() {
            continue;
        }
        let l = list::new(&mut g.m);
        for s in sets {
            let b = g.m.alloc(s.len() as u32);
            g.m.write(b, s);
            list::add(&mut g.m, l, b);
        }
        g.m.w32(a + unitdefs::STATE_BASE + i as u32 * unitdefs::STATE_SIZE + unitdefs::STATE_SPAWN_LIST, l);
    }
    a
}

/// Copies a parsed weapon definition into a fresh 0x200-byte block; its
/// spawn sets (0x32-byte blocks) go into a list at +0x1c0 when there are
/// any.
pub fn write_wep_def(g: &mut G, def: &WepDef) -> u32 {
    let a = g.m.alloc(def.img.len() as u32);
    g.m.write(a, &def.img);
    if !def.spawn_sets.is_empty() {
        let l = list::new(&mut g.m);
        for s in &def.spawn_sets {
            let b = g.m.alloc(s.len() as u32);
            g.m.write(b, s);
            list::add(&mut g.m, l, b);
        }
        g.m.w32(a + wepdefs::SPAWN_LIST, l);
    }
    a
}

/// Copies a parsed player definition into a fresh 0x104-byte block.
pub fn write_player_def(g: &mut G, def: &PlayerDef) -> u32 {
    let a = g.m.alloc(def.img.len() as u32);
    g.m.write(a, &def.img);
    a
}

/// `G_UnitDef_Init` (0x43fe60) / `G_UnitDef_MasterList_Build` (0x43ffe0):
/// every 'unde' in tag index order becomes a definition on the master
/// list and goes into its family (by name, `" Misc"` when it has none).
pub fn G_UnitDef_Init(g: &mut G) -> Result<(), String> {
    g.m.w8(unitdefs::INITIALISED, 1);
    G_UnitDef_MasterList_Build(g)
}

pub fn G_UnitDef_MasterList_Build(g: &mut G) -> Result<(), String> {
    sub_441870(g);
    let master = list::new(&mut g.m);
    g.m.w32(unitdefs::MASTER_LIST, master);
    sub_441d30(g);
    let fams = list::new(&mut g.m);
    g.m.w32(unitdefs::FAMILY_LIST, fams);
    for id in g.host.ids_of_tag(TAG_UNDE) {
        let Some(text) = g.host.read_text(TAG_UNDE, id) else { break };
        let host = &g.host;
        let def = UnitDef::parse(id, &text, &|i| host.has_image(i));
        let a = write_unit_def(g, &def);
        list::add(&mut g.m, master, a);
        let n = g.m.ri32(a + 0x12);
        if !(n > 0 && n <= 0x14) {
            return Err(format!("unit '{}': incorrect number of states ({n})", fourcc(id)));
        }
        sub_4419d0(g, a);
    }
    Ok(())
}

/// `G_WepDef_Init` (0x444d60) / `G_WepDef_MasterList_Build` (0x444de0).
pub fn G_WepDef_Init(g: &mut G) -> Result<(), String> {
    g.m.w8(wepdefs::INITIALISED, 1);
    G_WepDef_MasterList_Build(g)
}

pub fn G_WepDef_MasterList_Build(g: &mut G) -> Result<(), String> {
    wepdefs::G_WepDef_ShutDownInternal(g);
    let master = list::new(&mut g.m);
    g.m.w32(wepdefs::MASTER_LIST, master);
    for id in g.host.ids_of_tag(TAG_WEDE) {
        let Some(text) = g.host.read_text(TAG_WEDE, id) else { break };
        let host = &g.host;
        let def = WepDef::parse(id, &text, &|i| host.has_image(i));
        let a = write_wep_def(g, &def);
        list::add(&mut g.m, master, a);
    }
    Ok(())
}

/// `G_PlayerDef_Init` (0x435bb0) / `G_PlayerDef_MasterList_Build`
/// (0x435c30).
pub fn G_PlayerDef_Init(g: &mut G) -> Result<(), String> {
    g.m.w8(PLAYERDEF_INITIALISED, 1);
    G_PlayerDef_MasterList_Build(g)
}

/// `G_PlayerDef_ShutDownInternal` (0x4365f0).
pub fn G_PlayerDef_ShutDownInternal(g: &mut G) {
    let master = g.m.r32(PLAYERDEF_MASTER_LIST);
    if g.m.r8(PLAYERDEF_INITIALISED) == 0 || master == 0 {
        return;
    }
    for p in list::objects(&g.m, master) {
        if p != 0 && g.m.r32(p) == 0x499602d2 {
            g.m.free(p);
        }
    }
    list::delete_all(&mut g.m, master);
    g.m.free(master);
    g.m.w32(PLAYERDEF_MASTER_LIST, 0);
}

pub fn G_PlayerDef_MasterList_Build(g: &mut G) -> Result<(), String> {
    G_PlayerDef_ShutDownInternal(g);
    let master = list::new(&mut g.m);
    g.m.w32(PLAYERDEF_MASTER_LIST, master);
    for id in g.host.ids_of_tag(TAG_PLDE) {
        let Some(text) = g.host.read_text(TAG_PLDE, id) else { break };
        let host = &g.host;
        let def = PlayerDef::parse(id, &text, &|i| host.has_image(i));
        let a = write_player_def(g, &def);
        list::add(&mut g.m, master, a);
    }
    Ok(())
}

/// `G_PlayerDef_MasterList_GetRefPtrForIndex` (0x435f10).
pub fn G_PlayerDef_MasterList_GetRefPtrForIndex(g: &mut G, index: i32) -> u32 {
    let l = g.m.r32(PLAYERDEF_MASTER_LIST);
    list::nth(&g.m, l, index)
}

/// `G_PlayerDef_MasterList_GetIndexForID` (0x435e90).
pub fn G_PlayerDef_MasterList_GetIndexForID(g: &mut G, id: i32) -> i32 {
    if id as u32 == NONE {
        return -1;
    }
    let l = g.m.r32(PLAYERDEF_MASTER_LIST);
    for (i, p) in list::objects(&g.m, l).into_iter().enumerate() {
        if g.m.ri32(p + 4) == id {
            return i as i32;
        }
    }
    -1
}

// ---------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------

/// Everything the original loads at startup, in its order. `g.host` must
/// answer the pak queries (see [`PakHost`]). Afterwards the perm tables
/// (`G_Res_GetPerm*`), the unit, weapon and player master lists with the
/// unit families, and the level list are in memory.
pub fn load_game_data(g: &mut G) -> Result<(), String> {
    init_res_statics(g);
    G_Res_Init(g);
    G_Res_LoadPermData(g)?;
    G_Res_LoadPermResources(g)?;
    G_Level_Init(g);
    G_UnitDef_Init(g)?;
    G_WepDef_Init(g)?;
    G_PlayerDef_Init(g)?;
    Ok(())
}

/// What `sub_41fc80` does with the level modules when a level starts:
/// resets the unit resource-loading list, allocates a `G_Level` record
/// and the level-object list and loads the level with the given 1-based
/// number. Returns (level record, object list); dispose the objects with
/// `G_Level_DisposeLevelObjects`.
pub fn load_level(g: &mut G, one_based: i32) -> (u32, u32) {
    unitdefs::G_UnitDef_ResetResourceLoadingList(g);
    let lvl = g.m.alloc(LEVEL_SIZE);
    let objects = list::new(&mut g.m);
    G_Level_LoadByLevelListNumber(g, one_based, lvl, objects);
    (lvl, objects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::og::g::NullHost;
    use crate::game::og::mem::Mem;
    use crate::game::og::unitdefs::*;

    fn retail() -> Option<std::path::PathBuf> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for c in [root.join("retail"), root.join("../retail")] {
            if c.join("DeimosRising.exe").exists() {
                return Some(c);
            }
        }
        None
    }

    fn game() -> Option<G> {
        let dir = retail()?;
        let mut m = Mem::new();
        m.map_pe(&std::fs::read(dir.join("DeimosRising.exe")).ok()?).unwrap();
        let host = PakHost::open(&dir.join(" Data/Paks"), Box::new(NullHost)).ok()?;
        let mut g = G::new(m, Box::new(host));
        load_game_data(&mut g).expect("load");
        Some(g)
    }

    #[test]
    fn values_of_a_list() {
        let t = b"#Player 1\t\t<pl01>\r#Player 2\t\t<pl02>\r#x\t\t<calg>\t\t\t\r#y <>\r";
        let v = list_values(t);
        assert_eq!(v[0], b"pl01");
        assert_eq!(v[1], b"pl02");
        assert_eq!(v[2], b"calg");
        assert_eq!(pak_get_string(b"a\rbc\rd", 1, 8).unwrap()[..2], *b"bc");
    }

    #[test]
    fn loads_everything() {
        let Some(mut g) = game() else { return };
        // Perm tables.
        assert_eq!(G_Res_GetPermObjectID(&mut g, 0) as u32, u32::from_le_bytes(*b"pl01"));
        assert_eq!(G_Res_GetPermObjectID(&mut g, 1) as u32, u32::from_le_bytes(*b"pl02"));
        assert_eq!(G_Res_GetPermFloat(&mut g, 0), 2.0);
        assert_eq!(G_Res_GetPermFloat(&mut g, 1), 60.0);
        let ptr = G_Res_GetPermGameString(&mut g, 0);
        assert_eq!(g.m.cstr(ptr), b"Press Caps Lock");
        let mut r = [0i32; 4];
        let out = g.m.alloca(16);
        G_Res_GetPermRect(&mut g, 0, out);
        for k in 0..4 {
            r[k] = g.m.ri32(out + 4 * k as u32);
        }
        // "#Scorebar Player 1 Score <25, 81, 135, 95>": left 25, top 81,
        // right 135, bottom 95.
        assert_eq!(r, [81, 25, 95, 135]);
        // Units.
        let master = g.m.r32(MASTER_LIST);
        assert_eq!(list::num(&g.m, master), 386);
        let first = G_UnitDef_MasterList_GetRefPtrForIndex(&mut g, 0);
        assert_eq!(g.m.r32(first), 0x499602d2);
        let id = g.m.ri32(first + 4);
        assert_eq!(G_UnitDef_MasterList_GetRefPtrForID(&mut g, id), first);
        let fam = G_UnitDef_MasterList_GetFamilyPtrForUnitDef(&mut g, first);
        assert_ne!(fam, 0);
        assert_eq!(G_UnitDef_MasterList_GetRefPtrForID_ByFamily(&mut g, id, fam), first);
        let fams = g.m.r32(FAMILY_LIST);
        assert_eq!(list::num(&g.m, fams), g.m.ri32(NUM_FAMILIES));
        assert_eq!(g.m.ri32(NUM_UNITS_IN_FAMILIES), 386);
        // Weapons.
        let wl = g.m.r32(wepdefs::MASTER_LIST);
        assert_eq!(list::num(&g.m, wl), 5);
        // Players.
        let p0 = G_PlayerDef_MasterList_GetRefPtrForIndex(&mut g, 0);
        assert_ne!(p0, 0);
        assert_eq!(g.m.r32(p0), 0x499602d2);
        // Levels.
        assert_eq!(level::G_Level_GetNumInLevelList(&mut g), 12);
        let id1 = level::G_Level_GetLevelIDFor1BasedLevelListNumber(&mut g, 1);
        assert_eq!(level::G_Level_Get1BasedNumForLevelID(&mut g, id1), 1);
        let (lvl, objs) = load_level(&mut g, 1);
        assert!(list::num(&g.m, objs) > 0);
        assert_eq!(list::num(&g.m, objs), g.m.ri32(lvl + 0x278));
        // Spawn sets of some unit state hang as lists.
        let mut sets = 0;
        for u in list::objects(&g.m, master) {
            for s in 0..20u32 {
                if g.m.r32(u + STATE_BASE + s * STATE_SIZE + STATE_SPAWN_LIST) != 0 {
                    sets += 1;
                }
            }
        }
        assert!(sets > 0);
    }
}
