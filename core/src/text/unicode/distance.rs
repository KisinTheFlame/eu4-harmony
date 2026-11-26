//! Exact Euclidean distances on a pixel grid, from each pixel's center to
//! the nearest of some pixels' (Felzenszwalb and Huttenlocher's distance
//! transform): squared distances across each row, then down each column.
//! The outline around a map font's glyphs is drawn by them (`outline`), and
//! the map's distance fields are made from them (`map_field`).

/// Far from everything.
pub const FAR: f32 = 1e20;

/// The distance from each pixel's center to the nearest pixel of `seeds`
/// (`width` to a row); 0 on a seed, [`FAR`] or more with no seed at all.
pub fn distances(seeds: &[bool], width: u32, height: u32) -> Vec<f32> {
    nearest(seeds, width, height).into_iter().map(|(d, _)| d).collect()
}

/// For each pixel, the distance to the nearest pixel of `seeds` and that
/// pixel's index (`width` to a row); [`FAR`] or more, and any index, with no
/// seed at all.
pub fn nearest(seeds: &[bool], width: u32, height: u32) -> Vec<(f32, usize)> {
    let (width, height) = (width as usize, height as usize);
    // Across each row: the squared distance to the nearest seed in the row,
    // and its column.
    let mut squared: Vec<f32> = seeds.iter().map(|&s| if s { 0.0 } else { FAR }).collect();
    let mut column = vec![0usize; width * height];
    let mut line = Vec::new();
    for y in 0..height {
        let row = y * width..(y + 1) * width;
        let (d, at) = along(&squared[row.clone()]);
        squared[row.clone()].copy_from_slice(&d);
        column[row].copy_from_slice(&at);
    }
    // Down each column: the row whose nearest seed is nearest.
    let mut found = vec![(FAR, 0); width * height];
    for x in 0..width {
        line.clear();
        line.extend((0..height).map(|y| squared[y * width + x]));
        let (d, rows) = along(&line);
        for y in 0..height {
            let row = rows[y];
            found[y * width + x] = (d[y].sqrt(), row * width + column[row * width + x]);
        }
    }
    found
}

/// For each place on a line, the least of `f[q] + (p - q)²` over every `q`,
/// and that `q`: the lower envelope of the parabolas rooted at each place.
fn along(f: &[f32]) -> (Vec<f32>, Vec<usize>) {
    let n = f.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
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
            ((p as f32 - r as f32).powi(2) + f[r], r)
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_euclidean() {
        // One seed in the middle of a 7×5 grid.
        let mut seeds = vec![false; 35];
        seeds[2 * 7 + 3] = true;
        let d = distances(&seeds, 7, 5);
        assert_eq!(d[2 * 7 + 3], 0.0);
        assert_eq!(d[2 * 7 + 6], 3.0);
        assert!((d[0] - 13f32.sqrt()).abs() < 1e-5, "{}", d[0]);
        assert!((d[4 * 7 + 5] - 8f32.sqrt()).abs() < 1e-5);
        // No seed at all: everything far.
        assert!(distances(&[false; 6], 3, 2).iter().all(|&d| d >= 1e9));
    }

    #[test]
    fn finds_which_seed_is_nearest() {
        // Seeds at (1, 1) and (5, 3) of a 7×5 grid.
        let mut seeds = vec![false; 35];
        seeds[7 + 1] = true;
        seeds[3 * 7 + 5] = true;
        let found = nearest(&seeds, 7, 5);
        assert_eq!(found[0], (2f32.sqrt(), 7 + 1));
        assert_eq!(found[4 * 7 + 6], (2f32.sqrt(), 3 * 7 + 5));
        assert_eq!(found[3 * 7 + 5], (0.0, 3 * 7 + 5));
        assert_eq!(found[2 * 7 + 1].1, 7 + 1);
        assert_eq!(found[3 * 7 + 4].1, 3 * 7 + 5);
    }
}
