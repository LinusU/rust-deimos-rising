//! Port of the original's entity module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `public: __thiscall G_Entity::~G_Entity(void)` — 0x4130e0, 32 bytes, decomp: complete. 
pub fn G_Entity_dtor_G_Entity(g: &mut G, this: u32) {
    todo!("G_Entity_dtor_G_Entity 0x4130e0")
}

/// `public: void __thiscall G_Entity::Reset(void)` — 0x413100, 560 bytes, decomp: complete. 
pub fn G_Entity_Reset(g: &mut G, this: u32) {
    todo!("G_Entity_Reset 0x413100")
}

/// `public: void __thiscall G_Entity::SetUnitRef(struct G_UnitDef *)` — 0x413330, 464 bytes, decomp: matching. 
pub fn G_Entity_SetUnitRef(g: &mut G, this: u32, a0: u32) {
    todo!("G_Entity_SetUnitRef 0x413330")
}

/// `public: struct G_UnitDef_FileData_State * __thiscall G_Entity::GetCurrentStatePtr(void)` — 0x413500, 32 bytes, decomp: complete. 
pub fn G_Entity_GetCurrentStatePtr(g: &mut G, this: u32) -> u32 {
    todo!("G_Entity_GetCurrentStatePtr 0x413500")
}

/// `public: void __thiscall G_Entity::ChangeToWeaponPowerupReleaseState(int)` — 0x413520, 128 bytes, decomp: complete. 
pub fn G_Entity_ChangeToWeaponPowerupReleaseState(g: &mut G, this: u32, a0: i32) {
    todo!("G_Entity_ChangeToWeaponPowerupReleaseState 0x413520")
}

/// `public: void __thiscall G_Entity::ChangeState(bool, char const *, int, bool *, bool *)` — 0x4135a0, 2032 bytes, decomp: compiling. 
pub fn G_Entity_ChangeState(g: &mut G, this: u32, a0: bool, a1: u32, a2: i32, a3: u32, a4: u32) {
    todo!("G_Entity_ChangeState 0x4135a0")
}

/// `public: float __thiscall G_Entity::Hit(float, enum G_Game_PlayerNum, int)` — 0x413d90, 1008 bytes, decomp: compiling. 
pub fn G_Entity_Hit(g: &mut G, this: u32, a0: f32, a1: i32, a2: i32) -> f32 {
    todo!("G_Entity_Hit 0x413d90")
}

/// `public: void __thiscall G_Entity::DoMovementAI(int, bool *, bool *)` — 0x414180, 784 bytes, decomp: complete. 
pub fn G_Entity_DoMovementAI(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32) {
    todo!("G_Entity_DoMovementAI 0x414180")
}

/// `public: void __thiscall G_Entity::ProcessRules(int, bool *, bool *)` — 0x414490, 1120 bytes, decomp: compiling. 
pub fn G_Entity_ProcessRules(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32) {
    todo!("G_Entity_ProcessRules 0x414490")
}

/// `public: void __thiscall G_Entity::Animate(int)` — 0x4148f0, 496 bytes, decomp: compiling. 
pub fn G_Entity_Animate(g: &mut G, this: u32, a0: i32) {
    todo!("G_Entity_Animate 0x4148f0")
}

/// `public: void __thiscall G_Entity::SpawnControl(int)` — 0x414ae0, 1456 bytes, decomp: compiling. 
pub fn G_Entity_SpawnControl(g: &mut G, this: u32, a0: i32) {
    todo!("G_Entity_SpawnControl 0x414ae0")
}

/// `public: int __thiscall G_Entity::GetAngleFromSpriteInfo(void)` — 0x415090, 144 bytes, decomp: complete. 
pub fn G_Entity_GetAngleFromSpriteInfo(g: &mut G, this: u32) -> i32 {
    todo!("G_Entity_GetAngleFromSpriteInfo 0x415090")
}

/// `public: int __thiscall G_Entity::GetFrameForAngle(int)` — 0x415120, 224 bytes, decomp: compiling. 
pub fn G_Entity_GetFrameForAngle(g: &mut G, this: u32, a0: i32) -> i32 {
    todo!("G_Entity_GetFrameForAngle 0x415120")
}

/// `public: void __thiscall G_Entity::Destroy(enum G_Game_PlayerNum, int)` — 0x415200, 1872 bytes, decomp: matching. 
pub fn G_Entity_Destroy(g: &mut G, this: u32, a0: i32, a1: i32) {
    todo!("G_Entity_Destroy 0x415200")
}

/// `public: bool __thiscall G_Entity::CanSpawnOnMedia(void)` — 0x415950, 832 bytes, decomp: compiling. 
pub fn G_Entity_CanSpawnOnMedia(g: &mut G, this: u32) -> bool {
    todo!("G_Entity_CanSpawnOnMedia 0x415950")
}

/// `public: bool __thiscall G_Entity::IsWithinGameArea(void)` — 0x415c90, 144 bytes, decomp: complete. 
pub fn G_Entity_IsWithinGameArea(g: &mut G, this: u32) -> bool {
    todo!("G_Entity_IsWithinGameArea 0x415c90")
}

/// `private: void __thiscall G_Entity::Priv_MoveToTargetLoc(void)` — 0x415d20, 256 bytes, decomp: complete. 
pub fn G_Entity_Priv_MoveToTargetLoc(g: &mut G, this: u32) {
    todo!("G_Entity_Priv_MoveToTargetLoc 0x415d20")
}

/// `private: void __thiscall G_Entity::Priv_ConstrainInGameArea(void)` — 0x415e20, 416 bytes, decomp: complete. 
pub fn G_Entity_Priv_ConstrainInGameArea(g: &mut G, this: u32) {
    todo!("G_Entity_Priv_ConstrainInGameArea 0x415e20")
}

/// `private: void __thiscall G_Entity::Priv_DoCyclicMotion(void)` — 0x415fc0, 352 bytes, decomp: compiling. 
pub fn G_Entity_Priv_DoCyclicMotion(g: &mut G, this: u32) {
    todo!("G_Entity_Priv_DoCyclicMotion 0x415fc0")
}

/// `private: bool __thiscall G_Entity::Priv_DoRotationAsRequired(int)` — 0x416120, 320 bytes, decomp: complete. 
pub fn G_Entity_Priv_DoRotationAsRequired(g: &mut G, this: u32, a0: i32) -> bool {
    todo!("G_Entity_Priv_DoRotationAsRequired 0x416120")
}

/// `private: bool __thiscall G_Entity::Priv_RotateToTargetLoc(int)` — 0x416260, 704 bytes, decomp: compiling. 
pub fn G_Entity_Priv_RotateToTargetLoc(g: &mut G, this: u32, a0: i32) -> bool {
    todo!("G_Entity_Priv_RotateToTargetLoc 0x416260")
}

/// `private: void __thiscall G_Entity::Priv_Flee(enum G_UnitDef_FleeID)` — 0x416520, 1328 bytes, decomp: compiling. 
pub fn G_Entity_Priv_Flee(g: &mut G, this: u32, a0: i32) {
    todo!("G_Entity_Priv_Flee 0x416520")
}

/// `private: void __thiscall G_Entity::Priv_AdjustToRequiredVelocity(void)` — 0x416a50, 448 bytes, decomp: complete. 
pub fn G_Entity_Priv_AdjustToRequiredVelocity(g: &mut G, this: u32) {
    todo!("G_Entity_Priv_AdjustToRequiredVelocity 0x416a50")
}

/// `private: void __thiscall G_Entity::Priv_HoldToTarget(struct U_Utils_Vector *)` — 0x416c10, 224 bytes, decomp: complete. 
pub fn G_Entity_Priv_HoldToTarget(g: &mut G, this: u32, a0: u32) {
    todo!("G_Entity_Priv_HoldToTarget 0x416c10")
}

/// `private: void __thiscall G_Entity::Priv_ReverseFromTarget(struct U_Utils_Vector *, float)` — 0x416cf0, 128 bytes, decomp: complete. 
pub fn G_Entity_Priv_ReverseFromTarget(g: &mut G, this: u32, a0: u32, a1: f32) {
    todo!("G_Entity_Priv_ReverseFromTarget 0x416cf0")
}

/// `private: void __thiscall G_Entity::Priv_CheckSpawningAbilityAtStateChange(int)` — 0x416d70, 352 bytes, decomp: complete. 
pub fn G_Entity_Priv_CheckSpawningAbilityAtStateChange(g: &mut G, this: u32, a0: i32) {
    todo!("G_Entity_Priv_CheckSpawningAbilityAtStateChange 0x416d70")
}

/// `private: void __thiscall G_Entity::Priv_DeleteSpawnInfoList(class U_LinkedList *)` — 0x416ed0, 96 bytes, decomp: complete. 
pub fn G_Entity_Priv_DeleteSpawnInfoList(g: &mut G, this: u32, a0: u32) {
    todo!("G_Entity_Priv_DeleteSpawnInfoList 0x416ed0")
}

/// `private: void __thiscall G_Entity::Priv_ChangeStateOnShieldDepletion(int)` — 0x416f30, 128 bytes, decomp: complete. 
pub fn G_Entity_Priv_ChangeStateOnShieldDepletion(g: &mut G, this: u32, a0: i32) {
    todo!("G_Entity_Priv_ChangeStateOnShieldDepletion 0x416f30")
}

/// `private: bool __thiscall G_Entity::Priv_CheckWithinRangeOfPlayers(int, struct U_Utils_Vector *, enum G_Game_PlayerNum *)` — 0x416fb0, 112 bytes, decomp: complete. 
pub fn G_Entity_Priv_CheckWithinRangeOfPlayers(g: &mut G, this: u32, a0: i32, a1: u32, a2: u32) -> bool {
    todo!("G_Entity_Priv_CheckWithinRangeOfPlayers 0x416fb0")
}
