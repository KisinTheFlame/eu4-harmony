//! The game's GUI objects: elements, windows, the army panel, the main menu,
//! edit boxes and their editors, the GUI manager, and the diplomatic message
//! icons and the dialogs they open (docs/research/regional-combat/
//! native-ui-findings.txt, text-input.md, questions-and-offers.md).

/// Globals, by RVA.
pub mod globals {
    /// The GUI manager.
    pub const MANAGER: usize = 0x23494f0;
    /// The object whose floats at [`super::scale::ACROSS`] and
    /// [`super::scale::DOWN`] scale GUI units to window pixels.
    pub const SCALE: usize = 0x23494b8;
}

/// Classes, by vtable RVA.
pub mod classes {
    pub const BUTTON: usize = 0x1d58218;
    pub const TEXT_BOX: usize = 0x1d8fd10;
    /// A button's click signal.
    pub const SIGNAL: usize = 0x1d581f8;
    /// What a signal calls: a [`super::callback`].
    pub const CALLBACK: usize = 0x1cb80c8;
    /// A window built from a `windowType`, and the interface it keeps at
    /// [`super::window::GUI`].
    pub const WINDOW: usize = 0x1d926b0;
    pub const WINDOW_GUI: usize = 0x1d929d0;
    pub const ARMY_PANEL: usize = 0x1d87088;
    pub const MAIN_MENU: usize = 0x1d5a8c8;
    pub const EDIT_BOX: usize = 0x1d91358;
    /// An edit box's keyboard listener, at [`super::edit_box::LISTENER`].
    pub const EDIT_BOX_LISTENER: usize = 0x1d917d8;
    pub const EDITOR: usize = 0x1d915c0;
}

pub mod scale {
    /// f32 each.
    pub const ACROSS: usize = 0x4e0;
    pub const DOWN: usize = 0x4e4;
}

/// The GUI manager, at [`globals::MANAGER`].
pub mod manager {
    /// The keyboard listener that has the keyboard: an edit box's
    /// [`super::edit_box::LISTENER`] once one takes it, or null. The game
    /// may leave it set after the box lets go, so check the box too.
    pub const FOCUSED_LISTENER: usize = 0x210;
}

/// What buttons and text boxes share.
pub mod element {
    /// Whether it shows, u8.
    pub const VISIBLE: usize = 0xc5;
    /// Methods, by vtable offset: its name, as a `std::string*`.
    pub const NAME: usize = 0xa0;
    /// A button's: enable (with a u8), show, hide.
    pub const ENABLE: usize = 0x80;
    pub const SHOW: usize = 0x88;
    pub const HIDE: usize = 0x90;
}

pub mod button {
    /// Its click signal (see [`super::signal`]).
    pub const SIGNAL: usize = 0xc8;
    /// Whether it reports right clicks, u8, and the method that sets it.
    pub const RIGHT_CLICK: usize = 0x161;
    pub const SET_RIGHT_CLICK: usize = 0x170;
}

/// A signal: the callbacks subscribed to it, a list (see
/// [`super::super::list`]) whose `CHANGING` byte is set while it calls them.
pub mod signal {
    pub const LISTENERS: usize = 8;
    /// A pointer after the listeners: null on every button signal the
    /// framework binds; one that is not is refused.
    pub const AFTER_LISTENERS: usize = 0x20;
    /// Methods, by vtable offset.
    pub const SUBSCRIBE: usize = 8;
    pub const UNSUBSCRIBE: usize = 0x10;
}

/// What a signal calls, [`SIZE`] bytes: [`classes::CALLBACK`], the context
/// its functions get, then function pointers; the left click's is called
/// with (context, event) through the method [`LEFT`], the right click's with
/// (context, button) through [`RIGHT`].
pub mod callback {
    pub const SIZE: usize = 0x78;
    pub const CONTEXT: usize = 8;
    pub const LEFT_CLICK: usize = 0x70;
    pub const RIGHT_CLICK: usize = 0x48;
    pub const LEFT: usize = 8;
    pub const RIGHT: usize = 0x68;
}

/// The game's input: an object that records SDL's events and one that the
/// characters typed are posted to, by vtable offset of their methods
/// (docs/research/text-input.md §1).
pub mod events {
    pub const RECORD: usize = 0x20;
    pub const POST: usize = 0x18;
}

/// A click event.
pub mod event {
    /// Set when a callback took the click, u8.
    pub const CONSUMED: usize = 0x10;
}

/// A window, [`classes::WINDOW`].
pub mod window {
    /// Its interface, [`super::classes::WINDOW_GUI`].
    pub const GUI: usize = 0x30;
    /// Whether it is open, u8.
    pub const OPEN: usize = 0xf5;
    /// Methods, by vtable offset: the button, text box or icon of a name (a
    /// NUL-terminated string), and the start of its teardown.
    pub const FIND_BUTTON: usize = 0x68;
    pub const FIND_TEXT: usize = 0x98;
    pub const FIND_ICON: usize = 0xc8;
    pub const KILL: usize = 0x130;
}

pub mod army_panel {
    /// Its second interface, which its rebuild method gets.
    pub const INTERFACE: usize = 0x10;
    /// Its window, and the army it shows.
    pub const WINDOW: usize = 0x38;
    pub const ARMY: usize = 0x40;
    /// The method that gives the army.
    pub const SELECTED_ARMY: usize = 8;
}

pub mod main_menu {
    /// The page it shows, u32: 0 first, then the single-player and
    /// multiplayer lobbies, the game's settings and so on.
    pub const PAGE: usize = 0x900;
}

/// A text box, [`classes::TEXT_BOX`].
pub mod text_box {
    /// Its text, a `std::string`.
    pub const TEXT: usize = 0x170;
}

/// What holds the diplomatic message icons, at the map manager's
/// `campaign::map_manager::MESSAGE_ICON_HOLDER`.
pub mod message_icon_holder {
    /// The icons, a [`super::super::list`] of [`super::message_icon`]s.
    pub const ICONS: usize = 0x4c78;
}

/// A diplomatic message icon: what another country proposed, until the
/// player clicks it and its dialog opens.
pub mod message_icon {
    /// The dialog it opens when clicked; null once it has.
    pub const DIALOG: usize = 0x20;
    /// Nonzero once it is done with and goes, u8.
    pub const DONE: usize = 0x140;
}

/// A dialog: the game's base of every dialog with an agree and a decline
/// button.
pub mod dialog {
    /// Its window.
    pub const WINDOW: usize = 8;
    /// Nonzero once answered, u8: it goes.
    pub const DONE: usize = 0x114;
    /// Its methods, by vtable offset: what its agree and decline buttons
    /// call.
    pub const ACCEPT: usize = 0x50;
    pub const DECLINE: usize = 0x58;
    /// Whether it is still open to an answer, u8, which its update asks every
    /// frame. A diplomacy dialog's lets the offer go when it is not, mostly
    /// by declining it: so the game no longer takes an answer to it.
    pub const VALID: usize = 0x60;
}

/// A diplomacy dialog: a [`dialog`] whose own constructor sets [`MARK`]: a
/// callback object (its class [`super::classes::CALLBACK`], the dialog,
/// [`super::functions::DIPLOMACY_POPUP`]) none other has. Those about a
/// diplomatic action, the offers, accept with
/// [`super::functions::ACTION_DIALOG_ACCEPT`]; three others, the succession
/// war's among them, keep something else at [`ACTION`].
pub mod diplomacy_dialog {
    pub const MARK: usize = 0x1c0;
    pub const MARK_CONTEXT: usize = 0x1c8;
    pub const MARK_FUNCTION: usize = 0x1d0;
    /// Its own copy of the diplomatic action
    /// (`campaign::diplomatic_action`).
    pub const ACTION: usize = 0x238;
    /// How long one about an action takes an answer: up to this many months
    /// after the action was made (`campaign::diplomatic_action::DATE`), as
    /// its check ([`super::dialog::VALID`]) adds them with the game's own
    /// adding of months (0x14c8030); after that the check declines it.
    pub const ANSWER_MONTHS: usize = 2;
}

/// An edit box, [`classes::EDIT_BOX`].
pub mod edit_box {
    /// Its keyboard listener interface, [`super::classes::EDIT_BOX_LISTENER`].
    pub const LISTENER: usize = 0x1d0;
    /// Its text editor (see [`super::editor`]).
    pub const EDITOR: usize = 0xc8;
    /// Whether it has the keyboard, u8.
    pub const FOCUSED: usize = 0x260;
    /// Where UpdateCaret takes the caret's place from, relative to the box's
    /// parent: two insets for x and two for y (i32 each), and at
    /// `[box + SHIFT_HOLDER] + SHIFT` a shift (x, y).
    pub const INSET_X: [usize; 2] = [0x28c, 0x240];
    pub const INSET_Y: [usize; 2] = [0x290, 0x244];
    pub const SHIFT_HOLDER: usize = 0x1e0;
    pub const SHIFT: usize = 0x32604;
    /// The method that gives its place on screen (x, y).
    pub const PLACE: usize = 0xd8;
}

/// A text editor, [`classes::EDITOR`].
pub mod editor {
    /// The text, a `std::string`.
    pub const TEXT: usize = 0x30;
    /// The caret: byte column within its line and line, u16 each.
    pub const COLUMN: usize = 0x54;
    pub const LINE: usize = 0x56;
    /// The font it draws with.
    pub const FONT: usize = 0x98;
    /// Methods, by vtable offset: select one character to the left or the
    /// right, and "the caret moved", which Up and Down call last.
    pub const SELECT_LEFT: usize = 0xd0;
    pub const SELECT_RIGHT: usize = 0xe0;
    pub const CARET_MOVED: usize = 0x208;
}

/// The functions the methods above are, by RVA.
pub mod functions {
    pub const FIND_BUTTON: usize = 0x1548700;
    pub const FIND_TEXT: usize = 0x1548d30;
    pub const FIND_ICON: usize = 0x15492e0;
    pub const ELEMENT_NAME: usize = 0xfbf480;
    pub const LEFT_CLICK: usize = 0x836550;
    pub const RIGHT_CLICK: usize = 0x836530;
    pub const SELECTED_ARMY: usize = 0x129270;
    pub const EDIT_BOX_PLACE: usize = 0x15357a0;
    pub const EDITOR_SELECT_LEFT: usize = 0x1538560;
    pub const EDITOR_SELECT_RIGHT: usize = 0x1538670;
    pub const EDITOR_CARET_MOVED: usize = 0x1535950;
    /// What a diplomacy dialog's popup button calls, kept in its
    /// [`super::diplomacy_dialog::MARK`].
    pub const DIPLOMACY_POPUP: usize = 0x1079f90;
    /// The accept method ([`super::dialog::ACCEPT`]) of a diplomacy dialog
    /// about a diplomatic action: it sends the action's answer.
    pub const ACTION_DIALOG_ACCEPT: usize = 0x1066a40;
}

use super::campaign;
use crate::points::Point;

pub fn points() -> Vec<Point> {
    use self::{functions::*, globals::*, *};
    vec![
        // An edit box scaling its rectangle to pixels.
        Point::keep(
            0x15352edusize,
            "48 8b 0d c4 41 e1 00 48 8d 54 24 48 66 0f 6e 44 24 40 0f 5b c0 f3 0f 59 81 e0 04 00 00 f3 0f 2c c0 66 0f 6e 44 24 44 \
             89 44 24 40 0f 5b c0 f3 0f 59 81 e4 04 00 00 48 8b cf f3 0f 2c c0 89 44 24 44",
            "GUI scale",
        )
        .uses(&[SCALE, scale::ACROSS, scale::DOWN]),
        // An edit box taking the keyboard stores its listener.
        Point::keep(0x153537cusize, "48 8b 05 6d 41 e1 00", "GUI manager").uses(&[MANAGER]),
        Point::keep(0x1535394usize, "48 8d 8f d0 01 00 00 48 89 88 10 02 00 00", "edit box takes the keyboard")
            .uses(&[edit_box::LISTENER, manager::FOCUSED_LISTENER]),
        // The army panel binding its buttons: its window, a button by name,
        // the button's signal, a subscription.
        Point::keep(
            0x144fbb5usize,
            "48 8b 4e 38 48 8b 01 48 8d 15 35 4d 93 00 ff 50 68 48 8d 88 c8 00 00 00 48 8b 01 48 8d 96 a8 08 00 00 ff 50 08",
            "army panel buttons",
        )
        .uses(&[army_panel::WINDOW, window::FIND_BUTTON, button::SIGNAL, signal::SUBSCRIBE]),
        // Unsubscribing: the listeners, and a removal deferred while the
        // signal calls them.
        Point::keep(
            0xfbe370usize,
            "40 53 48 83 ec 20 48 8b d9 48 8b 49 08 48 85 c9 74 67 48 39 11 74 0f 48 8b 49 10 48 85 c9 75 f2 48 83 c4 20 5b c3 80 7b 1c 00",
            "signal listeners",
        )
        .uses(&[signal::LISTENERS, super::list::NEXT, signal::LISTENERS + super::list::CHANGING]),
        Point::keep(LEFT_CLICK, "48 8b 41 70 48 85 c0 74 07 48 8b 49 08 48 ff e0", "callback left click").uses(&[callback::LEFT_CLICK, callback::CONTEXT]),
        Point::keep(RIGHT_CLICK, "48 8b 41 48 48 85 c0 74 07 48 8b 49 08 48 ff e0", "callback right click").uses(&[callback::RIGHT_CLICK, callback::CONTEXT]),
        Point::keep(0x1411cd7usize, "48 8b 03 48 8b cb ff 50 10 c6 47 10 01", "click consumed").uses(&[event::CONSUMED]),
        // A window opening: its interface, then open.
        Point::keep(0x1547cdfusize, "48 8b 41 30 48 83 c1 30 b2 01 ff 90 10 02 00 00 48 8b 8b 20 05 00 00 c6 83 f5 00 00 00 01", "window open")
            .uses(&[window::GUI, window::OPEN]),
        Point::keep(SELECTED_ARMY, "48 8b 41 40 c3", "army panel army").uses(&[army_panel::ARMY]),
        // The panel's rebuild binds its buttons again from the panel, its
        // second interface back by INTERFACE.
        Point::keep(0x1450645usize, "48 8d 4b f0 e8 b2 f4 ff ff", "army panel rebuild"),
        Point::keep(FIND_BUTTON, "49 c7 c0 ff ff ff ff 66 0f 1f 84 00 00 00 00 00", "window button by name"),
        Point::keep(FIND_TEXT, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "window text box by name"),
        Point::keep(FIND_ICON, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "window icon by name"),
        Point::keep(ELEMENT_NAME, "48 8b 89 b0 00 00 00 48 8b 01 48 ff 60 40", "element name"),
        Point::keep(EDIT_BOX_PLACE, "48 89 5c 24 18 57 48 83 ec 30 48 8b d9 0f 29 74", "edit box place"),
        Point::keep(EDITOR_CARET_MOVED, "40 53 48 83 ec 20 48 8d 99 38 ff ff ff 48 8b 89", "editor caret moved"),
        // Edit box.
        Point::keep(0x15333ebusize, "48 8d 05 e6 e3 85 00 49 89 86 d0 01 00 00", "edit box keyboard listener").uses(&[classes::EDIT_BOX_LISTENER, edit_box::LISTENER]),
        Point::keep(0x1534216usize, "48 81 c1 c8 00 00 00", "edit box editor").uses(&[edit_box::EDITOR]),
        Point::keep(0x1535266usize, "c6 81 60 02 00 00 01", "edit box focus flag").uses(&[edit_box::FOCUSED]),
        Point::keep(0x153427eusize, "48 8b 81 e0 01 00 00 4c 8b 98 04 26 03 00", "edit box caret shift").uses(&[edit_box::SHIFT_HOLDER, edit_box::SHIFT]),
        Point::keep(0x15342adusize, "44 8b bf 8c 02 00 00 44 03 bf 40 02 00 00 44 03 7d 30 45 03 fb", "edit box caret x").uses(&[edit_box::INSET_X[0], edit_box::INSET_X[1]]),
        Point::keep(0x15342c2usize, "8b 87 90 02 00 00 03 87 44 02 00 00 03 45 34", "edit box caret y").uses(&[edit_box::INSET_Y[0], edit_box::INSET_Y[1]]),
        // Editor.
        Point::keep(0x1537020usize, "48 83 c1 30", "editor text").uses(&[editor::TEXT]),
        Point::keep(0x1538e42usize, "66 3b 47 56 75 06 66 3b 47 54", "editor caret line and column").uses(&[editor::LINE, editor::COLUMN]),
        Point::keep(0x15360bcusize, "48 8b 89 98 00 00 00", "editor font").uses(&[editor::FONT]),
        // Classes, where their constructors set them.
        Point::keep(0x10b55fcusize, "48 8d 05 15 2c ca 00 49 89 04 24 48 8d 05 ea 2b ca 00", "button and signal classes").uses(&[classes::BUTTON, classes::SIGNAL]),
        Point::keep(0x1529d66usize, "48 8d 0d a3 5f 86 00", "text box class").uses(&[classes::TEXT_BOX]),
        Point::keep(0x80afc4usize, "48 8d 05 fd d0 4a 01", "callback class").uses(&[classes::CALLBACK]),
        Point::keep(0x1543feausize, "48 8d 05 bf e6 84 00", "window class").uses(&[classes::WINDOW]),
        Point::keep(0x1543ff4usize, "48 8d 05 d5 e9 84 00", "window interface class").uses(&[classes::WINDOW_GUI]),
        Point::keep(0x144dfceusize, "48 8d 05 b3 90 93 00", "army panel class").uses(&[classes::ARMY_PANEL]),
        Point::keep(0x10d24e3usize, "48 8d 05 de 83 c8 00", "main menu class").uses(&[classes::MAIN_MENU]),
        Point::keep(0x15333d7usize, "48 8d 05 7a df 85 00", "edit box class").uses(&[classes::EDIT_BOX]),
        Point::keep(0x15333e1usize, "48 8d 05 d8 e1 85 00", "editor class").uses(&[classes::EDITOR]),
        // Methods the framework calls or compares.
        Point::method(classes::WINDOW, window::FIND_BUTTON, FIND_BUTTON, "window button method"),
        Point::method(classes::WINDOW, window::FIND_TEXT, FIND_TEXT, "window text box method"),
        Point::method(classes::WINDOW, window::FIND_ICON, FIND_ICON, "window icon method"),
        Point::method(classes::BUTTON, element::NAME, ELEMENT_NAME, "button name method"),
        Point::method(classes::TEXT_BOX, element::NAME, ELEMENT_NAME, "text box name method"),
        Point::method(classes::CALLBACK, callback::LEFT, LEFT_CLICK, "callback left click method"),
        Point::method(classes::CALLBACK, callback::RIGHT, RIGHT_CLICK, "callback right click method"),
        Point::method(classes::ARMY_PANEL, army_panel::SELECTED_ARMY, SELECTED_ARMY, "army panel army method"),
        Point::method(classes::EDIT_BOX, edit_box::PLACE, EDIT_BOX_PLACE, "edit box place method"),
        Point::method(classes::EDITOR, editor::SELECT_LEFT, EDITOR_SELECT_LEFT, "editor select left method"),
        Point::method(classes::EDITOR, editor::SELECT_RIGHT, EDITOR_SELECT_RIGHT, "editor select right method"),
        Point::method(classes::EDITOR, editor::CARET_MOVED, EDITOR_CARET_MOVED, "editor caret moved method"),
        // Offers (docs/research/questions-and-offers.md). Switching map modes
        // hides every message icon.
        Point::keep(0x8221aausize, "48 8b 9e 00 14 00 00 48 8b 9b 78 4c 00 00", "message icons")
            .uses(&[campaign::map_manager::MESSAGE_ICON_HOLDER, message_icon_holder::ICONS]),
        // A clicked icon hands its dialog to the open dialogs and is done.
        Point::keep(
            0x10807a6usize,
            "48 8b 89 00 1e 00 00 e8 5e c5 66 ff 48 c7 43 20 00 00 00 00 c6 83 40 01 00 00 01",
            "message icon clicked",
        )
        .uses(&[message_icon::DIALOG, message_icon::DONE]),
        // A dialog's update: its window, and whether it stays.
        Point::keep(0x10a090busize, "48 8b 4b 08", "dialog window").uses(&[dialog::WINDOW]),
        Point::keep(0x10a0941usize, "80 bb 14 01 00 00 00 75 15 48 8b 03 48 8b cb ff 50 60", "dialog done").uses(&[dialog::DONE, dialog::VALID]),
        // What the decline and agree buttons call.
        Point::keep(0x10a0f40usize, "48 8b 01 ff 60 58 cc cc 48 8b 01 ff 60 50", "dialog buttons").uses(&[dialog::DECLINE, dialog::ACCEPT]),
        // The diplomacy dialog's constructor leaves its mark.
        Point::keep(
            0x1066368usize,
            "48 8d 05 59 1d c5 00 48 89 83 c0 01 00 00 48 89 9b c8 01 00 00 48 8d 05 0c 3c 01 00 48 89 83 d0 01 00 00",
            "diplomacy dialog",
        )
        .uses(&[classes::CALLBACK, diplomacy_dialog::MARK, diplomacy_dialog::MARK_CONTEXT, DIPLOMACY_POPUP, diplomacy_dialog::MARK_FUNCTION]),
        // Accepting one about an action: the action, answered.
        Point::keep(
            ACTION_DIALOG_ACCEPT,
            "48 89 5c 24 18 55 56 57 48 83 ec 30 48 8b f9 48 8b b1 38 02 00 00 48 89 74 24 20 8b 6e 34 89 6c 24 28 c7 46 34 02 00 00 00",
            "action dialog accept",
        )
        .uses(&[diplomacy_dialog::ACTION, campaign::diplomatic_action::STATE]),
        // A diplomacy dialog asking whether its action is still unanswered,
        // and whether it was made no more than two months ago: its date, two
        // months on by the game's adding of months, not before today.
        Point::keep(0x1066b5dusize, "48 8b 83 38 02 00 00 83 78 34 00 0f 94 c0", "diplomacy dialog action")
            .uses(&[diplomacy_dialog::ACTION, campaign::diplomatic_action::STATE]),
        Point::keep(
            0x1066b16usize,
            "48 8b 81 38 02 00 00 48 8b d9 48 8d 4c 24 30 8b 50 28 89 54 24 30 ba 02 00 00 00 e8 fa 14 46 00",
            "diplomatic action date",
        )
        .uses(&[diplomacy_dialog::ACTION, campaign::diplomatic_action::DATE, diplomacy_dialog::ANSWER_MONTHS]),
        // Adding months (docs/research/dates.md), which the offers do as it
        // does: a year for each twelve, then the rest through the months.
        Point::keep(0x14c8030usize, "85 d2 0f 84 e4 00 00 00 48 89 74 24 20 57 48 83 ec 20 8b fa 48 89 5c 24 30 b8 ab aa aa 2a", "adding months"),
        // The alliance dialog showing both countries.
        Point::keep(0x1067aacusize, "48 8b 96 38 02 00 00 48 8b 52 20", "diplomatic action recipient")
            .uses(&[diplomacy_dialog::ACTION, campaign::diplomatic_action::TO]),
        // A dialog's description: its text box, by name, and the text.
        Point::keep(0x10a286fusize, "48 8b 01 48 8b da 48 8d 15 84 31 bc 00 ff 90 98 00 00 00 48 8b f0 48 8d b8 70 01 00 00", "dialog description")
            .uses(&[window::FIND_TEXT, text_box::TEXT]),
    ]
}
