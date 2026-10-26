//! Write EU4 Harmony extensions in Rust.
//!
//! An extension is a type implementing [`Extension`]; each method is an event
//! and receives a [`Game`], through which every capability is reached. A
//! `Game` exists only for the duration of a callback, on the game thread.
//! Export the type with [`export_extension!`]:
//!
//! ```ignore
//! use eu4_harmony_sdk::{export_extension, log, Click, Extension, Game};
//!
//! #[derive(Default)]
//! struct Hello;
//! impl Extension for Hello {
//!     fn on_button(&mut self, _game: &mut Game, name: &str, click: Click) {
//!         log!("{name} clicked: {click:?}");
//!     }
//! }
//! export_extension!(Hello::default());
//! ```
//!
//! [`testing::FakeGame`] stands in for the game in unit tests.
pub use eu4_harmony_abi as abi;

mod backend;
mod game;
pub mod testing;

pub use backend::Backend;
pub use game::*;

/// Events an extension reacts to. All default to doing nothing.
#[allow(unused_variables)]
pub trait Extension: 'static {
    /// A campaign was loaded or started; drop state from the previous one,
    /// then read [`Game::save_data`].
    fn on_session(&mut self, game: &mut Game) {}
    /// Every UI frame while a campaign runs, paused or not.
    fn on_frame(&mut self, game: &mut Game) {}
    /// Once per game day, after the day's update and visibility refresh.
    fn on_day(&mut self, game: &mut Game, date: i32) {}
    /// A button declared in this mod's UI fragments was clicked.
    fn on_button(&mut self, game: &mut Game, name: &str, click: Click) {}
    /// The route of an own army was changed by something other than an
    /// extension, usually the player.
    fn on_army_ordered(&mut self, game: &mut Game, army: ArmyId) {}
    /// An own army lost a battle.
    fn on_army_defeated(&mut self, game: &mut Game, army: ArmyId) {}
    /// Another extension took over the map coloring.
    fn on_map_lost(&mut self, game: &mut Game) {}
}

/// Appends a line to this extension's log file.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::__private::log(&format!($($arg)*)) };
}

/// Exports the extension entry point; `$make` builds the extension.
#[macro_export]
macro_rules! export_extension {
    ($make:expr) => {
        #[no_mangle]
        pub unsafe extern "C" fn eu4h_extension_main(
            host: *const $crate::abi::HostApi,
            out: *mut $crate::abi::ExtensionVTable,
        ) -> i32 {
            $crate::__private::entry(host, out, || ::std::boxed::Box::new($make))
        }
    };
}

#[doc(hidden)]
pub mod __private {
    use super::{backend::RealBackend, abi, ArmyId, Click, Extension, Game};
    use std::ffi::c_void;
    use std::sync::OnceLock;

    struct Host {
        host: usize,
        log: unsafe extern "C" fn(*mut c_void, *const u8, usize),
    }
    static HOST: OnceLock<Host> = OnceLock::new();

    pub fn log(message: &str) {
        if let Some(h) = HOST.get() {
            unsafe { (h.log)(h.host as *mut c_void, message.as_ptr(), message.len()) };
        }
    }

    type Boxed = Box<dyn Extension>;

    /// Runs one event with panics contained; a panic stops the extension.
    unsafe fn run(user: *mut c_void, game: *const abi::GameApi, name: &str, f: impl FnOnce(&mut dyn Extension, &mut Game)) -> i32 {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let ext = &mut *(user as *mut Boxed);
            let mut backend = RealBackend::new(game);
            let mut g = Game::new(&mut backend);
            f(ext.as_mut(), &mut g);
        }));
        match outcome {
            Ok(()) => abi::OK,
            Err(panic) => {
                let detail = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                log(&format!("panic in {name}: {detail}"));
                abi::INTERNAL
            }
        }
    }

    unsafe extern "C" fn on_session(user: *mut c_void, game: *const abi::GameApi) -> i32 {
        run(user, game, "on_session", |e, g| e.on_session(g))
    }
    unsafe extern "C" fn on_frame(user: *mut c_void, game: *const abi::GameApi) -> i32 {
        run(user, game, "on_frame", |e, g| e.on_frame(g))
    }
    unsafe extern "C" fn on_day(user: *mut c_void, game: *const abi::GameApi, date: i32) -> i32 {
        run(user, game, "on_day", |e, g| e.on_day(g, date))
    }
    unsafe extern "C" fn on_button(user: *mut c_void, game: *const abi::GameApi, name: *const u8, len: usize, click: u32, modifiers: u32) -> i32 {
        let name = String::from_utf8_lossy(std::slice::from_raw_parts(name, len)).into_owned();
        let click = Click {
            right: click == abi::CLICK_RIGHT,
            shift: modifiers & abi::MODIFIER_SHIFT != 0,
            ctrl: modifiers & abi::MODIFIER_CTRL != 0,
        };
        run(user, game, "on_button", |e, g| e.on_button(g, &name, click))
    }
    unsafe extern "C" fn on_army_ordered(user: *mut c_void, game: *const abi::GameApi, army: u64) -> i32 {
        run(user, game, "on_army_ordered", |e, g| e.on_army_ordered(g, ArmyId(army)))
    }
    unsafe extern "C" fn on_army_defeated(user: *mut c_void, game: *const abi::GameApi, army: u64) -> i32 {
        run(user, game, "on_army_defeated", |e, g| e.on_army_defeated(g, ArmyId(army)))
    }
    unsafe extern "C" fn on_map_lost(user: *mut c_void, game: *const abi::GameApi) -> i32 {
        run(user, game, "on_map_lost", |e, g| e.on_map_lost(g))
    }

    pub unsafe fn entry(host: *const abi::HostApi, out: *mut abi::ExtensionVTable, make: impl FnOnce() -> Boxed) -> i32 {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if host.is_null() || out.is_null() {
                return abi::INVALID_ARGUMENT;
            }
            let h = &*host;
            if h.size < std::mem::size_of::<abi::HostApi>() || h.abi_version != abi::ABI_VERSION {
                return abi::UNAVAILABLE;
            }
            let _ = HOST.set(Host { host: h.host as usize, log: h.log });
            let table = &mut *out;
            if table.size < std::mem::size_of::<abi::ExtensionVTable>() {
                log("the host's extension table is older than this SDK");
                return abi::UNAVAILABLE;
            }
            table.user = Box::into_raw(Box::new(make())) as *mut c_void;
            table.on_session = Some(on_session);
            table.on_frame = Some(on_frame);
            table.on_day = Some(on_day);
            table.on_button = Some(on_button);
            table.on_army_ordered = Some(on_army_ordered);
            table.on_army_defeated = Some(on_army_defeated);
            table.on_map_lost = Some(on_map_lost);
            abi::OK
        }))
        .unwrap_or(abi::INTERNAL)
    }
}
