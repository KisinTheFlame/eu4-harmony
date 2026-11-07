//! The windows the session's panels and the main menu's pages keep and never
//! kill as they go (docs/research/main-menu.md, "Which windows stay").
//!
//! Each of these owners makes its window, kills the one it had when it makes
//! it again, and leaves it as it goes: its destructor forgets it, or the
//! owner has none and is never deleted. Vanilla leaves them too, once a
//! process or at every in-game load, as it restarts at a return. Each
//! window, with every window and entry in its tree, would stay in the GUI
//! for the process.
//!
//! So each goes with its owner, as its owner kills it when it makes it
//! again: as the owner's destructor begins, or, for those never deleted, as
//! the map manager an in-process return lets go is going. Killing the
//! windows the session's builds made without knowing their owners crashed
//! the game (prototype-log.md, "Which holders keep the windows killed at a
//! return"); here only what owns a window kills it.
use super::{interface_parts::TAKE_WINDOW_OFF, LettingGo};
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::gui::window as gui_window,
    points::{GameFunction, Point},
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The windows killed so since the last world reset, for the log.
static KILLED: AtomicUsize = AtomicUsize::new(0);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset) };

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!("{} windows the session's panels and the main menu's pages left were killed with them", KILLED.swap(0, Ordering::Relaxed))
}

/// Kills the window at `owner + field`, if there is one, and forgets it there.
fn kill_field(m: &Memory, owner: usize, field: usize) -> crate::Result<()> {
    let window = m.ptr(owner + field)?;
    if window != 0 {
        m.write(owner + field, &0usize.to_le_bytes())?;
        kill(m, window)?;
    }
    Ok(())
}

fn kill(m: &Memory, window: usize) -> crate::Result<()> {
    m.call(m.vslot(window, gui_window::KILL)?, &[window])?;
    KILLED.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Runs a repair as an owner goes, logging what failed.
fn going(what: &str, f: impl FnOnce(&Memory) -> crate::Result<()>) {
    crate::guarded(what, || {
        let m = crate::hooked_memory();
        let _letting_go = LettingGo::new("panel's window");
        if let Err(e) = f(&m) {
            log!("repair: {what}: not let go: {e}");
        }
    });
}

fn this(frame: *mut HookFrame) -> usize {
    unsafe { (*frame).args[0] }
}

// The main menu's pages, made again at each main menu shown and deleted as
// it is switched out.

/// `(setup)`: the single player setup's destructor, which deletes its parts
/// but its start panel at [`SETUP_START_PANEL`]. The panel keeps its window
/// (`start_game_dialog`) and the ironman save chooser at
/// [`START_PANEL_IRONMAN_CHOOSER`], which keeps its own
/// (`IronmanSaveSelectDialog`); each one's deleting destructor kills its
/// window, the chooser first, as it holds the panel.
const SETUP_DESTRUCTOR: usize = 0x110cdf0;
const SETUP_START_PANEL: usize = 0x2ca8;
const START_PANEL: usize = 0x1d7ff70;
const START_PANEL_DELETE: GameFunction<2> = GameFunction::at(0x13b68e0);
const START_PANEL_IRONMAN_CHOOSER: usize = 0x298;
const IRONMAN_CHOOSER: usize = 0x1d7ff88;
const IRONMAN_CHOOSER_DELETE: GameFunction<2> = GameFunction::at(0x13b7910);

extern "C" fn setup_going(frame: *mut HookFrame) {
    let setup = this(frame);
    going("the single player setup's start panel", |m| {
        let panel = m.ptr(setup + SETUP_START_PANEL)?;
        if panel == 0 {
            return Ok(());
        }
        if m.ptr(panel)? != m.at(START_PANEL) {
            return Err(format!("{panel:#x} is not a start panel"));
        }
        m.write(setup + SETUP_START_PANEL, &0usize.to_le_bytes())?;
        let chooser = m.ptr(panel + START_PANEL_IRONMAN_CHOOSER)?;
        if chooser != 0 && m.ptr(chooser)? == m.at(IRONMAN_CHOOSER) {
            m.write(panel + START_PANEL_IRONMAN_CHOOSER, &0usize.to_le_bytes())?;
            IRONMAN_CHOOSER_DELETE.call(m, [chooser, 1])?;
            KILLED.fetch_add(1, Ordering::Relaxed);
        }
        START_PANEL_DELETE.call(m, [panel, 1])?;
        KILLED.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
}

/// `(hints, flags)`: the tutorial hints' deleting destructor, of the main
/// menu's tutorial page and of the map manager's, which deletes their
/// entries and not their window (`tutorial_minimized`).
const HINTS_DELETE: usize = 0x140c210;
const HINTS_WINDOW: usize = 0xe0;

extern "C" fn hints_going(frame: *mut HookFrame) {
    let hints = this(frame);
    going("the tutorial hints' window", |m| kill_field(m, hints, HINTS_WINDOW));
}

/// `(main menu)`: the main menu's switching out, which kills its window and
/// deletes its pages, and not its right click menu, which each main menu
/// shown makes again.
const MAIN_MENU_OUT: usize = 0x10d8330;
const MAIN_MENU_RIGHT_CLICK_MENU: usize = 0x7c8;

extern "C" fn main_menu_out(frame: *mut HookFrame) {
    let menu = this(frame);
    going("the main menu's right click menu", |m| kill_field(m, menu, MAIN_MENU_RIGHT_CLICK_MENU));
}

// The map manager's, which its destructor deletes.

/// `(interface, flags)`: the building interface's deleting destructor, which
/// kills its window and frees the vector of its tabs at [`BUILD_TABS`],
/// not the tabs. Its build deletes the tabs it had by their deleting
/// destructors (vtable `+8`) before it makes them again; here they go so as
/// the interface goes. The building tab puts entries it holds within it in
/// the tabs' list box, which deletes what it still holds as it goes: they
/// are taken out of it first, as the tab's own hide takes them out, and the
/// list box forgets the tab it tells.
const BUILD_INTERFACE_DELETE: usize = 0x81dcf0;
const BUILD_TABS: usize = 8;
/// The tabs' sizes, by their place among the tabs, as the build makes them.
const BUILD_TAB_SIZES: [usize; 13] = [0x668, 0x940, 0x790, 0xc0, 0xd0, 0xc0, 0x148, 0x5e0, 0x738, 0x158, 0x168, 0x1e0, 0x130];
const BUILD_TAB_DELETE_SLOT: usize = 8;
const BUILDING_TAB: usize = 2;
const BUILDING_TAB_CLASS: usize = 0x1d68050;
const BUILDING_TAB_DELETE: usize = 0x11ff8e0;
/// A tab's list box (`content_list`), the list of its entries, each node
/// holding an entry's part at [`ENTRY_PART`] in it, and the tab it tells,
/// set through its window part at [`LIST_BOX_PART`].
const TAB_LIST_BOX: usize = 0x20;
const LIST_BOX: usize = 0x1d90748;
const LIST_BOX_ENTRIES: usize = 0xe0;
const LIST_BOX_NODE_NEXT: usize = 0x10;
const ENTRY_PART: usize = 0x38;
const LIST_BOX_DETACH_SLOT: usize = 0x48;
const LIST_BOX_DETACH: GameFunction<1> = GameFunction::at(0x1530640);
const LIST_BOX_PART: usize = 8;
const LIST_BOX_PART_CLASS: usize = 0x1d90840;
const LIST_BOX_TOLD: usize = 0x60;
const SET_TOLD_SLOT: usize = 0x220;
const SET_TOLD: GameFunction<2> = GameFunction::at(0xfbf490);

extern "C" fn build_interface_going(frame: *mut HookFrame) {
    let interface = this(frame);
    going("the building interface's tabs", |m| {
        let begin = m.ptr(interface + BUILD_TABS)?;
        let end = m.ptr(interface + BUILD_TABS + 8)?;
        if begin == 0 || end < begin || (end - begin) / 8 > BUILD_TAB_SIZES.len() {
            return Ok(());
        }
        let count = (end - begin) / 8;
        let mut tabs = Vec::new();
        for (i, size) in BUILD_TAB_SIZES.iter().enumerate().take(count) {
            let tab = m.ptr(begin + i * 8)?;
            if tab != 0 {
                tabs.push((tab, *size));
            }
        }
        let within = |at: usize| tabs.iter().any(|&(tab, size)| (tab..tab + size).contains(&at));
        let building = m.ptr(begin + BUILDING_TAB * 8)?;
        if building != 0 && m.vslot(building, BUILD_TAB_DELETE_SLOT)? == m.at(BUILDING_TAB_DELETE) {
            let list_box = m.ptr(building + TAB_LIST_BOX)?;
            if list_box != 0 && m.vslot(list_box, LIST_BOX_DETACH_SLOT)? == m.at(LIST_BOX_DETACH) {
                let mut node = m.ptr(list_box + LIST_BOX_ENTRIES)?;
                let mut theirs = false;
                while node != 0 && !theirs {
                    theirs = within(m.ptr(node)?.wrapping_sub(ENTRY_PART));
                    node = m.ptr(node + LIST_BOX_NODE_NEXT)?;
                }
                if theirs {
                    LIST_BOX_DETACH.call(m, [list_box])?;
                }
                let part = list_box + LIST_BOX_PART;
                if within(m.ptr(list_box + LIST_BOX_TOLD)?) && m.vslot(part, SET_TOLD_SLOT)? == m.at(SET_TOLD) {
                    SET_TOLD.call(m, [part, 0])?;
                }
            }
        }
        for i in 0..count {
            let tab = m.ptr(begin + i * 8)?;
            if tab != 0 {
                m.write(begin + i * 8, &0usize.to_le_bytes())?;
                m.call(m.vslot(tab, BUILD_TAB_DELETE_SLOT)?, &[tab, 1])?;
                KILLED.fetch_add(1, Ordering::Relaxed);
            }
        }
        m.write(interface + BUILD_TABS + 8, &begin.to_le_bytes())?;
        Ok(())
    });
}

/// `(ledger, flags)`: the ledger's deleting destructor. Its window's kill
/// takes its bookmarks and filters with it, through their list boxes.
const LEDGER_DELETE: usize = 0x118a6f0;
const LEDGER_WINDOW: usize = 0x38;
/// `(goto box, flags)`: the goto box's deleting destructor.
const GOTO_BOX_DELETE: usize = 0x1142e70;
const GOTO_BOX_WINDOW: usize = 0x30;
/// `(trade interface, flags)`: the trade interface's deleting destructor,
/// which kills its window and not the trade company's, which its rebuild
/// kills.
const TRADE_INTERFACE_DELETE: usize = 0x13cd510;
const TRADE_INTERFACE: usize = 0x1d81c18;
const TRADE_COMPANY_WINDOW: usize = 0xb0;
/// `(menu)`: the load and save windows' base destructor (`menu_load`,
/// `menu_save`).
const SAVE_MENU_DESTRUCTOR: usize = 0x1172a90;
const SAVE_MENU_WINDOW: usize = 0x30;

extern "C" fn ledger_going(frame: *mut HookFrame) {
    let ledger = this(frame);
    going("the ledger's window", |m| kill_field(m, ledger, LEDGER_WINDOW));
}

extern "C" fn goto_box_going(frame: *mut HookFrame) {
    let goto_box = this(frame);
    going("the goto box's window", |m| kill_field(m, goto_box, GOTO_BOX_WINDOW));
}

extern "C" fn trade_interface_going(frame: *mut HookFrame) {
    let interface = this(frame);
    going("the trade company's window", |m| {
        if m.ptr(interface)? != m.at(TRADE_INTERFACE) {
            return Ok(());
        }
        kill_field(m, interface, TRADE_COMPANY_WINDOW)
    });
}

extern "C" fn save_menu_going(frame: *mut HookFrame) {
    let menu = this(frame);
    going("the load or save window", |m| kill_field(m, menu, SAVE_MENU_WINDOW));
}

// Deleted at the return's switch, and at every in-game load's rebuild.

/// `(view, flags)`: the domestic military view's deleting destructor, which
/// kills its own window and not the two choosers it makes apart.
const DOMESTIC_VIEW_DELETE: usize = 0xfaeca0;
const DOMESTIC_VIEW: usize = 0x1d47778;
const DOMESTIC_UNIT_SELECT: usize = 0x660;
const DOMESTIC_DOCTRINE_SELECT: usize = 0x668;
/// `(view, flags)`: the government view's deleting destructor, which leaves
/// its reform chooser, a 0x500-byte object with its window at
/// [`REFORM_SELECT_WINDOW`] and its deleting destructor in its second slot.
const GOVERNMENT_VIEW_DELETE: usize = 0xf33470;
const GOVERNMENT_VIEW: usize = 0x1d41a48;
const GOVERNMENT_REFORM_SELECT: usize = 0x500;
const REFORM_SELECT: usize = 0x1d52598;
const REFORM_SELECT_DELETE: GameFunction<2> = GameFunction::at(0x1056d90);
const REFORM_SELECT_DELETE_SLOT: usize = 8;
const REFORM_SELECT_WINDOW: usize = 0x10;
/// `(lobby, flags)`: the in-game lobby's deleting destructor; its window's
/// kill takes the chat's lines with it.
const LOBBY_DELETE: usize = 0x83a2c0;
const LOBBY_WINDOW: usize = 0xc0;
/// `(hot join, flags)`: the hot join's deleting destructor, which hides its
/// lost window and kills neither: its remake kills the lost window, takes
/// the request off the map manager's windows and kills it.
const HOT_JOIN_DELETE: usize = 0x9054b0;
const HOT_JOIN_MAP_MANAGER: usize = 0x50;
const HOT_JOIN_REQUEST: usize = 0xc0;
const HOT_JOIN_LOST: usize = 0xc8;

extern "C" fn domestic_view_going(frame: *mut HookFrame) {
    let view = this(frame);
    going("the domestic view's choosers", |m| {
        if m.ptr(view)? != m.at(DOMESTIC_VIEW) {
            return Ok(());
        }
        kill_field(m, view, DOMESTIC_DOCTRINE_SELECT)?;
        kill_field(m, view, DOMESTIC_UNIT_SELECT)
    });
}

extern "C" fn government_view_going(frame: *mut HookFrame) {
    let view = this(frame);
    going("the government view's reform chooser", |m| {
        if m.ptr(view)? != m.at(GOVERNMENT_VIEW) {
            return Ok(());
        }
        let select = m.ptr(view + GOVERNMENT_REFORM_SELECT)?;
        if select == 0 {
            return Ok(());
        }
        if m.vslot(select, REFORM_SELECT_DELETE_SLOT)? != m.at(REFORM_SELECT_DELETE) {
            return Err(format!("{select:#x} is not a reform chooser"));
        }
        m.write(view + GOVERNMENT_REFORM_SELECT, &0usize.to_le_bytes())?;
        kill_field(m, select, REFORM_SELECT_WINDOW)?;
        REFORM_SELECT_DELETE.call(m, [select, 1])?;
        Ok(())
    });
}

extern "C" fn lobby_going(frame: *mut HookFrame) {
    let lobby = this(frame);
    going("the in-game lobby's window", |m| kill_field(m, lobby, LOBBY_WINDOW));
}

/// The hot join's lost window, which its destructor still hides: killed
/// once it is done.
static HOT_JOIN_LOST_WINDOW: AtomicUsize = AtomicUsize::new(0);

extern "C" fn hot_join_going(frame: *mut HookFrame) {
    let hot_join = this(frame);
    going("the hot join's windows", |m| {
        HOT_JOIN_LOST_WINDOW.store(m.ptr(hot_join + HOT_JOIN_LOST)?, Ordering::Relaxed);
        let request = m.ptr(hot_join + HOT_JOIN_REQUEST)?;
        if request != 0 {
            m.write(hot_join + HOT_JOIN_REQUEST, &0usize.to_le_bytes())?;
            TAKE_WINDOW_OFF.call(m, [m.ptr(hot_join + HOT_JOIN_MAP_MANAGER)?, request])?;
            kill(m, request)?;
        }
        Ok(())
    });
}

extern "C" fn hot_join_gone(_frame: *mut HookFrame) {
    let lost = HOT_JOIN_LOST_WINDOW.swap(0, Ordering::Relaxed);
    if lost != 0 {
        going("the hot join's lost window", |m| kill(m, lost));
    }
}

/// `(province view, flags)`: the province view's deleting destructor, which
/// leaves the colonisation panel at [`PROVINCE_VIEW_COLONISATION`]. That
/// panel's deleting destructor takes its window off the map manager's
/// windows and kills it not, as the province view's rebuild deletes it too.
const PROVINCE_VIEW_DELETE: usize = 0x1343bd0;
const PROVINCE_VIEW: usize = 0x1d79470;
const PROVINCE_VIEW_COLONISATION: usize = 0x2f0;
const COLONISATION_DELETE: GameFunction<2> = GameFunction::at(0xe31150);
const COLONISATION: usize = 0x1d2a028;
const COLONISATION_WINDOW: usize = 0x558;

/// The colonisation panel's window, killed once the panel is gone.
static COLONISATION_WINDOW_GOING: AtomicUsize = AtomicUsize::new(0);

extern "C" fn province_view_going(frame: *mut HookFrame) {
    let view = this(frame);
    going("the province view's colonisation panel", |m| {
        if m.ptr(view)? != m.at(PROVINCE_VIEW) {
            return Ok(());
        }
        let panel = m.ptr(view + PROVINCE_VIEW_COLONISATION)?;
        if panel == 0 {
            return Ok(());
        }
        if m.ptr(panel)? != m.at(COLONISATION) {
            return Err(format!("{panel:#x} is not a colonisation panel"));
        }
        m.write(view + PROVINCE_VIEW_COLONISATION, &0usize.to_le_bytes())?;
        COLONISATION_DELETE.call(m, [panel, 1])?;
        Ok(())
    });
}

extern "C" fn colonisation_going(frame: *mut HookFrame) {
    let panel = this(frame);
    going("the colonisation panel's window", |m| {
        COLONISATION_WINDOW_GOING.store(m.ptr(panel + COLONISATION_WINDOW)?, Ordering::Relaxed);
        Ok(())
    });
}

extern "C" fn colonisation_gone(_frame: *mut HookFrame) {
    let window = COLONISATION_WINDOW_GOING.swap(0, Ordering::Relaxed);
    if window != 0 {
        going("the colonisation panel's window", |m| kill(m, window));
    }
}

/// `(summary, flags)`: the end-of-game summary's deleting destructor, which
/// leaves its statistics at [`SUMMARY_STATS`], whose own deleting
/// destructor kills not their window (`campaign_stats_win`).
const SUMMARY_DELETE: usize = 0x1088cc0;
const SUMMARY: usize = 0x1d562b0;
const SUMMARY_STATS: usize = 0x1b8;
const STATS: usize = 0x1d284c8;
const STATS_DELETE: GameFunction<2> = GameFunction::at(0xe25030);
const STATS_WINDOW: usize = 0x28;

extern "C" fn summary_going(frame: *mut HookFrame) {
    let summary = this(frame);
    going("the summary's statistics", |m| {
        if m.ptr(summary)? != m.at(SUMMARY) {
            return Ok(());
        }
        let stats = m.ptr(summary + SUMMARY_STATS)?;
        if stats == 0 {
            return Ok(());
        }
        if m.ptr(stats)? != m.at(STATS) {
            return Err(format!("{stats:#x} is not a summary's statistics"));
        }
        m.write(summary + SUMMARY_STATS, &0usize.to_le_bytes())?;
        kill_field(m, stats, STATS_WINDOW)?;
        STATS_DELETE.call(m, [stats, 1])?;
        Ok(())
    });
}

// Never deleted, or torn down inline by the map manager's destructor without
// their windows: let go as the map manager an in-process return lets go is
// going.

/// The map manager's: the diplomatic message icons' window
/// (`diplomessageicons_window`, `+0x1410`), which the build makes once and
/// puts among its windows; the country history (`+0x12b8`, 0x88 bytes, its
/// window at `+0`, no destructor) and the papacy (`+0x1298`, 0x570 bytes,
/// its window at `+0`, no destructor), which the build makes once, and the
/// game's own close of each; its constructor's right click menu, a window
/// (`+0x1448`), shortcut picker (`+0x1460`, its window at `+8`) and
/// flagship designer (`+0x1430`, its window at `+0x570`).
const COUNTRY_HISTORY: usize = 0x12b8;
const CLOSE_COUNTRY_HISTORY: GameFunction<1> = GameFunction::at(0x1045640);
const PAPACY: usize = 0x1298;
const CLOSE_PAPACY: GameFunction<1> = GameFunction::at(0x12d0a80);
const RIGHT_CLICK_MENU: usize = 0x1448;
/// The diplomatic message icons' window, among the map manager's windows.
const MESSAGE_ICONS_WINDOW: usize = 0x1410;
const SHORTCUT_PICKER_WINDOW: usize = 0x1468;
const FLAGSHIP_DESIGNER: usize = 0x1430;
const FLAGSHIP_DESIGNER_WINDOW: usize = 0x570;

/// As the map manager an in-process return lets go is going: the last made
/// first. The country history and the papacy are closed first, as the
/// papacy open is listed by the GUI; they stay, without their windows.
pub(super) fn map_manager_going(manager: usize) {
    going("the map manager's panels", |m| {
        for (field, close) in [(COUNTRY_HISTORY, CLOSE_COUNTRY_HISTORY), (PAPACY, CLOSE_PAPACY)] {
            let panel = m.ptr(manager + field)?;
            if panel != 0 && m.ptr(panel)? != 0 {
                close.call(m, [panel])?;
                kill_field(m, panel, 0)?;
            }
        }
        let icons = m.ptr(manager + MESSAGE_ICONS_WINDOW)?;
        if icons != 0 {
            m.write(manager + MESSAGE_ICONS_WINDOW, &0usize.to_le_bytes())?;
            TAKE_WINDOW_OFF.call(m, [manager, icons])?;
            kill(m, icons)?;
        }
        let designer = m.ptr(manager + FLAGSHIP_DESIGNER)?;
        if designer != 0 {
            kill_field(m, designer, FLAGSHIP_DESIGNER_WINDOW)?;
        }
        kill_field(m, manager, RIGHT_CLICK_MENU)?;
        kill_field(m, manager, SHORTCUT_PICKER_WINDOW)
    });
}

pub(super) fn points() -> Vec<Point> {
    vec![
        // The main menu's pages.
        Point::observe(
            SETUP_DESTRUCTOR,
            "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 4c 89 64 24 20 55 41 56 41 57 48 8b ec 48 81 ec 80 00 00 00",
            "single player setup destructor",
            1,
            Some(setup_going),
            None,
        ),
        Point::keep(
            0x110cbe8usize,
            "b9 a0 02 00 00 e8 e2 66 92 00 48 89 85 e8 00 00 00 49 8b d4 48 8b c8 e8 dc 98 2a 00 90 49 89 84 24 a8 2c 00 00",
            "the setup's start panel made",
        )
        .uses(&[SETUP_START_PANEL]),
        Point::keep(0x13b6860usize, "e8 bb 0d 00 00 90 48 89 87 98 02 00 00", "the start panel's ironman save chooser").uses(&[START_PANEL_IRONMAN_CHOOSER]),
        Point::method(START_PANEL, 0, START_PANEL_DELETE.rva(), "start panel deleting destructor"),
        Point::keep(
            START_PANEL_DELETE,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 8b f2 48 8b f9 48 8d 05 75 96 9c 00 48 89 01 48 8b 89 90 02 00",
            "start panel deleting destructor",
        ),
        Point::method(IRONMAN_CHOOSER, 0, IRONMAN_CHOOSER_DELETE.rva(), "ironman save chooser deleting destructor"),
        Point::keep(
            IRONMAN_CHOOSER_DELETE,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 8b f2 48 8b f9 48 8d 05 5d 86 9c 00 48 89 01 48 8b 49 38 48 85",
            "ironman save chooser deleting destructor",
        ),
        Point::observe(
            HINTS_DELETE,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 8b fa 48 8b f1 48 8d 05 2d 6c 97 00 48 89 01 e8 7d 0a 00 00",
            "tutorial hints deleting destructor",
            2,
            Some(hints_going),
            None,
        ),
        // Its loader kills the old window there before it makes a new one.
        Point::keep(0x140cae3usize, "49 8b 8f e0 00 00 00 48 85 c9 74 09 48 8b 01 ff 90 30 01 00 00", "tutorial hints' window killed")
            .uses(&[HINTS_WINDOW, gui_window::KILL]),
        Point::observe(
            MAIN_MENU_OUT,
            "40 53 48 83 ec 20 48 89 6c 24 30 48 8b d9 48 89 7c 24 40 48 8b b9 68 07 00 00",
            "main menu switched out",
            1,
            Some(main_menu_out),
            None,
        ),
        Point::keep(0x10d80abusize, "48 8b cf ff d3 49 89 87 c8 07 00 00", "main menu's right click menu").uses(&[MAIN_MENU_RIGHT_CLICK_MENU]),
        // The map manager's, deleted by its destructor.
        Point::observe(
            BUILD_INTERFACE_DELETE,
            "48 89 5c 24 10 57 48 83 ec 20 8b fa 48 8b d9 48 8d 05 5a 87 54 01 48 89 01 48 8b 41 30 48 85 c0 74 53",
            "building interface deleting destructor",
            2,
            Some(build_interface_going),
            None,
        ),
        // It frees the vector of its tabs, not the tabs.
        Point::keep(0x81ddebusize, "48 8d 4b 08 e8 cc 0c 87 ff 48 8d 05 cd d0 48 01", "building interface's tabs' vector freed").uses(&[BUILD_TABS]),
        // Its build deletes the tabs it had by their vtables' `+8`, and empties the vector.
        Point::keep(0x11c2cf0usize, "48 8b 4e 08 48 8b 46 10 48 2b c1 48 c1 f8 03 85 c0 74 36", "building interface's tabs").uses(&[BUILD_TABS]),
        Point::keep(0x11c2d10usize, "48 8b 4e 08 4c 8b 04 0b 4d 85 c0 74 12 49 8b 00 ba 01 00 00 00 49 8b c8 ff 50 08", "building interface's tabs deleted")
            .uses(&[BUILD_TAB_DELETE_SLOT]),
        Point::keep(0x11c2d39usize, "48 8b c1 4d 8b c4 48 8d 7e 10 48 8d 56 08 48 89 4e 10", "building interface's tabs emptied"),
        // The tabs' sizes, in their order.
        Point::keep(0x11c2dc7usize, "b9 68 06 00 00 e8", "land tab's size"),
        Point::keep(0x11c2decusize, "b9 40 09 00 00 e8", "naval tab's size"),
        Point::keep(0x11c2da1usize, "b9 90 07 00 00 e8", "building tab's size"),
        Point::keep(0x11c2e12usize, "b9 c0 00 00 00 e8", "fourth tab's size"),
        Point::keep(0x11c2f57usize, "48 8b 46 08 4c 89 68 18 b9 d0 00 00 00 e8", "fifth tab's size"),
        Point::keep(0x11c2f85usize, "b9 c0 00 00 00 e8", "sixth tab's size"),
        Point::keep(0x11c30d7usize, "48 8b 46 08 4c 89 68 28 b9 48 01 00 00 e8", "seventh tab's size"),
        Point::keep(0x11c310cusize, "b9 e0 05 00 00 e8", "eighth tab's size"),
        Point::keep(0x11c3132usize, "b9 38 07 00 00 e8", "ninth tab's size"),
        Point::keep(0x11c3158usize, "b9 58 01 00 00 e8", "tenth tab's size"),
        Point::keep(0x11c31a4usize, "b9 68 01 00 00 e8", "eleventh tab's size"),
        Point::keep(0x11c31causize, "b9 e0 01 00 00 e8", "twelfth tab's size"),
        Point::keep(0x11c317eusize, "b9 30 01 00 00 e8", "thirteenth tab's size"),
        Point::method(BUILDING_TAB_CLASS, BUILD_TAB_DELETE_SLOT, BUILDING_TAB_DELETE, "building tab deleting destructor"),
        Point::keep(BUILDING_TAB_DELETE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b f9 48 8d 35 cf 5f a6 00", "building tab deleting destructor"),
        // A tab's list box, and the tab it tells.
        Point::keep(0x11cca45usize, "48 85 ff 74 15 48 89 7b 20 48 8d 4f 08 48 8b 01 48 8b d3 ff 90 20 02 00 00", "tab's list box").uses(&[
            TAB_LIST_BOX,
            LIST_BOX_PART,
            SET_TOLD_SLOT,
        ]),
        Point::keep(0x1530c9fusize, "4c 8b d0 4c 8d 4e 38 33 c0 48 85 f6 49 89 42 10 4c 0f 44 c8 41 88 42 18 4d 89 0a 49 89 7a 08", "list box's entry added")
            .uses(&[ENTRY_PART, LIST_BOX_NODE_NEXT]),
        Point::keep(0x1200c05usize, "48 8b 43 20 48 8b 80 e0 00 00 00 48 85 c0 74 25 41 b8 41 00 00 00", "tab's list box's entries").uses(&[LIST_BOX_ENTRIES]),
        Point::method(LIST_BOX, LIST_BOX_DETACH_SLOT, LIST_BOX_DETACH.rva(), "list box detaches its entries"),
        Point::keep(
            LIST_BOX_DETACH,
            "40 57 48 83 ec 20 48 8b f9 48 81 c1 f8 00 00 00 48 8b 01 ff 50 48 48 8b 8f e0 00 00 00 48 85 c9 74 29",
            "list box detaches its entries",
        ),
        Point::method(LIST_BOX_PART_CLASS, SET_TOLD_SLOT, SET_TOLD.rva(), "list box part sets what it tells"),
        Point::keep(SET_TOLD, "48 89 51 58 c3", "list box part sets what it tells").uses(&[LIST_BOX_TOLD - LIST_BOX_PART]),
        Point::observe(
            TRADE_INTERFACE_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 4c 89 00 00 f6 c3 01 74 0d ba 28 05 00 00",
            "trade interface deleting destructor",
            2,
            Some(trade_interface_going),
            None,
        ),
        Point::method(TRADE_INTERFACE, 0, TRADE_INTERFACE_DELETE, "trade interface deleting destructor method"),
        // Its rebuild kills the trade company's window.
        Point::keep(0x13d1ae2usize, "48 8b 8e b0 00 00 00 48 8b 01 ff 90 30 01 00 00", "trade company's window killed")
            .uses(&[TRADE_COMPANY_WINDOW, gui_window::KILL]),
        Point::observe(
            LEDGER_DELETE,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 8b f2 48 8b d9 48 8d 05",
            "ledger deleting destructor",
            2,
            Some(ledger_going),
            None,
        ),
        Point::keep(0x118a988usize, "48 8b 49 38 48 85 c9 74 0d 48 8b 01 ff 90 30 01 00 00 4c 89 76 38", "ledger's window killed")
            .uses(&[LEDGER_WINDOW, gui_window::KILL]),
        Point::observe(
            GOTO_BOX_DELETE,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 8b ea 48 8b f9",
            "goto box deleting destructor",
            2,
            Some(goto_box_going),
            None,
        ),
        Point::keep(0x1144bc6usize, "48 8b 4b 30 48 85 c9 74 09 48 8b 01 ff 90 30 01 00 00", "goto box's window killed")
            .uses(&[GOTO_BOX_WINDOW, gui_window::KILL]),
        Point::observe(
            SAVE_MENU_DESTRUCTOR,
            "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 81 c1 68 02 00 00",
            "load and save windows' destructor",
            1,
            Some(save_menu_going),
            None,
        ),
        Point::keep(0x1172b29usize, "48 8b 0d c0 69 1d 01 48 8b 01 48 8d 93 68 02 00 00 ff 50 48 48 89 43 30", "load and save windows' window")
            .uses(&[SAVE_MENU_WINDOW]),
        // Deleted at the return's switch and at every rebuild.
        Point::observe(
            DOMESTIC_VIEW_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8d 05 c4 8a d9 00 48 89 01 8b da",
            "domestic view deleting destructor",
            2,
            Some(domestic_view_going),
            None,
        ),
        Point::method(DOMESTIC_VIEW, 0, DOMESTIC_VIEW_DELETE, "domestic view deleting destructor method"),
        Point::keep(0xfaf47eusize, "48 8b ce ff d7 48 89 83 60 06 00 00 48 8b 54 24 38", "domestic view's unit chooser").uses(&[DOMESTIC_UNIT_SELECT]),
        Point::keep(0xfaf607usize, "48 8b ce ff d7 48 89 83 68 06 00 00 48 8b 54 24 38", "domestic view's doctrine chooser").uses(&[DOMESTIC_DOCTRINE_SELECT]),
        Point::observe(
            GOVERNMENT_VIEW_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 2c 00 00 00 f6 c3 01 74 0d ba 70 09 00 00",
            "government view deleting destructor",
            2,
            Some(government_view_going),
            None,
        ),
        Point::method(GOVERNMENT_VIEW, 0, GOVERNMENT_VIEW_DELETE, "government view deleting destructor method"),
        Point::keep(0xf34603usize, "e8 e8 21 12 00 90 48 89 86 00 05 00 00 48 8b 48 18", "government view's reform chooser").uses(&[GOVERNMENT_REFORM_SELECT]),
        Point::keep(0x1056718usize, "48 8b 4a 58 48 8b 01 49 8b d0 ff 50 48 49 89 46 10", "reform chooser's window").uses(&[REFORM_SELECT_WINDOW]),
        Point::method(REFORM_SELECT, REFORM_SELECT_DELETE_SLOT, REFORM_SELECT_DELETE.rva(), "reform chooser deleting destructor"),
        Point::keep(
            REFORM_SELECT_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 8b da 48 81 c1 e0 00 00 00 e8 35 00 00 00",
            "reform chooser deleting destructor",
        ),
        Point::observe(
            LOBBY_DELETE,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48 89 7c 24 20 41 56 48 83 ec 20 8b ea 48 8b d9",
            "in-game lobby deleting destructor",
            2,
            Some(lobby_going),
            None,
        ),
        Point::keep(0x839674usize, "48 8d 54 24 40 48 8b cf ff d3 49 89 87 c0 00 00 00", "in-game lobby's window").uses(&[LOBBY_WINDOW]),
        Point::observe(
            HOT_JOIN_DELETE,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 8b ea 48 8b d9",
            "hot join deleting destructor",
            2,
            Some(hot_join_going),
            Some(hot_join_gone),
        ),
        // Its destructor hides the lost window, so it is killed after.
        Point::keep(0x9054deusize, "48 8b 89 c8 00 00 00 48 83 c1 30 48 8b 01 ff 90 90 00 00 00 48 8b 43 50", "hot join's lost window hidden")
            .uses(&[HOT_JOIN_LOST, HOT_JOIN_MAP_MANAGER]),
        Point::keep(0x908352usize, "48 8b cf ff d3 48 89 86 c0 00 00 00", "hot join's request").uses(&[HOT_JOIN_REQUEST]),
        // Its remake: the lost window killed, the request taken off and killed.
        Point::keep(
            0x907fc0usize,
            "48 89 5c 24 10 48 89 74 24 18 57 48 83 ec 40 48 8b d9 48 8b 89 c0 00 00 00 48 8b 01 ff 90 30 01 00 00",
            "hot join's windows made again",
        ),
        Point::observe(
            PROVINCE_VIEW_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 ec 24 00 00 f6 c3 01 74 0d ba f0 26 00 00",
            "province view deleting destructor",
            2,
            Some(province_view_going),
            None,
        ),
        Point::method(PROVINCE_VIEW, 0, PROVINCE_VIEW_DELETE, "province view deleting destructor method"),
        Point::keep(0x13453a2usize, "e8 b9 b0 ae ff 90 49 89 87 f0 02 00 00", "province view's colonisation panel").uses(&[PROVINCE_VIEW_COLONISATION]),
        Point::observe(
            COLONISATION_DELETE,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 8b fa 48 8b f1 48 8d 05 bd 8e ef 00 48 89 01 48 8b 89 58 05 00 00",
            "colonisation panel deleting destructor",
            2,
            Some(colonisation_going),
            Some(colonisation_gone),
        ),
        Point::method(COLONISATION, 0, COLONISATION_DELETE.rva(), "colonisation panel deleting destructor method"),
        // It takes its window off the map manager's windows, and kills it not.
        Point::keep(0xe31191usize, "48 8b 96 58 05 00 00 48 8b 8e 50 05 00 00 e8 dc ba 8b ff", "colonisation panel's window taken off")
            .uses(&[COLONISATION_WINDOW]),
        Point::observe(
            SUMMARY_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 8b da 48 81 c1 40 03 00 00",
            "summary deleting destructor",
            2,
            Some(summary_going),
            None,
        ),
        Point::method(SUMMARY, 0, SUMMARY_DELETE, "summary deleting destructor method"),
        Point::keep(0x10888c6usize, "ff 50 48 48 8b c8 48 89 47 28 48 8b 00 48 8d 15 3e 08 c3 00 ff 50 68", "summary statistics' window").uses(&[STATS_WINDOW]),
        Point::keep(0x10888eeusize, "90 48 89 be b8 01 00 00", "summary's statistics").uses(&[SUMMARY_STATS]),
        Point::method(STATS, 0, STATS_DELETE.rva(), "summary statistics deleting destructor"),
        Point::keep(STATS_DELETE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 8b fa 48 8b f1 48 81 c1 a8 00 00 00", "summary statistics deleting destructor"),
        // The map manager's, let go as it goes.
        Point::keep(
            0x81d2dcusize,
            "b9 88 00 00 00 e8 ee 5f 21 01 48 89 85 88 05 00 00 48 8b c8 e8 ab 6a 82 00 90 49 89 86 b8 12 00 00",
            "country history made",
        )
        .uses(&[COUNTRY_HISTORY]),
        Point::keep(0x1043e85usize, "ff d3 49 89 06 48 8b 54 24 38 48 83 fa 10 72 2e", "country history's window"),
        Point::keep(
            CLOSE_COUNTRY_HISTORY,
            "40 53 48 83 ec 20 48 8b d9 48 8b 09 48 83 c1 30 48 8b 01 ff 90 90 00 00 00 c6 43 08 00",
            "close the country history",
        ),
        Point::keep(0x81d297usize, "b9 70 05 00 00 e8 33 60 21 01 48 89 85 88 05 00 00 48 8b c8 e8 40 c3 aa 00 90 49 89 86 98 12 00 00", "papacy made")
            .uses(&[PAPACY]),
        Point::keep(0x12c9d84usize, "41 ff d0 48 89 06 48 8b 54 24 38 48 83 fa 10 72", "papacy's window"),
        Point::keep(CLOSE_PAPACY, "40 53 48 83 ec 20 48 8b 05 eb f3 06 01 48 8b d9 48 89 5c 24 30", "close the papacy"),
        // The GUI reload closes the papacy, then kills its window.
        Point::keep(0x82c968usize, "48 8b 0b 48 85 c9 74 0c 48 8b 01 ff 90 30 01 00 00 4c 89 2b", "papacy's window killed"),
        Point::keep(0x81d03dusize, "48 8b cf ff d3 49 89 86 10 14 00 00", "diplomatic message icons' window").uses(&[MESSAGE_ICONS_WINDOW]),
        Point::keep(0x80ad2ausize, "ff d3 48 89 86 48 14 00 00", "right click menu made").uses(&[RIGHT_CLICK_MENU]),
        Point::keep(0x13ab45eusize, "49 8d 53 d8 41 ff d0 48 89 43 08", "shortcut picker's window"),
        Point::keep(0x803bcfusize, "48 8d 8e 60 14 00 00 49 8b d7 e8 02 77 ba 00 90", "shortcut picker made"),
        Point::keep(0x10fa421usize, "48 8b 89 70 05 00 00 48 85 c9 74 09 48 8b 01 ff 90 30 01 00 00", "flagship designer's window killed")
            .uses(&[FLAGSHIP_DESIGNER_WINDOW, gui_window::KILL]),
        Point::keep(0x80b7f8usize, "48 8b b3 30 14 00 00 45 33 ed 48 8d 3d ef d1 4a 01 48 85 f6 0f 84 a1 00 00 00", "flagship designer torn down")
            .uses(&[FLAGSHIP_DESIGNER]),
    ]
}
