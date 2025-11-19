use std::ffi::c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleA(lpModuleName: *const u8) -> *mut c_void;
}

// Set this to the ImageBase Ghidra shows for your EXE
pub const EXE_IMAGE_BASE: usize = 0x0040_0000;

pub fn exe_base() -> usize {
    // Passing NULL gets the handle of the EXE.
    let exe_base = unsafe { GetModuleHandleA(std::ptr::null()) } as usize;
    if exe_base == 0 {
        panic!("GetModuleHandleA failed");
    }
    exe_base
}

/// Convert a *Ghidra VA* (e.g. 0x00462fa0) to a runtime pointer of type `T`.
pub unsafe fn exe_va_to<T>(va: usize) -> T {
    let base = exe_base();
    let rva = va - EXE_IMAGE_BASE;
    let real = base + rva;
    unsafe { std::mem::transmute_copy::<usize, T>(&real) }
}

/// Same but for data globals (returns a raw pointer of type `*mut T`)
pub unsafe fn exe_va_to_ptr<T>(va: usize) -> *mut T {
    unsafe { exe_va_to::<*mut T>(va) }
}

#[macro_export]
macro_rules! call_exe {
    (U_Manager_Register, $param1:expr, $param2:expr) => {{
        use std::ffi::CString;
        use std::os::raw::c_char;
        type FnTy = unsafe extern "C" fn(*const c_char, u8);

        let f: FnTy = unsafe { $crate::exe::exe_va_to::<FnTy>(0x00401de0) };
        let param1 = CString::new($param1).unwrap();
        let param2 = $param2;
        unsafe { f(param1.as_ptr(), param2) }
    }};

    (G_Level_BuildInfoList) => {{
        type FnTy = unsafe extern "C" fn();

        let f: FnTy = unsafe { $crate::exe::exe_va_to::<FnTy>(0x00429540) };
        unsafe { f() }
    }};
}

#[macro_export]
macro_rules! exe_global_mut {
    (DAT_004e7ea9) => {{
        unsafe {
            let ptr: *mut u32 = $crate::exe::exe_va_to_ptr::<u32>(0x004e7ea9);
            &mut *ptr
        }
    }};
}
