//! Audio playback via rodio: decoded `Sound` PCM → mixed SFX + music.
//!
//! `MixerDeviceSink` is the audio device; `sink.mixer()` is the root
//! `mixer::Mixer`. Each `Player::connect_new(sink.mixer())` binds a
//! transport-controlled queue to the device. The SFX bus is a custom
//! `mixer::Mixer` whose `MixerSource` output plays forever on one player,
//! so every `sfx.add(source)` overlaps freely.

use crate::assets::{GameAssets, Sound};
use crate::pak::ResId;
use bevy::prelude::*;
use rodio::buffer::SamplesBuffer;
use rodio::mixer::mixer as rodio_mixer;
use rodio::source::Zero;
use rodio::Source;
use rodio::{ChannelCount, DeviceSinkBuilder, Player, SampleRate};

const MIX_CHANNELS: u16 = 2;
const MIX_RATE: u32 = 44100;

#[derive(Resource)]
pub struct AudioEngine {
    /// Keeps the device stream alive.
    _sink: rodio::MixerDeviceSink,
    /// Player driving the SFX mixer's output (plays forever).
    _sfx_driver: Player,
    /// SFX bus input: `add()` a source per sound event (all overlap).
    sfx: rodio::mixer::Mixer,
    /// Music player bound to the device (append/stop/volume).
    music_player: Player,
    /// Currently playing music resource id.
    music_rid: Option<ResId>,
    /// SFX bus volume 0..1 (Player::set_volume on the sfx driver).
    sfx_volume: f32,
    /// Music volume 0..1.
    music_volume: f32,
}

impl AudioEngine {
    /// Open the default audio device. Returns None if unavailable.
    pub fn new() -> Option<AudioEngine> {
        let sink = DeviceSinkBuilder::open_default_sink().ok()?;
        let ch = ChannelCount::new(MIX_CHANNELS).unwrap();
        let rate = SampleRate::new(MIX_RATE).unwrap();

        // SFX bus: custom mixer → its output plays forever on a player.
        let (sfx, sfx_out) = rodio_mixer(ch, rate);
        // Keep-alive silence so the mixer never detaches while idle.
        sfx.add(Zero::new(ch, rate));
        let sfx_driver = Player::connect_new(sink.mixer());
        sfx_driver.append(sfx_out);
        sfx_driver.set_volume(0.8);

        let music_player = Player::connect_new(sink.mixer());

        Some(AudioEngine {
            _sink: sink,
            _sfx_driver: sfx_driver,
            sfx,
            music_player,
            music_rid: None,
            sfx_volume: 0.8,
            music_volume: 0.6,
        })
    }

    fn pcm_source(sound: &Sound) -> SamplesBuffer {
        let data: Vec<f32> = sound
            .samples
            .iter()
            .map(|&s| s as f32 / 32768.0)
            .collect();
        SamplesBuffer::new(
            ChannelCount::new(sound.channels.max(1) as u16).unwrap(),
            SampleRate::new(sound.sample_rate.max(1)).unwrap(),
            data,
        )
    }

    /// Play a one-shot sound effect (overlaps with others).
    pub fn play_sfx(&self, sound: &Sound) {
        self.sfx.add(Self::pcm_source(sound));
    }

    /// Play a one-shot sound at `volume` (0..1) and `pitch` (playback
    /// speed, 1.0 = as recorded).
    pub fn play_sfx_pitched(&self, sound: &Sound, volume: f32, pitch: f32) {
        let pitch = if pitch.is_finite() && pitch > 0.1 { pitch } else { 1.0 };
        self.sfx.add(Self::pcm_source(sound).speed(pitch).amplify(volume.clamp(0.0, 2.0)));
    }

    /// Switch to a looping music track.
    pub fn play_music(&mut self, sound: &Sound) {
        self.music_player.stop();
        self.music_player
            .append(Self::pcm_source(sound).repeat_infinite());
        self.music_player.set_volume(0.6);
        self.music_player.play();
    }

    /// Adjust both buses by `delta` (clamped 0..1). F3/F4 keys.
    pub fn adjust_volume(&mut self, delta: f32) {
        self.sfx_volume = (self.sfx_volume + delta).clamp(0.0, 1.0);
        self.music_volume = (self.music_volume + delta).clamp(0.0, 1.0);
        self._sfx_driver.set_volume(self.sfx_volume);
        self.music_player.set_volume(self.music_volume);
    }

    /// Ensure the level's music is playing (idempotent).
    pub fn ensure_music(&mut self, assets: &mut GameAssets, rid: ResId) {
        if self.music_rid == Some(rid) {
            return;
        }
        if let Some(sound) = assets.sound(rid) {
            // Clone the PCM so we don't hold the assets borrow.
            let pcm = Sound {
                samples: sound.samples.clone(),
                channels: sound.channels,
                sample_rate: sound.sample_rate,
            };
            self.play_music(&pcm);
            self.music_rid = Some(rid);
        }
    }
}
