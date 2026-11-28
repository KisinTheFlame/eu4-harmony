//! Windows Error Reporting's per-application LocalDumps rule for eu4.exe.
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
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

/// Ownership state of the Windows LocalDumps rule for `eu4.exe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Configuration {
    /// No per-application `eu4.exe` rule exists.
    NotConfigured,
    /// Configured with Harmony's dedicated `eu4-harmony-wer` folder.
    Harmony,
    /// Configured externally (custom folder or non-Harmony rule); never deleted by Harmony.
    External,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub(crate) fn harmony_wer_folder() -> Option<PathBuf> {
    eu4_harmony_folders::windows_dump_root().map(|p| p.join("eu4-harmony-wer"))
}

fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('/', "\\");
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s).trim_end_matches('\\');
        s.to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

pub(crate) fn status() -> io::Result<Configuration> {
    let mut key = null_mut();
    let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide(&game_key()).as_ptr(), 0, KEY_QUERY_VALUE | KEY_WOW64_64KEY, &mut key) };
    match status {
        ERROR_SUCCESS => {
            unsafe { RegCloseKey(key) };
            let own = read_folder_single(HKEY_LOCAL_MACHINE, &game_key())?;
            let expected = harmony_wer_folder();
            match (own, expected) {
                (Some(folder), Some(expected)) if same_folder(&folder, &expected) => Ok(Configuration::Harmony),
                _ => Ok(Configuration::External),
            }
        }
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(Configuration::NotConfigured),
        code => Err(io::Error::from_raw_os_error(code as i32)),
    }
}

/// RegGetValue expands REG_EXPAND_SZ for the player, even if a different
/// administrator approved the request. WER prefers the per-game value.
pub(super) fn dump_folder() -> io::Result<Option<PathBuf>> {
    read_folder(HKEY_LOCAL_MACHINE, [&game_key(), GLOBAL])
}

fn read_folder_single(hive: windows_sys::Win32::System::Registry::HKEY, subkey: &str) -> io::Result<Option<PathBuf>> {
    let mut buffer = vec![0u16; 32768];
    let mut bytes = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(hive, wide(subkey).as_ptr(), w!("DumpFolder"), RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY, null_mut(), buffer.as_mut_ptr().cast(), &mut bytes)
    };
    match status {
        ERROR_SUCCESS => {
            let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            let path = PathBuf::from(OsString::from_wide(&buffer[..length]));
            if path.is_absolute() {
                Ok(Some(path))
            } else {
                Err(io::Error::new(io::ErrorKind::InvalidData, "DumpFolder is not absolute"))
            }
        }
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
        code => Err(io::Error::from_raw_os_error(code as i32)),
    }
}

fn read_folder(hive: windows_sys::Win32::System::Registry::HKEY, keys: [&str; 2]) -> io::Result<Option<PathBuf>> {
    for subkey in keys {
        if let Some(path) = read_folder_single(hive, subkey)? {
            return Ok(Some(path));
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

/// Runs `reg.exe` twice in one elevated `cmd.exe` invocation so setting both
/// `DumpFolder` and `DumpType = 1` (MiniDump) requires only a single UAC prompt.
/// `%LOCALAPPDATA%` is unquoted with `^%` escapes so `cmd.exe` passes the
/// literal `REG_EXPAND_SZ` placeholder to `reg.exe` instead of expanding it
/// against the elevated administrator account's profile.
fn enable_cmd_arguments(key: &str) -> Vec<u16> {
    wide(&format!(
        r#"/d /c reg.exe add "{key}" /v DumpFolder /t REG_EXPAND_SZ /d ^%LOCALAPPDATA^%\CrashDumps\eu4-harmony-wer /f /reg:64 && reg.exe add "{key}" /v DumpType /t REG_DWORD /d 1 /f /reg:64"#
    ))
}

fn delete_key_arguments(key: &str) -> Vec<u16> {
    wide(&format!(r#"delete "{key}" /f /reg:64"#))
}

/// Only a system tool from `System32` runs elevated, from its own directory.
/// The tool name, verb, key, value and arguments are fixed at the call site.
fn run_system_tool(window: HWND, tool: &str, verb: PCWSTR, parameters: &[u16]) -> io::Result<()> {
    let hr = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };
    if hr < 0 {
        return Err(io::Error::other(format!("CoInitializeEx failed ({hr:#x})")));
    }
    let result = launch_system_tool(window, tool, verb, parameters);
    unsafe { CoUninitialize() };
    result
}

fn launch_system_tool(window: HWND, tool: &str, verb: PCWSTR, parameters: &[u16]) -> io::Result<()> {
    let directory = system_directory()?;
    let mut executable = directory[..directory.len() - 1].to_vec();
    executable.extend(wide(tool));
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
        return Err(io::Error::other(format!("Windows did not return the {tool} process")));
    }
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
    let mut code = 0;
    if unsafe { WaitForSingleObject(process.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0
        || unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if code != 0 {
        return Err(io::Error::other(format!("{tool} exited with code {code}")));
    }
    Ok(())
}

pub(crate) fn enable() -> io::Result<Configuration> {
    let current = status()?;
    if current != Configuration::NotConfigured {
        return Ok(current);
    }
    run_system_tool(crate::base::game_window().unwrap_or(null_mut()), r"\cmd.exe", w!("runas"), &enable_cmd_arguments(&format!(r"HKLM\{}", game_key())))?;
    let expected = harmony_wer_folder();
    let after = status()?;
    if expected.is_none() || dump_folder()? != expected || after != Configuration::Harmony {
        return Err(io::Error::other("Windows did not save the requested EU4 dump folder"));
    }
    Ok(after)
}

/// Removes Harmony's own `LocalDumps\eu4.exe` rule and clears old dumps inside
/// `%LOCALAPPDATA%\CrashDumps\eu4-harmony-wer`. Refuses to delete rules
/// configured externally.
pub(crate) fn disable() -> io::Result<Configuration> {
    match status()? {
        Configuration::NotConfigured => {
            clear_harmony_wer_dumps();
            return Ok(Configuration::NotConfigured);
        }
        Configuration::External => {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Windows crash dumps for EU4 are configured externally"));
        }
        Configuration::Harmony => {}
    }
    run_system_tool(crate::base::game_window().unwrap_or(null_mut()), r"\reg.exe", w!("runas"), &delete_key_arguments(&format!(r"HKLM\{}", game_key())))?;
    let after = status()?;
    if after != Configuration::NotConfigured {
        return Err(io::Error::other("Windows did not remove the EU4 dump rule"));
    }
    clear_harmony_wer_dumps();
    Ok(after)
}

fn clear_harmony_wer_dumps() {
    let Some(dir) = harmony_wer_folder() else { return };
    clear_wer_dumps_in(&dir);
}

fn clear_wer_dumps_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if eu4_harmony_folders::is_wer_dump(&name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let _ = std::fs::remove_dir(dir);
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Registry::{RegDeleteKeyW, HKEY_CURRENT_USER, RRF_NOEXPAND, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ};

    fn read_raw_expand_sz(hive: windows_sys::Win32::System::Registry::HKEY, subkey: &str, name: PCWSTR) -> io::Result<Option<String>> {
        let mut bytes = 0u32;
        let flags = RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND | RRF_SUBKEY_WOW6464KEY;
        let status = unsafe { RegGetValueW(hive, wide(subkey).as_ptr(), name, flags, null_mut(), null_mut(), &mut bytes) };
        match status {
            ERROR_SUCCESS => {}
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(None),
            code => return Err(io::Error::from_raw_os_error(code as i32)),
        }
        let mut raw = vec![0u16; (bytes as usize) / 2];
        let status = unsafe { RegGetValueW(hive, wide(subkey).as_ptr(), name, flags, null_mut(), raw.as_mut_ptr().cast(), &mut bytes) };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let len = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        Ok(Some(String::from_utf16_lossy(&raw[..len])))
    }

    fn read_dword(hive: windows_sys::Win32::System::Registry::HKEY, subkey: &str, name: PCWSTR) -> io::Result<Option<u32>> {
        let mut value = 0u32;
        let mut bytes = size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(hive, wide(subkey).as_ptr(), name, RRF_RT_REG_DWORD | RRF_SUBKEY_WOW6464KEY, null_mut(), (&raw mut value).cast(), &mut bytes)
        };
        match status {
            ERROR_SUCCESS => Ok(Some(value)),
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }

    #[test]
    fn the_system_tool_stores_folder_and_minidump_type_and_deletes_cleanly() {
        let base = format!(r"Software\eu4-harmony-test-wer-{}", std::process::id());
        let game = format!(r"{base}\eu4.exe");
        let expected = harmony_wer_folder().unwrap();

        struct Scratch(String);
        impl Drop for Scratch {
            fn drop(&mut self) {
                unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.0).as_ptr()) };
            }
        }
        let _base = Scratch(base.clone());
        let _game = Scratch(game.clone());
        assert_eq!(read_folder(HKEY_CURRENT_USER, [&game, &base]).unwrap(), None);
        run_system_tool(null_mut(), r"\cmd.exe", w!("open"), &enable_cmd_arguments(&format!(r"HKCU\{game}"))).unwrap();
        assert_eq!(
            read_raw_expand_sz(HKEY_CURRENT_USER, &game, w!("DumpFolder")).unwrap().as_deref(),
            Some(r"%LOCALAPPDATA%\CrashDumps\eu4-harmony-wer"),
            "DumpFolder must preserve unexpanded %LOCALAPPDATA% for cross-account UAC elevation"
        );
        assert_eq!(read_folder(HKEY_CURRENT_USER, [&game, &base]).unwrap(), Some(expected));
        assert_eq!(read_dword(HKEY_CURRENT_USER, &game, w!("DumpType")).unwrap(), Some(1));
        run_system_tool(null_mut(), r"\reg.exe", w!("open"), &delete_key_arguments(&format!(r"HKCU\{game}"))).unwrap();
        assert_eq!(read_folder(HKEY_CURRENT_USER, [&game, &base]).unwrap(), None);
    }

    #[test]
    fn clearing_wer_dumps_only_removes_eu4_dumps() {
        let dir = std::env::temp_dir().join(format!("eu4h-wer-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("eu4.exe.1234.dmp"), b"dump").unwrap();
        std::fs::write(dir.join("keep.txt"), b"note").unwrap();
        clear_wer_dumps_in(&dir);
        assert!(!dir.join("eu4.exe.1234.dmp").exists());
        assert!(dir.join("keep.txt").exists());
        std::fs::remove_file(dir.join("keep.txt")).unwrap();
        clear_wer_dumps_in(&dir);
        assert!(!dir.exists(), "empty folder removed");
    }
}
