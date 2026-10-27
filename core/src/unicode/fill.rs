//! Glyphs from system fonts in the game's font pages (ADR 0010): characters a
//! bitmap font lacks and, with the system font look, every character. A font
//! draws from one page texture, which the game reads from the page's wrapper
//! at every draw, so the framework gives a page a texture of its own with room
//! for system glyphs. Glyph positions and the page's size are baked into the
//! game's caches, so the page is sized once, when it loads, and a glyph is
//! drawn into it before its first use (docs/research/font-textures.md).
//!
//! - Bitmap fonts first (沿用原字体): a 32-bit page gets 1024 more rows below
//!   it, where a character the font lacks is drawn the first time it is
//!   needed. A compressed page (the Chinese language mod's) is left as it is;
//!   such fonts carry Chinese themselves.
//! - The system font look (系统字体): a new page holds every character, drawn
//!   from the system font; the font's own glyphs are replaced.
//!
//! Fonts with lines taller than 48 pixels are the map's and draw in the map
//! font; the rest in the body font. Pages are dynamic textures, which only
//! Direct3D 9Ex keeps across a device reset; on a plain Direct3D 9 device
//! nothing is filled.
use super::atlas::Page;
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
/// The engine's format number for a 32-bit page (A8R8G8B8).
const ARGB: u32 = 0xb;
const EXTRA_ROWS: u32 = 1024;
const MAP_LINE: i32 = 48;

struct Fill {
    texture: IDirect3DTexture9,
    width: u32,
    height: u32,
    atlas: Page,
    /// Rows above the system glyphs: the font's own page, or none.
    top: u32,
    family: String,
    /// The colour the glyphs are drawn in.
    ink: u32,
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
    /// The font whose page is loading on this thread, and the page's pixels
    /// (width, height, A8R8G8B8 rows) once the game has decoded them.
    static LOADING: RefCell<Option<(usize, Option<(u32, u32, Vec<u8>)>)>> = const { RefCell::new(None) };
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

/// Writes A8R8G8B8 rows (`width` pixels each) into the page at (x, y).
fn put(texture: &IDirect3DTexture9, x: u32, y: u32, width: u32, rows: &[u32]) -> windows::core::Result<()> {
    if width == 0 || rows.is_empty() {
        return Ok(());
    }
    let height = rows.len() as u32 / width;
    let rect = RECT { left: x as i32, top: y as i32, right: (x + width) as i32, bottom: (y + height) as i32 };
    let mut locked = D3DLOCKED_RECT::default();
    unsafe {
        texture.LockRect(0, &mut locked, &rect, 0)?;
        for row in 0..height as usize {
            let line = (locked.pBits as *mut u8).add(row * locked.Pitch as usize) as *mut u32;
            std::ptr::copy_nonoverlapping(rows.as_ptr().add(row * width as usize), line, width as usize);
        }
        texture.UnlockRect(0)
    }
}

/// `ink`, with the coverage as alpha.
fn inked(alpha: &[u8], ink: u32) -> Vec<u32> {
    alpha.iter().map(|&a| (a as u32) << 24 | ink).collect()
}

/// The colour `font`'s own glyphs have: that of the most opaque pixel of its
/// `H`. Body text is drawn tinted, so its pages are white; the map draws its
/// names in the page's colour, black in the fonts the game ships. Without the
/// page's pixels: black for a map font, white otherwise.
fn ink(m: &Memory, font: usize, own: Option<&(u32, u32, Vec<u8>)>, map: bool) -> u32 {
    let fallback = if map { 0 } else { 0x00ff_ffff };
    let Some((width, height, pixels)) = own else { return fallback };
    let Ok(record) = m.ptr(font + 0x120 + b'H' as usize * 8) else { return fallback };
    if record == 0 {
        return fallback;
    }
    let h = unsafe { *(record as *const Glyph) };
    let mut best = (0u8, fallback);
    for y in h.y as u32..(h.y as u32 + h.height as u32).min(*height) {
        for x in h.x as u32..(h.x as u32 + h.width as u32).min(*width) {
            let at = ((y * width + x) * 4) as usize;
            let [b, g, r, a] = [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]];
            if a > best.0 {
                best = (a, u32::from_le_bytes([b, g, r, 0]));
            }
        }
    }
    best.1
}

fn draw(fill: &mut Fill, scalar: u32) -> Option<Glyph> {
    let fonts = SYSTEM.as_ref()?;
    let family = fonts.family_for(&fill.family, scalar)?.to_string();
    let bitmap = fonts.rasterize(&family, scalar, fill.em).ok()?;
    let (mut x, mut y) = (0, 0);
    if bitmap.width > 0 && bitmap.height > 0 {
        (x, y) = match fill.atlas.place(bitmap.width, bitmap.height) {
            Some(spot) => spot,
            None => {
                if !std::mem::replace(&mut fill.full, true) {
                    log!("fill: a {}x{} page is full; characters past it show as '?'", fill.width, fill.height);
                }
                return None;
            }
        };
        if let Err(e) = put(&fill.texture, x, fill.top + y, bitmap.width, &inked(&bitmap.alpha, fill.ink)) {
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

/// A page for `font`: its own pixels (`own`, when the game decoded them) with
/// room below (bitmap fonts first), or an empty one (the system font look).
fn make_page(m: &Memory, font: usize, device: &IDirect3DDevice9, own: Option<(u32, u32, Vec<u8>)>, system: bool) -> crate::Result<Fill> {
    let fonts = SYSTEM.as_ref().ok_or("no system fonts")?;
    let line = m.i32(font + 0x960)?.max(1);
    let current = settings::current();
    let family = if line > MAP_LINE { current.map_font } else { current.body_font };
    let ink = ink(m, font, own.as_ref(), line > MAP_LINE);
    let own = if system { None } else { own };
    let (em, mut base) = metrics(fonts, &family, line as f32);
    let (width, height, top) = match &own {
        Some((w, h, _)) if h + EXTRA_ROWS <= 4096 => (*w, h + EXTRA_ROWS, *h),
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
    let mut texture = None;
    unsafe {
        device
            .CreateTexture(width, height, 1, D3DUSAGE_DYNAMIC as u32, D3DFMT_A8R8G8B8, D3DPOOL_DEFAULT, &mut texture, std::ptr::null_mut())
            .map_err(|e| format!("page texture: {e}"))?;
    }
    let texture = texture.ok_or("no page texture")?;
    // The font's own pixels on top, the rest clear, a band at a time.
    const BAND: u32 = 256;
    for band in (0..height).step_by(BAND as usize) {
        let rows = BAND.min(height - band);
        let mut pixels = vec![0u32; (width * rows) as usize];
        if let Some((w, h, page)) = &own {
            for y in band..(band + rows).min(*h) {
                let from = &page[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
                for (x, px) in from.chunks_exact(4).enumerate() {
                    pixels[((y - band) * width) as usize + x] = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
                }
            }
        }
        put(&texture, 0, band, width, &pixels).map_err(|e| format!("page pixels: {e}"))?;
    }
    if own.is_some() {
        // Chinese drawn among the font's own letters sits on their baseline.
        let h_glyph = m.ptr(font + 0x120 + b'H' as usize * 8)?;
        if h_glyph != 0 {
            let glyph = unsafe { *(h_glyph as *const Glyph) };
            base = (glyph.y_offset + glyph.height as i16) as f32;
        }
    }
    Ok(Fill { texture, width, height, atlas: Page::new(width, height - top), top, family, ink, em, base, drawn: HashMap::new(), full: false })
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
            // A 2D, single-level 32-bit page.
            if description.is_null() || *description.add(3) != ARGB || *description.add(4) != 0 || count != 1 {
                return;
            }
            let (width, height) = (*(levels.add(8) as *const u32), *(levels.add(0xc) as *const u32));
            let data = *(levels.add(0x18) as *const *const u8);
            if data.is_null() || width == 0 || height == 0 {
                return;
            }
            *pixels = Some((width, height, std::slice::from_raw_parts(data, (width * height * 4) as usize).to_vec()));
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
                // A compressed or shared page keeps its own glyphs only.
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
                log!("fill: {} page {}x{} for {:?} at {:.1} px", if system { "system" } else { "grown" }, fill.width, fill.height, fill.family, fill.em);
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
