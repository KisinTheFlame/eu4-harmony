//! What of the session's interface the game never lets go, which vanilla
//! never meets, as it restarts instead: the popups its message windows
//! hold, let go before an in-process return takes the interface down, and
//! what the map manager's destructor leaves, let go as it goes. The returns
//! it is gone after pass for what its interface may show until then
//! ([`super::waiting`]).
use super::LettingGo;
use crate::{
    hooks::Call,
    memory::Memory,
    objects::{
        campaign::{map_manager as manager_fields, message_window},
        list,
    },
    points::Point,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// `(map manager)`: the map manager's destructor, which its deleting
/// destructor (its vtable's first slot) calls as the application's idler
/// switch lets it go.
const MAP_MANAGER_DESTRUCTOR: usize = 0x80b570;
/// `(message icon holder)`: empties it of its icons.
const EMPTY_MESSAGE_ICONS: usize = 0xdffca0;
/// `(message icon holder)`: its destructor, which the idler switch calls
/// before it frees it.
const MESSAGE_ICON_HOLDER_DESTRUCTOR: usize = 0xdf5180;
/// `(map manager)`: the build of its interface, which an in-game load calls
/// again.
const INTERFACE_BUILD: usize = 0x81af10;
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
/// The map manager that return lets go is gone ([`map_manager_gone`]).
static GONE: AtomicBool = AtomicBool::new(false);

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
fn map_manager_going(call: &mut Call) {
    let manager = call.arg(0);
    if LEAVING.compare_exchange(manager, 0, Ordering::Relaxed, Ordering::Relaxed).is_err() {
        return;
    }
    let m = call.memory();
    match let_session_interface_go(&m, manager) {
        Ok(gone) => log!("repair: {gone} parts of the session's interface the map manager leaves were let go"),
        Err(e) => log!("repair: the session's interface was not let go: {e}"),
    }
    super::panels::map_manager_going(m, manager);
    GONE.store(true, Ordering::Relaxed);
}

/// The map manager that return lets go is gone, and its interface with it:
/// a return has passed for what waits for one ([`super::waiting`]).
///
/// Killing the windows its builds made that nothing killed, which stay in the
/// GUI, was tried (2026-11-06) and taken out: one of the GUI's objects keeps
/// windows of them in its lists, and its destructor, at a later return, called
/// into one the GUI's sweep had freed (a pure call), about once in ten rounds
/// (docs/prototype-log/2026-11-07-windows-killed-at-return-taken-out.md).
fn map_manager_gone(_: &mut Call) {
    if GONE.swap(false, Ordering::Relaxed) {
        super::waiting::Moment::Return.passed();
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
        let_message_icons_go(m, icons)?;
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

/// A message icon holder goes as the idler switch lets it go: emptied of its
/// icons, then destroyed, which lets its alerts' texts go, then freed.
fn let_message_icons_go(m: &Memory, holder: usize) -> crate::Result<()> {
    m.call(m.at(EMPTY_MESSAGE_ICONS), &[holder])?;
    m.call(m.at(MESSAGE_ICON_HOLDER_DESTRUCTOR), &[holder])?;
    crate::game_heap::delete(m, holder, manager_fields::MESSAGE_ICON_HOLDER_SIZE)
}

/// The map manager's interface is built again, at an in-game load: the
/// build empties the message icon holder it made before and makes another
/// in its place, and the one before stayed, about 10 KB with its alerts. So
/// it goes first, and the build, finding none, makes the new one.
fn interface_building(call: &mut Call) {
    let manager = call.arg(0);
    let m = call.memory();
    let let_go = (|| {
        let holder = m.ptr(manager + manager_fields::MESSAGE_ICON_HOLDER)?;
        if holder == 0 {
            return Ok(());
        }
        m.write(manager + manager_fields::MESSAGE_ICON_HOLDER, &0usize.to_le_bytes())?;
        let _letting_go = LettingGo::new("message icon holder");
        let_message_icons_go(&m, holder)
    })();
    if let Err(e) = let_go {
        log!("repair: the message icon holder the build before made was not let go: {e}");
    }
}

/// The repair as the world reset sees it ([`super::REPAIRS`]): it counts
/// nothing.
pub(super) const REPAIR: super::Repair = super::Repair { points, ..super::Repair::NONE };

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
        // Its deleting destructor, which the idler switch calls.
        Point::keep(0x80b42fusize, "e8 3c 01 00 00 f6 c3 01 74 0d ba c8 5b 00 00", "map manager deleting destructor"),
        Point::keep(EMPTY_MESSAGE_ICONS, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b f1 48 8d 59 40", "empty message icons"),
        // The rebuild empties the message icons, then makes a holder of this size.
        Point::keep(0x81d542usize, "e8 59 27 5e 00 b9 00 70 00 00", "message icon holder size"),
        Point::keep(
            MESSAGE_ICON_HOLDER_DESTRUCTOR,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 56 41 57 48 83 ec 40",
            "message icon holder destructor",
        ),
        // The idler switch destroys the holder, then frees it.
        Point::keep(0x81ec3eusize, "48 8b bb 00 14 00 00 48 85 ff 74 15 48 8b cf e8 2e 65 5d 00", "map manager's message icon holder destroyed")
            .uses(&[manager_fields::MESSAGE_ICON_HOLDER]),
        Point::observe(
            INTERFACE_BUILD,
            "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 c0 fa ff ff",
            "map manager interface build",
            1,
            Some(interface_building),
            None,
        ),
        // The build empties the holder there is, if any (MESSAGE_ICON_HOLDER).
        Point::keep(0x81d536usize, "49 8b 8e 00 14 00 00 48 85 c9 74 05", "map manager interface build's message icon holder")
            .uses(&[manager_fields::MESSAGE_ICON_HOLDER]),
    ]
}
