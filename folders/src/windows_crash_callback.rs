//! Register the player package's callback for this process. Windows invokes
//! it outside the game, including for fail-fast faults that bypass its filter.
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null;
use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
    System::{
        ErrorReporting::WerRegisterRuntimeExceptionModule,
        Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD},
    },
};

const KEY: PCWSTR = w!(r"Software\Microsoft\Windows\Windows Error Reporting\RuntimeExceptionHelperModules");
const CALLBACK: &str = "eu4_harmony_crash_dumps.dll";

pub fn register(folder: &Path) -> crate::Result<()> {
    let dll = folder.join(CALLBACK);
    if !dll.is_file() {
        return Err(format!("missing {}", dll.display()));
    }
    let path: Vec<_> = dll.as_os_str().encode_wide().chain([0]).collect();
    let value = 1u32;
    let status = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, path.as_ptr(), REG_DWORD, (&raw const value).cast(), 4) };
    if status != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(status as i32).to_string());
    }
    let hr = unsafe { WerRegisterRuntimeExceptionModule(path.as_ptr(), null()) };
    if hr < 0 {
        return Err(format!("WerRegisterRuntimeExceptionModule failed ({hr:#x})"));
    }
    Ok(())
}

/// Remove only this installation's callback name from the current account.
pub fn forget(folder: &Path) -> crate::Result<()> {
    let dll = std::path::absolute(folder).map_err(|e| e.to_string())?.join(CALLBACK);
    let path: Vec<_> = dll.as_os_str().encode_wide().chain([0]).collect();
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, KEY, path.as_ptr()) };
    match status {
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(()),
        _ => Err(std::io::Error::from_raw_os_error(status as i32).to_string()),
    }
}
