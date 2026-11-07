//! What of its interface the game leaves as it lets a part of it go, at
//! every interface rebuild in vanilla too: the map modes' window and the bar
//! of the units selected, which it never kills, and the outliner's entries,
//! which it never deletes.
use super::LettingGo;
use crate::{
    hooks::HookFrame,
    objects::{
        campaign::{classes, map_modes, outliner, unit_view},
        gui::window as gui_window,
        list,
    },
    points::Point,
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// `(map modes' panel)`, `(outliner)`: let the map manager's map modes'
/// panel and its outliner go, which its interface rebuild does before it
/// makes them again, and the framework as the map manager goes.
const LET_MAP_MODES_GO: usize = 0x81de20;
const LET_OUTLINER_GO: usize = 0x81dc30;

/// `(unit view, flags)`: the unit view's deleting destructor, its vtable's
/// first slot, which the interface rebuild calls before it makes it again,
/// and the framework as the map manager goes. `(map manager, window)`:
/// takes a window off the map manager's windows.
const UNIT_VIEW_DELETE: usize = 0x14719b0;
pub(super) const TAKE_WINDOW_OFF: usize = 0x6ecc80;
/// Where the unit view, making its selection's bar again, lets the old one
/// go: off the map manager's windows, then killed.
const SELECTION_BAR_MADE_AGAIN: usize = 0x1471540;

/// The window of the map modes' panel being let go, killed once it is.
static MAP_MODES_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// The map modes' panel is being let go: its window, which the game hides
/// and takes off the map manager's windows without killing it, would stay in
/// the GUI with the map modes' entries; it is kept to be killed once the
/// panel is gone ([`map_modes_gone`]), as the game kills its other windows.
extern "C" fn map_modes_going(frame: *mut HookFrame) {
    crate::guarded("map modes let go", || {
        let m = crate::hooked_memory();
        match m.ptr(unsafe { (*frame).args[0] } + map_modes::WINDOW) {
            Ok(window) => MAP_MODES_WINDOW.store(window, Ordering::Relaxed),
            Err(e) => log!("repair: the map modes' window was not read: {e}"),
        }
    });
}

/// The map modes' panel is gone: its window is killed, and the GUI lets it
/// go with what it holds at its next sweep.
extern "C" fn map_modes_gone(_: *mut HookFrame) {
    crate::guarded("map modes let go", || {
        let window = MAP_MODES_WINDOW.swap(0, Ordering::Relaxed);
        if window == 0 {
            return;
        }
        let m = crate::hooked_memory();
        if let Err(e) = m.vslot(window, gui_window::KILL).and_then(|kill| m.call(kill, &[window])) {
            log!("repair: the map modes' window was not killed: {e}");
        }
    });
}

/// The unit view is being deleted: the bar of the units selected, which its
/// destructor leaves, would stay in the GUI, shown in the next session when
/// units were selected; it goes first, as the unit view lets it go when it
/// makes it again: off the map manager's windows, then killed.
extern "C" fn unit_view_going(frame: *mut HookFrame) {
    crate::guarded("unit view delete", || {
        let m = crate::hooked_memory();
        let view = unsafe { (*frame).args[0] };
        let killed = (|| {
            let bar = m.ptr(view + unit_view::SELECTION_BAR)?;
            if bar == 0 {
                return Ok(());
            }
            m.write(view + unit_view::SELECTION_BAR, &0usize.to_le_bytes())?;
            m.call(m.at(TAKE_WINDOW_OFF), &[m.ptr(view + unit_view::MAP_MANAGER)?, bar])?;
            m.call(m.vslot(bar, gui_window::KILL)?, &[bar])?;
            crate::Result::Ok(())
        })();
        if let Err(e) = killed {
            log!("repair: the unit view's selection bar was not killed: {e}");
        }
    });
}

/// The outliner is being let go: its categories' entries, which the
/// function letting it go leaves (its headers with their windows among
/// them), are deleted first as its refresh deletes them: what shows them is
/// told first, then each goes by its first slot.
extern "C" fn outliner_going(frame: *mut HookFrame) {
    crate::guarded("outliner let go", || {
        let m = crate::hooked_memory();
        let outliner = unsafe { (*frame).args[0] };
        let deleted = (|| {
            let view = m.ptr(outliner + outliner::VIEW)?;
            if view != 0 {
                m.call(m.vslot(view, outliner::VIEW_EMPTIED)?, &[view])?;
            }
            let mut deleted = 0;
            for category in 0..outliner::CATEGORY_COUNT {
                let entries = outliner + outliner::CATEGORIES + category * outliner::CATEGORY_SIZE + outliner::ENTRIES;
                let mut node = m.ptr(entries + list::HEAD)?;
                while node != 0 {
                    let entry = m.ptr(node + list::ITEM)?;
                    if entry != 0 {
                        m.write(node + list::ITEM, &0usize.to_le_bytes())?;
                        let _letting_go = LettingGo::new("outliner's entry");
                        m.call(m.vslot(entry, 0)?, &[entry, 1])?;
                        deleted += 1;
                    }
                    node = m.ptr(node + list::NEXT)?;
                }
            }
            crate::Result::Ok(deleted)
        })();
        match deleted {
            Ok(n) => log!("repair: {n} entries of the outliner were deleted as it went"),
            Err(e) => log!("repair: the outliner's entries were not deleted: {e}"),
        }
    });
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(LET_OUTLINER_GO, "48 89 5c 24 08 57 48 83 ec 20 48 8b d9 48 8b 89 b0 04 00 00", "let the outliner go", 1, Some(outliner_going), None),
        Point::observe(
            LET_MAP_MODES_GO,
            "40 53 48 83 ec 20 48 8b d9 48 8b 49 08 48 83 c1 30",
            "let the map modes' panel go",
            1,
            Some(map_modes_going),
            Some(map_modes_gone),
        )
        .uses(&[map_modes::WINDOW]),
        Point::observe(
            UNIT_VIEW_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8d 05 7f 5b 91 00 8b fa 48 89 01 48 8b d9",
            "unit view deleting destructor",
            2,
            Some(unit_view_going),
            None,
        ),
        Point::method(classes::UNIT_VIEW, 0, UNIT_VIEW_DELETE, "unit view deleting destructor method"),
        // The rebuild deletes the unit view by its first slot.
        Point::keep(0x81cdbcusize, "49 8b 8e a0 5b 00 00 48 85 c9 74 0a 48 8b 01 ba 01 00 00 00 ff 10", "unit view deleted by the rebuild"),
        Point::keep(TAKE_WINDOW_OFF, "40 53 48 83 ec 20 48 8b d9 48 8b 89 90 00 00 00 48 85 c9", "take a window off the map manager's windows"),
        Point::keep(
            SELECTION_BAR_MADE_AGAIN,
            "48 8b 91 a0 01 00 00 48 85 d2 74 18 48 8b cf e8 2c b7 27 ff 48 8b 8b a0 01 00 00 48 8b 01 ff 90 30 01 00 00",
            "selection bar let go",
        )
        .uses(&[gui_window::KILL]),
    ]
}
