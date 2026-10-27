//! Notices to the player: problems the framework detects, such as an
//! unsupported executable, an extension that failed and was stopped, or a mod
//! that was skipped. Each kind is shown at most once per session.
//!
//! Notices wait in a queue for the framework's notice window, which shows
//! them one at a time. The window is spliced into the main menu's
//! `country_selection_panel` and the in-game `topbar`, both present for as
//! long as their screen is (docs/research/notice-window.md); `ui` binds it and
//! asks [`current`] what to show. When the window cannot be shown (the
//! executable is not supported, or binding it failed), the queue is flushed to
//! Windows message boxes and later notices go straight there.
//!
//! A notice is a framework localisation key with its values; it is put into
//! words, in the player language, when it is shown.
use crate::{
    lang, loc,
    splice::{self, Addition, Plan},
};
use eu4_harmony_loc::Language;
use std::collections::{HashSet, VecDeque};
use std::sync::{LazyLock, Mutex};

/// Unprefixed names of the window's elements.
pub const TITLE: &str = "notice_title";
pub const TEXT: &str = "notice_text";

/// Deliberately unlike the game's own dialogs (docs/v1.md): the textures are
/// the framework's, under `<game>/eu4-harmony/gfx/` (tools/notice_textures.py).
const SPRITES: &str = r#"spriteTypes = {
	spriteType = {
		name = "GFX_eu4harmonyfw_notice_background"
		texturefile = "eu4-harmony/gfx/notice_background.dds"
	}
	spriteType = {
		name = "GFX_eu4harmonyfw_notice_close"
		texturefile = "eu4-harmony/gfx/notice_close.dds"
		effectFile = "gfx/FX/buttonstate.lua"
		noOfFrames = 1
	}
}
"#;

/// Below the top bar and clear of vanilla's pause banner (y 100..170), centred
/// on the screen the way vanilla's `pause_window` is.
const WINDOW: &str = r#"
		iconType = {
			name = "notice_background"
			spriteType = "GFX_eu4harmonyfw_notice_background"
			position = { x = -220 y = 180 }
			Orientation = "CENTER_UP"
		}
		instantTextBoxType = {
			name = "notice_title"
			position = { x = -206 y = 186 }
			Orientation = "CENTER_UP"
			font = "vic_18"
			maxWidth = 360
			maxHeight = 20
			format = left
			text = ""
		}
		instantTextBoxType = {
			name = "notice_text"
			position = { x = -206 y = 220 }
			Orientation = "CENTER_UP"
			font = "vic_18"
			maxWidth = 412
			maxHeight = 120
			format = left
			text = ""
		}
		guiButtonType = {
			name = "notice_close"
			position = { x = 192 y = 184 }
			Orientation = "CENTER_UP"
			quadTextureSprite = "GFX_eu4harmonyfw_notice_close"
		}
"#;

/// The windows the notice window lives in, and their files.
const HOSTS: &[(&str, &str)] = &[("interface/frontend.gui", "country_selection_panel"), ("interface/topbar.gui", "topbar")];

/// Adds the notice window and its sprites to the splice plan.
pub fn splice(plan: &mut Plan) -> crate::Result<()> {
    for (file, window) in HOSTS {
        splice::add_framework(plan, file, window, WINDOW)?;
    }
    plan.add("interface/core.gfx", Addition::Append(SPRITES.to_string()));
    Ok(())
}

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
    /// The notice window takes them once it is bound.
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

/// The title and text the bound notice window shows now; takes the next
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

/// The notice window cannot be shown this session: shows the current and
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

fn message_box(message: &Message) {
    let text: Vec<u16> = message.words(lang::player()).encode_utf16().chain([0]).collect();
    std::thread::spawn(move || {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND};
        let title: Vec<u16> = "EU4 Harmony\0".encode_utf16().collect();
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONWARNING | MB_SETFOREGROUND) };
    });
}

#[cfg(test)]
mod tests {
    use crate::{pdx, splice::Plan};

    #[test]
    fn window_fragment_renders() {
        let rendered = pdx::render_fragment(super::WINDOW, crate::splice::FRAMEWORK_PREFIX, None).unwrap();
        let names: Vec<_> = rendered.elements.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["notice_background", super::TITLE, super::TEXT, "notice_close"]);
        assert!(rendered.text.contains("\"eu4harmonyfw_notice_close\""));
    }

    #[test]
    fn splice_covers_both_hosts_and_the_sprites() {
        let mut plan = Plan::default();
        super::splice(&mut plan).unwrap();
        assert_eq!(plan.elements.len(), 8);
        assert!(plan.for_file("interface/topbar.gui").is_some());
        assert!(plan.for_file("interface/frontend.gui").is_some());
        assert!(plan.for_file("interface/core.gfx").is_some());
    }

    /// Against the installed game's own files:
    /// `EU4_GAME_DIR=<install dir> cargo test -p eu4-harmony-core -- --ignored`
    #[test]
    #[ignore]
    fn splices_into_the_installed_game() {
        let dir = std::path::PathBuf::from(std::env::var("EU4_GAME_DIR").expect("set EU4_GAME_DIR"));
        let mut plan = Plan::default();
        super::splice(&mut plan).unwrap();
        for file in ["interface/topbar.gui", "interface/frontend.gui", "interface/core.gfx"] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap();
            let out = crate::splice::apply(&text, &plan.for_file(file).unwrap()).unwrap();
            assert!(out.len() > text.len(), "{file} unchanged");
        }
    }
}
