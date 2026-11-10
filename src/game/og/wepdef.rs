//! `G_WepDef`: a weapon definition as the original holds it in memory, a
//! 0x200-byte block filled by `G_WepDef_SetToDefaults` (0x445580) and the
//! parser at 0x445cf0 (`sub_045cf0`, called from `sub_045bb0`). Offsets
//! were checked against the disassembly. The spawn-set list hangs off
//! +0x1c0 in the original; here it is a vector of 0x32-byte images.

use super::token::*;

pub const DEF_SIZE: usize = 0x200;
pub const SPAWN_SET_SIZE: usize = 0x32;
pub const NONE: u32 = 0x656e6f6e;

/// The six-dword sound-settings default at 0x4f8fe3: id 'none', volumes
/// and priority 100, pitch 1.0..1.0.
const SOUND_DEFAULT: [u8; 24] = [
    0x6e, 0x6f, 0x6e, 0x65, 100, 0, 0, 0, 100, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0x80, 0x3f, 0, 0, 0x80,
    0x3f,
];

#[derive(Clone)]
pub struct WepDef {
    pub img: Box<[u8]>,
    /// Spawn sets (list at +0x1c0), in file order.
    pub spawn_sets: Vec<[u8; SPAWN_SET_SIZE]>,
}

macro_rules! rd {
    ($name:ident, $t:ty, $n:expr) => {
        pub fn $name(&self, off: usize) -> $t {
            <$t>::from_le_bytes(self.img[off..off + $n].try_into().unwrap())
        }
    };
}

impl WepDef {
    rd!(i32, i32, 4);
    rd!(u32, u32, 4);
    rd!(f32, f32, 4);
    rd!(u16, u16, 2);
    pub fn b(&self, off: usize) -> bool {
        self.img[off] != 0
    }
    pub fn id(&self) -> u32 {
        self.u32(4)
    }
    /// NUL-terminated string at `off`.
    pub fn str_at(&self, off: usize) -> &[u8] {
        let s = &self.img[off..];
        &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())]
    }

    /// `G_WepDef_SetToDefaults` (0x445580). The C++ port writes the sound
    /// block at +0x14c; the disassembly writes it at +0x14e.
    pub fn defaults() -> WepDef {
        let mut img = vec![0u8; DEF_SIZE].into_boxed_slice();
        w32(&mut img, 0, 0x499602d2);
        for off in [0x130, 0x166, 0x16e, 0x144, 0x148, 0x17e] {
            w32(&mut img, off, NONE);
        }
        img[0x14e..0x14e + 24].copy_from_slice(&SOUND_DEFAULT);
        for off in [0x1c8, 0x1d8, 0x1e6, 0x1f6] {
            w32(&mut img, off, NONE);
        }
        WepDef { img, spawn_sets: Vec::new() }
    }

    /// The parser at 0x445cf0 on decrypted text. `has_image` answers
    /// `G_Res_CanBeLoaded` for sprite references.
    pub fn parse(id: u32, text: &[u8], has_image: &dyn Fn(u32) -> bool) -> WepDef {
        let mut d = WepDef::defaults();
        d.img[4..8].copy_from_slice(&id.to_le_bytes());
        let t = text;
        let mut c = 0i32;
        let img = &mut d.img;
        macro_rules! at {
            ($o:expr) => {
                &mut img[$o..]
            };
        }
        get_id(t, &mut c, "#type_ID", at!(0x8));
        get_id(t, &mut c, "#default_ID", at!(0xc));
        get_string(t, &mut c, "#name_STR", at!(0x10), 0x20);
        get_string(t, &mut c, "#description1_STR", at!(0x30), 0x80);
        get_string(t, &mut c, "#description2_STR", at!(0xb0), 0x80);
        get_id(t, &mut c, "#scoreBarPreviewFace_ID", at!(0x130));
        check_sprite(img, 0x130, has_image);
        get_int(t, &mut c, "#scoreBarPreviewFrame_INT", at!(0x134));
        get_int(t, &mut c, "#maxAllowed_INT", at!(0x138));
        get_int(t, &mut c, "#minimumLevelAvailable_INT", at!(0x13c));
        get_int(t, &mut c, "#maximumLevelAvailable_INT", at!(0x140));
        if i32::from_le_bytes(img[0x140..0x144].try_into().unwrap()) <= 0 {
            img[0x140..0x144].copy_from_slice(&9999i32.to_le_bytes());
        }
        get_id(t, &mut c, "#player1AppearanceFace_ID", at!(0x144));
        check_sprite(img, 0x144, has_image);
        get_id(t, &mut c, "#player2AppearanceFace_ID", at!(0x148));
        check_sprite(img, 0x148, has_image);
        get_color(t, &mut c, "#playerGlow_COLOR", at!(0x14c));
        get_id(t, &mut c, "#selectionSound_ID", at!(0x14e));
        get_int(t, &mut c, "#selectionSound_MinVolume_INT", at!(0x152));
        get_int(t, &mut c, "#selectionSound_MaxVolume_INT", at!(0x156));
        get_int(t, &mut c, "#selectionSound_Priority_INT", at!(0x15a));
        get_float(t, &mut c, "#selectionSound_MinPitch_FLOAT", at!(0x15e));
        get_float(t, &mut c, "#selectionSound_MaxPitch_FLOAT", at!(0x162));
        get_id(t, &mut c, "#crosshairFace_ID", at!(0x166));
        check_sprite(img, 0x166, has_image);
        get_int(t, &mut c, "#crosshairFrame_INT", at!(0x16a));
        get_id(t, &mut c, "#crosshairLockedFace_ID", at!(0x16e));
        check_sprite(img, 0x16e, has_image);
        get_int(t, &mut c, "#crosshairLockedFrame_INT", at!(0x172));
        get_int(t, &mut c, "#crosshairXOffset_INT", at!(0x176));
        get_int(t, &mut c, "#crosshairYOffset_INT", at!(0x17a));
        get_id(t, &mut c, "#crosshairSpawnOnActivation_ID", at!(0x17e));
        get_int(t, &mut c, "#numAmmoInPack_INT", at!(0x182));
        get_int(t, &mut c, "#ammoWarnAtCount_INT", at!(0x186));
        get_string(t, &mut c, "#ammoWarning_STR", at!(0x18a), 0x20);
        get_bool(t, &mut c, "#autoRepeat_BOOL", at!(0x1aa));
        get_int(t, &mut c, "#delayBetweenLaunches_INT", at!(0x1ac));
        get_int(t, &mut c, "#delayBetweenLoadLaunches_INT", at!(0x1b0));
        get_int(t, &mut c, "#shieldIncrease_INT", at!(0x1b4));
        get_int(t, &mut c, "#livesIncrease_INT", at!(0x1b8));
        get_int(t, &mut c, "#invulnerableForTime_INT", at!(0x1bc));
        // The count is a stack local the original never presets; a missing
        // key reads stack garbage there, taken as 0 here.
        let mut sets = Vec::new();
        let mut num = 0i32;
        get_int_var(t, &mut c, "#spawn_NumUnitsToSpawn_INT", &mut num);
        for _ in 0..num.max(0) {
            sets.push(parse_spawn_set(t, &mut c));
        }
        get_int(t, &mut c, "#powerup_Air_TimeUntilActivation_INT", at!(0x1c4));
        get_id(t, &mut c, "#powerup_Air_ActivationSpawn_ID", at!(0x1c8));
        get_int(t, &mut c, "#powerup_Air_TimeBetweenPowerLevelChanges_INT", at!(0x1cc));
        get_int(t, &mut c, "#powerup_Air_MaxPowerLevel_INT", at!(0x1d0));
        get_int(t, &mut c, "#powerup_Air_OverloadTime_INT", at!(0x1d4));
        get_id(t, &mut c, "#powerup_Air_ReleaseSpawn_ID", at!(0x1d8));
        get_int(t, &mut c, "#powerup_Air_TimeBetweenReleaseSpawns_INT", at!(0x1dc));
        get_bool(t, &mut c, "#powerup_Air_DoReleaseOnMaxPowerLevel_BOOL", at!(0x1e0));
        get_int(t, &mut c, "#powerup_Ground_TimeUntilActivation_INT", at!(0x1e2));
        get_id(t, &mut c, "#powerup_Ground_ActivationSpawn_ID", at!(0x1e6));
        get_int(t, &mut c, "#powerup_Ground_TimeBetweenPowerLevelChanges_INT", at!(0x1ea));
        get_int(t, &mut c, "#powerup_Ground_MaxPowerLevel_INT", at!(0x1ee));
        get_int(t, &mut c, "#powerup_Ground_OverloadTime_INT", at!(0x1f2));
        get_id(t, &mut c, "#powerup_Ground_ReleaseSpawn_ID", at!(0x1f6));
        get_int(t, &mut c, "#powerup_Ground_TimeBetweenReleaseSpawns_INT", at!(0x1fa));
        get_bool(t, &mut c, "#powerup_Ground_DoReleaseOnMaxPowerLevel_BOOL", at!(0x1fe));
        d.spawn_sets = sets;
        d
    }
}

fn w32(img: &mut [u8], off: usize, v: u32) {
    img[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// An unloadable sprite reference becomes 'none'.
fn check_sprite(img: &mut [u8], off: usize, has_image: &dyn Fn(u32) -> bool) {
    let id = u32::from_le_bytes(img[off..off + 4].try_into().unwrap());
    if id != NONE && !has_image(id) {
        img[off..off + 4].copy_from_slice(&NONE.to_le_bytes());
    }
}

/// One spawn set: `sub_446a80` defaults (the 0x32-byte block is not
/// cleared by the allocator; bytes it does not set are 0 here), then the
/// `#spawn_*` tokens and the angle wrap into 0..360.
fn parse_spawn_set(t: &[u8], c: &mut i32) -> [u8; SPAWN_SET_SIZE] {
    let mut sp = [0u8; SPAWN_SET_SIZE];
    sp[0x20..0x24].copy_from_slice(&NONE.to_le_bytes());
    get_string(t, c, "#spawn_Name_STR", &mut sp[0..], 0x20);
    get_id(t, c, "#spawn_Unit_ID", &mut sp[0x20..]);
    get_int(t, c, "#spawn_XLoc_INT", &mut sp[0x24..]);
    get_int(t, c, "#spawn_YLoc_INT", &mut sp[0x28..]);
    get_bool(t, c, "#spawn_SetHeading_BOOL", &mut sp[0x2c..]);
    get_int(t, c, "#spawn_Angle_INT", &mut sp[0x2e..]);
    let mut a = i32::from_le_bytes(sp[0x2e..0x32].try_into().unwrap());
    if a < 0 {
        a += 0x168;
    } else if a > 0x167 {
        a -= 0x168;
    }
    if !(0..=0x167).contains(&a) {
        a = 0;
    }
    sp[0x2e..0x32].copy_from_slice(&a.to_le_bytes());
    sp
}
