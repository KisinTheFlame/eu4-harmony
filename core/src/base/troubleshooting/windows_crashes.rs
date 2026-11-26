//! Read crash evidence kept by Windows even when the game cannot write its own.
use std::fmt::Write as _;
use std::io;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, FILETIME, SYSTEMTIME};
use windows_sys::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
use windows_sys::Win32::System::EventLog::{
    EvtClose, EvtCreateRenderContext, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender, EvtRenderContextUser,
    EvtRenderContextValues, EvtRenderEventValues, EvtVarTypeFileTime, EvtVarTypeHexInt32 as EVT_HEX32, EvtVarTypeHexInt64 as EVT_HEX64, EvtVarTypeString,
    EvtVarTypeUInt32 as EVT_UINT32, EvtVarTypeUInt64 as EVT_UINT64, EVT_HANDLE, EVT_VARIANT,
};
use windows_sys::Win32::System::Time::FileTimeToSystemTime;

// Filter in Windows before reading any event. Data[1] is the executable name
// (Win10 omits @Name attributes, whereas both Win10 and Win11 use identical
// positional EventData indices); Data[11] (index 10) is AppPath.
const RECENT: PCWSTR =
    w!("*[System[Provider[@Name='Application Error'] and EventID=1000 and TimeCreated[timediff(@SystemTime) <= 2592000000]] and EventData[Data[1]='eu4.exe']]");
const MAX_EVENTS: usize = 128;
const NEXT_TIMEOUT_MS: u32 = 1000;
const MAX_EVENT_BYTES: u32 = 128 * 1024;

struct EventHandle(EVT_HANDLE);

impl EventHandle {
    fn checked(handle: EVT_HANDLE) -> io::Result<Self> {
        if handle == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        unsafe { EvtClose(self.0) };
    }
}

/// Querying is best effort. Even if the Windows log cannot be read, the
/// troubleshooting files carry what was collected and why the query stopped.
pub(super) fn report(game: &Path) -> String {
    let mut text = format!(
        "Windows Application Error event 1000 for this installation of eu4.exe\n\
         Looking back 30 days, up to {MAX_EVENTS} candidate events, newest first.\n\
         Compare run with Harmony's run ID to identify the same process. An event may be from another user folder or a run without Harmony.\n\
         Without a matching event, the reason the process ended remains unknown to this report.\n"
    );
    if let Err(e) = collect(game, &mut text) {
        let _ = writeln!(text, "Collection incomplete: {e}");
    }
    text
}

fn collect(game: &Path, text: &mut String) -> io::Result<()> {
    let found = EventHandle::checked(unsafe { EvtQuery(0, w!("Application"), RECENT, EvtQueryChannelPath | EvtQueryReverseDirection) })?;
    let system_paths = [w!("Event/System/TimeCreated/@SystemTime"), w!("Event/System/EventRecordID")];
    let system_context = EventHandle::checked(unsafe { EvtCreateRenderContext(system_paths.len() as u32, system_paths.as_ptr(), EvtRenderContextValues) })?;
    let user_context = EventHandle::checked(unsafe { EvtCreateRenderContext(0, std::ptr::null(), EvtRenderContextUser) })?;
    let exe = game.join("eu4.exe");
    let exe = exe.to_string_lossy();
    let mut exported = 0;
    let mut unreadable = 0;
    let mut last_error = None;
    let mut reached_end = false;
    for _ in 0..MAX_EVENTS {
        let mut handle = 0;
        let mut returned = 0;
        if unsafe { EvtNext(found.0, 1, &mut handle, NEXT_TIMEOUT_MS, 0, &mut returned) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NO_MORE_ITEMS as i32) {
                reached_end = true;
            } else {
                last_error = Some(error);
            }
            break;
        }
        let event = EventHandle(handle);
        match render(&system_context, &event, system_paths.len())
            .and_then(|system| render(&user_context, &event, 11).map(|user| (system, user)))
            .and_then(|(system, user)| summarize(&system, &user, &exe))
        {
            Ok(Some(record)) => {
                text.push_str(&record);
                exported += 1;
            }
            Ok(None) => (),
            Err(_) => unreadable += 1,
        }
    }
    let _ = writeln!(text, "Exported {exported} matching event(s); {unreadable} candidate event(s) could not be read.");
    if let Some(error) = last_error {
        return Err(error);
    }
    if !reached_end {
        let _ = writeln!(text, "Stopped at the candidate limit.");
    }
    Ok(())
}

fn render(context: &EventHandle, event: &EventHandle, min_count: usize) -> io::Result<Vec<EVT_VARIANT>> {
    let mut bytes = 0;
    let mut properties = 0;
    let ok = unsafe { EvtRender(context.0, event.0, EvtRenderEventValues, 0, null_mut(), &mut bytes, &mut properties) };
    let error = io::Error::last_os_error();
    if ok != 0 || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) || bytes > MAX_EVENT_BYTES {
        return Err(error);
    }
    // The API writes aligned variants followed by the strings they reference.
    let mut buffer = vec![EVT_VARIANT::default(); (bytes as usize).div_ceil(size_of::<EVT_VARIANT>())];
    if unsafe { EvtRender(context.0, event.0, EvtRenderEventValues, bytes, buffer.as_mut_ptr().cast(), &mut bytes, &mut properties) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if (properties as usize) < min_count || buffer.len() < min_count {
        return Err(io::Error::other("unexpected Application Error fields"));
    }
    Ok(buffer)
}

fn summarize(system: &[EVT_VARIANT], user: &[EVT_VARIANT], exe: &str) -> io::Result<Option<String>> {
    let [time, record, ..] = system else {
        return Err(io::Error::other("missing Application Error fields"));
    };
    let [_, app_version, app_timestamp, module, version, timestamp, code, offset, pid, created, path, ..] = user else {
        return Err(io::Error::other("missing Application Error fields"));
    };
    let path = string(path).ok_or_else(|| io::Error::other("missing Application Error executable path"))?;
    if !same_path(&path, exe) {
        return Ok(None);
    }

    let when = if time.Type == EvtVarTypeFileTime as u32 { utc(unsafe { time.Anonymous.FileTimeVal }) } else { None };
    let when = when.unwrap_or_else(|| "unknown".into());
    let id = if record.Type == EVT_UINT64 as u32 { unsafe { record.Anonymous.UInt64Val }.to_string() } else { "unknown".into() };
    let pid = hexadecimal(pid).and_then(|value| u32::try_from(value).ok()).filter(|&pid| pid != 0);
    let created = hexadecimal(created).filter(|&created| created != 0);
    let run = pid.zip(created).map_or_else(|| "unknown".into(), |(pid, created)| eu4_harmony_logging::process_run_id(pid, created));
    let field = |value| string(value).unwrap_or_else(|| "unknown".into());
    let hex = |value| hexadecimal(value).map_or_else(|| "unknown".into(), |number| format!("{number:#x}"));
    Ok(Some(format!(
        "{when} event_record={id} run={run} process_id={} process_created_filetime={} app_version={:?} app_timestamp={} module={:?} module_version={:?} module_timestamp={} exception_code={} fault_offset={}\n",
        pid.map_or_else(|| "unknown".into(), |pid| pid.to_string()),
        created.map_or_else(|| "unknown".into(), |created| format!("{created:#x}")),
        field(app_version), hex(app_timestamp), field(module), field(version), hex(timestamp), hex(code), hex(offset),
    )))
}

fn same_path(left: &str, right: &str) -> bool {
    let wide = |path: &str| {
        let path = path.replace('/', "\\");
        if let Some(tail) = path.strip_prefix(r"\\?\UNC\") {
            return r"\\".encode_utf16().chain(tail.encode_utf16()).collect::<Vec<_>>();
        }
        path.strip_prefix(r"\\?\").unwrap_or(&path).encode_utf16().collect()
    };
    let (left, right) = (wide(left), wide(right));
    unsafe { CompareStringOrdinal(left.as_ptr(), left.len() as i32, right.as_ptr(), right.len() as i32, 1) == CSTR_EQUAL }
}

fn string(value: &EVT_VARIANT) -> Option<String> {
    if value.Type != EvtVarTypeString as u32 {
        return None;
    }
    // For a string variant, Windows returns a NUL-terminated UTF-16 pointer
    // inside the rendered buffer, which the caller still owns.
    let ptr = unsafe { value.Anonymous.StringVal };
    if ptr.is_null() {
        return None;
    }
    let len = (0..).take_while(|&i| unsafe { *ptr.add(i) } != 0).count();
    Some(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }))
}

fn hexadecimal(value: &EVT_VARIANT) -> Option<u64> {
    // Application Error's process ID and creation time are typed hex values.
    // Exception code and offset can instead be hexadecimal strings.
    match value.Type as i32 {
        EVT_HEX32 | EVT_UINT32 => Some(u64::from(unsafe { value.Anonymous.UInt32Val })),
        EVT_HEX64 | EVT_UINT64 => Some(unsafe { value.Anonymous.UInt64Val }),
        _ => {
            let text = string(value)?;
            u64::from_str_radix(text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")).unwrap_or(&text), 16).ok()
        }
    }
}

fn utc(ticks: u64) -> Option<String> {
    let time = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc = SYSTEMTIME::default();
    (unsafe { FileTimeToSystemTime(&time, &mut utc) } != 0)
        .then(|| format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", utc.wYear, utc.wMonth, utc.wDay, utc.wHour, utc.wMinute, utc.wSecond, utc.wMilliseconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::EventLog::EVT_VARIANT_0;

    fn str_var(value: PCWSTR) -> EVT_VARIANT {
        EVT_VARIANT { Anonymous: EVT_VARIANT_0 { StringVal: value }, Count: 0, Type: EvtVarTypeString as u32 }
    }

    #[test]
    fn the_typed_windows_process_identity_matches_the_harmony_run_id() {
        let system = [EVT_VARIANT::default(); 2];
        let mut user = [EVT_VARIANT::default(); 11];
        user[3] = str_var(w!("ntdll.dll"));
        user[6] = str_var(w!("c0000374"));
        user[7] = str_var(w!("0000000000002bb3"));
        user[8] = EVT_VARIANT { Anonymous: EVT_VARIANT_0 { UInt32Val: 0x386 }, Count: 0, Type: EVT_HEX32 as u32 };
        user[9] = EVT_VARIANT { Anonymous: EVT_VARIANT_0 { UInt64Val: 0x1dd76a22ea180f6 }, Count: 0, Type: EVT_HEX64 as u32 };
        user[10] = str_var(w!(r"C:\Games\Europa Universalis IV\eu4.exe"));

        let exe = r"\\?\c:\games\europa universalis iv\eu4.exe";
        let line = summarize(&system, &user, exe).unwrap().unwrap();
        assert!(line.contains("run=902-1dd76a22ea180f6 "));
        assert!(line.contains("module=\"ntdll.dll\""));
        assert!(line.contains("exception_code=0xc0000374 fault_offset=0x2bb3"));
        assert!(summarize(&system, &user, r"D:\Other\eu4.exe").unwrap().is_none());

        user[9] = EVT_VARIANT::default();
        assert!(summarize(&system, &user, exe).unwrap().unwrap().contains("run=unknown process_id=902 "));
    }

    #[test]
    fn unnamed_string_event_data_fields_match_windows_10_layout() {
        let system = [EVT_VARIANT::default(); 2];
        let mut user = [EVT_VARIANT::default(); 11];
        user[0] = str_var(w!("eu4.exe"));
        user[3] = str_var(w!("eu4.exe"));
        user[6] = str_var(w!("c0000005"));
        user[7] = str_var(w!("00000000014ddab7"));
        user[8] = str_var(w!("0xc2c"));
        user[9] = str_var(w!("0x01dd7829cb8e99a4"));
        user[10] = str_var(w!(r"E:\SteamLibrary\steamapps\common\Europa Universalis IV\eu4.exe"));

        let exe = r"E:\SteamLibrary\steamapps\common\Europa Universalis IV\eu4.exe";
        let line = summarize(&system, &user, exe).unwrap().unwrap();
        assert!(line.contains("run=3116-1dd7829cb8e99a4 "));
        assert!(line.contains("module=\"eu4.exe\""));
        assert!(line.contains("exception_code=0xc0000005 fault_offset=0x14ddab7"));
    }

    #[test]
    fn live_windows_application_query_runs_without_collection_error() {
        let out = report(Path::new(r"C:\NonExistent\Europa Universalis IV"));
        assert!(!out.contains("Collection incomplete:"), "{out}");
        assert!(out.contains("Exported "), "{out}");
    }
}
