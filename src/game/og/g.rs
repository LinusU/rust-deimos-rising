//! The port's execution context: memory, the MSL `rand` state and the
//! host hooks for what the original got from QuickTime/DirectX (sprite
//! sizes, sound playback, film input).

use super::mem::Mem;

/// What the game logic needs from the outside world.
pub trait Host {
    /// Width and height of frame `frame` of sprite group `id`, as
    /// `U_Sprite` reports them; None when the group or frame is missing.
    fn sprite_frame_size(&self, id: u32, frame: i32) -> Option<(i32, i32)>;
    /// Number of frames in sprite group `id`.
    fn sprite_num_frames(&self, id: u32) -> i32;
    /// Whether an image resource exists (`G_Res_CanBeLoaded`).
    fn has_image(&self, id: u32) -> bool;
    /// Width and height of image `id`.
    fn image_size(&self, _id: u32) -> Option<(i32, i32)> {
        None
    }
    /// 16-bit pixels of image `id` (for the media mask).
    fn image_pixels16(&self, _id: u32) -> Option<(i32, i32, Vec<u16>)> {
        None
    }
    /// A sound starts (`U_Sound_Play`); pitch already rolled.
    fn sound_play(&mut self, _id: u32, _volume: i32, _pitch: f32) {}
    /// `U_Sound_IsPlaying`.
    fn sound_is_playing(&self, _id: u32) -> bool {
        false
    }
    /// The text of pak entry (`tag`, `id`) as the parsers see it:
    /// decrypted (`U_Utils_String_Encrypt`), without a terminator.
    fn read_text(&self, _tag: u32, _id: u32) -> Option<Vec<u8>> {
        None
    }
    /// Ids of every entry with `tag`, in the order the original's tag
    /// index enumerates them (`U_Pak_GetPtrToNextTagData(index, tag)`:
    /// pak files by name, entries in zip order).
    fn ids_of_tag(&self, _tag: u32) -> Vec<u32> {
        Vec::new()
    }
    /// `U_Pak_Tag_IsInIndex(tag, id)`.
    fn has_tag(&self, _tag: u32, _id: u32) -> bool {
        false
    }
}

/// A host with no assets, for tests.
pub struct NullHost;
impl Host for NullHost {
    fn sprite_frame_size(&self, _id: u32, _frame: i32) -> Option<(i32, i32)> {
        None
    }
    fn sprite_num_frames(&self, _id: u32) -> i32 {
        0
    }
    fn has_image(&self, _id: u32) -> bool {
        false
    }
}

pub struct G {
    pub m: Mem,
    /// MSL `rand` state (per-thread data in the original).
    pub rand_state: u32,
    pub host: Box<dyn Host>,
    /// Optional trace for the film oracle: `R` lines for rand() draws
    /// (new state, caller) plus whatever `trace` records.
    pub rand_log: Option<Vec<String>>,
    /// Live play: the inputs `G_Input_CachePlayerInputs` hands to the game,
    /// two `G_Input_PlayerInputs` of 7 bytes ([0]=up [1]=right [2]=down
    /// [3]=left [4]=ground weapon [5]=air weapon [6]=cycle). Zero during
    /// film replays, where the film supplies the inputs.
    pub live_inputs: [u8; 14],
    /// Live play: `U_Sprite_RenderLists_Add` appends here (see view.rs)
    /// while it is `Some`; film replays leave it `None` and draw nothing.
    pub draw_sink: Option<Vec<super::view::DrawItem>>,
}

impl G {
    pub fn new(m: Mem, host: Box<dyn Host>) -> G {
        G { m, rand_state: 1, host, rand_log: None, live_inputs: [0; 14], draw_sink: None }
    }

    /// Record an oracle trace line (only while tracing).
    pub fn trace(&mut self, f: impl FnOnce(&Self) -> String) {
        if self.rand_log.is_some() {
            let line = f(self);
            self.rand_log.as_mut().unwrap().push(line);
        }
    }

    /// `srand` (0x461620).
    pub fn srand(&mut self, seed: u32) {
        self.rand_state = seed;
    }

    /// `rand` (0x4615e0).
    #[track_caller]
    pub fn rand(&mut self) -> i32 {
        self.rand_state = self.rand_state.wrapping_mul(0x41c64e6d).wrapping_add(0x3039);
        if let Some(log) = self.rand_log.as_mut() {
            let l = std::panic::Location::caller();
            let f = l.file().rsplit('/').next().unwrap_or("");
            log.push(format!("R {:08x} {}:{}", self.rand_state, f, l.line()));
        }
        ((self.rand_state >> 16) & 0x7fff) as i32
    }

    /// `U_Utils_RandomInt` (0x40f7e0): equal bounds return without a draw.
    #[track_caller]
    pub fn random_int(&mut self, min: i32, max: i32) -> i32 {
        if min == max {
            return min;
        }
        let r = self.rand();
        min.wrapping_add(r % (max.wrapping_sub(min).wrapping_add(1)))
    }

    /// `U_Utils_RandomFloat` (0x40f840): equal bounds return without a
    /// draw; otherwise `rand() * (high - low) / 32767 + min(low, high)`.
    /// The result is left on the x87 stack at double precision; callers
    /// round it when they store it.
    #[track_caller]
    pub fn random_float(&mut self, low: f32, high: f32) -> f64 {
        if low == high {
            return low as f64;
        }
        // fcomp low,high; "low > high" picks high as the base.
        let base = if low > high { high } else { low };
        let r = self.rand();
        r as f64 * (high as f64 - low as f64) / 32767.0 + base as f64
    }
}
