//! Graceful-shutdown guard during the `version.dll` file swap and the brief
//! "Harmony has been updated. Restarting…" notice shown right before
//! relaunching `eu4.exe` (ADR auto-update-workshop-proxy).
//!
//! Proxy updates run before `eu4.exe` creates its window or Direct3D device,
//! so there is no game window to guard and no framework UI yet. This module
//! hides the Win32 console handler, shutdown gate, and native topmost notice
//! behind two scoped functions: [`guard_swap`] and [`with_restart_notice`].
use crate::{base::called_by_game, base::lang, base::loc};
use std::{
    sync::{Condvar, LazyLock, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect, GetDC, GetDeviceCaps, ReleaseDC, SelectObject,
            SetBkMode, SetTextColor, UpdateWindow, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
            DT_WORDBREAK, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, HFONT, LOGPIXELSY, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
        },
        System::{
            Console::{SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT},
            LibraryLoader::GetModuleHandleW,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW, GetSystemMetrics, PostMessageW, PostQuitMessage,
            RegisterClassExW, ShowWindow, TranslateMessage, CS_NOCLOSE, MSG, SC_CLOSE, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNA, WM_APP, WM_CLOSE, WM_CREATE,
            WM_ERASEBKGND, WM_PAINT, WM_QUERYENDSESSION, WM_SYSCOMMAND, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
        },
    },
};

/// How long the console control handler waits for the `version.dll` swap to
/// finish before yielding back to Windows.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// How long the "Harmony has been updated. Restarting…" notice stays on screen
/// before launching the replacement `eu4.exe`.
const NOTICE_DURATION: Duration = Duration::from_millis(1000);

const WM_APP_CLOSE: u32 = WM_APP + 1;
const WINDOW_CLASS: &str = "HarmonyProxyRestartingWindow";

/// Harmony's own palette (`framework_ui::widgets`), in Win32 `0x00BBGGRR` COLORREF.
const COLOR_BG: u32 = 0x0026_201c;
const COLOR_TITLE_BAR: u32 = 0x004a_3a2b;
const COLOR_BORDER: u32 = 0x00a8_8a6b;
const COLOR_TEXT: u32 = 0x00e8_e8e8;

struct ShutdownGate {
    state: Mutex<GateState>,
    changed: Condvar,
}

struct GateState {
    active: bool,
    requested: bool,
}

impl ShutdownGate {
    const fn new() -> Self {
        Self { state: Mutex::new(GateState { active: false, requested: false }), changed: Condvar::new() }
    }

    fn begin(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active = true;
        state.requested = false;
    }

    fn is_active(&self) -> bool {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).active
    }

    fn mark_requested(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.requested = true;
        self.changed.notify_all();
    }

    fn finish(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active = false;
        let requested = state.requested;
        self.changed.notify_all();
        requested
    }

    fn request_and_wait(&self, timeout: Duration) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.requested = true;
        self.changed.notify_all();
        let deadline = Instant::now() + timeout;
        while state.active {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            if remaining.is_zero() {
                return false;
            }
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap_or_else(|e| e.into_inner());
            state = next;
        }
        true
    }

    fn wait_unless_requested(&self, duration: Duration) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = Instant::now() + duration;
        while !state.requested {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            if remaining.is_zero() {
                break;
            }
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap_or_else(|e| e.into_inner());
            state = next;
        }
        state.requested
    }
}

static GATE: LazyLock<ShutdownGate> = LazyLock::new(ShutdownGate::new);
static NOTICE_TEXT: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

#[called_by_game]
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    if matches!(ctrl_type, CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT) {
        if GATE.is_active() {
            let _ = GATE.request_and_wait(SHUTDOWN_GRACE);
            return 1;
        }
        GATE.mark_requested();
    }
    0
}

struct ConsoleRegistration(bool);

impl ConsoleRegistration {
    fn install() -> Self {
        Self(unsafe { SetConsoleCtrlHandler(Some(console_ctrl_handler), 1) } != 0)
    }
}

impl Drop for ConsoleRegistration {
    fn drop(&mut self) {
        let _ = GATE.finish();
        if self.0 {
            unsafe { SetConsoleCtrlHandler(Some(console_ctrl_handler), 0) };
        }
    }
}

/// Runs `swap` while holding off graceful console/OS shutdown signals so the
/// `version.dll` replacement can finish cleanly. Returns `(result, shutdown_requested)`.
pub(super) fn guard_swap<T>(swap: impl FnOnce() -> T) -> (T, bool) {
    GATE.begin();
    let _handler = ConsoleRegistration::install();
    let result = swap();
    let shutdown_requested = GATE.finish();
    (result, shutdown_requested)
}

/// Shows the localized "Harmony has been updated. Restarting…" notice (unless
/// this is a hidden start), waits briefly so the player sees it, and runs
/// `after_notice` while the notice remains visible before closing it cleanly.
/// Returns `None` without calling `after_notice` if a shutdown signal arrived
/// during the wait.
pub(super) fn with_restart_notice<T>(after_notice: impl FnOnce() -> T) -> Option<T> {
    if crate::tweaks::hidden::hidden_start() {
        return Some(after_notice());
    }
    let _handler = ConsoleRegistration::install();
    let message = loc::framework(lang::player(), "EU4HARMONY_PROXY_RESTARTING", &[]);
    *NOTICE_TEXT.lock().unwrap_or_else(|e| e.into_inner()) = message;

    let _window = NoticeWindow::spawn();
    if GATE.wait_unless_requested(NOTICE_DURATION) {
        return None;
    }
    Some(after_notice())
}

struct NoticeWindow {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl NoticeWindow {
    fn spawn() -> Option<Self> {
        let ready = std::sync::Arc::new((Mutex::new(None::<isize>), Condvar::new()));
        let ready_worker = ready.clone();
        let thread = std::thread::Builder::new().name("harmony-proxy-restarting".into()).spawn(move || run_notice_window(ready_worker)).ok()?;

        let (lock, cvar) = &*ready;
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        while guard.is_none() {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let (next, _) = cvar.wait_timeout(guard, remaining).unwrap_or_else(|e| e.into_inner());
            guard = next;
        }
        Some(Self { hwnd: guard.unwrap_or(0), thread: Some(thread) })
    }
}

impl Drop for NoticeWindow {
    fn drop(&mut self) {
        if self.hwnd != 0 {
            unsafe { PostMessageW(self.hwnd as HWND, WM_APP_CLOSE, 0, 0) };
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn dpi_scale() -> f32 {
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        if dc.is_null() {
            return 1.0;
        }
        let dpi = GetDeviceCaps(dc, LOGPIXELSY as i32);
        ReleaseDC(std::ptr::null_mut(), dc);
        if dpi > 0 {
            (dpi as f32 / 96.0).clamp(1.0, 3.0)
        } else {
            1.0
        }
    }
}

fn run_notice_window(ready: std::sync::Arc<(Mutex<Option<isize>>, Condvar)>) {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class_name = wide(WINDOW_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_NOCLOSE,
            lpfnWndProc: Some(notice_window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);

        let scale = dpi_scale();
        let width = (420.0 * scale).round() as i32;
        let height = (116.0 * scale).round() as i32;
        let x = ((GetSystemMetrics(SM_CXSCREEN) - width) / 2).max(0);
        let y = ((GetSystemMetrics(SM_CYSCREEN) - height) / 2).max(0);

        let title = wide("Harmony");
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_SHOWNA);
            UpdateWindow(hwnd);
        }

        {
            let (lock, cvar) = &*ready;
            *lock.lock().unwrap_or_else(|e| e.into_inner()) = Some(hwnd as isize);
            cvar.notify_all();
        }
        if hwnd.is_null() {
            return;
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[called_by_game]
unsafe extern "system" fn notice_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => 0,
        WM_ERASEBKGND => 1,
        WM_CLOSE => 0,
        WM_SYSCOMMAND if (wparam & 0xfff0) == SC_CLOSE as usize => 0,
        WM_QUERYENDSESSION => {
            GATE.mark_requested();
            1
        }
        WM_APP_CLOSE => {
            DestroyWindow(hwnd);
            PostQuitMessage(0);
            0
        }
        WM_PAINT => {
            paint_notice_window(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint_notice_window(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let scale = dpi_scale();
    let title_h = (34.0 * scale).round() as i32;
    let pad_x = (20.0 * scale).round() as i32;
    let pad_y = (16.0 * scale).round() as i32;

    let bg_brush = CreateSolidBrush(COLOR_BG);
    let title_brush = CreateSolidBrush(COLOR_TITLE_BAR);
    let border_brush = CreateSolidBrush(COLOR_BORDER);

    FillRect(hdc, &client, bg_brush);
    let title_rect = RECT { left: client.left, top: client.top, right: client.right, bottom: (client.top + title_h).min(client.bottom) };
    FillRect(hdc, &title_rect, title_brush);
    FrameRect(hdc, &client, border_brush);

    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, COLOR_TEXT);

    let font_face = wide("Segoe UI");
    let make_font = |height: f32, weight: u32| -> HFONT {
        CreateFontW(
            -((height * scale).round() as i32),
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            u32::from(DEFAULT_CHARSET),
            u32::from(OUT_DEFAULT_PRECIS),
            u32::from(CLIP_DEFAULT_PRECIS),
            u32::from(CLEARTYPE_QUALITY),
            u32::from(DEFAULT_PITCH | FF_DONTCARE),
            font_face.as_ptr(),
        )
    };
    let title_font = make_font(15.0, FW_SEMIBOLD);
    let body_font = make_font(14.0, FW_NORMAL);

    let old_font = SelectObject(hdc, title_font);
    let mut title_text_rect = RECT { left: title_rect.left + pad_x, top: title_rect.top, right: title_rect.right - pad_x, bottom: title_rect.bottom };
    let title_text = wide("Harmony");
    DrawTextW(hdc, title_text.as_ptr(), (title_text.len() - 1) as i32, &mut title_text_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

    SelectObject(hdc, body_font);
    let message = NOTICE_TEXT.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let body_text = wide(&message);
    let mut body_rect = RECT { left: client.left + pad_x, top: title_rect.bottom + pad_y, right: client.right - pad_x, bottom: client.bottom - pad_y };
    DrawTextW(hdc, body_text.as_ptr(), (body_text.len() - 1) as i32, &mut body_rect, DT_LEFT | DT_WORDBREAK);

    SelectObject(hdc, old_font);
    DeleteObject(title_font);
    DeleteObject(body_font);
    DeleteObject(bg_brush);
    DeleteObject(title_brush);
    DeleteObject(border_brush);
    EndPaint(hwnd, &ps);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A graceful shutdown arriving during the file swap waits until `finish`
    /// completes, and `finish` reports that shutdown was requested.
    #[test]
    fn shutdown_gate_waits_for_active_swap_and_reports_request() {
        let gate = Arc::new(ShutdownGate::new());
        gate.begin();
        let waiter_gate = gate.clone();
        let waiter = std::thread::spawn(move || waiter_gate.request_and_wait(Duration::from_secs(2)));
        std::thread::sleep(Duration::from_millis(30));
        assert!(gate.is_active());
        assert!(gate.finish(), "finish reports the shutdown request");
        assert!(waiter.join().unwrap(), "shutdown handler observes graceful completion");
    }

    /// When a shutdown arrives during the restart notice wait, it wakes early.
    #[test]
    fn notice_wait_wakes_early_on_shutdown_request() {
        let gate = Arc::new(ShutdownGate::new());
        gate.begin();
        assert!(!gate.finish());

        let sleeper = gate.clone();
        let started = Instant::now();
        let handle = std::thread::spawn(move || sleeper.wait_unless_requested(Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(30));
        gate.mark_requested();
        assert!(handle.join().unwrap());
        assert!(started.elapsed() < Duration::from_secs(1), "woke promptly on shutdown");
    }

    /// A shutdown waiter times out cleanly if a swap never finishes.
    #[test]
    fn shutdown_gate_timeout_returns_false_when_still_active() {
        let gate = ShutdownGate::new();
        gate.begin();
        assert!(!gate.request_and_wait(Duration::from_millis(20)));
        assert!(gate.finish());
    }
}
