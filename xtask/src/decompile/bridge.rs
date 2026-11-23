//! Process, socket, and headless orchestration for persistent Ghidra projects.
use super::config::{self, ProjectLayout, ReadyMarker, ServerInfo, Toolchain};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    os::windows::{ffi::OsStrExt, io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const BRIDGE_JAVA_SOURCE: &str = include_str!("HarmonyDecompileBridge.java");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;

#[derive(Debug, Clone, Serialize)]
pub struct DecompileTarget {
    pub rva: u64,
    pub pdata_start: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecompileItemResult {
    pub ok: bool,
    pub requested_rva: u64,
    pub error: Option<String>,
    pub entry_rva: Option<u64>,
    pub offset_in_function: Option<i64>,
    pub ghidra_name: Option<String>,
    pub signature: Option<String>,
    pub c_code: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BridgeReply {
    ok: bool,
    error: Option<String>,
    pid: Option<u32>,
    image_base_hex: Option<String>,
    #[serde(default)]
    results: Vec<DecompileItemResult>,
}

#[derive(Serialize)]
struct OneshotConfig<'a> {
    mode: &'static str,
    output_file: String,
    timeout_seconds: u32,
    requests: &'a [DecompileTarget],
}

#[derive(Serialize)]
struct DaemonConfig {
    mode: &'static str,
    server_file: String,
    token: String,
    idle_timeout_ms: u64,
}

#[derive(Serialize)]
struct DaemonRequest<'a> {
    token: &'a str,
    op: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requests: Option<&'a [DecompileTarget]>,
}

struct ProjectMutex(windows_sys::Win32::Foundation::HANDLE);

impl ProjectMutex {
    fn lock(key: &str) -> Result<Self> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0},
            System::Threading::{CreateMutexW, WaitForSingleObject, INFINITE},
        };
        let wide: Vec<u16> = format!("Local\\eu4-harmony-dev-ghidra-{key}\0").encode_utf16().collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, wide.as_ptr()) };
        if handle.is_null() {
            return Err(format!("cannot open Ghidra project mutex for {key}: {}", std::io::Error::last_os_error()));
        }
        let waited = unsafe { WaitForSingleObject(handle, INFINITE) };
        if waited != WAIT_OBJECT_0 && waited != WAIT_ABANDONED {
            unsafe { CloseHandle(handle) };
            return Err(format!("cannot acquire Ghidra project mutex for {key}"));
        }
        Ok(Self(handle))
    }
}

impl Drop for ProjectMutex {
    fn drop(&mut self) {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::ReleaseMutex};
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

/// Initializes (or rebuilds) a persistent Ghidra project with full auto-analysis.
pub fn init_project(toolchain: &Toolchain, layout: &ProjectLayout, module_path: &Path, image_base: u64, image_size: u64, rebuild: bool) -> Result<ReadyMarker> {
    if rebuild {
        stop_project_daemon(layout)?;
    }
    let _guard = ProjectMutex::lock(&layout.key)?;
    if !rebuild {
        if let Some(marker) = layout.read_ready() {
            return Ok(marker);
        }
    } else {
        stop_project_daemon(layout)?;
    }

    let _ = fs::remove_file(&layout.ready_path);
    let _ = fs::remove_file(&layout.server_path);
    if layout.ghidra_project_dir.exists() {
        fs::remove_dir_all(&layout.ghidra_project_dir).map_err(|e| format!("cannot clean {}: {e}", layout.ghidra_project_dir.display()))?;
    }
    fs::create_dir_all(&layout.ghidra_project_dir).map_err(|e| format!("cannot create {}: {e}", layout.ghidra_project_dir.display()))?;
    ensure_bridge_script(layout)?;

    let args = [
        layout.ghidra_project_dir.display().to_string(),
        "harmony".to_string(),
        "-import".to_string(),
        module_path.display().to_string(),
        "-overwrite".to_string(),
        "-log".to_string(),
        layout.init_log.display().to_string(),
        "-scriptlog".to_string(),
        layout.script_log.display().to_string(),
    ];
    let mut cmd = build_headless_command(toolchain, &args)?;
    cmd.stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let status = cmd.status().map_err(|e| format!("cannot run {}: {e}", toolchain.analyze_headless.display()))?;
    if !status.success() {
        return Err(format!(
            "Ghidra import/analysis failed with exit code {} (see {})",
            status.code().map_or_else(|| "unknown".into(), |c| c.to_string()),
            layout.init_log.display()
        ));
    }
    if !layout.ghidra_project_dir.join("harmony.gpr").is_file() {
        return Err(format!("Ghidra finished without creating {}", layout.ghidra_project_dir.join("harmony.gpr").display()));
    }

    let marker = config::new_ready_marker(&layout.module_name, &layout.sha256, image_base, image_size);
    let json = serde_json::to_string_pretty(&marker).map_err(|e| e.to_string())?;
    atomic_write(&layout.ready_path, &format!("{json}\n"))?;
    Ok(marker)
}

/// Decompiles a batch of targets through the persistent warm server (auto-starting it if needed).
pub fn decompile_warm(
    toolchain: &Toolchain,
    layout: &ProjectLayout,
    targets: &[DecompileTarget],
    timeout_seconds: u32,
) -> Result<(u64, Vec<DecompileItemResult>)> {
    ensure_bridge_script(layout)?;
    let server = ensure_warm_server(toolchain, layout)?;
    let req = DaemonRequest { token: &server.token, op: "decompile", timeout_seconds: Some(timeout_seconds), requests: Some(targets) };
    let io_timeout = Duration::from_secs((u64::from(timeout_seconds) * (targets.len().max(1) as u64)).saturating_add(45));
    let reply = send_tcp_request(server.port, &req, io_timeout)?;
    if !reply.ok {
        return Err(reply.error.unwrap_or_else(|| "Ghidra daemon rejected decompile request".into()));
    }
    let image_base = parse_image_base_hex(reply.image_base_hex.as_deref(), &server.image_base_hex)?;
    Ok((image_base, reply.results))
}

/// Decompiles a batch of targets using a single headless invocation without leaving a warm server running.
pub fn decompile_oneshot(
    toolchain: &Toolchain,
    layout: &ProjectLayout,
    targets: &[DecompileTarget],
    timeout_seconds: u32,
) -> Result<(u64, Vec<DecompileItemResult>)> {
    ensure_bridge_script(layout)?;
    let nonce = random_token()?;
    let short = &nonce[..12];
    let cfg_path = layout.root.join(format!("oneshot-{short}-cfg.json"));
    let out_path = layout.root.join(format!("oneshot-{short}-out.json"));
    let log_path = layout.root.join(format!("oneshot-{short}.log"));

    let cfg = OneshotConfig { mode: "oneshot", output_file: out_path.display().to_string(), timeout_seconds, requests: targets };
    let cfg_json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
    atomic_write(&cfg_path, &format!("{cfg_json}\n"))?;

    let args = [
        layout.ghidra_project_dir.display().to_string(),
        "harmony".to_string(),
        "-process".to_string(),
        layout.module_name.clone(),
        "-noanalysis".to_string(),
        "-readOnly".to_string(),
        "-log".to_string(),
        log_path.display().to_string(),
        "-scriptlog".to_string(),
        layout.script_log.display().to_string(),
        "-scriptPath".to_string(),
        layout.scripts_dir.display().to_string(),
        "-postScript".to_string(),
        layout.bridge_script_path.display().to_string(),
        cfg_path.display().to_string(),
    ];
    let mut cmd = build_headless_command(toolchain, &args)?;
    cmd.stdin(Stdio::null());
    let output = cmd.output();

    let _ = fs::remove_file(&cfg_path);
    let reply_raw = fs::read_to_string(&out_path);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_file(&log_path);

    let output = output.map_err(|e| format!("cannot run {}: {e}", toolchain.analyze_headless.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("Ghidra oneshot failed (exit {:?}):\n{}\n{}", output.status.code(), tail_lines(&stdout, 20), tail_lines(&stderr, 20)));
    }

    let reply_text = reply_raw.map_err(|e| format!("Ghidra oneshot produced no result file {}: {e}", out_path.display()))?;
    let reply: BridgeReply = serde_json::from_str(&reply_text).map_err(|e| format!("invalid bridge reply: {e}"))?;
    if !reply.ok {
        return Err(reply.error.unwrap_or_else(|| "Ghidra oneshot returned error".into()));
    }
    let image_base = parse_image_base_hex(reply.image_base_hex.as_deref(), "0")?;
    Ok((image_base, reply.results))
}

/// Prints cached projects and active warm servers.
pub fn print_status(dev_dir: &Path) -> Result<()> {
    let projects = config::list_project_roots(dev_dir);
    if projects.is_empty() {
        println!("no Ghidra projects initialized under {}", config::projects_dir(dev_dir).display());
        return Ok(());
    }
    println!("Ghidra projects under {}:", config::projects_dir(dev_dir).display());
    for (key, root) in projects {
        let ready = config::read_ready_marker(&root.join("ready.json")).filter(|_| root.join("project").join("harmony.gpr").is_file());
        let server_path = root.join("server.json");
        let server_state = match fs::read_to_string(&server_path).ok().and_then(|s| serde_json::from_str::<ServerInfo>(&s).ok()) {
            None => "stopped".to_string(),
            Some(info) => {
                if ping_server(&info).is_ok() {
                    format!("running (pid {}, 127.0.0.1:{})", info.pid, info.port)
                } else if is_process_alive(info.pid) {
                    format!("unresponsive (pid {}, 127.0.0.1:{})", info.pid, info.port)
                } else {
                    "stale server.json (process gone)".to_string()
                }
            }
        };
        match ready {
            Some(m) => println!("  {key}: ready ({}, sha256 {}..), server: {server_state}", m.module, &m.sha256[..12.min(m.sha256.len())]),
            None => println!("  {key}: incomplete/uninitialized, server: {server_state}"),
        }
    }
    Ok(())
}

/// Stops warm server(s). When `module_filter` is `None`, stops all live servers.
pub fn stop_daemons(dev_dir: &Path, module_filter: Option<&str>) -> Result<()> {
    let mut stopped = 0usize;
    for (key, root) in config::list_project_roots(dev_dir) {
        if let Some(module) = module_filter {
            let ready_matches = config::read_ready_marker(&root.join("ready.json")).is_some_and(|m| m.module.eq_ignore_ascii_case(module));
            let prefix = format!("{module}-");
            let key_matches = key.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase());
            if !ready_matches && !key_matches {
                continue;
            }
        }
        let server_path = root.join("server.json");
        if stop_server_at(&server_path)? {
            println!("stopped warm Ghidra server for {key}");
            stopped += 1;
        }
    }
    if stopped == 0 {
        match module_filter {
            Some(m) => println!("no running warm Ghidra server for {m}"),
            None => println!("no running warm Ghidra servers"),
        }
    }
    Ok(())
}

fn stop_project_daemon(layout: &ProjectLayout) -> Result<()> {
    let _ = stop_server_at(&layout.server_path)?;
    Ok(())
}

fn stop_server_at(server_path: &Path) -> Result<bool> {
    let Ok(text) = fs::read_to_string(server_path) else { return Ok(false) };
    let Ok(info) = serde_json::from_str::<ServerInfo>(&text) else {
        let _ = fs::remove_file(server_path);
        return Ok(false);
    };
    let mut had_live = false;
    if is_process_alive(info.pid) {
        had_live = true;
        let _ =
            send_tcp_request(info.port, &DaemonRequest { token: &info.token, op: "shutdown", timeout_seconds: None, requests: None }, Duration::from_secs(3));
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && is_process_alive(info.pid) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if is_process_alive(info.pid) {
            terminate_process(info.pid)?;
        }
    }
    let _ = fs::remove_file(server_path);
    Ok(had_live)
}

fn ensure_warm_server(toolchain: &Toolchain, layout: &ProjectLayout) -> Result<ServerInfo> {
    if let Some(info) = layout.read_server() {
        if is_process_alive(info.pid) && ping_server(&info).is_ok() {
            return Ok(info);
        }
    }

    let _guard = ProjectMutex::lock(&layout.key)?;
    if let Some(info) = layout.read_server() {
        if is_process_alive(info.pid) && ping_server(&info).is_ok() {
            return Ok(info);
        }
        let _ = stop_server_at(&layout.server_path);
    }

    let token = random_token()?;
    let cfg = DaemonConfig { mode: "daemon", server_file: layout.server_path.display().to_string(), token: token.clone(), idle_timeout_ms: 15 * 60 * 1000 };
    let cfg_json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
    atomic_write(&layout.daemon_config_path, &format!("{cfg_json}\n"))?;

    let args = [
        layout.ghidra_project_dir.display().to_string(),
        "harmony".to_string(),
        "-process".to_string(),
        layout.module_name.clone(),
        "-noanalysis".to_string(),
        "-readOnly".to_string(),
        "-log".to_string(),
        layout.daemon_ghidra_log.display().to_string(),
        "-scriptlog".to_string(),
        layout.script_log.display().to_string(),
        "-scriptPath".to_string(),
        layout.scripts_dir.display().to_string(),
        "-postScript".to_string(),
        layout.bridge_script_path.display().to_string(),
        layout.daemon_config_path.display().to_string(),
    ];
    let daemon = spawn_detached_daemon(toolchain, &args, &layout.daemon_log)?;
    wait_for_warm_server(&daemon, layout, &token)
}

fn wait_for_warm_server(daemon: &DaemonProcess, layout: &ProjectLayout, expected_token: &str) -> Result<ServerInfo> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(info) = layout.read_server() {
            if info.token == expected_token && is_process_alive(info.pid) && ping_server(&info).is_ok() {
                return Ok(info);
            }
        }
        if let Some(code) = daemon.try_wait()? {
            let log_tail = fs::read_to_string(&layout.daemon_log).map(|s| tail_lines(&s, 25)).unwrap_or_default();
            return Err(format!("warm Ghidra server exited early (exit code {code}); log {}:\n{log_tail}", layout.daemon_log.display()));
        }
        if Instant::now() >= deadline {
            daemon.kill();
            let log_tail = fs::read_to_string(&layout.daemon_log).map(|s| tail_lines(&s, 25)).unwrap_or_default();
            return Err(format!("timed out waiting 90s for warm Ghidra server; log {}:\n{log_tail}", layout.daemon_log.display()));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

struct DaemonProcess(windows_sys::Win32::Foundation::HANDLE);

impl DaemonProcess {
    fn try_wait(&self) -> Result<Option<u32>> {
        use windows_sys::Win32::{
            Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
        };
        let waited = unsafe { WaitForSingleObject(self.0, 0) };
        if waited == WAIT_TIMEOUT {
            return Ok(None);
        }
        if waited != WAIT_OBJECT_0 {
            return Err(format!("failed checking warm Ghidra server: {}", std::io::Error::last_os_error()));
        }
        let mut code = 0u32;
        if unsafe { GetExitCodeProcess(self.0, &mut code) } == 0 {
            return Err(format!("failed reading warm Ghidra server exit code: {}", std::io::Error::last_os_error()));
        }
        Ok(Some(code))
    }

    fn kill(&self) {
        unsafe {
            windows_sys::Win32::System::Threading::TerminateProcess(self.0, 1);
        }
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn spawn_detached_daemon(toolchain: &Toolchain, args: &[String], daemon_log: &Path) -> Result<DaemonProcess> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT},
        System::Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, UpdateProcThreadAttribute, EXTENDED_STARTUPINFO_PRESENT,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW,
        },
    };

    struct AttrList {
        buf: Vec<u8>,
    }
    impl AttrList {
        fn new(handles: &[HANDLE]) -> Result<Self> {
            let mut size = 0usize;
            unsafe {
                InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
            }
            if size == 0 {
                return Err(format!("InitializeProcThreadAttributeList failed: {}", std::io::Error::last_os_error()));
            }
            let mut buf = vec![0u8; size];
            let ptr = buf.as_mut_ptr().cast::<std::ffi::c_void>() as LPPROC_THREAD_ATTRIBUTE_LIST;
            if unsafe { InitializeProcThreadAttributeList(ptr, 1, 0, &mut size) } == 0 {
                return Err(format!("InitializeProcThreadAttributeList failed: {}", std::io::Error::last_os_error()));
            }
            let list = Self { buf };
            let updated = unsafe {
                UpdateProcThreadAttribute(
                    list.as_ptr(),
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_ptr().cast(),
                    std::mem::size_of_val(handles),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            if updated == 0 {
                return Err(format!("UpdateProcThreadAttribute failed: {}", std::io::Error::last_os_error()));
            }
            Ok(list)
        }

        fn as_ptr(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
            self.buf.as_ptr().cast::<std::ffi::c_void>() as LPPROC_THREAD_ATTRIBUTE_LIST
        }
    }
    impl Drop for AttrList {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.as_ptr());
            }
        }
    }

    let nul_file = OpenOptions::new().read(true).write(true).open("NUL").map_err(|e| format!("cannot open NUL: {e}"))?;
    let log_file = File::create(daemon_log).map_err(|e| format!("cannot create {}: {e}", daemon_log.display()))?;
    let nul_handle = nul_file.as_raw_handle() as HANDLE;
    let log_handle = log_file.as_raw_handle() as HANDLE;

    for &handle in &[nul_handle, log_handle] {
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
            return Err(format!("SetHandleInformation failed: {}", std::io::Error::last_os_error()));
        }
    }
    let handles = [nul_handle, log_handle];
    let attrs = AttrList::new(&handles)?;

    let comspec = resolve_comspec();
    let cmd_line = build_daemon_cmd_line(&comspec, &toolchain.analyze_headless, args)?;
    let env_block = build_unicode_env_block(std::env::vars_os(), toolchain)?;

    let exe_wide: Vec<u16> = comspec.encode_utf16().chain([0]).collect();
    let mut cmd_wide: Vec<u16> = cmd_line.encode_utf16().chain([0]).collect();
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = nul_handle;
    startup.StartupInfo.hStdOutput = log_handle;
    startup.StartupInfo.hStdError = log_handle;
    startup.lpAttributeList = attrs.as_ptr();

    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // Restrict inherited handles to only `[NUL, daemon.log]` so caller pipes
    // stay completely isolated while still capturing cmd/launch.bat output.
    let created = unsafe {
        CreateProcessW(
            exe_wide.as_ptr(),
            cmd_wide.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT,
            env_block.as_ptr().cast(),
            std::ptr::null(),
            &startup.StartupInfo,
            &mut info,
        )
    };
    if created == 0 {
        return Err(format!("cannot start warm Ghidra server: {}", std::io::Error::last_os_error()));
    }
    unsafe {
        CloseHandle(info.hThread);
    }
    Ok(DaemonProcess(info.hProcess))
}

fn resolve_comspec() -> String {
    if let Some(comspec) = std::env::var_os("ComSpec").filter(|v| !v.is_empty()) {
        return comspec.to_string_lossy().into_owned();
    }
    if let Some(root) = std::env::var_os("SystemRoot").filter(|v| !v.is_empty()) {
        return Path::new(&root).join("System32").join("cmd.exe").display().to_string();
    }
    String::from("C:\\Windows\\System32\\cmd.exe")
}

fn validate_no_quotes_or_ctrl(text: &str, label: &str) -> Result<()> {
    if text.chars().any(|c| matches!(c, '"' | '\r' | '\n' | '\0')) {
        return Err(format!("{label} contains unsupported quotes or control characters: {text:?}"));
    }
    Ok(())
}

fn build_daemon_cmd_line(comspec: &str, analyze_headless: &Path, args: &[String]) -> Result<String> {
    validate_no_quotes_or_ctrl(comspec, "ComSpec")?;
    let tail = build_cmd_raw_args(analyze_headless, args)?;
    Ok(format!("\"{comspec}\" {tail}"))
}

fn build_unicode_env_block<I>(base: I, toolchain: &Toolchain) -> Result<Vec<u16>>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let mut old_path: Option<OsString> = None;
    let mut entries: Vec<(OsString, OsString)> = Vec::new();
    for (k, v) in base {
        let name = k.to_string_lossy();
        if name.eq_ignore_ascii_case("GHIDRA_HEADLESS_MAXMEM") {
            continue;
        }
        if toolchain.java_home.is_some() {
            if name.eq_ignore_ascii_case("JAVA_HOME") {
                continue;
            }
            if name.eq_ignore_ascii_case("PATH") {
                old_path = Some(v);
                continue;
            }
        }
        entries.push((k, v));
    }

    entries.push((OsString::from("GHIDRA_HEADLESS_MAXMEM"), OsString::from(&toolchain.max_mem)));
    if let Some(ref java_home) = toolchain.java_home {
        entries.push((OsString::from("JAVA_HOME"), java_home.as_os_str().to_os_string()));
        let bin = java_home.join("bin");
        let existing = old_path.unwrap_or_default();
        let joined = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&existing))).map_err(|e| format!("invalid PATH: {e}"))?;
        entries.push((OsString::from("PATH"), joined));
    }

    entries.sort_by_key(|(a, _)| a.to_string_lossy().to_ascii_uppercase());

    let mut block = Vec::new();
    for (k, v) in entries {
        block.extend(k.encode_wide().filter(|&u| u != 0));
        block.push(b'=' as u16);
        block.extend(v.encode_wide().filter(|&u| u != 0));
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn ping_server(info: &ServerInfo) -> Result<()> {
    let reply = send_tcp_request(info.port, &DaemonRequest { token: &info.token, op: "ping", timeout_seconds: None, requests: None }, Duration::from_secs(2))?;
    if reply.ok && reply.pid == Some(info.pid) {
        Ok(())
    } else {
        Err(reply.error.unwrap_or_else(|| "unexpected ping response".into()))
    }
}

fn send_tcp_request<T: Serialize>(port: u16, req: &T, io_timeout: Duration) -> Result<BridgeReply> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|e| format!("cannot connect to 127.0.0.1:{port}: {e}"))?;
    let _ = stream.set_read_timeout(Some(io_timeout));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    stream.write_all(line.as_bytes()).map_err(|e| format!("failed sending request to Ghidra bridge: {e}"))?;
    stream.write_all(b"\n").map_err(|e| format!("failed sending newline to Ghidra bridge: {e}"))?;
    stream.flush().map_err(|e| format!("failed flushing request to Ghidra bridge: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut reply_line = String::new();
    reader.read_line(&mut reply_line).map_err(|e| format!("failed reading Ghidra bridge reply: {e}"))?;
    if reply_line.trim().is_empty() {
        return Err("Ghidra bridge closed connection without a reply".into());
    }
    serde_json::from_str(&reply_line).map_err(|e| format!("invalid JSON from Ghidra bridge: {e}"))
}

fn ensure_bridge_script(layout: &ProjectLayout) -> Result<()> {
    let up_to_date = fs::read_to_string(&layout.bridge_script_path).ok().is_some_and(|existing| existing == BRIDGE_JAVA_SOURCE);
    if !up_to_date {
        atomic_write(&layout.bridge_script_path, BRIDGE_JAVA_SOURCE)?;
    }
    Ok(())
}

fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("cannot move {} to {}: {e}", tmp.display(), path.display()))
}

fn quote_batch_token(token: &str) -> Result<String> {
    if token.chars().any(|c| matches!(c, '"' | '\r' | '\n' | '\0')) {
        return Err(format!("cannot pass argument containing quotes or control characters to Ghidra: {token:?}"));
    }
    if token.is_empty() || token.chars().any(|c| matches!(c, ' ' | '\t' | '&' | '|' | '<' | '>' | '^' | '(' | ')' | '%')) {
        Ok(format!("\"{token}\""))
    } else {
        Ok(token.to_string())
    }
}

fn build_cmd_raw_args(analyze_headless: &Path, args: &[String]) -> Result<String> {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(format!("\"{}\"", analyze_headless.display()));
    for arg in args {
        parts.push(quote_batch_token(arg)?);
    }
    // `/d` disables AutoRun and ensures `%cmdcmdline%`'s 2nd token is not `/c` (avoiding `launch.bat`'s `pause`).
    // `/s` guarantees `cmd.exe` strips only the outer quotes wrapping `inner`.
    let inner = parts.join(" ");
    Ok(format!("/d /s /c \"{inner}\""))
}

fn build_headless_command(toolchain: &Toolchain, args: &[String]) -> Result<Command> {
    let raw = build_cmd_raw_args(&toolchain.analyze_headless, args)?;
    let mut cmd = Command::new("cmd.exe");
    cmd.raw_arg(raw);
    cmd.env("GHIDRA_HEADLESS_MAXMEM", &toolchain.max_mem);
    if let Some(ref java_home) = toolchain.java_home {
        cmd.env("JAVA_HOME", java_home);
        let bin = java_home.join("bin");
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let joined = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&existing))).map_err(|e| format!("invalid PATH: {e}"))?;
        cmd.env("PATH", joined);
    }
    Ok(cmd)
}

fn random_token() -> Result<String> {
    use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let mut bytes = [0u8; 32];
    let status = unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), bytes.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status != 0 {
        return Err(format!("BCryptGenRandom failed: {status:#x}"));
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn parse_image_base_hex(primary: Option<&str>, fallback: &str) -> Result<u64> {
    let raw = primary.unwrap_or(fallback).trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(raw, 16).map_err(|_| format!("invalid image_base_hex {raw:?}"))
}

fn is_process_alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    const STILL_ACTIVE: u32 = 259;
    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok && code == STILL_ACTIVE
    }
}

fn terminate_process(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return Ok(());
        }
        let ok = TerminateProcess(handle, 1) != 0;
        let err = std::io::Error::last_os_error();
        CloseHandle(handle);
        if ok || !is_process_alive(pid) {
            Ok(())
        } else {
            Err(format!("cannot stop pid {pid}: {err}"))
        }
    }
}

fn tail_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(count);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn batch_command_wraps_inner_tokens_with_slash_d_s_c() {
        let raw = build_cmd_raw_args(Path::new("C:\\Program Files\\Ghidra\\analyzeHeadless.bat"), &["C:\\my dir\\proj".into(), "-noanalysis".into()]).unwrap();
        assert_eq!(raw, "/d /s /c \"\"C:\\Program Files\\Ghidra\\analyzeHeadless.bat\" \"C:\\my dir\\proj\" -noanalysis\"");
        assert!(quote_batch_token("bad\"arg").is_err());
    }

    #[test]
    fn daemon_cmd_line_avoids_shell_redirection_metacharacters() {
        let cmd = build_daemon_cmd_line(
            "C:\\Windows\\System32\\cmd.exe",
            Path::new("D:\\Tools\\Ghidra\\support\\analyzeHeadless.bat"),
            &["C:\\cache dir\\project".into(), "harmony".into(), "-noanalysis".into()],
        )
        .unwrap();
        assert_eq!(
            cmd,
            "\"C:\\Windows\\System32\\cmd.exe\" /d /s /c \"\"D:\\Tools\\Ghidra\\support\\analyzeHeadless.bat\" \"C:\\cache dir\\project\" harmony -noanalysis\""
        );
    }

    #[test]
    fn unicode_env_block_deduplicates_case_insensitively_and_double_nuls() {
        let tc = Toolchain {
            ghidra_dir: PathBuf::from("D:\\ghidra"),
            analyze_headless: PathBuf::from("D:\\ghidra\\support\\analyzeHeadless.bat"),
            java_home: Some(PathBuf::from("D:\\jdk")),
            max_mem: "6G".into(),
        };
        let base = vec![
            (OsString::from("Path"), OsString::from("C:\\Windows\\System32")),
            (OsString::from("java_home"), OsString::from("C:\\old_jdk")),
            (OsString::from("GHIDRA_HEADLESS_MAXMEM"), OsString::from("2G")),
            (OsString::from("FOO"), OsString::from("bar")),
        ];
        let block = build_unicode_env_block(base, &tc).unwrap();
        assert!(block.ends_with(&[0, 0]));
        let text = String::from_utf16_lossy(&block[..block.len() - 2]);
        let entries: Vec<&str> = text.split('\0').collect();
        assert_eq!(entries, ["FOO=bar", "GHIDRA_HEADLESS_MAXMEM=6G", "JAVA_HOME=D:\\jdk", "PATH=D:\\jdk\\bin;C:\\Windows\\System32"]);
    }
}
