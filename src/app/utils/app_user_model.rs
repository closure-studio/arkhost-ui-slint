use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use windows_sys::Win32::{
    Foundation::STATUS_SUCCESS, UI::Shell::SetCurrentProcessExplicitAppUserModelID,
};

pub fn set_to_default_id() {
    set_id(consts::DEFAULT_ID);
}

pub fn set_to_authenticator_id() {
    set_id(consts::AUTHENTICATOR_ID);
}

// TODO: 安装快捷方式到开始菜单

pub fn set_id(id: impl AsRef<OsStr>) {
    let id_utf16: Vec<u16> = id.as_ref().encode_wide().chain([0]).collect();
    match unsafe { SetCurrentProcessExplicitAppUserModelID(id_utf16.as_ptr()) } {
        STATUS_SUCCESS => println!("[AppUserModel] ID was set to {:?}", id.as_ref()),
        e => println!("[AppUserModel] Error setting ID: {e:#x}"),
    }
}

pub mod consts {
    pub const DEFAULT_ID: &str = "net.ClosureStudio.ArkHostApp.UI";
    pub const AUTHENTICATOR_ID: &str = "net.ClosureStudio.ArkHostApp.Authenticator";
}
