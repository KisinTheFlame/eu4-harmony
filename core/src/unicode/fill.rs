//! Glyphs from system fonts in the game's font pages (ADR 0010): characters a
//! bitmap font lacks and, with the system font look, every character. A font
//! draws from one page texture, which the game reads from the page's wrapper
//! at every draw, so the framework gives a page a texture of its own with room
//! for system glyphs. Glyph positions and the page's size are baked into the
//! game's caches, so the page is sized once, when it loads, and a glyph is
//! drawn into it before its first use (docs/research/font-textures.md).
//!
//! - Bitmap fonts first (沿用原字体): the page gets 1024 more rows below it,
//!   where a character the font lacks is drawn the first time it is needed. A
//!   DXT5 page (the Chinese language mod's) stays DXT5, its new glyphs written
//!   as blocks (`dxt`).
//! - The system font look (系统字体): a new page holds every character, drawn
//!   from the system font; the font's own glyphs are replaced.
//!
//! Fonts with lines taller than 48 pixels are the map's and draw in the map
//! font; the rest in the body font. Pages are dynamic textures, which only
//! Direct3D 9Ex keeps across a device reset; on a plain Direct3D 9 device
//! nothing is filled.
use super::atlas::Page;
use super::dxt;
use super::encoding;
use super::glyphs::{self, Glyph};
use super::system_font::SystemFonts;
use crate::hooks::HookFrame;
use crate::memory::Memory;
use crate::points::Point;
use crate::settings::{self, TextAppearance};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{LazyLock, Mutex, MutexGuard};
use windows::core::Interface;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D9::*;

/// The font method that loads its page, then parses its `.fnt` (font).
const LOAD_PAGE: usize = 0x15953c0;
/// Creates a texture: renderer, wrapper, description, mip levels, count.
const CREATE_TEXTURE: usize = 0x16d7aa0;
/// Releases a texture and frees its wrapper (wrapper).
const DESTROY_TEXTURE: usize = 0x16d80f0;
/// The game's `operator new`, which allocates glyph records.
const ALLOCATE: usize = 0x1a332d4;
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
    texture: IDirect3DTexture9,
    compressed: bool,
    width: u32,
    height: u32,
    atlas: Page,
    /// Rows above the system glyphs: the font's own page, or none.
    top: u32,
    family: String,
    /// The colour the glyphs are drawn in.
    ink: u32,
    /// Em sizes for characters before `CJK` and for the rest, in pixels.
    letter_em: f32,
    em: f32,
    /// From the top of a line to the baseline, in pixels.
    base: f32,
    drawn: HashMap<u32, Option<Glyph>>,
    full: bool,
}

// The device was made multithreaded; each call takes its lock.
unsafe impl Send for Fill {}

static SYSTEM: LazyLock<Option<SystemFonts>> = LazyLock::new(|| SystemFonts::new().ok());
/// By page wrapper.
static PAGES: LazyLock<Mutex<HashMap<usize, Fill>>> = LazyLock::new(Default::default);
/// The page wrapper each filled font draws from, by font.
static OWNERS: LazyLock<Mutex<HashMap<usize, usize>>> = LazyLock::new(Default::default);

thread_local! {
    /// The font whose page is loading on this thread, and the page once the
    /// game has decoded it.
    static LOADING: RefCell<Option<(usize, Option<Own>)>> = const { RefCell::new(None) };
}

fn lock<T>(m: &'static LazyLock<Mutex<T>>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn memory() -> Memory {
    *crate::memory().expect("hooks run after startup")
}

/// The texture manager's entry for `font`'s page: the manager is
/// `[[font+0x48]+0x480]`, its entries 0x48 bytes at `+8`, the wrapper first,
/// the size at `+0x28`.
fn page_entry(m: &Memory, font: usize) -> crate::Result<(usize, usize)> {
    let index = m.i32(font + 0x970)?;
    let manager = m.ptr(m.ptr(font + 0x48)? + 0x480)?;
    let (begin, end) = (m.ptr(manager + 8)?, m.ptr(manager + 0x10)?);
    if index < 0 || begin + (index as usize + 1) * 0x48 > end {
        return Err("font page index out of range".into());
    }
    Ok((manager, begin + index as usize * 0x48))
}

/// The device, through the manager's renderer (`+0x60`, its device at `+8`).
fn device(m: &Memory, manager: usize) -> crate::Result<IDirect3DDevice9> {
    let raw = m.ptr(m.ptr(manager + 0x60)? + 8)? as *mut c_void;
    let device = unsafe { IDirect3DDevice9::from_raw_borrowed(&raw) }.ok_or("no device")?.clone();
    device.cast::<IDirect3DDevice9Ex>().map_err(|_| "not a Direct3D 9Ex device".to_string())?;
    Ok(device)
}

/// The bytes of one row of a `width`-pixel page: of pixels, or of blocks.
fn row_bytes(compressed: bool, width: u32) -> usize {
    if compressed { width.div_ceil(4) as usize * dxt::BLOCK } else { width as usize * 4 }
}

/// Writes `width` × `height` pixels at (x, y), given as rows of A8R8G8B8
/// pixels or of DXT5 blocks; on a DXT5 page (x, y) is a whole block.
fn put(texture: &IDirect3DTexture9, compressed: bool, x: u32, y: u32, width: u32, height: u32, data: &[u8]) -> windows::core::Result<()> {
    if width == 0 || height == 0 {
        return Ok(());
    }
    let rows = if compressed { height.div_ceil(4) } else { height } as usize;
    let row = row_bytes(compressed, width);
    // A DXT5 page locks whole blocks only.
    let (width, height) = if compressed { (width.next_multiple_of(4), height.next_multiple_of(4)) } else { (width, height) };
    let rect = RECT { left: x as i32, top: y as i32, right: (x + width) as i32, bottom: (y + height) as i32 };
    let mut locked = D3DLOCKED_RECT::default();
    unsafe {
        texture.LockRect(0, &mut locked, &rect, 0)?;
        for n in 0..rows {
            let line = (locked.pBits as *mut u8).add(n * locked.Pitch as usize);
            std::ptr::copy_nonoverlapping(data.as_ptr().add(n * row), line, row);
        }
        texture.UnlockRect(0)
    }
}

/// `ink`, with the coverage as alpha, as A8R8G8B8 pixels.
fn inked(alpha: &[u8], ink: u32) -> Vec<u8> {
    alpha.iter().flat_map(|&a| ((a as u32) << 24 | ink).to_le_bytes()).collect()
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
}

fn letter(m: &Memory, font: usize, own: &Own) -> Option<Letter> {
    let record = m.ptr(font + 0x120 + b'H' as usize * 8).ok().filter(|&r| r != 0)?;
    let h = unsafe { *(record as *const Glyph) };
    let (mut best, mut ink, mut rows) = (0, 0, None::<(u32, u32)>);
    for y in 0..(h.height as u32).min(own.height.saturating_sub(h.y as u32)) {
        for x in 0..(h.width as u32).min(own.width.saturating_sub(h.x as u32)) {
            let pixel = own.pixel(h.x as u32 + x, h.y as u32 + y);
            if pixel >> 24 > best {
                (best, ink) = (pixel >> 24, pixel & 0x00ff_ffff);
            }
            if pixel >> 24 >= 0x80 {
                rows = Some(rows.map_or((y, y), |(top, _)| (top, y)));
            }
        }
    }
    let (top, bottom) = rows?;
    Some(Letter { ink, top: h.y_offset as i32 + top as i32, baseline: h.y_offset as i32 + bottom as i32 + 1 })
}

/// Characters before the CJK blocks are sized to the font's own capitals;
/// the rest to its line.
const CJK: u32 = 0x2e80;

fn draw(fill: &mut Fill, scalar: u32) -> Option<Glyph> {
    let fonts = SYSTEM.as_ref()?;
    let family = fonts.family_for(&fill.family, scalar)?.to_string();
    let em = if scalar < CJK { fill.letter_em } else { fill.em };
    let bitmap = fonts.rasterize(&family, scalar, em).ok()?;
    let (mut x, mut y) = (0, 0);
    if bitmap.width > 0 && bitmap.height > 0 {
        // On a DXT5 page a glyph and its padding take whole blocks, so every
        // place is a whole block too.
        let (w, h) = match fill.compressed {
            true => ((bitmap.width + 1).next_multiple_of(4) - 1, (bitmap.height + 1).next_multiple_of(4) - 1),
            false => (bitmap.width, bitmap.height),
        };
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
            true => dxt::encode_bitmap(bitmap.width, bitmap.height, &bitmap.alpha, fill.ink).concat(),
            false => inked(&bitmap.alpha, fill.ink),
        };
        if let Err(e) = put(&fill.texture, fill.compressed, x, fill.top + y, bitmap.width, bitmap.height, &pixels) {
            log!("fill: cannot write a glyph: {e}");
            return None;
        }
    }
    Some(Glyph {
        x: x as u16,
        y: (fill.top + y) as u16,
        width: bitmap.width as u16,
        height: bitmap.height as u16,
        x_offset: bitmap.left as i16,
        y_offset: (fill.base + bitmap.top as f32).round() as i16,
        advance: bitmap.advance.round() as i16,
        kerned: 0,
        page: 0,
    })
}

fn glyph_of(fill: &mut Fill, scalar: u32) -> Option<Glyph> {
    if let Some(found) = fill.drawn.get(&scalar) {
        return *found;
    }
    let glyph = draw(fill, scalar);
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
        Some(g) => {
            glyphs::insert(font + 0x120, scalar, g);
            glyphs::get(font + 0x120, scalar)
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

/// A page for `font`: its own pixels (`own`, when the game decoded them) with
/// room below (bitmap fonts first), or an empty one (the system font look).
fn make_page(m: &Memory, font: usize, device: &IDirect3DDevice9, own: Option<Own>, system: bool) -> crate::Result<Fill> {
    let fonts = SYSTEM.as_ref().ok_or("no system fonts")?;
    let line = m.i32(font + 0x960)?.max(1);
    let current = settings::current();
    let family = if line > MAP_LINE { current.map_font } else { current.body_font };
    let letter = own.as_ref().and_then(|own| letter(m, font, own));
    let own = if system { None } else { own };
    let (em, mut base) = metrics(fonts, &family, line as f32);
    let mut caps = D3DCAPS9::default();
    unsafe { device.GetDeviceCaps(&mut caps) }.map_err(|e| format!("device caps: {e}"))?;
    let compressed = own.as_ref().is_some_and(|o| o.compressed);
    let (width, height, top) = match &own {
        // A DXT5 page is whole blocks, so the rows below start on one.
        Some(o) if o.height + EXTRA_ROWS <= caps.MaxTextureHeight => (o.width, o.height + EXTRA_ROWS, o.height),
        Some(_) => return Err("page too tall to grow".into()),
        None => {
            let side = match line {
                ..=24 => 1024,
                25..=48 => 2048,
                _ => 4096,
            };
            (side, side, 0)
        }
    };
    let format = if compressed { D3DFMT_DXT5 } else { D3DFMT_A8R8G8B8 };
    let mut texture = None;
    unsafe {
        device
            .CreateTexture(width, height, 1, D3DUSAGE_DYNAMIC as u32, format, D3DPOOL_DEFAULT, &mut texture, std::ptr::null_mut())
            .map_err(|e| format!("page texture: {e}"))?;
    }
    let texture = texture.ok_or("no page texture")?;
    // The font's own page on top, the rest clear (all zero bytes, in either
    // format), a band of rows at a time.
    const BAND: u32 = 256;
    let (row, rows_per_row) = (row_bytes(compressed, width), if compressed { 4 } else { 1 });
    for band in (0..height).step_by(BAND as usize) {
        let rows = BAND.min(height - band);
        let mut data = vec![0u8; (rows / rows_per_row) as usize * row];
        if let Some(own) = &own {
            let range = own_rows(own.height, band, rows, rows_per_row);
            let (from, to) = (range.start * row, range.end * row);
            data[..to - from].copy_from_slice(&own.data[from..to]);
        }
        put(&texture, compressed, 0, band, width, rows, &data).map_err(|e| format!("page pixels: {e}"))?;
    }
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
    Ok(Fill { texture, compressed, width, height, atlas: Page::new(width, height - top), top, family, ink, letter_em, em, base, drawn: HashMap::new(), full: false })
}

/// The system font look: the font's glyphs for Windows-1252 characters
/// become system glyphs, written into its own records (or records allocated
/// as the game allocates them, which it frees with the font).
fn replace_own_glyphs(m: &Memory, font: usize, fill: &mut Fill) -> crate::Result<()> {
    for byte in 0x20u8..=0xff {
        let Some(c) = encoding::to_utf8(&[byte]).chars().next() else { continue };
        if c.is_control() {
            continue;
        }
        let Some(glyph) = glyph_of(fill, c as u32) else { continue };
        let slot = font + 0x120 + byte as usize * 8;
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

extern "C" fn load_before(frame: *mut HookFrame) {
    let font = unsafe { (*frame).args[0] };
    LOADING.with(|l| *l.borrow_mut() = Some((font, None)));
}

extern "C" fn create_before(frame: *mut HookFrame) {
    crate::guarded("font page pixels", || unsafe {
        let args = &(*frame).args;
        let (description, levels, count) = (args[2] as *const u32, args[3] as *const u8, args[4] as u32);
        LOADING.with(|l| {
            let mut l = l.borrow_mut();
            let Some((_, pixels @ None)) = l.as_mut() else { return };
            // A 2D, single-level 32-bit or DXT5 page.
            if description.is_null() || *description.add(4) != 0 || count != 1 {
                return;
            }
            let compressed = match *description.add(3) {
                ARGB => false,
                f if DXT5.contains(&f) => true,
                _ => return,
            };
            let (width, height) = (*(levels.add(8) as *const u32), *(levels.add(0xc) as *const u32));
            let data = *(levels.add(0x18) as *const *const u8);
            if data.is_null() || width == 0 || height == 0 || (compressed && (width % 4 != 0 || height % 4 != 0)) {
                return;
            }
            let size = height as usize / if compressed { 4 } else { 1 } * row_bytes(compressed, width);
            *pixels = Some(Own { width, height, compressed, data: std::slice::from_raw_parts(data, size).to_vec() });
        });
    })
}

extern "C" fn load_after(frame: *mut HookFrame) {
    crate::guarded("font page fill", || {
        let font = unsafe { (*frame).args[0] };
        let Some((loading, own)) = LOADING.with(|l| l.borrow_mut().take()) else { return };
        if loading != font {
            return;
        }
        let system = settings::current().text_appearance == TextAppearance::System;
        let m = memory();
        let result = (|| -> crate::Result<()> {
            let (manager, entry) = page_entry(&m, font)?;
            let wrapper = m.ptr(entry)?;
            if wrapper == 0 || m.ptr(wrapper)? == 0 {
                return Ok(());
            }
            let mut pages = lock(&PAGES);
            if !pages.contains_key(&wrapper) {
                // A page loaded before, or in another format, keeps its own
                // glyphs only.
                if !system && own.is_none() {
                    return Ok(());
                }
                let fill = make_page(&m, font, &device(&m, manager)?, own, system)?;
                // The wrapper holds the game's reference: it gets one of
                // ours, and its old texture is released.
                let old = m.ptr(wrapper)? as *mut c_void;
                unsafe {
                    *(wrapper as *mut *mut c_void) = fill.texture.clone().into_raw();
                    drop(IDirect3DTexture9::from_raw(old));
                    *((entry + 0x28) as *mut u32) = fill.width;
                    *((entry + 0x2c) as *mut u32) = fill.height;
                }
                let look = match (system, fill.compressed) {
                    (true, _) => "system",
                    (false, true) => "grown DXT5",
                    (false, false) => "grown",
                };
                log!("fill: {look} page {}x{} for {:?} at {:.1} px (letters {:.1} px, baseline {:.0}), ink {:06x}", fill.width, fill.height, fill.family, fill.em, fill.letter_em, fill.base, fill.ink);
                pages.insert(wrapper, fill);
            }
            let fill = pages.get_mut(&wrapper).expect("just made");
            unsafe {
                *((font + 0x978) as *mut u32) = fill.width;
                *((font + 0x97c) as *mut u32) = fill.height;
            }
            lock(&OWNERS).insert(font, wrapper);
            if system {
                glyphs::forget(font + 0x120);
                replace_own_glyphs(&m, font, fill)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            log!("fill: font page left as it is: {e}");
        }
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
        Point::observe(LOAD_PAGE, "48 89 5c 24 08 57 48 81 ec 80 00 00 00 48 8b f9", "font page load", 1, Some(load_before), Some(load_after)),
        // Where the page's index and size come from: the manager at
        // context + 0x480, the entry's size at + 0x28.
        Point::keep(0x15954a4usize, "48 8b 88 80 04 00 00", "font page manager"),
        Point::keep(0x15954b0usize, "89 87 70 09 00 00", "font page index"),
        Point::keep(0x15954e8usize, "48 8d 1c c0 48 8b 5c da 28 48 89 9f 78 09 00 00", "font page size"),
        // The renderer the manager creates textures with.
        Point::keep(0x16c2e84usize, "49 8b 4f 60", "texture manager renderer"),
        Point::observe(CREATE_TEXTURE, "48 89 5c 24 20 55 56 57 41 54 41 55 41 56 41 57", "texture creation", 5, Some(create_before), None),
        Point::observe(DESTROY_TEXTURE, "40 53 48 83 ec 20 48 8b d9 ff 15 51 91 8f 00 48", "texture destruction", 1, Some(destroy_before), None),
        Point::keep(ALLOCATE, "40 53 48 83 ec 20 48 8b d9 eb 0f 48 8b cb e8 95", "game allocator"),
        // The `.fnt` loader allocates its 16-byte glyph records with it.
        Point::keep(0x1595cadusize, "b9 10 00 00 00 e8 1d d6 49 00", "glyph record allocation"),
    ]
}

#[cfg(test)]
mod tests {
    use super::own_rows;

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
