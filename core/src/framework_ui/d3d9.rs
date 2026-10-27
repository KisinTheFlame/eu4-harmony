//! Draws the framework UI's commands with the game's Direct3D 9 device, at the
//! end of a frame: untextured quads for fills, quads from the text page for
//! text. Everything the game had set on the device is put back afterwards.
use super::text::{UiFont, PAGE};
use super::widgets::{Color, Command, Rect};
use std::ffi::c_void;
use windows::core::Interface;
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

fn quad(out: &mut Vec<Vertex>, r: Rect, color: Color, uv: Option<Rect>) {
    // Direct3D 9 puts pixel centres on whole coordinates.
    let (x0, y0, x1, y1) = (r.x - 0.5, r.y - 0.5, r.x + r.w - 0.5, r.y + r.h - 0.5);
    let (u0, v0, u1, v1) = uv.map_or((0.0, 0.0, 0.0, 0.0), |t| (t.x / PAGE as f32, t.y / PAGE as f32, (t.x + t.w) / PAGE as f32, (t.y + t.h) / PAGE as f32));
    let v = |x, y, u, v| Vertex { x, y, z: 0.0, rhw: 1.0, color, u, v };
    out.extend([v(x0, y0, u0, v0), v(x1, y0, u1, v0), v(x0, y1, u0, v1), v(x1, y0, u1, v0), v(x1, y1, u1, v1), v(x0, y1, u0, v1)]);
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
            texture.LockRect(0, &mut locked, std::ptr::null(), D3DLOCK_DISCARD as u32)?;
            for row in 0..PAGE as usize {
                let line = (locked.pBits as *mut u8).add(row * locked.Pitch as usize) as *mut u32;
                for col in 0..PAGE as usize {
                    *line.add(col) = (alpha[row * PAGE as usize + col] as u32) << 24 | 0x00ff_ffff;
                }
            }
            texture.UnlockRect(0)?;
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
        for (state, value) in [(D3DSAMP_MINFILTER, D3DTEXF_POINT.0 as u32), (D3DSAMP_MAGFILTER, D3DTEXF_POINT.0 as u32), (D3DSAMP_MIPFILTER, D3DTEXF_NONE.0 as u32), (D3DSAMP_SRGBTEXTURE, 0)] {
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

    /// The device's viewport size in pixels.
    pub unsafe fn screen(device: *mut c_void) -> (f32, f32) {
        let Some(device) = IDirect3DDevice9::from_raw_borrowed(&device) else { return (0.0, 0.0) };
        let mut viewport = D3DVIEWPORT9::default();
        if device.GetViewport(&mut viewport).is_err() {
            return (0.0, 0.0);
        }
        (viewport.Width as f32, viewport.Height as f32)
    }

    /// Draws `commands` (in GUI units, `scale` pixels each) on `device` (an
    /// `IDirect3DDevice9*`) in a scene of their own, after the game's scene
    /// has ended and before it presents, leaving the device as the game had
    /// it. With no commands the device is left alone.
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
        let mut texts = Vec::new();
        for c in commands {
            if let Command::Text { x, y, text, size, color } = c {
                texts.push((font.layout(text, *size, *x, *y), *color));
            }
        }
        let texture = self.texture(device, font)?;
        if self.state.is_none() {
            self.state = Some(device.CreateStateBlock(D3DSBT_ALL)?);
        }
        let state = self.state.clone().expect("state block just made");
        state.Capture()?;
        let drawn = (|| {
            Self::set_states(device)?;
            let mut vertices = Vec::new();
            let mut textured = false;
            let mut next_text = texts.iter();
            for c in commands {
                match c {
                    Command::Fill { rect, color } => {
                        if textured {
                            Self::flush(device, &mut vertices, Some(&texture))?;
                            textured = false;
                        }
                        quad(&mut vertices, pixels(*rect, scale), *color, None);
                    }
                    Command::Text { .. } => {
                        if !textured {
                            Self::flush(device, &mut vertices, None)?;
                            textured = true;
                        }
                        let (quads, color) = next_text.next().expect("one layout per text");
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
