//! Places glyph bitmaps on a texture page, as an atlas: rows ("shelves")
//! as tall as the tallest glyph in them, filled left to right, each glyph
//! padded by one pixel so filtering never bleeds a neighbour in. A page may
//! grow: its glyphs keep their places, its shelves reach across the new
//! width and new ones open below.

/// Space between glyphs, in pixels.
const PAD: u32 = 1;

struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

pub struct Page {
    pub width: u32,
    pub height: u32,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl Page {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, shelves: Vec::new(), next_y: 0 }
    }

    /// Finds room for a `width` × `height` bitmap; None when the page is full.
    pub fn place(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (w, h) = (width + PAD, height + PAD);
        if w > self.width || h > self.height {
            return None;
        }
        // The lowest shelf it fits on without wasting more than half its height.
        let fits = self.shelves.iter_mut().filter(|s| s.height >= h && s.height <= h * 2 && s.next_x + w <= self.width).min_by_key(|s| s.height);
        if let Some(shelf) = fits {
            let at = (shelf.next_x, shelf.y);
            shelf.next_x += w;
            return Some(at);
        }
        if self.next_y + h > self.height {
            return None;
        }
        let y = self.next_y;
        self.next_y += h;
        self.shelves.push(Shelf { y, height: h, next_x: w });
        Some((0, y))
    }

    /// How much of the page's height is in use.
    pub fn used_height(&self) -> u32 {
        self.next_y
    }

    /// Makes the page `width` × `height`, never smaller than it is.
    pub fn grow(&mut self, width: u32, height: u32) {
        self.width = self.width.max(width);
        self.height = self.height.max(height);
    }

    /// How much of the page glyphs have claimed, 0 to 1: each shelf as far
    /// as it is filled. The rest is free, though the end of a shelf takes
    /// only glyphs about as tall as it.
    pub fn claimed(&self) -> f32 {
        let claimed: u64 = self.shelves.iter().map(|s| s.height as u64 * s.next_x as u64).sum();
        claimed as f32 / (self.width as u64 * self.height as u64).max(1) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    fn fills_shelves_left_to_right_then_opens_new_ones() {
        let mut page = Page::new(64, 32);
        assert_eq!(page.place(15, 15), Some((0, 0)));
        assert_eq!(page.place(15, 15), Some((16, 0)));
        assert_eq!(page.place(15, 12), Some((32, 0)));
        assert_eq!(page.place(15, 15), Some((48, 0)));
        // The first shelf is full; a new one starts below it.
        assert_eq!(page.place(15, 15), Some((0, 16)));
        assert_eq!(page.used_height(), 32);
        // A glyph far shorter than every shelf, or one too big, needs room the page lacks.
        assert_eq!(page.place(64, 1), None);
        assert_eq!(page.place(5, 5), None);
    }

    #[test]
    fn a_grown_page_keeps_its_glyphs_and_fills_the_room_it_gained() {
        let mut page = Page::new(32, 16);
        assert_eq!(page.place(15, 15), Some((0, 0)));
        assert_eq!(page.place(15, 15), Some((16, 0)));
        assert_eq!(page.claimed(), 1.0);
        assert_eq!(page.place(15, 15), None);
        // Taller: a new shelf below.
        page.grow(32, 32);
        assert_eq!(page.claimed(), 0.5);
        assert_eq!(page.place(15, 15), Some((0, 16)));
        // Wider: the shelves reach across.
        page.grow(64, 32);
        assert_eq!(page.place(15, 15), Some((32, 0)));
        assert_eq!(page.place(15, 15), Some((48, 0)));
        assert_eq!(page.claimed(), 0.625);
        // Never smaller.
        page.grow(16, 16);
        assert_eq!((page.width, page.height), (64, 32));
    }
}
