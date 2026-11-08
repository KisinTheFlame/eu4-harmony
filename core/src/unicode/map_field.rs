//! The names on the map drawn from distance fields (plans/map-names-distance-fields.md).
//! The map draws its names with a shader of its own, `gfx/FX/mapname.shader`,
//! which magnifies its font's page with a linear filter, so at the closest
//! zoom each texel of the page is spread over three or four pixels and the
//! names blur (research/map-name-shader.md). So the map font's page holds, for
//! each glyph, the signed distance from its ink's edge, and the shader,
//! changed as the game reads it, draws the ink from it, anti-aliased over one
//! pixel of the screen at any zoom, and the outline the player chose by its
//! distance from the ink, as the font's own page draws it (`outline`).
//!
//! The page is made when the font loads, before the map's setup reads the
//! shader; it holds distances only when the shader as the game will read it
//! has the line the change replaces, and the shader is changed only when the
//! page holds them.
use super::distance;
use super::outline::Outline;
use super::system_font::Bitmap;
use crate::physfs;
use crate::points::Point;
use crate::splice::{Addition, Plan};
use std::fmt::Write;
use std::sync::Mutex;

/// The font the map draws its names in: it asks for it by this name (0x10a36a2)
/// and keeps it at `map+0x4d0`. Nothing else draws in it.
pub const MAP_FONT: &[u8] = b"tahoma_60";
/// The map names' shader, as the map asks for it (with `.lua` for `.shader`).
const SHADER: &str = "gfx/FX/mapname.shader";
/// The pixel shader's read of the page, which the change replaces.
const SAMPLE: &str = "tex2D( DiffuseTexture, v.vTexCoord )";
/// What it becomes: [`decoder`]'s function.
const DECODED: &str = "Eu4HarmonyMapName( v.vTexCoord )";

/// The least distance a field holds on each side of the edge, in texels:
/// enough for the ink's anti-aliasing at the page's own size and for two mip
/// levels below it, where the glyphs' boxes still keep them apart.
const LEAST_SPREAD: u32 = 4;

/// How far from the edge a page's fields reach, in texels, on each side: past
/// the outline's reach, where its opacity is 0, or [`LEAST_SPREAD`].
pub fn spread(outline: Option<&Outline>) -> u32 {
    outline.map_or(LEAST_SPREAD, |o| o.reach() + 2).max(LEAST_SPREAD)
}

/// The field of a glyph whose coverage is `bitmap`: a bitmap larger by
/// `spread` on every side, whose alpha is 0.5 on the ink's edge, falling to 0
/// `spread` texels outside it and rising to 1 as far inside, and its pixels as
/// 0xAARRGGBB, white. The edge is found to a fraction of a texel from the
/// coverage: it runs through the pixels the ink covers in part, at a place
/// their coverage gives, and between whole ink and none where they touch.
pub fn field(bitmap: &Bitmap, spread: u32) -> (Bitmap, Vec<u32>) {
    let (width, height) = (bitmap.width + 2 * spread, bitmap.height + 2 * spread);
    let (w, h) = (width as usize, height as usize);
    let mut coverage = vec![0u8; w * h];
    for y in 0..bitmap.height as usize {
        let from = y * bitmap.width as usize;
        let to = (y + spread as usize) * w + spread as usize;
        coverage[to..to + bitmap.width as usize].copy_from_slice(&bitmap.alpha[from..from + bitmap.width as usize]);
    }
    let ink = |i: usize| coverage[i] >= 0x80;
    let edges: Vec<bool> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let beside = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
            // Whole ink beside none: the edge between them.
            let opposite = 255 - coverage[i];
            (1..=254).contains(&coverage[i]) || beside.into_iter().flatten().any(|j| coverage[j] == opposite)
        })
        .collect();
    let alpha: Vec<u8> = distance::nearest(&edges, width, height)
        .into_iter()
        .enumerate()
        .map(|(i, (d, edge))| {
            // The edge lies 0.5 less the edge pixel's coverage from its
            // center, outwards.
            let signed = if ink(i) { -d } else { d } + 0.5 - coverage[edge] as f32 / 255.0;
            ((0.5 - signed / (2 * spread) as f32).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect();
    let pixels = alpha.iter().map(|&a| (a as u32) << 24 | 0x00ff_ffff).collect();
    let spread = spread as i32;
    (Bitmap { width, height, left: bitmap.left - spread, top: bitmap.top - spread, advance: bitmap.advance, alpha }, pixels)
}

/// How the map's shader draws the names from the fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Shading {
    /// The fields' spread, in texels ([`spread`]).
    pub spread: u32,
    /// The ink's color, 0xRRGGBB.
    pub ink: u32,
    /// The outline around it, if any.
    pub outline: Option<Outline>,
}

/// The shading of the map font's page, once it holds fields.
static SHADING: Mutex<Option<Shading>> = Mutex::new(None);

/// The map font's page holds fields, to be drawn so.
pub fn shade(shading: Shading) {
    *SHADING.lock().unwrap_or_else(|e| e.into_inner()) = Some(shading);
}

/// A color, 0xRRGGBB, as an HLSL `float3`.
fn float3(color: u32) -> String {
    let channel = |shift: u32| (color >> shift & 0xff) as f32 / 255.0;
    format!("float3( {:.4}f, {:.4}f, {:.4}f )", channel(16), channel(8), channel(0))
}

/// The HLSL function that draws a name's pixel from the page's field: the ink
/// anti-aliased over one pixel of the screen, over the outline at its
/// opacity, interpolated between whole texels of distance from the ink as
/// `outline` draws it; its colors not premultiplied, as the page's were.
fn decoder(shading: &Shading) -> String {
    let mut band = String::new();
    if let Some(outline) = &shading.outline {
        // Each whole texel's opacity, spread linearly to the next.
        band.push_str("\t\t\tfloat t = max( vDistance + 0.5f, 1.0f );\n");
        for (i, &a) in outline.alpha().iter().enumerate() {
            let _ = writeln!(band, "\t\t\tvBand += {:.4}f * saturate( 1.0f - abs( t - {}.0f ) );", a as f32 / 255.0, i + 1);
        }
    }
    let color = shading.outline.as_ref().map_or(0, |o| o.color);
    format!(
        "
		// EU4 Harmony: the font page holds each glyph's distance from its ink's
		// edge, 0.5 on the edge and {spread} texels from it at 0 and 1.
		float4 Eu4HarmonyMapName( float2 vUV )
		{{
			float vDistance = ( 0.5f - tex2D( DiffuseTexture, vUV ).a ) * {range}.0f;
			float vPixel = max( length( float2( ddx( vDistance ), ddy( vDistance ) ) ), 0.0001f );
			float vInk = saturate( 0.5f - vDistance / vPixel );
			float vBand = 0.0f;
{band}			float vAlpha = vInk + vBand * ( 1.0f - vInk );
			float3 vColor = ( {ink} * vInk + {color} * ( vBand * ( 1.0f - vInk ) ) ) / max( vAlpha, 0.0001f );
			return float4( vColor, vAlpha );
		}}
",
        spread = shading.spread,
        range = 2 * shading.spread,
        ink = float3(shading.ink),
        color = float3(color),
    )
}

/// The map names' shader `text` drawing from fields: the decoder put at the
/// start of the pixel shader's code, and the page's read through it. An error
/// when the text has not the one read it changes.
fn decoded(text: &str, shading: &Shading) -> crate::Result<String> {
    let at = text.find(SAMPLE).ok_or("the pixel shader does not read the page as the game's does")?;
    if text[at + SAMPLE.len()..].contains(SAMPLE) {
        return Err("the pixel shader reads the page more than once".into());
    }
    let code = text[..at].rfind("[[").ok_or("the page is read outside a shader's code")? + 2;
    Ok(format!("{}{}{}{}", &text[..code], decoder(shading), &text[code..at], DECODED) + &text[at + SAMPLE.len()..])
}

/// Whether the map names' shader, as the game will read it, can draw from
/// fields; why not otherwise.
pub fn shader_decodes() -> crate::Result<()> {
    let text = String::from_utf8(physfs::read(SHADER)?).map_err(|_| format!("{SHADER} is not UTF-8"))?;
    decoded(&text, &Shading { spread: LEAST_SPREAD, ink: 0, outline: None }).map(|_| ())
}

/// The change to the shader's text: none until the map font's page holds
/// fields.
fn edit(text: &str) -> crate::Result<String> {
    match &*SHADING.lock().unwrap_or_else(|e| e.into_inner()) {
        Some(shading) => decoded(text, shading),
        None => Ok(text.to_string()),
    }
}

pub fn plan(plan: &mut Plan) {
    plan.add(&physfs::normalize(SHADER), Addition::Edit { what: "the map names' shader", edit });
}

pub fn points() -> Vec<Point> {
    vec![
        // The map's setup builds the name as a string, 9 bytes from 0x1d57b78,
        // and keeps the font it finds at `map+0x4d0` (0x10a36d6).
        Point::keep(0x10a36a2usize, "48 c7 44 24 50 09 00 00 00 f2 0f 10 05 c5 44 cb 00", "the map asks for its font by name"),
        Point::keep(0x1d57b78usize, "74 61 68 6f 6d 61 5f 36 30 00", "the map font's name"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's `gfx/FX/mapname.shader`, its pixel shader.
    const PIXEL_SHADER: &str = "PixelShader =
{
	MainCode PixelShader
	[[
		float4 main( VS_OUTPUT v ) : PDX_COLOR
		{
			float4 vSample = tex2D( DiffuseTexture, v.vTexCoord );
			vSample.a *= vTargetOpacity_Fade.x * vTargetOpacity_Fade.y;
			return vSample;
		}
	]]
}
";

    fn value(field: &Bitmap, x: u32, y: u32) -> u8 {
        field.alpha[(y * field.width + x) as usize]
    }

    #[test]
    fn a_field_is_half_on_the_edge() {
        // A block of ink in a 16×16 glyph, rows and columns 2 to 13, its
        // right column covered by half.
        let mut alpha = vec![0u8; 256];
        for y in 2..14 {
            for x in 2..14 {
                alpha[y * 16 + x] = if x == 13 { 128 } else { 255 };
            }
        }
        let bitmap = Bitmap { width: 16, height: 16, left: 1, top: -15, advance: 17.0, alpha };
        let (field, pixels) = field(&bitmap, 4);
        assert_eq!((field.width, field.height, field.left, field.top, field.advance), (24, 24, -3, -19, 17.0));
        // Spread 4: a texel of distance is 1/8 of the range, about 32.
        let at = |x: u32| value(&field, 4 + x, 4 + 8);
        // The edge through the middle of the half-covered column.
        assert_eq!(at(13), 128);
        assert!(at(14).abs_diff(96) <= 1, "{}", at(14));
        // On the left, the edge between whole ink and none, half a texel
        // from either.
        assert!(at(1).abs_diff(112) <= 1, "{}", at(1));
        assert!(at(2).abs_diff(143) <= 1, "{}", at(2));
        // Five texels in and more, past the spread: 1.
        assert_eq!(at(8), 255);
        // Far outside: 0.
        assert_eq!(value(&field, 0, 0), 0);
        assert_eq!(pixels[0], 0x00ff_ffff);
        assert_eq!(pixels[(12 * 24 + 12) as usize], 0xffff_ffff);
    }

    #[test]
    fn the_spread_reaches_past_the_outline() {
        assert_eq!(spread(None), 4);
        let outline = Outline::measure(&[0xff00_0000, 0x4080_8080, 0x2080_8080, 0x1080_8080, 0x0880_8080], 5, 1, 0);
        assert_eq!(outline.as_ref().map(Outline::reach), Some(4));
        assert_eq!(spread(outline.as_ref()), 6);
    }

    #[test]
    fn the_shader_reads_the_page_through_the_decoder() {
        let shading = Shading { spread: 8, ink: 0x100c10, outline: None };
        let text = decoded(PIXEL_SHADER, &shading).unwrap();
        assert!(text.contains("float4 vSample = Eu4HarmonyMapName( v.vTexCoord );"), "{text}");
        assert!(!text.contains(SAMPLE));
        // The decoder before `main`, inside the shader's code.
        let (code, decoder, main) = (text.find("[[").unwrap(), text.find("float4 Eu4HarmonyMapName").unwrap(), text.find("float4 main").unwrap());
        assert!(code < decoder && decoder < main);
        assert!(text.contains("* 16.0f;") && text.contains("float3( 0.0627f, 0.0471f, 0.0627f )"));
        assert!(!text.contains("vBand +="));
        // A shader that reads the page otherwise, or twice, is left alone.
        assert!(decoded(&PIXEL_SHADER.replace(SAMPLE, "tex2D( DiffuseTexture, vUV )"), &shading).is_err());
        assert!(decoded(&format!("{PIXEL_SHADER}{PIXEL_SHADER}"), &shading).is_err());
    }

    #[test]
    fn the_outline_falls_off_as_its_page_draws_it() {
        let outline = Outline::measure(&[0xff10_0c10, 0xbe83_8283, 0x8d83_8283, 0x0883_8283], 4, 1, 0x10_0c10).unwrap();
        let text = decoder(&Shading { spread: spread(Some(&outline)), ink: 0x10_0c10, outline: Some(outline) });
        assert!(text.contains("vBand += 0.7451f * saturate( 1.0f - abs( t - 1.0f ) );"), "{text}");
        assert!(text.contains("vBand += 0.5529f * saturate( 1.0f - abs( t - 2.0f ) );"), "{text}");
        assert!(text.contains("float3( 0.5137f, 0.5098f, 0.5137f )"), "{text}");
    }
}
