//! Glyphs from system fonts in the game's font pages
//! (ADR game-text-in-system-fonts): every character the game draws. A font
//! draws from one page texture, which the game reads from the page's wrapper at
//! every draw, so the framework gives a page a new texture of its own, where
//! every character is drawn from the system font and the font's own glyphs are
//! replaced. A glyph is drawn into it before its first use: the font's
//! Windows-1252 characters as it loads, the rest the first time they are
//! needed (docs/research/font-textures.md). The font's own page is read only
//! for how its `H` looks: the color of its ink and its outline.
//!
//! A page starts small and grows as it fills (ADR font-pages-grow): a page
//! three quarters full, or without room for a glyph, gets a texture twice as
//! large at the next frame the main thread presents ([`grow_pages`]), its
//! glyphs copied to where they were. Which glyphs a page holds, which wait
//! and when it grows is kept by [`pages`]; this module gives it Direct3D
//! ([`PageTexture`]) and the game's fonts ([`InGame`]). The game keeps what
//! it lays out with glyphs' places in fractions of the page's size, so every
//! font drawing from the page lays its text out again: its cached text is
//! dropped. A glyph without room until then shows as '?' for that frame. The
//! names on the map keep their glyphs' places too: when a map font's page
//! grows, the map builds them again in a game.
//!
//! The game stretches a font's glyphs by the interface scale, with a linear
//! filter, so an interface font's glyphs are drawn that many times larger and
//! its scale divided to match: they reach the window a texel to a pixel, as
//! sharp as the system font draws them ([`density`]).
//!
//! Fonts with lines taller than 48 pixels are the map's and draw in the map
//! font, inside the outline the font's own glyphs have when the player keeps
//! it (`outline`); framework fonts ([`framework_fonts`]) in the system font
//! of their own kind, such as the game console's; the rest in the heading
//! font or the body text font, as their names say ([`kind`]). Light text and dark text get glyphs of their
//! own (`tone`): the fonts that load one page may draw in either, as
//! `vic_18` in white and `vic_18_black` in black do, so a page holds a
//! character's glyph in each tone its fonts draw in ([`shown`]). Pages live
//! in video memory,
//! like the game's own textures, which only Direct3D 9Ex keeps across a
//! device reset; on a plain Direct3D 9 device nothing is filled. They are
//! written through small system-memory textures: pages made dynamic and
//! locked a band at a time made Direct3D keep several copies of each, 425 MB
//! for the map's.
use super::costs;
use super::dxt;
use super::framework_fonts;
use super::game_text;
use super::glyphs::{self, Glyph};
use super::map_field::{self, Shading};
use super::outline::Outline;
use super::pages::{self, Drawn, Game, Made, NoRoom, Pages, Shared, Texture};
use super::system_font::{Face, SystemFonts};
use super::tone::Tone;
use crate::game_string;
use crate::hooks::Call;
use crate::memory::Memory;
use crate::objects::text::{self as objects, font::OWN_GLYPHS, texture_description, texture_level, textures};
use crate::points::Point;
use crate::settings;
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::LazyLock;
use windows::core::Interface;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Direct3D9::*;

/// Creates a texture: renderer, wrapper, description, mip levels, count.
const CREATE_TEXTURE: usize = 0x16d7aa0;
/// Releases a texture and frees its wrapper (wrapper).
const DESTROY_TEXTURE: usize = 0x16d80f0;
/// The engine's format numbers for a 32-bit page (A8R8G8B8) and a DXT5 one
/// (0x16f9850 maps them to Direct3D's).
const ARGB: u32 = 0xb;
const DXT5: [u32; 2] = [5, 6];
const MAP_LINE: i32 = 48;

/// A font's own page as the game decoded it, its first level only: read for
/// how its glyphs look.
struct Own {
    width: u32,
    height: u32,
    /// DXT5 blocks; A8R8G8B8 rows otherwise.
    compressed: bool,
    data: Vec<u8>,
}

impl Own {
    /// The pixel at (x, y) as 0xAARRGGBB.
    fn pixel(&self, x: u32, y: u32) -> u32 {
        if self.compressed {
            let at = ((y / 4) * self.width.div_ceil(4) + x / 4) as usize * dxt::BLOCK;
            let block: &[u8; dxt::BLOCK] = self.data[at..at + dxt::BLOCK].try_into().expect("a whole block");
            dxt::decode(block)[((y % 4) * 4 + x % 4) as usize]
        } else {
            let at = ((y * self.width + x) * 4) as usize;
            u32::from_le_bytes(self.data[at..at + 4].try_into().expect("a whole pixel"))
        }
    }
}

/// A page's texture in video memory, on the game's device, written through
/// a system-memory texture.
pub struct PageTexture {
    device: IDirect3DDevice9,
    texture: IDirect3DTexture9,
    /// The system-memory texture glyphs are written through, and its size.
    staging: Option<(u32, u32, IDirect3DTexture9)>,
    /// The page has mip levels, which the card makes from its first.
    mips: bool,
}

// Pages are made only on a device made multithreaded (see `device`), which
// takes its lock in each call, so any thread that draws text may fill one.
unsafe impl Send for PageTexture {}

impl Texture for PageTexture {
    /// Writes a glyph's pixels through a staging texture kept for the page
    /// and made larger when a glyph needs it.
    fn put(&mut self, x: u32, y: u32, width: u32, height: u32, data: &[u8]) -> crate::Result<()> {
        if !matches!(&self.staging, Some((w, h, _)) if *w >= width && *h >= height) {
            let (w, h) = (width.next_multiple_of(64), height.next_multiple_of(64));
            self.staging = Some((w, h, staging(&self.device, w, h)?));
        }
        let (_, _, staging) = self.staging.as_ref().expect("made above");
        put(&self.device, &self.texture, staging, x, y, width, height, data).map_err(|e| e.to_string())
    }

    /// A new texture, the old one's pixels copied into it on the card.
    fn grown(&self, (width, height): (u32, u32), (to_width, to_height): (u32, u32)) -> crate::Result<Self> {
        let texture = page_texture(&self.device, to_width, to_height, self.mips)?;
        let drawn = RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 };
        unsafe { self.device.StretchRect(&surface(&self.texture)?, &drawn, &surface(&texture)?, &drawn, D3DTEXF_NONE) }
            .map_err(|e| format!("copying the page: {e}"))?;
        Ok(Self { device: self.device.clone(), texture, staging: self.staging.clone(), mips: self.mips })
    }

    fn make_mips(&self) {
        if self.mips {
            unsafe { self.texture.GenerateMipSubLevels() };
        }
    }
}

/// How a page's glyphs are drawn.
struct Look {
    face: Face,
    /// The colour the glyphs are drawn in.
    ink: u32,
    /// The outline drawn around them, by the page, or by the map's shader
    /// when the page holds distance fields.
    outline: Option<Outline>,
    /// The spread of the distance fields the page holds instead of colors,
    /// for the map's names (`map_field`).
    field: Option<u32>,
    /// The em size, in pixels.
    em: f32,
    /// From the top of a line to the baseline, in pixels.
    base: f32,
    /// How many pixels of the page a GUI unit takes (see [`density`]).
    density: f32,
    /// The tones its fonts have drawn in so far, for the log.
    tones: Vec<Tone>,
}

static SYSTEM: LazyLock<Option<SystemFonts>> = LazyLock::new(|| SystemFonts::new().ok());
static PAGES: LazyLock<Shared<PageTexture, Look>> = LazyLock::new(Default::default);

/// The largest page: glyphs' places are read as signed 16-bit numbers.
const LARGEST: u32 = 16384;

thread_local! {
    /// The font whose page is loading on this thread, and the page once the
    /// game has decoded it.
    static LOADING: RefCell<Loading> = const { RefCell::new(Loading { font: None, page: None }) };
}

/// The page a font is loading on this thread: begun when the font's method
/// that loads its page (0x15953c0) starts, given the pixels of the first
/// texture the game creates inside that call, and taken when the method
/// returns. A texture created outside that call, or after the first, is not
/// the font's page.
struct Loading {
    font: Option<usize>,
    page: Option<Own>,
}

impl Loading {
    fn begin(&mut self, font: usize) {
        *self = Loading { font: Some(font), page: None };
    }

    /// Whether a texture created now would be the loading font's page.
    fn wants_page(&self) -> bool {
        self.font.is_some() && self.page.is_none()
    }

    fn give(&mut self, page: Own) {
        if self.wants_page() {
            self.page = Some(page);
        }
    }

    /// The page `font` loaded, when it was `font` that was loading (None
    /// otherwise); the inner None when the game made no page of a kind the
    /// framework can read.
    fn finish(&mut self, font: usize) -> Option<Option<Own>> {
        let done = std::mem::replace(self, Loading { font: None, page: None });
        (done.font == Some(font)).then_some(done.page)
    }
}

/// The texture manager's entry for `font`'s page.
fn page_entry(m: &Memory, font: usize) -> crate::Result<(usize, usize)> {
    let index = m.i32(font + objects::font::PAGE_INDEX)?;
    let manager = m.ptr(m.ptr(font + objects::font::CONTEXT)? + textures::MANAGER)?;
    let (begin, end) = (m.ptr(manager + textures::ENTRIES)?, m.ptr(manager + textures::ENTRIES + 8)?);
    if index < 0 || begin + (index as usize + 1) * textures::ENTRY > end {
        return Err("font page index out of range".into());
    }
    Ok((manager, begin + index as usize * textures::ENTRY))
}

/// The device, through the manager's renderer: a Direct3D 9Ex one, which
/// keeps pages in video memory across a reset, made multithreaded, since
/// text is drawn, and so glyphs filled in, on more threads than one.
fn device(m: &Memory, manager: usize) -> crate::Result<IDirect3DDevice9> {
    let raw = m.ptr(m.ptr(manager + textures::RENDERER)? + objects::renderer::DEVICE)? as *mut c_void;
    let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&raw) }.ok_or("no device")?.clone();
    device.cast::<IDirect3DDevice9Ex>().map_err(|_| "not a Direct3D 9Ex device".to_string())?;
    let mut made = D3DDEVICE_CREATION_PARAMETERS::default();
    unsafe { device.GetCreationParameters(&mut made) }.map_err(|e| format!("device creation parameters: {e}"))?;
    if made.BehaviorFlags & D3DCREATE_MULTITHREADED as u32 == 0 {
        return Err("the device is not multithreaded".into());
    }
    Ok(device)
}

/// The bytes of one row of a `width`-pixel page: of pixels, or of blocks.
fn row_bytes(compressed: bool, width: u32) -> usize {
    if compressed {
        width.div_ceil(4) as usize * dxt::BLOCK
    } else {
        width as usize * 4
    }
}

/// A system-memory texture to write a page through.
fn staging(device: &IDirect3DDevice9, width: u32, height: u32) -> crate::Result<IDirect3DTexture9> {
    let mut texture = None;
    unsafe { device.CreateTexture(width, height, 1, 0, D3DFMT_A8R8G8B8, D3DPOOL_SYSTEMMEM, &mut texture, std::ptr::null_mut()) }
        .map_err(|e| format!("staging texture: {e}"))?;
    texture.ok_or_else(|| "no staging texture".to_string())
}

/// Writes `width` × `height` pixels at (x, y) of `page`, given as rows of
/// A8R8G8B8 pixels, through `staging`, a system-memory texture at least that
/// large, from which the device copies them into the page.
#[expect(clippy::too_many_arguments, reason = "the device, the textures, a rectangle and its pixels; a struct would only rename them")]
fn put(
    device: &IDirect3DDevice9,
    page: &IDirect3DTexture9,
    staging: &IDirect3DTexture9,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    data: &[u8],
) -> windows::core::Result<()> {
    if width == 0 || height == 0 {
        return Ok(());
    }
    let row = width as usize * 4;
    let rect = RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 };
    let at = POINT { x: x as i32, y: y as i32 };
    let mut locked = D3DLOCKED_RECT::default();
    unsafe {
        staging.LockRect(0, &mut locked, &rect, 0)?;
        for n in 0..height as usize {
            let line = (locked.pBits as *mut u8).add(n * locked.Pitch as usize);
            std::ptr::copy_nonoverlapping(data.as_ptr().add(n * row), line, row);
        }
        staging.UnlockRect(0)?;
        device.UpdateSurface(&staging.GetSurfaceLevel(0)?, &rect, &page.GetSurfaceLevel(0)?, &at)
    }
}

/// A page texture's pixels.
fn surface(texture: &IDirect3DTexture9) -> crate::Result<IDirect3DSurface9> {
    unsafe { texture.GetSurfaceLevel(0) }.map_err(|e| format!("page surface: {e}"))
}

/// An empty page texture, `width` × `height`: in video memory, and a render
/// target, so that a page that grows can be copied into its next one there;
/// with `mips`, mip levels the card makes from the first.
fn page_texture(device: &IDirect3DDevice9, width: u32, height: u32, mips: bool) -> crate::Result<IDirect3DTexture9> {
    let mut texture = None;
    let (levels, usage) = if mips { (0, D3DUSAGE_RENDERTARGET | D3DUSAGE_AUTOGENMIPMAP) } else { (1, D3DUSAGE_RENDERTARGET) };
    unsafe {
        device
            .CreateTexture(width, height, levels, usage as u32, D3DFMT_A8R8G8B8, D3DPOOL_DEFAULT, &mut texture, std::ptr::null_mut())
            .map_err(|e| format!("page texture: {e}"))?;
        let texture = texture.ok_or("no page texture")?;
        device.ColorFill(&surface(&texture)?, std::ptr::null(), 0).map_err(|e| format!("page pixels: {e}"))?;
        Ok(texture)
    }
}

/// `D3DOK_NOAUTOGEN`: the format is there, but the card makes no mip levels
/// for it.
const NO_AUTOGEN: i32 = 0x0876_086f;

/// Whether the card makes the mip levels of a page that is a render target.
fn makes_mips(device: &IDirect3DDevice9) -> bool {
    let check = || -> windows::core::Result<bool> {
        let mut made = D3DDEVICE_CREATION_PARAMETERS::default();
        let mut mode = D3DDISPLAYMODE::default();
        unsafe {
            device.GetCreationParameters(&mut made)?;
            device.GetDisplayMode(0, &mut mode)?;
            let d3d = device.GetDirect3D()?;
            // Through the table: the success it answers with when it makes
            // none is still a success to the wrapper.
            let usage = (D3DUSAGE_RENDERTARGET | D3DUSAGE_AUTOGENMIPMAP) as u32;
            let answer =
                (d3d.vtable().CheckDeviceFormat)(d3d.as_raw(), made.AdapterOrdinal, made.DeviceType, mode.Format, usage, D3DRTYPE_TEXTURE, D3DFMT_A8R8G8B8);
            Ok(answer.is_ok() && answer.0 != NO_AUTOGEN)
        }
    };
    check().unwrap_or(false)
}

/// `ink`, with the coverage as alpha, as 0xAARRGGBB pixels.
fn inked(alpha: &[u8], ink: u32) -> Vec<u32> {
    alpha.iter().map(|&a| (a as u32) << 24 | ink).collect()
}

/// What the font's own `H` shows, read from its page.
struct Letter {
    /// The colour of its most opaque pixel. Body text is drawn tinted, so
    /// its pages are white; the map draws its names in the page's colour,
    /// black in the fonts the game ships.
    ink: u32,
    /// The outline around it, if it has one.
    outline: Option<Outline>,
}

/// What the font's own `H` shows; None when none of it is at least half
/// opaque.
fn letter(m: &Memory, font: usize, own: &Own) -> Option<Letter> {
    let record = m.ptr(font + OWN_GLYPHS + b'H' as usize * 8).ok().filter(|&r| r != 0)?;
    let bytes = m.bytes(record, std::mem::size_of::<Glyph>()).ok()?;
    let h = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const Glyph) };
    let width = (h.width as u32).min(own.width.saturating_sub(h.x as u32));
    let height = (h.height as u32).min(own.height.saturating_sub(h.y as u32));
    let pixels: Vec<u32> = (0..height).flat_map(|y| (0..width).map(move |x| own.pixel(h.x as u32 + x, h.y as u32 + y))).collect();
    let (mut best, mut ink) = (0, 0);
    for &pixel in &pixels {
        if pixel >> 24 > best {
            (best, ink) = (pixel >> 24, pixel & 0x00ff_ffff);
        }
    }
    (best >= 0x80).then(|| Letter { ink, outline: Outline::measure(&pixels, width, height, ink) })
}

/// The game's record for `bitmap` drawn at (`x`, `y`) of the page, on lines
/// whose baseline is `base` pixels down.
fn record(bitmap: &super::system_font::Bitmap, (x, y): (u32, u32), base: f32) -> Glyph {
    Glyph {
        x: x as u16,
        y: y as u16,
        width: bitmap.width as u16,
        height: bitmap.height as u16,
        x_offset: bitmap.left as i16,
        y_offset: (base + bitmap.top as f32).round() as i16,
        advance: bitmap.advance.round() as i16,
        kerned: 0,
        page: 0,
    }
}

/// `scalar`'s glyph as the page draws it; None when no system font has the
/// character.
fn draw(look: &Look, scalar: u32, tone: Tone) -> Option<Drawn> {
    let fonts = SYSTEM.as_ref()?;
    let face = fonts.face_for(&look.face, scalar)?;
    let mut bitmap = fonts.rasterize(&face, scalar, look.em).ok()?;
    tone.apply(&mut bitmap.alpha);
    let drawn = bitmap.width > 0 && bitmap.height > 0;
    let (bitmap, pixels) = match (look.field, &look.outline) {
        (Some(spread), _) if drawn => map_field::field(&bitmap, spread),
        (None, Some(outline)) if drawn => outline.around(&bitmap, look.ink),
        _ => {
            let pixels = inked(&bitmap.alpha, look.ink);
            (bitmap, pixels)
        }
    };
    let pixels = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
    Some(Drawn { glyph: record(&bitmap, (0, 0), look.base), pixels })
}

/// The glyph a filled `font` draws for `scalar`, which its own table does not
/// hold: drawn into its page the first time. Null when the font is not filled
/// or no system font has the character.
pub fn glyph(font: usize, scalar: u32) -> *const Glyph {
    let glyph = PAGES.lock().glyph(font, scalar, draw);
    match glyph {
        // The page keeps what it drew for every font that draws from it; the
        // font gets the record the game reads in its glyph table, where the
        // next lookup finds it first.
        Some(g) => {
            glyphs::insert(font, scalar, g);
            glyphs::get(font, scalar)
        }
        None => std::ptr::null(),
    }
}

/// The device a filled font's page lives on; None for a font not filled.
pub fn device_of(font: usize) -> Option<IDirect3DDevice9> {
    let pages = PAGES.lock();
    let (wrapper, _) = pages.owner(font)?;
    pages.texture(wrapper).map(|texture| texture.device.clone())
}

/// The em size and baseline that fit `face` to a line of `line` pixels.
fn metrics(fonts: &SystemFonts, face: &Face, line: f32) -> (f32, f32) {
    let (ascent, descent) = fonts.line_metrics(face).unwrap_or((0.9, 0.25));
    let em = line / (ascent + descent);
    (em, em * ascent)
}

/// Whether a font with lines `line` pixels tall is one of the map's.
fn draws_the_map(line: i32) -> bool {
    line > MAP_LINE
}

/// What a font draws, which picks the system font family it draws in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Body,
    Heading,
    Map,
    /// The game console's text, in the framework font made for it.
    Console,
}

/// The game's fonts for headings, by their names in `interface/core.gfx`:
/// its fonts with lines of 20 GUI units or more, below the map's.
const HEADINGS: &[&[u8]] = &[
    b"standard_22",
    b"mapfont_small",
    b"AnimatedMapText",
    b"chat_largefont",
    b"Main_24",
    b"Main_24_black",
    b"pie_headline_20",
    b"vic_22",
    b"vic_22_black",
    b"vic_22s",
    b"vic_29",
    b"vic_29_black",
    b"vic_29s",
    b"vic_36",
    b"vic_36_black",
    b"vic_36s",
];

/// The game's fonts for body text, in `interface/core.gfx` and
/// `interface/chatfonts.gfx`: its fonts with lines of 18 GUI units or less.
const BODY: &[&[u8]] = &[
    b"ToolTip_Font",
    b"ToolTip_Font_Big",
    b"standard_18",
    b"messagelog",
    b"Main_16",
    b"Main_16_white_bold",
    b"Main_16_black",
    b"Main_16_plain",
    b"Main_14",
    b"Main_14_plain",
    b"Main_14_grey",
    b"Main_14_black",
    b"Main_14_red",
    b"Main_14_bold",
    b"Main_14_orange",
    b"Main_14_eu",
    b"garamond_14",
    b"garamond_14_bold",
    b"garamond_14_bold_orange",
    b"ledger",
    b"ledger_headline",
    b"vic_18",
    b"vic_18_black",
    b"vic_18s",
    b"vic_18_grey",
    b"Arial12",
    b"FPS_Font",
    b"null_font",
    b"standard_font",
    b"chat_input_font",
];

/// Lines a font of a name the game does not have needs to draw headings.
const HEADING_LINE: i32 = 20;

/// What the font named `name`, with lines `line` pixels tall, draws. A mod
/// that gives the game's fonts other files keeps their names, which the
/// interface asks for them by, while their lines change: the Chinese language
/// mod draws `vic_22`'s headings on 18-pixel lines, as tall as `vic_18`'s body
/// text in the game. So the name says what the font draws, and only a name
/// the game does not have goes by the line.
fn kind(name: &[u8], line: i32) -> Kind {
    if let Some(font) = framework_fonts::named(name) {
        font.kind
    } else if draws_the_map(line) {
        Kind::Map
    } else if HEADINGS.contains(&name) {
        Kind::Heading
    } else if BODY.contains(&name) || line < HEADING_LINE {
        Kind::Body
    } else {
        Kind::Heading
    }
}

/// The most pixels of a page a GUI unit takes.
const MOST_DENSE: f32 = 4.0;

/// How many pixels of its page a GUI unit of `font` takes: the interface
/// scale the font loads at, when it is above 1 (up to [`MOST_DENSE`]). The
/// game draws a font's glyphs that many times larger than their pixels, with
/// a linear filter, which blurs them; drawn that much larger, and with the
/// font's scale divided by as much ([`dense_line`]), they reach the window a
/// texel to a pixel.
fn density(m: &Memory, font: usize) -> f32 {
    let scale = m.ptr(font + objects::font::CONTEXT).and_then(|context| m.u32(context + objects::context::INTERFACE_SCALE)).map(f32::from_bits);
    match scale {
        Ok(s) if s > 1.0 && s.is_finite() => s.min(MOST_DENSE),
        _ => 1.0,
    }
}

/// The line height and scale for a font of lines `line` GUI units tall whose
/// glyphs are `density` times their size in GUI units: the game takes a line
/// as `(line height * scale) as i32` GUI units, so the line height grows to
/// at least `line * density` and keeps that.
fn dense_line(line: i32, density: f32) -> Option<(i32, f32)> {
    let scale = 1.0 / density;
    let least = (line as f32 * density).ceil() as i32;
    (least..least + 2).find(|&l| (l as f32 * scale) as i32 == line).map(|l| (l, scale))
}

/// Makes `font` draw from glyphs `density` times their size in GUI units.
/// Only once: a font whose scale is no longer the 1.0 it was made with keeps
/// it.
fn scale_font(m: &Memory, font: usize, density: f32) -> crate::Result<()> {
    if m.u32(font + objects::font::SCALE)? != 1.0f32.to_bits() {
        return Ok(());
    }
    let line = m.i32(font + objects::font::LINE_HEIGHT)?;
    let (line, scale) = dense_line(line, density).ok_or_else(|| format!("no line height draws {line} GUI units at {density}"))?;
    unsafe {
        *((font + objects::font::LINE_HEIGHT) as *mut i32) = line;
        *((font + objects::font::SCALE) as *mut f32) = scale;
    }
    Ok(())
}

/// How wide and tall the square page of a font that keeps its page's size
/// is: sized to its `line` in GUI units, and `growth` times as wide and tall,
/// so that it holds as many glyphs `growth` times as large (drawn denser, or
/// inside an outline), up to the device's largest texture, `max` (width,
/// height).
fn page_side(line: i32, growth: f32, max: (u32, u32)) -> u32 {
    let side: u32 = match line {
        ..=24 => 1024,
        25..=48 => 2048,
        _ => 4096,
    };
    ((side as f32 * growth / 256.0).ceil() as u32 * 256).min(max.0).min(max.1)
}

/// How wide and tall a page that grows starts: a quarter of the `side` it
/// would keep, which it reaches after two steps ([`grown`]).
fn first_side(side: u32) -> u32 {
    (side / 2).next_multiple_of(256).min(side)
}

/// The color, 0xRRGGBB, text of a font of `kind` shows in, drawn from a page
/// whose ink is `ink` with the font's `color` (0xAARRGGBB, 0 when its `.gfx`
/// files give none): the page's ink tinted with it, as the game draws
/// interface text. The map draws its names in the page's own colors.
fn shown(ink: u32, color: u32, kind: Kind) -> u32 {
    if kind == Kind::Map || color == 0 {
        return ink & 0x00ff_ffff;
    }
    let channel = |shift: u32| ((ink >> shift & 0xff) * (color >> shift & 0xff) / 255) << shift;
    channel(16) | channel(8) | channel(0)
}

/// The system font and weight fonts of `kind` draw in: the player's choice,
/// else the player language's default.
fn chosen_face(kind: Kind) -> Face {
    let role = match kind {
        Kind::Body => settings::Role::Body,
        Kind::Heading => settings::Role::Heading,
        Kind::Map => settings::Role::Map,
        Kind::Console => settings::Role::Console,
    };
    settings::current().face(role, |family| SYSTEM.as_ref().is_some_and(|fonts| fonts.has_family(family)))
}

/// An empty page for `font`, which draws `kind`, in the ink and the outline
/// of its own page (`own`, when the game decoded it).
fn make_page(m: &Memory, font: usize, device: &IDirect3DDevice9, own: Option<&Own>, kind: Kind) -> crate::Result<(Made<PageTexture>, Look)> {
    let fonts = SYSTEM.as_ref().ok_or("no system fonts")?;
    let line = m.i32(font + objects::font::LINE_HEIGHT)?.max(1);
    let current = settings::current();
    let map = kind == Kind::Map;
    let face = chosen_face(kind);
    let letter = own.and_then(|own| letter(m, font, own));
    let outline = letter.as_ref().and_then(|l| l.outline.clone()).filter(|_| map && current.map_outline);
    let name = String::from_utf8_lossy(&game_string::read(m, font + objects::font::NAME).unwrap_or_default()).into_owned();
    // The map's own font holds distance fields when the map's shader can
    // draw from them.
    let field = (map && name.as_bytes() == map_field::MAP_FONT)
        .then(|| {
            map_field::shader_decodes()
                .inspect_err(|e| log!("fill: the map font's page holds colors, as the map names' shader cannot draw from distances: {e}"))
        })
        .and_then(Result::ok)
        .map(|()| map_field::spread(outline.as_ref()));
    // The map's names are drawn on the map, which the interface scale does
    // not stretch.
    let density = if map { 1.0 } else { density(m, font) };
    let (em, base) = metrics(fonts, &face, line as f32 * density);
    let mut caps = D3DCAPS9::default();
    unsafe { device.GetDeviceCaps(&mut caps) }.map_err(|e| format!("device caps: {e}"))?;
    let reach = field.unwrap_or_else(|| outline.as_ref().map_or(0, |o| o.reach()));
    let growth = (line as f32 * density + 2.0 * reach as f32) / line as f32;
    let max = (caps.MaxTextureWidth.min(LARGEST), caps.MaxTextureHeight.min(LARGEST));
    let side = page_side(line, growth, max);
    // A page grows by copying it into its next one, a texture to a texture.
    let max = (caps.DevCaps2 & D3DDEVCAPS2_CAN_STRETCHRECT_FROM_TEXTURES as u32 != 0).then_some(max);
    let side = if max.is_some() { first_side(side) } else { side };
    let mips = field.is_some() && makes_mips(device);
    let creating = crate::heap::Change::begin();
    let texture = page_texture(device, side, side, mips)?;
    log!("fill: {side}x{side} page texture: creating it {}", creating.end());
    // Without the page's pixels: black for a map font, white otherwise.
    let ink = letter.as_ref().map_or(if map { 0 } else { 0x00ff_ffff }, |l| l.ink);
    if let Some(spread) = field {
        map_field::shade(Shading { spread, ink, outline: outline.clone() });
    }
    let texture = PageTexture { device: device.clone(), texture, staging: None, mips };
    let look = Look { face, ink, outline, field, em, base, density, tones: Vec::new() };
    Ok((Made { texture, size: (side, side), max, name, map }, look))
}

/// The font's glyphs for Windows-1252 characters become system glyphs,
/// written into its own records (or records allocated as the game allocates
/// them, which it frees with the font).
fn replace_own_glyphs(m: &Memory, font: usize, wrapper: usize, pages: &mut Pages<PageTexture, Look>, tone: Tone) -> crate::Result<()> {
    for byte in 0x20u8..=0xff {
        let (c, _) = game_text::char_at(&[byte]);
        if c.is_control() {
            continue;
        }
        let glyph = match pages.glyph_in(wrapper, c as u32, tone, draw) {
            Ok(glyph) => glyph,
            // These glyphs are the font's own records, which no later lookup
            // fills: the page grows at once, as no text is being laid out
            // on this thread while a font loads.
            Err(NoRoom) => {
                pages.grow(wrapper, &mut InGame { m })?;
                pages.glyph_in(wrapper, c as u32, tone, draw).ok().flatten()
            }
        };
        let Some(glyph) = glyph else { continue };
        let slot = font + OWN_GLYPHS + byte as usize * 8;
        let mut record = m.ptr(slot)?;
        if record == 0 {
            record = crate::game_heap::new(m, std::mem::size_of::<Glyph>())?;
            if record == 0 {
                continue;
            }
            unsafe { *(slot as *mut usize) = record };
        }
        unsafe { *(record as *mut Glyph) = glyph };
    }
    Ok(())
}

/// `font` starts loading on this thread (see [`Loading`]).
pub fn loading(font: usize) {
    LOADING.with(|l| l.borrow_mut().begin(font));
}

fn create_before(call: &mut Call) {
    let (description, levels, count) = (call.arg_as::<*const u8>(2), call.arg_as::<*const u8>(3), call.arg_as::<u32>(4));
    unsafe {
        let at = |base: *const u8, offset: usize| *(base.add(offset) as *const u32);
        LOADING.with(|l| {
            let mut l = l.borrow_mut();
            if !l.wants_page() {
                return;
            }
            // A 2D 32-bit or DXT5 page, its first level first.
            if description.is_null() || at(description, texture_description::KIND) != 0 || count == 0 {
                return;
            }
            let compressed = match at(description, texture_description::FORMAT) {
                ARGB => false,
                f if DXT5.contains(&f) => true,
                _ => return,
            };
            let (width, height) = (at(levels, texture_level::WIDTH), at(levels, texture_level::HEIGHT));
            let data = *(levels.add(texture_level::DATA) as *const *const u8);
            if data.is_null() || width == 0 || height == 0 || (compressed && (width % 4 != 0 || height % 4 != 0)) {
                return;
            }
            let size = height as usize / if compressed { 4 } else { 1 } * row_bytes(compressed, width);
            l.give(Own { width, height, compressed, data: std::slice::from_raw_parts(data, size).to_vec() });
        });
    }
}

/// A font's load ended in a fault: nothing is filled in for it.
pub fn abandoned(font: usize) {
    LOADING.with(|l| l.borrow_mut().finish(font));
}

/// `font` has loaded: gives it a page of system glyphs. Returns whether the
/// font now draws every character from the system font, so that the glyphs
/// its file brought no longer apply.
pub fn loaded(font: usize) -> bool {
    let Some(own) = LOADING.with(|l| l.borrow_mut().finish(font)) else { return false };
    let m = crate::hooked_memory();
    let result = (|| -> crate::Result<bool> {
        let (manager, entry) = page_entry(&m, font)?;
        let wrapper = m.ptr(entry)?;
        if wrapper == 0 || m.ptr(wrapper)? == 0 {
            return Ok(false);
        }
        // A name that cannot be read goes by the line.
        let name = game_string::read(&m, font + objects::font::NAME).unwrap_or_default();
        let kind = kind(&name, m.i32(font + objects::font::LINE_HEIGHT)?);
        let framework_font = framework_fonts::named(&name);
        let name = String::from_utf8_lossy(&name);
        let mut pages = PAGES.lock();
        match pages.with(wrapper) {
            // The page's glyphs are its first font's, in that font's face.
            Some(look) if look.face != chosen_face(kind) => {
                log!("fill: font {name:?} ({kind:?}) draws from a page in {}", look.face);
            }
            Some(_) => {}
            None => {
                let (made, look) = costs::PAGES.time(|| make_page(&m, font, &device(&m, manager)?, own.as_ref(), kind))?;
                // The wrapper holds the game's reference: it gets one of
                // ours, and its old texture is released.
                let old = m.ptr(wrapper)? as *mut c_void;
                let (width, height) = made.size;
                unsafe {
                    *(wrapper as *mut *mut c_void) = made.texture.texture.clone().into_raw();
                    drop(IDirect3DTexture9::from_raw(old));
                    *((entry + textures::ENTRY_SIZE) as *mut u32) = width;
                    *((entry + textures::ENTRY_SIZE + 4) as *mut u32) = height;
                }
                let outline = look.outline.as_ref().map_or("none".to_string(), |o| format!("{:06x}, {} px", o.color, o.reach()));
                let grows = made.max.map_or("keeps its size".to_string(), |(w, h)| format!("grows up to {w}x{h}"));
                let holds = match (look.field, made.texture.mips) {
                    (Some(spread), true) => format!("distance fields of spread {spread}, with mip levels"),
                    (Some(spread), false) => format!("distance fields of spread {spread}, without mip levels: the card makes none"),
                    (None, _) => "colors".to_string(),
                };
                log!(
                    "fill: system page {width}x{height} ({grows}, holding {holds}) for font {name:?} ({kind:?}) in {0} at {1:.1} px (baseline {2:.0}), {3:.2} px a GUI unit, ink {4:06x}, outline {outline}",
                    look.face,
                    look.em,
                    look.base,
                    look.density,
                    look.ink
                );
                pages.add(wrapper, made, look);
            }
        }
        let look = pages.with_mut(wrapper).expect("just made");
        let shown = shown(look.ink, m.u32(font + objects::font::COLOR)?, kind);
        let tone = Tone::of(shown);
        if !look.tones.contains(&tone) {
            look.tones.push(tone);
            log!("fill: font {name:?} ({kind:?}) is the first to draw {tone:?} text, in {shown:06x}, from the page in {} at {:.1} px", look.face, look.em);
        }
        let density = look.density;
        let (width, height) = pages.size(wrapper).expect("just made");
        unsafe {
            *((font + objects::font::PAGE_SIZE) as *mut u32) = width;
            *((font + objects::font::PAGE_SIZE + 4) as *mut u32) = height;
        }
        if density != 1.0 {
            scale_font(&m, font, density)?;
        }
        pages.draws_from(font, wrapper, tone);
        replace_own_glyphs(&m, font, wrapper, &mut pages, tone)?;
        if let Some(framework_font) = framework_font {
            framework_fonts::drawn(framework_font);
        }
        Ok(true)
    })();
    result.unwrap_or_else(|e| {
        log!("fill: font page left as it is: {e}");
        false
    })
}

/// The game's side of a page that grows.
struct InGame<'a> {
    m: &'a Memory,
}

impl Game<PageTexture> for InGame<'_> {
    /// The game's page and every font drawing from it take the grown
    /// texture; and those fonts drop the text they keep laid out, which holds
    /// glyphs' places in fractions of the page's old size.
    fn take(&mut self, wrapper: usize, fonts: &[usize], texture: &PageTexture, (width, height): (u32, u32)) -> crate::Result<()> {
        let m = self.m;
        let &first = fonts.first().ok_or("no font draws from the page")?;
        let (_, entry) = page_entry(m, first)?;
        if m.ptr(entry)? != wrapper {
            return Err("the font's page is not in its entry".into());
        }
        let old = m.ptr(wrapper)? as *mut c_void;
        unsafe {
            *(wrapper as *mut *mut c_void) = texture.texture.clone().into_raw();
            drop(IDirect3DTexture9::from_raw(old));
            *((entry + textures::ENTRY_SIZE) as *mut u32) = width;
            *((entry + textures::ENTRY_SIZE + 4) as *mut u32) = height;
            for &font in fonts {
                *((font + objects::font::PAGE_SIZE) as *mut u32) = width;
                *((font + objects::font::PAGE_SIZE + 4) as *mut u32) = height;
                // A text whose slot has no key is laid out again.
                std::ptr::write_bytes((font + objects::font::TEXT_KEYS) as *mut i32, 0, objects::font::TEXT_SLOTS);
            }
        }
        Ok(())
    }

    fn build_map_names(&mut self) -> crate::Result<bool> {
        rebuild_map_names(self.m)
    }
}

/// The names on the map are built again from the map's fonts' pages as they
/// are now, when a game is going on; returns whether they were.
fn rebuild_map_names(m: &Memory) -> crate::Result<bool> {
    use crate::objects::{application, campaign, text::map};
    let world = m.ptr(m.at(campaign::globals::WORLD))?;
    let manager = if world == 0 { 0 } else { m.ptr(world + campaign::world::MAP_MANAGER)? };
    if manager == 0 || m.ptr(manager)? != m.at(campaign::classes::MAP_MANAGER) {
        return Ok(false);
    }
    let map = m.ptr(m.ptr(manager + campaign::map_manager::APPLICATION)? + application::MAP)?;
    if map == 0 {
        return Ok(false);
    }
    let building = std::time::Instant::now();
    map::REBUILD_NAMES.call(m, [map])?;
    log!("fill: the names on the map built again in {:.1?}", building.elapsed());
    Ok(true)
}

/// Every page asks for room (the test switch `grow-pages`).
#[cfg(feature = "dev-test-switches")]
pub fn want_room_everywhere() {
    PAGES.lock().want_room_everywhere();
}

/// Pages that asked for room grow, at the frame the main thread presents
/// ([`pages::present`]).
pub fn grow_pages() {
    let m = crate::hooked_memory();
    pages::present(&PAGES, &mut InGame { m: &m });
}

fn destroy_before(call: &mut Call) {
    PAGES.lock().destroyed(call.arg(0));
}

/// The font is being destroyed.
pub fn forget(font: usize) {
    PAGES.lock().forget(font);
}

pub fn points() -> Vec<Point> {
    vec![
        Point::observe(CREATE_TEXTURE, "48 89 5c 24 20 55 56 57 41 54 41 55 41 56 41 57", "texture creation", 5, Some(create_before), None),
        Point::observe(DESTROY_TEXTURE, "40 53 48 83 ec 20 48 8b d9 ff 15 51 91 8f 00 48", "texture destruction", 1, Some(destroy_before), None),
        // The `.fnt` loader allocates its 16-byte glyph records with it.
        Point::keep(0x1595cadusize, "b9 10 00 00 00 e8 1d d6 49 00", "glyph record allocation"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{dense_line, draws_the_map, first_side, kind, page_side, record, shown, Kind, Loading, Own, BODY, HEADINGS};
    use crate::unicode::system_font::Bitmap;
    use crate::unicode::tone::Tone;

    #[test]
    fn a_font_draws_what_its_name_says() {
        // The game's own files.
        assert_eq!(kind(b"vic_18", 18), Kind::Body);
        assert_eq!(kind(b"vic_22", 22), Kind::Heading);
        assert_eq!(kind(b"tahoma_60", 100), Kind::Map);
        // The Chinese language mod's: headings on lines as tall as the
        // game's body text, and the map's on shorter ones.
        assert_eq!(kind(b"vic_22", 18), Kind::Heading);
        assert_eq!(kind(b"standard_18", 16), Kind::Body);
        assert_eq!(kind(b"tahoma_60", 88), Kind::Map);
        // A name the game does not have goes by the line.
        assert_eq!(kind(b"my_mod_font", 18), Kind::Body);
        assert_eq!(kind(b"my_mod_font", 20), Kind::Heading);
        assert_eq!(kind(b"", 14), Kind::Body);
        assert_eq!(kind(b"eu4harmony_console", 14), Kind::Console);
        assert_eq!(kind(b"Arial12", 14), Kind::Body);
        assert!(HEADINGS.iter().all(|name| !BODY.contains(name)));
    }

    #[test]
    fn text_shows_in_its_page_tinted_with_its_font() {
        // `vic_18` and `vic_18_black` on one white page.
        assert_eq!(shown(0xffffff, 0xffffffff, Kind::Body), 0xffffff);
        assert_eq!(shown(0xffffff, 0xff000000, Kind::Body), 0x000000);
        assert_eq!(shown(0xffffff, 0xffffbb00, Kind::Body), 0xffbb00);
        assert_eq!(Tone::of(shown(0xffffff, 0xffffffff, Kind::Body)), Tone::Light);
        assert_eq!(Tone::of(shown(0xffffff, 0xff000000, Kind::Heading)), Tone::Dark);
        // A grey page, each channel multiplied by the font's.
        assert_eq!(shown(0x808080, 0xff80ff00, Kind::Body), 0x408000);
        // A font whose files give no color keeps its page's.
        assert_eq!(shown(0xffffff, 0, Kind::Body), 0xffffff);
        // The map draws its page's colors: the Chinese language mod's ink, not
        // `tahoma_60`'s `0xff333333`.
        assert_eq!(shown(0x100c10, 0xff333333, Kind::Map), 0x100c10);
        assert_eq!(Tone::of(shown(0x100c10, 0xff333333, Kind::Map)), Tone::Dark);
    }

    #[test]
    fn a_denser_font_keeps_its_line_in_gui_units() {
        for line in 8..=48 {
            for density in [1.0f32, 1.1, 1.25, 1.5, 1.75, 2.0, 3.0, 4.0] {
                let (height, scale) = dense_line(line, density).unwrap();
                assert_eq!((height as f32 * scale) as i32, line, "{line} at {density}");
                assert!(height as f32 >= line as f32 * density, "{line} at {density}: {height}");
            }
        }
        assert_eq!(dense_line(18, 1.5), Some((27, 1.0 / 1.5)));
        assert_eq!(dense_line(18, 1.0), Some((18, 1.0)));
    }

    #[test]
    fn pages_are_sized_to_the_line() {
        let max = (16384, 16384);
        assert_eq!(page_side(18, 1.0, max), 1024);
        assert_eq!(page_side(48, 1.0, max), 2048);
        assert_eq!(page_side(72, 1.0, max), 4096);
        // Glyphs drawn 1.5 times as large: as many fit on a page 1.5 times
        // as wide and tall, which the device's largest texture caps.
        assert_eq!(page_side(18, 1.5, max), 1536);
        assert_eq!(page_side(48, 1.25, max), 2560);
        assert_eq!(page_side(18, 1.5, (8192, 1280)), 1280);
        // A map font's glyphs inside a 6-pixel outline, on 88-pixel lines.
        assert_eq!(page_side(88, 100.0 / 88.0, max), 4864);
        assert!(!draws_the_map(48) && draws_the_map(49));
    }

    #[test]
    fn a_page_starts_at_a_quarter_of_the_side_it_would_keep() {
        assert_eq!(first_side(1536), 768);
        assert_eq!(first_side(1024), 512);
        assert_eq!(first_side(256), 256);
    }

    #[test]
    fn a_glyph_is_recorded_where_it_was_drawn() {
        let bitmap = Bitmap { width: 12, height: 14, left: 1, top: -11, advance: 13.6, alpha: Vec::new() };
        let glyph = record(&bitmap, (40, 8), 13.4);
        assert_eq!((glyph.x, glyph.y, glyph.width, glyph.height), (40, 8, 12, 14));
        assert_eq!((glyph.x_offset, glyph.y_offset, glyph.advance), (1, 2, 14));
    }

    fn page(width: u32) -> Own {
        Own { width, height: 4, compressed: false, data: vec![0; width as usize * 16] }
    }

    #[test]
    fn a_font_gets_the_first_texture_made_while_it_loads() {
        let mut loading = Loading { font: None, page: None };
        // Outside a font's load: no font's page.
        loading.give(page(1));
        assert!(loading.finish(0x10).is_none());
        loading.begin(0x10);
        loading.give(page(2));
        loading.give(page(3));
        assert_eq!(loading.finish(0x10).unwrap().unwrap().width, 2);
        // Taken once.
        assert!(loading.finish(0x10).is_none());
        // Another font's return does not take this one's page.
        loading.begin(0x20);
        assert!(loading.finish(0x30).is_none());
        // A page of no kind the framework reads.
        loading.begin(0x40);
        assert!(loading.finish(0x40).unwrap().is_none());
    }
}
