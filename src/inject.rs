use std::ffi::c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleA(lpModuleName: *const u8) -> *mut c_void;
    fn VirtualProtect(
        lpAddress: *mut c_void,
        dwSize: usize,
        flNewProtect: u32,
        lpflOldProtect: *mut u32,
    ) -> i32;
}

const PAGE_EXECUTE_READWRITE: u32 = 0x40;

// Set this to whatever Ghidra shows as the ImageBase of the EXE.
// For most old games it's 0x00400000.
const EXE_IMAGE_BASE: usize = 0x00400000;

/// Patch a function in the main EXE to jump to `new_fn`.
///
/// `orig_va` is the *virtual address* from Ghidra (e.g. 0x00462fa0).
/// `new_fn` is the function pointer (e.g. `_GetDirectXVersion as usize`).
pub unsafe fn replace_exe_function(orig_va: usize, new_fn: usize) {
    // 1. Get the base address of the main module in this process.
    //    Passing NULL gets the handle of the EXE.
    let exe_base = unsafe { GetModuleHandleA(std::ptr::null()) } as usize;
    if exe_base == 0 {
        panic!("GetModuleHandleA failed");
    }

    // 2. Convert the Ghidra VA to an RVA, then to the real runtime address.
    //
    //    Ghidra: VA = EXE_IMAGE_BASE + RVA
    //    Runtime: real_addr = exe_base + RVA
    let rva = orig_va.saturating_sub(EXE_IMAGE_BASE);
    let target = exe_base + rva;

    // 3. Make the page writable/executable so we can modify the code.
    let mut old_protect: u32 = 0;
    let ok = unsafe {
        VirtualProtect(
            target as *mut c_void,
            5, // we will write a 5-byte JMP
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        )
    };
    if ok == 0 {
        panic!("VirtualProtect failed");
    }

    // 4. Build a relative JMP from target -> new_fn
    //    JMP rel32: E9 <rel32>, where:
    //    rel32 = new_fn - (target + 5)
    let rel = (new_fn as isize) - (target as isize) - 5;
    let patch: [u8; 5] = [
        0xE9,
        (rel & 0xFF) as u8,
        ((rel >> 8) & 0xFF) as u8,
        ((rel >> 16) & 0xFF) as u8,
        ((rel >> 24) & 0xFF) as u8,
    ];

    // 5. Overwrite the first 5 bytes of the original function
    unsafe {
        std::ptr::copy_nonoverlapping(patch.as_ptr(), target as *mut u8, patch.len());
    }

    // 6. Restore original protection (nice but optional)
    let mut _dummy: u32 = 0;
    unsafe {
        VirtualProtect(target as *mut c_void, 5, old_protect, &mut _dummy);
    }
}
