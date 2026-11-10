//! `G_PlayerDef`: a player definition as the original holds it in memory,
//! a 0x104-byte block seeded by `sub_4366d0` (0x4366d0) and filled by the
//! parser `sub_4368a0` (0x4368a0). Offsets were checked against the
//! disassembly (the packed struct in the C++ port has the same layout
//! except that `entry_InvulnerabilityTime` sits at +0x8a).

use super::token::*;

pub const DEF_SIZE: usize = 0x104;
pub const NONE: u32 = 0x656e6f6e;

/// The six-dword sound-settings default at 0x4ed19b: id 'none', volumes
/// and priority 100, pitch 1.0..1.0.
const SOUND_DEFAULT: [u8; 24] = [
    0x6e, 0x6f, 0x6e, 0x65, 100, 0, 0, 0, 100, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0x80, 0x3f, 0, 0, 0x80,
    0x3f,
];

#[derive(Clone)]
pub struct PlayerDef {
    pub img: Box<[u8]>,
}

macro_rules! rd {
    ($name:ident, $t:ty, $n:expr) => {
        pub fn $name(&self, off: usize) -> $t {
            <$t>::from_le_bytes(self.img[off..off + $n].try_into().unwrap())
        }
    };
}

impl PlayerDef {
    rd!(i32, i32, 4);
    rd!(u32, u32, 4);
    rd!(f32, f32, 4);
    rd!(u16, u16, 2);
    pub fn id(&self) -> u32 {
        self.u32(4)
    }
    /// NUL-terminated string at `off`.
    pub fn str_at(&self, off: usize) -> &[u8] {
        let s = &self.img[off..];
        &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())]
    }

    /// The defaults `sub_4366d0` writes into the zeroed block.
    pub fn defaults() -> PlayerDef {
        let mut img = vec![0u8; DEF_SIZE].into_boxed_slice();
        img[0..4].copy_from_slice(&0x499602d2u32.to_le_bytes());
        for off in [0x28, 0x30, 0x38, 0x40, 0x9e, 0x6e, 0xba, 0xc2, 0xc6, 0xca, 0xce] {
            img[off..off + 4].copy_from_slice(&NONE.to_le_bytes());
        }
        img[0xec..0xec + 24].copy_from_slice(&SOUND_DEFAULT);
        PlayerDef { img }
    }

    /// `sub_4368a0` on decrypted text. `has_image` answers
    /// `G_Res_CanBeLoaded` for sprite references.
    pub fn parse(id: u32, text: &[u8], has_image: &dyn Fn(u32) -> bool) -> PlayerDef {
        let mut d = PlayerDef::defaults();
        d.img[4..8].copy_from_slice(&id.to_le_bytes());
        let t = text;
        let mut c = 0i32;
        let img = &mut d.img;
        macro_rules! at {
            ($o:expr) => {
                &mut img[$o..]
            };
        }
        get_string(t, &mut c, "#name_STR", at!(0x8), 0x20);
        get_id(t, &mut c, "#spriteHighScore_ID", at!(0x28));
        check_sprite(img, 0x28, has_image);
        get_int(t, &mut c, "#spriteHighScoreFrame_INT", at!(0x2c));
        get_id(t, &mut c, "#spriteScoreBar_ID", at!(0x30));
        check_sprite(img, 0x30, has_image);
        get_int(t, &mut c, "#spriteScoreBarFrame_INT", at!(0x34));
        get_id(t, &mut c, "#spriteScoreBarPower_ID", at!(0x40));
        check_sprite(img, 0x40, has_image);
        get_int(t, &mut c, "#spriteScoreBarPowerFrame_INT", at!(0x44));
        get_id(t, &mut c, "#spriteScoreBarShield_ID", at!(0x38));
        check_sprite(img, 0x38, has_image);
        get_int(t, &mut c, "#spriteScoreBarShieldFrame_INT", at!(0x3c));
        get_int(t, &mut c, "#defaultShieldPercentage_INT", at!(0x48));
        get_int(t, &mut c, "#shieldWarningPercentage_INT", at!(0x4c));
        get_int(t, &mut c, "#shieldBaseHitPercentage_INT", at!(0x50));
        get_int(t, &mut c, "#shieldHitDelay_INT", at!(0x54));
        get_color(t, &mut c, "#hitGlowColor_COLOR", at!(0x58));
        get_int(t, &mut c, "#hitGlowSpeed_INT", at!(0x5a));
        get_int(t, &mut c, "#life_MaxNum_INT", at!(0x5e));
        get_int(t, &mut c, "#life_NumInitial_INT", at!(0x62));
        get_int(t, &mut c, "#life_InitialRequiredScore_INT", at!(0x66));
        get_int(t, &mut c, "#life_AdditionalRequiredScore_INT", at!(0x6a));
        get_id(t, &mut c, "#life_Spawn_ID", at!(0x6e));
        get_int(t, &mut c, "#waitingTime_INT", at!(0x72));
        get_int(t, &mut c, "#filmIntroTime_INT", at!(0x76));
        get_int(t, &mut c, "#introTime_INT", at!(0x7a));
        get_int(t, &mut c, "#gameOverTime_INT", at!(0x7e));
        get_int(t, &mut c, "#dyingTime_INT", at!(0x82));
        get_int(t, &mut c, "#finalDyingTime_INT", at!(0x86));
        get_int(t, &mut c, "#entry_soloStartX_INT", at!(0x8e));
        get_int(t, &mut c, "#entry_soloStartY_INT", at!(0x92));
        get_int(t, &mut c, "#entry_multiStartX_INT", at!(0x96));
        get_int(t, &mut c, "#entry_multiStartY_INT", at!(0x9a));
        get_id(t, &mut c, "#entry_Spawn_ID", at!(0x9e));
        get_float(t, &mut c, "#entry_StartVelocityX_FLOAT", at!(0xa2));
        get_float(t, &mut c, "#entry_StartVelocityY_FLOAT", at!(0xa6));
        get_float(t, &mut c, "#entry_TargetVelocityX_FLOAT", at!(0xaa));
        get_float(t, &mut c, "#entry_TargetVelocityY_FLOAT", at!(0xae));
        get_float(t, &mut c, "#entry_VelocityDelta_FLOAT", at!(0xb2));
        get_int(t, &mut c, "#entry_InitialDelay_INT", at!(0xb6));
        get_int(t, &mut c, "#entry_InvulnerabilityTime_INT", at!(0x8a));
        get_id(t, &mut c, "#death_Spawn_ID", at!(0xba));
        get_int(t, &mut c, "#death_Duration_INT", at!(0xbe));
        get_id(t, &mut c, "#active_MoneyCounterSpawn_ID", at!(0xc2));
        get_id(t, &mut c, "#active_SpawnOnHit_ID", at!(0xc6));
        get_id(t, &mut c, "#active_ShieldWarningObject_ID", at!(0xca));
        get_id(t, &mut c, "#active_DefenceBonusObject_ID", at!(0xce));
        get_float(t, &mut c, "#active_DefaultMaxSpeed_FLOAT", at!(0xd2));
        get_float(t, &mut c, "#active_VelocityDelta_FLOAT", at!(0xd6));
        get_int(t, &mut c, "#powerupOverload_NumWarnings_INT", at!(0xda));
        get_int(t, &mut c, "#powerupOverload_InitialTimeBetweenWarnings_INT", at!(0xde));
        get_int(t, &mut c, "#powerupOverload_MinimumTimeBetweenWarnings_INT", at!(0xe2));
        get_float(t, &mut c, "#powerupOverload_WarningFadePercent_FLOAT", at!(0xe6));
        get_color(t, &mut c, "#powerupOverload_Hilite_COLOR", at!(0xea));
        get_id(t, &mut c, "#powerupOverloadSound_ID", at!(0xec));
        get_int(t, &mut c, "#powerupOverloadSound_MinVolume_INT", at!(0xf0));
        get_int(t, &mut c, "#powerupOverloadSound_MaxVolume_INT", at!(0xf4));
        get_int(t, &mut c, "#powerupOverloadSound_Priority_INT", at!(0xf8));
        get_float(t, &mut c, "#powerupOverloadSound_MinPitch_FLOAT", at!(0xfc));
        get_float(t, &mut c, "#powerupOverloadSound_MaxPitch_FLOAT", at!(0x100));
        d
    }
}

/// An unloadable sprite reference becomes 'none'.
fn check_sprite(img: &mut [u8], off: usize, has_image: &dyn Fn(u32) -> bool) {
    let id = u32::from_le_bytes(img[off..off + 4].try_into().unwrap());
    if id != NONE && !has_image(id) {
        img[off..off + 4].copy_from_slice(&NONE.to_le_bytes());
    }
}
