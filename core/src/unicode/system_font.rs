//! Glyphs from system fonts (GLOSSARY), drawn with DirectWrite into 8-bit
//! coverage bitmaps: for characters a bitmap font lacks, and for every
//! character when the player picks the system font look (ADR 0010).
//!
//! A character goes to the first of the chosen family and the fallback
//! families that has it.
use crate::Result;
use std::collections::HashMap;
use std::sync::Mutex;
use windows::core::{Interface, HSTRING};
use windows::Win32::Graphics::DirectWrite::*;

/// Tried after the chosen family, in order.
const FALLBACK: &[&str] = &[
    "Microsoft YaHei",
    "SimSun",
    "SimSun-ExtB",
    "SimSun-ExtG",
    "Microsoft JhengHei",
    "Segoe UI",
    "Segoe UI Symbol",
    "Segoe UI Emoji",
    "Nirmala UI",
    "Malgun Gothic",
    "Yu Gothic",
];

/// One glyph's coverage, positioned against the pen on the baseline.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    /// From the pen to the bitmap's left edge, in pixels.
    pub left: i32,
    /// From the baseline to the bitmap's top edge, in pixels; negative is up.
    pub top: i32,
    /// How far the pen moves, in pixels.
    pub advance: f32,
    /// Row-major coverage, 0..=255, `width * height` bytes.
    pub alpha: Vec<u8>,
}

pub struct SystemFonts {
    factory: IDWriteFactory2,
    collection: IDWriteFontCollection,
    faces: Mutex<HashMap<String, Option<IDWriteFont>>>,
}

// DirectWrite's shared factory and its objects are free-threaded.
unsafe impl Send for SystemFonts {}
unsafe impl Sync for SystemFonts {}

impl SystemFonts {
    pub fn new() -> Result<Self> {
        unsafe {
            let factory: IDWriteFactory2 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).map_err(|e| format!("DWriteCreateFactory: {e}"))?;
            let mut collection = None;
            factory.GetSystemFontCollection(&mut collection, false).map_err(|e| format!("system font collection: {e}"))?;
            let collection = collection.ok_or("no system font collection")?;
            Ok(Self { factory, collection, faces: Mutex::new(HashMap::new()) })
        }
    }

    /// The regular face of an installed family, if there is one.
    fn font(&self, family: &str) -> Option<IDWriteFont> {
        let mut faces = self.faces.lock().unwrap_or_else(|e| e.into_inner());
        faces
            .entry(family.to_string())
            .or_insert_with(|| unsafe {
                let (mut index, mut exists) = (0u32, false.into());
                self.collection.FindFamilyName(&HSTRING::from(family), &mut index, &mut exists).ok()?;
                if !exists.as_bool() {
                    return None;
                }
                self.collection
                    .GetFontFamily(index)
                    .ok()?
                    .GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL)
                    .ok()
            })
            .clone()
    }

    /// Whether `family` is installed.
    pub fn has_family(&self, family: &str) -> bool {
        self.font(family).is_some()
    }

    /// Installed families that have `scalar`, as (English name, name in the
    /// player's language), sorted by the latter.
    pub fn families_with(&self, scalar: u32, chinese: bool) -> Vec<(String, String)> {
        let name = |names: &IDWriteLocalizedStrings, locale: &str| -> Option<String> {
            unsafe {
                let (mut index, mut exists) = (0u32, false.into());
                names.FindLocaleName(&HSTRING::from(locale), &mut index, &mut exists).ok()?;
                if !exists.as_bool() {
                    return None;
                }
                let len = names.GetStringLength(index).ok()? as usize;
                let mut buf = vec![0u16; len + 1];
                names.GetString(index, &mut buf).ok()?;
                Some(String::from_utf16_lossy(&buf[..len]))
            }
        };
        let mut out = Vec::new();
        unsafe {
            for i in 0..self.collection.GetFontFamilyCount() {
                let Ok(family) = self.collection.GetFontFamily(i) else { continue };
                let Ok(names) = family.GetFamilyNames() else { continue };
                let Some(english) = name(&names, "en-us") else { continue };
                let has = family
                    .GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL)
                    .and_then(|f| f.HasCharacter(scalar))
                    .is_ok_and(|h| h.as_bool());
                if has && !english.starts_with('@') {
                    let shown = if chinese { name(&names, "zh-cn").unwrap_or_else(|| english.clone()) } else { english.clone() };
                    out.push((english, shown));
                }
            }
        }
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out.dedup();
        out
    }

    /// The family that draws `scalar`: `family` when it has it, else the first
    /// fallback family that does.
    pub fn family_for<'a>(&self, family: &'a str, scalar: u32) -> Option<&'a str>
    where
        'static: 'a,
    {
        std::iter::once(family).chain(FALLBACK.iter().copied()).find(|f| {
            self.font(f).is_some_and(|font| unsafe { font.HasCharacter(scalar).is_ok_and(|has| has.as_bool()) })
        })
    }

    /// How far `family`'s lines reach above and below the baseline, in ems.
    pub fn line_metrics(&self, family: &str) -> Option<(f32, f32)> {
        let font = self.font(family)?;
        let mut metrics = DWRITE_FONT_METRICS::default();
        unsafe { font.GetMetrics(&mut metrics) };
        let em = metrics.designUnitsPerEm as f32;
        (em > 0.0).then(|| (metrics.ascent as f32 / em, metrics.descent as f32 / em))
    }

    /// Draws `scalar` in `family` at an em size of `em` pixels.
    pub fn rasterize(&self, family: &str, scalar: u32, em: f32) -> Result<Bitmap> {
        let font = self.font(family).ok_or_else(|| format!("no font family {family:?}"))?;
        unsafe {
            let face = font.CreateFontFace().map_err(|e| format!("font face: {e}"))?;
            let mut index = 0u16;
            face.GetGlyphIndices(&scalar, 1, &mut index).map_err(|e| format!("glyph index: {e}"))?;
            let mut design = DWRITE_FONT_METRICS::default();
            face.GetMetrics(&mut design);
            let mut metrics = DWRITE_GLYPH_METRICS::default();
            face.GetDesignGlyphMetrics(&index, 1, &mut metrics, false).map_err(|e| format!("glyph metrics: {e}"))?;
            let advance = metrics.advanceWidth as f32 * em / design.designUnitsPerEm as f32;
            let zero = 0.0f32;
            let run = DWRITE_GLYPH_RUN {
                fontFace: std::mem::ManuallyDrop::new(Some(face.clone())),
                fontEmSize: em,
                glyphCount: 1,
                glyphIndices: &index,
                glyphAdvances: &zero,
                glyphOffsets: std::ptr::null(),
                isSideways: false.into(),
                bidiLevel: 0,
            };
            let analysis = self.factory.CreateGlyphRunAnalysis(
                &run,
                None,
                DWRITE_RENDERING_MODE_NATURAL,
                DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_GRID_FIT_MODE_DEFAULT,
                DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                0.0,
                0.0,
            );
            drop(std::mem::ManuallyDrop::into_inner(run.fontFace));
            let analysis = analysis.map_err(|e| format!("glyph run analysis: {e}"))?;
            let bounds = analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1).map_err(|e| format!("texture bounds: {e}"))?;
            let (width, height) = ((bounds.right - bounds.left).max(0) as u32, (bounds.bottom - bounds.top).max(0) as u32);
            let mut alpha = vec![0u8; (width * height) as usize];
            if !alpha.is_empty() {
                analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &bounds, &mut alpha).map_err(|e| format!("alpha texture: {e}"))?;
            }
            Ok(Bitmap { width, height, left: bounds.left, top: bounds.top, advance, alpha })
        }
    }
}

/// Keeps `Interface` in use for `clone()` on COM pointers.
#[allow(dead_code)]
fn _interface<T: Interface>(_: &T) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_chinese_character_with_grey_edges() {
        let fonts = SystemFonts::new().unwrap();
        // Diagonal and curved strokes, so the edges are partly covered.
        let family = fonts.family_for("Microsoft YaHei", '永' as u32).unwrap();
        let glyph = fonts.rasterize(family, '永' as u32, 18.0).unwrap();
        assert!(glyph.width > 8 && glyph.width <= 20 && glyph.height > 8 && glyph.height <= 22, "{glyph:?}");
        assert!(glyph.top < 0, "the glyph sits above the baseline: {}", glyph.top);
        assert!((15.0..=20.0).contains(&glyph.advance), "{}", glyph.advance);
        let levels: std::collections::BTreeSet<u8> = glyph.alpha.iter().copied().collect();
        assert!(levels.len() > 8, "anti-aliased, not two-level: {levels:?}");
    }

    #[test]
    fn falls_back_for_a_character_the_family_lacks() {
        let fonts = SystemFonts::new().unwrap();
        // Arial has no ideographs.
        assert_ne!(fonts.family_for("Arial", '中' as u32), Some("Arial"));
        assert!(fonts.family_for("Arial", '中' as u32).is_some());
        assert!(!fonts.has_family("No Such Family Anywhere"));
    }
}
