//! DXT5 (BC3) blocks, as a compressed font page holds them (the Chinese
//! language mod's): 16 bytes for 4×4 pixels, alpha first (two 8-bit ends and
//! a 3-bit index per pixel), then color (two RGB565 ends and a 2-bit index
//! per pixel).

pub const BLOCK: usize = 16;

/// The eight alphas a block with ends `a0 > a1` can give.
fn alpha_levels(a0: u8, a1: u8) -> [u8; 8] {
    let (a0, a1) = (a0 as u32, a1 as u32);
    let mut levels = [a0 as u8, a1 as u8, 0, 0, 0, 0, 0, 0];
    for (k, level) in levels.iter_mut().enumerate().skip(2) {
        *level = (((8 - k as u32) * a0 + (k as u32 - 1) * a1) / 7) as u8;
    }
    levels
}

fn rgb888(c: u16) -> [u32; 3] {
    let (r, g, b) = ((c >> 11) as u32 & 0x1f, (c >> 5) as u32 & 0x3f, c as u32 & 0x1f);
    [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]
}

/// The four colors a block with these ends gives; DXT5 always uses the
/// four-color mode.
fn palette(c0: u16, c1: u16) -> [[u32; 3]; 4] {
    let (e0, e1) = (rgb888(c0), rgb888(c1));
    [e0, e1, [0, 1, 2].map(|i| (2 * e0[i] + e1[i]) / 3), [0, 1, 2].map(|i| (e0[i] + 2 * e1[i]) / 3)]
}

/// The 16 pixels of a block as 0xAARRGGBB, row by row.
pub fn decode(block: &[u8; BLOCK]) -> [u32; 16] {
    let (a0, a1) = (block[0], block[1]);
    let alphas: [u8; 8] = if a0 > a1 {
        alpha_levels(a0, a1)
    } else {
        let (x, y) = (a0 as u32, a1 as u32);
        let mid = |k: u32| (((5 - k) * x + k * y) / 5) as u8;
        [a0, a1, mid(1), mid(2), mid(3), mid(4), 0, 255]
    };
    let mut bits = [0u8; 8];
    bits[..6].copy_from_slice(&block[2..8]);
    let alpha_bits = u64::from_le_bytes(bits);
    let (c0, c1) = (u16::from_le_bytes([block[8], block[9]]), u16::from_le_bytes([block[10], block[11]]));
    let colors = palette(c0, c1);
    let color_bits = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
    let mut out = [0u32; 16];
    for (i, px) in out.iter_mut().enumerate() {
        let a = alphas[(alpha_bits >> (3 * i) & 7) as usize] as u32;
        let [r, g, b] = colors[(color_bits >> (2 * i) & 3) as usize];
        *px = a << 24 | r << 16 | g << 8 | b;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block of these alpha and color ends and each pixel's alpha and
    /// color codes.
    fn block(alpha_ends: [u8; 2], alpha_codes: [u64; 16], color_ends: [u16; 2], color_codes: [u32; 16]) -> [u8; BLOCK] {
        let mut block = [0u8; BLOCK];
        block[..2].copy_from_slice(&alpha_ends);
        let alpha = alpha_codes.iter().enumerate().fold(0u64, |bits, (i, &code)| bits | code << (3 * i));
        block[2..8].copy_from_slice(&alpha.to_le_bytes()[..6]);
        block[8..10].copy_from_slice(&color_ends[0].to_le_bytes());
        block[10..12].copy_from_slice(&color_ends[1].to_le_bytes());
        let color = color_codes.iter().enumerate().fold(0u32, |bits, (i, &code)| bits | code << (2 * i));
        block[12..].copy_from_slice(&color.to_le_bytes());
        block
    }

    #[test]
    fn eight_alphas_go_from_the_higher_end_to_the_lower() {
        let pixels = decode(&block([255, 0], std::array::from_fn(|i| i as u64 % 8), [0xffff, 0], [0; 16]));
        assert_eq!(pixels[..8].iter().map(|px| px >> 24).collect::<Vec<_>>(), [255, 0, 218, 182, 145, 109, 72, 36]);
        assert!(pixels.iter().all(|px| px & 0x00ff_ffff == 0x00ff_ffff));
    }

    #[test]
    fn six_alphas_go_from_the_lower_end_to_the_higher_with_clear_and_opaque() {
        let alphas = decode(&block([0, 100], std::array::from_fn(|i| i as u64 % 8), [0, 0], [0; 16])).map(|px| px >> 24);
        assert_eq!(alphas[..8], [0, 100, 20, 40, 60, 80, 0, 255]);
    }

    #[test]
    fn four_colors_go_between_the_ends() {
        // A third and two thirds of the way from black to white.
        let colors = decode(&block([255, 255], [0; 16], [0, 0xffff], std::array::from_fn(|i| i as u32 % 4))).map(|px| px & 0x00ff_ffff);
        assert_eq!(colors[..4], [0, 0x00ff_ffff, 0x0055_5555, 0x00aa_aaaa]);
    }

    #[test]
    fn an_all_zero_block_is_clear() {
        assert!(decode(&[0; BLOCK]).iter().all(|px| px >> 24 == 0));
    }
}
