//! Parsers for the game's text data formats (`unde`, `leve`, `plde`, `wede`,
//! `flli`, `reli`, `coli`, `edli`, `tefo`, `stli`, `pref`).
//!
//! All of these are `#key_TYPE <value>` token streams scanned by a forward
//! cursor, mirroring `U_Token.cpp` in the original: `strstr` for the key,
//! then `<` … `>` delimiters, with the cursor left on the closing `>` so
//! repeated keys (per-state, per-rule, per-spawn-set) resolve in file order.

use crate::pak::{id, ResId, NONE_ID};

/// Cursor over a decrypted data file, reproducing `U_Token_Get*` semantics.
pub struct Cursor<'a> {
    text: &'a str,
    pub offset: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(text: &'a str) -> Self {
        Cursor { text, offset: 0 }
    }

    /// `U_Token_GetPtrAndLength`: find `key` at/after the cursor, then the
    /// `<` … `>` (or custom) delimited value after it. Leaves the cursor on
    /// the closing delimiter, exactly like the original.
    fn ptr_and_len(&mut self, key: &str) -> Option<(usize, usize)> {
        let hay = self.text.get(self.offset..)?;
        let found = hay.find(key)? + self.offset;
        let start = self.text[found..].find('<')? + found;
        let end = self.text[start..].find('>')? + start;
        self.offset = end;
        Some((start + 1, end - start - 1))
    }

    /// Raw value string for `key` (between `<` and `>`).
    pub fn get_raw(&mut self, key: &str) -> Option<String> {
        let (start, len) = self.ptr_and_len(key)?;
        Some(self.text[start..start + len].to_string())
    }

    pub fn get_string(&mut self, key: &str) -> String {
        self.get_raw(key).unwrap_or_default()
    }

    pub fn get_int(&mut self, key: &str) -> i64 {
        self.get_raw(key)
            .and_then(|v| {
                let t = v.trim();
                t.parse::<i64>()
                    .ok()
                    .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
            })
            .unwrap_or(0)
    }

    pub fn get_float(&mut self, key: &str) -> f64 {
        self.get_raw(key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    pub fn get_bool(&mut self, key: &str) -> bool {
        self.get_raw(key)
            .map(|v| v.trim().eq_ignore_ascii_case("TRUE"))
            .unwrap_or(false)
    }

    /// 4-character ID inside `<...>`. IDs are exactly 4 bytes and may
    /// legitimately contain spaces (e.g. `icb `), so don't trim first.
    pub fn get_id(&mut self, key: &str) -> ResId {
        match self.get_raw(key) {
            Some(v) if v.len() == 4 => id(&v),
            Some(v) => {
                let t = v.trim();
                if t.len() == 4 { id(t) } else { NONE_ID }
            }
            None => NONE_ID,
        }
    }

    /// `#rrggbb` / `#rgb` colour → 0xRRGGBB.
    pub fn get_color(&mut self, key: &str) -> u32 {
        let v = self.get_raw(key).unwrap_or_default();
        let h = v.trim().trim_start_matches('#');
        if h.len() == 6 {
            u32::from_str_radix(h, 16).unwrap_or(0)
        } else if h.len() == 3 {
            let r = u32::from_str_radix(&h[0..1], 16).unwrap_or(0);
            let g = u32::from_str_radix(&h[1..2], 16).unwrap_or(0);
            let b = u32::from_str_radix(&h[2..3], 16).unwrap_or(0);
            (r << 20) | (r << 16) | (g << 12) | (g << 8) | (b << 4) | b
        } else {
            0
        }
    }

    /// `<x1, y1, x2, y2>` rect.
    pub fn get_rect(&mut self, key: &str) -> [i64; 4] {
        let mut out = [0i64; 4];
        if let Some(v) = self.get_raw(key) {
            let mut it = v.split(',').map(|s| s.trim().parse::<i64>().unwrap_or(0));
            for slot in out.iter_mut() {
                *slot = it.next().unwrap_or(0);
            }
        }
        out
    }

    /// Iterate every `<...>` value following each occurrence of `key`
    /// (used by list formats like `reli`/`coli`/`edli` where `#` is the key).
    pub fn values_for_key(&mut self, key: &str) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(v) = self.get_raw(key) {
            out.push(v);
        }
        out
    }
}

/// Scan every `#name<value>` pair in `text`. The list formats (`flli`,
/// `reli`, `coli`, `edli`) pack all entries onto a single `\r`-separated
/// line with no separator between `>` and the next `#`, so line-based
/// parsing only ever sees the first entry.
fn named_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    let b = text.as_bytes();
    while i < b.len() {
        if b[i] == b'#' {
            if let Some(lt_rel) = text[i..].find('<') {
                let lt = i + lt_rel;
                if let Some(gt_rel) = text[lt..].find('>') {
                    let gt = lt + gt_rel;
                    out.push((
                        text[i + 1..lt].trim().to_string(),
                        text[lt + 1..gt].trim().to_string(),
                    ));
                    i = gt + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// .flli — positional float table (220 entries). Names are documentation only;
// the original indexes the table by position, but we keep names for clarity.
// ---------------------------------------------------------------------------

pub struct FillList {
    /// (name, value) in file order.
    pub entries: Vec<(String, f64)>,
}

impl FillList {
    pub fn parse(text: &str) -> FillList {
        FillList {
            entries: named_values(text)
                .into_iter()
                .map(|(n, v)| (n, v.parse().unwrap_or(0.0)))
                .collect(),
        }
    }

    /// Positional access, matching `G_Res_GetPermFloat(index)`.
    pub fn get(&self, index: usize) -> f64 {
        self.entries.get(index).map(|e| e.1).unwrap_or(0.0)
    }

    /// Named access for readability in ported code.
    pub fn named(&self, name: &str) -> Option<f64> {
        self.entries.iter().find(|e| e.0 == name).map(|e| e.1)
    }
}

// ---------------------------------------------------------------------------
// .reli / .coli / .edli — `#Name<tabs><value>` lists
// ---------------------------------------------------------------------------

/// `.reli` file: named rect list. The original stores `U_Rect{top,left,
/// bottom,right}` but the file order is `<left, top, bottom, right>`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

/// `.reli` file: named rect list.
pub struct RectList {
    /// (name, rect) in file order.
    pub entries: Vec<(String, Rect)>,
}

impl RectList {
    pub fn parse(text: &str) -> RectList {
        let entries = named_values(text)
            .into_iter()
            .map(|(name, v)| {
                let mut it = v.split(',').map(|s| s.trim().parse::<i64>().unwrap_or(0));
                let rect = Rect {
                    left: it.next().unwrap_or(0),
                    top: it.next().unwrap_or(0),
                    bottom: it.next().unwrap_or(0),
                    right: it.next().unwrap_or(0),
                };
                (name, rect)
            })
            .collect();
        RectList { entries }
    }

    pub fn get(&self, name: &str) -> Option<Rect> {
        self.entries.iter().find(|e| e.0 == name).map(|e| e.1)
    }

    pub fn get_index(&self, index: usize) -> Option<Rect> {
        self.entries.get(index).map(|e| e.1)
    }
}

/// `.coli` file: named colour list.
pub struct ColorList {
    pub entries: Vec<(String, u32)>,
}

impl ColorList {
    pub fn parse(text: &str) -> ColorList {
        ColorList {
            entries: named_values(text)
                .into_iter()
                .map(|(name, h)| {
                    let h = h.trim_start_matches('#');
                    let val = if h.len() == 6 {
                        u32::from_str_radix(h, 16).unwrap_or(0)
                    } else {
                        0
                    };
                    (name, val)
                })
                .collect(),
        }
    }

    pub fn get(&self, name: &str) -> Option<u32> {
        self.entries.iter().find(|e| e.0 == name).map(|e| e.1)
    }

    pub fn get_index(&self, index: usize) -> Option<u32> {
        self.entries.get(index).map(|e| e.1)
    }
}

/// `.edli` file: named 4-char-ID list (e.g. sprite group → face ID).
pub struct IdList {
    pub entries: Vec<(String, ResId)>,
}

impl IdList {
    pub fn parse(text: &str) -> IdList {
        IdList {
            entries: named_values(text)
                .into_iter()
                .map(|(name, v)| {
                    // IDs are exactly 4 bytes and may contain spaces.
                    let rid = if v.len() == 4 {
                        id(&v)
                    } else {
                        let t = v.trim();
                        if t.len() == 4 { id(t) } else { NONE_ID }
                    };
                    (name, rid)
                })
                .collect(),
        }
    }

    pub fn get(&self, name: &str) -> Option<ResId> {
        self.entries.iter().find(|e| e.0 == name).map(|e| e.1)
    }

    pub fn get_index(&self, index: usize) -> Option<ResId> {
        self.entries.get(index).map(|e| e.1)
    }
}

// ---------------------------------------------------------------------------
// .leve — level definition
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct LevelObject {
    pub unit_id: ResId,
    pub layer_id: ResId,
    pub x: i64,
    pub y: i64,
    pub heading_degrees: i64,
    pub stationary: bool,
    pub terrain_effects: bool,
}

#[derive(Clone)]
pub struct LevelDef {
    pub name: String,
    pub identifier: String,
    pub description: String,
    pub copyright: String,
    /// background rect `<x1,y1,x2,y2>` in level pixels.
    pub background: [i64; 4],
    pub background_image: ResId,
    pub preview_image: ResId,
    pub music: ResId,
    pub media_mask: ResId,
    pub briefing: ResId,
    pub objects: Vec<LevelObject>,
}

impl LevelDef {
    pub fn parse(text: &str) -> LevelDef {
        let mut cur = Cursor::new(text);
        let mut level = LevelDef {
            name: cur.get_string("#name_STR"),
            identifier: cur.get_string("#indentifier_STR"),
            description: cur.get_string("#description_STR"),
            copyright: cur.get_string("#copyright_STR"),
            background: cur.get_rect("#background_RECT"),
            background_image: cur.get_id("#backgroundImage_ID"),
            preview_image: cur.get_id("#previewImage_ID"),
            music: cur.get_id("#music_ID"),
            media_mask: cur.get_id("#mediaMask_ID"),
            briefing: cur.get_id("#briefing_ID"),
            objects: Vec::new(),
        };
        let num_objects = cur.get_int("#numObjects_INT") as usize;
        for _ in 0..num_objects {
            let unit_id = cur.get_id("#unit_ID");
            if unit_id != NONE_ID {
                level.objects.push(LevelObject {
                    unit_id,
                    layer_id: cur.get_id("#layer_ID"),
                    x: cur.get_int("#xLoc_INT"),
                    y: cur.get_int("#yLoc_INT"),
                    heading_degrees: cur.get_int("#headingDegrees_INT"),
                    stationary: cur.get_bool("#isStationary_BOOL"),
                    terrain_effects: cur.get_bool("#enableTerrainEffects_BOOL"),
                });
            }
        }
        level
    }
}

// ---------------------------------------------------------------------------
// .unde — unit definition (core + states + spawn sets + rules)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct SoundRef {
    pub id: ResId,
    pub min_volume: i64,
    pub max_volume: i64,
    pub priority: i64,
    pub min_pitch: f64,
    pub max_pitch: f64,
}

impl SoundRef {
    fn parse(cur: &mut Cursor, key: &str) -> SoundRef {
        SoundRef {
            id: cur.get_id(&format!("#{}_ID", key)),
            min_volume: cur.get_int(&format!("#{}_MinVolume_INT", key)),
            max_volume: cur.get_int(&format!("#{}_MaxVolume_INT", key)),
            priority: cur.get_int(&format!("#{}_Priority_INT", key)),
            min_pitch: cur.get_float(&format!("#{}_MinPitch_FLOAT", key)),
            max_pitch: cur.get_float(&format!("#{}_MaxPitch_FLOAT", key)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SpawnSet {
    pub name: String,
    pub spawn_id: ResId,
    pub x_offset: i64,
    pub y_offset: i64,
    pub adjust_offset_for_rotation: bool,
    pub absolute_coordinates: bool,
    pub rate_min: i64,
    pub rate_max: i64,
    pub volley_min: i64,
    pub volley_max: i64,
    pub delay_between_min: i64,
    pub delay_between_max: i64,
    pub repeat_spawns: bool,
    pub dont_spawn_offscreen: bool,
    pub pause_rotation_while_spawning: bool,
    pub time_to_pause_rotation: i64,
    pub spawn_if_fleeing: bool,
    pub stationary_option: bool,
    pub terrain_effects_option: bool,
    pub set_heading: bool,
    pub heading_degrees: i64,
}

#[derive(Clone, Debug, Default)]
pub struct StateRule {
    pub name: String,
    pub unit_id: ResId,
    pub range: i64,
    pub condition: String,
    pub action: String,
}

#[derive(Clone, Debug, Default)]
pub struct UnitState {
    pub name: String,
    pub on_range: f64,
    pub on_range_change_to: String,
    pub on_hit_change_to: String,
    pub on_hit_change_state_delay: i64,
    pub on_timer_min: i64,
    pub on_timer_max: i64,
    pub on_timer_change_to: String,
    pub on_counter: i64,
    pub on_counter_change_to: String,
    pub sprite_face: ResId,
    pub sprite_frame_min: i64,
    pub sprite_frame_max: i64,
    pub use_parent_direction: bool,
    pub required_visibility_percent: i64,
    pub visibility_delta_percent: i64,
    pub required_scale_percent: i64,
    pub scale_delta_percent: i64,
    pub tint_percent: i64,
    pub tint_delta_percent: i64,
    pub tint_color: u32,
    pub do_colorise: bool,
    pub num_directions: i64,
    pub frames_per_direction: i64,
    pub frame_delay: i64,
    pub frame_delta: i64,
    pub animate_backwards: bool,
    pub loop_animation: bool,
    pub continuous_frame_randomisation: bool,
    pub rotate_to_target: bool,
    pub max_speed: f64,
    pub delta: f64,
    pub hold_max_speed: f64,
    pub hold_delta: f64,
    pub flee_id: ResId,
    pub flee_speed: f64,
    pub flee_delta: f64,
    pub entry_sound: SoundRef,
    pub sound_loop_delay: i64,
    pub sound_max_num_to_play: i64,
    pub sound_loop: bool,
    pub sound_allow_only_one_instance: bool,
    pub sound_repeat_on_state_change: bool,
    pub invulnerable_until_children_destroyed: bool,
    pub invulnerable_until_owner_destroyed: bool,
    pub use_owners_visibility: bool,
    pub use_owners_scale: bool,
    pub destroy_on_owner_destruction: bool,
    pub delete_on_owner_deletion: bool,
    pub pass_hits_to_owner: bool,
    pub visually_reflect_owner_hits: bool,
    pub destroy_owner_on_destruction: bool,
    pub lock_to_owner_loc: bool,
    pub link_to_owner_loc: bool,
    pub orbit_owner: bool,
    pub collision_spawn_id: ResId,
    pub collision_repeat_spawns: bool,
    pub collision_spawn_delay: i64,
    pub spawn_sets: Vec<SpawnSet>,
    pub motion_blur_required: bool,
    pub motion_blur_allow_glow: bool,
    pub motion_blur_min_time: i64,
    pub motion_blur_max_time: i64,
    pub motion_blur_initial_visibility: f64,
    pub motion_blur_visibility_delta: f64,
    pub particles_id: ResId,
    pub particles_color: u32,
    pub particles_repeat: bool,
    pub particles_repeat_delay: i64,
    pub particles_max_bursts: i64,
    pub pause_vertical_scrolling: bool,
    pub collides: bool,
    pub shields_do_not_deplete_on_collision: bool,
    pub pickup_no_appearance_change: bool,
    pub hunts: bool,
    pub cyclic_motion: bool,
    pub hold_position_to_target: bool,
    pub reverse_direction_on_reaction: bool,
    pub is_targetable: bool,
    pub collides_with_players: bool,
    pub delete_on_no_active_players: bool,
    pub destruct_on_no_active_players: bool,
    pub destruct_if_scrolling_not_paused: bool,
    pub draw_to_terrain: bool,
    pub do_not_glow_on_collision: bool,
    pub use_on_shield_depletion: bool,
    pub use_on_weapon_powerup_release: bool,
    pub rules: Vec<StateRule>,
}

#[derive(Clone, Debug, Default)]
pub struct UnitDef {
    pub id: ResId,
    pub name: String,
    pub family_name: String,
    pub description: String,
    pub num_in_group_min: i64,
    pub num_in_group_max: i64,
    pub group_delay_min: i64,
    pub group_delay_max: i64,
    pub appears_percent: i64,
    pub delete_existing_owned_by_player: bool,
    pub do_not_spawn_if_type_exists: bool,
    pub harmless_to_players: bool,
    pub player_projectile: bool,
    pub can_be_hit_by_player_projectile: bool,
    pub terrain_effect: bool,
    pub constrain_in_game_area: bool,
    pub casts_shadows: bool,
    pub adjust_shadow_loc_for_scaling: bool,
    pub randomise_initial_loc: bool,
    pub adjust_initial_loc_for_owner_scale: bool,
    pub initially_hunts_closest_player: bool,
    pub hittable_when_invisible: bool,
    pub initial_heading_set_in_editor: bool,
    pub flees_north_on_no_players: bool,
    pub flees_south_on_no_players: bool,
    pub collides_with_ground_obstacles: bool,
    pub spawn_only_when_players_active: bool,
    pub use_preview_appearance_in_editor: bool,
    pub allow_stationary_in_editor: bool,
    pub include_in_air_accuracy: bool,
    pub include_in_ground_accuracy: bool,
    pub editor_preview_sprite_face: ResId,
    pub editor_preview_sprite_frame: i64,
    pub pickup_type: ResId,
    pub pickup_multiplier_spawn: ResId,
    pub pickup_value: i64,
    pub destruct_spawn: ResId,
    pub destruct_sound: SoundRef,
    pub destruct_particle: ResId,
    pub destruct_particle_color: u32,
    pub destruct_notice: String,
    pub destruct_num_coins: i64,
    pub destruct_coin: ResId,
    pub destruct_coin_on_group_kill: ResId,
    pub destruct_destroy_children: bool,
    pub destruct_delete_children: bool,
    pub destruct_draw_to_terrain: bool,
    pub destruct_release_random_bonus: bool,
    pub destruct_create_obstacle: bool,
    pub shields_max: f64,
    pub shields_level_increment: f64,
    pub shields_base: f64,
    pub score: i64,
    pub damage: f64,
    pub initial_visibility_percent: i64,
    pub initial_scale_percent: i64,
    pub initial_scale_percent_tolerance: i64,
    pub draw_layer: ResId,
    pub entry_notice: String,
    pub entry_notice_delay: i64,
    pub display_notice_once_only: bool,
    pub entry_notice_sound: SoundRef,
    pub hit_particles: ResId,
    pub hit_particle_circular_burst: bool,
    pub hit_particles_color: u32,
    pub shield_sound: SoundRef,
    pub unshielded_sound: SoundRef,
    pub deletion_spawn: ResId,
    pub x_offset_min: f64,
    pub x_offset_max: f64,
    pub y_offset_min: f64,
    pub y_offset_max: f64,
    pub initial_heading: i64,
    pub initial_heading_tolerance: i64,
    pub use_owner_heading: bool,
    pub initial_speed_min: f64,
    pub initial_speed_max: f64,
    pub do_burst: bool,
    pub do_implode: bool,
    pub is_ground_based: bool,
    pub do_death_spawn_on_any_media: bool,
    pub media_impact_size: ResId,
    pub states: Vec<UnitState>,
    /// Set when any state references owner/child linkage (the original's
    /// `def+0x11` flag, used to decide whether owner tracking is needed).
    pub uses_owner_linkage: bool,
}

impl UnitDef {
    pub fn parse(id_: ResId, text: &str) -> UnitDef {
        let mut cur = Cursor::new(text);
        let mut def = UnitDef {
            id: id_,
            name: cur.get_string("#name_STR"),
            family_name: cur.get_string("#familyName_STR"),
            description: cur.get_string("#description_STR"),
            num_in_group_min: cur.get_int("#numInGroupMin_INT"),
            num_in_group_max: cur.get_int("#numInGroupMax_INT"),
            group_delay_min: cur.get_int("#groupDelayMin_INT"),
            group_delay_max: cur.get_int("#groupDelayMax_INT"),
            appears_percent: cur.get_int("#appearsPercent_INT"),
            delete_existing_owned_by_player: cur
                .get_bool("#deleteExistingEntitiesOfThisTypeOwnedByPlayer_BOOL"),
            do_not_spawn_if_type_exists: cur.get_bool("#doNotSpawnIfTypeAlreadyExists_BOOL"),
            harmless_to_players: cur.get_bool("#harmlessToPlayers_BOOL"),
            player_projectile: cur.get_bool("#playerProjectile_BOOL"),
            can_be_hit_by_player_projectile: cur.get_bool("#canBeHitByPlayerProjectile_BOOL"),
            terrain_effect: cur.get_bool("#terrainEffect_BOOL"),
            constrain_in_game_area: cur.get_bool("#constrainInGameArea_BOOL"),
            casts_shadows: cur.get_bool("#castsShadows_BOOL"),
            adjust_shadow_loc_for_scaling: cur.get_bool("#adjustShadowLocForScaling_BOOL"),
            randomise_initial_loc: cur.get_bool("#randomiseInitialLoc_BOOL"),
            adjust_initial_loc_for_owner_scale: cur
                .get_bool("#adjustInitialLocForOwnerScale_BOOL"),
            initially_hunts_closest_player: cur.get_bool("#initiallyHuntsClosestPlayer_BOOL"),
            hittable_when_invisible: cur.get_bool("#hittableWhenInvisible_BOOL"),
            initial_heading_set_in_editor: cur.get_bool("#initialHeadingSetInEditor_BOOL"),
            flees_north_on_no_players: cur.get_bool("#fleesNorthOnNoActivePlayers_BOOL"),
            flees_south_on_no_players: cur.get_bool("#fleesSouthOnNoActivePlayers_BOOL"),
            collides_with_ground_obstacles: cur.get_bool("#collidesWithGroundObstacles_BOOL"),
            spawn_only_when_players_active: cur
                .get_bool("#canBeSpawnedOnlyWhenPlayersActive_BOOL"),
            use_preview_appearance_in_editor: cur
                .get_bool("#usePreviewAppearanceInPlacementEditor_BOOL"),
            allow_stationary_in_editor: cur
                .get_bool("#allowStationaryOptionInPlacementEditor_BOOL"),
            include_in_air_accuracy: cur.get_bool("#includeInAirAccuracyCount_BOOL"),
            include_in_ground_accuracy: cur.get_bool("#includeInGroundAccuracyCount_BOOL"),
            editor_preview_sprite_face: cur.get_id("#editorPreviewSpriteFace_ID"),
            editor_preview_sprite_frame: cur.get_int("#editorPreviewSpriteFrame_INT"),
            pickup_type: cur.get_id("#pickup_Type_ID"),
            pickup_multiplier_spawn: cur.get_id("#pickup_MultiplierSpawn_ID"),
            pickup_value: cur.get_int("#pickup_Value_INT"),
            destruct_spawn: cur.get_id("#destructSpawn_ID"),
            destruct_sound: SoundRef::parse(&mut cur, "destructSound"),
            destruct_particle: cur.get_id("#destructParticle_ID"),
            destruct_particle_color: cur.get_color("#destructParticleColor_COLOR"),
            destruct_notice: cur.get_string("#destructNotice_STR"),
            destruct_num_coins: cur.get_int("#destructNumCoinsToRelease_INT"),
            destruct_coin: cur.get_id("#destructCoin_ID"),
            destruct_coin_on_group_kill: cur.get_id("#destructCoinOnGroupKill_ID"),
            destruct_destroy_children: cur.get_bool("#destructDestroyChildren_BOOL"),
            destruct_delete_children: cur.get_bool("#destructDeleteChildren_BOOL"),
            destruct_draw_to_terrain: cur.get_bool("#destructDrawToTerrain_BOOL"),
            destruct_release_random_bonus: cur.get_bool("#destructReleaseRandomBonus_BOOL"),
            destruct_create_obstacle: cur.get_bool("#destructCreateObstacle_BOOL"),
            shields_max: cur.get_float("#shields_MaxAmount_FLOAT"),
            shields_level_increment: cur.get_float("#shields_LevelIncrement_FLOAT"),
            shields_base: cur.get_float("#shields_BaseAmount_FLOAT"),
            score: cur.get_int("#score_INT"),
            damage: cur.get_float("#damage_FLOAT"),
            initial_visibility_percent: cur.get_int("#initialVisibilityPercent_INT"),
            initial_scale_percent: cur.get_int("#initialScalePercent_INT"),
            initial_scale_percent_tolerance: cur.get_int("#initialScalePercentTolerance_INT"),
            draw_layer: cur.get_id("#drawLayer_ID"),
            entry_notice: cur.get_string("#entryNotice_STR"),
            entry_notice_delay: cur.get_int("#entryNoticeDelay_INT"),
            display_notice_once_only: cur.get_bool("#displayNoticeOnceOnly_BOOL"),
            entry_notice_sound: SoundRef::parse(&mut cur, "entryNoticeSound"),
            hit_particles: cur.get_id("#hitParticles_ID"),
            hit_particle_circular_burst: cur.get_bool("#hitParticleDoCircularBurst_BOOL"),
            hit_particles_color: cur.get_color("#hitParticlesColor_COLOR"),
            shield_sound: SoundRef::parse(&mut cur, "shieldSound"),
            unshielded_sound: SoundRef::parse(&mut cur, "unshieldedSound"),
            deletion_spawn: cur.get_id("#deletionSpawn_ID"),
            x_offset_min: cur.get_float("#xOffsetMin_FLOAT"),
            x_offset_max: cur.get_float("#xOffsetMax_FLOAT"),
            y_offset_min: cur.get_float("#yOffsetMin_FLOAT"),
            y_offset_max: cur.get_float("#yOffsetMax_FLOAT"),
            initial_heading: cur.get_int("#initialHeading_INT"),
            initial_heading_tolerance: cur.get_int("#initialHeadingTolerance_INT"),
            use_owner_heading: cur.get_bool("#useOwnerHeading_BOOL"),
            initial_speed_min: cur.get_float("#initialSpeedMin_FLOAT"),
            initial_speed_max: cur.get_float("#initialSpeedMax_FLOAT"),
            do_burst: cur.get_bool("#doBurst_BOOL"),
            do_implode: cur.get_bool("#doImplode_BOOL"),
            is_ground_based: cur.get_bool("#isGroundBased_BOOL"),
            do_death_spawn_on_any_media: cur.get_bool("#doDeathSpawnOnAnyMedia_BOOL"),
            media_impact_size: cur.get_id("#mediaImpactSize_ID"),
            states: Vec::new(),
            uses_owner_linkage: false,
        };
        // shields_MaxAmount must be >= shields_BaseAmount (original clamps).
        if def.shields_max < def.shields_base {
            def.shields_max = def.shields_base;
        }
        let num_states = cur.get_int("#numStates_INT") as usize;
        for _ in 0..num_states {
            def.states.push(Self::parse_state(&mut cur));
        }
        def.uses_owner_linkage = def.states.iter().any(|s| {
            s.use_parent_direction
                || s.invulnerable_until_children_destroyed
                || s.invulnerable_until_owner_destroyed
                || s.use_owners_visibility
                || s.use_owners_scale
                || s.destroy_on_owner_destruction
                || s.delete_on_owner_deletion
                || s.pass_hits_to_owner
                || s.visually_reflect_owner_hits
                || s.destroy_owner_on_destruction
                || s.lock_to_owner_loc
                || s.link_to_owner_loc
                || s.orbit_owner
        });
        def
    }

    fn parse_state(cur: &mut Cursor) -> UnitState {
        // Field read order mirrors G_UnitDef_ParseState exactly: the cursor is
        // sequential, so every Get* must happen at its position in the file.
        let mut st = UnitState {
            name: cur.get_string("#stateName_STR"),
            on_range: cur.get_float("#stateOnRange_FLOAT"),
            on_range_change_to: cur.get_string("#stateOnRangeChangeTo_STR"),
            on_hit_change_to: cur.get_string("#stateOnHitChangeTo_STR"),
            on_hit_change_state_delay: cur.get_int("#stateOnHitChangeStateDelay_INT"),
            on_timer_min: cur.get_int("#stateOnTimerMin_INT"),
            on_timer_max: cur.get_int("#stateOnTimerMax_INT"),
            on_timer_change_to: cur.get_string("#stateOnTimerChangeTo_STR"),
            on_counter: cur.get_int("#stateOnCounter_INT"),
            on_counter_change_to: cur.get_string("#stateOnCounterChangeTo_STR"),
            sprite_face: cur.get_id("#stateSpriteFace_ID"),
            sprite_frame_min: cur.get_int("#stateSpriteFrameMin_INT"),
            sprite_frame_max: cur.get_int("#stateSpriteFrameMax_INT"),
            use_parent_direction: cur.get_bool("#stateUseParentDirection_BOOL"),
            required_visibility_percent: cur.get_int("#stateRequiredVisibilityPercent_INT"),
            visibility_delta_percent: cur.get_int("#stateVisibilityDeltaPercent_INT"),
            required_scale_percent: cur.get_int("#stateRequiredScalePercent_INT"),
            scale_delta_percent: cur.get_int("#stateScaleDeltaPercent_INT"),
            tint_percent: cur.get_int("#stateTintPercent_INT"),
            tint_delta_percent: cur.get_int("#stateTintDeltaPercent_INT"),
            tint_color: cur.get_color("#stateTintColor_COLOR"),
            do_colorise: cur.get_bool("#stateDoColorise_BOOL"),
            num_directions: cur.get_int("#stateNumDirections_INT"),
            frames_per_direction: cur.get_int("#stateFramesPerDirection_INT"),
            frame_delay: cur.get_int("#stateFrameDelay_INT"),
            frame_delta: cur.get_int("#stateFrameDelta_INT"),
            animate_backwards: cur.get_bool("#stateDoAnimateBackwards_BOOL"),
            loop_animation: cur.get_bool("#stateDoLoopAnimation_BOOL"),
            continuous_frame_randomisation: cur
                .get_bool("#stateContinuousFrameRandomisation_BOOL"),
            rotate_to_target: cur.get_bool("#stateDoRotateToTarget_BOOL"),
            max_speed: cur.get_float("#stateMaxSpeed_FLOAT"),
            delta: cur.get_float("#stateDelta_FLOAT"),
            hold_max_speed: cur.get_float("#stateHoldMaxSpeed_FLOAT"),
            hold_delta: cur.get_float("#stateHoldDelta_FLOAT"),
            flee_id: cur.get_id("#stateFlee_ID"),
            flee_speed: cur.get_float("#stateFleeSpeed_FLOAT"),
            flee_delta: cur.get_float("#stateFleeDelta_FLOAT"),
            entry_sound: SoundRef::default(),
            sound_loop_delay: 0,
            sound_max_num_to_play: 0,
            sound_loop: false,
            sound_allow_only_one_instance: false,
            sound_repeat_on_state_change: false,
            invulnerable_until_children_destroyed: false,
            invulnerable_until_owner_destroyed: false,
            use_owners_visibility: false,
            use_owners_scale: false,
            destroy_on_owner_destruction: false,
            delete_on_owner_deletion: false,
            pass_hits_to_owner: false,
            visually_reflect_owner_hits: false,
            destroy_owner_on_destruction: false,
            lock_to_owner_loc: false,
            link_to_owner_loc: false,
            orbit_owner: false,
            collision_spawn_id: NONE_ID,
            collision_repeat_spawns: false,
            collision_spawn_delay: 0,
            spawn_sets: Vec::new(),
            motion_blur_required: false,
            motion_blur_allow_glow: false,
            motion_blur_min_time: 0,
            motion_blur_max_time: 0,
            motion_blur_initial_visibility: 0.0,
            motion_blur_visibility_delta: 0.0,
            particles_id: NONE_ID,
            particles_color: 0,
            particles_repeat: false,
            particles_repeat_delay: 0,
            particles_max_bursts: 0,
            pause_vertical_scrolling: false,
            collides: false,
            shields_do_not_deplete_on_collision: false,
            pickup_no_appearance_change: false,
            hunts: false,
            cyclic_motion: false,
            hold_position_to_target: false,
            reverse_direction_on_reaction: false,
            is_targetable: false,
            collides_with_players: false,
            delete_on_no_active_players: false,
            destruct_on_no_active_players: false,
            destruct_if_scrolling_not_paused: false,
            draw_to_terrain: false,
            do_not_glow_on_collision: false,
            use_on_shield_depletion: false,
            use_on_weapon_powerup_release: false,
            rules: Vec::new(),
        };
        st.entry_sound = SoundRef::parse(cur, "stateEntrySound");
        st.sound_loop_delay = cur.get_int("#stateSoundLoopDelay_INT");
        st.sound_max_num_to_play = cur.get_int("#stateSoundMaxNumToPlay_INT");
        st.sound_loop = cur.get_bool("#stateSoundLoop_BOOL");
        st.sound_allow_only_one_instance =
            cur.get_bool("#stateSoundAllowOnlyOneInstance_BOOL");
        st.sound_repeat_on_state_change =
            cur.get_bool("#stateSoundRepeatOnStateChange_BOOL");
        st.invulnerable_until_children_destroyed =
            cur.get_bool("#invulnerableUntilAllChildrenDestroyed_BOOL");
        st.invulnerable_until_owner_destroyed =
            cur.get_bool("#invulnerableUntilOwnerDestroyed_BOOL");
        st.use_owners_visibility = cur.get_bool("#useOwnersVisibility_BOOL");
        st.use_owners_scale = cur.get_bool("#useOwnersScale_BOOL");
        st.destroy_on_owner_destruction =
            cur.get_bool("#canBeDestroyedOnOwnerDestruction_BOOL");
        st.delete_on_owner_deletion = cur.get_bool("#canBeDeletedOnOwnerDeletion_BOOL");
        st.pass_hits_to_owner = cur.get_bool("#passHitsToOwner_BOOL");
        st.visually_reflect_owner_hits = cur.get_bool("#visuallyReflectOwnerHits_BOOL");
        st.destroy_owner_on_destruction = cur.get_bool("#destroyOwnerOnDestruction_BOOL");
        st.collision_spawn_id = cur.get_id("#collision_Spawn_ID");
        st.collision_repeat_spawns = cur.get_bool("#collision_RepeatSpawns_BOOL");
        st.collision_spawn_delay = cur.get_int("#collision_SpawnDelay_INT");
        if st.on_timer_max < st.on_timer_min {
            st.on_timer_max = st.on_timer_min;
        }
        let num_spawn_sets = cur.get_int("stateNumSpawnSets_INT") as usize;
        for _ in 0..num_spawn_sets {
            st.spawn_sets.push(SpawnSet {
                name: cur.get_string("#stateSpawnSetName_STR"),
                spawn_id: cur.get_id("#stateSpawnSetSpawn_ID"),
                x_offset: cur.get_int("#stateSpawnSetXOffset_INT"),
                y_offset: cur.get_int("#stateSpawnSetYOffset_INT"),
                adjust_offset_for_rotation: cur
                    .get_bool("#stateSpawnSetAdjustOffsetForUnitRotation_BOOL"),
                absolute_coordinates: cur.get_bool("#stateSpawnSet_AbsoluteCoordinates_BOOL"),
                rate_min: cur.get_int("#stateSpawnSetRateMin_INT"),
                rate_max: cur.get_int("#stateSpawnSetRateMax_INT"),
                volley_min: cur.get_int("#stateSpawnSetNumInVolleyMin_INT"),
                volley_max: cur.get_int("#stateSpawnSetNumInVolleyMax_INT"),
                delay_between_min: cur.get_int("#stateSpawnSetDelayBetweenEntitiesMin_INT"),
                delay_between_max: cur.get_int("#stateSpawnSetDelayBetweenEntitiesMax_INT"),
                repeat_spawns: cur.get_bool("#stateSpawnSetRepeatSpawns_BOOL"),
                dont_spawn_offscreen: cur.get_bool("#stateSpawnSetDon'tSpawnOffscreen_BOOL"),
                pause_rotation_while_spawning: cur
                    .get_bool("#stateSpawnSetPauseAnyRotationWhileSpawning_BOOL"),
                time_to_pause_rotation: cur
                    .get_int("#stateSpawnSetTimeToPauseRotationAfterSpawning_INT"),
                spawn_if_fleeing: cur.get_bool("#stateSpawnSetSpawnIfFleeing_BOOL"),
                stationary_option: cur.get_bool("#stateSpawnSet_StationaryOption_BOOL"),
                terrain_effects_option: cur.get_bool("#stateSpawnSet_TerrainEffectsOption_BOOL"),
                set_heading: cur.get_bool("#stateSpawnSetSetHeading_BOOL"),
                heading_degrees: cur.get_int("#stateSpawnSetHeadingDegrees_INT"),
            });
        }
        st.motion_blur_required = cur.get_bool("#state_MotionBlur_Required_BOOL");
        st.motion_blur_allow_glow = cur.get_bool("#state_MotionBlur_AllowGlowDrawing_BOOL");
        st.motion_blur_min_time = cur.get_int("#state_MotionBlur_MinTimeBetweenBlurs_INT");
        st.motion_blur_max_time = cur.get_int("#state_MotionBlur_MaxTimeBetweenBlurs_INT");
        st.motion_blur_initial_visibility =
            cur.get_float("#state_MotionBlur_InitialVisibilityPercent_FLOAT");
        st.motion_blur_visibility_delta =
            cur.get_float("#state_MotionBlur_VisibilityDeltaPercent_FLOAT");
        st.particles_id = cur.get_id("#stateParticles_ID");
        st.particles_color = cur.get_color("#stateParticlesColor_COLOR");
        st.particles_repeat = cur.get_bool("#stateParticlesRepeat_BOOL");
        st.particles_repeat_delay = cur.get_int("#stateParticles_RepeatDelay_INT");
        st.particles_max_bursts = cur.get_int("#stateParticles_MaxNumBursts_INT");
        st.pause_vertical_scrolling = cur.get_bool("#statePauseVerticalScrolling_BOOL");
        st.collides = cur.get_bool("#stateCollides_BOOL");
        st.shields_do_not_deplete_on_collision =
            cur.get_bool("#stateInvulnerable_ShieldsDoNotDepleteOnCollision_BOOL");
        st.pickup_no_appearance_change =
            cur.get_bool("#statePickup_DoNotChangeAppearanceOnStateChange_BOOL");
        st.hunts = cur.get_bool("#stateHunts_BOOL");
        st.cyclic_motion = cur.get_bool("#stateCyclicMotion_BOOL");
        st.hold_position_to_target = cur.get_bool("#stateHoldPositionToTarget_BOOL");
        st.reverse_direction_on_reaction =
            cur.get_bool("#stateReverseDirectionOnReaction_BOOL");
        st.is_targetable = cur.get_bool("#stateIsTargetable_BOOL");
        st.collides_with_players = cur.get_bool("#stateCollidesWithPlayers_BOOL");
        st.lock_to_owner_loc = cur.get_bool("#stateLockToOwnerLoc_BOOL");
        st.link_to_owner_loc = cur.get_bool("#stateLinkToOwnerLoc_BOOL");
        st.orbit_owner = cur.get_bool("#stateOrbitOwner_BOOL");
        st.delete_on_no_active_players = cur.get_bool("#stateDeleteOnNoActivePlayers_BOOL");
        st.destruct_on_no_active_players =
            cur.get_bool("#stateDestructOnNoActivePlayers_BOOL");
        st.destruct_if_scrolling_not_paused =
            cur.get_bool("#stateDestructIfVerticalScrollingNotPaused_BOOL");
        st.draw_to_terrain = cur.get_bool("#stateDrawToTerrain_BOOL");
        st.do_not_glow_on_collision = cur.get_bool("#stateDoNotGlowOnCollision_BOOL");
        st.use_on_shield_depletion =
            cur.get_bool("#stateUseThisStateOnShieldDepletion_BOOL");
        st.use_on_weapon_powerup_release =
            cur.get_bool("#stateUseThisStateOnWeaponPowerupRelease_BOOL");
        let num_rules = cur.get_int("#stateNumRules_INT") as usize;
        for _ in 0..num_rules {
            st.rules.push(StateRule {
                name: cur.get_string("#stateRuleName_STR"),
                unit_id: cur.get_id("#stateRuleUnit_ID"),
                range: cur.get_int("#stateRuleRange_INT"),
                condition: cur.get_string("#stateRuleCondition_STR"),
                action: cur.get_string("stateRuleAction_STR"),
            });
        }
        st
    }
}

// ---------------------------------------------------------------------------
// .plde — player definition
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct PlayerDef {
    pub name: String,
    pub sprite_high_score: ResId,
    pub sprite_high_score_frame: i64,
    pub sprite_score_bar: ResId,
    pub sprite_score_bar_frame: i64,
    pub sprite_score_bar_power: ResId,
    pub sprite_score_bar_power_frame: i64,
    pub sprite_score_bar_shield: ResId,
    pub sprite_score_bar_shield_frame: i64,
    pub default_shield_percentage: i64,
    pub shield_warning_percentage: i64,
    pub shield_base_hit_percentage: i64,
    pub shield_hit_delay: i64,
    pub hit_glow_color: u32,
    pub hit_glow_speed: i64,
    pub life_max_num: i64,
    pub life_num_initial: i64,
    pub life_initial_required_score: i64,
    pub life_additional_required_score: i64,
    pub life_spawn: ResId,
    pub waiting_time: i64,
    pub film_intro_time: i64,
    pub intro_time: i64,
    pub game_over_time: i64,
    pub dying_time: i64,
    pub final_dying_time: i64,
    pub entry_solo_start_x: i64,
    pub entry_solo_start_y: i64,
    pub entry_multi_start_x: i64,
    pub entry_multi_start_y: i64,
    pub entry_spawn: ResId,
    pub entry_start_velocity_x: f64,
    pub entry_start_velocity_y: f64,
    pub entry_target_velocity_x: f64,
    pub entry_target_velocity_y: f64,
    pub entry_velocity_delta: f64,
    pub entry_initial_delay: i64,
    pub entry_invulnerability_time: i64,
    pub death_spawn: ResId,
    pub death_duration: i64,
    pub active_money_counter_spawn: ResId,
    pub active_spawn_on_hit: ResId,
    pub active_shield_warning_object: ResId,
    pub active_defence_bonus_object: ResId,
    pub active_default_max_speed: f64,
    pub active_velocity_delta: f64,
    pub powerup_overload_num_warnings: i64,
    pub powerup_overload_initial_time_between_warnings: i64,
    pub powerup_overload_min_time_between_warnings: i64,
    pub powerup_overload_warning_fade_percent: f64,
    pub powerup_overload_hilite: u32,
    pub powerup_overload_sound: SoundRef,
    // Control bindings (keyboard) — parsed below if present.
    pub bindings: Vec<(String, String)>,
}

impl PlayerDef {
    pub fn parse(text: &str) -> PlayerDef {
        let mut cur = Cursor::new(text);
        let mut def = PlayerDef {
            name: cur.get_string("#name_STR"),
            sprite_high_score: cur.get_id("#spriteHighScore_ID"),
            sprite_high_score_frame: cur.get_int("#spriteHighScoreFrame_INT"),
            sprite_score_bar: cur.get_id("#spriteScoreBar_ID"),
            sprite_score_bar_frame: cur.get_int("#spriteScoreBarFrame_INT"),
            sprite_score_bar_power: cur.get_id("#spriteScoreBarPower_ID"),
            sprite_score_bar_power_frame: cur.get_int("#spriteScoreBarPowerFrame_INT"),
            sprite_score_bar_shield: cur.get_id("#spriteScoreBarShield_ID"),
            sprite_score_bar_shield_frame: cur.get_int("#spriteScoreBarShieldFrame_INT"),
            default_shield_percentage: cur.get_int("#defaultShieldPercentage_INT"),
            shield_warning_percentage: cur.get_int("#shieldWarningPercentage_INT"),
            shield_base_hit_percentage: cur.get_int("#shieldBaseHitPercentage_INT"),
            shield_hit_delay: cur.get_int("#shieldHitDelay_INT"),
            hit_glow_color: cur.get_color("#hitGlowColor_COLOR"),
            hit_glow_speed: cur.get_int("#hitGlowSpeed_INT"),
            life_max_num: cur.get_int("#life_MaxNum_INT"),
            life_num_initial: cur.get_int("#life_NumInitial_INT"),
            life_initial_required_score: cur.get_int("#life_InitialRequiredScore_INT"),
            life_additional_required_score: cur.get_int("#life_AdditionalRequiredScore_INT"),
            life_spawn: cur.get_id("#life_Spawn_ID"),
            waiting_time: cur.get_int("#waitingTime_INT"),
            film_intro_time: cur.get_int("#filmIntroTime_INT"),
            intro_time: cur.get_int("#introTime_INT"),
            game_over_time: cur.get_int("#gameOverTime_INT"),
            dying_time: cur.get_int("#dyingTime_INT"),
            final_dying_time: cur.get_int("#finalDyingTime_INT"),
            entry_solo_start_x: cur.get_int("#entry_soloStartX_INT"),
            entry_solo_start_y: cur.get_int("#entry_soloStartY_INT"),
            entry_multi_start_x: cur.get_int("#entry_multiStartX_INT"),
            entry_multi_start_y: cur.get_int("#entry_multiStartY_INT"),
            entry_spawn: cur.get_id("#entry_Spawn_ID"),
            entry_start_velocity_x: cur.get_float("#entry_StartVelocityX_FLOAT"),
            entry_start_velocity_y: cur.get_float("#entry_StartVelocityY_FLOAT"),
            entry_target_velocity_x: cur.get_float("#entry_TargetVelocityX_FLOAT"),
            entry_target_velocity_y: cur.get_float("#entry_TargetVelocityY_FLOAT"),
            entry_velocity_delta: cur.get_float("#entry_VelocityDelta_FLOAT"),
            entry_initial_delay: cur.get_int("#entry_InitialDelay_INT"),
            entry_invulnerability_time: cur.get_int("#entry_InvulnerabilityTime_INT"),
            death_spawn: cur.get_id("#death_Spawn_ID"),
            death_duration: cur.get_int("#death_Duration_INT"),
            active_money_counter_spawn: cur.get_id("#active_MoneyCounterSpawn_ID"),
            active_spawn_on_hit: cur.get_id("#active_SpawnOnHit_ID"),
            active_shield_warning_object: cur.get_id("#active_ShieldWarningObject_ID"),
            active_defence_bonus_object: cur.get_id("#active_DefenceBonusObject_ID"),
            active_default_max_speed: cur.get_float("#active_DefaultMaxSpeed_FLOAT"),
            active_velocity_delta: cur.get_float("#active_VelocityDelta_FLOAT"),
            powerup_overload_num_warnings: cur.get_int("#powerupOverload_NumWarnings_INT"),
            powerup_overload_initial_time_between_warnings: cur
                .get_int("#powerupOverload_InitialTimeBetweenWarnings_INT"),
            powerup_overload_min_time_between_warnings: cur
                .get_int("#powerupOverload_MinimumTimeBetweenWarnings_INT"),
            powerup_overload_warning_fade_percent: cur
                .get_float("#powerupOverload_WarningFadePercent_FLOAT"),
            powerup_overload_hilite: cur.get_color("#powerupOverload_Hilite_COLOR"),
            powerup_overload_sound: SoundRef::default(),
            bindings: Vec::new(),
        };
        def.powerup_overload_sound.id = cur.get_id("#powerupOverloadSound_ID");
        def.powerup_overload_sound.min_volume =
            cur.get_int("#powerupOverloadSound_MinVolume_INT");
        def.powerup_overload_sound.max_volume =
            cur.get_int("#powerupOverloadSound_MaxVolume_INT");
        def.powerup_overload_sound.priority =
            cur.get_int("#powerupOverloadSound_Priority_INT");
        def.powerup_overload_sound.min_pitch =
            cur.get_float("#powerupOverloadSound_MinPitch_FLOAT");
        def.powerup_overload_sound.max_pitch =
            cur.get_float("#powerupOverloadSound_MaxPitch_FLOAT");
        def
    }
}

// ---------------------------------------------------------------------------
// .wede — weapon definition
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct WeaponSpawn {
    pub name: String,
    pub unit_id: ResId,
    pub x_loc: i64,
    pub y_loc: i64,
    pub set_heading: bool,
    pub angle: i64,
}

#[derive(Clone, Debug, Default)]
pub struct WeaponPowerup {
    pub air_time_until_activation: i64,
    pub air_activation_spawn: ResId,
    pub air_time_between_power_changes: i64,
    pub air_max_power_level: i64,
    pub air_overload_time: i64,
    pub air_release_spawn: ResId,
    pub air_time_between_release_spawns: i64,
    pub air_do_release_on_max: bool,
    pub ground_time_until_activation: i64,
    pub ground_activation_spawn: ResId,
    pub ground_time_between_power_changes: i64,
    pub ground_max_power_level: i64,
    pub ground_overload_time: i64,
    pub ground_release_spawn: ResId,
    pub ground_time_between_release_spawns: i64,
    pub ground_do_release_on_max: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WeaponDef {
    pub id: ResId,
    pub weapon_type: ResId,
    pub default_id: ResId,
    pub name: String,
    pub description1: String,
    pub description2: String,
    pub score_bar_preview_face: ResId,
    pub score_bar_preview_frame: i64,
    pub max_allowed: i64,
    pub min_level_available: i64,
    pub max_level_available: i64,
    pub player1_appearance_face: ResId,
    pub player2_appearance_face: ResId,
    pub player_glow_color: u32,
    pub selection_sound: SoundRef,
    pub crosshair_face: ResId,
    pub crosshair_frame: i64,
    pub crosshair_locked_face: ResId,
    pub crosshair_locked_frame: i64,
    pub crosshair_x_offset: i64,
    pub crosshair_y_offset: i64,
    pub crosshair_spawn_on_activation: ResId,
    pub num_ammo_in_pack: i64,
    pub ammo_warn_at_count: i64,
    pub ammo_warning: String,
    pub auto_repeat: bool,
    pub delay_between_launches: i64,
    pub delay_between_load_launches: i64,
    pub shield_increase: i64,
    pub lives_increase: i64,
    pub invulnerable_for_time: i64,
    pub spawns: Vec<WeaponSpawn>,
    pub powerup: WeaponPowerup,
}

impl WeaponDef {
    pub fn parse(id_: ResId, text: &str) -> WeaponDef {
        let mut cur = Cursor::new(text);
        let mut def = WeaponDef {
            id: id_,
            weapon_type: cur.get_id("#type_ID"),
            default_id: cur.get_id("#default_ID"),
            name: cur.get_string("#name_STR"),
            description1: cur.get_string("#description1_STR"),
            description2: cur.get_string("#description2_STR"),
            score_bar_preview_face: cur.get_id("#scoreBarPreviewFace_ID"),
            score_bar_preview_frame: cur.get_int("#scoreBarPreviewFrame_INT"),
            max_allowed: cur.get_int("#maxAllowed_INT"),
            min_level_available: cur.get_int("#minimumLevelAvailable_INT"),
            max_level_available: cur.get_int("#maximumLevelAvailable_INT"),
            player1_appearance_face: cur.get_id("#player1AppearanceFace_ID"),
            player2_appearance_face: cur.get_id("#player2AppearanceFace_ID"),
            player_glow_color: cur.get_color("#playerGlow_COLOR"),
            selection_sound: SoundRef::parse(&mut cur, "selectionSound"),
            crosshair_face: cur.get_id("#crosshairFace_ID"),
            crosshair_frame: cur.get_int("#crosshairFrame_INT"),
            crosshair_locked_face: cur.get_id("#crosshairLockedFace_ID"),
            crosshair_locked_frame: cur.get_int("#crosshairLockedFrame_INT"),
            crosshair_x_offset: cur.get_int("#crosshairXOffset_INT"),
            crosshair_y_offset: cur.get_int("#crosshairYOffset_INT"),
            crosshair_spawn_on_activation: cur.get_id("#crosshairSpawnOnActivation_ID"),
            num_ammo_in_pack: cur.get_int("#numAmmoInPack_INT"),
            ammo_warn_at_count: cur.get_int("#ammoWarnAtCount_INT"),
            ammo_warning: cur.get_string("#ammoWarning_STR"),
            auto_repeat: cur.get_bool("#autoRepeat_BOOL"),
            delay_between_launches: cur.get_int("#delayBetweenLaunches_INT"),
            delay_between_load_launches: cur.get_int("#delayBetweenLoadLaunches_INT"),
            shield_increase: cur.get_int("#shieldIncrease_INT"),
            lives_increase: cur.get_int("#livesIncrease_INT"),
            invulnerable_for_time: cur.get_int("#invulnerableForTime_INT"),
            spawns: Vec::new(),
            powerup: WeaponPowerup::default(),
        };
        let num_units = cur.get_int("#spawn_NumUnitsToSpawn_INT") as usize;
        for _ in 0..num_units {
            def.spawns.push(WeaponSpawn {
                name: cur.get_string("#spawn_Name_STR"),
                unit_id: cur.get_id("#spawn_Unit_ID"),
                x_loc: cur.get_int("#spawn_XLoc_INT"),
                y_loc: cur.get_int("#spawn_YLoc_INT"),
                set_heading: cur.get_bool("#spawn_SetHeading_BOOL"),
                angle: cur.get_int("#spawn_Angle_INT"),
            });
        }
        def.powerup.air_time_until_activation =
            cur.get_int("#powerup_Air_TimeUntilActivation_INT");
        def.powerup.air_activation_spawn = cur.get_id("#powerup_Air_ActivationSpawn_ID");
        def.powerup.air_time_between_power_changes =
            cur.get_int("#powerup_Air_TimeBetweenPowerLevelChanges_INT");
        def.powerup.air_max_power_level = cur.get_int("#powerup_Air_MaxPowerLevel_INT");
        def.powerup.air_overload_time = cur.get_int("#powerup_Air_OverloadTime_INT");
        def.powerup.air_release_spawn = cur.get_id("#powerup_Air_ReleaseSpawn_ID");
        def.powerup.air_time_between_release_spawns =
            cur.get_int("#powerup_Air_TimeBetweenReleaseSpawns_INT");
        def.powerup.air_do_release_on_max =
            cur.get_bool("#powerup_Air_DoReleaseOnMaxPowerLevel_BOOL");
        def.powerup.ground_time_until_activation =
            cur.get_int("#powerup_Ground_TimeUntilActivation_INT");
        def.powerup.ground_activation_spawn = cur.get_id("#powerup_Ground_ActivationSpawn_ID");
        def.powerup.ground_time_between_power_changes =
            cur.get_int("#powerup_Ground_TimeBetweenPowerLevelChanges_INT");
        def.powerup.ground_max_power_level = cur.get_int("#powerup_Ground_MaxPowerLevel_INT");
        def.powerup.ground_overload_time = cur.get_int("#powerup_Ground_OverloadTime_INT");
        def.powerup.ground_release_spawn = cur.get_id("#powerup_Ground_ReleaseSpawn_ID");
        def.powerup.ground_time_between_release_spawns =
            cur.get_int("#powerup_Ground_TimeBetweenReleaseSpawns_INT");
        def.powerup.ground_do_release_on_max =
            cur.get_bool("#powerup_Ground_DoReleaseOnMaxPowerLevel_BOOL");
        def
    }
}

// ---------------------------------------------------------------------------
// .tefo — text format (font/style description)
// ---------------------------------------------------------------------------

/// A text format: named style parameters read by `U_Text`/`G_Text`.
/// Stored as key→value pairs since access is by name.
#[derive(Clone, Debug, Default)]
pub struct TextFormat {
    /// Ordered (key-without-#-and-type-suffix is kept whole) pairs.
    pub fields: Vec<(String, String)>,
}

impl TextFormat {
    pub fn parse(text: &str) -> TextFormat {
        // `.tefo` files are `\r`-separated `#Key_TYPE <value>` entries with
        // `//` comments. `named_values` skips comments (no `#`/`<>` pair).
        // The `_TYPE` suffix is stripped so `Loc_X_INT` is read as `Loc_X`.
        let fields = named_values(text)
            .into_iter()
            .map(|(k, v)| {
                let name = match k.rfind('_') {
                    Some(i)
                        if k[i + 1..]
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) =>
                    {
                        k[..i].to_string()
                    }
                    _ => k,
                };
                (name, v)
            })
            .collect();
        TextFormat { fields }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|f| f.0 == key).map(|f| f.1.as_str())
    }

    pub fn get_int(&self, key: &str) -> i64 {
        self.get(key)
            .and_then(|v| {
                let t = v.trim();
                t.parse::<i64>()
                    .ok()
                    .or_else(|| t.parse::<f64>().ok().map(|f| f as i64))
            })
            .unwrap_or(0)
    }

    pub fn get_float(&self, key: &str) -> f64 {
        self.get(key).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.get(key).map(|v| v.eq_ignore_ascii_case("TRUE")).unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// .stli — still text (HTML-ish pages: <title>, <page>, body text)
// ---------------------------------------------------------------------------

pub struct StillText {
    /// Strings in file order; the original indexes them by `\r` position.
    pub strings: Vec<String>,
}

impl StillText {
    pub fn parse(text: &str) -> StillText {
        StillText {
            strings: text
                .split(['\r', '\n'])
                .map(|s| s.trim_end().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.strings.get(index).map(|s| s.as_str())
    }
}
