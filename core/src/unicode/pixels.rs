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
use super::fill;
use super::glyphs::Glyph;
use crate::points::Point;
use crate::resume;
use std::cell::Cell;
use windows::Win32::Graphics::Direct3D9::{IDirect3DDevice9, D3DVIEWPORT9};

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

resume!(BEGIN_RESUME: BEGUN = MAIN_TEXT_BEGUN);
resume!(MAIN_QUAD_RESUME: MAIN_QUAD_DONE = MAIN_TEXT_QUAD_DONE);
resume!(BUTTON_QUAD_RESUME: BUTTON_DONE = BUTTON_QUAD_DONE);

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

fn record(glyph: usize) -> Glyph {
    unsafe { std::ptr::read_unaligned(glyph as *const Glyph) }
}

thread_local! {
    /// Main text's pixels for the call drawing on this thread, None when
    /// its font is not filled.
    static MAIN: Cell<Option<Transform>> = const { Cell::new(None) };
}

/// Main text with `font` begins.
extern "C" fn main_text_begins(font: usize) {
    crate::guarded("text pixels", || MAIN.set(fill::device_of(font).and_then(|device| Transform::of(&device))))
}

/// Main text's glyph `glyph`: its corners, `left` and `bottom` and the
/// `right` and `top` after them, snapped in place; the origin at `origin`.
unsafe extern "C" fn main_text_quad(glyph: usize, left: *mut f32, bottom: *mut f32, right_top: *mut [f32; 2], origin: *const [f32; 3]) {
    crate::guarded("text pixels", || {
        let Some(t) = MAIN.get() else { return };
        let g = record(glyph);
        let (o, [right, top]) = unsafe { (*origin, *right_top) };
        let exact = Quad { left: unsafe { *left }, right, bottom: unsafe { *bottom }, top };
        if let Some(q) = snap(t, (o[0], o[1]), exact, g.width, g.height) {
            unsafe {
                (*left, *bottom, *right_top) = (q.left, q.bottom, [q.right, q.top]);
            }
        }
    })
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
unsafe extern "C" fn button_quad(font: usize, glyph: usize, ints: *mut [u64; 2], top: *mut i32, bottom: u32, pen: u32, scale: u32, ox: u32, oy: u32) -> u32 {
    crate::guarded("button text pixels", || {
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
    })
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
}
