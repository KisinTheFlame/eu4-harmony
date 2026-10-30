//! The outline a map font draws around its glyphs, given to system glyphs on
//! the map. The map fonts of the game and of the Chinese language mod draw
//! dark ink inside a lighter band that fades out over a few pixels, so names
//! stand out from the map; their pages hold it, and the game draws the page's
//! colors as they are. The band is read from the font's own page, as its
//! color and how opaque it is at each distance from the ink, and drawn
//! around a system glyph by each pixel's distance from that glyph's ink.
use super::system_font::Bitmap;

/// The farthest from the ink an outline is read, in pixels.
const MOST_REACH: usize = 16;
/// Pixels less opaque than this are where an outline fades out, or where
/// ink without one is anti-aliased.
const FAINT: u32 = 64;
/// How far apart the ink and the faint pixels' colors must be, in the sum
/// of their channels' differences, for the faint pixels to be an outline.
const LEAST_CONTRAST: u32 = 96;
/// An outline ends where it is first on average less opaque than this:
/// further out, a glyph's box may hold the edge of the glyph beside it.
const LEAST_ALPHA: u32 = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    /// 0x00RRGGBB.
    pub color: u32,
    /// How opaque the outline is one pixel from the ink, two pixels, and so on.
    alpha: Vec<u8>,
}

impl Outline {
    /// The outline around a glyph of a bitmap font, from its pixels
    /// (0xAARRGGBB, `width` to a row) and its ink color (0x00RRGGBB): the
    /// faint pixels' color, and at each whole pixel of distance from the
    /// ink the pixels' mean opacity. None when the faint pixels are the
    /// ink's color, its edges only anti-aliased.
    pub fn measure(pixels: &[u32], width: u32, height: u32, ink: u32) -> Option<Outline> {
        let faint: Vec<u32> = pixels.iter().copied().filter(|p| (1..FAINT).contains(&(p >> 24))).collect();
        if faint.is_empty() {
            return None;
        }
        let channel = |shift: u32| faint.iter().map(|p| p >> shift & 0xff).sum::<u32>() / faint.len() as u32;
        let color = channel(16) << 16 | channel(8) << 8 | channel(0);
        if difference(color, ink) < LEAST_CONTRAST {
            return None;
        }
        let is_ink: Vec<bool> = pixels.iter().map(|&p| p >> 24 >= 0x80 && difference(p, ink) < difference(p, color)).collect();
        let distance = distances(&is_ink, width, height);
        let mut sums = [(0u32, 0u32); MOST_REACH];
        for (p, d) in pixels.iter().zip(&distance) {
            let at = d.round() as usize;
            if (1..=MOST_REACH).contains(&at) {
                sums[at - 1].0 += p >> 24;
                sums[at - 1].1 += 1;
            }
        }
        let mut alpha: Vec<u8> = sums.iter().map(|&(sum, n)| sum.checked_div(n).unwrap_or(0) as u8).collect();
        let reach = alpha.iter().position(|&a| (a as u32) < LEAST_ALPHA).unwrap_or(MOST_REACH);
        alpha.truncate(reach);
        (reach > 0).then_some(Outline { color, alpha })
    }

    /// How many pixels beyond the ink the outline reaches.
    pub fn reach(&self) -> u32 {
        self.alpha.len() as u32
    }

    /// How opaque the outline is `distance` pixels from the ink, 0.0..=255.0;
    /// under the ink as next to it.
    fn at(&self, distance: f32) -> f32 {
        let d = distance.max(1.0);
        let (whole, part) = (d.floor() as usize, d.fract());
        let alpha = |at: usize| self.alpha.get(at - 1).map_or(0.0, |&a| a as f32);
        alpha(whole) * (1.0 - part) + alpha(whole + 1) * part
    }

    /// A glyph drawn in `ink` (0x00RRGGBB) with `bitmap`'s coverage, inside
    /// the outline: the bitmap made larger by the outline's reach on every
    /// side, its alpha the outlined glyph's, and its pixels as 0xAARRGGBB,
    /// the ink over the outline.
    pub fn around(&self, bitmap: &Bitmap, ink: u32) -> (Bitmap, Vec<u32>) {
        let reach = self.reach();
        let (width, height) = (bitmap.width + 2 * reach, bitmap.height + 2 * reach);
        let mut coverage = vec![0u8; (width * height) as usize];
        for y in 0..bitmap.height {
            let from = (y * bitmap.width) as usize;
            let to = ((y + reach) * width + reach) as usize;
            coverage[to..to + bitmap.width as usize].copy_from_slice(&bitmap.alpha[from..from + bitmap.width as usize]);
        }
        let is_ink: Vec<bool> = coverage.iter().map(|&c| c >= 0x80).collect();
        let distance = distances(&is_ink, width, height);
        let pixels: Vec<u32> = coverage.iter().zip(&distance).map(|(&c, &d)| over(ink, c, self.color, self.at(d))).collect();
        let alpha = pixels.iter().map(|p| (p >> 24) as u8).collect();
        let outlined = Bitmap { width, height, left: bitmap.left - reach as i32, top: bitmap.top - reach as i32, advance: bitmap.advance, alpha };
        (outlined, pixels)
    }
}

/// The sum of the differences between two colors' channels.
fn difference(a: u32, b: u32) -> u32 {
    [16, 8, 0].iter().map(|s| (a >> s & 0xff).abs_diff(b >> s & 0xff)).sum()
}

/// `ink` at `coverage` over `color` at `under` (0.0..=255.0), as one
/// 0xAARRGGBB pixel, its color not premultiplied: the map blends a page's
/// colors by their alpha.
fn over(ink: u32, coverage: u8, color: u32, under: f32) -> u32 {
    let (top, under) = (coverage as f32 / 255.0, under / 255.0);
    let alpha = top + under * (1.0 - top);
    if alpha <= 0.0 {
        return 0;
    }
    let channel = |shift: u32| {
        let (i, o) = ((ink >> shift & 0xff) as f32, (color >> shift & 0xff) as f32);
        ((i * top + o * under * (1.0 - top)) / alpha).round() as u32
    };
    ((alpha * 255.0).round() as u32) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0)
}

/// Far from everything, for [`distances`].
const FAR: f32 = 1e20;

/// The distance from each pixel's center to the nearest ink pixel's, in
/// pixels (`width` to a row); 0 on the ink, [`FAR`] or more with no ink at
/// all. Exact: squared distances across each row, then down each column
/// (Felzenszwalb and Huttenlocher's distance transform).
fn distances(ink: &[bool], width: u32, height: u32) -> Vec<f32> {
    let (width, height) = (width as usize, height as usize);
    let mut squared: Vec<f32> = ink.iter().map(|&i| if i { 0.0 } else { FAR }).collect();
    let mut line = Vec::new();
    for y in 0..height {
        line.clear();
        line.extend_from_slice(&squared[y * width..(y + 1) * width]);
        squared[y * width..(y + 1) * width].copy_from_slice(&along(&line));
    }
    for x in 0..width {
        line.clear();
        line.extend((0..height).map(|y| squared[y * width + x]));
        for (y, d) in along(&line).into_iter().enumerate() {
            squared[y * width + x] = d;
        }
    }
    squared.into_iter().map(f32::sqrt).collect()
}

/// For each place on a line, the least of `f[q] + (p - q)²` over every `q`:
/// the lower envelope of the parabolas rooted at each place.
fn along(f: &[f32]) -> Vec<f32> {
    let n = f.len();
    if n == 0 {
        return Vec::new();
    }
    // The parabolas on the envelope, and where each starts to be lowest.
    let (mut roots, mut starts) = (vec![0usize; n], vec![0f32; n + 1]);
    let mut k = 0;
    starts[0] = -FAR;
    starts[1] = FAR;
    for q in 1..n {
        let meets = |r: usize| ((f[q] + (q * q) as f32) - (f[r] + (r * r) as f32)) / (2 * q - 2 * r) as f32;
        let mut s = meets(roots[k]);
        while s <= starts[k] {
            k -= 1;
            s = meets(roots[k]);
        }
        k += 1;
        roots[k] = q;
        starts[k] = s;
        starts[k + 1] = FAR;
    }
    k = 0;
    (0..n)
        .map(|p| {
            while starts[k + 1] < p as f32 {
                k += 1;
            }
            let r = roots[k];
            (p as f32 - r as f32).powi(2) + f[r]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `size`-pixel square glyph: a solid block of `ink` `margin` pixels in
    /// from each side, inside an outline of `color` whose opacity at each
    /// distance from the block `profile` gives.
    fn glyph(size: u32, margin: u32, ink: u32, color: u32, profile: &[u8]) -> Vec<u32> {
        let inside = margin..size - margin;
        let mut pixels = Vec::new();
        for y in 0..size {
            for x in 0..size {
                let dx = if x < inside.start { inside.start - x } else { x.saturating_sub(inside.end - 1) };
                let dy = if y < inside.start { inside.start - y } else { y.saturating_sub(inside.end - 1) };
                let d = ((dx * dx + dy * dy) as f32).sqrt().round() as usize;
                pixels.push(match d {
                    0 => 0xff00_0000 | ink,
                    d => profile.get(d - 1).map_or(0, |&a| (a as u32) << 24 | color),
                });
            }
        }
        pixels
    }

    #[test]
    fn distances_are_euclidean() {
        // One ink pixel in the middle of a 7×5 grid.
        let mut ink = vec![false; 35];
        ink[2 * 7 + 3] = true;
        let d = distances(&ink, 7, 5);
        assert_eq!(d[2 * 7 + 3], 0.0);
        assert_eq!(d[2 * 7 + 6], 3.0);
        assert!((d[0] - 13f32.sqrt()).abs() < 1e-5, "{}", d[0]);
        assert!((d[4 * 7 + 5] - 8f32.sqrt()).abs() < 1e-5);
        // No ink at all: everything far.
        assert!(distances(&[false; 6], 3, 2).iter().all(|&d| d >= 1e9));
    }

    #[test]
    fn reads_a_map_fonts_outline() {
        // Like the game's map font: black ink in a grey band fading out over
        // five pixels.
        let profile = [133, 106, 80, 53, 26];
        let mut pixels = glyph(40, 8, 0, 0x91_9191, &profile);
        // The edge of the glyph beside it, past where the outline fades out.
        pixels[39 * 40..].fill(20 << 24 | 0x91_9191);
        let outline = Outline::measure(&pixels, 40, 40, 0).unwrap();
        assert_eq!(outline.color, 0x91_9191);
        assert_eq!(outline.reach(), 5);
        for (got, want) in outline.alpha.iter().zip(profile) {
            assert!(got.abs_diff(want) <= 12, "{:?} against {profile:?}", outline.alpha);
        }
    }

    #[test]
    fn anti_aliased_ink_is_no_outline() {
        // White text on a white page whose edges are only anti-aliased, as
        // the interface's fonts are.
        let pixels = glyph(20, 4, 0xff_ffff, 0xff_ffff, &[48, 16]);
        assert_eq!(Outline::measure(&pixels, 20, 20, 0xff_ffff), None);
        // Nothing faint at all.
        assert_eq!(Outline::measure(&[0xff00_0000; 4], 2, 2, 0), None);
    }

    #[test]
    fn draws_the_outline_around_a_glyph() {
        let outline = Outline { color: 0x80_8080, alpha: vec![200, 100] };
        // A 2×1 glyph: one pixel of solid ink and one a quarter covered.
        let bitmap = Bitmap { width: 2, height: 1, left: 1, top: -1, advance: 3.0, alpha: vec![255, 64] };
        let (outlined, pixels) = outline.around(&bitmap, 0x10_0c10);
        assert_eq!((outlined.width, outlined.height, outlined.left, outlined.top, outlined.advance), (6, 5, -1, -3, 3.0));
        let at = |x: u32, y: u32| pixels[(y * 6 + x) as usize];
        // The ink, opaque, in its color.
        assert_eq!(at(2, 2), 0xff10_0c10);
        // The quarter-covered pixel, one from the ink: ink over the outline
        // at 200, more opaque than either, its color between theirs.
        assert_eq!(at(3, 2) >> 24, 214);
        assert_eq!(at(3, 2) & 0xff, 0x5e);
        // Two pixels out, the outline alone at 100; three out, nothing.
        assert_eq!(at(0, 2), 100 << 24 | 0x80_8080);
        assert_eq!(at(2, 0), 100 << 24 | 0x80_8080);
        assert_eq!(at(4, 2), 100 << 24 | 0x80_8080);
        assert_eq!(at(5, 2), 0);
        // Between: 2.24 pixels from the ink, a quarter of the way to none.
        assert_eq!(at(0, 1) >> 24, 76);
        assert_eq!(outlined.alpha[2 * 6 + 2], 255);
    }

    /// The outline of the game's own map font, from its `H`:
    /// `EU4_GAME_DIR=<install dir> cargo test -p eu4-harmony-core -- --ignored`
    #[test]
    #[ignore]
    fn reads_the_outline_of_the_games_map_font() {
        let fonts = std::path::PathBuf::from(std::env::var("EU4_GAME_DIR").expect("set EU4_GAME_DIR")).join("gfx/fonts");
        let table = std::fs::read_to_string(fonts.join("Mapfont.fnt")).unwrap();
        let h = table.lines().find(|l| l.split_whitespace().next() == Some("char") && l.contains("id=72 ")).unwrap();
        let field = |name: &str| -> u32 { h.split_whitespace().find_map(|f| f.strip_prefix(name)?.strip_prefix('=')?.parse().ok()).unwrap() };
        let (x, y, width, height) = (field("x"), field("y"), field("width"), field("height"));
        // An uncompressed 32-bit DDS, its first level after the 128-byte header.
        let dds = std::fs::read(fonts.join("Mapfont.dds")).unwrap();
        let page_width = u32::from_le_bytes(dds[16..20].try_into().unwrap());
        assert_eq!(u32::from_le_bytes(dds[88..92].try_into().unwrap()), 32, "32 bits a pixel");
        let pixel = |px: u32, py: u32| {
            let at = 128 + 4 * (py * page_width + px) as usize;
            u32::from_le_bytes(dds[at..at + 4].try_into().unwrap())
        };
        let pixels: Vec<u32> = (y..y + height).flat_map(|py| (x..x + width).map(move |px| pixel(px, py))).collect();
        let ink = pixels.iter().max_by_key(|p| *p >> 24).unwrap() & 0x00ff_ffff;
        let outline = Outline::measure(&pixels, width, height, ink).unwrap();
        // Black ink in a light grey band a few pixels wide that fades out.
        assert!(ink & 0xff < 0x40 && outline.color & 0xff > 0x60, "ink {ink:06x}, outline {:06x}", outline.color);
        assert!((3..=10).contains(&outline.reach()), "{outline:?}");
        assert!(outline.alpha.windows(2).all(|w| w[0] >= w[1]), "{outline:?}");
    }

    #[test]
    fn opacity_falls_off_between_whole_pixels() {
        let outline = Outline { color: 0, alpha: vec![200, 100] };
        assert_eq!(outline.at(0.0), 200.0);
        assert_eq!(outline.at(1.0), 200.0);
        assert_eq!(outline.at(1.5), 150.0);
        assert_eq!(outline.at(2.5), 50.0);
        assert_eq!(outline.at(3.0), 0.0);
        assert_eq!(outline.at(40.0), 0.0);
    }
}
