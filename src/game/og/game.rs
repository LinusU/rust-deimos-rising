//! Port of the original's game module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `long __cdecl G_Game_GetLevelID(void)` — 0x41f4e0, 16 bytes, decomp: complete. 
pub fn G_Game_GetLevelID(g: &mut G) -> i32 {
    todo!("G_Game_GetLevelID 0x41f4e0")
}

/// `int __cdecl G_Game_GetCurrentLevelNumber(void)` — 0x41f4f0, 16 bytes, decomp: complete. 
pub fn G_Game_GetCurrentLevelNumber(g: &mut G) -> i32 {
    todo!("G_Game_GetCurrentLevelNumber 0x41f4f0")
}

/// `int __cdecl G_Game_GameTime_Get(void)` — 0x41f500, 16 bytes, decomp: complete. 
pub fn G_Game_GameTime_Get(g: &mut G) -> i32 {
    todo!("G_Game_GameTime_Get 0x41f500")
}

/// `bool __cdecl G_Game_IsWaitingForLevelToEnd(void)` — 0x41f510, 16 bytes, decomp: complete. 
pub fn G_Game_IsWaitingForLevelToEnd(g: &mut G) -> bool {
    todo!("G_Game_IsWaitingForLevelToEnd 0x41f510")
}

/// `bool __cdecl G_Game_GroundAccuracy_CheckForRewardThisLevel(void)` — 0x41f520, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_CheckForRewardThisLevel(g: &mut G) -> bool {
    todo!("G_Game_GroundAccuracy_CheckForRewardThisLevel 0x41f520")
}

/// `void __cdecl G_Game_GroundAccuracy_SetRewardDone(void)` — 0x41f530, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_SetRewardDone(g: &mut G) {
    todo!("G_Game_GroundAccuracy_SetRewardDone 0x41f530")
}

/// `void * __cdecl G_Game_Player_GetPtr(enum G_Game_PlayerNum)` — 0x41f540, 16 bytes, decomp: complete. 
pub fn G_Game_Player_GetPtr(g: &mut G, a0: i32) -> u32 {
    todo!("G_Game_Player_GetPtr 0x41f540")
}

/// `bool __cdecl G_Game_Player_GetClosestActive(struct U_Utils_Vector *, struct U_Utils_Vector *, float *, enum G_Game_PlayerNum *)` — 0x41f550, 384 bytes, decomp: compiling. 
pub fn G_Game_Player_GetClosestActive_41f550(g: &mut G, a0: u32, a1: u32, a2: u32, a3: u32) -> bool {
    todo!("G_Game_Player_GetClosestActive_41f550 0x41f550")
}

/// `bool __cdecl G_Game_Player_GetClosestActive(struct U_Utils_Vector *, struct U_Utils_Vector *)` — 0x41f6d0, 432 bytes, decomp: compiling. 
pub fn G_Game_Player_GetClosestActive_41f6d0(g: &mut G, a0: u32, a1: u32) -> bool {
    todo!("G_Game_Player_GetClosestActive_41f6d0 0x41f6d0")
}

/// `bool __cdecl G_Game_Player_GetActiveLoc(enum G_Game_PlayerNum, struct U_Utils_Vector *)` — 0x41f880, 96 bytes, decomp: complete. 
pub fn G_Game_Player_GetActiveLoc(g: &mut G, a0: i32, a1: u32) -> bool {
    todo!("G_Game_Player_GetActiveLoc 0x41f880")
}

/// `bool __cdecl G_Game_Player_IsAnyActive(void)` — 0x41f8e0, 80 bytes, decomp: complete. 
pub fn G_Game_Player_IsAnyActive(g: &mut G) -> bool {
    todo!("G_Game_Player_IsAnyActive 0x41f8e0")
}

/// `void __cdecl G_Game_Player_AdjustScore(enum G_Game_PlayerNum, int, bool)` — 0x41f930, 64 bytes, decomp: complete. 
pub fn G_Game_Player_AdjustScore(g: &mut G, a0: i32, a1: i32, a2: bool) {
    todo!("G_Game_Player_AdjustScore 0x41f930")
}

/// `void __cdecl G_Game_GroundAccuracy_AddTarget(void)` — 0x41f970, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_AddTarget(g: &mut G) {
    todo!("G_Game_GroundAccuracy_AddTarget 0x41f970")
}

/// `void __cdecl G_Game_GroundAccuracy_AddDestroyed(void)` — 0x41f980, 16 bytes, decomp: complete. 
pub fn G_Game_GroundAccuracy_AddDestroyed(g: &mut G) {
    todo!("G_Game_GroundAccuracy_AddDestroyed 0x41f980")
}

/// `bool __cdecl G_Game_GetShadowsOption(void)` — 0x41f990, 32 bytes, decomp: complete. 
pub fn G_Game_GetShadowsOption(g: &mut G) -> bool {
    todo!("G_Game_GetShadowsOption 0x41f990")
}

/// `void __cdecl sub_41fc80(bool, int)` — 0x41fc80, 1184 bytes, decomp: compiling. 
pub fn sub_41fc80(g: &mut G, a0: bool, a1: i32) {
    todo!("sub_41fc80 0x41fc80")
}

/// `void __cdecl sub_020280(unsigned char *, unsigned char, unsigned char *, class G_Film *)` — 0x420280, 1216 bytes, decomp: compiling. 
pub fn sub_420280(g: &mut G, a0: u32, a1: u8, a2: u32, a3: u32) {
    todo!("sub_420280 0x420280")
}

/// `_sub_420790` — 0x420790, 64 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_420790(g: &mut G) {
    todo!("sub_420790 0x420790")
}

/// `_sub_4207d0` — 0x4207d0, 32 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_4207d0(g: &mut G) {
    todo!("sub_4207d0 0x4207d0")
}

/// `_sub_4208d0` — 0x4208d0, 96 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_4208d0(g: &mut G) {
    todo!("sub_4208d0 0x4208d0")
}

/// `void __cdecl sub_020930(unsigned int)` — 0x420930, 1120 bytes, decomp: compiling. 
pub fn sub_420930(g: &mut G, a0: u32) {
    todo!("sub_420930 0x420930")
}

/// `bool __cdecl sub_020d90(unsigned int)` — 0x420d90, 2464 bytes, decomp: compiling. 
pub fn sub_420d90(g: &mut G, a0: u32) -> bool {
    todo!("sub_420d90 0x420d90")
}
