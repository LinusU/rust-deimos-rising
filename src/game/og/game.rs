//! Port of the original's game module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;
use super::bgnd::*;
use super::debris::*;
use super::eg::*;
use super::gameobject::*;
use super::input::*;
use super::level::*;
use super::mem::list;
use super::motionblur::*;
use super::notice::*;
use super::particle::*;
use super::player::*;
use super::resource::*;
use super::scorebar::*;
use super::umath::ftol;
use super::wepdefs::sub_4459a0;

// Game globals.
pub const PLAYING: u32 = 0x4e4824; // byte: the game loop runs while set
pub const MONEY_DONE: u32 = 0x4e4825; // byte: all money counters finished
pub const NO_PLAYER_IN_GAME: u32 = 0x4e4826; // byte
pub const ACCURACY_REWARD_PENDING: u32 = 0x4e4827; // byte
pub const ACCURACY_REWARD: u32 = 0x4e4828; // byte
pub const FINAL_LEVEL: u32 = 0x4e4829; // byte
pub const LEVELS_PLAYED: u32 = 0x4e482a; // dword
pub const CURRENT_LEVEL: u32 = 0x4e482e; // dword (1-based number in the list)
pub const LEVEL_ID: u32 = 0x4e4832;
pub const GAME_TIME: u32 = 0x4e4836;
pub const GAME_MODE: u32 = 0x4e483a;
pub const SHADOWS_VALID: u32 = 0x4e481b; // byte
pub const SHADOWS: u32 = 0x4e4846; // byte
pub const END_SEQUENCE_STARTED: u32 = 0x4e4847; // byte
pub const END_SEQUENCE_TIME: u32 = 0x4e4848;
pub const GAME_OVER_TIME: u32 = 0x4e4850;
pub const FLAG_4854: u32 = 0x4e4854; // byte
pub const WAITING_FOR_LEVEL_END: u32 = 0x4e4855; // byte
pub const ACC_TARGETS: u32 = 0x4e4856;
pub const ACC_DESTROYED: u32 = 0x4e485a;
pub const LEVELS_COMPLETED: u32 = 0x4e485e;
pub const PLAYERS: u32 = 0x4e481c; // G_Player*[2]
pub const DEFAULT_LOC: u32 = 0x4e40f0;
// End-of-level screen state.
pub const EOL_STATE: u32 = 0x4e4862;
pub const EOL_TIME: u32 = 0x4e4866;
pub const EOL_ALPHA: u32 = 0x4e486a;
pub const EOL_STEP_TIME: u32 = 0x4e486e;
pub const EOL_BONUS: u32 = 0x4e4872;
pub const EOL_BONUS_STEP: u32 = 0x4e4876;
pub const EOL_TEXT: u32 = 0x4e487a;
pub const EOL_PERCENT: u32 = 0x4e497a;
pub const EOL_LIVES_BONUS_ON: u32 = 0x4e497e; // byte
pub const EOL_LIVES_BONUS_COUNT: u32 = 0x4e4980;

/// `sub_4589c0` (the registration check): a registration number is set.
pub fn registration_is(g: &mut G) -> bool {
    g.m.ri32(0x52b688) > 0
}

/// `U_Sound_Play` (0x44f5f0): rolls the pitch, then the (never drawing)
/// volume, and hands them to the host.
pub fn U_Sound_Play(g: &mut G, settings: u32, flag: bool) {
    if g.m.r32(settings) == 0x656e6f6e {
        return;
    }
    let vol = g.m.ri32(settings + 4);
    let lo = g.m.rf32(settings + 0x10);
    let hi = g.m.rf32(settings + 0x14);
    let pitch = g.random_float(lo, hi) as f32;
    let volume = g.random_int(vol, vol);
    let id = g.m.r32(settings);
    g.host.sound_play(id, volume, pitch);
}

/// `sub_44f660`: play sound `id` at the default pitch ([0x4ffa86]).
pub fn sound_default(g: &mut G, id: i32, _priority: i32, volume: i32) {
    let pitch = g.m.rf32(0x4ffa86);
    g.host.sound_play(id as u32, volume, pitch);
}

/// `U_Utils_Safe_strncpy` (0x40f780): strncpy, then terminate at `n`.
pub fn U_Utils_Safe_strncpy(g: &mut G, dst: u32, src: u32, n: i32) -> u32 {
    if dst == 0 || src == 0 || n < 1 {
        return 0;
    }
    let bytes = g.m.cstr(src).to_vec();
    let mut i = 0u32;
    while (i as i32) < n {
        let b = if (i as usize) < bytes.len() { bytes[i as usize] } else { 0 };
        g.m.w8(dst + i, b);
        i += 1;
    }
    g.m.w8(dst + n as u32, 0);
    dst
}

enum SArg {
    I(i32),
    S(u32),
}

/// The subset of `sprintf` (0x456250) the end-of-level texts use: `%s`,
/// `%i`/`%d` and `%%`.
fn sprintf(g: &mut G, dst: u32, fmt: u32, args: &[SArg]) {
    let f = g.m.cstr(fmt).to_vec();
    let mut out: Vec<u8> = Vec::new();
    let mut ai = 0;
    let mut i = 0;
    while i < f.len() {
        if f[i] == b'%' && i + 1 < f.len() {
            i += 1;
            match f[i] {
                b'%' => out.push(b'%'),
                b's' => {
                    if let Some(SArg::S(p)) = args.get(ai) {
                        out.extend_from_slice(g.m.cstr(*p));
                    }
                    ai += 1;
                }
                b'i' | b'd' => {
                    if let Some(SArg::I(v)) = args.get(ai) {
                        out.extend_from_slice(v.to_string().as_bytes());
                    }
                    ai += 1;
                }
                _ => {}
            }
        } else {
            out.push(f[i]);
        }
        i += 1;
    }
    g.m.strcpy(dst, &out);
}

fn pf_int(g: &mut G, i: i32) -> i32 {
    ftol(G_Res_GetPermFloat(g, i) as f64)
}

fn player(g: &G, i: u32) -> u32 {
    g.m.r32(PLAYERS + i * 4)
}

/// The spawn request the level code builds for the end-of-level objects
/// (`req` copied from 0x4e47e5, then the object id and the game area's
/// size divided by [0x4e499e]).
fn request_level_object(g: &mut G, perm_index: i32) {
    let id = G_Res_GetPermObjectID(g, perm_index);
    if id as u32 == 0x656e6f6e {
        return;
    }
    let sp = g.m.sp;
    let req = g.m.alloca(0x28);
    for k in 0..8 {
        let v = g.m.r32(0x4e47e5 + k * 4);
        g.m.w32(req + k * 4, v);
    }
    let v = g.m.r32(0x4e4805);
    g.m.w32(req + 0x20, v);
    let v = g.m.r32(0x4e4809);
    g.m.w32(req + 0x24, v);
    g.m.wi32(req, id);
    let div = g.m.rf32(0x4e499e) as f64;
    let w = G_Res_GetPermFloat(g, 0x36) as f64;
    g.m.wf32(req + 4, (w / div) as f32);
    let h = G_Res_GetPermFloat(g, 0x37) as f64;
    g.m.wf32(req + 8, (h / div) as f32);
    G_EG_RequestSpawn(g, req, 0, 0);
    g.m.pop_to(sp);
}

/// Squared distance of `loc` to `r` truncated to int, through
/// `U_Math_Sqrt` (x87 double arithmetic, no float rounding in between).
fn dist_to(g: &mut G, loc: u32, r: u32) -> f32 {
    let dx = g.m.rf32(loc) as f64 - g.m.rf32(r) as f64;
    let dy = g.m.rf32(loc + 4) as f64 - g.m.rf32(r + 4) as f64;
    super::umath::sqrt(ftol(dy * dy + dx * dx))
}

/// `long __cdecl G_Game_GetLevelID(void)` — 0x41f4e0, 16 bytes, decomp: complete. 
pub fn G_Game_GetLevelID(g: &mut G) -> i32 {
    g.m.ri32(LEVEL_ID)
}

/// `int __cdecl G_Game_GetCurrentLevelNumber(void)` — 0x41f4f0, 16 bytes, decomp: complete. 
pub fn G_Game_GetCurrentLevelNumber(g: &mut G) -> i32 {
    g.m.ri32(CURRENT_LEVEL)
}

/// `int __cdecl G_Game_GameTime_Get(void)` — 0x41f500, 16 bytes, decomp: complete. 
pub fn G_Game_GameTime_Get(g: &mut G) -> i32 {
    g.m.ri32(GAME_TIME)
}

/// `bool __cdecl G_Game_IsWaitingForLevelToEnd(void)` — 0x41f510, 16 bytes, decomp: complete. 
pub fn G_Game_IsWaitingForLevelToEnd(g: &mut G) -> bool {
    g.m.rb(WAITING_FOR_LEVEL_END)
}

/// `bool __cdecl G_Game_GroundAccuracy_CheckForRewardThisLevel(void)` — 0x41f520, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_CheckForRewardThisLevel(g: &mut G) -> bool {
    g.m.rb(ACCURACY_REWARD)
}

/// `void __cdecl G_Game_GroundAccuracy_SetRewardDone(void)` — 0x41f530, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_SetRewardDone(g: &mut G) {
    g.m.w8(ACCURACY_REWARD, 0);
}

/// `void * __cdecl G_Game_Player_GetPtr(enum G_Game_PlayerNum)` — 0x41f540, 16 bytes, decomp: complete. 
pub fn G_Game_Player_GetPtr(g: &mut G, a0: i32) -> u32 {
    g.m.r32(PLAYERS.wrapping_add((a0 as u32).wrapping_mul(4)))
}

/// `bool __cdecl G_Game_Player_GetClosestActive(struct U_Utils_Vector *, struct U_Utils_Vector *, float *, enum G_Game_PlayerNum *)` — 0x41f550, 384 bytes, decomp: compiling. 
pub fn G_Game_Player_GetClosestActive_41f550(g: &mut G, a0: u32, a1: u32, a2: u32, a3: u32) -> bool {
    // a0 = reference location, a1 = out location, a2 = out distance, a3 = out player number
    let sp = g.m.sp;
    let loc = g.m.alloca(16); // loc[2]: {x, y}
    let mut min_dist: u32 = 0; // [ebp-0x30], a float's bits
    let mut dist = [0u32; 2];
    let mut num = [0i32; 2];
    let mut last_player = 0u32; // [ebp-0x2c]
    g.m.w32(a2, 0);
    let v = g.m.r32(DEFAULT_LOC);
    g.m.w32(a1, v);
    let v = g.m.r32(DEFAULT_LOC + 4);
    g.m.w32(a1 + 4, v);
    let mut found = false;
    let mut closest: i32 = -1;
    let mut i = 0u32;
    while i < 2 {
        let p = player(g, i);
        last_player = p;
        if G_Player_IsState(g, p, 4) {
            found = true;
            G_GameObject_GetLoc_4240e0(g, p, loc + i * 8);
            let d = dist_to(g, loc + i * 8, a0);
            dist[i as usize] = d.to_bits();
            num[i as usize] = G_Player_GetNumber(g, p);
            if closest == -1 {
                min_dist = dist[i as usize];
                closest = i as i32;
            } else if (d as f64) < f32::from_bits(min_dist) as f64 {
                min_dist = d.to_bits();
                closest = i as i32;
            }
        }
        i += 1;
    }
    // `mov ecx,[ebp+edi*8-0x28]` with edi == -1 reads [ebp-0x30] (the
    // minimum distance, 0) and [ebp-0x2c] (the last player pointer).
    if closest < 0 {
        g.m.w32(a1, min_dist);
        g.m.w32(a1 + 4, last_player);
    } else {
        let v = g.m.r32(loc + closest as u32 * 8);
        g.m.w32(a1, v);
        let v = g.m.r32(loc + closest as u32 * 8 + 4);
        g.m.w32(a1 + 4, v);
    }
    g.m.w32(a2, min_dist);
    if !found {
        g.m.wi32(a3, -1);
    } else {
        g.m.wi32(a3, num[closest as usize]);
    }
    g.m.pop_to(sp);
    found
}

/// `bool __cdecl G_Game_Player_GetClosestActive(struct U_Utils_Vector *, struct U_Utils_Vector *)` — 0x41f6d0, 432 bytes, decomp: compiling. 
pub fn G_Game_Player_GetClosestActive_41f6d0(g: &mut G, a0: u32, a1: u32) -> bool {
    let sp = g.m.sp;
    let loc = g.m.alloca(16);
    let v = g.m.r32(DEFAULT_LOC);
    g.m.w32(a1, v);
    let v = g.m.r32(DEFAULT_LOC + 4);
    g.m.w32(a1 + 4, v);
    let mut found = false;
    let mut multiple = false;
    let mut first = 0u32;
    let mut i = 0u32;
    while i < 2 {
        let p = player(g, i);
        if G_Player_IsState(g, p, 4) {
            if found {
                multiple = true;
            } else {
                first = p;
            }
            found = true;
        }
        i += 1;
    }
    if found {
        if !multiple {
            G_GameObject_GetLoc_4240e0(g, first, a1);
        } else {
            let mut min_dist: f32 = 0.0;
            let mut closest: i32 = -1;
            let mut i = 0u32;
            while i < 2 {
                let p = player(g, i);
                if G_Player_IsState(g, p, 4) {
                    G_GameObject_GetLoc_4240e0(g, p, loc + i * 8);
                    let d = dist_to(g, loc + i * 8, a0);
                    if closest == -1 {
                        min_dist = d;
                        closest = i as i32;
                    } else if (d as f64) < min_dist as f64 {
                        min_dist = d;
                        closest = i as i32;
                    }
                }
                i += 1;
            }
            let v = g.m.r32(loc + closest as u32 * 8);
            g.m.w32(a1, v);
            let v = g.m.r32(loc + closest as u32 * 8 + 4);
            g.m.w32(a1 + 4, v);
        }
    }
    g.m.pop_to(sp);
    found
}

/// `bool __cdecl G_Game_Player_GetActiveLoc(enum G_Game_PlayerNum, struct U_Utils_Vector *)` — 0x41f880, 96 bytes, decomp: complete. 
pub fn G_Game_Player_GetActiveLoc(g: &mut G, a0: i32, a1: u32) -> bool {
    let p = g.m.r32(PLAYERS.wrapping_add((a0 as u32).wrapping_mul(4)));
    let active = G_Player_IsState(g, p, 4);
    if active {
        G_GameObject_GetLoc_4240e0(g, p, a1);
    }
    active
}

/// `bool __cdecl G_Game_Player_IsAnyActive(void)` — 0x41f8e0, 80 bytes, decomp: complete. 
pub fn G_Game_Player_IsAnyActive(g: &mut G) -> bool {
    let mut any = false;
    let mut i = 0u32;
    while i < 2 {
        let p = player(g, i);
        if G_Player_IsState(g, p, 4) {
            any = true;
            break;
        }
        i += 1;
    }
    any
}

/// `void __cdecl G_Game_Player_AdjustScore(enum G_Game_PlayerNum, int, bool)` — 0x41f930, 64 bytes, decomp: complete. 
pub fn G_Game_Player_AdjustScore(g: &mut G, a0: i32, a1: i32, a2: bool) {
    if a0 > -1 && a0 < 2 {
        let p = player(g, a0 as u32);
        G_Player_Score_Adjust(g, p, a1, a2);
    }
}

/// `void __cdecl G_Game_GroundAccuracy_AddTarget(void)` — 0x41f970, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_AddTarget(g: &mut G) {
    let v = g.m.ri32(ACC_TARGETS).wrapping_add(1);
    g.m.wi32(ACC_TARGETS, v);
}

/// `void __cdecl G_Game_GroundAccuracy_AddDestroyed(void)` — 0x41f980, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_AddDestroyed(g: &mut G) {
    let v = g.m.ri32(ACC_DESTROYED).wrapping_add(1);
    g.m.wi32(ACC_DESTROYED, v);
}

/// `bool __cdecl G_Game_GetShadowsOption(void)` — 0x41f990, 32 bytes, decomp: complete. 
pub fn G_Game_GetShadowsOption(g: &mut G) -> bool {
    if !g.m.rb(SHADOWS_VALID) {
        true
    } else {
        g.m.rb(SHADOWS)
    }
}

/// `void __cdecl sub_41fc80(bool, int)` — 0x41fc80, 1184 bytes, decomp: compiling. 
pub fn sub_41fc80(g: &mut G, a0: bool, a1: i32) {
    // Level start. a0 = "restarting" (keeps the music), a1 = G_Level*.
    // Not ported: music, the display's buffers and painting, the unit
    // definition resource list (0x4409c0), G_Message_ResetAtLevelStart
    // (0x42ca70), the sprite render lists (0x4094c0) and the "demo ad".
    let level = a1 as u32;
    g.m.wi32(GAME_TIME, 0);
    g.m.w8(END_SEQUENCE_STARTED, 0);
    let t = g.m.r32(GAME_TIME);
    g.m.w32(END_SEQUENCE_TIME, t);
    g.m.w8(FLAG_4854, 0);
    sub_420790(g);
    sub_4207d0(g);
    sub_4208d0(g);
    g.m.w8(ACCURACY_REWARD, 0);
    if g.m.rb(ACCURACY_REWARD_PENDING) {
        g.m.w8(ACCURACY_REWARD_PENDING, 0);
        g.m.w8(ACCURACY_REWARD, 1);
    }
    let sp = g.m.sp;
    let lst = g.m.alloca(12);
    let sel: i32;
    if g.m.ri32(LEVELS_PLAYED) == 0 {
        let id = g.m.ri32(LEVEL_ID);
        sel = G_Level_Get1BasedNumForLevelID(g, id);
        G_Level_LoadByLevelListNumber(g, sel, level, lst);
    } else {
        let id = g.m.ri32(LEVEL_ID);
        let cur = G_Level_Get1BasedNumForLevelID(g, id);
        sel = cur.wrapping_add(1);
        let n = G_Level_GetNumInLevelList(g);
        if sel > n {
            g.m.w32(LEVEL_ID, 0x656e6f6e);
        } else {
            let id = G_Level_GetLevelIDFor1BasedLevelListNumber(g, sel);
            g.m.wi32(LEVEL_ID, id);
            G_Level_LoadByLevelListNumber(g, sel, level, lst);
            if !registration_is(g) {
                let demo = G_Level_GetNumLevelsInDemo(g);
                if sel > demo {
                    // G_Interface_DisplayAd(true)
                    g.m.w8(PLAYING, 0);
                    list::delete_all(&mut g.m, lst);
                    g.m.pop_to(sp);
                    return;
                }
            }
        }
    }
    if g.m.r32(LEVEL_ID) != 0x656e6f6e && !a0 && g.m.r8(0x4e4999) == 0 {
        let demo = G_Level_GetNumLevelsInDemo(g);
        if sel > demo {
            G_Level_HackTest_Fux0r(g);
            g.m.w8(0x4e4998, 1);
        }
    }
    let v = g.m.ri32(LEVELS_PLAYED).wrapping_add(1);
    g.m.wi32(LEVELS_PLAYED, v);
    if g.m.r32(LEVEL_ID) == 0x656e6f6e {
        // U_Music_Pause(false); the demo ad
        g.m.w8(PLAYING, 0);
        list::delete_all(&mut g.m, lst);
        g.m.pop_to(sp);
        return;
    }
    if g.m.r8(0x4e4998) != 0 {
        g.m.w8(PLAYING, 0);
        list::delete_all(&mut g.m, lst);
        g.m.pop_to(sp);
        return;
    }
    g.m.wi32(CURRENT_LEVEL, sel);
    // G_WepDef_LoadResourcesForLevel (0x4456b0)
    sub_4459a0(g, 0x41414550, sel);
    sub_4459a0(g, 0x47414550, sel);
    sub_4459a0(g, 0x43455053, sel);
    // (resize of the display's buffer and the music start are not ported)
    let mut i = 0u32;
    while i < 2 {
        let p = player(g, i);
        let id = g.m.ri32(LEVEL_ID);
        let time = g.m.ri32(GAME_TIME);
        G_Player_ResetAtLevelStart(g, p, id, time);
        i += 1;
    }
    G_Debris_ResetAtLevelStart(g);
    G_Particle_ResetAtLevelStart(g);
    G_Bgnd_ResetAtLevelStart(g, level);
    G_EG_ResetAtLevelStart(g, lst);
    G_Level_DisposeLevelObjects(g, lst);
    G_MotionBlur_ResetAtLevelStart(g);
    G_Input_ResetAtLevelStart(g);
    G_Notice_ResetAtLevelStart(g);
    G_Bgnd_DoInitialMapSpawns(g);
    let cur = g.m.ri32(CURRENT_LEVEL);
    request_level_object(g, cur.wrapping_add(9));
    let p0 = player(g, 0);
    let p1 = player(g, 1);
    G_ScoreBar_ResetAtLevelStart(g, p0, p1);
    list::delete_all(&mut g.m, lst);
    g.m.pop_to(sp);
}

/// `void __cdecl sub_020280(unsigned char *, unsigned char, unsigned char *, class G_Film *)` — 0x420280, 1216 bytes, decomp: compiling. 
pub fn sub_420280(g: &mut G, a0: u32, a1: u8, a2: u32, a3: u32) {
    // a0 = first-active flag, a1 = pause flag, a2 = "game over spawned"
    // flag, a3 = G_Film*.
    G_Input_Clear(g);
    // 0x411620 (the console is active) is treated as never active.
    G_Input_CachePlayerInputs(g);
    if g.m.r8(a0) == 0 {
        let p0 = player(g, 0);
        if G_Player_IsState(g, p0, 4) {
            g.m.w8(a0, 1);
        }
    }
    let t = g.m.ri32(GAME_TIME);
    G_Notice_Process(g, t);
    G_Debris_Process(g);
    G_Particle_Process(g);
    G_MotionBlur_Process(g);
    let mut i = 0u32;
    while i < 2 {
        let p = player(g, i);
        let first = g.m.r8(a0) != 0;
        let time = g.m.ri32(GAME_TIME);
        let mode = g.m.ri32(GAME_MODE);
        let waiting = g.m.rb(WAITING_FOR_LEVEL_END);
        G_Player_Process(g, p, first, a1 != 0, time, a3, mode, waiting);
        i += 1;
    }
    let p0 = player(g, 0);
    let p1 = player(g, 1);
    G_ScoreBar_Process(g, p0, p1);
    let mut any_in_game = false;
    let mut i = 0u32;
    while i < 2 {
        let p = player(g, i);
        if G_Player_InGame(g, p) {
            any_in_game = true;
            break;
        }
        i += 1;
    }
    if !any_in_game {
        g.m.w8(NO_PLAYER_IN_GAME, 1);
        if g.m.r8(a2) == 0 {
            request_level_object(g, 0x18);
            g.m.w8(a2, 1);
            let t = g.m.r32(GAME_TIME);
            g.m.w32(GAME_OVER_TIME, t);
        } else {
            let d = pf_int(g, 0xd);
            if g.m.ri32(GAME_TIME) > d.wrapping_add(g.m.ri32(GAME_OVER_TIME)) {
                g.m.w8(PLAYING, 0);
            }
        }
    }
    if G_Bgnd_Process(g) == 1 {
        g.m.w8(WAITING_FOR_LEVEL_END, 1);
        if g.m.r8(END_SEQUENCE_STARTED) == 0 {
            if g.m.r8(NO_PLAYER_IN_GAME) == 0 {
                let n = G_Level_GetNumInLevelList(g);
                let n2 = G_Level_GetNumInLevelList(g);
                if g.m.ri32(CURRENT_LEVEL) == n2 && g.m.ri32(LEVELS_PLAYED) == n {
                    g.m.w8(FINAL_LEVEL, 1);
                }
                if g.m.r8(FINAL_LEVEL) == 0 {
                    request_level_object(g, 0x16);
                } else {
                    request_level_object(g, 0x17);
                }
                g.m.w8(END_SEQUENCE_STARTED, 1);
                let t = g.m.r32(GAME_TIME);
                g.m.w32(END_SEQUENCE_TIME, t);
                let mut i = 0u32;
                while i < 2 {
                    let p = player(g, i);
                    if G_Player_InGame(g, p) {
                        G_Player_SetInvulnerability(g, p, true, false);
                    }
                    i += 1;
                }
                let t = g.m.ri32(GAME_TIME);
                sub_420930(g, t as u32);
            } else {
                g.m.w8(END_SEQUENCE_STARTED, 1);
            }
        } else {
            let t = g.m.ri32(GAME_TIME);
            if sub_420d90(g, t as u32) {
                let mut all = true;
                let mut started = false;
                let mut i = 0u32;
                while i < 2 {
                    let p = player(g, i);
                    if G_Player_InGame(g, p) && !G_Player_MoneyCounter_IsActive(g, p) {
                        let t = g.m.ri32(GAME_TIME);
                        started = G_Player_MoneyCounter_Start(g, p, t, started);
                        all = false;
                    }
                    i += 1;
                }
                let mut i = 0u32;
                while i < 2 {
                    let p = player(g, i);
                    if G_Player_InGame(g, p) {
                        let t = g.m.ri32(GAME_TIME);
                        if !G_Player_MoneyCounter_Process(g, p, t) {
                            all = false;
                        }
                    }
                    i += 1;
                }
                if all {
                    g.m.w8(MONEY_DONE, 1);
                }
            }
        }
    }
    let t = g.m.ri32(GAME_TIME);
    if G_EG_Process(g, t) {
        G_Bgnd_StopVerticalScrolling(g);
    } else {
        G_Bgnd_ResumeVerticalScrolling(g);
    }
}

/// `_sub_420790` — 0x420790, 64 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_420790(g: &mut G) {
    for k in 0..5 {
        g.m.w32(0x4e4984 + k * 4, 0);
    }
}

/// `_sub_4207d0` — 0x4207d0, 32 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_4207d0(g: &mut G) {
    g.m.w32(ACC_TARGETS, 0);
    g.m.w32(ACC_DESTROYED, 0);
}

/// `_sub_4208d0` — 0x4208d0, 96 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_4208d0(g: &mut G) {
    g.m.w32(EOL_STATE, 0);
    g.m.w32(EOL_TIME, 0);
    g.m.w32(EOL_ALPHA, 0x20);
    g.m.w32(EOL_STEP_TIME, 0);
    g.m.w32(EOL_BONUS, 0);
    g.m.w32(EOL_BONUS_STEP, 0);
    g.m.w8(EOL_TEXT, 0);
    g.m.w32(EOL_PERCENT, 0);
    g.m.w8(EOL_LIVES_BONUS_ON, 0);
    g.m.w32(EOL_LIVES_BONUS_COUNT, 0);
}

/// `void __cdecl sub_020930(unsigned int)` — 0x420930, 1120 bytes, decomp: compiling. 
pub fn sub_420930(g: &mut G, a0: u32) {
    // Start of the end-of-level score tally.
    g.m.w32(EOL_STATE, 1);
    g.m.w32(EOL_TIME, a0);
    g.m.w32(EOL_STEP_TIME, a0);
    let targets = g.m.ri32(ACC_TARGETS);
    let destroyed = g.m.ri32(ACC_DESTROYED);
    let hundred = g.m.rf64(0x4e49a8);
    let pct: f32 = if targets > 0 {
        ((destroyed as f64 / targets as f64) * hundred) as f32
    } else {
        0.0
    };
    let pct_i = ftol(pct as f64);
    g.m.wi32(EOL_PERCENT, pct_i);
    let cutoff = pf_int(g, 0xbc);
    let p = pct as f64;
    // fcomp: proceeds when neither unordered nor below.
    let ge = |t: f64| t <= p;
    if p >= hundred {
        g.m.w8(ACCURACY_REWARD_PENDING, 1);
        let v = pf_int(g, 0xbd);
        g.m.wi32(EOL_BONUS, v);
    } else if ge(100i32.wrapping_sub(cutoff) as f64) {
        let v = pf_int(g, 0xbe);
        g.m.wi32(EOL_BONUS, v);
    } else if ge(100i32.wrapping_sub(cutoff.wrapping_mul(2)) as f64) {
        let v = pf_int(g, 0xbf);
        g.m.wi32(EOL_BONUS, v);
    } else if ge(100i32.wrapping_sub(cutoff.wrapping_mul(3)) as f64) {
        let v = pf_int(g, 0xc0);
        g.m.wi32(EOL_BONUS, v);
    } else if ge(100i32.wrapping_sub(cutoff.wrapping_mul(4)) as f64) {
        let v = pf_int(g, 0xc1);
        g.m.wi32(EOL_BONUS, v);
    } else {
        let v = pf_int(g, 0xc2);
        g.m.wi32(EOL_BONUS, v);
    }
    let bonus = g.m.ri32(EOL_BONUS).wrapping_mul(g.m.ri32(CURRENT_LEVEL));
    g.m.wi32(EOL_BONUS, bonus);
    if bonus > 0 {
        let mut product = bonus as f32;
        product = (G_Res_GetPermFloat(g, 0xc8) as f64 * product as f64) as f32;
        let cap = pf_int(g, 0xc7);
        // fild cap; fcomp product; test ah,0x45: cap > product
        if (cap as f64) > product as f64 {
            product = cap as f32;
        }
        g.m.wi32(EOL_BONUS_STEP, ftol(product as f64));
    } else {
        g.m.w32(EOL_BONUS_STEP, 0);
    }
    let s = G_Res_GetPermGameString(g, 0xb);
    let bytes = g.m.cstr(s).to_vec();
    g.m.strcpy(EOL_TEXT, &bytes);
    g.m.w8(EOL_LIVES_BONUS_ON, 0);
    g.m.w32(EOL_LIVES_BONUS_COUNT, 0);
}

/// `bool __cdecl sub_020d90(unsigned int)` — 0x420d90, 2464 bytes, decomp: compiling. 
pub fn sub_420d90(g: &mut G, a0: u32) -> bool {
    // One step of the end-of-level tally state machine; true when it is
    // finished. The texts go to the buffer at 0x4e487a.
    let time = a0 as i32;
    let mut done = false;
    let state = g.m.r32(EOL_STATE);
    match state {
        0 => done = true,
        1 => {
            let mut d = pf_int(g, 0xc3);
            if g.m.rb(FINAL_LEVEL) {
                d = pf_int(g, 0xc4);
            }
            if time > g.m.ri32(EOL_TIME).wrapping_add(d) {
                g.m.w32(EOL_STATE, 2);
                g.m.wi32(EOL_TIME, time);
                let id = G_Res_GetPermSoundID(g, 0x14);
                sound_default(g, id, 0x32, 0x64);
            }
        }
        2 => {
            let n = pf_int(g, 0xcb);
            let mut k = 0;
            while k < n {
                if g.m.r32(EOL_ALPHA) != 0 {
                    let v = g.m.r32(EOL_ALPHA).wrapping_sub(1);
                    g.m.w32(EOL_ALPHA, v);
                }
                k += 1;
            }
            let d = pf_int(g, 0xc5);
            if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                g.m.w32(EOL_STATE, 3);
                g.m.wi32(EOL_TIME, time);
                let id = G_Res_GetPermSoundID(g, 0x14);
                sound_default(g, id, 0x32, 0x64);
                let s = G_Res_GetPermGameString(g, 0xb);
                let pct = g.m.ri32(EOL_PERCENT);
                sprintf(g, EOL_TEXT, 0x4e4dc1, &[SArg::S(s), SArg::I(pct)]);
            }
        }
        3 => {
            let d = pf_int(g, 0xc5);
            if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                g.m.w32(EOL_STATE, 4);
                g.m.wi32(EOL_TIME, time);
                let s12 = G_Res_GetPermGameString(g, 0xc);
                let s11 = G_Res_GetPermGameString(g, 0xb);
                let pct = g.m.ri32(EOL_PERCENT);
                sprintf(g, EOL_TEXT, 0x4e4dc8, &[SArg::S(s11), SArg::I(pct), SArg::S(s12)]);
                let id = G_Res_GetPermSoundID(g, 0x14);
                sound_default(g, id, 0x32, 0x64);
            }
        }
        4 => {
            let d = pf_int(g, 0xc5);
            if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                g.m.w32(EOL_STATE, 5);
                g.m.wi32(EOL_TIME, time);
                if g.m.ri32(EOL_PERCENT) == 0x64 {
                    let v = g.m.ri32(LEVELS_COMPLETED).wrapping_add(1);
                    g.m.wi32(LEVELS_COMPLETED, v);
                    let id = G_Res_GetPermSoundID(g, 0x16);
                    sound_default(g, id, 0x32, 0x64);
                } else if g.m.ri32(EOL_BONUS) == 0 {
                    let id = G_Res_GetPermSoundID(g, 0x15);
                    sound_default(g, id, 0x32, 0x64);
                } else {
                    let id = G_Res_GetPermSoundID(g, 0x14);
                    sound_default(g, id, 0x32, 0x64);
                }
            }
        }
        5 => {
            let bonus = g.m.ri32(EOL_BONUS);
            let pct = g.m.ri32(EOL_PERCENT);
            if bonus == 0 {
                let s13 = G_Res_GetPermGameString(g, 0xd);
                let s12 = G_Res_GetPermGameString(g, 0xc);
                let s11 = G_Res_GetPermGameString(g, 0xb);
                sprintf(
                    g,
                    EOL_TEXT,
                    0x4e4dd1,
                    &[SArg::S(s11), SArg::I(pct), SArg::S(s12), SArg::S(s13)],
                );
            } else {
                let s12 = G_Res_GetPermGameString(g, 0xc);
                let s11 = G_Res_GetPermGameString(g, 0xb);
                sprintf(
                    g,
                    EOL_TEXT,
                    0x4e4ddc,
                    &[SArg::S(s11), SArg::I(pct), SArg::S(s12), SArg::I(bonus)],
                );
            }
            let d = pf_int(g, 0xc5);
            if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                g.m.w32(EOL_STATE, 6);
                g.m.wi32(EOL_TIME, time);
            }
        }
        6 => {
            if g.m.ri32(EOL_BONUS) > 0 {
                let d = pf_int(g, 0xc6);
                if time > d.wrapping_add(g.m.ri32(EOL_STEP_TIME)) {
                    g.m.wi32(EOL_STEP_TIME, time);
                    let step = g.m.ri32(EOL_BONUS_STEP);
                    let b = g.m.ri32(EOL_BONUS).wrapping_sub(step);
                    g.m.wi32(EOL_BONUS, b);
                    if b < 0 {
                        g.m.w32(EOL_BONUS, 0);
                    }
                    let mut i = 0i32;
                    while i < 2 {
                        let amount = g.m.ri32(EOL_BONUS_STEP);
                        if i > -1 && i < 2 {
                            let p = player(g, i as u32);
                            G_Player_Score_Adjust(g, p, amount, false);
                        }
                        i += 1;
                    }
                    let id = G_Res_GetPermSoundID(g, 0x14);
                    sound_default(g, id, 0x32, 0x64);
                    let bonus = g.m.ri32(EOL_BONUS);
                    let pct = g.m.ri32(EOL_PERCENT);
                    let s12 = G_Res_GetPermGameString(g, 0xc);
                    let s11 = G_Res_GetPermGameString(g, 0xb);
                    sprintf(
                        g,
                        EOL_TEXT,
                        0x4e4ddc,
                        &[SArg::S(s11), SArg::I(pct), SArg::S(s12), SArg::I(bonus)],
                    );
                }
            } else {
                g.m.w32(EOL_STATE, 7);
                g.m.wi32(EOL_TIME, time);
            }
        }
        7 => {
            let d = pf_int(g, 0xc9);
            if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                g.m.w32(EOL_STATE, 8);
                g.m.wi32(EOL_TIME, time);
            }
        }
        8 => {
            if g.m.r32(EOL_ALPHA) < 0x20 {
                let a = pf_int(g, 0xcc);
                let v = g.m.r32(EOL_ALPHA).wrapping_add(a as u32);
                g.m.w32(EOL_ALPHA, v);
                if v > 0x20 {
                    g.m.w32(EOL_ALPHA, 0x20);
                }
            }
            let d = pf_int(g, 0xca);
            if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                let n = G_Level_GetNumInLevelList(g);
                if g.m.ri32(LEVELS_COMPLETED) != n {
                    done = true;
                } else {
                    g.m.w8(EOL_LIVES_BONUS_ON, 1);
                    g.m.w32(EOL_LIVES_BONUS_COUNT, 0);
                    g.m.w32(EOL_STATE, 9);
                    g.m.wi32(EOL_TIME, time);
                    g.m.w32(EOL_ALPHA, 0);
                    let s = G_Res_GetPermGameString(g, 0xa);
                    let bytes = g.m.cstr(s).to_vec();
                    g.m.strcpy(EOL_TEXT, &bytes);
                    let id = G_Res_GetPermSoundID(g, 0x17);
                    sound_default(g, id, 0x64, 0x64);
                }
            }
        }
        9 => {
            let d = pf_int(g, 0xce);
            if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                g.m.w32(EOL_STATE, 0xa);
                g.m.wi32(EOL_TIME, time);
                g.m.w32(EOL_ALPHA, 0x20);
                g.m.w8(EOL_TEXT, 0);
            }
        }
        10 => {
            if g.m.r8(EOL_LIVES_BONUS_ON) == 0 {
                done = true;
            } else {
                let d = pf_int(g, 0xcf);
                if time > d.wrapping_add(g.m.ri32(EOL_TIME)) {
                    let c = g.m.ri32(EOL_LIVES_BONUS_COUNT).wrapping_add(1);
                    g.m.wi32(EOL_LIVES_BONUS_COUNT, c);
                    let mut i = 0i32;
                    while i < 2 {
                        let amount = pf_int(g, 0xcd);
                        if i > -1 && i < 2 {
                            let p = player(g, i as u32);
                            G_Player_Score_Adjust(g, p, amount, true);
                        }
                        i += 1;
                    }
                    let n = pf_int(g, 0xd0);
                    if g.m.ri32(EOL_LIVES_BONUS_COUNT) >= n {
                        g.m.w8(EOL_LIVES_BONUS_ON, 0);
                    } else {
                        g.m.w32(EOL_STATE, 9);
                        g.m.wi32(EOL_TIME, time);
                        g.m.w32(EOL_ALPHA, 0);
                        let s = G_Res_GetPermGameString(g, 0xa);
                        let bytes = g.m.cstr(s).to_vec();
                        g.m.strcpy(EOL_TEXT, &bytes);
                    }
                }
            }
        }
        _ => {}
    }
    done
}
