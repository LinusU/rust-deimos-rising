//! Port of the original's notice module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `void __cdecl G_Notice_ResetAtLevelStart(void)` — 0x42df90, 192 bytes, decomp: complete. 
pub fn G_Notice_ResetAtLevelStart(g: &mut G) {
    todo!("G_Notice_ResetAtLevelStart 0x42df90")
}

/// `void __cdecl G_Notice_Request(struct G_Notice_Settings *, int)` — 0x42e050, 304 bytes, decomp: matching. 
pub fn G_Notice_Request(g: &mut G, a0: u32, a1: i32) {
    todo!("G_Notice_Request 0x42e050")
}

/// `void __cdecl G_Notice_Process(int)` — 0x42e180, 464 bytes, decomp: complete. 
pub fn G_Notice_Process(g: &mut G, a0: i32) {
    todo!("G_Notice_Process 0x42e180")
}
