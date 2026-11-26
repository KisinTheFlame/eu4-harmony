//! The alpha a system glyph's coverage becomes, which depends on how light
//! the text it is drawn for is. The game blends text with what lies under it
//! in the screen's sRGB values, not in light: an edge pixel half covered by
//! white ink over black gets the value 128, a fifth of white's light, where
//! half its light would be 188. So light text on dark ground loses its edges
//! and looks thinner and greyer than its font, the more so the thinner its
//! strokes: at body text sizes a Song face's strokes cover few pixels
//! wholly. The framework UI blends its text the same way. Glyphs for light
//! text are drawn with the alpha that gives, over black, the light a blend in
//! light would ([`Tone::Light`]). Dark text on light ground comes out heavier
//! than its font instead, as Windows draws it too; it keeps its coverage.
//!
//! Fonts that draw in light and dark colors can share a page, so a page holds
//! a character's glyph in each tone its fonts need (`fill`).
use std::sync::LazyLock;

/// The luminance from which a color is light: middle grey, which looks
/// halfway between black and white.
const MIDDLE_GREY: f32 = 0.18;

/// How a glyph's coverage becomes its alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    /// For light text, taken to be on dark ground: the alpha that gives over
    /// black the light the coverage would.
    Light,
    /// For dark text: the coverage.
    Dark,
}

impl Tone {
    /// The tone of text in `color`, 0xRRGGBB (bits above are ignored).
    pub fn of(color: u32) -> Self {
        if luminance(color) >= MIDDLE_GREY {
            Tone::Light
        } else {
            Tone::Dark
        }
    }

    /// Turns `coverage` into alpha, in place.
    pub fn apply(self, coverage: &mut [u8]) {
        if self == Tone::Light {
            for a in coverage {
                *a = LIGHT[*a as usize];
            }
        }
    }
}

/// The sRGB value, 0 to 1, the screen shows `light` (0 to 1) with.
fn encode(light: f32) -> f32 {
    if light <= 0.003_130_8 {
        light * 12.92
    } else {
        1.055 * light.powf(1.0 / 2.4) - 0.055
    }
}

/// The light an sRGB value (0 to 1) shows.
fn decode(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The relative luminance of 0xRRGGBB, from 0 (black) to 1 (white).
fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| decode(((color >> shift) & 0xff) as f32 / 255.0);
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// Alpha by coverage for light text: white ink covering a share of a pixel
/// over black gives it that share of white's light, which the screen shows
/// with that light's sRGB value.
static LIGHT: LazyLock<[u8; 256]> = LazyLock::new(|| std::array::from_fn(|c| (encode(c as f32 / 255.0) * 255.0).round() as u8));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_lighter_than_middle_grey_are_light() {
        // The game's fonts: white, grey, red, orange, the pale blue of the
        // map's floating text.
        for color in [0xffffff, 0xbbbbbb, 0xff0000, 0xffbb00, 0x7799ff, 0xffffff99] {
            assert_eq!(Tone::of(color), Tone::Light, "{color:06x}");
        }
        // Black, the map's inks, the dark green of `vic_18_black`'s `G`.
        for color in [0x000000, 0xff000000, 0x333333, 0x100c10, 0x18701a] {
            assert_eq!(Tone::of(color), Tone::Dark, "{color:06x}");
        }
        assert!((luminance(0xffffff) - 1.0).abs() < 1e-6 && luminance(0) == 0.0);
    }

    #[test]
    fn light_text_gets_the_alpha_a_blend_in_light_would_show() {
        let mut alpha: Vec<u8> = (0..=255).collect();
        Tone::Light.apply(&mut alpha);
        assert_eq!((alpha[0], alpha[255]), (0, 255));
        // Half covered: half of white's light.
        assert_eq!(alpha[128], 188);
        assert!(alpha.windows(2).all(|w| w[0] <= w[1]), "{alpha:?}");
        assert!(alpha.iter().enumerate().all(|(c, &a)| a as usize >= c));
        // Back to light, each value is its coverage.
        assert!(alpha.iter().enumerate().all(|(c, &a)| (decode(a as f32 / 255.0) * 255.0 - c as f32).abs() < 1.5));
    }

    #[test]
    fn dark_text_keeps_its_coverage() {
        let mut alpha: Vec<u8> = (0..=255).collect();
        Tone::Dark.apply(&mut alpha);
        assert!(alpha.iter().enumerate().all(|(c, &a)| a as usize == c));
    }
}
