use crate::{call_exe, exe_global_mut};

#[unsafe(no_mangle)]
pub extern "C" fn init() {
    call_exe!(U_Manager_Register, "Level", 1);
    *exe_global_mut!(DAT_004e7ea9) = 1;
    call_exe!(G_Level_BuildInfoList);
}
