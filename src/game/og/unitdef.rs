//! `G_UnitDef`: a unit definition as the original holds it in memory, a
//! 0x79e0-byte block (header, file data at +0x16, twenty 0x5da-byte states
//! at +0x4d8) filled by `G_UnitDef_SetToDefaults` (0x4405e0) and
//! `G_UnitDef_ParseUnitDef` (0x4422a0) / `G_UnitDef_ParseState`
//! (0x4431f0). Ported code reads the same offsets the disassembly reads.
//! Spawn sets hang off each state in a linked list (state +0x5d6); here
//! they are a vector of 0x56-byte images per state.

use super::token::*;

pub const DEF_SIZE: usize = 0x79e0;
pub const STATE_BASE: usize = 0x4d8;
pub const STATE_SIZE: usize = 0x5da;
pub const MAX_STATES: usize = 0x14;
pub const SPAWN_SET_SIZE: usize = 0x56;
pub const NONE: u32 = 0x656e6f6e;
pub const GRND: u32 = 0x646e7267;
pub const AIR: u32 = 0x20726961;

/// The six-dword sound-settings default at 0x4f6093: id 'none', volumes
/// and priority 100, pitch 1.0..1.0.
const SOUND_DEFAULT: [u8; 24] = [
    0x6e, 0x6f, 0x6e, 0x65, 100, 0, 0, 0, 100, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0x80, 0x3f, 0, 0, 0x80,
    0x3f,
];

#[derive(Clone)]
pub struct UnitDef {
    pub img: Box<[u8]>,
    /// Spawn sets per state, in file order.
    pub spawn_sets: Vec<Vec<[u8; SPAWN_SET_SIZE]>>,
}

macro_rules! rd {
    ($name:ident, $t:ty, $n:expr) => {
        pub fn $name(&self, off: usize) -> $t {
            <$t>::from_le_bytes(self.img[off..off + $n].try_into().unwrap())
        }
    };
}

impl UnitDef {
    rd!(i32, i32, 4);
    rd!(u32, u32, 4);
    rd!(f32, f32, 4);
    rd!(u16, u16, 2);
    pub fn u8(&self, off: usize) -> u8 {
        self.img[off]
    }
    pub fn b(&self, off: usize) -> bool {
        self.img[off] != 0
    }
    pub fn id(&self) -> u32 {
        self.u32(4)
    }
    pub fn num_states(&self) -> i32 {
        self.i32(0x12)
    }
    /// Offset of state `i` inside the image.
    pub fn st(i: usize) -> usize {
        STATE_BASE + i * STATE_SIZE
    }
    /// NUL-terminated string at `off`.
    pub fn str_at(&self, off: usize) -> &[u8] {
        let s = &self.img[off..];
        &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())]
    }
    /// `#isGroundBased_BOOL`; the loader turns it into the 'grnd'/'air '
    /// class at +8.
    pub fn is_ground(&self) -> bool {
        self.u32(8) == GRND
    }

    /// `G_UnitDef_SetToDefaults` (0x4405e0).
    pub fn defaults() -> UnitDef {
        let mut img = vec![0u8; DEF_SIZE].into_boxed_slice();
        w32(&mut img, 0, 0x499602d2);
        w32(&mut img, 4, NONE);
        w32(&mut img, 8, AIR);
        w32(&mut img, 0xc, 10000);
        w32(&mut img, 0x12, 1);
        for j in 0..0x14 {
            w32(&mut img, 0x2d0 + j * 4, NONE);
        }
        for off in [0x420, 0x444, 0x45c, 0x4b4] {
            img[off..off + 24].copy_from_slice(&SOUND_DEFAULT);
        }
        for off in [0x474, 0x478, 0x4a2, 0x4a6, 0x4cc, 0x4d0] {
            w32(&mut img, off, NONE);
        }
        w32(&mut img, 0x190, 1);
        w32(&mut img, 0x194, 1);
        w32(&mut img, 0x1a8, 100);
        w32(&mut img, 0x1b0, 100);
        w32(&mut img, 0x1bc, 100);
        for j in 0..MAX_STATES {
            Self::state_defaults(&mut img, j);
            if j == 0 {
                let name = Self::st(0) + 0x496;
                img[name..name + 8].copy_from_slice(b"State 1\0");
            }
        }
        UnitDef { img, spawn_sets: vec![Vec::new(); MAX_STATES] }
    }

    /// `G_UnitDef_SetStateDefaults` (0x440860), including the rule-table
    /// reset `sub_441990` (state +0x24).
    fn state_defaults(img: &mut [u8], i: usize) {
        let s = Self::st(i);
        img[s..s + STATE_SIZE].fill(0);
        img[s..s + 24].copy_from_slice(&SOUND_DEFAULT);
        w32(img, s + 0x20, 1);
        for r in 0..5 {
            let rp = s + 0x24 + r * 0x88;
            w32(img, rp + 4, NONE);
            img[rp + 8] = 0;
            img[rp + 0x48] = 0;
            w32(img, rp + 0x88, 0);
        }
        w32(img, s + 0x24, 0);
        w32(img, s + 0x2d0, NONE);
        img[s + 0x2d4] = 0;
        img[s + 0x2d5] = 0;
        w32(img, s + 0x2e0, NONE);
        w32(img, s + 0x31c, NONE);
        w32(img, s + 0x300, NONE);
        w32(img, s + 0x3b6, 100);
        w32(img, s + 0x3be, 100);
    }

    /// `G_UnitDef_ParseUnitDef` (0x4422a0) on decrypted text, then the
    /// class tag the caller (0x44222b) derives from `isGroundBased`.
    /// `has_image` answers `G_Res_CanBeLoaded` for sprite references.
    pub fn parse(id: u32, text: &[u8], has_image: &dyn Fn(u32) -> bool) -> UnitDef {
        let mut d = UnitDef::defaults();
        d.img[4..8].copy_from_slice(&id.to_le_bytes());
        let t = text;
        let mut c = 0i32;
        const B: usize = 0x16;
        let img = &mut d.img;
        macro_rules! at {
            ($o:expr) => {
                &mut img[B + $o..]
            };
        }
        get_string(t, &mut c, "#name_STR", at!(0x000), 0x40);
        get_string(t, &mut c, "#familyName_STR", at!(0x040), 0x40);
        get_string(t, &mut c, "#description_STR", at!(0x080), 0x80);
        get_int(t, &mut c, "#numInGroupMin_INT", at!(0x17a));
        get_int(t, &mut c, "#numInGroupMax_INT", at!(0x17e));
        get_int(t, &mut c, "#groupDelayMin_INT", at!(0x182));
        get_int(t, &mut c, "#groupDelayMax_INT", at!(0x186));
        get_int(t, &mut c, "#appearsPercent_INT", at!(0x1a6));
        get_bool(t, &mut c, "#deleteExistingEntitiesOfThisTypeOwnedByPlayer_BOOL", at!(0x101));
        get_bool(t, &mut c, "#doNotSpawnIfTypeAlreadyExists_BOOL", at!(0x100));
        get_bool(t, &mut c, "#harmlessToPlayers_BOOL", at!(0x102));
        get_bool(t, &mut c, "#playerProjectile_BOOL", at!(0x103));
        get_bool(t, &mut c, "#canBeHitByPlayerProjectile_BOOL", at!(0x104));
        get_bool(t, &mut c, "#terrainEffect_BOOL", at!(0x11a));
        get_bool(t, &mut c, "#constrainInGameArea_BOOL", at!(0x105));
        get_bool(t, &mut c, "#castsShadows_BOOL", at!(0x106));
        get_bool(t, &mut c, "#adjustShadowLocForScaling_BOOL", at!(0x114));
        get_bool(t, &mut c, "#randomiseInitialLoc_BOOL", at!(0x115));
        get_bool(t, &mut c, "#adjustInitialLocForOwnerScale_BOOL", at!(0x116));
        get_bool(t, &mut c, "#initiallyHuntsClosestPlayer_BOOL", at!(0x107));
        get_bool(t, &mut c, "#hittableWhenInvisible_BOOL", at!(0x109));
        get_bool(t, &mut c, "#initialHeadingSetInEditor_BOOL", at!(0x10c));
        get_bool(t, &mut c, "#fleesNorthOnNoActivePlayers_BOOL", at!(0x10e));
        get_bool(t, &mut c, "#fleesSouthOnNoActivePlayers_BOOL", at!(0x10f));
        get_bool(t, &mut c, "#collidesWithGroundObstacles_BOOL", at!(0x110));
        get_bool(t, &mut c, "#canBeSpawnedOnlyWhenPlayersActive_BOOL", at!(0x112));
        get_bool(t, &mut c, "#usePreviewAppearanceInPlacementEditor_BOOL", at!(0x117));
        get_bool(t, &mut c, "#allowStationaryOptionInPlacementEditor_BOOL", at!(0x118));
        get_bool(t, &mut c, "#includeInAirAccuracyCount_BOOL", at!(0x11b));
        get_bool(t, &mut c, "#includeInGroundAccuracyCount_BOOL", at!(0x11c));
        get_id(t, &mut c, "#editorPreviewSpriteFace_ID", at!(0x2ba));
        check_sprite(img, B + 0x2ba, has_image);
        get_int(t, &mut c, "#editorPreviewSpriteFrame_INT", at!(0x1a2));
        get_id(t, &mut c, "#pickup_Type_ID", at!(0x4b6));
        get_id(t, &mut c, "#pickup_MultiplierSpawn_ID", at!(0x4ba));
        get_int(t, &mut c, "#pickup_Value_INT", at!(0x4be));
        get_id(t, &mut c, "#destructSpawn_ID", at!(0x45e));
        get_id(t, &mut c, "#destructSound_ID", at!(0x49e));
        get_int(t, &mut c, "#destructSound_MinVolume_INT", at!(0x4a2));
        get_int(t, &mut c, "#destructSound_MaxVolume_INT", at!(0x4a6));
        get_int(t, &mut c, "#destructSound_Priority_INT", at!(0x4aa));
        get_float(t, &mut c, "#destructSound_MinPitch_FLOAT", at!(0x4ae));
        get_float(t, &mut c, "#destructSound_MaxPitch_FLOAT", at!(0x4b2));
        get_id(t, &mut c, "#destructParticle_ID", at!(0x462));
        get_color(t, &mut c, "#destructParticleColor_COLOR", at!(0x466));
        get_string(t, &mut c, "#destructNotice_STR", at!(0x468), 0x20);
        get_int(t, &mut c, "#destructNumCoinsToRelease_INT", at!(0x488));
        get_id(t, &mut c, "#destructCoin_ID", at!(0x48c));
        get_id(t, &mut c, "#destructCoinOnGroupKill_ID", at!(0x490));
        get_bool(t, &mut c, "#destructDestroyChildren_BOOL", at!(0x494));
        get_bool(t, &mut c, "#destructDeleteChildren_BOOL", at!(0x495));
        get_bool(t, &mut c, "#destructDrawToTerrain_BOOL", at!(0x496));
        get_bool(t, &mut c, "#destructReleaseRandomBonus_BOOL", at!(0x498));
        get_bool(t, &mut c, "#destructCreateObstacle_BOOL", at!(0x497));
        get_float(t, &mut c, "#shields_MaxAmount_FLOAT", at!(0x42a));
        get_float(t, &mut c, "#shields_LevelIncrement_FLOAT", at!(0x426));
        get_float(t, &mut c, "#shields_BaseAmount_FLOAT", at!(0x422));
        {
            let rf = |img: &[u8], o: usize| f32::from_le_bytes(img[B + o..B + o + 4].try_into().unwrap());
            if rf(img, 0x42a) < rf(img, 0x422) {
                let base: [u8; 4] = img[B + 0x422..B + 0x426].try_into().unwrap();
                img[B + 0x42a..B + 0x42e].copy_from_slice(&base);
            }
        }
        get_int(t, &mut c, "#score_INT", at!(0x49a));
        get_float(t, &mut c, "#damage_FLOAT", at!(0x25a));
        get_int(t, &mut c, "#initialVisibilityPercent_INT", at!(0x19a));
        get_int(t, &mut c, "#initialScalePercent_INT", at!(0x192));
        get_int(t, &mut c, "#initialScalePercentTolerance_INT", at!(0x196));
        get_id(t, &mut c, "#drawLayer_ID", at!(0x2c6));
        get_string(t, &mut c, "#entryNotice_STR", at!(0x30a), 0x100);
        get_int(t, &mut c, "#entryNoticeDelay_INT", at!(0x19e));
        get_bool(t, &mut c, "#displayNoticeOnceOnly_BOOL", at!(0x108));
        get_id(t, &mut c, "#entryNoticeSound_ID", at!(0x40a));
        get_int(t, &mut c, "#entryNoticeSound_MinVolume_INT", at!(0x40e));
        get_int(t, &mut c, "#entryNoticeSound_MaxVolume_INT", at!(0x412));
        get_int(t, &mut c, "#entryNoticeSound_Priority_INT", at!(0x416));
        get_float(t, &mut c, "#entryNoticeSound_MinPitch_FLOAT", at!(0x41a));
        get_float(t, &mut c, "#entryNoticeSound_MaxPitch_FLOAT", at!(0x41e));
        get_id(t, &mut c, "#hitParticles_ID", at!(0x2be));
        get_bool(t, &mut c, "#hitParticleDoCircularBurst_BOOL", at!(0x119));
        get_color(t, &mut c, "#hitParticlesColor_COLOR", at!(0x166));
        get_id(t, &mut c, "#shieldSound_ID", at!(0x42e));
        get_int(t, &mut c, "#shieldSound_MinVolume_INT", at!(0x432));
        get_int(t, &mut c, "#shieldSound_MaxVolume_INT", at!(0x436));
        get_int(t, &mut c, "#shieldSound_Priority_INT", at!(0x43a));
        get_float(t, &mut c, "#shieldSound_MinPitch_FLOAT", at!(0x43e));
        get_float(t, &mut c, "#shieldSound_MaxPitch_FLOAT", at!(0x442));
        get_id(t, &mut c, "#unshieldedSound_ID", at!(0x446));
        get_int(t, &mut c, "#unshieldedSound_MinVolume_INT", at!(0x44a));
        get_int(t, &mut c, "#unshieldedSound_MaxVolume_INT", at!(0x44e));
        get_int(t, &mut c, "#unshieldedSound_Priority_INT", at!(0x452));
        get_float(t, &mut c, "#unshieldedSound_MinPitch_FLOAT", at!(0x456));
        get_float(t, &mut c, "#unshieldedSound_MaxPitch_FLOAT", at!(0x45a));
        get_id(t, &mut c, "#deletionSpawn_ID", at!(0x2c2));
        get_float(t, &mut c, "#xOffsetMin_FLOAT", at!(0x242));
        get_float(t, &mut c, "#xOffsetMax_FLOAT", at!(0x246));
        get_float(t, &mut c, "#yOffsetMin_FLOAT", at!(0x24a));
        get_float(t, &mut c, "#yOffsetMax_FLOAT", at!(0x24e));
        get_int(t, &mut c, "#initialHeading_INT", at!(0x18a));
        get_int(t, &mut c, "#initialHeadingTolerance_INT", at!(0x18e));
        get_bool(t, &mut c, "#useOwnerHeading_BOOL", at!(0x111));
        get_float(t, &mut c, "#initialSpeedMin_FLOAT", at!(0x252));
        get_float(t, &mut c, "#initialSpeedMax_FLOAT", at!(0x256));
        get_bool(t, &mut c, "#doBurst_BOOL", at!(0x10a));
        get_bool(t, &mut c, "#doImplode_BOOL", at!(0x10b));
        get_bool(t, &mut c, "#isGroundBased_BOOL", at!(0x10d));
        get_bool(t, &mut c, "#doDeathSpawnOnAnyMedia_BOOL", at!(0x113));
        get_id(t, &mut c, "#mediaImpactSize_ID", at!(0x2ca));
        let mut num_states = 0i32;
        get_int_var(t, &mut c, "#numStates_INT", &mut num_states);
        img[0x12..0x16].copy_from_slice(&num_states.to_le_bytes());
        for i in 0..num_states.clamp(0, MAX_STATES as i32) as usize {
            d.parse_state(t, i, &mut c, has_image);
        }
        let class = if d.img[0x123] != 0 { GRND } else { AIR };
        d.img[8..12].copy_from_slice(&class.to_le_bytes());
        d
    }

    /// `G_UnitDef_ParseState` (0x4431f0).
    fn parse_state(&mut self, t: &[u8], i: usize, c: &mut i32, has_image: &dyn Fn(u32) -> bool) {
        let s = Self::st(i);
        let img = &mut self.img;
        macro_rules! at {
            ($o:expr) => {
                &mut img[s + $o..]
            };
        }
        macro_rules! owner_flag {
            ($o:expr) => {
                if img[s + $o] != 0 {
                    img[0x11] = 1;
                }
            };
        }
        let ri = |img: &[u8], o: usize| i32::from_le_bytes(img[s + o..s + o + 4].try_into().unwrap());
        get_string(t, c, "#stateName_STR", at!(0x496), 0x40);
        get_float(t, c, "#stateOnRange_FLOAT", at!(0x446));
        get_string(t, c, "#stateOnRangeChangeTo_STR", at!(0x556), 0x40);
        get_string(t, c, "#stateOnHitChangeTo_STR", at!(0x596), 0x40);
        get_int(t, c, "#stateOnHitChangeStateDelay_INT", at!(0x3b2));
        get_int(t, c, "#stateOnTimerMin_INT", at!(0x3a6));
        get_int(t, c, "#stateOnTimerMax_INT", at!(0x3aa));
        if ri(img, 0x3aa) < ri(img, 0x3a6) {
            let v = ri(img, 0x3a6);
            img[s + 0x3aa..s + 0x3ae].copy_from_slice(&v.to_le_bytes());
        }
        get_string(t, c, "#stateOnTimerChangeTo_STR", at!(0x4d6), 0x40);
        if ri(img, 0x3aa) > 0 && img[s + 0x4d6] == 0 {
            img[s + 0x3a6..s + 0x3ae].fill(0);
        }
        get_int(t, c, "#stateOnCounter_INT", at!(0x3ae));
        get_string(t, c, "#stateOnCounterChangeTo_STR", at!(0x516), 0x40);
        get_id(t, c, "#stateSpriteFace_ID", at!(0x300));
        check_sprite(img, s + 0x300, has_image);
        get_int(t, c, "#stateSpriteFrameMin_INT", at!(0x308));
        get_int(t, c, "#stateSpriteFrameMax_INT", at!(0x30c));
        get_bool(t, c, "#stateUseParentDirection_BOOL", at!(0x320));
        owner_flag!(0x320);
        get_int(t, c, "#stateRequiredVisibilityPercent_INT", at!(0x3be));
        get_int(t, c, "#stateVisibilityDeltaPercent_INT", at!(0x3c2));
        get_int(t, c, "#stateRequiredScalePercent_INT", at!(0x3b6));
        get_int(t, c, "#stateScaleDeltaPercent_INT", at!(0x3ba));
        get_int(t, c, "#stateTintPercent_INT", at!(0x3c6));
        get_int(t, c, "#stateTintDeltaPercent_INT", at!(0x3ca));
        get_color(t, c, "#stateTintColor_COLOR", at!(0x32e));
        get_bool(t, c, "#stateDoColorise_BOOL", at!(0x349));
        get_int(t, c, "#stateNumDirections_INT", at!(0x304));
        get_int(t, c, "#stateFramesPerDirection_INT", at!(0x310));
        get_int(t, c, "#stateFrameDelay_INT", at!(0x314));
        get_int(t, c, "#stateFrameDelta_INT", at!(0x318));
        get_bool(t, c, "#stateDoAnimateBackwards_BOOL", at!(0x2fc));
        get_bool(t, c, "#stateDoLoopAnimation_BOOL", at!(0x2fd));
        get_bool(t, c, "#stateContinuousFrameRandomisation_BOOL", at!(0x2fe));
        get_bool(t, c, "#stateDoRotateToTarget_BOOL", at!(0x2ff));
        get_float(t, c, "#stateMaxSpeed_FLOAT", at!(0x452));
        get_float(t, c, "#stateDelta_FLOAT", at!(0x456));
        get_float(t, c, "#stateHoldMaxSpeed_FLOAT", at!(0x44a));
        get_float(t, c, "#stateHoldDelta_FLOAT", at!(0x44e));
        get_id(t, c, "#stateFlee_ID", at!(0x31c));
        get_float(t, c, "#stateFleeSpeed_FLOAT", at!(0x45a));
        get_float(t, c, "#stateFleeDelta_FLOAT", at!(0x45e));
        get_id(t, c, "#stateEntrySound_ID", at!(0x0));
        get_int(t, c, "#stateEntrySound_MinVolume_INT", at!(0x4));
        get_int(t, c, "#stateEntrySound_MaxVolume_INT", at!(0x8));
        get_int(t, c, "#stateEntrySound_Priority_INT", at!(0xc));
        get_float(t, c, "#stateEntrySound_MinPitch_FLOAT", at!(0x10));
        get_float(t, c, "#stateEntrySound_MaxPitch_FLOAT", at!(0x14));
        get_int(t, c, "#stateSoundLoopDelay_INT", at!(0x1c));
        get_int(t, c, "#stateSoundMaxNumToPlay_INT", at!(0x20));
        get_bool(t, c, "#stateSoundLoop_BOOL", at!(0x18));
        get_bool(t, c, "#stateSoundAllowOnlyOneInstance_BOOL", at!(0x19));
        get_bool(t, c, "#stateSoundRepeatOnStateChange_BOOL", at!(0x1a));
        get_bool(t, c, "#invulnerableUntilAllChildrenDestroyed_BOOL", at!(0x321));
        owner_flag!(0x321);
        get_bool(t, c, "#invulnerableUntilOwnerDestroyed_BOOL", at!(0x322));
        owner_flag!(0x322);
        get_bool(t, c, "#useOwnersVisibility_BOOL", at!(0x323));
        owner_flag!(0x323);
        get_bool(t, c, "#useOwnersScale_BOOL", at!(0x324));
        owner_flag!(0x324);
        get_bool(t, c, "#canBeDestroyedOnOwnerDestruction_BOOL", at!(0x325));
        owner_flag!(0x325);
        get_bool(t, c, "#canBeDeletedOnOwnerDeletion_BOOL", at!(0x326));
        owner_flag!(0x326);
        get_bool(t, c, "#passHitsToOwner_BOOL", at!(0x327));
        owner_flag!(0x327);
        get_bool(t, c, "#visuallyReflectOwnerHits_BOOL", at!(0x328));
        owner_flag!(0x328);
        get_bool(t, c, "#destroyOwnerOnDestruction_BOOL", at!(0x329));
        owner_flag!(0x329);
        get_id(t, c, "#collision_Spawn_ID", at!(0x2e0));
        get_bool(t, c, "#collision_RepeatSpawns_BOOL", at!(0x2e4));
        get_int(t, c, "#collision_SpawnDelay_INT", at!(0x2e6));
        let mut sets = Vec::new();
        let mut n_sets = 0i32;
        get_int_var(t, c, "stateNumSpawnSets_INT", &mut n_sets);
        for _ in 0..n_sets.max(0) {
            sets.push(parse_spawn_set(t, c));
        }
        get_bool(t, c, "#state_MotionBlur_Required_BOOL", at!(0x2ea));
        get_bool(t, c, "#state_MotionBlur_AllowGlowDrawing_BOOL", at!(0x2eb));
        get_int(t, c, "#state_MotionBlur_MinTimeBetweenBlurs_INT", at!(0x2ec));
        get_int(t, c, "#state_MotionBlur_MaxTimeBetweenBlurs_INT", at!(0x2f0));
        get_float(t, c, "#state_MotionBlur_InitialVisibilityPercent_FLOAT", at!(0x2f4));
        get_float(t, c, "#state_MotionBlur_VisibilityDeltaPercent_FLOAT", at!(0x2f8));
        get_id(t, c, "#stateParticles_ID", at!(0x2d0));
        get_color(t, c, "#stateParticlesColor_COLOR", at!(0x2d4));
        get_bool(t, c, "#stateParticlesRepeat_BOOL", at!(0x2d6));
        get_int(t, c, "#stateParticles_RepeatDelay_INT", at!(0x2d8));
        get_int(t, c, "#stateParticles_MaxNumBursts_INT", at!(0x2dc));
        get_bool(t, c, "#statePauseVerticalScrolling_BOOL", at!(0x342));
        get_bool(t, c, "#stateCollides_BOOL", at!(0x343));
        get_bool(t, c, "#stateInvulnerable_ShieldsDoNotDepleteOnCollision_BOOL", at!(0x344));
        get_bool(t, c, "#statePickup_DoNotChangeAppearanceOnStateChange_BOOL", at!(0x353));
        get_bool(t, c, "#stateHunts_BOOL", at!(0x345));
        get_bool(t, c, "#stateCyclicMotion_BOOL", at!(0x346));
        get_bool(t, c, "#stateHoldPositionToTarget_BOOL", at!(0x348));
        get_bool(t, c, "#stateReverseDirectionOnReaction_BOOL", at!(0x347));
        get_bool(t, c, "#stateIsTargetable_BOOL", at!(0x34a));
        get_bool(t, c, "#stateCollidesWithPlayers_BOOL", at!(0x34b));
        get_bool(t, c, "#stateLockToOwnerLoc_BOOL", at!(0x32a));
        owner_flag!(0x32a);
        get_bool(t, c, "#stateLinkToOwnerLoc_BOOL", at!(0x32b));
        owner_flag!(0x32b);
        get_bool(t, c, "#stateOrbitOwner_BOOL", at!(0x32c));
        owner_flag!(0x32c);
        get_bool(t, c, "#stateDeleteOnNoActivePlayers_BOOL", at!(0x34c));
        get_bool(t, c, "#stateDestructOnNoActivePlayers_BOOL", at!(0x34d));
        get_bool(t, c, "#stateDestructIfVerticalScrollingNotPaused_BOOL", at!(0x34e));
        get_bool(t, c, "#stateDrawToTerrain_BOOL", at!(0x34f));
        get_bool(t, c, "#stateDoNotGlowOnCollision_BOOL", at!(0x350));
        get_bool(t, c, "#stateUseThisStateOnShieldDepletion_BOOL", at!(0x352));
        get_bool(t, c, "#stateUseThisStateOnWeaponPowerupRelease_BOOL", at!(0x351));
        img[s + 0x24..s + 0x28].fill(0);
        let mut n_rules = 0i32;
        get_int_var(t, c, "#stateNumRules_INT", &mut n_rules);
        let mut scratch = [0u8; 0x40];
        for r in 0..n_rules.max(0) as usize {
            let rp = s + r * 0x88;
            get_string(t, c, "#stateRuleName_STR", &mut scratch, 0x40);
            get_id(t, c, "#stateRuleUnit_ID", &mut img[rp + 0x28..]);
            get_int(t, c, "#stateRuleRange_INT", &mut img[rp + 0xac..]);
            get_string(t, c, "#stateRuleCondition_STR", &mut img[rp + 0x2c..], 0x40);
            get_string(t, c, "stateRuleAction_STR", &mut img[rp + 0x6c..], 0x40);
            if u32::from_le_bytes(img[rp + 0x28..rp + 0x2c].try_into().unwrap()) != NONE {
                let n = ri(img, 0x24) + 1;
                img[s + 0x24..s + 0x28].copy_from_slice(&n.to_le_bytes());
            }
        }
        self.spawn_sets[i] = sets;
    }

    /// Rule `r` of state `i`: (unit id, condition, action, range).
    pub fn rule(&self, i: usize, r: usize) -> (u32, &[u8], &[u8], i32) {
        let rp = Self::st(i) + r * 0x88;
        (self.u32(rp + 0x28), self.str_at(rp + 0x2c), self.str_at(rp + 0x6c), self.i32(rp + 0xac))
    }
}

fn w32(img: &mut [u8], off: usize, v: u32) {
    img[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// `sub_444660`: an unloadable sprite reference becomes 'none'.
fn check_sprite(img: &mut [u8], off: usize, has_image: &dyn Fn(u32) -> bool) {
    let id = u32::from_le_bytes(img[off..off + 4].try_into().unwrap());
    if id != NONE && !has_image(id) {
        img[off..off + 4].copy_from_slice(&NONE.to_le_bytes());
    }
}

/// One `G_UnitDef_SpawnSet`: `SpawnSet_SetToDefaults` (0x440940) then the
/// spawn-set tokens of `ParseState`. The id at +0x20 defaults to the
/// value at 0x4f5ffb ('none').
fn parse_spawn_set(t: &[u8], c: &mut i32) -> [u8; SPAWN_SET_SIZE] {
    let mut sp = [0u8; SPAWN_SET_SIZE];
    sp[0x20..0x24].copy_from_slice(&NONE.to_le_bytes());
    sp[0x34..0x38].copy_from_slice(&1i32.to_le_bytes());
    sp[0x38..0x3c].copy_from_slice(&1i32.to_le_bytes());
    get_string(t, c, "#stateSpawnSetName_STR", &mut sp[0..], 0x20);
    get_id(t, c, "#stateSpawnSetSpawn_ID", &mut sp[0x20..]);
    get_int(t, c, "#stateSpawnSetXOffset_INT", &mut sp[0x24..]);
    get_int(t, c, "#stateSpawnSetYOffset_INT", &mut sp[0x28..]);
    get_bool(t, c, "#stateSpawnSetAdjustOffsetForUnitRotation_BOOL", &mut sp[0x44..]);
    get_bool(t, c, "#stateSpawnSet_AbsoluteCoordinates_BOOL", &mut sp[0x45..]);
    get_int(t, c, "#stateSpawnSetRateMin_INT", &mut sp[0x2c..]);
    get_int(t, c, "#stateSpawnSetRateMax_INT", &mut sp[0x30..]);
    get_int(t, c, "#stateSpawnSetNumInVolleyMin_INT", &mut sp[0x34..]);
    get_int(t, c, "#stateSpawnSetNumInVolleyMax_INT", &mut sp[0x38..]);
    get_int(t, c, "#stateSpawnSetDelayBetweenEntitiesMin_INT", &mut sp[0x3c..]);
    get_int(t, c, "#stateSpawnSetDelayBetweenEntitiesMax_INT", &mut sp[0x40..]);
    get_bool(t, c, "#stateSpawnSetRepeatSpawns_BOOL", &mut sp[0x46..]);
    get_bool(t, c, "#stateSpawnSetDon'tSpawnOffscreen_BOOL", &mut sp[0x47..]);
    get_bool(t, c, "#stateSpawnSetPauseAnyRotationWhileSpawning_BOOL", &mut sp[0x48..]);
    get_int(t, c, "#stateSpawnSetTimeToPauseRotationAfterSpawning_INT", &mut sp[0x4a..]);
    get_bool(t, c, "#stateSpawnSetSpawnIfFleeing_BOOL", &mut sp[0x4e..]);
    get_bool(t, c, "#stateSpawnSet_StationaryOption_BOOL", &mut sp[0x54..]);
    get_bool(t, c, "#stateSpawnSet_TerrainEffectsOption_BOOL", &mut sp[0x55..]);
    get_bool(t, c, "#stateSpawnSetSetHeading_BOOL", &mut sp[0x4f..]);
    get_int(t, c, "#stateSpawnSetHeadingDegrees_INT", &mut sp[0x50..]);
    sp
}
