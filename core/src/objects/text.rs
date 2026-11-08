//! What the game draws text with: fonts, their pages in the texture manager,
//! and the renderer that makes textures (docs/research/unicode-text.md,
//! font-textures.md).

/// A font (vtable 0x1d99120).
pub mod font {
    /// Its name in the `.gfx` files (`vic_18`), a `std::string`: the game
    /// looks fonts up by it.
    pub const NAME: usize = 0x18;
    /// The color its text is drawn in, 0xAARRGGBB, from the `color` its
    /// `.gfx` files give it (0 when they give none): its page is tinted with
    /// it.
    pub const COLOR: usize = 0x58;
    /// 256 pointers to glyph records, by byte (the font's own table).
    pub const OWN_GLYPHS: usize = 0x120;
    /// The line height, i32; times [`SCALE`] it is the line in GUI units.
    pub const LINE_HEIGHT: usize = 0x960;
    /// f32, 1.0 when the font is made: how many GUI units a pixel of its page
    /// takes. Every text path multiplies the line height and each glyph's
    /// size, offsets and advance by it. Code that holds the glyph table
    /// reaches it at [`SCALE_FROM_GLYPHS`] from there.
    pub const SCALE: usize = 0x968;
    pub const SCALE_FROM_GLYPHS: usize = SCALE - OWN_GLYPHS;
    /// The page's index in the texture manager, i32.
    pub const PAGE_INDEX: usize = 0x970;
    /// The page's width and height, u32 each.
    pub const PAGE_SIZE: usize = 0x978;
    /// The graphics context, which holds the texture manager.
    pub const CONTEXT: usize = 0x48;
    /// Main text keeps the last texts it laid out, each in a vertex buffer
    /// of its own, in `TEXT_SLOTS` slots: here each slot's key, i32, a hash
    /// of the text and how it is drawn. A slot whose key is 0 is free, and
    /// the next text not found takes it and is laid out again.
    pub const TEXT_KEYS: usize = 0x3a28;
    pub const TEXT_SLOTS: usize = 40;
}

/// The graphics context, at `[font.CONTEXT]`.
pub mod context {
    /// The interface scale, f32, as fonts load: how many window pixels a GUI
    /// unit takes.
    pub const INTERFACE_SCALE: usize = 0x32600;
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

/// The map the game draws, at the application's `MAP`: the names on it are
/// meshes built from its font's page, which keep their glyphs' places in
/// fractions of the page's size.
pub mod map {
    /// The font its country names are drawn in.
    pub const NAMES_FONT: usize = 0x4d0;
    /// Builds every country's name again, when the map has its font, and
    /// marks the map mode for its refresh at the next frame, which builds
    /// the other names again: `(map)`.
    pub const REBUILD_NAMES: crate::points::GameFunction<1> = crate::points::GameFunction::at(0x10a4680);
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

/// An element that shows its text through a texture of its own, as buttons,
/// instant text boxes and tooltips do: 0x1550e90 draws the text into the
/// texture, 0x1552040 builds the mesh that shows it and 0x1551840 draws the
/// mesh.
pub mod text_texture {
    /// The texture's wrapper, its `IDirect3DTexture9` first; null until the
    /// text is drawn.
    pub const TEXTURE: usize = 0x278;
    /// The text's width and height in GUI units, i32 each: the mesh spans
    /// them, its UVs all of the texture unless the game makes text textures
    /// a power of two across and down.
    pub const SIZE: usize = 0x280;
    /// The mesh's first vertex, the texture's bottom left, in GUI units from
    /// the element's place and with the game's half pixel in it: x and y,
    /// f32 each.
    pub const CORNER: usize = 0x2c8;
    /// From the first vertex to the last, the texture's top right: x and y,
    /// f32 each. The text's size when the mesh is neither cut nor turned.
    pub const EXTENT: usize = 0x2d0;
}

use crate::{game_string, points::Point};

pub fn points() -> Vec<Point> {
    use self::*;
    vec![
        // Looking a font up by name: each font's name, its size and its
        // capacity, against the name asked for.
        Point::keep(
            0x14db450usize,
            "48 8b 33 48 8d 4e 18 48 8b d7 49 83 fe 10 72 03 48 8b 17 4c 8b 41 10 48 83 79 18 10 72 03 48 8b 09 4d 3b c7 75 09",
            "font lookup by name",
        )
        .uses(&[font::NAME, game_string::SIZE, game_string::CAPACITY]),
        // The `.gfx` key `color`, read as hex into the font, and main text
        // splitting it into the channels it draws with.
        Point::keep(0x1594b46usize, "4c 8d 41 58 48 8d 8b 44 04 00 00", "font color key").uses(&[font::COLOR]),
        Point::keep(0x1598997usize, "41 8b 4e 58 44 0f b6 c9", "font color drawn").uses(&[font::COLOR]),
        Point::keep(0x1595cf2usize, "49 89 84 ff 20 01 00 00", "font own glyph table").uses(&[font::OWN_GLYPHS]),
        Point::keep(0x159ba20usize, "66 0f 6e 81 60 09 00 00 0f 5b c0 f3 0f 59 81 68 09 00 00", "font line height and scale")
            .uses(&[font::LINE_HEIGHT, font::SCALE]),
        // Made 1.0, through the glyph table in rdi.
        Point::keep(0x1594663usize, "c7 87 48 08 00 00 00 00 80 3f", "font scale made").uses(&[font::SCALE_FROM_GLYPHS]),
        // The page load reads the interface scale through the context.
        Point::keep(0x15953f5usize, "48 8b 41 48 f3 0f 10 80 00 26 03 00", "font load interface scale").uses(&[font::CONTEXT, context::INTERFACE_SCALE]),
        // The font's page load: the page's index, then through the context
        // the manager, its entries and the entry's size, kept as the page's.
        Point::keep(
            0x15954b0usize,
            "89 87 70 09 00 00 48 8b 4f 48 48 8b 89 80 04 00 00 85 c0 78 2c 48 8b 51 08 48 8b 49 10 48 2b ca 48 c1 f9 03 \
             49 b8 39 8e e3 38 8e e3 38 8e 49 0f af c8 3b c1 7d 0b 8b c0 48 8d 1c c0 48 8b 5c da 28 48 89 9f 78 09 00 00",
            "font page lookup",
        )
        .uses(&[font::PAGE_INDEX, font::CONTEXT, textures::MANAGER, textures::ENTRIES, textures::ENTRIES + 8, textures::ENTRY_SIZE, font::PAGE_SIZE]),
        // Main text looking for a text it keeps: each slot's key against
        // the text's, and the first slot whose key is 0 taken if none is.
        Point::keep(
            0x1598de8usize,
            "49 8d 96 28 3a 00 00 90 44 8b 02 45 3b c3 0f 84 f7 23 00 00 41 83 fd ff 75 13 45 85 c0 75 0e 44 8b e8 89 85 58 23 00 00 \
             f2 44 0f 11 19 ff c0 48 83 c2 04 48 83 c1 08 83 f8 28",
            "font text cache keys",
        )
        .uses(&[font::TEXT_KEYS, font::TEXT_SLOTS]),
        Point::keep(0x16c2e84usize, "49 8b 4f 60", "texture manager renderer").uses(&[textures::RENDERER]),
        Point::keep(
            map::REBUILD_NAMES,
            "40 53 48 83 ec 20 48 83 b9 d0 04 00 00 00 48 8b d9 74 12 e8 d8 ab 00 00 c6 83 a0 25 00 00 01",
            "map names built again",
        )
        .uses(&[map::NAMES_FONT]),
        // Presenting a frame through the renderer's device.
        Point::keep(0x16d4db6usize, "48 8b d9 48 c7 44 24 20 00 00 00 00 48 8b 49 08 45 33 c9 45 33 c0 33 d2 48 8b 01 ff 90 88 00 00 00", "renderer device")
            .uses(&[renderer::DEVICE]),
        // Creating a texture: the description's format and kind, then the
        // mip levels' sizes and pixels.
        Point::keep(0x16d7ab7usize, "49 8b f9 49 8b f0 4c 8b f2 4c 8b d9 33 db 89 5a 0c 45 8b 50 0c", "texture format").uses(&[texture_description::FORMAT]),
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
        // Showing an element's text texture: the texture set for the draw,
        // the size the mesh is built from, and the first vertex and the
        // extent the mesh builder keeps.
        Point::keep(0x1551ac3usize, "4c 8d 83 78 02 00 00", "text texture shown").uses(&[text_texture::TEXTURE]),
        Point::keep(0x155209eusize, "8b 89 80 02 00 00", "text texture mesh width").uses(&[text_texture::SIZE]),
        Point::keep(0x1552050usize, "44 8b 81 84 02 00 00", "text texture mesh height").uses(&[text_texture::SIZE + 4]),
        Point::keep(0x1552309usize, "f3 0f 11 93 c8 02 00 00", "text texture mesh corner x").uses(&[text_texture::CORNER]),
        Point::keep(0x155232eusize, "f3 44 0f 11 8b cc 02 00 00", "text texture mesh corner y").uses(&[text_texture::CORNER + 4]),
        Point::keep(0x1552498usize, "f3 0f 11 bb d0 02 00 00", "text texture mesh extent x").uses(&[text_texture::EXTENT]),
        Point::keep(0x15524c7usize, "f3 44 0f 11 83 d4 02 00 00", "text texture mesh extent y").uses(&[text_texture::EXTENT + 4]),
    ]
}
