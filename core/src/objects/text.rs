//! What the game draws text with: fonts, their pages in the texture manager,
//! and the renderer that makes textures (docs/research/unicode-text.md,
//! font-textures.md).

/// A font (vtable 0x1d99120).
pub mod font {
    /// 256 pointers to glyph records, by byte (the font's own table).
    pub const OWN_GLYPHS: usize = 0x120;
    /// The line height, i32; times [`SCALE`] it is the line in pixels.
    pub const LINE_HEIGHT: usize = 0x960;
    /// f32. Code that holds the glyph table reaches it at
    /// [`SCALE_FROM_GLYPHS`] from there.
    pub const SCALE: usize = 0x968;
    pub const SCALE_FROM_GLYPHS: usize = SCALE - OWN_GLYPHS;
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
    /// The renderer the manager creates textures with.
    pub const RENDERER: usize = 0x60;
}

/// The renderer: the game's way to Direct3D 9.
pub mod renderer {
    /// The `IDirect3DDevice9`.
    pub const DEVICE: usize = 8;
}

/// What the renderer's texture creation is told: the format (a
/// `D3DFORMAT`) and the kind, 0 for a 2D texture (u32 each).
pub mod texture_description {
    pub const FORMAT: usize = 0xc;
    pub const KIND: usize = 0x10;
}

/// A mip level the texture creation fills the texture with: its width and
/// height (u32 each) and its pixels.
pub mod texture_level {
    pub const WIDTH: usize = 8;
    pub const HEIGHT: usize = 0xc;
    pub const DATA: usize = 0x18;
}

use crate::points::Point;

pub fn points() -> Vec<Point> {
    use self::*;
    vec![
        Point::keep(0x1595cf2usize, "49 89 84 ff 20 01 00 00", "font own glyph table").uses(&[font::OWN_GLYPHS]),
        Point::keep(0x159ba20usize, "66 0f 6e 81 60 09 00 00 0f 5b c0 f3 0f 59 81 68 09 00 00", "font line height and scale")
            .uses(&[font::LINE_HEIGHT, font::SCALE]),
        // The font's page load: the page's index, then through the context
        // the manager, its entries and the entry's size, kept as the page's.
        Point::keep(
            0x15954b0usize,
            "89 87 70 09 00 00 48 8b 4f 48 48 8b 89 80 04 00 00 85 c0 78 2c 48 8b 51 08 48 8b 49 10 48 2b ca 48 c1 f9 03 \
             49 b8 39 8e e3 38 8e e3 38 8e 49 0f af c8 3b c1 7d 0b 8b c0 48 8d 1c c0 48 8b 5c da 28 48 89 9f 78 09 00 00",
            "font page lookup",
        )
        .uses(&[font::PAGE_INDEX, font::CONTEXT, textures::MANAGER, textures::ENTRIES, textures::ENTRIES + 8, textures::ENTRY_SIZE, font::PAGE_SIZE]),
        Point::keep(0x16c2e84usize, "49 8b 4f 60", "texture manager renderer").uses(&[textures::RENDERER]),
        // Presenting a frame through the renderer's device.
        Point::keep(
            0x16d4db6usize,
            "48 8b d9 48 c7 44 24 20 00 00 00 00 48 8b 49 08 45 33 c9 45 33 c0 33 d2 48 8b 01 ff 90 88 00 00 00",
            "renderer device",
        )
        .uses(&[renderer::DEVICE]),
        // Creating a texture: the description's format and kind, then the
        // mip levels' sizes and pixels.
        Point::keep(0x16d7ab7usize, "49 8b f9 49 8b f0 4c 8b f2 4c 8b d9 33 db 89 5a 0c 45 8b 50 0c", "texture format")
            .uses(&[texture_description::FORMAT]),
        Point::keep(0x16d7b2dusize, "8b 56 10 85 d2", "texture kind").uses(&[texture_description::KIND]),
        // From here rdi points at the level's width.
        Point::keep(
            0x16d7bceusize,
            "48 83 c7 08 8b e9 ba 10 00 00 00 0f 1f 80 00 00 00 00 4c 8b 7f 10 45 84 ed 74 1d 8b 46 0c ff c8 b9 08 00 00 00 83 f8 01 \
             0f 47 ca 8b 1f 83 c3 03 c1 eb 02 0f af d9 eb 08 8b d8 0f af 1f c1 eb 03",
            "texture level width and pixels",
        )
        .uses(&[texture_level::WIDTH, texture_level::DATA - texture_level::WIDTH]),
        Point::keep(0x16d7c2cusize, "33 c9 48 89 4c 24 50 8b 47 04", "texture level height").uses(&[texture_level::HEIGHT - texture_level::WIDTH]),
    ]
}
