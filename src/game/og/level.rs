//! Port of the original's level module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `int __cdecl G_Level_GetNumLevelsInDemo(void)` — 0x429480, 16 bytes, decomp: complete. 
pub fn G_Level_GetNumLevelsInDemo(g: &mut G) -> i32 {
    todo!("G_Level_GetNumLevelsInDemo 0x429480")
}

/// `void __cdecl G_Level_HackTest_Fux0r(void)` — 0x429490, 16 bytes, decomp: complete. 
pub fn G_Level_HackTest_Fux0r(g: &mut G) {
    todo!("G_Level_HackTest_Fux0r 0x429490")
}

/// `int __cdecl G_Level_GetNumInLevelList(void)` — 0x429790, 64 bytes, decomp: complete. 
pub fn G_Level_GetNumInLevelList(g: &mut G) -> i32 {
    todo!("G_Level_GetNumInLevelList 0x429790")
}

/// `int __cdecl G_Level_Get1BasedNumForLevelID(long)` — 0x4297d0, 160 bytes, decomp: complete. 
pub fn G_Level_Get1BasedNumForLevelID(g: &mut G, a0: i32) -> i32 {
    todo!("G_Level_Get1BasedNumForLevelID 0x4297d0")
}

/// `long __cdecl G_Level_GetLevelIDFor1BasedLevelListNumber(int)` — 0x429870, 160 bytes, decomp: complete. 
pub fn G_Level_GetLevelIDFor1BasedLevelListNumber(g: &mut G, a0: i32) -> i32 {
    todo!("G_Level_GetLevelIDFor1BasedLevelListNumber 0x429870")
}

/// `void __cdecl G_Level_LoadByLevelListNumber(int, struct G_Level *, class U_LinkedList *)` — 0x429910, 240 bytes, decomp: complete. 
pub fn G_Level_LoadByLevelListNumber(g: &mut G, a0: i32, a1: u32, a2: u32) {
    todo!("G_Level_LoadByLevelListNumber 0x429910")
}

/// `void __cdecl G_Level_LoadByID(long, struct G_Level *, class U_LinkedList *)` — 0x429a00, 48 bytes, decomp: complete. 
pub fn G_Level_LoadByID(g: &mut G, a0: i32, a1: u32, a2: u32) {
    todo!("G_Level_LoadByID 0x429a00")
}

/// `void __cdecl G_Level_DisposeLevelObjects(class U_LinkedList *)` — 0x429a30, 64 bytes, decomp: complete. 
pub fn G_Level_DisposeLevelObjects(g: &mut G, a0: u32) {
    todo!("G_Level_DisposeLevelObjects 0x429a30")
}

/// `_sub_429ae0` — 0x429ae0, 144 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_429ae0(g: &mut G) {
    todo!("sub_429ae0 0x429ae0")
}

/// `_sub_429b70` — 0x429b70, 864 bytes, decomp: complete. Signature unknown: define it from the decomp and the callers.
pub fn sub_429b70(g: &mut G) {
    todo!("sub_429b70 0x429b70")
}
