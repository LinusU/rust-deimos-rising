//! Port of the original's level module (G_Level.cc, 0x429410..0x42a000):
//! the level list (id and 1-based number of each of the twelve levels, in
//! the order of the `g_demoLevelNames` table) and the parser of a 'leve'
//! file into a `G_Level` record plus the list of level objects.
//!
//! `G_Level` as the parser fills it (0x288 bytes): name +0 (0x20),
//! identifier +0x20 (0x40), background U_Rect {top, left, bottom, right}
//! +0x60, background image id +0x70, preview image id +0x74, description
//! +0x78 (0x100), copyright +0x178 (0x100), number of objects +0x278,
//! music id +0x27c, media mask id +0x280, briefing id +0x284. A level
//! object (0x16 bytes): unit id +0, layer id +4, x +8, y +0xc, heading
//! +0x10, stationary +0x14, terrain effects +0x15.
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::mem::list;
use super::token::*;

/// `sizeof(G_Level)` as the parser sees it.
pub const LEVEL_SIZE: u32 = 0x288;
/// `sizeof(level object)`.
pub const LEVEL_OBJECT_SIZE: u32 = 0x16;
/// `sPriv_LevelInfoListPtr`: list of {id, 1-based number} (8 bytes each).
pub const LEVEL_LIST: u32 = 0x4e7eaa;
/// `g_demoLevelNames[12][0x40]` (encrypted in the exe).
pub const DEMO_LEVEL_NAMES: u32 = 0x4e7ba9;
/// Whether the module is initialised (`dat_4e7ea9`, byte).
pub const INITIALISED: u32 = 0x4e7ea9;

const NONE: u32 = 0x656e6f6e;
const LEVE: u32 = 0x6576656c;
const UNDE: u32 = 0x65646e75;

/// `U_Assert_DataFailure`/`U_Assert_FileFailure`: the original shows an
/// error and quits.
fn assert_failure(what: &str) -> ! {
    panic!("G_Level: {what}")
}

/// `G_Level_Init` (0x429410): marks the module initialised and builds the
/// level list.
pub fn G_Level_Init(g: &mut G) {
    g.m.w8(INITIALISED, 1);
    G_Level_BuildInfoList(g);
}

/// `G_Level_ShutDownInternal` (0x429a70): frees the level list.
pub fn G_Level_ShutDownInternal(g: &mut G) {
    if g.m.r8(INITIALISED) == 0 {
        return;
    }
    let l = g.m.r32(LEVEL_LIST);
    if l == 0 {
        return;
    }
    loop {
        let obj = list::delete_first(&mut g.m, l);
        if obj == 0 {
            break;
        }
        g.m.free(obj);
    }
    g.m.free(l);
    g.m.w32(LEVEL_LIST, 0);
}

/// `G_Level_BuildInfoList` (0x429540): for each of the twelve names in
/// `g_demoLevelNames`, the first 'leve' (in tag index order) whose
/// identifier matches becomes {id, 1-based number}.
pub fn G_Level_BuildInfoList(g: &mut G) {
    G_Level_ShutDownInternal(g);
    let l = list::new(&mut g.m);
    g.m.w32(LEVEL_LIST, l);
    let ids = g.host.ids_of_tag(LEVE);
    let sp = g.m.sp;
    let temp = g.m.alloca(LEVEL_SIZE);
    for i in 0..12u32 {
        // Copy the 0x40-byte name, terminate it and decrypt it
        // (`U_Utils_String_Encrypt` = nibble swap + xor 0xff).
        let mut name: Vec<u8> = g.m.bytes(DEMO_LEVEL_NAMES + i * 0x40, 0x40).to_vec();
        name[0x3f] = 0;
        let len = name.iter().position(|&b| b == 0).unwrap_or(0x3f);
        let mut name = name[..len].to_vec();
        for b in name.iter_mut() {
            *b = ((*b >> 4) | (*b << 4)) ^ 0xff;
        }
        let mut found = false;
        for &id in &ids {
            sub_429ae0(g, id as i32, temp, 0);
            if g.m.strcmp(temp + 0x20, &name) == 0 {
                found = true;
                let level = g.m.alloc(8);
                list::add(&mut g.m, l, level);
                g.m.clear(level, 4);
                g.m.w32(level, id);
                g.m.wi32(level + 4, i as i32 + 1);
                break;
            }
        }
        if !found {
            g.m.pop_to(sp);
            assert_failure("level list: no level file for a demo level name");
        }
    }
    g.m.pop_to(sp);
}

/// `int __cdecl G_Level_GetNumLevelsInDemo(void)` — 0x429480, 16 bytes, decomp: complete.
pub fn G_Level_GetNumLevelsInDemo(g: &mut G) -> i32 {
    4
}

/// `void __cdecl G_Level_HackTest_Fux0r(void)` — 0x429490, 16 bytes, decomp: complete.
pub fn G_Level_HackTest_Fux0r(g: &mut G) {
    G_Level_ShutDownInternal(g);
}

fn level_list(g: &mut G, line: &str) -> u32 {
    let l = g.m.r32(LEVEL_LIST);
    if l == 0 {
        assert_failure(line);
    }
    l
}

/// `int __cdecl G_Level_GetNumInLevelList(void)` — 0x429790, 64 bytes, decomp: complete.
pub fn G_Level_GetNumInLevelList(g: &mut G) -> i32 {
    let l = level_list(g, "sPriv_LevelInfoListPtr");
    list::num(&g.m, l)
}

/// `int __cdecl G_Level_Get1BasedNumForLevelID(long)` — 0x4297d0, 160 bytes, decomp: complete.
pub fn G_Level_Get1BasedNumForLevelID(g: &mut G, a0: i32) -> i32 {
    let l = level_list(g, "sPriv_LevelInfoListPtr");
    let mut result = 0;
    for level in list::objects(&g.m, l) {
        if g.m.ri32(level) == a0 {
            result = g.m.ri32(level + 4);
            break;
        }
    }
    result
}

/// `long __cdecl G_Level_GetLevelIDFor1BasedLevelListNumber(int)` — 0x429870, 160 bytes, decomp: complete.
pub fn G_Level_GetLevelIDFor1BasedLevelListNumber(g: &mut G, a0: i32) -> i32 {
    let l = level_list(g, "sPriv_LevelInfoListPtr");
    let mut result = NONE as i32;
    for level in list::objects(&g.m, l) {
        if g.m.ri32(level + 4) == a0 {
            result = g.m.ri32(level);
            break;
        }
    }
    result
}

/// `void __cdecl G_Level_LoadByLevelListNumber(int, struct G_Level *, class U_LinkedList *)` — 0x429910, 240 bytes, decomp: complete.
pub fn G_Level_LoadByLevelListNumber(g: &mut G, a0: i32, a1: u32, a2: u32) {
    let l = level_list(g, "sPriv_LevelInfoListPtr");
    let mut id = NONE as i32;
    for level in list::objects(&g.m, l) {
        if g.m.ri32(level + 4) == a0 {
            id = g.m.ri32(level);
            break;
        }
    }
    if id as u32 == NONE {
        assert_failure("tagID not_eq kG_GameObject_ID_None");
    }
    G_Level_LoadByID(g, id, a1, a2);
}

/// `void __cdecl G_Level_LoadByID(long, struct G_Level *, class U_LinkedList *)` — 0x429a00, 48 bytes, decomp: complete.
pub fn G_Level_LoadByID(g: &mut G, a0: i32, a1: u32, a2: u32) {
    sub_429ae0(g, a0, a1, a2);
}

/// `void __cdecl G_Level_DisposeLevelObjects(class U_LinkedList *)` — 0x429a30, 64 bytes, decomp: complete.
pub fn G_Level_DisposeLevelObjects(g: &mut G, a0: u32) {
    if a0 == 0 {
        return;
    }
    loop {
        let obj = list::delete_first(&mut g.m, a0);
        if obj == 0 {
            break;
        }
        g.m.free(obj);
    }
}

/// `void __cdecl sub_429ae0(long id, G_Level *level, U_LinkedList *list)`
/// — 0x429ae0: fetches the 'leve' file (asserting it exists), decrypts it
/// and hands the text to `sub_429b70`.
pub fn sub_429ae0(g: &mut G, id: i32, level: u32, objects: u32) {
    let Some(text) = g.host.read_text(LEVE, id as u32) else {
        assert_failure("U_Pak_GetAndCheckPtrToTagData: level file missing");
    };
    sub_429b70(g, &text, level, objects);
}

/// `U_Token_GetRect` (0x40f2c0) into `out`: the first value is the left
/// edge, then top, right, bottom; the U_Rect is {top, left, bottom,
/// right}. Missing values stay 0.
fn token_get_rect(g: &mut G, text: &[u8], c: &mut i32, key: &str, out: u32) {
    g.m.clear(out, 16);
    let before = *c;
    let mut tmp = [0u8; 0x80];
    get_string(text, c, key, &mut tmp, 0x80);
    if *c == before {
        return;
    }
    let len = tmp.iter().position(|&b| b == 0).unwrap_or(0x7f);
    // strtok(s, ","): runs of commas separate tokens, none are empty.
    let mut value = 0i32;
    let mut vals = [0i32; 4];
    let mut n = 0;
    for tok in tmp[..len].split(|&b| b == b',').filter(|t| !t.is_empty()) {
        if let Some(v) = scan_i(tok) {
            value = v;
        }
        if n < 4 {
            vals[n] = value;
        }
        n += 1;
        if n == 4 {
            break;
        }
    }
    g.m.wi32(out, vals[1]);
    g.m.wi32(out + 4, vals[0]);
    g.m.wi32(out + 8, vals[3]);
    g.m.wi32(out + 12, vals[2]);
}

/// `void sub_429b70(const char *text, G_Level *level, U_LinkedList *list)`
/// — 0x429b70: parses a 'leve' file into the record and, when `objects`
/// is non-null, replaces its contents with the level's objects. Objects
/// whose unit is not in the pak are dropped (and `numObjects` shrinks).
pub fn sub_429b70(g: &mut G, text: &[u8], level: u32, objects: u32) {
    let t = text;
    let mut c = 0i32;
    macro_rules! out {
        ($off:expr, $n:expr) => {
            g.m.bytes_mut(level + $off, $n)
        };
    }
    get_string(t, &mut c, "#name_STR", out!(0, 0x20), 0x20);
    get_string(t, &mut c, "#indentifier_STR", out!(0x20, 0x40), 0x40);
    get_string(t, &mut c, "#description_STR", out!(0x78, 0x100), 0x100);
    get_string(t, &mut c, "#copyright_STR", out!(0x178, 0x100), 0x100);
    token_get_rect(g, t, &mut c, "#background_RECT", level + 0x60);
    get_id(t, &mut c, "#backgroundImage_ID", out!(0x70, 4));
    get_id(t, &mut c, "#previewImage_ID", out!(0x74, 4));
    get_id(t, &mut c, "#music_ID", out!(0x27c, 4));
    get_id(t, &mut c, "#mediaMask_ID", out!(0x280, 4));
    get_id(t, &mut c, "#briefing_ID", out!(0x284, 4));
    get_int(t, &mut c, "#numObjects_INT", out!(0x278, 4));

    if objects == 0 {
        return;
    }
    loop {
        let obj = list::delete_first(&mut g.m, objects);
        if obj == 0 {
            break;
        }
        g.m.free(obj);
    }
    // `unitID` is a stack local the original never presets.
    let mut unit_id = [0u8; 4];
    let mut i = 0;
    while i < g.m.ri32(level + 0x278) {
        get_id(t, &mut c, "#unit_ID", &mut unit_id);
        let id = u32::from_le_bytes(unit_id);
        if g.host.has_tag(UNDE, id) {
            let obj = g.m.alloc(LEVEL_OBJECT_SIZE);
            list::add(&mut g.m, objects, obj);
            g.m.clear(obj, LEVEL_OBJECT_SIZE);
            g.m.w32(obj, id);
            get_id(t, &mut c, "#layer_ID", g.m.bytes_mut(obj + 4, 4));
            get_int(t, &mut c, "#xLoc_INT", g.m.bytes_mut(obj + 8, 4));
            get_int(t, &mut c, "#yLoc_INT", g.m.bytes_mut(obj + 0xc, 4));
            get_int(t, &mut c, "#headingDegrees_INT", g.m.bytes_mut(obj + 0x10, 4));
            get_bool(t, &mut c, "#isStationary_BOOL", g.m.bytes_mut(obj + 0x14, 1));
            get_bool(t, &mut c, "#enableTerrainEffects_BOOL", g.m.bytes_mut(obj + 0x15, 1));
        } else {
            let n = g.m.ri32(level + 0x278).wrapping_sub(1);
            g.m.wi32(level + 0x278, n);
        }
        i += 1;
    }
}
