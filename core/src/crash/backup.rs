//! Write the fault from another process while its original exception pointers
//! are live. The game's crash writer resumes after a bounded wait.
use std::io;
use std::os::windows::{
    io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::CommandExt,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use windows_sys::Win32::{
    Foundation::{DuplicateHandle, WAIT_OBJECT_0},
    System::Threading::{GetCurrentProcess, WaitForSingleObject, CREATE_NO_WINDOW, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
};

static WRITER: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();

pub(super) fn init(core: &Path, folder: PathBuf) {
    let _ = WRITER.set((core.join("eu4-harmony-dump-writer.exe"), folder));
}

pub(super) fn capture(tid: u32, pointers: usize) {
    let Some((exe, folder)) = WRITER.get() else { return };
    if pointers != 0 && tid != 0 {
        if let Err(e) = run(exe, folder, tid, pointers) {
            crate::log::write_within(format_args!("crash: backup minidump unavailable: {e}"), std::time::Duration::ZERO);
        }
    }
}

fn run(exe: &Path, folder: &Path, tid: u32, pointers: usize) -> io::Result<()> {
    let process = unsafe { GetCurrentProcess() };
    let mut handle = std::ptr::null_mut();
    if unsafe { DuplicateHandle(process, process, process, &mut handle, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let target = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut child = Command::new(exe)
        .arg(folder)
        .arg(tid.to_string())
        .arg(pointers.to_string())
        .stdin(Stdio::from(target))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    if unsafe { WaitForSingleObject(child.as_raw_handle(), 5000) } != WAIT_OBJECT_0 {
        child.kill()?;
        child.wait()?;
        return Err(io::Error::new(io::ErrorKind::TimedOut, "the minidump writer timed out"));
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(status.code().map_or_else(|| io::Error::other("the minidump writer stopped"), io::Error::from_raw_os_error));
    }
    Ok(())
}
