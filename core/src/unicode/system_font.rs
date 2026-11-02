//! Glyphs from system fonts (GLOSSARY), drawn with DirectWrite into 8-bit
//! coverage bitmaps, anti-aliased across and down: for every character the
//! game draws (ADR 0021). Anti-aliased across only, as DirectWrite's natural
//! mode draws, the slanting strokes of Chinese characters show steps.
//!
//! A character goes to the first of the chosen family and the fallback
//! families that has it, in the chosen weight when that family has it, else
//! the weight just below ([`weight_for`]). Weights are only those a family's
//! maker made; none is simulated.
use crate::Result;
use std::collections::HashMap;
use std::sync::Mutex;
use windows::core::HSTRING;
use windows::Win32::Graphics::DirectWrite::*;

/// Tried after the chosen family, in order.
const FALLBACK: &[&str] = &[
    crate::settings::DEFAULT_FONT,
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

/// The weight a font's regular face has.
pub const REGULAR: u16 = 400;

/// A system font in one weight (GLOSSARY): what text asks to be drawn in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Face {
    pub family: String,
    /// From 1 to 999; see [`REGULAR`].
    pub weight: u16,
}

impl Face {
    pub fn new(family: impl Into<String>, weight: u16) -> Self {
        Self { family: family.into(), weight }
    }

    pub fn regular(family: impl Into<String>) -> Self {
        Self::new(family, REGULAR)
    }
}

impl std::fmt::Display for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {}", self.family, self.weight)
    }
}

/// Of a family's weights, lightest first, the index of the one that draws
/// `wanted`: that weight, else the heaviest one lighter than it, else the
/// lightest.
fn weight_for(have: &[u16], wanted: u16) -> Option<usize> {
    have.iter().rposition(|&w| w <= wanted).or((!have.is_empty()).then_some(0))
}

/// DirectWrite's names for its weights.
const NAMES: &[(u16, &str)] = &[
    (100, "Thin"),
    (200, "ExtraLight"),
    (300, "Light"),
    (350, "SemiLight"),
    (400, "Regular"),
    (500, "Medium"),
    (600, "SemiBold"),
    (700, "Bold"),
    (800, "ExtraBold"),
    (900, "Black"),
    (950, "ExtraBlack"),
];

/// The name of the standard weight nearest `weight`: for a weight whose font
/// gives it no English name, or that no installed font has.
pub fn weight_name(weight: u16) -> &'static str {
    NAMES.iter().min_by_key(|(w, _)| w.abs_diff(weight)).map_or("Regular", |(_, name)| name)
}

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

/// One of a family's weights.
struct Weight {
    weight: u16,
    /// What the font calls it in English.
    name: String,
    font: IDWriteFont,
}

pub struct SystemFonts {
    factory: IDWriteFactory2,
    collection: IDWriteFontCollection,
    /// Each family's weights, lightest first; None when it is not installed.
    families: Mutex<HashMap<String, Option<Vec<Weight>>>>,
}

/// The string of `names` in `locale`.
fn localized(names: &IDWriteLocalizedStrings, locale: &str) -> Option<String> {
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
            Ok(Self { factory, collection, families: Mutex::new(HashMap::new()) })
        }
    }

    /// The weights of an installed family: its faces as wide and as upright
    /// as its regular one, as their maker made them.
    fn load(&self, family: &str) -> Option<Vec<Weight>> {
        unsafe {
            let (mut index, mut exists) = (0u32, false.into());
            self.collection.FindFamilyName(&HSTRING::from(family), &mut index, &mut exists).ok()?;
            if !exists.as_bool() {
                return None;
            }
            let family = self.collection.GetFontFamily(index).ok()?;
            let regular = family.GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL).ok()?;
            let (stretch, style) = (regular.GetStretch(), regular.GetStyle());
            let mut weights: Vec<Weight> = Vec::new();
            for i in 0..family.GetFontCount() {
                let Ok(font) = family.GetFont(i) else { continue };
                if font.GetStretch() != stretch || font.GetStyle() != style || font.GetSimulations() != DWRITE_FONT_SIMULATIONS_NONE {
                    continue;
                }
                let weight = font.GetWeight().0.clamp(1, 999) as u16;
                if weights.iter().any(|w| w.weight == weight) {
                    continue;
                }
                let name = font.GetFaceNames().ok().and_then(|names| localized(&names, "en-us")).unwrap_or_else(|| weight_name(weight).to_string());
                weights.push(Weight { weight, name, font });
            }
            weights.sort_by_key(|w| w.weight);
            (!weights.is_empty()).then_some(weights)
        }
    }

    /// What `f` makes of `family`'s weights, when it is installed.
    fn with_weights<T>(&self, family: &str, f: impl FnOnce(&[Weight]) -> T) -> Option<T> {
        let mut families = self.families.lock().unwrap_or_else(|e| e.into_inner());
        families.entry(family.to_string()).or_insert_with(|| self.load(family)).as_deref().map(f)
    }

    /// The weight of `face`'s family that draws it (see [`weight_for`]).
    fn font(&self, face: &Face) -> Option<IDWriteFont> {
        self.with_weights(&face.family, |weights| {
            let have: Vec<u16> = weights.iter().map(|w| w.weight).collect();
            weight_for(&have, face.weight).map(|i| weights[i].font.clone())
        })
        .flatten()
    }

    /// Whether `family` is installed.
    pub fn has_family(&self, family: &str) -> bool {
        self.with_weights(family, |_| ()).is_some()
    }

    /// The weights `family` comes in, lightest first, each with its English
    /// name; none when it is not installed.
    pub fn weights(&self, family: &str) -> Vec<(u16, String)> {
        self.with_weights(family, |weights| weights.iter().map(|w| (w.weight, w.name.clone())).collect()).unwrap_or_default()
    }

    /// `face` as it is drawn: in the weight of its family that draws it (see
    /// [`weight_for`]); None when the family is not installed.
    pub fn drawn(&self, face: &Face) -> Option<Face> {
        let have: Vec<u16> = self.weights(&face.family).into_iter().map(|(w, _)| w).collect();
        weight_for(&have, face.weight).map(|i| Face::new(face.family.clone(), have[i]))
    }

    /// Installed families that have `scalar`, as (English name, name in the
    /// player's language), sorted by the latter.
    pub fn families_with(&self, scalar: u32, chinese: bool) -> Vec<(String, String)> {
        let mut out = Vec::new();
        unsafe {
            for i in 0..self.collection.GetFontFamilyCount() {
                let Ok(family) = self.collection.GetFontFamily(i) else { continue };
                let Ok(names) = family.GetFamilyNames() else { continue };
                let Some(english) = localized(&names, "en-us") else { continue };
                let has = family
                    .GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL)
                    .and_then(|f| f.HasCharacter(scalar))
                    .is_ok_and(|h| h.as_bool());
                if has && !english.starts_with('@') {
                    let shown = if chinese { localized(&names, "zh-cn").unwrap_or_else(|| english.clone()) } else { english.clone() };
                    out.push((english, shown));
                }
            }
        }
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out.dedup();
        out
    }

    /// The face that draws `scalar`: `face` when its family has it, else the
    /// first fallback family that does, in the same weight.
    pub fn face_for(&self, face: &Face, scalar: u32) -> Option<Face> {
        std::iter::once(face.family.as_str())
            .chain(FALLBACK.iter().copied())
            .map(|family| Face::new(family, face.weight))
            .find(|f| self.font(f).is_some_and(|font| unsafe { font.HasCharacter(scalar).is_ok_and(|has| has.as_bool()) }))
    }

    /// How far `face`'s lines reach above and below the baseline, in ems.
    pub fn line_metrics(&self, face: &Face) -> Option<(f32, f32)> {
        let metrics = self.metrics(face)?;
        let em = metrics.designUnitsPerEm as f32;
        (em > 0.0).then(|| (metrics.ascent as f32 / em, metrics.descent as f32 / em))
    }

    fn metrics(&self, face: &Face) -> Option<DWRITE_FONT_METRICS> {
        let font = self.font(face)?;
        let mut metrics = DWRITE_FONT_METRICS::default();
        unsafe { font.GetMetrics(&mut metrics) };
        Some(metrics)
    }

    /// Draws `scalar` in `face` at an em size of `em` pixels.
    pub fn rasterize(&self, face: &Face, scalar: u32, em: f32) -> Result<Bitmap> {
        let font = self.font(face).ok_or_else(|| format!("no font family {:?}", face.family))?;
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
                DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC,
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

/// For tests: whether this machine has a font that draws Chinese. Some
/// Windows images, a build server's among them, have none, and the tests that
/// draw Chinese then have nothing to draw with; it says so.
#[cfg(test)]
pub(crate) fn chinese_installed() -> bool {
    let found = SystemFonts::new().is_ok_and(|fonts| !fonts.families_with('永' as u32, false).is_empty());
    if !found {
        eprintln!("no installed font draws Chinese; skipped");
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_chinese_character_with_grey_edges() {
        if !chinese_installed() {
            return;
        }
        let fonts = SystemFonts::new().unwrap();
        // Diagonal and curved strokes, so the edges are partly covered.
        let face = fonts.face_for(&Face::regular("Microsoft YaHei"), '永' as u32).unwrap();
        let glyph = fonts.rasterize(&face, '永' as u32, 18.0).unwrap();
        assert!(glyph.width > 8 && glyph.width <= 20 && glyph.height > 8 && glyph.height <= 22, "{glyph:?}");
        assert!(glyph.top < 0, "the glyph sits above the baseline: {}", glyph.top);
        assert!((15.0..=20.0).contains(&glyph.advance), "{}", glyph.advance);
        let levels: std::collections::BTreeSet<u8> = glyph.alpha.iter().copied().collect();
        assert!(levels.len() > 8, "anti-aliased, not two-level: {levels:?}");
    }

    #[test]
    fn grey_edges_run_down_as_well_as_across() {
        if !chinese_installed() {
            return;
        }
        let fonts = SystemFonts::new().unwrap();
        // A horizontal stroke: anti-aliased across only, it is one row of
        // solid ink.
        let face = fonts.face_for(&Face::regular("Microsoft YaHei"), '一' as u32).unwrap();
        let glyph = fonts.rasterize(&face, '一' as u32, 20.1).unwrap();
        let middle: Vec<u8> = (0..glyph.height).map(|y| glyph.alpha[(y * glyph.width + glyph.width / 2) as usize]).collect();
        assert!(middle.iter().any(|a| (32..224).contains(a)), "{middle:?}");
    }

    #[test]
    fn falls_back_for_a_character_the_family_lacks() {
        if !chinese_installed() {
            return;
        }
        let fonts = SystemFonts::new().unwrap();
        // Arial has no ideographs; the family that draws them is asked for
        // the same weight.
        let face = fonts.face_for(&Face::new("Arial", 700), '中' as u32).unwrap();
        assert_ne!(face.family, "Arial");
        assert_eq!(face.weight, 700);
        assert!(!fonts.has_family("No Such Family Anywhere"));
        assert!(fonts.weights("No Such Family Anywhere").is_empty());
        assert_eq!(fonts.drawn(&Face::regular("No Such Family Anywhere")), None);
    }

    /// How much ink `scalar` takes, drawn in `face`.
    fn ink(fonts: &SystemFonts, face: &Face, scalar: char) -> u32 {
        fonts.rasterize(face, scalar as u32, 24.0).unwrap().alpha.iter().map(|&a| a as u32).sum()
    }

    #[test]
    fn draws_the_weights_a_family_comes_in() {
        let fonts = SystemFonts::new().unwrap();
        if !fonts.has_family("Microsoft YaHei") {
            eprintln!("Microsoft YaHei is not installed; skipped");
            return;
        }
        let weights = fonts.weights("Microsoft YaHei");
        let names: Vec<&str> = weights.iter().map(|(_, name)| name.as_str()).collect();
        assert!(names.contains(&"Regular") && names.contains(&"Bold"), "{weights:?}");
        assert!(weights.windows(2).all(|w| w[0].0 < w[1].0), "lightest first: {weights:?}");
        let (regular, bold) = (ink(&fonts, &Face::regular("Microsoft YaHei"), '永'), ink(&fonts, &Face::new("Microsoft YaHei", 700), '永'));
        assert!(bold > regular * 6 / 5, "bold {bold}, regular {regular}");
        // It has no SemiBold: that draws as Regular, the weight below.
        assert_eq!(fonts.drawn(&Face::new("Microsoft YaHei", 600)), Some(Face::regular("Microsoft YaHei")));
        assert_eq!(ink(&fonts, &Face::new("Microsoft YaHei", 600), '永'), regular);
    }

    #[test]
    fn a_weight_a_family_lacks_draws_in_the_one_below() {
        let have = [290, 400, 700];
        assert_eq!(weight_for(&have, 400), Some(1));
        assert_eq!(weight_for(&have, 600), Some(1));
        assert_eq!(weight_for(&have, 900), Some(2));
        assert_eq!(weight_for(&have, 300), Some(0));
        // Nothing lighter: the lightest.
        assert_eq!(weight_for(&have, 100), Some(0));
        assert_eq!(weight_for(&[500, 700], 400), Some(0));
        assert_eq!(weight_for(&[], 400), None);
    }

    #[test]
    fn a_weight_without_an_english_name_takes_the_nearest_standard_one() {
        assert_eq!(weight_name(400), "Regular");
        assert_eq!(weight_name(290), "Light");
        assert_eq!(weight_name(350), "SemiLight");
        assert_eq!(weight_name(999), "ExtraBlack");
        assert_eq!(Face::new("KaiTi", 700).to_string(), "\"KaiTi\" 700");
    }
}
