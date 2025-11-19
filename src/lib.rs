use std::ffi::c_void;

mod exe;
mod game;
mod inject;

const DLL_PROCESS_ATTACH: u32 = 1;

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(
    _hinst_dll: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            inject::replace_exe_function(0x00429410, game::level::init as usize);
            inject::replace_exe_function(0x00462fa0, game::init::GetDirectXVersion as usize);
        }
    }

    1
}
