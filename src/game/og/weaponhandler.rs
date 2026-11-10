//! Port of the original's weaponhandler module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `public: void __thiscall G_WeaponHandler::SetUpForAppearance(bool)` — 0x446f40, 464 bytes, decomp: complete. 
pub fn G_WeaponHandler_SetUpForAppearance(g: &mut G, this: u32, a0: bool) {
    todo!("G_WeaponHandler_SetUpForAppearance 0x446f40")
}

/// `public: void __thiscall G_WeaponHandler::ChangeWeapon(enum G_WepDef_Type, struct G_WepDef *)` — 0x447110, 400 bytes, decomp: complete. 
pub fn G_WeaponHandler_ChangeWeapon(g: &mut G, this: u32, a0: i32, a1: u32) {
    todo!("G_WeaponHandler_ChangeWeapon 0x447110")
}

/// `public: enum G_WepHandler_Result __thiscall G_WeaponHandler::Process(struct U_Utils_Vector *, bool, bool, bool, int, bool *)` — 0x4472e0, 1552 bytes, decomp: compiling. 
pub fn G_WeaponHandler_Process(g: &mut G, this: u32, a0: u32, a1: bool, a2: bool, a3: bool, a4: i32, a5: u32) -> i32 {
    todo!("G_WeaponHandler_Process 0x4472e0")
}

/// `public: void __thiscall G_WeaponHandler::Crosshair_Hilite(bool)` — 0x4478f0, 80 bytes, decomp: complete. 
pub fn G_WeaponHandler_Crosshair_Hilite(g: &mut G, this: u32, a0: bool) {
    todo!("G_WeaponHandler_Crosshair_Hilite 0x4478f0")
}

/// `public: void __thiscall G_WeaponHandler::UpdateLoc(struct U_Utils_Vector *)` — 0x447940, 32 bytes, decomp: complete. 
pub fn G_WeaponHandler_UpdateLoc(g: &mut G, this: u32, a0: u32) {
    todo!("G_WeaponHandler_UpdateLoc 0x447940")
}

/// `public: float __thiscall G_WeaponHandler::AirPowerup_GetPercentage(void)` — 0x447960, 16 bytes, decomp: complete. 
pub fn G_WeaponHandler_AirPowerup_GetPercentage(g: &mut G, this: u32) -> f32 {
    todo!("G_WeaponHandler_AirPowerup_GetPercentage 0x447960")
}

/// `public: bool __thiscall G_WeaponHandler::AirWeapon_HasChanged(void)` — 0x447970, 16 bytes, decomp: complete. 
pub fn G_WeaponHandler_AirWeapon_HasChanged(g: &mut G, this: u32) -> bool {
    todo!("G_WeaponHandler_AirWeapon_HasChanged 0x447970")
}

/// `public: void __thiscall G_WeaponHandler::AirWeapon_GetScoreBarInfo(struct G_WepHandler_ScoreBarInfo *)` — 0x447980, 320 bytes, decomp: compiling. 
pub fn G_WeaponHandler_AirWeapon_GetScoreBarInfo(g: &mut G, this: u32, a0: u32) {
    todo!("G_WeaponHandler_AirWeapon_GetScoreBarInfo 0x447980")
}

/// `public: struct G_WepDef * __thiscall G_WeaponHandler::AirWeapon_GetCurrentOrQueuedWeaponRefPtr(void)` — 0x447ac0, 16 bytes, decomp: complete. 
pub fn G_WeaponHandler_AirWeapon_GetCurrentOrQueuedWeaponRefPtr(g: &mut G, this: u32) -> u32 {
    todo!("G_WeaponHandler_AirWeapon_GetCurrentOrQueuedWeaponRefPtr 0x447ac0")
}

/// `private: bool __thiscall G_WeaponHandler::Priv_CheckSpawning_Ground(int)` — 0x447c70, 240 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_CheckSpawning_Ground(g: &mut G, this: u32, a0: i32) -> bool {
    todo!("G_WeaponHandler_Priv_CheckSpawning_Ground 0x447c70")
}

/// `private: bool __thiscall G_WeaponHandler::Priv_CheckSpawning_Air(int)` — 0x447d60, 80 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_CheckSpawning_Air(g: &mut G, this: u32, a0: i32) -> bool {
    todo!("G_WeaponHandler_Priv_CheckSpawning_Air 0x447d60")
}

/// `private: bool __thiscall G_WeaponHandler::Priv_CheckSpawning_Auxilary(int)` — 0x447db0, 160 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_CheckSpawning_Auxilary(g: &mut G, this: u32, a0: i32) -> bool {
    todo!("G_WeaponHandler_Priv_CheckSpawning_Auxilary 0x447db0")
}

/// `private: void __thiscall G_WeaponHandler::Priv_AirPowerup_Process(int, struct U_Utils_Vector *, struct G_WepDef *, enum G_WepHandler_Result *)` — 0x447e50, 944 bytes, decomp: compiling. 
pub fn G_WeaponHandler_Priv_AirPowerup_Process(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32, a3: u32) {
    todo!("G_WeaponHandler_Priv_AirPowerup_Process 0x447e50")
}

/// `private: void __thiscall G_WeaponHandler::Priv_Spawn_Ground(struct U_Utils_Vector *)` — 0x448200, 592 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_Spawn_Ground(g: &mut G, this: u32, a0: u32) {
    todo!("G_WeaponHandler_Priv_Spawn_Ground 0x448200")
}

/// `private: void __thiscall G_WeaponHandler::Priv_Spawn_Air(struct U_Utils_Vector *)` — 0x448450, 320 bytes, decomp: compiling. 
pub fn G_WeaponHandler_Priv_Spawn_Air(g: &mut G, this: u32, a0: u32) {
    todo!("G_WeaponHandler_Priv_Spawn_Air 0x448450")
}

/// `private: void __thiscall G_WeaponHandler::Priv_Spawn_Auxilary(struct U_Utils_Vector *)` — 0x448590, 400 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_Spawn_Auxilary(g: &mut G, this: u32, a0: u32) {
    todo!("G_WeaponHandler_Priv_Spawn_Auxilary 0x448590")
}

/// `private: struct G_WeaponHandler::Priv_WeaponInfo * __thiscall G_WeaponHandler::Priv_WeaponInfoList_GetPtrToWeaponInfo(class U_LinkedList *, long)` — 0x4487b0, 128 bytes, decomp: complete. 
pub fn G_WeaponHandler_Priv_WeaponInfoList_GetPtrToWeaponInfo(g: &mut G, this: u32, a0: u32, a1: i32) -> u32 {
    todo!("G_WeaponHandler_Priv_WeaponInfoList_GetPtrToWeaponInfo 0x4487b0")
}

/// `_sub_448890` — 0x448890, 80 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_448890(g: &mut G) {
    todo!("sub_448890 0x448890")
}
