//! Where the game keeps what the Unicode module reads and writes inside its
//! objects (docs/research/unicode-text.md, font-textures.md, text-input.md).
//! Each field is named once here, and [`points`] keeps an instruction of the
//! game's own that uses it, so a build that moved a field fails the startup
//! check instead of the module reading the wrong one.

/// A font (vtable 0x1d99120).
pub mod font {
    /// 256 pointers to glyph records, by byte (the font's own table).
    pub const OWN_GLYPHS: usize = 0x120;
    /// The line height, i32; times [`SCALE`] it is the line in pixels.
    pub const LINE_HEIGHT: usize = 0x960;
    /// f32.
    pub const SCALE: usize = 0x968;
    /// The page's index in the texture manager, i32.
    pub const PAGE_INDEX: usize = 0x970;
    /// The page's width and height, u32 each.
    pub const PAGE_SIZE: usize = 0x978;
    /// The graphics context, which holds the texture manager.
    pub const CONTEXT: usize = 0x48;
}

/// The texture manager, at `[font.CONTEXT] + MANAGER`.
pub mod textures {
    pub const MANAGER: usize = 0x480;
    /// Its entries: a vector (begin, end) of `ENTRY`-byte records.
    pub const ENTRIES: usize = 0x8;
    pub const ENTRY: usize = 0x48;
    /// In an entry: the texture's wrapper first, then at `ENTRY_SIZE` its
    /// width and height (u32 each).
    pub const ENTRY_SIZE: usize = 0x28;
    /// The renderer the manager creates textures with; the device is at
    /// +8 in it.
    pub const RENDERER: usize = 0x60;
}

/// The GUI manager, held by the global [`gui::MANAGER`].
pub mod gui {
    pub const MANAGER: usize = 0x23494f0;
    /// The keyboard listener that has the keyboard: an edit box's
    /// [`super::edit_box::LISTENER`] once one takes it, or null. The game
    /// may leave it set after the box lets go, so check the box too.
    pub const FOCUSED_LISTENER: usize = 0x210;
}

/// An edit box (vtable 0x1d91358).
pub mod edit_box {
    /// Its keyboard listener interface, whose vtable is [`LISTENER_VT`].
    pub const LISTENER: usize = 0x1d0;
    pub const LISTENER_VT: usize = 0x1d917d8;
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
}

/// A text editor (vtable 0x1d915c0).
pub mod editor {
    /// The text, a `std::string`.
    pub const TEXT: usize = 0x30;
    /// The caret: byte column within its line and line, u16 each.
    pub const COLUMN: usize = 0x54;
    pub const LINE: usize = 0x56;
    /// The font it draws with.
    pub const FONT: usize = 0x98;
}

use crate::points::Point;

pub fn points() -> Vec<Point> {
    vec![
        // Font.
        Point::keep(0x1595cf2usize, "49 89 84 ff 20 01 00 00", "font own glyph table"),
        Point::keep(0x159ba20usize, "66 0f 6e 81 60 09 00 00 0f 5b c0 f3 0f 59 81 68 09 00 00", "font line height and scale"),
        // The font's page load: the page's index, then through the context
        // the manager, its entries and the entry's size, kept as the page's.
        Point::keep(
            0x15954b0usize,
            "89 87 70 09 00 00 48 8b 4f 48 48 8b 89 80 04 00 00 85 c0 78 2c 48 8b 51 08 48 8b 49 10 48 2b ca 48 c1 f9 03 \
             49 b8 39 8e e3 38 8e e3 38 8e 49 0f af c8 3b c1 7d 0b 8b c0 48 8d 1c c0 48 8b 5c da 28 48 89 9f 78 09 00 00",
            "font page lookup",
        ),
        Point::keep(0x16c2e84usize, "49 8b 4f 60", "texture manager renderer"),
        // GUI manager: an edit box taking the keyboard stores its listener.
        Point::keep(0x153537cusize, "48 8b 05 6d 41 e1 00", "GUI manager"),
        Point::keep(0x1535394usize, "48 8d 8f d0 01 00 00 48 89 88 10 02 00 00", "edit box takes the keyboard"),
        // Edit box.
        Point::keep(0x15333ebusize, "48 8d 05 e6 e3 85 00 49 89 86 d0 01 00 00", "edit box keyboard listener"),
        Point::keep(0x1534216usize, "48 81 c1 c8 00 00 00", "edit box editor"),
        Point::keep(0x1535266usize, "c6 81 60 02 00 00 01", "edit box focus flag"),
        Point::keep(0x153427eusize, "48 8b 81 e0 01 00 00 4c 8b 98 04 26 03 00", "edit box caret shift"),
        Point::keep(0x15342adusize, "44 8b bf 8c 02 00 00 44 03 bf 40 02 00 00 44 03 7d 30 45 03 fb", "edit box caret x"),
        Point::keep(0x15342c2usize, "8b 87 90 02 00 00 03 87 44 02 00 00 03 45 34", "edit box caret y"),
        // Editor.
        Point::keep(0x1537020usize, "48 83 c1 30", "editor text"),
        Point::keep(0x1538e42usize, "66 3b 47 56 75 06 66 3b 47 54", "editor caret line and column"),
        Point::keep(0x15360bcusize, "48 8b 89 98 00 00 00", "editor font"),
    ]
}
