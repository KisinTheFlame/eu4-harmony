//! DXT5 (BC3) blocks, for glyphs drawn into a compressed font page: 16 bytes
//! for 4×4 pixels, alpha first (two 8-bit ends and a 3-bit index per pixel),
//! then colour (two RGB565 ends and a 2-bit index per pixel).

pub const BLOCK: usize = 16;

/// A block of `ink` (0x00RRGGBB) with these 16 alphas, row by row. The alphas
/// go between the block's lowest and highest in eight steps.
pub fn encode(alpha: &[u8; 16], ink: u32) -> [u8; BLOCK] {
    let (low, high) = (*alpha.iter().min().unwrap(), *alpha.iter().max().unwrap());
    let mut block = [0u8; BLOCK];
    block[0] = high;
    block[1] = low;
    if high > low {
        let levels = alpha_levels(high, low);
        let mut bits = 0u64;
        for (i, &a) in alpha.iter().enumerate() {
            let code = (0..8).min_by_key(|&c| (levels[c] as i32 - a as i32).abs()).unwrap();
            bits |= (code as u64) << (3 * i);
        }
        block[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
    }
    let colour = rgb565(ink).to_le_bytes();
    block[8..10].copy_from_slice(&colour);
    block[10..12].copy_from_slice(&colour);
    block
}

/// The eight alphas a block with ends `a0 > a1` can give.
fn alpha_levels(a0: u8, a1: u8) -> [u8; 8] {
    let (a0, a1) = (a0 as u32, a1 as u32);
    let mut levels = [a0 as u8, a1 as u8, 0, 0, 0, 0, 0, 0];
    for (k, level) in levels.iter_mut().enumerate().skip(2) {
        *level = (((8 - k as u32) * a0 + (k as u32 - 1) * a1) / 7) as u8;
    }
    levels
}

fn rgb565(rgb: u32) -> u16 {
    let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
    ((r >> 3) << 11 | (g >> 2) << 5 | b >> 3) as u16
}

fn rgb888(c: u16) -> [u32; 3] {
    let (r, g, b) = ((c >> 11) as u32 & 0x1f, (c >> 5) as u32 & 0x3f, c as u32 & 0x1f);
    [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]
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
    let (e0, e1) = (rgb888(c0), rgb888(c1));
    // DXT5 always uses the four-colour mode.
    let colours: [[u32; 3]; 4] = [e0, e1, [0, 1, 2].map(|i| (2 * e0[i] + e1[i]) / 3), [0, 1, 2].map(|i| (e0[i] + 2 * e1[i]) / 3)];
    let colour_bits = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
    let mut out = [0u32; 16];
    for (i, px) in out.iter_mut().enumerate() {
        let a = alphas[(alpha_bits >> (3 * i) & 7) as usize] as u32;
        let [r, g, b] = colours[(colour_bits >> (2 * i) & 3) as usize];
        *px = a << 24 | r << 16 | g << 8 | b;
    }
    out
}

/// A `width`×`height` coverage bitmap as blocks of `ink`, padded with clear
/// pixels to whole blocks; row by row, `width.div_ceil(4)` blocks to a row.
pub fn encode_bitmap(width: u32, height: u32, alpha: &[u8], ink: u32) -> Vec<[u8; BLOCK]> {
    let (across, down) = (width.div_ceil(4), height.div_ceil(4));
    let mut blocks = Vec::with_capacity((across * down) as usize);
    for by in 0..down {
        for bx in 0..across {
            let mut cell = [0u8; 16];
            for (i, a) in cell.iter_mut().enumerate() {
                let (x, y) = (bx * 4 + i as u32 % 4, by * 4 + i as u32 / 4);
                if x < width && y < height {
                    *a = alpha[(y * width + x) as usize];
                }
            }
            blocks.push(encode(&cell, ink));
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_keeps_its_alphas_within_a_step_and_its_ink() {
        let alpha: [u8; 16] = [0, 17, 34, 51, 68, 85, 102, 119, 136, 153, 170, 187, 204, 221, 238, 255];
        let pixels = decode(&encode(&alpha, 0x00ff_ffff));
        for (px, &a) in pixels.iter().zip(&alpha) {
            assert!(((px >> 24) as i32 - a as i32).abs() <= 255 / 14 + 1, "{a} came back as {}", px >> 24);
            assert_eq!(px & 0x00ff_ffff, 0x00ff_ffff);
        }
        let black = decode(&encode(&alpha, 0));
        assert!(black.iter().all(|px| px & 0x00ff_ffff == 0));
    }

    #[test]
    fn flat_and_clear_blocks_stay_exact() {
        assert!(decode(&encode(&[0; 16], 0x00ff_ffff)).iter().all(|px| px >> 24 == 0));
        assert!(decode(&encode(&[200; 16], 0x00ff_ffff)).iter().all(|px| px >> 24 == 200));
        // An all-zero block is clear.
        assert!(decode(&[0; BLOCK]).iter().all(|px| px >> 24 == 0));
    }

    #[test]
    fn a_bitmap_is_padded_to_whole_blocks() {
        // 5×2: two blocks across, one down; the padding is clear.
        let alpha = [255u8; 10];
        let blocks = encode_bitmap(5, 2, &alpha, 0x00ff_ffff);
        assert_eq!(blocks.len(), 2);
        let first = decode(&blocks[0]);
        assert!(first[..8].iter().all(|px| px >> 24 == 255));
        assert!(first[8..].iter().all(|px| px >> 24 == 0));
        let second = decode(&blocks[1]);
        assert_eq!([second[0] >> 24, second[1] >> 24, second[4] >> 24, second[5] >> 24], [255, 0, 255, 0]);
    }
}
