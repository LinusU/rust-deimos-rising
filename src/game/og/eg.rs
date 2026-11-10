//! Port of the original's eg module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `void __cdecl G_EG_ResetAtLevelStart(class U_LinkedList *)` — 0x417690, 624 bytes, decomp: compiling. 
pub fn G_EG_ResetAtLevelStart(g: &mut G, a0: u32) {
    todo!("G_EG_ResetAtLevelStart 0x417690")
}

/// `void __cdecl G_EG_CheckForMapSpawnsAtRow(int)` — 0x417900, 400 bytes, decomp: complete. 
pub fn G_EG_CheckForMapSpawnsAtRow(g: &mut G, a0: i32) {
    todo!("G_EG_CheckForMapSpawnsAtRow 0x417900")
}

/// `void __cdecl G_EG_RequestSpawn(struct G_EG_SpawnRequest *, struct G_Entity_Ref *, struct G_UnitDef *)` — 0x417a90, 1136 bytes, decomp: compiling. 
pub fn G_EG_RequestSpawn(g: &mut G, a0: u32, a1: u32, a2: u32) {
    todo!("G_EG_RequestSpawn 0x417a90")
}

/// `void __cdecl G_EG_CacheOwnerLocOffsetsAndAngles(class G_Entity *)` — 0x417f00, 800 bytes, decomp: complete. 
pub fn G_EG_CacheOwnerLocOffsetsAndAngles(g: &mut G, a0: u32) {
    todo!("G_EG_CacheOwnerLocOffsetsAndAngles 0x417f00")
}

/// `bool __cdecl G_EG_Process(int)` — 0x418220, 3648 bytes, decomp: compiling. 
pub fn G_EG_Process(g: &mut G, a0: i32) -> bool {
    todo!("G_EG_Process 0x418220")
}

/// `void __cdecl G_EG_DisposePlayersChildren(enum G_Game_PlayerNum)` — 0x4198d0, 320 bytes, decomp: complete. 
pub fn G_EG_DisposePlayersChildren(g: &mut G, a0: i32) {
    todo!("G_EG_DisposePlayersChildren 0x4198d0")
}

/// `void __cdecl G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum(int, int)` — 0x419a10, 224 bytes, decomp: complete. 
pub fn G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum(g: &mut G, a0: i32, a1: i32) {
    todo!("G_EG_ChangeStateOnWeaponPowerupReleaseByUniqueEntityNum 0x419a10")
}

/// `void __cdecl G_EG_DisposeByUniqueEntityNum(int)` — 0x419af0, 256 bytes, decomp: complete. 
pub fn G_EG_DisposeByUniqueEntityNum(g: &mut G, a0: i32) {
    todo!("G_EG_DisposeByUniqueEntityNum 0x419af0")
}

/// `bool __cdecl G_EG_RuleCondition_IsEntityTrackingPlayer(long, struct U_Utils_Vector *, int)` — 0x419bf0, 384 bytes, decomp: matching. 
pub fn G_EG_RuleCondition_IsEntityTrackingPlayer(g: &mut G, a0: i32, a1: u32, a2: i32) -> bool {
    todo!("G_EG_RuleCondition_IsEntityTrackingPlayer 0x419bf0")
}

/// `bool __cdecl G_EG_RuleCondition_IsEntityActive(long, struct U_Utils_Vector *, int)` — 0x419d70, 384 bytes, decomp: compiling. 
pub fn G_EG_RuleCondition_IsEntityActive(g: &mut G, a0: i32, a1: u32, a2: i32) -> bool {
    todo!("G_EG_RuleCondition_IsEntityActive 0x419d70")
}

/// `int __cdecl G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType(long)` — 0x419ef0, 240 bytes, decomp: complete. 
pub fn G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType(g: &mut G, a0: i32) -> i32 {
    todo!("G_EG_RuleCondition_GetNumEntitiesActive_ByEntityType 0x419ef0")
}

/// `bool __cdecl G_EG_RuleCondition_IsAnyDestroyableAirEntityActive(void)` — 0x419fe0, 224 bytes, decomp: complete. 
pub fn G_EG_RuleCondition_IsAnyDestroyableAirEntityActive(g: &mut G) -> bool {
    todo!("G_EG_RuleCondition_IsAnyDestroyableAirEntityActive 0x419fe0")
}

/// `bool __cdecl G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive(void)` — 0x41a0c0, 224 bytes, decomp: complete. 
pub fn G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive(g: &mut G) -> bool {
    todo!("G_EG_RuleCondition_IsAnyDestroyableGroundEntityActive 0x41a0c0")
}

/// `void __cdecl G_EntityGroup_sub_1a4d0(void)` — 0x41a4d0, 224 bytes, decomp: compiling. 
pub fn G_EntityGroup_sub_1a4d0(g: &mut G) {
    todo!("G_EntityGroup_sub_1a4d0 0x41a4d0")
}

/// `_sub_41a5b0` — 0x41a5b0, 528 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41a5b0(g: &mut G) {
    todo!("sub_41a5b0 0x41a5b0")
}

/// `void __cdecl G_EntityGroup_sub_1a7c0(void)` — 0x41a7c0, 208 bytes, decomp: compiling. 
pub fn G_EntityGroup_sub_1a7c0(g: &mut G) {
    todo!("G_EntityGroup_sub_1a7c0 0x41a7c0")
}

/// `_sub_41a890` — 0x41a890, 256 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41a890(g: &mut G) {
    todo!("sub_41a890 0x41a890")
}

/// `void __cdecl sub_01a990(class G_Entity *, struct G_UnitDef *, struct G_EG_SpawnRequest *, int, int *, unsigned char, int, struct G_Entity_Ref *)` — 0x41a990, 1152 bytes, decomp: compiling. 
pub fn sub_41a990(g: &mut G, a0: u32, a1: u32, a2: u32, a3: i32, a4: u32, a5: u8, a6: i32, a7: u32) {
    todo!("sub_41a990 0x41a990")
}

/// `bool __cdecl sub_01ae10(void *, void *, unsigned char, bool)` — 0x41ae10, 640 bytes, decomp: matching. 
pub fn sub_41ae10(g: &mut G, a0: u32, a1: u32, a2: u8, a3: bool) -> bool {
    todo!("sub_41ae10 0x41ae10")
}

/// `_sub_41b090` — 0x41b090, 288 bytes, decomp: matching. Signature unknown: define it from the decomp and the callers.
pub fn sub_41b090(g: &mut G) {
    todo!("sub_41b090 0x41b090")
}

/// `_sub_41b1b0` — 0x41b1b0, 288 bytes, decomp: matching. Signature unknown: define it from the decomp and the callers.
pub fn sub_41b1b0(g: &mut G) {
    todo!("sub_41b1b0 0x41b1b0")
}

/// `void __cdecl sub_01b2d0(void)` — 0x41b2d0, 768 bytes, decomp: compiling. 
pub fn sub_41b2d0(g: &mut G) {
    todo!("sub_41b2d0 0x41b2d0")
}

/// `_sub_41b5d0` — 0x41b5d0, 128 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41b5d0(g: &mut G) {
    todo!("sub_41b5d0 0x41b5d0")
}

/// `_sub_41b650` — 0x41b650, 176 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41b650(g: &mut G) {
    todo!("sub_41b650 0x41b650")
}

/// `_sub_41b700` — 0x41b700, 64 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41b700(g: &mut G) {
    todo!("sub_41b700 0x41b700")
}

/// `_sub_41b740` — 0x41b740, 224 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41b740(g: &mut G) {
    todo!("sub_41b740 0x41b740")
}

/// `_sub_41b820` — 0x41b820, 256 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41b820(g: &mut G) {
    todo!("sub_41b820 0x41b820")
}

/// `void __cdecl sub_01b920(class G_Entity *, int)` — 0x41b920, 1136 bytes, decomp: compiling. 
pub fn sub_41b920(g: &mut G, a0: u32, a1: i32) {
    todo!("sub_41b920 0x41b920")
}

/// `_sub_41bd90` — 0x41bd90, 240 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41bd90(g: &mut G) {
    todo!("sub_41bd90 0x41bd90")
}

/// `_sub_41be80` — 0x41be80, 240 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41be80(g: &mut G) {
    todo!("sub_41be80 0x41be80")
}

/// `void __cdecl sub_01bf70(class G_Entity *)` — 0x41bf70, 576 bytes, decomp: compiling. 
pub fn sub_41bf70(g: &mut G, a0: u32) {
    todo!("sub_41bf70 0x41bf70")
}

/// `_sub_41c1b0` — 0x41c1b0, 288 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41c1b0(g: &mut G) {
    todo!("sub_41c1b0 0x41c1b0")
}

/// `_sub_41c3b0` — 0x41c3b0, 400 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41c3b0(g: &mut G) {
    todo!("sub_41c3b0 0x41c3b0")
}

/// `void __cdecl sub_01c540(class G_Entity *, class G_Entity *)` — 0x41c540, 768 bytes, decomp: compiling. 
pub fn sub_41c540(g: &mut G, a0: u32, a1: u32) {
    todo!("sub_41c540 0x41c540")
}

/// `void __cdecl sub_01c840(class G_Entity *, class G_Entity *, unsigned char, int, int, int)` — 0x41c840, 896 bytes, decomp: compiling. 
pub fn sub_41c840(g: &mut G, a0: u32, a1: u32, a2: u8, a3: i32, a4: i32, a5: i32) {
    todo!("sub_41c840 0x41c840")
}

/// `void __cdecl sub_01cbc0(class G_Entity *)` — 0x41cbc0, 560 bytes, decomp: compiling. 
pub fn sub_41cbc0(g: &mut G, a0: u32) {
    todo!("sub_41cbc0 0x41cbc0")
}

/// `_sub_41cdf0` — 0x41cdf0, 288 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41cdf0(g: &mut G) {
    todo!("sub_41cdf0 0x41cdf0")
}

/// `_sub_41cf10` — 0x41cf10, 144 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41cf10(g: &mut G) {
    todo!("sub_41cf10 0x41cf10")
}

/// `_sub_41cfa0` — 0x41cfa0, 144 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41cfa0(g: &mut G) {
    todo!("sub_41cfa0 0x41cfa0")
}

/// `void __cdecl G_EntityGroup_sub_1d110(void)` — 0x41d110, 48 bytes, decomp: complete. 
pub fn G_EntityGroup_sub_1d110(g: &mut G) {
    todo!("G_EntityGroup_sub_1d110 0x41d110")
}

/// `void __cdecl G_EntityGroup_sub_1d140(void)` — 0x41d140, 144 bytes, decomp: compiling. 
pub fn G_EntityGroup_sub_1d140(g: &mut G) {
    todo!("G_EntityGroup_sub_1d140 0x41d140")
}

/// `_sub_41d1d0` — 0x41d1d0, 224 bytes, decomp: compiling. Signature unknown: define it from the decomp and the callers.
pub fn sub_41d1d0(g: &mut G) {
    todo!("sub_41d1d0 0x41d1d0")
}

/// `_sub_41d2b0` — 0x41d2b0, 48 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_41d2b0(g: &mut G) {
    todo!("sub_41d2b0 0x41d2b0")
}
