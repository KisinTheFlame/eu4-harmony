//! Draws the framework UI's commands with the game's Direct3D 9 device, at the
//! end of a frame: untextured quads for fills, quads from the text page for
//! text. Everything the game had set on the device is put back afterwards.
use super::text::{UiFont, PAGE};
use super::widgets::{Color, Command, Rect};
use std::ffi::c_void;
use windows::core::Interface;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D9::*;

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    x: f32,
    y: f32,
    z: f32,
    rhw: f32,
    color: u32,
    u: f32,
    v: f32,
}
const FVF: u32 = D3DFVF_XYZRHW | D3DFVF_DIFFUSE | D3DFVF_TEX1;

#[derive(Default)]
pub struct Backend {
    texture: Option<IDirect3DTexture9>,
    state: Option<IDirect3DStateBlock9>,
}

// The device and its objects are used only on the thread that presents.
unsafe impl Send for Backend {}

/// A rectangle in GUI units, in pixels.
fn pixels(r: Rect, scale: f32) -> Rect {
    let (x0, y0) = ((r.x * scale).round(), (r.y * scale).round());
    Rect { x: x0, y: y0, w: ((r.x + r.w) * scale).round() - x0, h: ((r.y + r.h) * scale).round() - y0 }
}

/// Clamps a GUI clip rectangle to the active Direct3D viewport bounds. Returns
/// `None` when the clamped scissor rectangle is empty or inverted, so callers
/// skip drawing instead of passing an invalid `RECT` to `SetScissorRect`.
fn clamp_scissor_rect(rect: Rect, scale: f32, bounds: RECT) -> Option<RECT> {
    let p = pixels(rect, scale);
    let left = (p.x.round() as i32).clamp(bounds.left, bounds.right);
    let top = (p.y.round() as i32).clamp(bounds.top, bounds.bottom);
    let right = ((p.x + p.w).round() as i32).clamp(bounds.left, bounds.right);
    let bottom = ((p.y + p.h).round() as i32).clamp(bounds.top, bounds.bottom);
    (left < right && top < bottom).then_some(RECT { left, top, right, bottom })
}

fn quad(out: &mut Vec<Vertex>, r: Rect, color: Color, uv: Option<Rect>) {
    // Direct3D 9 puts pixel centres on whole coordinates.
    let (x0, y0, x1, y1) = (r.x - 0.5, r.y - 0.5, r.x + r.w - 0.5, r.y + r.h - 0.5);
    let (u0, v0, u1, v1) = uv.map_or((0.0, 0.0, 0.0, 0.0), |t| (t.x / PAGE as f32, t.y / PAGE as f32, (t.x + t.w) / PAGE as f32, (t.y + t.h) / PAGE as f32));
    let v = |x, y, u, v| Vertex { x, y, z: 0.0, rhw: 1.0, color, u, v };
    out.extend([v(x0, y0, u0, v0), v(x1, y0, u1, v0), v(x0, y1, u0, v1), v(x1, y0, u1, v0), v(x1, y1, u1, v1), v(x0, y1, u0, v1)]);
}

const CORNER_STEPS: usize = 6;

fn rounded_ring(r: Rect, radius: f32) -> [(f32, f32); (CORNER_STEPS + 1) * 4] {
    use std::f32::consts::{FRAC_PI_2, PI};
    let (x0, y0, x1, y1) = (r.x - 0.5, r.y - 0.5, r.x + r.w - 0.5, r.y + r.h - 0.5);
    let rad = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let corners = [(x0 + rad, y0 + rad, PI), (x1 - rad, y0 + rad, PI + FRAC_PI_2), (x1 - rad, y1 - rad, 0.0), (x0 + rad, y1 - rad, FRAC_PI_2)];
    let mut ring = [(0.0, 0.0); (CORNER_STEPS + 1) * 4];
    let mut k = 0;
    for (cx, cy, start) in corners {
        for s in 0..=CORNER_STEPS {
            let a = start + FRAC_PI_2 * (s as f32 / CORNER_STEPS as f32);
            ring[k] = (cx + a.cos() * rad, cy + a.sin() * rad);
            k += 1;
        }
    }
    ring
}

fn round_quad(out: &mut Vec<Vertex>, r: Rect, color: Color, radius: f32) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    if radius <= 0.5 {
        quad(out, r, color, None);
        return;
    }
    let ring = rounded_ring(r, radius);
    let center = Vertex { x: r.x + r.w / 2.0 - 0.5, y: r.y + r.h / 2.0 - 0.5, z: 0.0, rhw: 1.0, color, u: 0.0, v: 0.0 };
    let v = |(x, y): (f32, f32)| Vertex { x, y, z: 0.0, rhw: 1.0, color, u: 0.0, v: 0.0 };
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        out.extend([center, v(a), v(b)]);
    }
}

fn round_stroke(out: &mut Vec<Vertex>, r: Rect, color: Color, radius: f32, thickness: f32) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let t = thickness.max(1.0).min(r.w / 2.0).min(r.h / 2.0);
    if radius <= 0.5 {
        quad(out, Rect { x: r.x, y: r.y, w: r.w, h: t }, color, None);
        quad(out, Rect { x: r.x, y: r.y + r.h - t, w: r.w, h: t }, color, None);
        if r.h > 2.0 * t {
            quad(out, Rect { x: r.x, y: r.y + t, w: t, h: r.h - 2.0 * t }, color, None);
            quad(out, Rect { x: r.x + r.w - t, y: r.y + t, w: t, h: r.h - 2.0 * t }, color, None);
        }
        return;
    }
    let inner = Rect { x: r.x + t, y: r.y + t, w: (r.w - 2.0 * t).max(0.0), h: (r.h - 2.0 * t).max(0.0) };
    let outer_ring = rounded_ring(r, radius);
    let inner_ring = rounded_ring(inner, (radius - t).max(0.0));
    let v = |(x, y): (f32, f32)| Vertex { x, y, z: 0.0, rhw: 1.0, color, u: 0.0, v: 0.0 };
    for i in 0..outer_ring.len() {
        let j = (i + 1) % outer_ring.len();
        let (o0, o1, i0, i1) = (v(outer_ring[i]), v(outer_ring[j]), v(inner_ring[i]), v(inner_ring[j]));
        out.extend([o0, o1, i0, o1, i1, i0]);
    }
}

impl Backend {
    /// Drops what lives in the device's default pool, before the game resets it.
    pub fn release(&mut self) {
        self.texture = None;
        self.state = None;
    }

    unsafe fn texture(&mut self, device: &IDirect3DDevice9, font: &UiFont) -> windows::core::Result<IDirect3DTexture9> {
        if self.texture.is_none() {
            let mut texture = None;
            device.CreateTexture(PAGE, PAGE, 1, D3DUSAGE_DYNAMIC as u32, D3DFMT_A8R8G8B8, D3DPOOL_DEFAULT, &mut texture, std::ptr::null_mut())?;
            self.texture = texture;
            font.mark_dirty();
        }
        let texture = self.texture.clone().expect("texture just made");
        if let Some(alpha) = font.take_page() {
            let mut locked = D3DLOCKED_RECT::default();
            if let Err(e) = texture.LockRect(0, &mut locked, std::ptr::null(), D3DLOCK_DISCARD as u32) {
                font.mark_dirty();
                return Err(e);
            }
            for row in 0..PAGE as usize {
                let line = (locked.pBits as *mut u8).add(row * locked.Pitch as usize) as *mut u32;
                for col in 0..PAGE as usize {
                    *line.add(col) = (alpha[row * PAGE as usize + col] as u32) << 24 | 0x00ff_ffff;
                }
            }
            if let Err(e) = texture.UnlockRect(0) {
                font.mark_dirty();
                return Err(e);
            }
        }
        Ok(texture)
    }

    unsafe fn set_states(device: &IDirect3DDevice9) -> windows::core::Result<()> {
        device.SetVertexShader(None)?;
        device.SetPixelShader(None)?;
        device.SetFVF(FVF)?;
        for (state, value) in [
            (D3DRS_ZENABLE, 0),
            (D3DRS_ZWRITEENABLE, 0),
            (D3DRS_ALPHATESTENABLE, 0),
            (D3DRS_CULLMODE, D3DCULL_NONE.0 as u32),
            (D3DRS_LIGHTING, 0),
            (D3DRS_FOGENABLE, 0),
            (D3DRS_STENCILENABLE, 0),
            (D3DRS_SCISSORTESTENABLE, 0),
            (D3DRS_ALPHABLENDENABLE, 1),
            (D3DRS_SRCBLEND, D3DBLEND_SRCALPHA.0 as u32),
            (D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA.0 as u32),
            (D3DRS_BLENDOP, D3DBLENDOP_ADD.0 as u32),
            (D3DRS_COLORWRITEENABLE, 0xf),
            (D3DRS_SRGBWRITEENABLE, 0),
        ] {
            device.SetRenderState(state, value)?;
        }
        for (state, value) in [
            (D3DTSS_COLOROP, D3DTOP_MODULATE.0 as u32),
            (D3DTSS_COLORARG1, D3DTA_TEXTURE),
            (D3DTSS_COLORARG2, D3DTA_DIFFUSE),
            (D3DTSS_ALPHAOP, D3DTOP_MODULATE.0 as u32),
            (D3DTSS_ALPHAARG1, D3DTA_TEXTURE),
            (D3DTSS_ALPHAARG2, D3DTA_DIFFUSE),
        ] {
            device.SetTextureStageState(0, state, value)?;
        }
        device.SetTextureStageState(1, D3DTSS_COLOROP, D3DTOP_DISABLE.0 as u32)?;
        for (state, value) in [
            (D3DSAMP_MINFILTER, D3DTEXF_POINT.0 as u32),
            (D3DSAMP_MAGFILTER, D3DTEXF_POINT.0 as u32),
            (D3DSAMP_MIPFILTER, D3DTEXF_NONE.0 as u32),
            (D3DSAMP_SRGBTEXTURE, 0),
        ] {
            device.SetSamplerState(0, state, value)?;
        }
        Ok(())
    }

    unsafe fn flush(device: &IDirect3DDevice9, vertices: &mut Vec<Vertex>, texture: Option<&IDirect3DTexture9>) -> windows::core::Result<()> {
        if vertices.is_empty() {
            return Ok(());
        }
        match texture {
            Some(t) => device.SetTexture(0, t)?,
            None => {
                device.SetTexture(0, None::<&IDirect3DBaseTexture9>)?;
                device.SetTextureStageState(0, D3DTSS_COLOROP, D3DTOP_SELECTARG2.0 as u32)?;
                device.SetTextureStageState(0, D3DTSS_ALPHAOP, D3DTOP_SELECTARG2.0 as u32)?;
            }
        }
        device.DrawPrimitiveUP(D3DPT_TRIANGLELIST, (vertices.len() / 3) as u32, vertices.as_ptr() as *const c_void, std::mem::size_of::<Vertex>() as u32)?;
        if texture.is_none() {
            device.SetTextureStageState(0, D3DTSS_COLOROP, D3DTOP_MODULATE.0 as u32)?;
            device.SetTextureStageState(0, D3DTSS_ALPHAOP, D3DTOP_MODULATE.0 as u32)?;
        }
        vertices.clear();
        Ok(())
    }

    /// Draws `commands` (in GUI units, `scale` pixels each) on `device` (an
    /// `IDirect3DDevice9*`) in a scene of their own, after the game's scene
    /// has ended and before it presents, leaving the device as the game had
    /// it. With no commands the device is left alone.
    ///
    /// # Safety
    /// `device` is null or the game's live device.
    pub unsafe fn draw(&mut self, device: *mut c_void, font: &UiFont, commands: &[Command], scale: f32) -> windows::core::Result<()> {
        let Some(device) = IDirect3DDevice9::from_raw_borrowed(&device) else { return Ok(()) };
        if commands.is_empty() {
            return Ok(());
        }
        device.BeginScene()?;
        let drawn = self.draw_scene(device, font, commands, scale);
        device.EndScene()?;
        drawn
    }

    unsafe fn draw_scene(&mut self, device: &IDirect3DDevice9, font: &UiFont, commands: &[Command], scale: f32) -> windows::core::Result<()> {
        // Lay out text first, so the page holds every glyph before it is uploaded.
        let texts: Vec<(Option<&crate::base::settings::Face>, &str, f32, f32, f32)> = commands
            .iter()
            .filter_map(|c| match c {
                Command::Text { x, y, text, size, face, .. } => Some((face.as_ref(), text.as_str(), *size, *x, *y)),
                Command::Fill { .. } | Command::RoundFill { .. } | Command::RoundStroke { .. } | Command::Clip(_) => None,
            })
            .collect();
        let texts = font.layout_frame(&texts);
        let texture = self.texture(device, font)?;
        if self.state.is_none() {
            self.state = Some(device.CreateStateBlock(D3DSBT_ALL)?);
        }
        let state = self.state.clone().expect("state block just made");
        state.Capture()?;
        let mut viewport = D3DVIEWPORT9::default();
        device.GetViewport(&mut viewport)?;
        let bounds = RECT {
            left: viewport.X as i32,
            top: viewport.Y as i32,
            right: (viewport.X.saturating_add(viewport.Width)).min(i32::MAX as u32) as i32,
            bottom: (viewport.Y.saturating_add(viewport.Height)).min(i32::MAX as u32) as i32,
        };
        let outline_step = scale.max(1.0).round();
        let drawn = (|| {
            Self::set_states(device)?;
            let mut vertices = Vec::new();
            let mut textured = false;
            let mut clipped_out = false;
            let mut next_text = texts.iter();
            for c in commands {
                match c {
                    Command::Clip(clip) => {
                        Self::flush(device, &mut vertices, textured.then_some(&texture))?;
                        if let Some(rect) = clip {
                            if let Some(scissor) = clamp_scissor_rect(*rect, scale, bounds) {
                                clipped_out = false;
                                device.SetScissorRect(&scissor)?;
                                device.SetRenderState(D3DRS_SCISSORTESTENABLE, 1)?;
                            } else {
                                clipped_out = true;
                                device.SetRenderState(D3DRS_SCISSORTESTENABLE, 0)?;
                            }
                        } else {
                            clipped_out = false;
                            device.SetRenderState(D3DRS_SCISSORTESTENABLE, 0)?;
                        }
                    }
                    Command::Fill { rect, color } => {
                        if clipped_out {
                            continue;
                        }
                        if textured {
                            Self::flush(device, &mut vertices, Some(&texture))?;
                            textured = false;
                        }
                        quad(&mut vertices, pixels(*rect, scale), *color, None);
                    }
                    Command::RoundFill { rect, color, radius } => {
                        if clipped_out {
                            continue;
                        }
                        if textured {
                            Self::flush(device, &mut vertices, Some(&texture))?;
                            textured = false;
                        }
                        round_quad(&mut vertices, pixels(*rect, scale), *color, *radius * scale);
                    }
                    Command::RoundStroke { rect, color, radius, thickness } => {
                        if clipped_out {
                            continue;
                        }
                        if textured {
                            Self::flush(device, &mut vertices, Some(&texture))?;
                            textured = false;
                        }
                        round_stroke(&mut vertices, pixels(*rect, scale), *color, *radius * scale, (*thickness * scale).max(1.0));
                    }
                    Command::Text { color, outline, .. } => {
                        let quads = next_text.next().expect("one layout per text");
                        if clipped_out {
                            continue;
                        }
                        if !textured {
                            Self::flush(device, &mut vertices, None)?;
                            textured = true;
                        }
                        if *outline {
                            const OFFSETS: [(f32, f32); 8] =
                                [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)];
                            for q in quads {
                                for (dx, dy) in OFFSETS {
                                    let shifted = Rect { x: q.screen.x + dx * outline_step, y: q.screen.y + dy * outline_step, ..q.screen };
                                    quad(&mut vertices, shifted, 0xd805080d, Some(q.page));
                                }
                            }
                        }
                        for q in quads {
                            quad(&mut vertices, q.screen, *color, Some(q.page));
                        }
                    }
                }
            }
            Self::flush(device, &mut vertices, textured.then_some(&texture))
        })();
        state.Apply()?;
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_scissor_rect_bounds_and_rejects_empty_or_offscreen_clips() {
        let bounds = RECT { left: 0, top: 0, right: 1280, bottom: 720 };
        // Partially off-screen right/bottom (e.g., 720p at 2x GUI scale) clamps safely.
        assert_eq!(
            clamp_scissor_rect(Rect { x: -10.0, y: 100.0, w: 800.0, h: 500.0 }, 2.0, bounds),
            Some(RECT { left: 0, top: 200, right: 1280, bottom: 720 })
        );
        // Entirely below viewport returns None instead of an inverted/empty RECT.
        assert_eq!(clamp_scissor_rect(Rect { x: 20.0, y: 800.0, w: 400.0, h: 200.0 }, 1.0, bounds), None);
        // Zero or negative dimensions return None.
        assert_eq!(clamp_scissor_rect(Rect { x: 10.0, y: 10.0, w: 0.0, h: 50.0 }, 1.0, bounds), None);
        assert_eq!(clamp_scissor_rect(Rect { x: 10.0, y: 10.0, w: -5.0, h: 50.0 }, 1.0, bounds), None);
    }
}
