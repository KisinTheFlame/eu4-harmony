//! The game's GUI objects: elements, windows, the army panel, the main menu,
//! edit boxes and their editors, the GUI manager, and the diplomatic message
//! icons and the dialogs they open (docs/research/interface-bindings.md,
//! text-input.md, questions-and-offers.md).

/// Globals, by RVA.
pub mod globals {
    /// The GUI manager.
    pub const MANAGER: usize = 0x23494f0;
    /// A different camera suppresses the ordinary scene's input preparation.
    pub const OTHER_CAMERA: usize = 0x233ff4f;
    /// The object whose floats at [`super::scale::ACROSS`] and
    /// [`super::scale::DOWN`] scale GUI units to window pixels.
    pub const SCALE: usize = 0x23494b8;
}

/// Classes, by vtable RVA.
pub mod classes {
    pub const BUTTON: usize = 0x1d58218;
    /// What a window builds the button its definition names by `backGround`
    /// as: a button of its own input (`+0x48`, `0x1571570`) with a button's
    /// enable method and state.
    pub const BACKGROUND: usize = 0x1d954b0;
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
    /// A sprite type (see [`super::sprite_type`]).
    pub const SPRITE_TYPE: usize = 0x1d9cf40;
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
    /// The scene the windows are drawn in, whose size is
    /// [`super::scene::SIZE`].
    pub const SCENE: usize = 0xc0;
    /// Its method that builds a window from its definition, by vtable
    /// offset: [`super::functions::CREATE_WINDOW`].
    pub const CREATE_WINDOW: usize = 0x48;
}

/// The scene, at [`manager::SCENE`].
pub mod scene {
    /// The screen's width and height in GUI units, i32 each.
    pub const SIZE: usize = 0x490;
}

/// What buttons and text boxes share.
pub mod element {
    /// Whether it shows, u8.
    pub const VISIBLE: usize = 0xc5;
    /// Its animations, a vector of entries of 16 bytes: the animation, and
    /// at [`ANIMATION_OWNED`] whether the element deletes it as it goes, u8.
    pub const ANIMATIONS: usize = 0x90;
    pub const ANIMATION_OWNED: usize = 8;
    /// Methods, by vtable offset: its name, as a `std::string*`.
    pub const NAME: usize = 0xa0;
    /// A button's: enable (with a u8), show, hide.
    pub const ENABLE: usize = 0x80;
    pub const SHOW: usize = 0x88;
    pub const HIDE: usize = 0x90;
    /// Its method that takes the mouse's input (see [`super::mouse_input`]).
    pub const INPUT: usize = 0x48;
    /// A button's or an icon's: its place as resolved (into an i64 x, y),
    /// its frame (an i32, from 1; a progress bar's value, 0 to 100), the
    /// country index of a flag (an i32), and its place set by its
    /// orientation (an i64 x | y << 32).
    pub const PLACED: usize = 0x98;
    pub const FRAME: usize = 0xa8;
    pub const INDEX: usize = 0x1b0;
    pub const PLACE: usize = 0x288;
}

pub mod button {
    /// Its click signal (see [`super::signal`]).
    pub const SIGNAL: usize = 0xc8;
    /// Whether it reports right clicks, u8, and the method that sets it.
    pub const RIGHT_CLICK: usize = 0x161;
    pub const SET_RIGHT_CLICK: usize = 0x170;
    /// Whether it reports double clicks, u8, and the method that sets it.
    pub const DOUBLE_CLICK: usize = 0x162;
    pub const SET_DOUBLE_CLICK: usize = 0x160;
    /// Its method that gives a flag its country, by vtable offset: with a
    /// pointer to the country's tag (u64) and the u8s 1 and 0.
    pub const SET_COUNTRY: usize = 0x218;
}

/// The mouse's input as an element takes it, through its method
/// [`element::INPUT`]: what happened ([`mouse_input::PRESSED`],
/// [`mouse_input::RELEASED`], or [`mouse_input::DOUBLE`] for a press that
/// soon follows another), with which button, from which device.
pub mod mouse_input {
    pub const KIND: usize = 0x14;
    pub const BUTTON: usize = 0x18;
    pub const DEVICE: usize = 0x50;
    pub const PRESSED: u32 = 3;
    pub const RELEASED: u32 = 4;
    pub const DOUBLE: u32 = 5;
    /// The right mouse button, and the mouse as the device.
    pub const RIGHT: u32 = 2;
    pub const MOUSE: u32 = 0;
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
/// its functions get, then function pointers. [`CLICKED`] is called with
/// (context, button) through the method [`CLICKED_METHOD`], for a left click
/// and, when [`DOUBLE_CLICKED`] is null, for a double click too (through
/// [`DOUBLE_CLICK_METHOD`]); the right click's with (context, button)
/// through [`RIGHT`].
pub mod callback {
    pub const SIZE: usize = 0x78;
    pub const CONTEXT: usize = 8;
    pub const CLICKED: usize = 0x40;
    pub const DOUBLE_CLICKED: usize = 0x50;
    pub const RIGHT_CLICK: usize = 0x48;
    pub const CLICKED_METHOD: usize = 0x10;
    pub const DOUBLE_CLICK_METHOD: usize = 0x58;
    pub const RIGHT: usize = 0x68;
}

/// The game's input: an object that records SDL's events and one that the
/// characters typed are posted to, by vtable offset of their methods
/// (docs/research/text-input.md §1).
pub mod events {
    pub const RECORD: usize = 0x20;
    pub const POST: usize = 0x18;
}

/// A window, [`classes::WINDOW`].
pub mod window {
    /// Its interface, [`super::classes::WINDOW_GUI`].
    pub const GUI: usize = 0x30;
    /// Whether it is open, u8.
    pub const OPEN: usize = 0xf5;
    /// Its background, the element its definition names by `backGround`,
    /// which is not among its elements by name; null when it has none.
    pub const BACKGROUND: usize = 0x520;
    /// Methods, by vtable offset: the button, text box or icon of a name (a
    /// NUL-terminated string), and the start of its teardown.
    pub const FIND_BUTTON: usize = 0x68;
    pub const FIND_TEXT: usize = 0x98;
    pub const FIND_ICON: usize = 0xc8;
    pub const KILL: usize = 0x130;
    /// The method that answers its [`super::list_box`] of a name, given as a
    /// `std::string`, by vtable offset.
    pub const FIND_LIST_BOX: usize = 0x1a8;
    /// The method that answers its subwindow of a name, given as a
    /// `std::string`, by vtable offset.
    pub const FIND_WINDOW: usize = 0x1c0;
    /// Its method that puts it at a place on the screen (an i64 x | y << 32,
    /// GUI units from the screen's top left), by vtable offset; and that
    /// place, where dragging it moves it, i32 x and y.
    pub const SET_POSITION: usize = 0x30;
    pub const POSITION: usize = 0x598;
}

/// A window's interface, [`classes::WINDOW_GUI`], at [`window::GUI`]: its
/// methods that show and hide the window, by vtable offset.
pub mod window_gui {
    pub const SHOW: usize = 0x88;
    pub const HIDE: usize = 0x90;
}

/// A list box, in a window.
pub mod list_box {
    pub const ELEMENT: usize = 8;
    pub const ELEMENT_CLASS: usize = 0x1d90fd8;
    pub const SHOW: usize = 0x1533224;
    pub const HIDE: usize = 0x153320c;
    /// The method that answers where its entries start, by vtable offset:
    /// the address of the first node of a linked list (null when empty),
    /// each node the entry at 0 and the next node at 0x10.
    pub const ENTRIES: usize = 0x58;
    /// Its methods that take a row (a [`super::list_entry`], and a u8 to lay
    /// the rows out) and that deletes every row, by vtable offset.
    pub const ADD: usize = 0x20;
    pub const CLEAR: usize = 0xb8;
}

/// A list box's row, [`SIZE`] bytes of the game's heap, which the list box
/// owns once it has it: the window it is built from, at [`WINDOW`].
pub mod list_entry {
    pub const SIZE: usize = 0x50;
    pub const WINDOW: usize = 0x40;
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
    /// The page it shows, u32: [`FIRST_PAGE`], then the single-player and
    /// multiplayer lobbies, the game's settings and so on.
    pub const PAGE: usize = 0x900;
    pub const FIRST_PAGE: u32 = 0;
    /// The single player setup (`super::super::setup`).
    pub const SETUP_PAGE: u32 = 3;
    /// The pages, an array of pointers by page number, each to the page's
    /// interface.
    pub const PAGES: usize = 0x8e8;
    /// The application, as every idler keeps it, whose idler switch starts
    /// a game.
    pub use crate::objects::application::idler::APPLICATION;
    /// The start of a game was accepted (its start command ran), u8.
    pub const LAUNCH_ACCEPTED: usize = 0x714;
    /// The game starts at the main menu's next frame, u8.
    pub const LAUNCH: usize = 0x715;
    /// The single player setup is entered to continue a save, u8.
    pub const CONTINUING: usize = 0x910;
}

/// A text box, [`classes::TEXT_BOX`].
pub mod text_box {
    /// Its text, a `std::string`.
    pub const TEXT: usize = 0x170;
    /// Its animation, made with it and kept by it alone: on its element's
    /// [`super::element::ANIMATIONS`] while it plays, off them while not.
    pub const ANIMATION: usize = 0x210;
}

/// A texture's transparency mask, which the texture handler keeps by the
/// texture's ID for telling where a sprite can be clicked: a
/// `std::vector<bool>`, its words a vector of u32 (begin, end, capacity) and
/// its length in bits. The texture loader (`0x16c2980`) resizes the mask of
/// the ID it is given to the texture's pixels and writes every bit; a
/// resize keeps the buffer it has when it shrinks.
pub mod transparency_mask {
    pub const WORDS: usize = 0;
    pub const CAPACITY: usize = 0x10;
    pub const BITS: usize = 0x18;
}

/// A sprite type: one of the sprites the `.gfx` files define, made at
/// startup and kept by the process (`spritetype.cpp`, constructor
/// `0x156ba70`), as one of the game's graphics.
pub mod sprite_type {
    /// Its name, a game string (`GFX_…`).
    pub const NAME: usize = 0x10;
    /// The group of graphics it is one of, u32: 1 the main menu's, 3 the
    /// game's; and whether its group loaded it, u8.
    pub const GROUP: usize = 0x40;
    pub const LOADED: usize = 0x44;
    /// Its method that unloads it with its group, by vtable offset.
    pub const UNLOAD: usize = 0x48;
    /// The ID its texture is lent by, i32, -1 while it is not loaded.
    pub const TEXTURE: usize = 0x258;
    /// How many shown sprites show it, u32; and whether its texture is
    /// loaded only once one shows it, u8: the first loads it, the last to
    /// hide unloads it.
    pub const SHOWN: usize = 0x288;
    pub const LAZY: usize = 0x28c;
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
    /// Its own copy of the diplomatic action (`diplomacy::action`).
    pub const ACTION: usize = 0x238;
    /// How long one about an action takes an answer: up to this many months
    /// after the action was made (`diplomacy::action::MADE`), as
    /// its check ([`super::dialog::VALID`]) adds them with the game's own
    /// adding of months (0x14c8030); after that the check declines it.
    pub const ANSWER_MONTHS: usize = 2;
}

/// An edit box, [`classes::EDIT_BOX`].
pub mod edit_box {
    /// Its keyboard listener interface, [`super::classes::EDIT_BOX_LISTENER`].
    pub const LISTENER: usize = 0x1d0;
    /// Its caret's sprite.
    pub const CARET_SPRITE: usize = 0x1f0;
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
    use crate::points::GameFunction;

    pub const FIND_BUTTON: GameFunction<2> = GameFunction::at(0x1548700);
    pub const FIND_TEXT: GameFunction<2> = GameFunction::at(0x1548d30);
    pub const FIND_ICON: GameFunction<2> = GameFunction::at(0x15492e0);
    pub const ELEMENT_NAME: GameFunction<1> = GameFunction::at(0xfbf480);
    pub const CLICKED: usize = 0x836570;
    pub const DOUBLE_CLICKED: usize = 0x836590;
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
    /// Adds months to a date: `(&date, months)`.
    pub const ADD_MONTHS: GameFunction<2> = GameFunction::at(0x14c8030);
    /// Takes a listener off a signal: `(signal, wrapper)`.
    pub const UNSUBSCRIBE: GameFunction<2> = GameFunction::at(0xfbe370);
    /// Extension windows (docs/research/extension-windows.md,
    /// window-elements.md): reading an interface file again
    /// `(manager, std::string* path)`, which replaces the definitions of the
    /// same names; building a window from its definition
    /// `(manager, std::string* name)`; a window's interface's show and hide;
    /// a window's place; a window's list box by name `(window, std::string*)`.
    pub const READ_DEFINITIONS: GameFunction<2> = GameFunction::at(0x14eb7d0);
    pub const CREATE_WINDOW: GameFunction<2> = GameFunction::at(0x14e9030);
    pub const WINDOW_SHOW: usize = 0x1547d90;
    pub const WINDOW_HIDE: usize = 0x1547b00;
    pub const SET_POSITION: GameFunction<2> = GameFunction::at(0x15472e0);
    pub const FIND_LIST_BOX: GameFunction<2> = GameFunction::at(0x1548420);
    /// `(window, std::string* name, report)`: its subwindow of the name, from
    /// the list it keeps at `+0x958`, or 0; with `report` set, a miss is also
    /// written in the game's error.log.
    pub const FIND_WINDOW: GameFunction<3> = GameFunction::at(0x15481a0);
    /// A button's frame, and the icons' (two classes of them).
    pub const BUTTON_FRAME: usize = 0x15aeda0;
    pub const ICON_FRAMES: [usize; 2] = [0x1585ed0, 0x1572280];
    pub const ICON_SHOWS: [usize; 2] = [0x1585e80, 0x1571d80];
    pub const ICON_HIDES: [usize; 2] = [0x1585eb0, 0x1571da0];
    /// A flag's country, and the index beside it.
    pub const SET_COUNTRY: GameFunction<2> = GameFunction::at(0x10b56b0);
    pub const SET_INDEX: GameFunction<2> = GameFunction::at(0xfaab90);
    /// A row: built `(entry, manager, std::string* window type)`, added
    /// `(list, entry, lay out)`; every row deleted `(list)`.
    pub const LIST_ENTRY: GameFunction<3> = GameFunction::at(0x152df80);
    pub const LIST_ADD: GameFunction<3> = GameFunction::at(0x152e910);
    pub const LIST_CLEAR: GameFunction<1> = GameFunction::at(0x15320f0);
    /// A button's place by its orientation, an icon's as resolved (two
    /// classes of them: an `iconType`'s first), and the place either
    /// resolved.
    pub const BUTTON_PLACE: GameFunction<2> = GameFunction::at(0x15ae930);
    pub const ICON_PLACES: [usize; 2] = [0x1585c80, 0x1571cf0];
    pub const PLACED: GameFunction<2> = GameFunction::at(0x1571dc0);
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
            UNSUBSCRIBE,
            "40 53 48 83 ec 20 48 8b d9 48 8b 49 08 48 85 c9 74 67 48 39 11 74 0f 48 8b 49 10 48 85 c9 75 f2 48 83 c4 20 5b c3 80 7b 1c 00",
            "signal listeners",
        )
        .uses(&[signal::LISTENERS, super::list::NEXT, signal::LISTENERS + super::list::CHANGING]),
        Point::keep(CLICKED, "48 8b 41 40 48 85 c0 74 07 48 8b 49 08 48 ff e0", "callback clicked").uses(&[callback::CLICKED, callback::CONTEXT]),
        // A double click falls back to the click when it has no function of
        // its own.
        Point::keep(DOUBLE_CLICKED, "48 8b 41 50 48 85 c0 74 07 48 8b 49 08 48 ff e0 48 8b 01 48 ff 60 10", "callback double click").uses(&[
            callback::DOUBLE_CLICKED,
            callback::CONTEXT,
            callback::CLICKED_METHOD,
        ]),
        Point::keep(RIGHT_CLICK, "48 8b 41 48 48 85 c0 74 07 48 8b 49 08 48 ff e0", "callback right click").uses(&[callback::RIGHT_CLICK, callback::CONTEXT]),
        // A left click hands each listener the button, the click's first
        // field; a double click, which the game sends in place of a quick
        // second press, hands it the button too, when the button reports
        // double clicks.
        Point::keep(0x1594244usize, "48 8b 02 48 8b 51 08 48 8b c8 4c 8b 00 48 8b 12 41 ff 50 10", "signal click listener").uses(&[callback::CLICKED_METHOD]),
        Point::keep(0x1592d2busize, "83 f8 05 75 75 80 bf 62 01 00 00 00", "button double click").uses(&[button::DOUBLE_CLICK]),
        Point::keep(0x1592d85usize, "48 8b d7 48 8d 8f c8 00 00 00 e8 bc 07 00 00", "button double click signal").uses(&[button::SIGNAL]),
        Point::keep(0x1593590usize, "48 8b 0b 48 8b d7 48 8b 5b 10 48 8b 01 ff 50 58", "signal double click listener").uses(&[callback::DOUBLE_CLICK_METHOD]),
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
        Point::keep(0x15333ebusize, "48 8d 05 e6 e3 85 00 49 89 86 d0 01 00 00", "edit box keyboard listener")
            .uses(&[classes::EDIT_BOX_LISTENER, edit_box::LISTENER]),
        Point::keep(0x1534216usize, "48 81 c1 c8 00 00 00", "edit box editor").uses(&[edit_box::EDITOR]),
        Point::keep(0x1535266usize, "c6 81 60 02 00 00 01", "edit box focus flag").uses(&[edit_box::FOCUSED]),
        Point::keep(0x153427eusize, "48 8b 81 e0 01 00 00 4c 8b 98 04 26 03 00", "edit box caret shift").uses(&[edit_box::SHIFT_HOLDER, edit_box::SHIFT]),
        Point::keep(0x15342adusize, "44 8b bf 8c 02 00 00 44 03 bf 40 02 00 00 44 03 7d 30 45 03 fb", "edit box caret x")
            .uses(&[edit_box::INSET_X[0], edit_box::INSET_X[1]]),
        Point::keep(0x15342c2usize, "8b 87 90 02 00 00 03 87 44 02 00 00 03 45 34", "edit box caret y").uses(&[edit_box::INSET_Y[0], edit_box::INSET_Y[1]]),
        // Editor.
        Point::keep(0x1537020usize, "48 83 c1 30", "editor text").uses(&[editor::TEXT]),
        Point::keep(0x1538e42usize, "66 3b 47 56 75 06 66 3b 47 54", "editor caret line and column").uses(&[editor::LINE, editor::COLUMN]),
        Point::keep(0x15360bcusize, "48 8b 89 98 00 00 00", "editor font").uses(&[editor::FONT]),
        // Classes, where their constructors set them.
        Point::keep(0x10b55fcusize, "48 8d 05 15 2c ca 00 49 89 04 24 48 8d 05 ea 2b ca 00", "button and signal classes")
            .uses(&[classes::BUTTON, classes::SIGNAL]),
        Point::keep(0x1529d66usize, "48 8d 0d a3 5f 86 00", "text box class").uses(&[classes::TEXT_BOX]),
        Point::keep(0x80afc4usize, "48 8d 05 fd d0 4a 01", "callback class").uses(&[classes::CALLBACK]),
        Point::keep(0x1543feausize, "48 8d 05 bf e6 84 00", "window class").uses(&[classes::WINDOW]),
        Point::keep(0x1543ff4usize, "48 8d 05 d5 e9 84 00", "window interface class").uses(&[classes::WINDOW_GUI]),
        Point::keep(0x144dfceusize, "48 8d 05 b3 90 93 00", "army panel class").uses(&[classes::ARMY_PANEL]),
        Point::keep(0x10d24e3usize, "48 8d 05 de 83 c8 00", "main menu class").uses(&[classes::MAIN_MENU]),
        Point::keep(0x15333d7usize, "48 8d 05 7a df 85 00", "edit box class").uses(&[classes::EDIT_BOX]),
        Point::keep(0x15333e1usize, "48 8d 05 d8 e1 85 00", "editor class").uses(&[classes::EDITOR]),
        // Methods the framework calls or compares.
        Point::method(classes::WINDOW, window::FIND_BUTTON, FIND_BUTTON.rva(), "window button method"),
        Point::method(classes::WINDOW, window::FIND_TEXT, FIND_TEXT.rva(), "window text box method"),
        Point::method(classes::WINDOW, window::FIND_ICON, FIND_ICON.rva(), "window icon method"),
        Point::method(classes::BUTTON, element::NAME, ELEMENT_NAME.rva(), "button name method"),
        Point::method(classes::TEXT_BOX, element::NAME, ELEMENT_NAME.rva(), "text box name method"),
        Point::method(classes::CALLBACK, callback::CLICKED_METHOD, CLICKED, "callback clicked method"),
        Point::method(classes::CALLBACK, callback::DOUBLE_CLICK_METHOD, DOUBLE_CLICKED, "callback double click method"),
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
        Point::keep(0x10807a6usize, "48 8b 89 00 1e 00 00 e8 5e c5 66 ff 48 c7 43 20 00 00 00 00 c6 83 40 01 00 00 01", "message icon clicked")
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
        .uses(&[diplomacy_dialog::ACTION, super::diplomacy::action::STATE]),
        // Adding months (docs/research/dates.md), which the offers do as it
        // does: a year for each twelve, then the rest through the months.
        Point::keep(ADD_MONTHS, "85 d2 0f 84 e4 00 00 00 48 89 74 24 20 57 48 83 ec 20 8b fa 48 89 5c 24 30 b8 ab aa aa 2a", "adding months"),
        // A dialog's description: its text box, by name, and the text.
        Point::keep(0x10a286fusize, "48 8b 01 48 8b da 48 8d 15 84 31 bc 00 ff 90 98 00 00 00 48 8b f0 48 8d b8 70 01 00 00", "dialog description")
            .uses(&[window::FIND_TEXT, text_box::TEXT]),
        // The main menu (docs/research/main-menu.md). The continue box: single
        // player, then the setup shown and found among the pages.
        Point::keep(0x10ef62eusize, "c6 80 10 09 00 00 01 48 8b 05 3c 08 25 01 44 89 b0 7c 74 00 00 c7 80 b4 23 00 00 01 00 00 00", "continue box mode")
            .uses(&[main_menu::CONTINUING, campaign::globals::WORLD, campaign::world::MULTIPLAYER, campaign::world::SETUP]),
        Point::keep(0x10ef66eusize, "48 8b 80 e8 08 00 00 48 8b 48 18 4c 8d 61 d8", "continue box setup page")
            .uses(&[main_menu::PAGES, main_menu::SETUP_PAGE as usize * 8]),
        // `-start_tag`: the main menu's frame shows the setup.
        Point::keep(0x10d3a68usize, "41 80 be 30 07 00 00 00 74 0f 41 c6 86 30 07 00 00 00 ba 03 00 00 00", "start tag setup page")
            .uses(&[main_menu::SETUP_PAGE as usize]),
        // A start: accepted, at the next frame, through the idler switch.
        Point::keep(0x10dc29eusize, "c6 83 14 07 00 00 01", "main menu start accepted").uses(&[main_menu::LAUNCH_ACCEPTED]),
        Point::keep(0x10da695usize, "80 bf 15 07 00 00 00", "main menu start").uses(&[main_menu::LAUNCH]),
        Point::keep(0x10daed7usize, "48 8b 8f 30 03 00 00 e8 4d 76 3e 00", "main menu switches idler").uses(&[main_menu::APPLICATION]),
        // Extension windows (docs/research/extension-windows.md,
        // window-elements.md): their definitions read, a window built in the
        // scene, shown, hidden and placed.
        Point::keep(READ_DEFINITIONS, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 48 8d ac 24 f0 f7 ff ff", "reading an interface file"),
        Point::keep(CREATE_WINDOW, "48 89 5c 24 10 57 48 83 ec 40 48 8b f9 48 8b da 48 81 c1 c8 00 00 00", "building a window"),
        Point::keep(0x14e9099usize, "48 8b 97 c0 00 00 00", "window scene").uses(&[manager::SCENE]),
        Point::keep(0x14e90b2usize, "4c 8b 8a 90 04 00 00", "scene size").uses(&[scene::SIZE]),
        // Show/hide entry bytes are owned by window_ordering's observers.
        Point::keep(SET_POSITION, "48 83 ec 28 4c 8b 99 30 0a 00 00 4c 8b c2 48 89 91 98 05 00 00", "window position").uses(&[window::POSITION]),
        Point::keep(FIND_LIST_BOX, "48 83 ec 28 48 83 7a 18 10 48 8b 01", "window list box by name"),
        Point::keep(FIND_WINDOW, "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8d 6c 24 d0", "window subwindow by name"),
        Point::method(classes::WINDOW, window::SET_POSITION, SET_POSITION.rva(), "window position method"),
        Point::method(classes::WINDOW, window::FIND_LIST_BOX, FIND_LIST_BOX.rva(), "window list box method"),
        Point::method(classes::WINDOW, window::FIND_WINDOW, FIND_WINDOW.rva(), "window subwindow method"),
        Point::keep(0x1570eaausize, "48 8d 05 ff 45 82 00 48 89 07", "window background constructor's vtable").uses(&[classes::BACKGROUND]),
        Point::method(classes::BACKGROUND, element::ENABLE, 0xfaab30, "window background enable method"),
        // The root's show takes its background and marks itself open.
        Point::keep(0x1547cefusize, "48 8b 8b 20 05 00 00 c6 83 f5 00 00 00 01", "window background").uses(&[window::BACKGROUND, window::OPEN]),
        Point::method(classes::WINDOW_GUI, window_gui::SHOW, WINDOW_SHOW, "window show method"),
        Point::method(classes::WINDOW_GUI, window_gui::HIDE, WINDOW_HIDE, "window hide method"),
        // Their elements: frames, icons shown and hidden, flags.
        Point::keep(BUTTON_FRAME, "89 91 70 02 00 00 48 8b 89 90 02 00 00 0f bf d2 48 8b 01 48 ff a0 a8 00", "button frame"),
        Point::keep(ICON_FRAMES[0], "48 8b 89 68 01 00 00 48 8b 01 48 ff a0 a8 00 00 00", "icon frame"),
        Point::keep(ICON_FRAMES[1], "40 53 48 83 ec 20 89 91 e4 01 00 00 48 8b d9", "icon box frame"),
        Point::keep(ICON_SHOWS[0], "80 b9 e1 01 00 00 00 75 1f c6 81 c5 00 00 00 01", "icon show"),
        Point::keep(ICON_SHOWS[1], "c6 81 c5 00 00 00 01 48 8b 89 68 01 00 00", "icon box show"),
        Point::keep(ICON_HIDES[0], "c6 81 c5 00 00 00 00 c7 81 94 01 00 00 00 00 00", "icon hide"),
        Point::keep(ICON_HIDES[1], "c6 81 c5 00 00 00 00 48 8b 89 68 01 00 00", "icon box hide"),
        Point::keep(SET_COUNTRY, "48 85 d2 74 5f 48 89 74 24 18 41 56 48 83 ec 30 48 8b 01 48 8b f2", "flag country"),
        Point::keep(SET_INDEX, "89 91 38 01 00 00 c3", "flag index"),
        Point::method(classes::BUTTON, element::FRAME, BUTTON_FRAME, "button frame method"),
        Point::method(classes::BUTTON, button::SET_COUNTRY, SET_COUNTRY.rva(), "button country method"),
        Point::method(classes::BUTTON, element::INDEX, SET_INDEX.rva(), "button index method"),
        // Lists: a row built from its window type, added, every row deleted.
        Point::keep(LIST_ENTRY, "48 89 5c 24 10 48 89 74 24 18 48 89 4c 24 08 57 48 83 ec 20 49 8b f0", "list row"),
        Point::keep(0x152e00eusize, "ff 50 48 90 48 89 43 40", "list row window").uses(&[manager::CREATE_WINDOW, list_entry::WINDOW]),
        Point::keep(LIST_ADD, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 40 45 0f b6 e0", "list add"),
        Point::keep(LIST_CLEAR, "48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 48 8b f1 33 ed 89 a9 88 02", "list clear"),
        // The panel button strip's places.
        Point::keep(BUTTON_PLACE, "48 89 54 24 10 53 48 83 ec 20 8b 91 30 02 00 00 4c 8d 81 34 02 00 00", "button place"),
        Point::keep(ICON_PLACES[0], "48 89 54 24 10 53 48 83 ec 20 80 b9 e3 01 00 00 00 48 8b d9 48 89 91 98", "icon place"),
        Point::keep(ICON_PLACES[1], "48 83 ec 28 48 89 91 78 01 00 00 48 8b 89 68 01 00 00", "icon box place"),
        Point::keep(PLACED, "48 8b 81 78 01 00 00 48 89 02 48 8b c2 c3", "element place"),
        Point::method(classes::BUTTON, element::PLACE, BUTTON_PLACE.rva(), "button place method"),
        Point::method(classes::BUTTON, element::PLACED, PLACED.rva(), "button placed method"),
    ]
}
