//! Bevy rendering layer: turns `Sim` state into sprites each frame.
//!
//! World→screen mapping (top-left origin, y down):
//!   screen_x = world_x - cam_x          (cam_x = side-scroll offset)
//!   screen_y = scroll + 480 - world_y
//! Bevy world→NDC: camera centered, so bevy_pos = (screen_x - 320, 240 - screen_y).

use crate::assets::GameAssets;
use crate::game::sim::{Entity as SimEntity, PlayerPhase, Sim, SCREEN_H, SCREEN_W};
use crate::pak::ResId;
use bevy::prelude::*;
use std::collections::HashMap;

/// Cache of decoded sprite frames → Bevy images.
#[derive(Resource, Default)]
pub struct TextureBook {
    /// (sprite rid, frame index) → image handle.
    frames: HashMap<(ResId, i64), Handle<Image>>,
    /// im16 rid → handle.
    images16: HashMap<ResId, Handle<Image>>,
}

/// Marks a Bevy entity as representing a sim entity (keyed by unique_num).
#[derive(Component)]
pub struct SimSprite {
    pub key: u32,
}

/// Marks the terrain background sprite.
#[derive(Component)]
pub struct TerrainSprite;

/// Marks a player ship sprite.
#[derive(Component)]
pub struct PlayerSprite {
    pub slot: usize,
}

/// The running game sim.
#[derive(Resource)]
pub struct GameSim(pub Sim);

/// Input state gathered from Bevy keys.
#[derive(Resource, Default)]
pub struct FrameInput {
    pub p1: crate::game::sim::SlotInput,
    pub p2: crate::game::sim::SlotInput,
    /// Number of players in this run.
    pub players: usize,
    /// Level list index for next level / flow control.
    pub level_index: usize,
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

pub fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Convert RGBA pixels to a Bevy image with nearest-neighbour sampling.
pub fn rgba_image(w: u32, h: u32, rgba: &[u8]) -> Image {
    let mut img = Image::new(
        bevy::render::render_resource::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        rgba.to_vec(),
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    img.sampler = bevy::image::ImageSampler::nearest();
    img
}

/// Get-or-create the Bevy image for one sprite frame.
fn frame_image(
    assets: &mut GameAssets,
    images: &mut Assets<Image>,
    book: &mut TextureBook,
    rid: ResId,
    frame: i64,
) -> Option<Handle<Image>> {
    let key = (rid, frame);
    if let Some(h) = book.frames.get(&key) {
        return Some(h.clone());
    }
    let sprite = assets.sprite(rid)?;
    let f = sprite.frames.get(frame.max(0) as usize).or_else(|| sprite.frames.first())?;
    let img = rgba_image(f.width, f.height, &f.rgba);
    let handle = images.add(img);
    book.frames.insert(key, handle.clone());
    Some(handle)
}

fn image16_handle(
    assets: &mut GameAssets,
    images: &mut Assets<Image>,
    book: &mut TextureBook,
    rid: ResId,
) -> Option<Handle<Image>> {
    if let Some(h) = book.images16.get(&rid) {
        return Some(h.clone());
    }
    let (w, h, rgba) = assets.image16(rid)?.clone();
    let img = rgba_image(w, h, &rgba);
    let handle = images.add(img);
    book.images16.insert(rid, handle.clone());
    Some(handle)
}

// ---------------------------------------------------------------------------
// Frame sync
// ---------------------------------------------------------------------------

/// screen coords → bevy world coords (camera at origin, y up).
pub fn to_world(sx: f32, sy: f32) -> Vec3 {
    Vec3::new(sx - SCREEN_W * 0.5, SCREEN_H * 0.5 - sy, 0.0)
}

/// Sync all sim entities to Bevy sprites (spawn/despawn/update).
pub fn sync_entities(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &SimSprite, &mut Sprite, &mut Transform)>,
) {
    let sim = &sim_res.0;

    // Desired set: unique_num -> (sprite rid, frame, screen pos).
    let mut desired: HashMap<u32, (ResId, i64, Vec2, f32)> = HashMap::new();
    for e in &sim.entities {
        if e.dead {
            continue;
        }
        let (rid, frame) = entity_sprite(&*assets, e);
        let sp = e.screen_pos(sim);
        desired.insert(e.unique_num, (rid, frame, sp, e.hit_flash as f32));
    }

    // Despawn removed.
    let mut alive_keys = Vec::new();
    for (bevy_e, ss, _, _) in q.iter() {
        if desired.contains_key(&ss.key) {
            alive_keys.push(ss.key);
        } else {
            commands.entity(bevy_e).despawn();
        }
    }

    // Spawn/update.
    for (key, (rid, frame, sp, flash)) in &desired {
        let handle = match frame_image(&mut *assets, &mut *images, &mut *book, *rid, *frame) {
            Some(h) => h,
            None => continue,
        };
        // Find existing bevy entity for this key.
        let mut found = None;
        for (bevy_e, ss, sprite, transform) in q.iter_mut() {
            if ss.key == *key {
                found = Some((bevy_e, sprite, transform));
                break;
            }
        }
        let wpos = to_world(sp.x, sp.y);
        if let Some((_, mut sprite, mut transform)) = found {
            sprite.image = handle;
            transform.translation = wpos;
            // Hit flash: brighten.
            sprite.color = if *flash > 0.0 {
                Color::srgb(2.0, 2.0, 2.0)
            } else {
                Color::WHITE
            };
        } else {
            commands.spawn((
                Sprite::from_image(handle),
                Transform::from_translation(wpos),
                SimSprite { key: *key },
            ));
        }
    }
}

/// Pick the sprite rid + frame index for an entity's current state.
fn entity_sprite(assets: &GameAssets, e: &SimEntity) -> (ResId, i64) {
    if let Some(def) = assets.units.get(&e.unit) {
        if !def.states.is_empty() {
            let st = &def.states[e.state.min(def.states.len() - 1)];
            if st.sprite_face != crate::pak::NONE_ID {
                return (st.sprite_face, e.frame);
            }
        }
        // Fallback: editor preview sprite.
        if def.editor_preview_sprite_face != crate::pak::NONE_ID {
            return (def.editor_preview_sprite_face, e.frame);
        }
    }
    (e.unit, e.frame)
}

/// Sync terrain background.
pub fn sync_terrain(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &TerrainSprite, &mut Sprite, &mut Transform), Without<PlayerSprite>>,
) {
    let sim = &sim_res.0;
    let rid = sim.level.background_image;
    let Some(handle) = image16_handle(&mut *assets, &mut *images, &mut *book, rid) else {
        return;
    };
    // Visible slice: image rows [top, top+480] where top = levelH - 480 - scroll.
    let level_h = sim.level_height();
    let img_top = (level_h - SCREEN_H - sim.scroll).max(0.0) as u32;
    // Terrain sprite: 480 wide x 480 tall slice at screen (0,0)..(480,480).
    let center = to_world(240.0, 240.0);
    if q.is_empty() {
        commands.spawn((
            Sprite {
                image: handle,
                rect: Some(bevy::math::Rect::new(0.0, img_top as f32, 480.0, img_top as f32 + SCREEN_H)),
                ..default()
            },
            Transform::from_translation(center),
            TerrainSprite,
        ));
    } else {
        for (_, _, mut sprite, mut transform) in q.iter_mut() {
            sprite.image = handle.clone();
            sprite.rect = Some(bevy::math::Rect::new(
                0.0,
                img_top as f32,
                480.0,
                img_top as f32 + SCREEN_H,
            ));
            transform.translation = center;
        }
    }
}

/// Sync player ships.
pub fn sync_players(
    mut commands: Commands,
    sim_res: Res<GameSim>,
    mut assets: ResMut<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    mut book: ResMut<TextureBook>,
    mut q: Query<(Entity, &PlayerSprite, &mut Sprite, &mut Transform)>,
) {
    let sim = &sim_res.0;
    let mut seen = [false; 2];
    for slot in 0..2 {
        let p = match sim.players[slot].as_ref() {
            Some(p) if p.phase != PlayerPhase::Dead => p,
            _ => continue,
        };
        seen[slot] = true;
        // Ship sprite = current air weapon's appearance face for this player.
        let wdef = assets.weapons.get(&p.air_weapon);
        let face = wdef.map(|w| {
            if slot == 0 { w.player1_appearance_face } else { w.player2_appearance_face }
        }).unwrap_or_else(|| crate::pak::id("pl1o"));
        let frame_idx = p.tilt_frame as i64;
        let Some(handle) = frame_image(&mut *assets, &mut *images, &mut *book, face, frame_idx) else {
            continue;
        };
        let wpos = to_world(p.screen.x, p.screen.y);
        // Blink while invulnerable.
        let blink = p.invuln > 0 && (sim.game_time / 4) % 2 == 0;
        let mut found = None;
        for (bevy_e, ps, sprite, transform) in q.iter_mut() {
            if ps.slot == slot {
                found = Some((bevy_e, sprite, transform));
                break;
            }
        }
        if let Some((_, mut sprite, mut transform)) = found {
            sprite.image = handle;
            transform.translation = wpos;
            sprite.color = if blink { Color::srgba(1.0, 1.0, 1.0, 0.4) } else { Color::WHITE };
        } else {
            commands.spawn((
                Sprite::from_image(handle),
                Transform::from_translation(wpos),
                PlayerSprite { slot },
            ));
        }
    }
    // Despawn dead/absent players.
    for (bevy_e, ps, _, _) in q.iter() {
        if !seen[ps.slot] {
            commands.entity(bevy_e).despawn();
        }
    }
}

// ---------------------------------------------------------------------------
// App wiring
// ---------------------------------------------------------------------------

use crate::game::sim::{PlayerInput, SlotInput};
use bevy::window::{Window, WindowPlugin, WindowResolution};

/// Build the input state from keyboard state.
fn gather_input(keys: Res<ButtonInput<KeyCode>>, mut fi: ResMut<FrameInput>) {
    // P1: WASD + Space fire + LeftShift bomb. P2: Arrows + Enter fire + RShift bomb.
    fi.p1 = SlotInput {
        up: keys.pressed(KeyCode::KeyW),
        down: keys.pressed(KeyCode::KeyS),
        left: keys.pressed(KeyCode::KeyA),
        right: keys.pressed(KeyCode::KeyD),
        fire: keys.pressed(KeyCode::Space),
        bomb: keys.pressed(KeyCode::ShiftLeft),
    };
    fi.p2 = SlotInput {
        up: keys.pressed(KeyCode::ArrowUp),
        down: keys.pressed(KeyCode::ArrowDown),
        left: keys.pressed(KeyCode::ArrowLeft),
        right: keys.pressed(KeyCode::ArrowRight),
        fire: keys.pressed(KeyCode::Enter) || keys.pressed(KeyCode::NumpadEnter),
        bomb: keys.pressed(KeyCode::ShiftRight),
    };
}

fn sim_system(
    time: Res<Time>,
    fi: Res<FrameInput>,
    mut sim: ResMut<GameSim>,
    assets: Res<GameAssets>,
) {
    let input = PlayerInput { p1: fi.p1, p2: fi.p2 };
    sim.0.advance(&*assets, &input, time.delta_secs());
}

/// Start a level: build Sim + spawn players.
pub fn start_level(
    assets: &mut GameAssets,
    level_id: ResId,
    players: usize,
    seed: u64,
) -> Option<Sim> {
    let level = assets.levels.get(&level_id)?.clone();
    let mut sim = Sim::new(level, level_id, seed);
    // 1-based level number for weapon level-availability filtering.
    let mut level_ids: Vec<ResId> = assets.levels.keys().copied().collect();
    level_ids.sort();
    sim.level_number = level_ids.iter().position(|&r| r == level_id).map(|i| i as i64 + 1).unwrap_or(1);
    sim.phase = crate::game::sim::Phase::Playing;
    for slot in 0..players.min(2) {
        sim.add_player(assets, slot, players > 1);
    }
    Some(sim)
}

/// Run the game. `assets` must already be opened + `load_all_data()` called.
pub fn run(assets: GameAssets, level_id: ResId, players: usize) {
    let mut assets = assets;
    let sim = start_level(&mut assets, level_id, players, 0xC0FFEE)
        .expect("level not found");

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(bevy::image::ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: WindowResolution::new(640, 480),
                    title: "Deimos Rising".into(),
                    ..default()
                }),
                ..default()
            }),
    );
    app.insert_resource(assets)
        .insert_resource(GameSim(sim))
        .insert_resource(TextureBook::default())
        .insert_resource(FrameInput {
            p1: SlotInput::default(),
            p2: SlotInput::default(),
            players,
            level_index: 0,
        })
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.05)));
    if let Some(engine) = crate::game::audio::AudioEngine::new() {
        app.insert_resource(engine);
    }

    app.add_systems(Startup, (setup, setup_hud))
        .add_systems(
            Update,
            (
                gather_input,
                sim_system,
                crate::game::audio::drain_events,
                sync_terrain,
                sync_entities,
                sync_players,
                hud_system,
            )
                .chain(),
        );
    app.run();
}

// ---------------------------------------------------------------------------
// HUD
// ---------------------------------------------------------------------------

/// One score-bar text line per player slot.
#[derive(Component)]
pub struct HudText {
    pub slot: usize,
}

/// Spawn the two HUD text lines (top corners).
pub fn setup_hud(mut commands: Commands) {
    for slot in 0..2 {
        let (left, right) = if slot == 0 {
            (Val::Px(6.0), Val::Auto)
        } else {
            (Val::Auto, Val::Px(6.0))
        };
        commands.spawn((
            Text::new(""),
            TextFont {
                font_size: bevy::text::FontSize::Px(15.0),
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.95, 1.0)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(4.0 + slot as f32 * 16.0),
                left,
                right,
                ..default()
            },
            HudText { slot },
        ));
    }
}

/// Refresh HUD text from sim state each frame.
pub fn hud_system(
    sim_res: Res<GameSim>,
    assets: Res<GameAssets>,
    mut q: Query<(&HudText, &mut Text)>,
) {
    let sim = &sim_res.0;
    for (hud, mut text) in q.iter_mut() {
        let slot = hud.slot;
        let Some(p) = sim.players[slot].as_ref() else {
            text.0 = String::new();
            continue;
        };
        let wname = assets
            .weapons
            .get(&p.air_weapon)
            .map(|w| w.name.trim_start_matches("Air - ").to_string())
            .unwrap_or_else(|| "---".into());
        text.0 = format!(
            "{:>7}  {:>4}  x{:<2}  L{:<2}  SH{:>3}  {}",
            p.score,
            p.money,
            p.multiplier,
            p.lives,
            p.shields as i64,
            wname,
        );
    }
}
