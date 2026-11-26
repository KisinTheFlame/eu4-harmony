//! Windows Error Reporting's per-application LocalDumps rule for eu4.exe.
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, HWND, WAIT_OBJECT_0};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegGetValueW, RegOpenKeyExW, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_WOW64_64KEY, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY,
};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

const GLOBAL: &str = r"SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps";
fn game_key() -> String {
    format!(r"{GLOBAL}\eu4.exe")
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub(crate) fn configured() -> io::Result<bool> {
    let mut key = null_mut();
    let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide(&game_key()).as_ptr(), 0, KEY_QUERY_VALUE | KEY_WOW64_64KEY, &mut key) };
    match status {
        ERROR_SUCCESS => {
            unsafe { RegCloseKey(key) };
            Ok(true)
        }
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(false),
        code => Err(io::Error::from_raw_os_error(code as i32)),
    }
}

/// RegGetValue expands REG_EXPAND_SZ for the player, even if a different
/// administrator approved the request. WER prefers the per-game value.
pub(super) fn dump_folder() -> io::Result<Option<PathBuf>> {
    read_folder(HKEY_LOCAL_MACHINE, [&game_key(), GLOBAL])
}

fn read_folder(hive: windows_sys::Win32::System::Registry::HKEY, keys: [&str; 2]) -> io::Result<Option<PathBuf>> {
    for subkey in keys {
        let mut buffer = vec![0u16; 32768];
        let mut bytes = (buffer.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                hive,
                wide(subkey).as_ptr(),
                w!("DumpFolder"),
                RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
                null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        match status {
            ERROR_SUCCESS => {
                let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
                let path = PathBuf::from(OsString::from_wide(&buffer[..length]));
                return if path.is_absolute() { Ok(Some(path)) } else { Err(io::Error::new(io::ErrorKind::InvalidData, "DumpFolder is not absolute")) };
            }
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => continue,
            code => return Err(io::Error::from_raw_os_error(code as i32)),
        }
    }
    Ok(None)
}

fn system_directory() -> io::Result<Vec<u16>> {
    let mut path = vec![0u16; 32768];
    let length = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    if length == 0 || length >= path.len() {
        return Err(io::Error::last_os_error());
    }
    path.truncate(length + 1);
    Ok(path)
}

fn add_folder_arguments(key: &str) -> Vec<u16> {
    wide(&format!(r#"add "{key}" /v DumpFolder /t REG_EXPAND_SZ /d "%LOCALAPPDATA%\CrashDumps\eu4-harmony-wer" /f /reg:64"#))
}

/// Only the system's reg.exe runs elevated, from its own directory. The
/// verb, key, value and arguments are fixed at the call site.
fn run_reg(window: HWND, verb: PCWSTR, parameters: &[u16]) -> io::Result<()> {
    let hr = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };
    if hr < 0 {
        return Err(io::Error::other(format!("CoInitializeEx failed ({hr:#x})")));
    }
    let result = launch_reg(window, verb, parameters);
    unsafe { CoUninitialize() };
    result
}

fn launch_reg(window: HWND, verb: PCWSTR, parameters: &[u16]) -> io::Result<()> {
    let directory = system_directory()?;
    let mut executable = directory[..directory.len() - 1].to_vec();
    executable.extend(wide(r"\reg.exe"));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        hwnd: window,
        lpVerb: verb,
        lpFile: executable.as_ptr(),
        lpDirectory: directory.as_ptr(),
        lpParameters: parameters.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.hProcess.is_null() {
        return Err(io::Error::other("Windows did not return the reg.exe process"));
    }
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
    let mut code = 0;
    if unsafe { WaitForSingleObject(process.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0
        || unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if code != 0 {
        return Err(io::Error::other(format!("reg.exe exited with code {code}")));
    }
    Ok(())
}

pub(crate) fn enable() -> io::Result<()> {
    if configured()? {
        return Ok(());
    }
    run_reg(crate::base::game_window().unwrap_or(null_mut()), w!("runas"), &add_folder_arguments(&format!(r"HKLM\{}", game_key())))?;
    let expected = eu4_harmony_folders::windows_dump_root().map(|p| p.join("eu4-harmony-wer"));
    if expected.is_none() || dump_folder()? != expected || !configured()? {
        return Err(io::Error::other("Windows did not save the requested EU4 dump folder"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Registry::{RegDeleteKeyW, HKEY_CURRENT_USER};

    #[test]
    fn the_system_tool_stores_an_expandable_per_user_folder() {
        let base = format!(r"Software\eu4-harmony-test-wer-{}", std::process::id());
        let game = format!(r"{base}\eu4.exe");
        let expected = eu4_harmony_folders::windows_dump_root().unwrap().join("eu4-harmony-wer");

        struct Scratch(String);
        impl Drop for Scratch {
            fn drop(&mut self) {
                unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.0).as_ptr()) };
            }
        }
        let _base = Scratch(base.clone());
        let _game = Scratch(game.clone());
        assert_eq!(read_folder(HKEY_CURRENT_USER, [&game, &base]).unwrap(), None);
        run_reg(null_mut(), w!("open"), &add_folder_arguments(&format!(r"HKCU\{game}"))).unwrap();
        assert_eq!(read_folder(HKEY_CURRENT_USER, [&game, &base]).unwrap(), Some(expected));
    }
}
