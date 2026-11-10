//! Port of the original's gameobject module. Stubs generated from the call
//! graph; replace each `todo!` with a translation (docs/faithful-port.md).
#![allow(non_snake_case, unused_variables, unused_mut, clippy::all)]

use super::g::G;

/// `public: __thiscall G_GameObject::G_GameObject(void)` — 0x423e30, 48 bytes, decomp: complete. 
pub fn G_GameObject_G_GameObject(g: &mut G, this: u32) {
    todo!("G_GameObject_G_GameObject 0x423e30")
}

/// `public: __thiscall G_GameObject::~G_GameObject(void)` — 0x423e60, 16 bytes, decomp: complete. 
pub fn G_GameObject_dtor_G_GameObject(g: &mut G, this: u32) {
    todo!("G_GameObject_dtor_G_GameObject 0x423e60")
}

/// `public: void __thiscall G_GameObject::SetDefaults(void)` — 0x423e70, 256 bytes, decomp: complete. 
pub fn G_GameObject_SetDefaults(g: &mut G, this: u32) {
    todo!("G_GameObject_SetDefaults 0x423e70")
}

/// `public: void __thiscall G_GameObject::AdjustVisibilityAndTinting(void)` — 0x423f70, 240 bytes, decomp: matching. 
pub fn G_GameObject_AdjustVisibilityAndTinting(g: &mut G, this: u32) {
    todo!("G_GameObject_AdjustVisibilityAndTinting 0x423f70")
}

/// `public: void __thiscall G_GameObject::DoScaling(void)` — 0x424060, 112 bytes, decomp: matching. 
pub fn G_GameObject_DoScaling(g: &mut G, this: u32) {
    todo!("G_GameObject_DoScaling 0x424060")
}

/// `public: struct U_Utils_Vector * __thiscall G_GameObject::GetLoc(void)` — 0x4240d0, 16 bytes, decomp: complete. 
pub fn G_GameObject_GetLoc_4240d0(g: &mut G, this: u32) -> u32 {
    todo!("G_GameObject_GetLoc_4240d0 0x4240d0")
}

/// `public: void __thiscall G_GameObject::GetLoc(struct U_Utils_Vector *) const` — 0x4240e0, 32 bytes, decomp: complete. 
pub fn G_GameObject_GetLoc_4240e0(g: &mut G, this: u32, a0: u32) {
    todo!("G_GameObject_GetLoc_4240e0 0x4240e0")
}

/// `public: void __thiscall G_GameObject::GetLoc(float *, float *) const` — 0x424100, 32 bytes, decomp: complete. 
pub fn G_GameObject_GetLoc_424100(g: &mut G, this: u32, a0: u32, a1: u32) {
    todo!("G_GameObject_GetLoc_424100 0x424100")
}

/// `public: void __thiscall G_GameObject::SetLoc(struct U_Utils_Vector *)` — 0x424120, 32 bytes, decomp: complete. 
pub fn G_GameObject_SetLoc_424120(g: &mut G, this: u32, a0: u32) {
    todo!("G_GameObject_SetLoc_424120 0x424120")
}

/// `public: void __thiscall G_GameObject::SetLoc(float, float)` — 0x424140, 32 bytes, decomp: complete. 
pub fn G_GameObject_SetLoc_424140(g: &mut G, this: u32, a0: f32, a1: f32) {
    todo!("G_GameObject_SetLoc_424140 0x424140")
}

/// `public: void __thiscall G_GameObject::CalculateDimensions(void)` — 0x424160, 176 bytes, decomp: complete. 
pub fn G_GameObject_CalculateDimensions(g: &mut G, this: u32) {
    todo!("G_GameObject_CalculateDimensions 0x424160")
}

/// `public: void __thiscall G_GameObject::GetBounds(struct U_Rect *) const` — 0x424210, 288 bytes, decomp: complete. 
pub fn G_GameObject_GetBounds_424210(g: &mut G, this: u32, a0: u32) {
    todo!("G_GameObject_GetBounds_424210 0x424210")
}

/// `public: void __thiscall G_GameObject::GetBounds(int *, int *, int *, int *) const` — 0x424330, 304 bytes, decomp: complete. 
pub fn G_GameObject_GetBounds_424330(g: &mut G, this: u32, a0: u32, a1: u32, a2: u32, a3: u32) {
    todo!("G_GameObject_GetBounds_424330 0x424330")
}

/// `public: void __thiscall G_GameObject::GetDimensions(int *, int *) const` — 0x424460, 32 bytes, decomp: complete. 
pub fn G_GameObject_GetDimensions(g: &mut G, this: u32, a0: u32, a1: u32) {
    todo!("G_GameObject_GetDimensions 0x424460")
}

/// `public: void __thiscall G_GameObject::Glow_Start(unsigned short, int, bool)` — 0x424480, 64 bytes, decomp: complete. 
pub fn G_GameObject_Glow_Start(g: &mut G, this: u32, a0: u16, a1: i32, a2: bool) {
    todo!("G_GameObject_Glow_Start 0x424480")
}

/// `public: void __thiscall G_GameObject::Glow_Stop(void)` — 0x4244c0, 16 bytes, decomp: complete. 
pub fn G_GameObject_Glow_Stop(g: &mut G, this: u32) {
    todo!("G_GameObject_Glow_Stop 0x4244c0")
}

/// `public: void __thiscall G_GameObject::Glow_Process(void)` — 0x4244d0, 96 bytes, decomp: complete. 
pub fn G_GameObject_Glow_Process(g: &mut G, this: u32) {
    todo!("G_GameObject_Glow_Process 0x4244d0")
}

/// `public: bool __thiscall G_GameObject::MoveAndCheckPosition(int, bool)` — 0x424530, 480 bytes, decomp: compiling. 
pub fn G_GameObject_MoveAndCheckPosition(g: &mut G, this: u32, a0: i32, a1: bool) -> bool {
    todo!("G_GameObject_MoveAndCheckPosition 0x424530")
}
