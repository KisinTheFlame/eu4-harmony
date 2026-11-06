//! What of the session's interface the game never lets go, which vanilla
//! never meets, as it restarts instead: the popups its message windows
//! hold, let go before an in-process return takes the interface down, and
//! what the map manager's destructor leaves, let go as it goes, and the
//! windows its interface's builds made that nothing killed, killed once it
//! is gone.
use super::LettingGo;
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::{
        campaign::{map_manager as manager_fields, message_window},
        gui::window as gui_window,
        list,
    },
    points::Point,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    Mutex,
};

/// `(map manager)`: the map manager's destructor, which its deleting
/// destructor (its vtable's first slot) calls as the application's idler
/// switch lets it go.
const MAP_MANAGER_DESTRUCTOR: usize = 0x80b570;
/// `(message icon holder)`: empties it of its icons.
const EMPTY_MESSAGE_ICONS: usize = 0xdffca0;
/// The map manager's constructor, which makes part of its interface, and
/// its interface's build, which a load, a new game and an in-game load call
/// (`(map manager)`).
const MAP_MANAGER_CONSTRUCTOR: usize = 0x803270;
const INTERFACE_BUILD: usize = 0x81af10;
/// `(type, ...) -> window`: the GUI's factory makes a window of a type.
const MAKE_WINDOW: usize = 0x155c260;
/// `(window)`: every window's destructor.
const WINDOW_DESTRUCTOR: usize = 0x1546730;
/// Whether a window is killed, u8: its kill sets it, and the GUI's sweep
/// frees it at its next frame.
const KILLED: usize = 0xf4;

/// The session's interface the map manager's destructor leaves, as an
/// in-process return takes the session down
/// (ADR return-to-the-main-menu-in-process).
pub struct SessionInterface {
    manager: usize,
    /// Those of the session's windows open as popups, the end-of-game
    /// summary the player left from say, which the world reset lets go.
    popups: Vec<usize>,
}

/// The map manager an in-process return lets go at the idler switch.
static LEAVING: AtomicUsize = AtomicUsize::new(0);
/// The map manager that return lets go is gone: the windows its builds made
/// are killed ([`map_manager_gone`]).
static GONE: AtomicBool = AtomicBool::new(false);

/// How many of the map manager's constructor and its interface's builds
/// run, and on which thread.
static BUILDING: AtomicU32 = AtomicU32::new(0);
static BUILDING_THREAD: AtomicU32 = AtomicU32::new(0);
/// The windows the map manager's constructor and its interface's builds
/// made that are not gone, each with when it was made, of all the windows
/// they made: a session's panels, made once for each build, which their
/// panels let go without killing them, as vanilla does at every in-game
/// load (docs/research/main-menu.md, "Which windows stay").
static MADE: Mutex<Option<HashMap<usize, u64>>> = Mutex::new(None);
static MADE_COUNT: AtomicU64 = AtomicU64::new(0);
/// The windows killed so since the last world reset.
pub static WINDOWS_KILLED: AtomicUsize = AtomicUsize::new(0);

impl SessionInterface {
    /// Before an in-process return takes the session's interface down. A
    /// message window holds the popup it was made with and never lets it go,
    /// nor does anything else: the popup, its window and the sprites it
    /// shows, which keep counting as shown, would stay for the process. So
    /// the popups of the message windows that are not closed, which the
    /// in-game load marks as closed, go first, unless open, which go with
    /// the other popups.
    pub fn before_reset(m: &Memory, manager: usize) -> crate::Result<Self> {
        let popups = crate::game::questions::items(m, manager + manager_fields::POPUPS).map_err(|e| e.message)?;
        let mut node = m.ptr(manager + manager_fields::MESSAGE_WINDOWS + list::HEAD)?;
        while node != 0 {
            let window = m.ptr(node + list::ITEM)?;
            if window != 0 && m.u8(node + list::REMOVED)? == 0 && m.u8(window + message_window::CLOSING)? == 0 {
                let popup = m.ptr(window + message_window::POPUP)?;
                if popup != 0 && !popups.contains(&popup) {
                    m.write(window + message_window::POPUP, &0usize.to_le_bytes())?;
                    let _letting_go = LettingGo::new("message window's popup");
                    m.call(m.vslot(popup, 0)?, &[popup, 1])?;
                }
            }
            node = m.ptr(node + list::NEXT)?;
        }
        Ok(Self { manager, popups })
    }

    /// After that reset: the interface is let go as the map manager goes
    /// ([`let_session_interface_go`]), but for the popups the reset let go.
    pub fn let_go_with_map_manager(self, m: &Memory) -> crate::Result<()> {
        for field in manager_fields::SESSION_WINDOWS {
            if self.popups.contains(&m.ptr(self.manager + field)?) {
                log!("repair: the reset let the popup at {field:#x} go");
                m.write(self.manager + field, &0usize.to_le_bytes())?;
            }
        }
        LEAVING.store(self.manager, Ordering::Relaxed);
        Ok(())
    }
}

/// The map manager's destructor begins: after an in-process return, what
/// it never lets go of the session's interface goes first
/// ([`let_session_interface_go`]). Not before: until the idler switch has
/// switched the map manager out, it uses that interface.
extern "C" fn map_manager_going(frame: *mut HookFrame) {
    crate::guarded("map manager destructor", || {
        let manager = unsafe { (*frame).args[0] };
        if LEAVING.compare_exchange(manager, 0, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            return;
        }
        let m = crate::hooked_memory();
        match let_session_interface_go(&m, manager) {
            Ok(gone) => log!("repair: {gone} parts of the session's interface the map manager leaves were let go"),
            Err(e) => log!("repair: the session's interface was not let go: {e}"),
        }
        GONE.store(true, Ordering::Relaxed);
    });
}

/// The map manager that return lets go is gone, and every panel of its
/// interface with it: the windows its builds made that nothing killed stay
/// in the GUI, drawn by nothing, held by nothing but their parents. Those
/// are killed, the last made first, as the game kills its windows; a
/// window killed with its parent is not killed again. The GUI's sweep frees
/// them.
extern "C" fn map_manager_gone(_frame: *mut HookFrame) {
    crate::guarded("map manager gone", || {
        if !GONE.swap(false, Ordering::Relaxed) {
            return;
        }
        let made = MADE.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
        let mut windows: Vec<(usize, u64)> = made.into_iter().collect();
        windows.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
        let m = crate::hooked_memory();
        let mut killed = 0;
        for (window, _) in windows {
            let kill = || -> crate::Result<bool> {
                if m.u8(window + KILLED)? != 0 {
                    return Ok(false);
                }
                m.call(m.vslot(window, gui_window::KILL)?, &[window])?;
                Ok(true)
            };
            match kill() {
                Ok(true) => killed += 1,
                Ok(false) => {}
                Err(e) => log!("repair: a window of the session's interface was not killed: {e}"),
            }
        }
        WINDOWS_KILLED.fetch_add(killed, Ordering::Relaxed);
    });
}

extern "C" fn building(_frame: *mut HookFrame) {
    BUILDING_THREAD.store(crate::thread_id(), Ordering::Relaxed);
    BUILDING.fetch_add(1, Ordering::Relaxed);
}

extern "C" fn built(_frame: *mut HookFrame) {
    let _ = BUILDING.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1));
}

extern "C" fn window_made(frame: *mut HookFrame) {
    let h = unsafe { &*frame };
    if h.fault != 0 || h.result == 0 || BUILDING.load(Ordering::Relaxed) == 0 || crate::thread_id() != BUILDING_THREAD.load(Ordering::Relaxed) {
        return;
    }
    let at = MADE_COUNT.fetch_add(1, Ordering::Relaxed);
    MADE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(h.result, at);
}

extern "C" fn window_going(frame: *mut HookFrame) {
    let window = unsafe { (*frame).args[0] };
    if let Some(made) = MADE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        made.remove(&window);
    }
}

/// The map manager lets some of the session's interface go as it goes, and
/// the rest stays in the game's GUI, drawn, updated and under the mouse in
/// the main menu, its buttons calling into the map manager that went: what
/// it never lets go is let go first, the last made first, as the game lets
/// its windows go.
fn let_session_interface_go(m: &Memory, manager: usize) -> crate::Result<usize> {
    let _letting_go = LettingGo::new("session's interface");
    let mut gone = 0;
    let mut take = |field: usize| -> crate::Result<usize> {
        let object = m.ptr(manager + field)?;
        if object != 0 {
            m.write(manager + field, &0usize.to_le_bytes())?;
            gone += 1;
        }
        Ok(object)
    };
    let icons = take(manager_fields::MESSAGE_ICON_HOLDER)?;
    if icons != 0 {
        m.call(m.at(EMPTY_MESSAGE_ICONS), &[icons])?;
        crate::game_heap::delete(m, icons, manager_fields::MESSAGE_ICON_HOLDER_SIZE)?;
    }
    for field in manager_fields::SESSION_WINDOWS.iter().rev() {
        let window = take(*field)?;
        if window != 0 {
            m.call(m.vslot(window, 0)?, &[window, 1])?;
        }
    }
    for (field, let_go) in manager_fields::SESSION_OBJECTS.iter().rev() {
        let object = take(*field)?;
        if object != 0 {
            m.call(m.at(*let_go), &[object])?;
        }
    }
    // Its minimap, which its destructor leaves too.
    let minimap = take(manager_fields::MINIMAP)?;
    if minimap != 0 {
        m.call(m.vslot(minimap, 0)?, &[minimap, 1])?;
    }
    Ok(gone)
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(
            MAP_MANAGER_DESTRUCTOR,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 ec 40 48 8b d9",
            "map manager destructor",
            1,
            Some(map_manager_going),
            Some(map_manager_gone),
        ),
        Point::observe(
            MAP_MANAGER_CONSTRUCTOR,
            "48 8b c4 48 89 58 18 48 89 50 10 48 89 48 08 55 56 57 41 54 41 55 41 56 41 57",
            "map manager constructor",
            6,
            Some(building),
            Some(built),
        ),
        Point::observe(
            INTERFACE_BUILD,
            "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 c0 fa ff ff",
            "map manager interface build",
            1,
            Some(building),
            Some(built),
        ),
        Point::observe(MAKE_WINDOW, "4c 89 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08", "window made", 5, None, Some(window_made)),
        Point::observe(WINDOW_DESTRUCTOR, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", "window destructor", 1, Some(window_going), None),
        Point::keep(0x1547534usize, "c6 81 f4 00 00 00 01", "window killed").uses(&[KILLED]),
        // Its deleting destructor, which the idler switch calls.
        Point::keep(0x80b42fusize, "e8 3c 01 00 00 f6 c3 01 74 0d ba c8 5b 00 00", "map manager deleting destructor"),
        Point::keep(EMPTY_MESSAGE_ICONS, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b f1 48 8d 59 40", "empty message icons"),
        // The rebuild empties the message icons, then makes a holder of this size.
        Point::keep(0x81d542usize, "e8 59 27 5e 00 b9 00 70 00 00", "message icon holder size"),
    ]
}
