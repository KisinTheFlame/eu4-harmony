//! Notices to the player: problems the framework detects, such as an
//! extension that failed and was stopped, or a mod that was skipped. Each kind
//! is shown at most once per session.
//!
//! Notices wait in a queue for the notice window of the framework UI
//! (`framework_ui`), which shows them one at a time, over the main menu and
//! the game alike, and asks [`current`] what to show. When the framework UI
//! cannot be drawn, the queue is flushed to Windows message boxes and later
//! notices go straight there. A failed startup or a fatal error goes straight
//! to a message box ([`tell_and_wait`]).
//!
//! A notice is a framework localisation key with its values; it is put into
//! words, in the player language, when it is shown.
use crate::{lang, loc};
use eu4_harmony_loc::Language;
use std::collections::{HashSet, VecDeque};
use std::sync::{LazyLock, Mutex};

/// What a notice says: a key of the framework's localisation
/// (`core/localisation/`) and the values of its `$NAME$`s.
#[derive(Clone, Debug)]
pub struct Message {
    key: &'static str,
    args: Vec<(&'static str, String)>,
}

impl Message {
    pub fn new(key: &'static str) -> Self {
        Self { key, args: Vec::new() }
    }

    pub fn arg(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.args.push((name, value.into()));
        self
    }

    fn words(&self, language: Language) -> String {
        let args: Vec<(&str, &str)> = self.args.iter().map(|(n, v)| (*n, v.as_str())).collect();
        loc::framework(language, self.key, &args)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// The notice window takes them.
    Window,
    MessageBox,
}

struct Notices {
    mode: Mode,
    shown: HashSet<String>,
    queue: VecDeque<Message>,
    /// On screen in the notice window until the player closes it.
    current: Option<Message>,
}

static NOTICES: LazyLock<Mutex<Notices>> =
    LazyLock::new(|| Mutex::new(Notices { mode: Mode::Window, shown: HashSet::new(), queue: VecDeque::new(), current: None }));

fn notices() -> std::sync::MutexGuard<'static, Notices> {
    NOTICES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Tells the player about a problem once per `once`, without blocking the game.
pub fn post(once: &str, message: Message) {
    log!("notice: {}", message.words(Language::English).replace('\n', " "));
    let mut n = notices();
    if !n.shown.insert(once.to_string()) {
        return;
    }
    match n.mode {
        Mode::Window => n.queue.push_back(message),
        Mode::MessageBox => message_box(&message),
    }
}

/// The title and text the notice window shows now; takes the next
/// notice from the queue when none is showing.
pub(crate) fn current() -> Option<(String, String)> {
    let mut n = notices();
    if n.mode != Mode::Window {
        return None;
    }
    if n.current.is_none() {
        n.current = n.queue.pop_front();
    }
    let language = lang::player();
    let title = match n.queue.len() {
        0 => Message::new("EU4HARMONY_NOTICE_TITLE"),
        waiting => Message::new("EU4HARMONY_NOTICE_TITLE_MORE").arg("COUNT", waiting.to_string()),
    };
    n.current.as_ref().map(|m| (title.words(language), m.words(language)))
}

/// The player closed the notice window.
pub(crate) fn dismiss() {
    notices().current = None;
}

/// The notice window cannot be drawn this session: shows the current and
/// queued notices in message boxes, and sends later notices there too.
pub(crate) fn fall_back(reason: &str) {
    let mut n = notices();
    if n.mode == Mode::MessageBox {
        return;
    }
    log!("notices fall back to message boxes: {reason}");
    n.mode = Mode::MessageBox;
    let pending: Vec<Message> = n.current.take().into_iter().chain(n.queue.drain(..)).collect();
    pending.iter().for_each(message_box);
}

fn show(message: &Message, style: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_SETFOREGROUND};
    let text: Vec<u16> = message.words(lang::player()).encode_utf16().chain([0]).collect();
    let title: Vec<u16> = "EU4 Harmony\0".encode_utf16().collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_SETFOREGROUND | style) };
}

fn message_box(message: &Message) {
    let message = Message { key: message.key, args: message.args.clone() };
    std::thread::spawn(move || show(&message, windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONWARNING));
}

/// Shows `message` in a message box and waits until the player closes it. For
/// a failed startup and a fatal error, where there is no notice window and no
/// game to keep running alongside it.
pub(crate) fn tell_and_wait(message: &Message) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_TOPMOST};
    log!("notice: {}", message.words(Language::English).replace('\n', " "));
    show(message, MB_ICONERROR | MB_TOPMOST);
}
