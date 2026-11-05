//! The framework UI's text: characters from a system font, drawn once each
//! into one coverage page, measured by their advances and wrapped where
//! Unicode line breaking allows. Widgets measure in GUI units; glyphs are
//! drawn at the size in pixels the game's GUI scale gives them, in the tone
//! of light text, which is all the framework UI draws (`unicode::tone`).
use super::widgets::{Measure, Rect};
use crate::unicode::atlas::Page;
use crate::unicode::system_font::{Face, SystemFonts};
use crate::unicode::tone::Tone;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

pub const PAGE: u32 = 1024;

#[derive(Clone, Copy)]
struct Placed {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    left: i32,
    top: i32,
    advance: f32,
}

/// One glyph to draw: where on screen, and where on the page (in pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub screen: Rect,
    pub page: Rect,
}

struct Cache {
    /// A glyph that did not fit on the page is kept with its advance alone,
    /// and draws nothing until the page starts over.
    glyphs: HashMap<(char, u32), Option<Placed>>,
    page: Page,
    /// Coverage, `PAGE * PAGE` bytes.
    pixels: Vec<u8>,
    /// Pixels changed since the page was last uploaded.
    dirty: bool,
    /// A glyph did not fit since this was last taken.
    overflowed: bool,
    /// The log said once that a frame needed more glyphs than the page holds.
    told: bool,
}

pub struct UiFont {
    fonts: SystemFonts,
    face: Face,
    cache: RefCell<Cache>,
    /// Pixels per GUI unit.
    scale: Cell<f32>,
}

impl UiFont {
    pub fn new(face: &Face) -> crate::Result<Self> {
        let fonts = SystemFonts::new()?;
        let face = if fonts.has_family(&face.family) { face.clone() } else { Face::new(crate::settings::LAST_RESORT, face.weight) };
        Ok(Self {
            fonts,
            face,
            cache: RefCell::new(Cache {
                glyphs: HashMap::new(),
                page: Page::new(PAGE, PAGE),
                pixels: vec![0; (PAGE * PAGE) as usize],
                dirty: true,
                overflowed: false,
                told: false,
            }),
            scale: Cell::new(1.0),
        })
    }

    /// Sets how many pixels a GUI unit takes.
    pub fn set_scale(&self, scale: f32) {
        self.scale.set(scale);
    }

    /// The glyph for `c` at `size` pixels.
    fn glyph(&self, c: char, size: f32) -> Option<Placed> {
        let key = (c, size.to_bits());
        if let Some(found) = self.cache.borrow().glyphs.get(&key) {
            return *found;
        }
        let placed = self.draw(c, size);
        self.cache.borrow_mut().glyphs.insert(key, placed);
        placed
    }

    fn draw(&self, c: char, size: f32) -> Option<Placed> {
        let face = self.fonts.face_for(&self.face, c as u32)?;
        let mut bitmap = self.fonts.rasterize(&face, c as u32, size).ok()?;
        Tone::Light.apply(&mut bitmap.alpha);
        let mut cache = self.cache.borrow_mut();
        // A full page starts over only between layouts ([`Self::layout_frame`]):
        // text laid out before would point at glyphs no longer there.
        let Some(spot) = cache.page.place(bitmap.width.max(1), bitmap.height.max(1)) else {
            cache.overflowed = true;
            return Some(Placed { x: 0, y: 0, w: 0, h: 0, left: bitmap.left, top: bitmap.top, advance: bitmap.advance });
        };
        for row in 0..bitmap.height {
            let from = (row * bitmap.width) as usize;
            let to = ((spot.1 + row) * PAGE + spot.0) as usize;
            cache.pixels[to..to + bitmap.width as usize].copy_from_slice(&bitmap.alpha[from..from + bitmap.width as usize]);
        }
        cache.dirty = true;
        Some(Placed { x: spot.0, y: spot.1, w: bitmap.width, h: bitmap.height, left: bitmap.left, top: bitmap.top, advance: bitmap.advance })
    }

    /// The glyphs of one line of text whose top-left is at (x, y), all in GUI
    /// units; the quads are in pixels.
    fn layout(&self, text: &str, size: f32, x: f32, y: f32) -> Vec<Quad> {
        let scale = self.scale.get();
        let size = (size * scale).round();
        let baseline = (y * scale).round() + (size * 0.95).round();
        let mut pen = (x * scale).round();
        let mut out = Vec::new();
        for c in text.chars() {
            let Some(g) = self.glyph(c, size) else { continue };
            if g.w > 0 && g.h > 0 {
                out.push(Quad {
                    screen: Rect { x: (pen + g.left as f32).round(), y: baseline + g.top as f32, w: g.w as f32, h: g.h as f32 },
                    page: Rect { x: g.x as f32, y: g.y as f32, w: g.w as f32, h: g.h as f32 },
                });
            }
            pen += g.advance;
        }
        out
    }

    /// The glyphs of each of a frame's texts, `(text, size, x, y)` as for
    /// [`Self::layout`], all on the page as it is uploaded after. When a
    /// glyph did not fit, the page starts over and the frame's text is laid
    /// out again, onto a page that holds this frame's glyphs alone. A frame
    /// that needs more than the page holds goes without those that do not
    /// fit; frames after it find them so and draw without them, rather than
    /// start the page over each frame, until a new glyph does not fit.
    pub fn layout_frame(&self, texts: &[(&str, f32, f32, f32)]) -> Vec<Vec<Quad>> {
        let lay_out = || texts.iter().map(|&(text, size, x, y)| self.layout(text, size, x, y)).collect::<Vec<_>>();
        let quads = lay_out();
        if !self.take_overflowed() {
            return quads;
        }
        {
            let mut cache = self.cache.borrow_mut();
            cache.glyphs.clear();
            cache.page = Page::new(PAGE, PAGE);
            cache.pixels.fill(0);
            cache.dirty = true;
        }
        let quads = lay_out();
        if self.take_overflowed() && !std::mem::replace(&mut self.cache.borrow_mut().told, true) {
            log!("framework UI: a frame needs more glyphs than the text page holds; those that do not fit are left out");
        }
        quads
    }

    fn take_overflowed(&self) -> bool {
        std::mem::take(&mut self.cache.borrow_mut().overflowed)
    }

    /// The whole page has to go up again, to a texture made anew.
    pub fn mark_dirty(&self) {
        self.cache.borrow_mut().dirty = true;
    }

    /// The page's coverage, if it changed since the last call.
    pub fn take_page(&self) -> Option<Vec<u8>> {
        let mut cache = self.cache.borrow_mut();
        std::mem::take(&mut cache.dirty).then(|| cache.pixels.clone())
    }
}

impl Measure for UiFont {
    fn width(&self, text: &str, size: f32) -> f32 {
        let scale = self.scale.get();
        let pixels = (size * scale).round();
        text.chars().filter_map(|c| self.glyph(c, pixels)).map(|g| g.advance).sum::<f32>() / scale
    }

    fn wrap(&self, text: &str, size: f32, width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let breaks: Vec<usize> = crate::icu::load()
                .ok()
                .and_then(|icu| icu.line_breaks(paragraph).ok())
                .map(|b| b.into_iter().map(|b| b.offset).collect())
                .unwrap_or_else(|| vec![paragraph.len()]);
            let fits = |from: usize, to: usize| self.width(paragraph[from..to].trim_end(), size) <= width;
            let before = lines.len();
            // `fit` is the furthest break the current line can end at.
            let (mut start, mut fit) = (0, 0);
            for &end in &breaks {
                if fits(start, end) {
                    fit = end;
                    continue;
                }
                if fit > start {
                    lines.push(paragraph[start..fit].trim_end().to_string());
                    start = fit;
                }
                if fits(start, end) {
                    fit = end;
                } else {
                    // A single piece wider than the line stands on its own.
                    lines.push(paragraph[start..end].trim_end().to_string());
                    (start, fit) = (end, end);
                }
            }
            if start < paragraph.len() || lines.len() == before {
                lines.push(paragraph[start..].trim_end().to_string());
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_wraps_between_characters_and_english_at_spaces() {
        if !crate::unicode::system_font::chinese_installed() {
            return;
        }
        let font = UiFont::new(&Face::regular("Microsoft YaHei")).unwrap();
        let one = font.width("中", 16.0);
        assert!((12.0..=20.0).contains(&one), "{one}");
        let lines = font.wrap("中文字体的显示效果和以前一样", 16.0, one * 5.5);
        assert!(lines.len() >= 3, "{lines:?}");
        assert!(lines.iter().all(|l| font.width(l, 16.0) <= one * 5.5 + 0.5), "{lines:?}");
        assert_eq!(lines.concat(), "中文字体的显示效果和以前一样");
        let words = font.wrap("one two three four", 16.0, font.width("three four", 16.0) + 1.0);
        assert_eq!(words, ["one two", "three four"]);
        assert_eq!(font.wrap("a\n\nb", 16.0, 100.0), ["a", "", "b"]);
    }

    #[test]
    fn the_framework_ui_draws_light_text_only() {
        use super::super::widgets::{MUTED, TEXT};
        assert_eq!(Tone::of(TEXT), Tone::Light);
        assert_eq!(Tone::of(MUTED), Tone::Light);
    }

    #[test]
    fn layout_places_glyphs_left_to_right_and_fills_the_page() {
        if !crate::unicode::system_font::chinese_installed() {
            return;
        }
        let font = UiFont::new(&Face::regular("Microsoft YaHei")).unwrap();
        let quads = font.layout("汉字", 18.0, 100.0, 50.0);
        assert_eq!(quads.len(), 2);
        assert!(quads[1].screen.x > quads[0].screen.x);
        assert!(quads.iter().all(|q| q.screen.y >= 50.0 && q.screen.y < 50.0 + 18.0));
        let page = font.take_page().expect("new glyphs dirty the page");
        let q = quads[0].page;
        let covered = (0..q.h as u32)
            .flat_map(|r| (0..q.w as u32).map(move |c| (r, c)))
            .filter(|&(r, c)| page[((q.y as u32 + r) * PAGE + q.x as u32 + c) as usize] > 0)
            .count();
        assert!(covered > 20);
        assert!(font.take_page().is_none());
    }

    /// Whether every quad of a frame, of characters all different, draws a
    /// glyph of its own from `page`: none overlaps another, and each covers
    /// some of it.
    fn each_draws_its_own(quads: &[Quad], page: &[u8]) -> bool {
        let apart = |a: &Rect, b: &Rect| a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
        let covers = |q: &Rect| (0..q.h as u32).any(|r| (0..q.w as u32).any(|c| page[((q.y as u32 + r) * PAGE + q.x as u32 + c) as usize] > 0));
        quads.iter().enumerate().all(|(i, a)| covers(&a.page) && quads[i + 1..].iter().all(|b| apart(&a.page, &b.page)))
    }

    #[test]
    fn a_frame_draws_from_the_page_it_uploads_when_the_page_fills() {
        if !crate::unicode::system_font::chinese_installed() {
            return;
        }
        let font = UiFont::new(&Face::regular("Microsoft YaHei")).unwrap();
        // Characters this large fill the page with about a hundred.
        let text = |from: u32, n: u32| (from..from + n).filter_map(char::from_u32).collect::<String>();
        let frame = |texts: &[String]| font.layout_frame(&texts.iter().map(|t| (t.as_str(), 100.0, 0.0, 0.0)).collect::<Vec<_>>()).concat();
        let first = [text(0x4e00, 60)];
        frame(&first);
        font.take_page().expect("the first frame's glyphs");
        // The second frame's glyphs do not fit beside the first's.
        let second = [text(0x4e00 + 60, 30), text(0x4e00 + 90, 30)];
        let quads = frame(&second);
        assert_eq!(quads.len(), 60);
        assert!(each_draws_its_own(&quads, &font.take_page().expect("the second frame's glyphs")));
        // A frame with more than the page holds draws what fits, and the
        // next frame like it finds the page as it is.
        let third = [text(0x4e00 + 120, 50), text(0x4e00 + 170, 50), text(0x4e00 + 220, 50)];
        let quads = frame(&third);
        assert!((1..150).contains(&quads.len()), "{}", quads.len());
        assert!(each_draws_its_own(&quads, &font.take_page().expect("the third frame's glyphs")));
        assert_eq!(frame(&third), quads);
        assert!(font.take_page().is_none(), "the page started over again");
    }
}
