//! A short-lived writer for the game's confirmed exception. The process
//! handle comes from the game, never from a PID supplied on the command line.
#![windows_subsystem = "windows"]
use std::io::{self, ErrorKind};
use std::os::windows::io::AsHandle;
use std::path::Path;

fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let invalid = || io::Error::new(ErrorKind::InvalidInput, "expected a folder, thread ID and exception pointers");
    let [folder, thread, pointers] = args.as_slice() else { return Err(invalid()) };
    let tid = thread.to_str().and_then(|s| s.parse::<u32>().ok()).filter(|&v| v != 0).ok_or_else(invalid)?;
    let pointers = pointers.to_str().and_then(|s| s.parse::<usize>().ok()).filter(|&v| v != 0).ok_or_else(invalid)?;
    eu4_harmony_crash_dumps::write_game(Path::new(folder), io::stdin().as_handle(), tid, pointers)
}

fn main() {
    if let Err(e) = run() {
        std::process::exit(e.raw_os_error().unwrap_or(windows_sys::Win32::Foundation::ERROR_GEN_FAILURE as i32));
    }
}
