//! Notices to the player: problems the framework detects, such as an
//! extension that failed and was stopped, or a mod that was skipped. A
//! notice is shown once while the game runs, once in each session, or every
//! time, as whoever posts it says ([`Repeat`]).
//!
//! Notices wait in a queue for the notice window of the framework UI
//! (`framework_ui`), which shows them one at a time, over the main menu and
//! the game alike, and asks [`current`] what to show. When the framework UI
//! cannot be drawn, the queue is flushed to Windows message boxes and later
//! notices go straight there. A failed startup or a fatal error goes straight
//! to a message box ([`tell_and_wait`]). While the game is hidden
//! (`crate::hidden`), no message box is shown: the log has every notice.
//!
//! A notice is a framework localisation key with its values; it is put into
//! words, in the player language, when it is shown.
use crate::{lang, loc};
use eu4_harmony_loc::Language;
use std::collections::{HashMap, VecDeque};
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

/// How often the player is told of notices of one key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Repeat {
    /// Once while the game runs.
    Run,
    /// Once in each session.
    Session,
    /// Every time.
    Always,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// The notice window takes them.
    Window,
    MessageBox,
}

struct Notices {
    mode: Mode,
    /// The keys told of, each with the session it was last told in.
    shown: HashMap<String, u64>,
    /// The notices waiting, each with its number.
    queue: VecDeque<(u64, Message)>,
    /// On screen in the notice window until the player closes it.
    current: Option<(u64, Message)>,
    /// The number the next notice gets.
    next: u64,
}

static NOTICES: LazyLock<Mutex<Notices>> =
    LazyLock::new(|| Mutex::new(Notices { mode: Mode::Window, shown: HashMap::new(), queue: VecDeque::new(), current: None, next: 1 }));

fn notices() -> std::sync::MutexGuard<'static, Notices> {
    NOTICES.lock().unwrap_or_else(|e| e.into_inner())
}

impl Notices {
    /// Whether a notice of `key` posted in the session `session` is told, as
    /// `repeat` says; it then counts as told.
    fn tells(&mut self, key: &str, repeat: Repeat, session: u64) -> bool {
        let told = match (repeat, self.shown.get(key)) {
            (Repeat::Always, _) | (_, None) => true,
            (Repeat::Run, Some(_)) => false,
            (Repeat::Session, Some(&in_session)) => in_session != session,
        };
        if told && repeat != Repeat::Always {
            self.shown.insert(key.to_string(), session);
        }
        told
    }
}

/// Tells the player about a problem, without blocking the game: of the
/// notices of one `key`, as often as `repeat` says.
pub fn post(key: &str, repeat: Repeat, message: Message) {
    log!("notice: {}", message.words(Language::English).replace('\n', " "));
    let session = crate::sessions::number();
    let mut n = notices();
    if !n.tells(key, repeat, session) {
        return;
    }
    match n.mode {
        Mode::Window => {
            let number = n.next;
            n.next += 1;
            n.queue.push_back((number, message));
        }
        Mode::MessageBox => message_box(&message),
    }
}

/// What the notice window shows now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Showing {
    /// Which notice it is: the window stays where the player put it while
    /// this stays the same, though the title changes as more notices wait.
    pub number: u64,
    pub title: String,
    pub text: String,
}

/// What the notice window shows now; takes the next notice from the queue
/// when none is showing.
pub(crate) fn current() -> Option<Showing> {
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
    n.current.as_ref().map(|(number, m)| Showing { number: *number, title: title.words(language), text: m.words(language) })
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
    let pending: Vec<Message> = n.current.take().into_iter().chain(n.queue.drain(..)).map(|(_, m)| m).collect();
    pending.iter().for_each(message_box);
}

fn show(message: &Message, style: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_OK, MB_SETFOREGROUND};
    if crate::hidden::hidden() {
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The notice showing keeps its number while more come and its title
    /// counts them; the next one shown has another.
    #[test]
    fn a_notice_keeps_its_number_while_more_wait() {
        post("test:first", Repeat::Run, Message::new("EU4HARMONY_NOT_LOADED").arg("MOD", "1").arg("DETAIL", "d"));
        let first = current().unwrap();
        post("test:second", Repeat::Run, Message::new("EU4HARMONY_NOT_LOADED").arg("MOD", "2").arg("DETAIL", "d"));
        post("test:second", Repeat::Run, Message::new("EU4HARMONY_NOT_LOADED").arg("MOD", "2").arg("DETAIL", "d"));
        let again = current().unwrap();
        assert_eq!((again.number, &again.text), (first.number, &first.text));
        assert_ne!(again.title, first.title, "the title counts the notice waiting");
        dismiss();
        let second = current().unwrap();
        assert!(second.number > first.number && second.text.contains('2'), "{second:?}");
        dismiss();
        assert_eq!(current(), None);
    }

    /// A notice of a key is told once in the run, once in each session, or
    /// every time.
    #[test]
    fn a_notice_is_told_as_often_as_it_repeats() {
        let mut n = Notices { mode: Mode::Window, shown: HashMap::new(), queue: VecDeque::new(), current: None, next: 1 };
        assert!(n.tells("run", Repeat::Run, 1));
        assert!(!n.tells("run", Repeat::Run, 1));
        assert!(!n.tells("run", Repeat::Run, 2));
        assert!(n.tells("session", Repeat::Session, 1));
        assert!(!n.tells("session", Repeat::Session, 1));
        assert!(n.tells("session", Repeat::Session, 2));
        assert!(n.tells("always", Repeat::Always, 1));
        assert!(n.tells("always", Repeat::Always, 1));
    }
}
