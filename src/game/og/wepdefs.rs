//! Port of the original's wepdefs module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `struct G_WepDef * __cdecl G_WepDef_MasterList_GetRefPtrForIndex(int)` — 0x445030, 128 bytes, decomp: complete. 
pub fn G_WepDef_MasterList_GetRefPtrForIndex(g: &mut G, a0: i32) -> u32 {
    todo!("G_WepDef_MasterList_GetRefPtrForIndex 0x445030")
}

/// `struct G_WepDef * __cdecl G_WepDef_MasterList_GetNextRefPtrByType(enum G_WepDef_Type, long, int)` — 0x4450b0, 192 bytes, decomp: matching. 
pub fn G_WepDef_MasterList_GetNextRefPtrByType(g: &mut G, a0: i32, a1: i32, a2: i32) -> u32 {
    todo!("G_WepDef_MasterList_GetNextRefPtrByType 0x4450b0")
}

/// `void __cdecl sub_0459a0(long, int)` — 0x4459a0, 144 bytes, decomp: complete. 
pub fn sub_4459a0(g: &mut G, a0: i32, a1: i32) {
    todo!("sub_4459a0 0x4459a0")
}

/// `void __cdecl sub_045a30(struct G_WepDef *)` — 0x445a30, 384 bytes, decomp: complete. 
pub fn sub_445a30(g: &mut G, a0: u32) {
    todo!("sub_445a30 0x445a30")
}
