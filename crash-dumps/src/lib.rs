//! A Windows Error Reporting callback, loaded into WerFault after a fatal
//! exception. The saved exception and registers already live in this process.
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, BorrowedHandle};
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::{E_INVALIDARG, E_UNEXPECTED, FILETIME, GENERIC_WRITE, HANDLE, S_OK},
    Storage::FileSystem::{FileDispositionInfo, SetFileInformationByHandle, DELETE, FILE_DISPOSITION_INFO},
    System::{
        Diagnostics::Debug::{MiniDumpNormal, MiniDumpWriteDump, CONTEXT, EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION},
        ErrorReporting::WER_RUNTIME_EXCEPTION_INFORMATION,
        Threading::{GetProcessId, GetProcessTimes, GetThreadId, QueryFullProcessImageNameW},
    },
};

fn write_dump(info: &WER_RUNTIME_EXCEPTION_INFORMATION) -> io::Result<()> {
    let mut image = [0; 32768];
    let mut len = image.len() as u32;
    if unsafe { QueryFullProcessImageNameW(info.hProcess, 0, image.as_mut_ptr(), &mut len) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let image = String::from_utf16_lossy(&image[..len as usize]);
    if !Path::new(&image).file_name().is_some_and(|name| name.eq_ignore_ascii_case("eu4.exe")) {
        return Ok(());
    }
    let dir = eu4_harmony_folders::windows_dump_root().ok_or_else(|| io::Error::other("no local app data folder"))?.join("eu4-harmony");
    let tid = unsafe { GetThreadId(info.hThread) };
    let mut record = info.exceptionRecord;
    // Nested record pointers would point into the faulting process.
    record.ExceptionRecord = null_mut();
    #[repr(align(16))]
    struct Aligned(CONTEXT);
    let mut aligned = Aligned(info.context);
    let mut pointers = EXCEPTION_POINTERS { ExceptionRecord: &mut record, ContextRecord: &mut aligned.0 };
    let exception = MINIDUMP_EXCEPTION_INFORMATION { ThreadId: tid, ExceptionPointers: &mut pointers, ClientPointers: 0 };
    write(&dir, info.hProcess, &exception)
}

/// The game supplies a read-only handle to itself and keeps the fault's
/// pointers live while the writer runs. DbgHelp reads them in the game.
pub fn write_game(dir: &Path, process: BorrowedHandle<'_>, tid: u32, pointers: usize) -> io::Result<()> {
    let exception = MINIDUMP_EXCEPTION_INFORMATION { ThreadId: tid, ExceptionPointers: pointers as *mut _, ClientPointers: 1 };
    write(dir, process.as_raw_handle(), &exception)
}

fn write(dir: &Path, process: HANDLE, exception: &MINIDUMP_EXCEPTION_INFORMATION) -> io::Result<()> {
    let pid = unsafe { GetProcessId(process) };
    let mut times = [FILETIME::default(); 4];
    let [created, exited, kernel, user] = &mut times;
    if pid == 0 || exception.ThreadId == 0 || unsafe { GetProcessTimes(process, created, exited, kernel, user) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let created = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    fs::create_dir_all(dir)?;
    let name = format!("harmony-{pid}-{created:016x}");
    let stage = dir.join(format!("{name}.partial"));
    let file = OpenOptions::new().write(true).create_new(true).access_mode(GENERIC_WRITE | DELETE).share_mode(0).open(&stage)?;
    let complete = dir.join(format!("{name}.dmp"));
    if complete.exists() {
        drop(file);
        return fs::remove_file(stage);
    }
    // Windows drops the partial file even if the game's timeout ends the
    // writer in DbgHelp. Publish only after all bytes have reached the file.
    let result = delete_on_close(&file, true).and_then(|()| {
        if unsafe { MiniDumpWriteDump(process, pid, file.as_raw_handle(), MiniDumpNormal, exception, null(), null()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        file.sync_all()?;
        delete_on_close(&file, false)
    });
    drop(file);
    if let Err(e) = result.and_then(|()| fs::rename(&stage, complete)) {
        let _ = fs::remove_file(stage);
        return Err(e);
    }
    prune(dir);
    Ok(())
}

fn delete_on_close(file: &File, delete: bool) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: delete };
    if unsafe { SetFileInformationByHandle(file.as_raw_handle(), FileDispositionInfo, (&raw const disposition).cast(), size_of_val(&disposition) as u32) } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Keep the last five completed Harmony captures. Leave Windows' own files alone.
fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut dumps = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some((_, created)) = name.to_str().and_then(eu4_harmony_folders::captured_dump_id) else { continue };
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            if let Ok(written) = entry.metadata().and_then(|metadata| metadata.modified()) {
                dumps.push((written, created, entry.path()));
            }
        }
    }
    dumps.sort_unstable();
    for (_, _, path) in dumps.iter().rev().skip(5) {
        let _ = fs::remove_file(path);
    }
}

// Windows requires these three export names. Declining ownership lets the
// normal Windows reporting and any existing LocalDumps policy continue.
#[no_mangle]
unsafe extern "system" fn OutOfProcessExceptionEventCallback(
    _: *const c_void,
    info: *const WER_RUNTIME_EXCEPTION_INFORMATION,
    claimed: *mut i32,
    _: *mut u16,
    _: *mut u32,
    _: *mut u32,
) -> i32 {
    if claimed.is_null() || info.is_null() {
        return E_INVALIDARG;
    }
    *claimed = 0;
    if (*info).dwSize as usize >= std::mem::size_of::<WER_RUNTIME_EXCEPTION_INFORMATION>() && (*info).bIsFatal != 0 {
        let _ = write_dump(&*info);
    }
    S_OK
}

#[no_mangle]
extern "system" fn OutOfProcessExceptionEventSignatureCallback(
    _: *const c_void,
    _: *const WER_RUNTIME_EXCEPTION_INFORMATION,
    _: u32,
    _: *mut u16,
    _: *mut u32,
    _: *mut u16,
    _: *mut u32,
) -> i32 {
    E_UNEXPECTED
}

#[no_mangle]
extern "system" fn OutOfProcessExceptionEventDebuggerLaunchCallback(
    _: *const c_void,
    _: *const WER_RUNTIME_EXCEPTION_INFORMATION,
    _: *mut i32,
    _: *mut u16,
    _: *mut u32,
    _: *mut i32,
) -> i32 {
    E_UNEXPECTED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_only_rotates_completed_harmony_captures() {
        let dir = std::env::temp_dir().join(format!("harmony-dump-retention-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let captures: Vec<_> = (1..=7).map(|n| dir.join(format!("harmony-123-{n:016x}.dmp"))).collect();
        for file in &captures {
            fs::write(file, []).unwrap();
        }
        for name in ["eu4.exe.123.dmp", "harmony-123-0000000000000001.partial", "notes.txt"] {
            fs::write(dir.join(name), []).unwrap();
        }
        prune(&dir);
        assert!(captures[..2].iter().all(|file| !file.exists()));
        assert!(captures[2..].iter().all(|file| file.exists()));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 8);
        fs::remove_dir_all(dir).unwrap();
    }
}
