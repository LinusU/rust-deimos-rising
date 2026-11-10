//! Port of the original's player module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `_sub_4305a0` — 0x4305a0, 368 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_4305a0(g: &mut G) {
    todo!("sub_4305a0 0x4305a0")
}

/// `public: void __thiscall G_Player::ResetAtLevelStart(long, int)` — 0x430f40, 352 bytes, decomp: complete. 
pub fn G_Player_ResetAtLevelStart(g: &mut G, this: u32, a0: i32, a1: i32) {
    todo!("G_Player_ResetAtLevelStart 0x430f40")
}

/// `private: void __thiscall G_Player::Priv_ResetPosition(void)` — 0x4310a0, 144 bytes, decomp: complete. 
pub fn G_Player_Priv_ResetPosition(g: &mut G, this: u32) {
    todo!("G_Player_Priv_ResetPosition 0x4310a0")
}

/// `public: bool __thiscall G_Player::InGame(void)` — 0x431130, 16 bytes, decomp: complete. 
pub fn G_Player_InGame(g: &mut G, this: u32) -> bool {
    todo!("G_Player_InGame 0x431130")
}

/// `public: bool __thiscall G_Player::IsAboutToBeRemovedFromGame(void)` — 0x431140, 32 bytes, decomp: complete. 
pub fn G_Player_IsAboutToBeRemovedFromGame(g: &mut G, this: u32) -> bool {
    todo!("G_Player_IsAboutToBeRemovedFromGame 0x431140")
}

/// `public: enum G_Player_StateFlag __thiscall G_Player::GetState(void)` — 0x431160, 16 bytes, decomp: complete. 
pub fn G_Player_GetState(g: &mut G, this: u32) -> i32 {
    todo!("G_Player_GetState 0x431160")
}

/// `public: bool __thiscall G_Player::IsState(enum G_Player_StateFlag)` — 0x431170, 32 bytes, decomp: complete. 
pub fn G_Player_IsState(g: &mut G, this: u32, a0: i32) -> bool {
    todo!("G_Player_IsState 0x431170")
}

/// `public: void __thiscall G_Player::SetState(enum G_Player_StateFlag, int)` — 0x431190, 32 bytes, decomp: complete. 
pub fn G_Player_SetState(g: &mut G, this: u32, a0: i32, a1: i32) {
    todo!("G_Player_SetState 0x431190")
}

/// `public: enum G_Game_PlayerNum __thiscall G_Player::GetNumber(void)` — 0x4311b0, 16 bytes, decomp: complete. 
pub fn G_Player_GetNumber(g: &mut G, this: u32) -> i32 {
    todo!("G_Player_GetNumber 0x4311b0")
}

/// `public: struct G_PlayerDef * __thiscall G_Player::GetPlayerDefinitionRefPtr(void)` — 0x4311c0, 16 bytes, decomp: complete. 
pub fn G_Player_GetPlayerDefinitionRefPtr(g: &mut G, this: u32) -> u32 {
    todo!("G_Player_GetPlayerDefinitionRefPtr 0x4311c0")
}

/// `public: int __thiscall G_Player::Lives_Get(void)` — 0x431270, 16 bytes, decomp: complete. 
pub fn G_Player_Lives_Get(g: &mut G, this: u32) -> i32 {
    todo!("G_Player_Lives_Get 0x431270")
}

/// `public: void __thiscall G_Player::Lives_Add(bool)` — 0x4312a0, 272 bytes, decomp: compiling. 
pub fn G_Player_Lives_Add(g: &mut G, this: u32, a0: bool) {
    todo!("G_Player_Lives_Add 0x4312a0")
}

/// `public: void __thiscall G_Player::PowerupOverload_Reset(void)` — 0x4313b0, 96 bytes, decomp: complete. 
pub fn G_Player_PowerupOverload_Reset(g: &mut G, this: u32) {
    todo!("G_Player_PowerupOverload_Reset 0x4313b0")
}

/// `public: void __thiscall G_Player::PowerupOverload_Process(int)` — 0x431410, 560 bytes, decomp: complete. 
pub fn G_Player_PowerupOverload_Process(g: &mut G, this: u32, a0: i32) {
    todo!("G_Player_PowerupOverload_Process 0x431410")
}

/// `public: void __thiscall G_Player::Hit(float, int)` — 0x431640, 720 bytes, decomp: matching. 
pub fn G_Player_Hit(g: &mut G, this: u32, a0: f32, a1: i32) {
    todo!("G_Player_Hit 0x431640")
}

/// `public: void __thiscall G_Player::Shields_Reset(bool)` — 0x431910, 112 bytes, decomp: complete. 
pub fn G_Player_Shields_Reset(g: &mut G, this: u32, a0: bool) {
    todo!("G_Player_Shields_Reset 0x431910")
}

/// `public: void __thiscall G_Player::Shields_IncreasePercentage(float)` — 0x431980, 160 bytes, decomp: complete. 
pub fn G_Player_Shields_IncreasePercentage(g: &mut G, this: u32, a0: f32) {
    todo!("G_Player_Shields_IncreasePercentage 0x431980")
}

/// `public: float __thiscall G_Player::Shields_GetPercentage(void)` — 0x431a20, 16 bytes, decomp: complete. 
pub fn G_Player_Shields_GetPercentage(g: &mut G, this: u32) -> f32 {
    todo!("G_Player_Shields_GetPercentage 0x431a20")
}

/// `public: void __thiscall G_Player::Shields_SetPercentage(float)` — 0x431a30, 32 bytes, decomp: complete. 
pub fn G_Player_Shields_SetPercentage(g: &mut G, this: u32, a0: f32) {
    todo!("G_Player_Shields_SetPercentage 0x431a30")
}

/// `public: void __thiscall G_Player::Money_Reset(void)` — 0x431a50, 32 bytes, decomp: complete. 
pub fn G_Player_Money_Reset(g: &mut G, this: u32) {
    todo!("G_Player_Money_Reset 0x431a50")
}

/// `public: void __thiscall G_Player::Money_Change(int)` — 0x431a70, 64 bytes, decomp: complete. 
pub fn G_Player_Money_Change(g: &mut G, this: u32, a0: i32) {
    todo!("G_Player_Money_Change 0x431a70")
}

/// `public: int __thiscall G_Player::Money_Get(void)` — 0x431ab0, 16 bytes, decomp: complete. 
pub fn G_Player_Money_Get(g: &mut G, this: u32) -> i32 {
    todo!("G_Player_Money_Get 0x431ab0")
}

/// `public: void __thiscall G_Player::Money_Set(int)` — 0x431ac0, 32 bytes, decomp: complete. 
pub fn G_Player_Money_Set(g: &mut G, this: u32, a0: i32) {
    todo!("G_Player_Money_Set 0x431ac0")
}

/// `public: void __thiscall G_Player::MoneyCounter_Reset(void)` — 0x431ae0, 112 bytes, decomp: complete. 
pub fn G_Player_MoneyCounter_Reset(g: &mut G, this: u32) {
    todo!("G_Player_MoneyCounter_Reset 0x431ae0")
}

/// `public: bool __thiscall G_Player::MoneyCounter_Start(int, bool)` — 0x431b50, 848 bytes, decomp: matching. 
pub fn G_Player_MoneyCounter_Start(g: &mut G, this: u32, a0: i32, a1: bool) -> bool {
    todo!("G_Player_MoneyCounter_Start 0x431b50")
}

/// `public: bool __thiscall G_Player::MoneyCounter_Process(int)` — 0x431ea0, 1520 bytes, decomp: compiling. 
pub fn G_Player_MoneyCounter_Process(g: &mut G, this: u32, a0: i32) -> bool {
    todo!("G_Player_MoneyCounter_Process 0x431ea0")
}

/// `public: bool __thiscall G_Player::MoneyCounter_IsActive(void)` — 0x432490, 16 bytes, decomp: complete. 
pub fn G_Player_MoneyCounter_IsActive(g: &mut G, this: u32) -> bool {
    todo!("G_Player_MoneyCounter_IsActive 0x432490")
}

/// `public: bool __thiscall G_Player::IsInvulnerable(void)` — 0x4324a0, 16 bytes, decomp: complete. 
pub fn G_Player_IsInvulnerable(g: &mut G, this: u32) -> bool {
    todo!("G_Player_IsInvulnerable 0x4324a0")
}

/// `public: void __thiscall G_Player::SetInvulnerability(bool, bool)` — 0x4324b0, 80 bytes, decomp: complete. 
pub fn G_Player_SetInvulnerability(g: &mut G, this: u32, a0: bool, a1: bool) {
    todo!("G_Player_SetInvulnerability 0x4324b0")
}

/// `public: void __thiscall G_Player::Destroy(int)` — 0x432500, 800 bytes, decomp: complete. 
pub fn G_Player_Destroy(g: &mut G, this: u32, a0: i32) {
    todo!("G_Player_Destroy 0x432500")
}

/// `public: void __thiscall G_Player::Process(bool, bool, int, class G_Film *, enum G_Game_Mode, bool)` — 0x432820, 9712 bytes, decomp: compiling. 
pub fn G_Player_Process(g: &mut G, this: u32, a0: bool, a1: bool, a2: i32, a3: u32, a4: i32, a5: bool) {
    todo!("G_Player_Process 0x432820")
}

/// `public: int __thiscall G_Player::Score_Get(void)` — 0x434f10, 16 bytes, decomp: complete. 
pub fn G_Player_Score_Get(g: &mut G, this: u32) -> i32 {
    todo!("G_Player_Score_Get 0x434f10")
}

/// `public: void __thiscall G_Player::Score_Adjust(int, bool)` — 0x434f40, 336 bytes, decomp: compiling. 
pub fn G_Player_Score_Adjust(g: &mut G, this: u32, a0: i32, a1: bool) {
    todo!("G_Player_Score_Adjust 0x434f40")
}

/// `public: void __thiscall G_Player::Multiplier_Advance(void)` — 0x435090, 112 bytes, decomp: complete. 
pub fn G_Player_Multiplier_Advance(g: &mut G, this: u32) {
    todo!("G_Player_Multiplier_Advance 0x435090")
}

/// `public: enum G_Player_MultiplierValue __thiscall G_Player::Multiplier_Get(void)` — 0x435100, 16 bytes, decomp: complete. 
pub fn G_Player_Multiplier_Get(g: &mut G, this: u32) -> i32 {
    todo!("G_Player_Multiplier_Get 0x435100")
}

/// `private: void __thiscall G_Player::Priv_Appear(int)` — 0x4351d0, 512 bytes, decomp: compiling. 
pub fn G_Player_Priv_Appear(g: &mut G, this: u32, a0: i32) {
    todo!("G_Player_Priv_Appear 0x4351d0")
}

/// `private: void __thiscall G_Player::Priv_ResetSpriteInfo(void)` — 0x4353d0, 64 bytes, decomp: complete. 
pub fn G_Player_Priv_ResetSpriteInfo(g: &mut G, this: u32) {
    todo!("G_Player_Priv_ResetSpriteInfo 0x4353d0")
}

/// `private: void __thiscall G_Player::Priv_SetSpriteFaceAccordingToAirWeapon(void)` — 0x435410, 80 bytes, decomp: complete. 
pub fn G_Player_Priv_SetSpriteFaceAccordingToAirWeapon(g: &mut G, this: u32) {
    todo!("G_Player_Priv_SetSpriteFaceAccordingToAirWeapon 0x435410")
}

/// `private: void __thiscall G_Player::Priv_Multiplier_Reset(void)` — 0x435460, 16 bytes, decomp: complete. 
pub fn G_Player_Priv_Multiplier_Reset(g: &mut G, this: u32) {
    todo!("G_Player_Priv_Multiplier_Reset 0x435460")
}

/// `private: void __thiscall G_Player::Priv_Multiplier_SpawnForCurrentMultiplier(void)` — 0x435470, 304 bytes, decomp: compiling. 
pub fn G_Player_Priv_Multiplier_SpawnForCurrentMultiplier(g: &mut G, this: u32) {
    todo!("G_Player_Priv_Multiplier_SpawnForCurrentMultiplier 0x435470")
}

/// `private: void __thiscall G_Player::Priv_ProcessState(int, bool, bool)` — 0x4355a0, 560 bytes, decomp: compiling. 
pub fn G_Player_Priv_ProcessState(g: &mut G, this: u32, a0: i32, a1: bool, a2: bool) {
    todo!("G_Player_Priv_ProcessState 0x4355a0")
}

/// `private: void __thiscall G_Player::Priv_GetInputs(class G_Film *, enum G_Game_Mode)` — 0x4357d0, 192 bytes, decomp: compiling. 
pub fn G_Player_Priv_GetInputs(g: &mut G, this: u32, a0: u32, a1: i32) {
    todo!("G_Player_Priv_GetInputs 0x4357d0")
}
