//! Text on whole pixels. The game places text in GUI units, which the
//! interface scale stretches to pixels, so at a scale above 1 a glyph's
//! corners fall between pixels, and the text effect's linear filter spreads
//! each texel over two. Its own correction, an origin of (-0.5, 0.5) GUI
//! units, is Direct3D 9's half pixel only at a scale of 1
//! (docs/research/font-textures.md). A glyph drawn a texel to a pixel, as
//! `fill` draws system glyphs, is moved here so that its edges fall on pixel
//! edges and it is whole texels wide and tall:
//!
//! - in main text (0x15988e0), which draws straight to the screen;
//! - in button and tooltip text (0x1596660), which draws into a texture of
//!   its own and snaps each glyph to whole GUI units, cutting a glyph down or
//!   stretching it by up to one: the exact corners are kept instead, and
//!   snapped to the texture's pixels.
//!
//! Where the pixels are comes from the device: the projection in the first
//! four vertex shader constants and the viewport, as the text effect uses
//! them.
//!
//! The texture button and tooltip text is drawn into is then shown at the
//! element's place, in whole GUI units and with the same half pixel, and over
//! the text's size in GUI units, which the texture rounds up to whole pixels
//! (0x1551840): a texture made a texel to a pixel, and not a power of two, is
//! moved to pixel edges and shown at its own size.
use super::buttons;
use super::fill;
use super::glyphs::Glyph;
use crate::called_by_game;
use crate::objects::text::text_texture;
use crate::points::Point;
use crate::resume;
use std::cell::Cell;
use std::ffi::c_void;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D9::{IDirect3DDevice9, IDirect3DTexture9, D3DSURFACE_DESC, D3DVIEWPORT9};

/// Main text, once a call, where the origin is set: `mov rax, -1`.
const MAIN_TEXT_BEGIN: usize = 0x159a1ea;
const MAIN_TEXT_BEGUN: usize = 0x159a1f1;
/// Main text, once a glyph's corners are known: `movsx eax, word [rdx];
/// movd xmm0, [r14+0x978]`.
const MAIN_TEXT_QUAD: usize = 0x159a954;
const MAIN_TEXT_QUAD_DONE: usize = 0x159a960;
/// Button text, where the glyph's bottom is cut to whole GUI units:
/// `cvttss2si eax, xmm1; movd xmm2, eax`.
const BUTTON_QUAD: usize = 0x159858c;
const BUTTON_QUAD_DONE: usize = 0x1598594;
/// A text texture shown, before its matrix goes to the constants: `mov r9d,
/// -1; lea r8, [rbp-0x40]`.
const TEXTURE_SHOWN: usize = 0x1551a73;
const TEXTURE_SHOWN_DONE: usize = 0x1551a7d;

resume!(BEGIN_RESUME: BEGUN = MAIN_TEXT_BEGUN);
resume!(MAIN_QUAD_RESUME: MAIN_QUAD_DONE = MAIN_TEXT_QUAD_DONE);
resume!(BUTTON_QUAD_RESUME: BUTTON_DONE = BUTTON_QUAD_DONE);
resume!(TEXTURE_RESUME: TEXTURE_DONE = TEXTURE_SHOWN_DONE);

/// How GUI units become pixels: x in pixels is `ax * x + bx`, y is
/// `ay * y + by`; GUI y grows upward, so `ay` is negative.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Transform {
    ax: f32,
    bx: f32,
    ay: f32,
    by: f32,
}

impl Transform {
    /// From a projection, the constants `c0` to `c3` as 16 floats, and the
    /// viewport: None for anything but a plain 2D projection.
    fn new(m: &[f32; 16], viewport: &D3DVIEWPORT9) -> Option<Self> {
        let plain = m[1] == 0.0 && m[4] == 0.0 && m[8] == 0.0 && m[9] == 0.0 && m[3] == 0.0 && m[7] == 0.0 && m[11] == 0.0 && m[15] == 1.0;
        let (w, h) = (viewport.Width as f32, viewport.Height as f32);
        let t = Transform {
            ax: m[0] * w / 2.0,
            bx: (m[12] + 1.0) * w / 2.0 + viewport.X as f32,
            ay: -m[5] * h / 2.0,
            by: (1.0 - m[13]) * h / 2.0 + viewport.Y as f32,
        };
        (plain && t.ax > 0.0 && t.ay < 0.0).then_some(t)
    }

    fn of(device: &IDirect3DDevice9) -> Option<Self> {
        let (mut m, mut viewport) = ([0f32; 16], D3DVIEWPORT9::default());
        unsafe {
            device.GetVertexShaderConstantF(0, m.as_mut_ptr(), 4).ok()?;
            device.GetViewport(&mut viewport).ok()?;
        }
        Self::new(&m, &viewport)
    }

    /// Puts `self` into `m`, a plain 2D projection with `viewport`.
    fn write(self, m: &mut [f32; 16], viewport: &D3DVIEWPORT9) {
        let (w, h) = (viewport.Width as f32, viewport.Height as f32);
        m[0] = self.ax * 2.0 / w;
        m[12] = (self.bx - viewport.X as f32) * 2.0 / w - 1.0;
        m[5] = -self.ay * 2.0 / h;
        m[13] = 1.0 - (self.by - viewport.Y as f32) * 2.0 / h;
    }
}

/// A glyph's corners in GUI units, before the origin is added.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Quad {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
}

/// `exact`, `width` by `height` texels, moved so that with `origin` added
/// its edges fall on pixel edges and it is as many pixels wide and tall:
/// None when `t` does not draw it a texel to a pixel. In Direct3D 9 pixel
/// `k` is centred on `k`, so its edges are at `k - 0.5` and `k + 0.5`.
///
/// A glyph goes to the pixel its own density puts it at, as many texels a
/// GUI unit as it has: a text texture rounds its size up to whole pixels,
/// so its projection can take a GUI unit as a little more, and glyphs
/// placed by it would drift apart along a line.
fn snap(t: Transform, origin: (f32, f32), exact: Quad, width: u16, height: u16) -> Option<Quad> {
    let (width, height) = (width as f32, height as f32);
    let (across, down) = (exact.right - exact.left, exact.top - exact.bottom);
    let near = |pixels: f32, texels: f32| (pixels - texels).abs() <= 0.5 + texels * 0.015;
    if width < 1.0 || height < 1.0 || !near(t.ax * across, width) || !near(-t.ay * down, height) {
        return None;
    }
    let (ox, oy) = origin;
    let left = ((width / across) * (exact.left + ox) + t.bx + 0.5).round();
    let top = (-(height / down) * (exact.top + oy) + t.by + 0.5).round();
    let x = |edge: f32| (edge - 0.5 - t.bx) / t.ax - ox;
    let y = |edge: f32| (edge - 0.5 - t.by) / t.ay - oy;
    Some(Quad { left: x(left), right: x(left + width), bottom: y(top + height), top: y(top) })
}

/// A text texture of `texels` that `t` shows over a mesh of `size` GUI units,
/// from `corner` (the texture's bottom left) to `extent` from it: `t`
/// changed so that the texture's edges fall on pixel edges and it is shown
/// a texel to a pixel. None when the mesh is cut or turned (`extent` is not
/// `size`), or when `t` does not show the texture about a texel to a pixel:
/// the texture is the size in pixels rounded up.
fn show(t: Transform, corner: (f32, f32), extent: (f32, f32), size: [i32; 2], texels: [u32; 2]) -> Option<Transform> {
    let (width, height) = (size[0] as f32, size[1] as f32);
    let (across, down) = (texels[0] as f32, texels[1] as f32);
    let rounded_up = |pixels: f32, texels: f32| texels >= pixels - 0.01 && texels < pixels + 1.01;
    if width < 1.0 || height < 1.0 || extent != (width, height) || !rounded_up(t.ax * width, across) || !rounded_up(-t.ay * height, down) {
        return None;
    }
    let (ax, ay) = (across / width, -down / height);
    let left = (t.ax * corner.0 + t.bx + 0.5).round() - 0.5;
    let bottom = (t.ay * corner.1 + t.by + 0.5).round() - 0.5;
    Some(Transform { ax, bx: left - ax * corner.0, ay, by: bottom - ay * corner.1 })
}

fn record(glyph: usize) -> Glyph {
    unsafe { std::ptr::read_unaligned(glyph as *const Glyph) }
}

thread_local! {
    /// Main text's pixels for the call drawing on this thread, None when
    /// its font is not filled.
    static MAIN: Cell<Option<Transform>> = const { Cell::new(None) };
}

/// Main text with `font` begins.
#[called_by_game]
extern "C" fn main_text_begins(font: usize) {
    MAIN.set(fill::device_of(font).and_then(|device| Transform::of(&device)))
}

/// Main text's glyph `glyph`: its corners, `left` and `bottom` and the
/// `right` and `top` after them, snapped in place; the origin at `origin`.
#[called_by_game]
unsafe extern "C" fn main_text_quad(glyph: usize, left: *mut f32, bottom: *mut f32, right_top: *mut [f32; 2], origin: *const [f32; 3]) {
    let Some(t) = MAIN.get() else { return };
    let g = record(glyph);
    let (o, [right, top]) = unsafe { (*origin, *right_top) };
    let exact = Quad { left: unsafe { *left }, right, bottom: unsafe { *bottom }, top };
    if let Some(q) = snap(t, (o[0], o[1]), exact, g.width, g.height) {
        unsafe {
            (*left, *bottom, *right_top) = (q.left, q.bottom, [q.right, q.top]);
        }
    }
}

/// Replaces `movsx eax, word [rdx]; movd xmm0, [r14+0x978]`, with the glyph
/// in `rdx`, its left in `xmm4` (and the game's `[rsp+0x50]`), its right in
/// `xmm14`, its bottom in `xmm2` (and `[rsp+0x70]`) and its top in `xmm15`;
/// the origin at the game's `[rbp-0x20]`.
#[unsafe(naked)]
extern "C" fn main_text_quad_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdx",
            "lea rdx, [rsp + 0xd0]",
            "lea r8, [rsp + 0xb0]",
            "movss dword ptr [rsp + 0x40], xmm14",
            "movss dword ptr [rsp + 0x44], xmm15",
            "lea r9, [rsp + 0x40]",
            "mov rax, [rbp]",
            "sub rax, 0x20",
            "mov [rsp + 0x20], rax",
            call snapped,
            "movss xmm14, dword ptr [rsp + 0x40]",
            "movss xmm15, dword ptr [rsp + 0x44]",
        ),
        "movss dword ptr [rsp + 0x50], xmm4",
        "movss dword ptr [rsp + 0x70], xmm2",
        "movsx eax, word ptr [rdx]",
        "movd xmm0, dword ptr [r14 + {page_width}]",
        "jmp qword ptr [rip + {done}]",
        snapped = sym main_text_quad,
        page_width = const crate::objects::text::font::PAGE_SIZE,
        done = sym MAIN_QUAD_DONE,
    );
}

/// Replaces `mov rax, -1` once main text has set its origin; the font is in
/// `r14`.
#[unsafe(naked)]
extern "C" fn main_text_begin_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rcx, r14", call begins),
        "mov rax, -1",
        "jmp qword ptr [rip + {begun}]",
        begins = sym main_text_begins,
        begun = sym BEGUN,
    );
}

/// Button text's glyph `glyph` of `font`, at pen `pen` with the font's
/// `scale`, its bottom `bottom` exact, its origin (`ox`, `oy`): the corners
/// its vertices take, as floats, where the game cut them to whole GUI units.
/// `ints` holds the game's left and right (`r10d`, `r11d`) and `top` its top
/// (the game's `[rbp+0x2c]`); all three are replaced by their floats' bits,
/// and the bottom's bits are returned.
#[called_by_game]
unsafe extern "C" fn button_quad(font: usize, glyph: usize, ints: *mut [u64; 2], top: *mut i32, bottom: u32, pen: u32, scale: u32, ox: u32, oy: u32) -> u32 {
    let (bottom, pen, scale) = (f32::from_bits(bottom), f32::from_bits(pen), f32::from_bits(scale));
    let g = record(glyph);
    let left = pen + g.x_offset as f32 * scale;
    let exact = Quad { left, right: left + g.width as f32 * scale, bottom, top: bottom + g.height as f32 * scale };
    let snapped = fill::device_of(font)
        .and_then(|device| Transform::of(&device))
        .and_then(|t| snap(t, (f32::from_bits(ox), f32::from_bits(oy)), exact, g.width, g.height));
    // Without pixels to snap to, the game's own whole units.
    let q = snapped.unwrap_or(Quad { left: exact.left.trunc(), right: exact.right.trunc(), bottom: exact.bottom.trunc(), top: exact.top.trunc() });
    unsafe {
        *ints = [q.left.to_bits() as u64, q.right.to_bits() as u64];
        *top = q.top.to_bits() as i32;
    }
    q.bottom.to_bits()
}

/// Replaces `cvttss2si eax, xmm1; movd xmm2, eax`: the glyph's exact bottom
/// is in `xmm1`, its left and right cut to whole GUI units in `r10d` and
/// `r11d`, its top in the game's `[rbp+0x2c]`; the glyph in `r8`, the font in
/// `rdi`, the pen in `xmm10`, the scale in `xmm11`, the origin in `xmm14` and
/// `xmm15`. The conversions of the four back to floats are written out
/// (see [`points`]), so floats' bits go where the ints were.
#[unsafe(naked)]
extern "C" fn button_quad_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdi",
            "mov rdx, r8",
            "lea r8, [rsp + 0x78]",
            "mov r9, [rbp]",
            "add r9, 0x2c",
            "movd dword ptr [rsp + 0x20], xmm1",
            "movd dword ptr [rsp + 0x28], xmm10",
            "movd dword ptr [rsp + 0x30], xmm11",
            "movd dword ptr [rsp + 0x38], xmm14",
            "movd dword ptr [rsp + 0x40], xmm15",
            call snapped,
            "mov eax, dword ptr [rsp + 0x48]",
        ),
        "movd xmm2, eax",
        "jmp qword ptr [rip + {done}]",
        snapped = sym button_quad,
        done = sym BUTTON_DONE,
    );
}

/// The text texture of `element` is shown with `matrix`, which goes to the
/// vertex shader's constants next.
#[called_by_game]
unsafe extern "C" fn texture_shown(element: usize, matrix: *mut [f32; 16]) {
    unsafe {
        let wrapper = *((element + text_texture::TEXTURE) as *const usize);
        // A texture a power of two across shows only the part of it the text
        // takes.
        if wrapper == 0 || buttons::powers_of_two() {
            return;
        }
        let raw = *(wrapper as *const *mut c_void);
        let Some(texture) = IDirect3DTexture9::from_raw_borrowed(&raw) else { return };
        let (mut desc, mut viewport) = (D3DSURFACE_DESC::default(), D3DVIEWPORT9::default());
        if texture.GetLevelDesc(0, &mut desc).is_err() {
            return;
        }
        let Ok(device) = texture.GetDevice() else { return };
        if device.GetViewport(&mut viewport).is_err() {
            return;
        }
        let m = &mut *matrix;
        let Some(t) = Transform::new(m, &viewport) else { return };
        let size = *((element + text_texture::SIZE) as *const [i32; 2]);
        let [cx, cy] = *((element + text_texture::CORNER) as *const [f32; 2]);
        let [ex, ey] = *((element + text_texture::EXTENT) as *const [f32; 2]);
        if let Some(shown) = show(t, (cx, cy), (ex, ey), size, [desc.Width, desc.Height]) {
            shown.write(m, &viewport);
        }
    }
}

/// Replaces `mov r9d, -1; lea r8, [rbp-0x40]` as a text texture is shown:
/// the element is in `rbx`, the matrix at the game's `[rbp-0x40]`.
#[unsafe(naked)]
extern "C" fn texture_shown_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rcx, rbx", "mov rdx, [rbp]", "sub rdx, 0x40", call shown),
        "mov r9d, -1",
        "lea r8, [rbp - 0x40]",
        "jmp qword ptr [rip + {done}]",
        shown = sym texture_shown,
        done = sym TEXTURE_DONE,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(MAIN_TEXT_BEGIN, "48 c7 c0 ff ff ff ff", "text drawing begins", main_text_begin_stub as *const () as usize, &BEGIN_RESUME),
        Point::jump(
            MAIN_TEXT_QUAD,
            "0f bf 02 66 41 0f 6e 86 78 09 00 00",
            "text drawing glyph corners",
            main_text_quad_stub as *const () as usize,
            &MAIN_QUAD_RESUME,
        )
        .uses(&[crate::objects::text::font::PAGE_SIZE]),
        Point::jump(BUTTON_QUAD, "f3 0f 2c c1 66 0f 6e d0", "button text glyph corners", button_quad_stub as *const () as usize, &BUTTON_QUAD_RESUME),
        // The four corners go into the vertices as floats already.
        Point::write(0x1598594usize, "0f 5b d2", "button text bottom kept", "0f 1f 00"),
        Point::write(0x159859cusize, "0f 5b c0", "button text left kept", "0f 1f 00"),
        Point::write(0x1598601usize, "0f 5b ff", "button text top kept", "0f 1f 00"),
        Point::write(0x1598643usize, "0f 5b c0", "button text right kept", "0f 1f 00"),
        Point::jump(TEXTURE_SHOWN, "41 b9 ff ff ff ff 4c 8d 45 c0", "text texture matrix", texture_shown_stub as *const () as usize, &TEXTURE_RESUME),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(width: u32, height: u32) -> D3DVIEWPORT9 {
        D3DVIEWPORT9 { X: 0, Y: 0, Width: width, Height: height, MinZ: 0.0, MaxZ: 1.0 }
    }

    /// The interface's projection at 3840x2160 and a scale of `s`, moved by
    /// `(tx, ty)`, as the game sets it.
    fn screen(s: f32, tx: f32, ty: f32) -> Transform {
        let (w, h) = (3840.0 / s, 2160.0 / s);
        let mut m = [0f32; 16];
        (m[0], m[5], m[10], m[12], m[13], m[15]) = (2.0 / w, 2.0 / h, 1.0, tx, ty, 1.0);
        Transform::new(&m, &viewport(3840, 2160)).unwrap()
    }

    fn on_pixel_edges(t: Transform, origin: (f32, f32), q: Quad) {
        let px = |v: f32| t.ax * (v + origin.0) + t.bx + 0.5;
        let py = |v: f32| t.ay * (v + origin.1) + t.by + 0.5;
        for e in [px(q.left), px(q.right), py(q.top), py(q.bottom)] {
            assert!((e - e.round()).abs() < 1e-3, "{e} is no pixel edge in {q:?}");
        }
    }

    #[test]
    fn glyphs_drawn_a_texel_to_a_pixel_land_on_pixel_edges_whole_texels_wide() {
        for (s, tx) in [(1.5f32, 0.56875f32), (1.6, -0.22416669), (1.0, 0.3)] {
            let t = screen(s, tx, -0.1377778);
            // A 22 by 21 glyph at a pen between pixels, as main text has it.
            let exact = Quad { left: 103.3, right: 103.3 + 22.0 / s, bottom: 40.1, top: 40.1 + 21.0 / s };
            let q = snap(t, (-0.5, 0.5), exact, 22, 21).unwrap();
            on_pixel_edges(t, (-0.5, 0.5), q);
            assert!((t.ax * (q.right - q.left) - 22.0).abs() < 1e-3);
            assert!((-t.ay * (q.top - q.bottom) - 21.0).abs() < 1e-3);
            // Moved by less than a pixel.
            assert!((t.ax * (q.left - exact.left)).abs() <= 1.0);
        }
    }

    #[test]
    fn a_text_texture_slightly_denser_than_the_glyphs_still_takes_them_whole() {
        // 149 by 31 GUI units drawn into a texture of 239 by 50 pixels, as a
        // button's text is at 1.6: 1.604 and 1.613 pixels a unit.
        let mut m = [0f32; 16];
        (m[0], m[5], m[10], m[12], m[13], m[15]) = (2.0 / 149.0, 2.0 / 31.0, 1.0, -1.0, -1.0, 1.0);
        let t = Transform::new(&m, &viewport(239, 50)).unwrap();
        let glyph = |pen: f32| Quad { left: pen, right: pen + 13.75, bottom: 9.125, top: 22.25 };
        let (a, b) = (snap(t, (-0.5, 0.5), glyph(46.5), 22, 21).unwrap(), snap(t, (-0.5, 0.5), glyph(46.5 + 13.75 * 20.0), 22, 21).unwrap());
        on_pixel_edges(t, (-0.5, 0.5), a);
        on_pixel_edges(t, (-0.5, 0.5), b);
        assert!((t.ax * (a.right - a.left) - 22.0).abs() < 1e-3);
        // Twenty glyphs on, still 22 pixels a glyph: the texture's slightly
        // larger unit does not spread them.
        assert!((t.ax * (b.left - a.left) - 440.0).abs() < 1e-2, "{}", t.ax * (b.left - a.left));
    }

    #[test]
    fn glyphs_stretched_by_the_interface_scale_are_left_alone() {
        // A bitmap font's 12-pixel glyph at a scale of 1, drawn 1.6 times
        // larger.
        let t = screen(1.6, 0.0, 0.0);
        let exact = Quad { left: 10.0, right: 22.0, bottom: 0.0, top: 14.0 };
        assert_eq!(snap(t, (-0.5, 0.5), exact, 12, 14), None);
    }

    #[test]
    fn only_a_plain_projection_has_pixels() {
        let mut m = [0f32; 16];
        (m[0], m[5], m[10], m[15]) = (0.001, 0.001, 1.0, 1.0);
        assert!(Transform::new(&m, &viewport(100, 100)).is_some());
        m[1] = 0.0005;
        assert_eq!(Transform::new(&m, &viewport(100, 100)), None);
    }

    /// A text texture's matrix at 3840x2160 and a scale of `s`, for an
    /// element at `place` in GUI units, y from the bottom.
    fn placed(s: f32, place: (f32, f32)) -> [f32; 16] {
        let (w, h) = (3840.0 / s, 2160.0 / s);
        let mut m = [0f32; 16];
        (m[0], m[5], m[10], m[12], m[13], m[15]) = (2.0 / w, 2.0 / h, 1.0, 2.0 * place.0 / w - 1.0, 2.0 * place.1 / h - 1.0, 1.0);
        m
    }

    #[test]
    fn a_text_texture_made_a_texel_to_a_pixel_is_shown_on_pixel_edges_at_its_size() {
        // The treasury tooltip at 1.5 and a button's text at 1.6, both at
        // places between pixels; the mesh starts at the half pixel.
        let v = viewport(3840, 2160);
        let corner = (-0.5, 0.5);
        for (s, size, texels, place) in [(1.5f32, [358, 292], [537, 438], (1001.0, 517.0)), (1.6, [149, 31], [239, 50], (931.0, 420.0))] {
            let mut m = placed(s, place);
            let t = Transform::new(&m, &v).unwrap();
            let shown = show(t, corner, (size[0] as f32, size[1] as f32), size, texels).unwrap();
            shown.write(&mut m, &v);
            let back = Transform::new(&m, &v).unwrap();
            for (a, b) in [(back.ax, shown.ax), (back.bx, shown.bx), (back.ay, shown.ay), (back.by, shown.by)] {
                assert!((a - b).abs() < 1e-2, "{back:?} is not {shown:?}");
            }
            let (left, bottom) = (back.ax * corner.0 + back.bx + 0.5, back.ay * corner.1 + back.by + 0.5);
            let (right, top) = (left + back.ax * size[0] as f32, bottom + back.ay * size[1] as f32);
            for e in [left, right, bottom, top] {
                assert!((e - e.round()).abs() < 1e-2, "{e} is no pixel edge");
            }
            assert!((right - left - texels[0] as f32).abs() < 1e-2 && (bottom - top - texels[1] as f32).abs() < 1e-2);
            // Moved by half a pixel at most.
            assert!((left - 0.5 - (t.ax * corner.0 + t.bx)).abs() <= 0.5);
            assert!((bottom - 0.5 - (t.ay * corner.1 + t.by)).abs() <= 0.5);
        }
    }

    #[test]
    fn a_text_texture_stretched_by_the_interface_scale_is_left_alone() {
        // A bitmap font's text, drawn a texel to a GUI unit.
        let t = Transform::new(&placed(1.5, (1001.0, 517.0)), &viewport(3840, 2160)).unwrap();
        assert_eq!(show(t, (-0.5, 0.5), (358.0, 292.0), [358, 292], [358, 292]), None);
    }

    #[test]
    fn a_cut_or_turned_mesh_is_left_alone() {
        let t = Transform::new(&placed(1.5, (1001.0, 517.0)), &viewport(3840, 2160)).unwrap();
        assert!(show(t, (-0.5, 0.5), (358.0, 292.0), [358, 292], [537, 438]).is_some());
        assert_eq!(show(t, (-0.5, 0.5), (356.0, 292.0), [358, 292], [537, 438]), None);
        assert_eq!(show(t, (-0.5, 0.5), (350.1, 300.3), [358, 292], [537, 438]), None);
    }
}
