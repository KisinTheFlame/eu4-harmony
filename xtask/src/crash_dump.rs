//! `cargo xtask crash-dump`: runs debugger commands on a crash dump with
//! Windows' own `dbgeng.dll`, no debugger to install.
//!
//! The game writes a folder per crash under the user folder's `crashes\`
//! (`eu4_<date>_<time>`), with `minidump.dmp` and `exception.txt`; the
//! framework's logs are in its `eu4-harmony\logs\`.
use crate::{built, built_package, Result};
use std::{ffi::CString, path::PathBuf};
use windows::{
    core::PCSTR,
    Win32::System::Diagnostics::Debug::Extensions::{DebugCreate, IDebugClient, IDebugControl, DEBUG_EXECUTE_DEFAULT, DEBUG_OUTCTL_THIS_CLIENT},
};

/// `<dump> <log> [--symbols <dir>] <command>...`
pub fn run(raw: &[String]) -> Result<()> {
    let usage = "usage: cargo xtask crash-dump <dump> <log> [--symbols <dir>] <command>...";
    let [dump, log, rest @ ..] = raw else { return Err(usage.into()) };
    let (symbols, commands) = match rest {
        [flag, dir, commands @ ..] if flag == "--symbols" => (dir.as_str(), commands),
        commands => ("C:\\symbols", commands),
    };
    if commands.is_empty() {
        return Err(usage.into());
    }
    // The release build's symbols, or the player package's, whichever the
    // dump's core was built as: the debugger takes the ones that match it.
    read(dump, log, &[built(), built_package()], symbols, commands)
}

/// Runs `commands` on the crash dump `dump`, writing their output to `log`,
/// with the symbols of the builds in `builds` and the system's cached in
/// `symbols`.
pub fn read(dump: &str, log: &str, builds: &[PathBuf], symbols: &str, commands: &[String]) -> Result<()> {
    let text = |s: &str| CString::new(s).map_err(|_| format!("{s:?} holds a null character"));
    let builds: Vec<String> = builds.iter().map(|b| b.display().to_string()).collect();
    let path = format!(".sympath {};srv*{symbols}*https://msdl.microsoft.com/download/symbols", builds.join(";"));
    let (dump, log, path) = (text(dump)?, text(log)?, text(&path)?);
    let commands: Vec<CString> = commands.iter().map(|c| text(c)).collect::<Result<_>>()?;
    let ran = || -> windows::core::Result<()> {
        unsafe {
            let client: IDebugClient = DebugCreate()?;
            let control: IDebugControl = windows::core::Interface::cast(&client)?;
            control.OpenLogFile(PCSTR::from_raw(log.as_ptr().cast()), false)?;
            let execute = |command: &CString| control.Execute(DEBUG_OUTCTL_THIS_CLIENT, PCSTR::from_raw(command.as_ptr().cast()), DEBUG_EXECUTE_DEFAULT);
            execute(&path)?;
            client.OpenDumpFile(PCSTR::from_raw(dump.as_ptr().cast()))?;
            control.WaitForEvent(0, u32::MAX)?;
            // A command that fails says so in the log; the rest still run.
            for command in &commands {
                let _ = execute(command);
            }
            control.CloseLogFile()
        }
    };
    ran().map_err(|e| format!("dbgeng: {e}"))
}
