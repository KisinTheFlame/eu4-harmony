//! Glyphs from system fonts in the game's font pages (ADR 0010): characters a
//! bitmap font lacks and, with the system font look, every character. A font
//! draws from one page texture, which the game reads from the page's wrapper
//! at every draw, so the framework gives a page a texture of its own with room
//! for system glyphs. Glyph positions and the page's size are baked into the
//! game's caches, so the page is sized once, when it loads, and a glyph is
//! drawn into it before its first use (docs/research/font-textures.md).
//!
//! - Bitmap fonts first ("Keep the fonts as they are" in the settings
//!   window): the page gets 1024 more rows below it, where a character the
//!   font lacks is drawn the first time it is needed. A DXT5 page (the
//!   Chinese language mod's) stays DXT5, its new glyphs written as blocks
//!   (`dxt`).
//! - The system font look ("System fonts"): a new page holds every
//!   character, drawn from the system font; the font's own glyphs are
//!   replaced. The game stretches a font's glyphs by the interface scale,
//!   with a linear filter, so an interface font's glyphs are drawn that many
//!   times larger and its scale divided to match: they reach the window a
//!   texel to a pixel, as sharp as the system font draws them ([`density`]).
//!
//! Fonts with lines taller than 48 pixels are the map's and draw in the map
//! font, inside the outline the font's own glyphs have when the player keeps
//! it (`outline`); the rest in the body font. Pages live in video memory,
//! like the game's own textures, which only Direct3D 9Ex keeps across a
//! device reset; on a plain Direct3D 9 device nothing is filled. They are
//! written through small system-memory textures: pages made dynamic and
//! locked a band at a time made Direct3D keep several copies of each, 425 MB
//! for the map's.
use super::atlas::Page;
use super::costs;
use super::dxt;
use super::game_text;
use super::glyphs::{self, Glyph};
use super::outline::Outline;
use super::system_font::SystemFonts;
use crate::hooks::HookFrame;
use crate::memory::Memory;
use crate::objects::text::{self as objects, font::OWN_GLYPHS, texture_description, texture_level, textures};
use crate::points::Point;
use crate::settings::{self, TextAppearance};
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{LazyLock, Mutex, MutexGuard};
use windows::core::Interface;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Direct3D9::*;

/// Creates a texture: renderer, wrapper, description, mip levels, count.
const CREATE_TEXTURE: usize = 0x16d7aa0;
/// Releases a texture and frees its wrapper (wrapper).
const DESTROY_TEXTURE: usize = 0x16d80f0;
/// The game's `operator new`, which allocates glyph records here and the
/// recruit command in `game::recruiting`.
pub const ALLOCATE: usize = 0x1a332d4;
/// The engine's format numbers for a 32-bit page (A8R8G8B8) and a DXT5 one
/// (0x16f9850 maps them to Direct3D's).
const ARGB: u32 = 0xb;
const DXT5: [u32; 2] = [5, 6];
const EXTRA_ROWS: u32 = 1024;
const MAP_LINE: i32 = 48;

/// A page as the game decoded it.
struct Own {
    width: u32,
    height: u32,
    /// DXT5 blocks; A8R8G8B8 rows otherwise.
    compressed: bool,
    /// Whether the texture has smaller levels too. Only the first is kept:
    /// such a page is not grown, only read for how its glyphs look.
    mipmapped: bool,
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

struct Fill {
    device: IDirect3DDevice9,
    texture: IDirect3DTexture9,
    /// The system-memory texture glyphs are written through, and its size.
    staging: Option<(u32, u32, IDirect3DTexture9)>,
    compressed: bool,
    width: u32,
    height: u32,
    atlas: Page,
    /// Rows above the system glyphs: the font's own page, or none.
    top: u32,
    family: String,
    /// The colour the glyphs are drawn in.
    ink: u32,
    /// The outline drawn around them.
    outline: Option<Outline>,
    /// Em sizes for characters before `CJK` and for the rest, in pixels.
    letter_em: f32,
    em: f32,
    /// From the top of a line to the baseline, in pixels.
    base: f32,
    /// How many pixels of the page a GUI unit takes (see [`density`]).
    density: f32,
    drawn: HashMap<u32, Option<Glyph>>,
    full: bool,
}

// Pages are made only on a device made multithreaded (see `device`), which
// takes its lock in each call, so any thread that draws text may fill one.
unsafe impl Send for Fill {}

static SYSTEM: LazyLock<Option<SystemFonts>> = LazyLock::new(|| SystemFonts::new().ok());
/// By page wrapper.
static PAGES: LazyLock<Mutex<HashMap<usize, Fill>>> = LazyLock::new(Default::default);
/// The page wrapper each filled font draws from, by font.
static OWNERS: LazyLock<Mutex<HashMap<usize, usize>>> = LazyLock::new(Default::default);

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
    /// framework can grow.
    fn finish(&mut self, font: usize) -> Option<Option<Own>> {
        let done = std::mem::replace(self, Loading { font: None, page: None });
        (done.font == Some(font)).then_some(done.page)
    }
}

fn lock<T>(m: &'static LazyLock<Mutex<T>>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
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

/// A system-memory texture to write a page of `format` through.
fn staging(device: &IDirect3DDevice9, format: D3DFORMAT, width: u32, height: u32) -> crate::Result<IDirect3DTexture9> {
    let mut texture = None;
    unsafe { device.CreateTexture(width, height, 1, 0, format, D3DPOOL_SYSTEMMEM, &mut texture, std::ptr::null_mut()) }
        .map_err(|e| format!("staging texture: {e}"))?;
    texture.ok_or_else(|| "no staging texture".to_string())
}

/// Writes `width` × `height` pixels at (x, y) of `page`, given as rows of
/// A8R8G8B8 pixels or of DXT5 blocks (on a DXT5 page (x, y) is a whole
/// block), through `staging`, a system-memory texture of the page's format at
/// least that large, from which the device copies them into the page.
#[expect(clippy::too_many_arguments, reason = "the device, the textures, the format, a rectangle and its pixels; a struct would only rename them")]
fn put(
    device: &IDirect3DDevice9,
    page: &IDirect3DTexture9,
    staging: &IDirect3DTexture9,
    compressed: bool,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    data: &[u8],
) -> windows::core::Result<()> {
    if width == 0 || height == 0 {
        return Ok(());
    }
    let rows = if compressed { height.div_ceil(4) } else { height } as usize;
    let row = row_bytes(compressed, width);
    // A DXT5 page takes whole blocks only.
    let (width, height) = if compressed { (width.next_multiple_of(4), height.next_multiple_of(4)) } else { (width, height) };
    let rect = RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 };
    let at = POINT { x: x as i32, y: y as i32 };
    let mut locked = D3DLOCKED_RECT::default();
    unsafe {
        staging.LockRect(0, &mut locked, &rect, 0)?;
        for n in 0..rows {
            let line = (locked.pBits as *mut u8).add(n * locked.Pitch as usize);
            std::ptr::copy_nonoverlapping(data.as_ptr().add(n * row), line, row);
        }
        staging.UnlockRect(0)?;
        device.UpdateSurface(&staging.GetSurfaceLevel(0)?, &rect, &page.GetSurfaceLevel(0)?, &at)
    }
}

impl Fill {
    /// Writes a glyph's pixels at (x, y) of the page, through a staging
    /// texture kept for the page and made larger when a glyph needs it.
    fn put(&mut self, x: u32, y: u32, width: u32, height: u32, data: &[u8]) -> crate::Result<()> {
        if !matches!(&self.staging, Some((w, h, _)) if *w >= width && *h >= height) {
            let (w, h) = (width.next_multiple_of(64), height.next_multiple_of(64));
            let format = if self.compressed { D3DFMT_DXT5 } else { D3DFMT_A8R8G8B8 };
            self.staging = Some((w, h, staging(&self.device, format, w, h)?));
        }
        let (_, _, staging) = self.staging.as_ref().expect("made above");
        put(&self.device, &self.texture, staging, self.compressed, x, y, width, height, data).map_err(|e| e.to_string())
    }
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
    /// From the top of a line to the top and the bottom (the baseline) of its
    /// ink, in pixels. Some fonts' glyphs fill the whole line, so the
    /// glyph's own box does not tell where the baseline is.
    top: i32,
    baseline: i32,
    /// The outline around it, if it has one.
    outline: Option<Outline>,
}

fn letter(m: &Memory, font: usize, own: &Own) -> Option<Letter> {
    let record = m.ptr(font + OWN_GLYPHS + b'H' as usize * 8).ok().filter(|&r| r != 0)?;
    let bytes = m.bytes(record, std::mem::size_of::<Glyph>()).ok()?;
    let h = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const Glyph) };
    let width = (h.width as u32).min(own.width.saturating_sub(h.x as u32));
    let height = (h.height as u32).min(own.height.saturating_sub(h.y as u32));
    let pixels: Vec<u32> = (0..height).flat_map(|y| (0..width).map(move |x| own.pixel(h.x as u32 + x, h.y as u32 + y))).collect();
    let (mut best, mut ink, mut rows) = (0, 0, None::<(u32, u32)>);
    for (i, &pixel) in pixels.iter().enumerate() {
        if pixel >> 24 > best {
            (best, ink) = (pixel >> 24, pixel & 0x00ff_ffff);
        }
        if pixel >> 24 >= 0x80 {
            let y = i as u32 / width;
            rows = Some(rows.map_or((y, y), |(top, _)| (top, y)));
        }
    }
    let (top, bottom) = rows?;
    Some(Letter {
        ink,
        top: h.y_offset as i32 + top as i32,
        baseline: h.y_offset as i32 + bottom as i32 + 1,
        outline: Outline::measure(&pixels, width, height, ink),
    })
}

/// Characters before the CJK blocks are sized to the font's own capitals;
/// the rest to its line.
const CJK: u32 = 0x2e80;

/// The em size `scalar` is drawn at, given the font's for letters and for
/// the rest.
fn em_for(scalar: u32, letter_em: f32, em: f32) -> f32 {
    if scalar < CJK {
        letter_em
    } else {
        em
    }
}

/// The room a glyph `width` by `height` pixels takes in the atlas, which adds
/// a pixel of padding: on a DXT5 page it and its padding take whole blocks,
/// so every place is a whole block too.
fn room(width: u32, height: u32, compressed: bool) -> (u32, u32) {
    match compressed {
        true => ((width + 1).next_multiple_of(4) - 1, (height + 1).next_multiple_of(4) - 1),
        false => (width, height),
    }
}

/// The game's record for `bitmap` drawn at (`x`, `y`) of the system glyphs'
/// area, which starts `top` rows down the page, on lines whose baseline is
/// `base` pixels down.
fn record(bitmap: &super::system_font::Bitmap, (x, y): (u32, u32), top: u32, base: f32) -> Glyph {
    Glyph {
        x: x as u16,
        y: (top + y) as u16,
        width: bitmap.width as u16,
        height: bitmap.height as u16,
        x_offset: bitmap.left as i16,
        y_offset: (base + bitmap.top as f32).round() as i16,
        advance: bitmap.advance.round() as i16,
        kerned: 0,
        page: 0,
    }
}

fn draw(fill: &mut Fill, scalar: u32) -> Option<Glyph> {
    let fonts = SYSTEM.as_ref()?;
    let family = fonts.family_for(&fill.family, scalar)?.to_string();
    let bitmap = fonts.rasterize(&family, scalar, em_for(scalar, fill.letter_em, fill.em)).ok()?;
    let (bitmap, pixels) = match &fill.outline {
        Some(outline) if bitmap.width > 0 && bitmap.height > 0 => outline.around(&bitmap, fill.ink),
        _ => {
            let pixels = inked(&bitmap.alpha, fill.ink);
            (bitmap, pixels)
        }
    };
    let (mut x, mut y) = (0, 0);
    if bitmap.width > 0 && bitmap.height > 0 {
        let (w, h) = room(bitmap.width, bitmap.height, fill.compressed);
        (x, y) = match fill.atlas.place(w, h) {
            Some(spot) => spot,
            None => {
                if !std::mem::replace(&mut fill.full, true) {
                    log!("fill: a {}x{} page is full; characters past it show as '?'", fill.width, fill.height);
                }
                return None;
            }
        };
        let pixels = match fill.compressed {
            true => dxt::encode_bitmap(bitmap.width, bitmap.height, &pixels).concat(),
            false => pixels.iter().flat_map(|p| p.to_le_bytes()).collect(),
        };
        if let Err(e) = fill.put(x, fill.top + y, bitmap.width, bitmap.height, &pixels) {
            log!("fill: cannot write a glyph: {e}");
            return None;
        }
    }
    Some(record(&bitmap, (x, y), fill.top, fill.base))
}

fn glyph_of(fill: &mut Fill, scalar: u32) -> Option<Glyph> {
    if let Some(found) = fill.drawn.get(&scalar) {
        return *found;
    }
    let glyph = costs::GLYPHS.time(|| draw(fill, scalar));
    fill.drawn.insert(scalar, glyph);
    glyph
}

/// The glyph a filled `font` draws for `scalar`, which its bitmap glyphs
/// lack: drawn into its page the first time. Null when the font is not filled
/// or no system font has the character.
pub fn glyph(font: usize, scalar: u32) -> *const Glyph {
    let Some(wrapper) = lock(&OWNERS).get(&font).copied() else { return std::ptr::null() };
    let glyph = match lock(&PAGES).get_mut(&wrapper) {
        Some(fill) => glyph_of(fill, scalar),
        None => None,
    };
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

/// The em size and baseline that fit `family` to a line of `line` pixels.
fn metrics(fonts: &SystemFonts, family: &str, line: f32) -> (f32, f32) {
    let (ascent, descent) = fonts.line_metrics(family).unwrap_or((0.9, 0.25));
    let em = line / (ascent + descent);
    (em, em * ascent)
}

/// Which rows of a font's own page (`own_height` pixels tall, rows of `per`
/// pixel rows each) fall in the `rows` pixel rows from `band`.
fn own_rows(own_height: u32, band: u32, rows: u32, per: u32) -> std::ops::Range<usize> {
    (band.min(own_height) / per) as usize..((band + rows).min(own_height) / per) as usize
}

/// Whether a font with lines `line` pixels tall is one of the map's.
fn draws_the_map(line: i32) -> bool {
    line > MAP_LINE
}

/// The most pixels of a page a GUI unit takes.
const MOST_DENSE: f32 = 4.0;

/// How many pixels of its page a GUI unit of `font` takes under the system
/// font look: the interface scale the font loads at, when it is above 1 (up
/// to [`MOST_DENSE`]). The game draws a font's glyphs that many times larger
/// than their pixels, with a linear filter, which blurs them; drawn that much
/// larger, and with the font's scale divided by as much ([`dense_line`]),
/// they reach the window a texel to a pixel.
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

/// The width and height of the page a font gets, and the row its system
/// glyphs start at: its own page (`own`, width and height) with
/// [`EXTRA_ROWS`] below when the device allows `max` (width, height), or a
/// square one sized to its `line` in GUI units, and `growth` times as wide
/// and tall, so that it holds as many glyphs `growth` times as large: drawn
/// denser, or inside an outline.
fn page_size(own: Option<(u32, u32)>, line: i32, growth: f32, max: (u32, u32)) -> crate::Result<(u32, u32, u32)> {
    match own {
        // A DXT5 page is whole blocks, so the rows below start on one.
        Some((width, height)) if height + EXTRA_ROWS <= max.1 => Ok((width, height + EXTRA_ROWS, height)),
        Some(_) => Err("page too tall to grow".into()),
        None => {
            let side: u32 = match line {
                ..=24 => 1024,
                25..=48 => 2048,
                _ => 4096,
            };
            let side = ((side as f32 * growth / 256.0).ceil() as u32 * 256).min(max.0).min(max.1);
            Ok((side, side, 0))
        }
    }
}

/// A page for `font`: its own pixels (`own`, when the game decoded them) with
/// room below (bitmap fonts first), or an empty one (the system font look).
fn make_page(m: &Memory, font: usize, device: &IDirect3DDevice9, own: Option<Own>, system: bool) -> crate::Result<Fill> {
    let fonts = SYSTEM.as_ref().ok_or("no system fonts")?;
    let line = m.i32(font + objects::font::LINE_HEIGHT)?.max(1);
    let current = settings::current();
    let map = draws_the_map(line);
    let family = if map { current.map_font } else { current.body_font };
    let letter = own.as_ref().and_then(|own| letter(m, font, own));
    let own = if system { None } else { own };
    let outline = letter.as_ref().and_then(|l| l.outline.clone()).filter(|_| map && current.map_outline);
    // The map's names are drawn on the map, which the interface scale does
    // not stretch.
    let density = if system && !map { density(m, font) } else { 1.0 };
    let (em, mut base) = metrics(fonts, &family, line as f32 * density);
    let mut caps = D3DCAPS9::default();
    unsafe { device.GetDeviceCaps(&mut caps) }.map_err(|e| format!("device caps: {e}"))?;
    let compressed = own.as_ref().is_some_and(|o| o.compressed);
    let reach = outline.as_ref().map_or(0, |o| o.reach());
    let growth = (line as f32 * density + 2.0 * reach as f32) / line as f32;
    let (width, height, top) = page_size(own.as_ref().map(|o| (o.width, o.height)), line, growth, (caps.MaxTextureWidth, caps.MaxTextureHeight))?;
    let format = if compressed { D3DFMT_DXT5 } else { D3DFMT_A8R8G8B8 };
    let mut texture = None;
    let creating = crate::heap::Change::begin();
    unsafe {
        device.CreateTexture(width, height, 1, 0, format, D3DPOOL_DEFAULT, &mut texture, std::ptr::null_mut()).map_err(|e| format!("page texture: {e}"))?;
    }
    let texture = texture.ok_or("no page texture")?;
    let created = creating.end();
    let writing = crate::heap::Change::begin();
    // The font's own page on top, the rest clear (all zero bytes, in either
    // format), a band of rows at a time through one staging texture.
    const BAND: u32 = 256;
    let band = staging(device, format, width, BAND)?;
    let (row, rows_per_row) = (row_bytes(compressed, width), if compressed { 4 } else { 1 });
    for top in (0..height).step_by(BAND as usize) {
        let rows = BAND.min(height - top);
        let mut data = vec![0u8; (rows / rows_per_row) as usize * row];
        if let Some(own) = &own {
            let range = own_rows(own.height, top, rows, rows_per_row);
            let (from, to) = (range.start * row, range.end * row);
            data[..to - from].copy_from_slice(&own.data[from..to]);
        }
        put(device, &texture, &band, compressed, 0, top, width, rows, &data).map_err(|e| format!("page pixels: {e}"))?;
    }
    log!("fill: {width}x{height} page texture: creating it {created}; writing it {}", writing.end());
    // Without the page's pixels: black for a map font, white otherwise.
    let ink = letter.as_ref().map_or(if line > MAP_LINE { 0 } else { 0x00ff_ffff }, |l| l.ink);
    let mut letter_em = em;
    if let (Some(l), false) = (&letter, system) {
        // Characters drawn among the font's own letters sit on their
        // baseline, and letters are as tall as its capitals.
        base = l.baseline as f32;
        if let Some(cap) = fonts.cap_height(&family) {
            letter_em = (l.baseline - l.top) as f32 / cap;
        }
    }
    Ok(Fill {
        device: device.clone(),
        texture,
        staging: None,
        compressed,
        width,
        height,
        atlas: Page::new(width, height - top),
        top,
        family,
        ink,
        outline,
        letter_em,
        em,
        base,
        density,
        drawn: HashMap::new(),
        full: false,
    })
}

/// The system font look: the font's glyphs for Windows-1252 characters
/// become system glyphs, written into its own records (or records allocated
/// as the game allocates them, which it frees with the font).
fn replace_own_glyphs(m: &Memory, font: usize, fill: &mut Fill) -> crate::Result<()> {
    for byte in 0x20u8..=0xff {
        let (c, _) = game_text::char_at(&[byte]);
        if c.is_control() {
            continue;
        }
        let Some(glyph) = glyph_of(fill, c as u32) else { continue };
        let slot = font + OWN_GLYPHS + byte as usize * 8;
        let mut record = m.ptr(slot)?;
        if record == 0 {
            record = m.call(m.at(ALLOCATE), &[std::mem::size_of::<Glyph>()])?;
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

extern "C" fn create_before(frame: *mut HookFrame) {
    crate::guarded("font page pixels", || unsafe {
        let args = &(*frame).args;
        let (description, levels, count) = (args[2] as *const u8, args[3] as *const u8, args[4] as u32);
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
            l.give(Own { width, height, compressed, mipmapped: count > 1, data: std::slice::from_raw_parts(data, size).to_vec() });
        });
    })
}

/// `font` has loaded: fills its page. Returns whether the font now draws
/// every character from the system font, so that the glyphs its file brought
/// no longer apply.
/// A font's load ended in a fault: nothing is filled in for it.
pub fn abandoned(font: usize) {
    LOADING.with(|l| l.borrow_mut().finish(font));
}

pub fn loaded(font: usize) -> bool {
    let Some(own) = LOADING.with(|l| l.borrow_mut().finish(font)) else { return false };
    let system = settings::current().text_appearance == TextAppearance::System;
    let m = crate::hooked_memory();
    let result = (|| -> crate::Result<bool> {
        let (manager, entry) = page_entry(&m, font)?;
        let wrapper = m.ptr(entry)?;
        if wrapper == 0 || m.ptr(wrapper)? == 0 {
            return Ok(false);
        }
        let mut pages = lock(&PAGES);
        if let Entry::Vacant(vacant) = pages.entry(wrapper) {
            // A page loaded before, in another format or with smaller
            // levels keeps its own glyphs only.
            if !system && own.as_ref().is_none_or(|o| o.mipmapped) {
                return Ok(false);
            }
            let fill = costs::PAGES.time(|| make_page(&m, font, &device(&m, manager)?, own, system))?;
            // The wrapper holds the game's reference: it gets one of
            // ours, and its old texture is released.
            let old = m.ptr(wrapper)? as *mut c_void;
            unsafe {
                *(wrapper as *mut *mut c_void) = fill.texture.clone().into_raw();
                drop(IDirect3DTexture9::from_raw(old));
                *((entry + textures::ENTRY_SIZE) as *mut u32) = fill.width;
                *((entry + textures::ENTRY_SIZE + 4) as *mut u32) = fill.height;
            }
            let look = match (system, fill.compressed) {
                (true, _) => "system",
                (false, true) => "grown DXT5",
                (false, false) => "grown",
            };
            let outline = fill.outline.as_ref().map_or("none".to_string(), |o| format!("{:06x}, {} px", o.color, o.reach()));
            log!(
                "fill: {look} page {}x{} for {:?} at {:.1} px (letters {:.1} px, baseline {:.0}), {:.2} px a GUI unit, ink {:06x}, outline {outline}",
                fill.width,
                fill.height,
                fill.family,
                fill.em,
                fill.letter_em,
                fill.base,
                fill.density,
                fill.ink
            );
            vacant.insert(fill);
        }
        let fill = pages.get_mut(&wrapper).expect("just made");
        unsafe {
            *((font + objects::font::PAGE_SIZE) as *mut u32) = fill.width;
            *((font + objects::font::PAGE_SIZE + 4) as *mut u32) = fill.height;
        }
        if fill.density != 1.0 {
            scale_font(&m, font, fill.density)?;
        }
        lock(&OWNERS).insert(font, wrapper);
        if system {
            replace_own_glyphs(&m, font, fill)?;
        }
        Ok(system)
    })();
    result.unwrap_or_else(|e| {
        log!("fill: font page left as it is: {e}");
        false
    })
}

extern "C" fn destroy_before(frame: *mut HookFrame) {
    crate::guarded("font page release", || {
        let wrapper = unsafe { (*frame).args[0] };
        if lock(&PAGES).remove(&wrapper).is_some() {
            lock(&OWNERS).retain(|_, w| *w != wrapper);
        }
    })
}

/// The font is being destroyed.
pub fn forget(font: usize) {
    lock(&OWNERS).remove(&font);
}

pub fn points() -> Vec<Point> {
    vec![
        Point::observe(CREATE_TEXTURE, "48 89 5c 24 20 55 56 57 41 54 41 55 41 56 41 57", "texture creation", 5, Some(create_before), None),
        Point::observe(DESTROY_TEXTURE, "40 53 48 83 ec 20 48 8b d9 ff 15 51 91 8f 00 48", "texture destruction", 1, Some(destroy_before), None),
        Point::keep(ALLOCATE, "40 53 48 83 ec 20 48 8b d9 eb 0f 48 8b cb e8 95", "game allocator"),
        // The `.fnt` loader allocates its 16-byte glyph records with it.
        Point::keep(0x1595cadusize, "b9 10 00 00 00 e8 1d d6 49 00", "glyph record allocation"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{dense_line, draws_the_map, em_for, own_rows, page_size, record, room, Loading, Own};
    use crate::unicode::system_font::Bitmap;

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
    fn pages_grow_below_or_are_sized_to_the_line() {
        let max = (16384, 16384);
        assert_eq!(page_size(Some((2048, 4096)), 60, 1.0, max).unwrap(), (2048, 5120, 4096));
        assert!(page_size(Some((2048, 16000)), 60, 1.0, max).is_err());
        assert_eq!(page_size(None, 18, 1.0, max).unwrap(), (1024, 1024, 0));
        assert_eq!(page_size(None, 48, 1.0, max).unwrap(), (2048, 2048, 0));
        assert_eq!(page_size(None, 72, 1.0, max).unwrap(), (4096, 4096, 0));
        // Glyphs drawn 1.5 times as large: as many fit on a page 1.5 times
        // as wide and tall, which the device's largest texture caps.
        assert_eq!(page_size(None, 18, 1.5, max).unwrap(), (1536, 1536, 0));
        assert_eq!(page_size(None, 48, 1.25, max).unwrap(), (2560, 2560, 0));
        assert_eq!(page_size(None, 18, 1.5, (8192, 1280)).unwrap(), (1280, 1280, 0));
        // A map font's glyphs inside a 6-pixel outline, on 88-pixel lines.
        assert_eq!(page_size(None, 88, 100.0 / 88.0, max).unwrap(), (4864, 4864, 0));
        assert!(!draws_the_map(48) && draws_the_map(49));
    }

    #[test]
    fn glyphs_are_placed_and_sized_as_the_page_needs() {
        // Latin letters go by the capitals' size, the rest by the line.
        assert_eq!(em_for('H' as u32, 11.0, 14.0), 11.0);
        assert_eq!(em_for('中' as u32, 11.0, 14.0), 14.0);
        // With its padding a glyph takes whole 4x4 blocks on a DXT5 page.
        assert_eq!(room(10, 13, true), (11, 15));
        assert_eq!(room(11, 15, true), (11, 15));
        assert_eq!(room(10, 13, false), (10, 13));
        let bitmap = Bitmap { width: 12, height: 14, left: 1, top: -11, advance: 13.6, alpha: Vec::new() };
        let glyph = record(&bitmap, (40, 8), 1024, 13.4);
        assert_eq!((glyph.x, glyph.y, glyph.width, glyph.height), (40, 1032, 12, 14));
        assert_eq!((glyph.x_offset, glyph.y_offset, glyph.advance), (1, 2, 14));
    }

    fn page(width: u32) -> Own {
        Own { width, height: 4, compressed: false, mipmapped: false, data: vec![0; width as usize * 16] }
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
        // A page of no kind the framework grows.
        loading.begin(0x40);
        assert!(loading.finish(0x40).unwrap().is_none());
    }

    #[test]
    fn bands_past_the_own_page_copy_nothing() {
        // A 4096-row DXT5 page grown by 1024 rows, copied 256 rows at a time.
        assert_eq!(own_rows(4096, 0, 256, 4), 0..64);
        assert_eq!(own_rows(4096, 3840, 256, 4), 960..1024);
        assert_eq!(own_rows(4096, 4096, 256, 4), 1024..1024);
        assert_eq!(own_rows(4096, 4864, 256, 4), 1024..1024);
        // A 32-bit page 100 rows tall.
        assert_eq!(own_rows(100, 0, 256, 1), 0..100);
    }
}
