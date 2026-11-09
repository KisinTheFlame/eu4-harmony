//! The pages filled fonts draw from, as the framework keeps them (ADR
//! font-pages-grow): which glyphs a page holds and where, which wait for
//! room, when it grows and what follows, and which fonts draw from it. A
//! glyph is drawn the first time it is asked for; one that finds no room
//! waits, asked for again only once the page has grown, which it does at the
//! next frame the main thread presents ([`present`]), or at once while a
//! font loads. A page grown in a map font leaves the names on the map to be
//! built again, at the first frame of a game.
//!
//! A page reaches its texture through [`Texture`] and the game's objects
//! through [`Game`]: `fill` gives them Direct3D and the game's fonts, and the
//! tests a texture in memory, so a page's whole course is tested here.
use super::atlas;
use super::costs;
use super::glyphs::Glyph;
use super::tone::Tone;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

/// How much of a page may be claimed before it grows: a page that grows
/// before it is full leaves no character without a glyph until then.
const FULL_ENOUGH: f32 = 0.75;

/// The texture a page's glyphs are written into.
pub trait Texture: Sized {
    /// Writes `width` × `height` pixels at (x, y), given as rows of A8R8G8B8
    /// pixels.
    fn put(&mut self, x: u32, y: u32, width: u32, height: u32, data: &[u8]) -> crate::Result<()>;
    /// A texture of `size` (width, height) holding this one's pixels of
    /// `drawn`, its top left, where they are.
    fn grown(&self, drawn: (u32, u32), size: (u32, u32)) -> crate::Result<Self>;
    /// Makes its mip levels again from its first, when it has any: glyphs
    /// were written since they were last made.
    fn make_mips(&self);
}

/// What a page's growth changes in the game.
pub trait Game<T> {
    /// The game's page at `wrapper`, and `fonts`, which draw from it, take
    /// `texture`, of `size`, and lay their text out again.
    fn take(&mut self, wrapper: usize, fonts: &[usize], texture: &T, size: (u32, u32)) -> crate::Result<()>;
    /// Builds the names on the map again from the map fonts' pages as they
    /// are; false, building nothing, outside a game.
    fn build_map_names(&mut self) -> crate::Result<bool>;
}

/// A glyph drawn for a page: the game's record of it, its place left to the
/// page, and its pixels, rows of A8R8G8B8 as wide as the record.
pub struct Drawn {
    pub glyph: Glyph,
    pub pixels: Vec<u8>,
}

/// The page had no room for a glyph.
#[derive(Debug, PartialEq, Eq)]
pub struct NoRoom;

/// A page as it is made.
pub struct Made<T> {
    pub texture: T,
    /// Its size, (width, height).
    pub size: (u32, u32),
    /// The largest it may grow to; None when it keeps its size.
    pub max: Option<(u32, u32)>,
    /// The first font that drew from it, for the log.
    pub name: String,
    /// Its fonts are the map's, which the names on the map are built from.
    pub map: bool,
}

struct Page<T> {
    texture: T,
    /// Its glyphs' places, and its size.
    atlas: atlas::Page,
    max: Option<(u32, u32)>,
    name: String,
    map: bool,
    /// The page is to grow at the next present.
    wants_room: bool,
    /// Glyphs were written since its mip levels were last made.
    mips_stale: bool,
    drawn: HashMap<(u32, Tone), Option<Glyph>>,
    /// Glyphs that found no room, drawn once the page has grown. While a
    /// game loads, the main thread presents no frame for long: the names on
    /// the map ask for thousands meanwhile, each asked for many times.
    waiting: HashSet<(u32, Tone)>,
    /// It has logged that it is full.
    full: bool,
}

impl<T: Texture> Page<T> {
    fn size(&self) -> (u32, u32) {
        (self.atlas.width, self.atlas.height)
    }

    /// The size the page grows to next; None when it keeps the one it has.
    fn next_size(&self) -> Option<(u32, u32)> {
        grown(self.size(), self.max?)
    }

    /// Asks for the page to grow at the next present, when it can.
    fn want_room(&mut self) {
        if self.next_size().is_some() {
            self.wants_room = true;
        }
    }

    /// The glyph `key` names, drawn by `draw` the first time it is asked
    /// for; one without room is asked for again once the page has grown.
    fn glyph(&mut self, key: (u32, Tone), draw: impl FnOnce() -> Option<Drawn>) -> Result<Option<Glyph>, NoRoom> {
        if let Some(found) = self.drawn.get(&key) {
            return Ok(*found);
        }
        if self.waiting.contains(&key) {
            return Err(NoRoom);
        }
        let glyph = costs::GLYPHS.time(|| self.draw(draw)).inspect_err(|_| {
            self.waiting.insert(key);
        })?;
        self.drawn.insert(key, glyph);
        Ok(glyph)
    }

    /// Draws a glyph into the page; None when `draw` has none.
    fn draw(&mut self, draw: impl FnOnce() -> Option<Drawn>) -> Result<Option<Glyph>, NoRoom> {
        let Some(Drawn { mut glyph, pixels }) = draw() else { return Ok(None) };
        let (width, height) = (glyph.width as u32, glyph.height as u32);
        if width > 0 && height > 0 {
            let Some((x, y)) = self.atlas.place(width, height) else {
                self.want_room();
                if !self.wants_room && !std::mem::replace(&mut self.full, true) {
                    let (width, height) = self.size();
                    log!("fill: the {width}x{height} page of font {:?} is full; characters past it show as '?'", self.name);
                }
                return Err(NoRoom);
            };
            if self.atlas.claimed() > FULL_ENOUGH {
                self.want_room();
            }
            if let Err(e) = self.texture.put(x, y, width, height, &pixels) {
                log!("fill: cannot write a glyph: {e}");
                return Ok(None);
            }
            self.mips_stale = true;
            (glyph.x, glyph.y) = (x as u16, y as u16);
        }
        Ok(Some(glyph))
    }
}

/// The size a page of `size` (width, height) grows to: twice as tall, or
/// twice as wide once it is taller than wide, up to `max`. None when it is
/// as large as it may be.
pub fn grown((width, height): (u32, u32), max: (u32, u32)) -> Option<(u32, u32)> {
    let taller = (width, (height * 2).min(max.1));
    let wider = ((width * 2).min(max.0), height);
    let order = if height <= width { [taller, wider] } else { [wider, taller] };
    order.into_iter().find(|&size| size != (width, height))
}

/// Every filled page, by the game's page wrapper, with `L`, what its glyphs
/// are drawn with; and the page each filled font draws from, by font.
pub struct Pages<T, L> {
    pages: HashMap<usize, (Page<T>, L)>,
    /// The page wrapper each font draws from, and in which tone.
    owners: HashMap<usize, (usize, Tone)>,
    /// A map font's page grew since the names on the map were last built.
    map_names_stale: bool,
}

impl<T, L> Default for Pages<T, L> {
    fn default() -> Self {
        Self { pages: HashMap::new(), owners: HashMap::new(), map_names_stale: false }
    }
}

impl<T: Texture, L> Pages<T, L> {
    /// The page the game keeps at `wrapper` is `made`, its glyphs drawn with
    /// `with`.
    pub fn add(&mut self, wrapper: usize, made: Made<T>, with: L) {
        let Made { texture, size: (width, height), max, name, map } = made;
        let page = Page {
            texture,
            atlas: atlas::Page::new(width, height),
            max,
            name,
            map,
            wants_room: false,
            mips_stale: false,
            drawn: HashMap::new(),
            waiting: HashSet::new(),
            full: false,
        };
        self.pages.insert(wrapper, (page, with));
    }

    pub fn has(&self, wrapper: usize) -> bool {
        self.pages.contains_key(&wrapper)
    }

    /// What the page at `wrapper` draws its glyphs with.
    pub fn with(&self, wrapper: usize) -> Option<&L> {
        self.pages.get(&wrapper).map(|(_, with)| with)
    }

    pub fn with_mut(&mut self, wrapper: usize) -> Option<&mut L> {
        self.pages.get_mut(&wrapper).map(|(_, with)| with)
    }

    pub fn texture(&self, wrapper: usize) -> Option<&T> {
        self.pages.get(&wrapper).map(|(page, _)| &page.texture)
    }

    /// The page's size, (width, height).
    pub fn size(&self, wrapper: usize) -> Option<(u32, u32)> {
        self.pages.get(&wrapper).map(|(page, _)| page.size())
    }

    /// `font` draws from the page at `wrapper`, in `tone`.
    pub fn draws_from(&mut self, font: usize, wrapper: usize, tone: Tone) {
        self.owners.insert(font, (wrapper, tone));
    }

    /// The page wrapper `font` draws from, and in which tone.
    pub fn owner(&self, font: usize) -> Option<(usize, Tone)> {
        self.owners.get(&font).copied()
    }

    /// The font is destroyed.
    pub fn forget(&mut self, font: usize) {
        self.owners.remove(&font);
    }

    /// The game's page at `wrapper` is destroyed, and with it the fonts'
    /// hold on it.
    pub fn destroyed(&mut self, wrapper: usize) {
        if self.pages.remove(&wrapper).is_some() {
            self.owners.retain(|_, (w, _)| *w != wrapper);
        }
    }

    /// The glyph `font` draws for `scalar`, drawn into its page with `draw`
    /// the first time; None for a font not filled, a character `draw` has no
    /// glyph for, or one waiting for room.
    pub fn glyph(&mut self, font: usize, scalar: u32, draw: impl FnOnce(&L, u32, Tone) -> Option<Drawn>) -> Option<Glyph> {
        let (wrapper, tone) = self.owner(font)?;
        self.glyph_in(wrapper, scalar, tone, draw).ok().flatten()
    }

    /// The glyph the page at `wrapper` holds for `scalar` in `tone`, drawn
    /// with `draw` the first time.
    pub fn glyph_in(&mut self, wrapper: usize, scalar: u32, tone: Tone, draw: impl FnOnce(&L, u32, Tone) -> Option<Drawn>) -> Result<Option<Glyph>, NoRoom> {
        let Some((page, with)) = self.pages.get_mut(&wrapper) else { return Ok(None) };
        page.glyph((scalar, tone), || draw(with, scalar, tone))
    }

    /// Gives the page at `wrapper` its next size now, when it has one: a
    /// texture with its glyphs where they were, which the game takes
    /// through `game`; and its waiting glyphs are asked for again.
    pub fn grow(&mut self, wrapper: usize, game: &mut impl Game<T>) -> crate::Result<()> {
        let fonts: Vec<usize> = self.owners.iter().filter(|(_, (w, _))| *w == wrapper).map(|(&font, _)| font).collect();
        let Some((page, _)) = self.pages.get_mut(&wrapper) else { return Ok(()) };
        page.wants_room = false;
        let Some(size) = page.next_size() else { return Ok(()) };
        let from = page.size();
        if fonts.is_empty() {
            return Err("no font draws from the page".into());
        }
        let growing = crate::heap::Change::begin();
        let texture = page.texture.grown(from, size)?;
        game.take(wrapper, &fonts, &texture, size)?;
        page.texture = texture;
        page.mips_stale = true;
        page.atlas.grow(size.0, size.1);
        page.waiting.clear();
        if page.map {
            self.map_names_stale = true;
        }
        let ((old_width, old_height), (width, height)) = (from, size);
        log!("fill: the page of font {:?} grew from {old_width}x{old_height} to {width}x{height}, for {} fonts: {}", page.name, fonts.len(), growing.end());
        Ok(())
    }

    /// Every page asks for room (the test switch `grow-pages`).
    pub fn want_room_everywhere(&mut self) {
        self.pages.values_mut().for_each(|(page, _)| page.want_room());
    }

    /// Pages that asked for room grow, a page that cannot keeping its size
    /// from then on, and those written since make their mip levels; returns
    /// whether the names on the map are to be built again.
    fn presented(&mut self, game: &mut impl Game<T>) -> bool {
        let growing: Vec<usize> = self.pages.iter().filter(|(_, (page, _))| page.wants_room).map(|(&wrapper, _)| wrapper).collect();
        for wrapper in growing {
            if let Err(e) = costs::GROWN.time(|| self.grow(wrapper, game)) {
                let (page, _) = self.pages.get_mut(&wrapper).expect("growing");
                log!("fill: the page of font {:?} keeps its size: {e}", page.name);
                page.max = None;
            }
        }
        for (page, _) in self.pages.values_mut().filter(|(page, _)| page.mips_stale) {
            page.mips_stale = false;
            page.texture.make_mips();
        }
        self.map_names_stale
    }
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the pages do at the frame the main thread presents: just before it,
/// where the main thread lays out no text, and no text the framework drew a
/// glyph for is kept with only some of its glyphs on a page's new size. With
/// the pages free, since building the names on the map draws their glyphs,
/// the names are built again, once a game is going on: outside a game the
/// map shows no names, but a game started from the main menu draws those it
/// built there (tested: a page grown at the main menu left a new game's
/// country names out).
pub fn present<T: Texture, L>(pages: &Mutex<Pages<T, L>>, game: &mut impl Game<T>) {
    if !lock(pages).presented(game) {
        return;
    }
    let built = match game.build_map_names() {
        Ok(built) => built,
        Err(e) => {
            log!("fill: the names on the map were not built again: {e}");
            true
        }
    };
    if built {
        lock(pages).map_names_stale = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{grown, present, Drawn, Game, Made, NoRoom, Pages, Texture};
    use crate::unicode::glyphs::Glyph;
    use crate::unicode::tone::Tone;
    use std::cell::Cell;
    use std::sync::Mutex;

    /// A page's texture in memory: one byte a pixel, each glyph's first.
    struct Memory {
        width: u32,
        pixels: Vec<u8>,
        mips: Cell<u32>,
    }

    impl Memory {
        fn new((width, height): (u32, u32)) -> Self {
            Self { width, pixels: vec![0; (width * height) as usize], mips: Cell::new(0) }
        }

        fn at(&self, x: u32, y: u32) -> u8 {
            self.pixels[(y * self.width + x) as usize]
        }
    }

    impl Texture for Memory {
        fn put(&mut self, x: u32, y: u32, width: u32, height: u32, data: &[u8]) -> crate::Result<()> {
            for row in 0..height {
                for column in 0..width {
                    self.pixels[((y + row) * self.width + x + column) as usize] = data[((row * width + column) * 4) as usize];
                }
            }
            Ok(())
        }

        fn grown(&self, (width, height): (u32, u32), size: (u32, u32)) -> crate::Result<Self> {
            let mut next = Memory::new(size);
            for y in 0..height {
                for x in 0..width {
                    next.pixels[(y * size.0 + x) as usize] = self.at(x, y);
                }
            }
            Ok(next)
        }

        fn make_mips(&self) {
            self.mips.set(self.mips.get() + 1);
        }
    }

    /// The game's side: what took a grown page, and when the names on the
    /// map were built.
    #[derive(Default)]
    struct World {
        in_game: bool,
        taken: Vec<(usize, Vec<usize>, (u32, u32))>,
        refuse: bool,
        names_built: u32,
    }

    impl Game<Memory> for World {
        fn take(&mut self, wrapper: usize, fonts: &[usize], _: &Memory, size: (u32, u32)) -> crate::Result<()> {
            if self.refuse {
                return Err("the font's page is not in its entry".into());
            }
            let mut fonts = fonts.to_vec();
            fonts.sort();
            self.taken.push((wrapper, fonts, size));
            Ok(())
        }

        fn build_map_names(&mut self) -> crate::Result<bool> {
            if self.in_game {
                self.names_built += 1;
            }
            Ok(self.in_game)
        }
    }

    const WRAPPER: usize = 0x1000;
    const FONT: usize = 0x2000;

    /// Pages with one, of `size`, growing up to `max`, which `FONT` draws
    /// from in light text.
    fn pages(size: (u32, u32), max: Option<(u32, u32)>, map: bool) -> Mutex<Pages<Memory, ()>> {
        let mut pages = Pages::default();
        pages.add(WRAPPER, Made { texture: Memory::new(size), size, max, name: "vic_18".into(), map }, ());
        pages.draws_from(FONT, WRAPPER, Tone::Light);
        Mutex::new(pages)
    }

    /// A glyph 15 pixels square, its pixels the character's byte, drawn as
    /// `draws` counts.
    fn square<'a>(draws: &'a Cell<u32>) -> impl Fn(&(), u32, Tone) -> Option<Drawn> + 'a {
        move |_, scalar, _| {
            draws.set(draws.get() + 1);
            Some(Drawn { glyph: Glyph { width: 15, height: 15, advance: 16, ..Glyph::default() }, pixels: vec![scalar as u8; 15 * 15 * 4] })
        }
    }

    fn glyph(pages: &Mutex<Pages<Memory, ()>>, scalar: u32, draws: &Cell<u32>) -> Option<Glyph> {
        pages.lock().unwrap().glyph(FONT, scalar, square(draws))
    }

    #[test]
    fn a_glyph_is_drawn_once_and_written_where_its_record_says() {
        let pages = pages((64, 64), None, false);
        let draws = Cell::new(0);
        let a = glyph(&pages, 'a' as u32, &draws).unwrap();
        let b = glyph(&pages, 'b' as u32, &draws).unwrap();
        assert_eq!(glyph(&pages, 'a' as u32, &draws), Some(a));
        assert_eq!(draws.get(), 2);
        assert_eq!(((a.x, a.y), (b.x, b.y)), ((0, 0), (16, 0)));
        let pages = pages.lock().unwrap();
        let texture = pages.texture(WRAPPER).unwrap();
        assert_eq!((texture.at(0, 0), texture.at(16 + 14, 14)), (b'a', b'b'));
    }

    #[test]
    fn a_character_without_a_glyph_is_asked_for_once() {
        let pages = pages((64, 64), None, false);
        let draws = Cell::new(0);
        let none = |_: &(), _, _| {
            draws.set(draws.get() + 1);
            None
        };
        assert_eq!(pages.lock().unwrap().glyph(FONT, 0x4e00, none), None);
        assert_eq!(pages.lock().unwrap().glyph(FONT, 0x4e00, none), None);
        assert_eq!(draws.get(), 1);
        // A font that draws from no page gets none, and draws nothing.
        assert_eq!(pages.lock().unwrap().glyph(FONT + 8, 'a' as u32, none), None);
        assert_eq!(draws.get(), 1);
    }

    #[test]
    fn a_glyph_without_room_waits_and_is_drawn_once_after_the_page_grows() {
        let pages = pages((32, 32), Some((64, 64)), false);
        let mut world = World::default();
        let draws = Cell::new(0);
        for c in ['a', 'b', 'c', 'd'] {
            assert!(glyph(&pages, c as u32, &draws).is_some());
        }
        // Full: the next glyph waits, asked for many times as a game loads
        // and drawn only once the page has grown.
        for _ in 0..100 {
            assert_eq!(glyph(&pages, 'e' as u32, &draws), None);
        }
        assert_eq!(draws.get(), 5);
        present(&pages, &mut world);
        assert_eq!(world.taken, vec![(WRAPPER, vec![FONT], (32, 64))]);
        assert_eq!(pages.lock().unwrap().size(WRAPPER), Some((32, 64)));
        let e = glyph(&pages, 'e' as u32, &draws).unwrap();
        assert_eq!(glyph(&pages, 'e' as u32, &draws), Some(e));
        assert_eq!(draws.get(), 6);
        assert_eq!((e.x, e.y), (0, 32));
        // The glyphs drawn before kept their places, and their pixels.
        let pages = pages.lock().unwrap();
        assert_eq!(pages.texture(WRAPPER).unwrap().at(16, 16), b'd');
    }

    #[test]
    fn a_page_three_quarters_full_grows_at_the_next_present() {
        let pages = pages((64, 64), Some((128, 128)), false);
        let mut world = World::default();
        let draws = Cell::new(0);
        // Twelve of sixteen squares: three quarters, not past them.
        for c in 0..12 {
            glyph(&pages, 'a' as u32 + c, &draws).unwrap();
        }
        present(&pages, &mut world);
        assert!(world.taken.is_empty());
        glyph(&pages, 'z' as u32, &draws).unwrap();
        present(&pages, &mut world);
        assert_eq!(world.taken, vec![(WRAPPER, vec![FONT], (64, 128))]);
        // Once grown, it asks for no more until it fills again.
        present(&pages, &mut world);
        assert_eq!(world.taken.len(), 1);
    }

    #[test]
    fn a_page_that_cannot_grow_stops_asking() {
        // A page that keeps its size never asks.
        let pages = pages((32, 32), None, false);
        let mut world = World::default();
        let draws = Cell::new(0);
        for c in 0..5 {
            glyph(&pages, 'a' as u32 + c, &draws);
        }
        present(&pages, &mut world);
        assert!(world.taken.is_empty());
        // One whose growth fails keeps its size from then on.
        let pages = self::pages((32, 32), Some((64, 64)), false);
        let mut world = World { refuse: true, ..World::default() };
        for c in 0..5 {
            glyph(&pages, 'a' as u32 + c, &draws);
        }
        present(&pages, &mut world);
        assert_eq!(pages.lock().unwrap().size(WRAPPER), Some((32, 32)));
        world.refuse = false;
        pages.lock().unwrap().want_room_everywhere();
        assert_eq!(glyph(&pages, 'z' as u32, &draws), None);
        present(&pages, &mut world);
        assert!(world.taken.is_empty());
        assert_eq!(pages.lock().unwrap().size(WRAPPER), Some((32, 32)));
    }

    #[test]
    fn a_map_page_grown_at_the_main_menu_has_its_names_built_in_the_first_frame_of_a_game() {
        let pages = pages((32, 32), Some((64, 64)), true);
        let mut world = World::default();
        let draws = Cell::new(0);
        for c in 0..5 {
            glyph(&pages, 'a' as u32 + c, &draws);
        }
        // At the main menu: the page grows, the names wait for a game.
        present(&pages, &mut world);
        assert_eq!(world.taken.len(), 1);
        present(&pages, &mut world);
        assert_eq!(world.names_built, 0);
        // The first frame of a game builds them, once.
        world.in_game = true;
        present(&pages, &mut world);
        present(&pages, &mut world);
        assert_eq!(world.names_built, 1);
        // A page of the interface's fonts leaves them as they are.
        let pages = self::pages((32, 32), Some((64, 64)), false);
        for c in 0..5 {
            glyph(&pages, 'a' as u32 + c, &draws);
        }
        present(&pages, &mut world);
        assert_eq!((world.taken.len(), world.names_built), (2, 1));
    }

    #[test]
    fn mip_levels_are_made_once_after_glyphs_are_written() {
        let pages = pages((64, 64), None, false);
        let mut world = World::default();
        let draws = Cell::new(0);
        glyph(&pages, 'a' as u32, &draws);
        glyph(&pages, 'b' as u32, &draws);
        present(&pages, &mut world);
        present(&pages, &mut world);
        assert_eq!(pages.lock().unwrap().texture(WRAPPER).unwrap().mips.get(), 1);
        // A glyph already drawn writes nothing.
        glyph(&pages, 'a' as u32, &draws);
        present(&pages, &mut world);
        assert_eq!(pages.lock().unwrap().texture(WRAPPER).unwrap().mips.get(), 1);
    }

    #[test]
    fn a_page_grows_for_every_font_that_draws_from_it_and_goes_with_its_texture() {
        let pages = pages((32, 32), Some((64, 64)), false);
        let mut world = World::default();
        let draws = Cell::new(0);
        pages.lock().unwrap().draws_from(FONT + 8, WRAPPER, Tone::Dark);
        pages.lock().unwrap().draws_from(FONT + 16, WRAPPER, Tone::Light);
        pages.lock().unwrap().forget(FONT + 16);
        // Light and dark glyphs of one character are glyphs of their own.
        let light = glyph(&pages, 'a' as u32, &draws).unwrap();
        let dark = pages.lock().unwrap().glyph(FONT + 8, 'a' as u32, square(&draws)).unwrap();
        assert_ne!((light.x, light.y), (dark.x, dark.y));
        pages.lock().unwrap().want_room_everywhere();
        present(&pages, &mut world);
        assert_eq!(world.taken, vec![(WRAPPER, vec![FONT, FONT + 8], (32, 64))]);
        // The game destroyed the page: its fonts draw from nothing.
        pages.lock().unwrap().destroyed(WRAPPER);
        assert_eq!(pages.lock().unwrap().owner(FONT), None);
        assert_eq!(glyph(&pages, 'b' as u32, &draws), None);
    }

    #[test]
    fn a_glyph_without_room_while_a_font_loads_is_drawn_once_the_page_grows_at_once() {
        let pages = pages((32, 32), Some((64, 64)), false);
        let mut world = World::default();
        let draws = Cell::new(0);
        let mut pages = pages.lock().unwrap();
        for c in 0..4 {
            pages.glyph_in(WRAPPER, 'a' as u32 + c, Tone::Light, square(&draws)).unwrap();
        }
        assert_eq!(pages.glyph_in(WRAPPER, 'e' as u32, Tone::Light, square(&draws)), Err(NoRoom));
        pages.grow(WRAPPER, &mut world).unwrap();
        assert!(pages.glyph_in(WRAPPER, 'e' as u32, Tone::Light, square(&draws)).unwrap().is_some());
        assert_eq!(world.taken.len(), 1);
    }

    #[test]
    fn a_page_starts_at_a_quarter_and_doubles() {
        let max = (16384, 16384);
        // Taller first, then wider, until it is as large as it may be.
        assert_eq!(grown((768, 768), max), Some((768, 1536)));
        assert_eq!(grown((768, 1536), max), Some((1536, 1536)));
        assert_eq!(grown((16384, 8192), max), Some((16384, 16384)));
        assert_eq!(grown((16384, 16384), max), None);
        // Up to the device's largest, then the other way.
        assert_eq!(grown((768, 768), (8192, 1000)), Some((768, 1000)));
        assert_eq!(grown((768, 1000), (8192, 1000)), Some((1536, 1000)));
        assert_eq!(grown((8192, 1000), (8192, 1000)), None);
    }
}
