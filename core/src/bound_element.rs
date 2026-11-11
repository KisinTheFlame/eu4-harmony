//! A game element the framework has bound, in a vanilla window that hosts an
//! extension's elements (`crate::ui`) or in an extension window or pop-up
//! (`crate::windows`): how the framework makes it show what is wanted of it,
//! and how it hears the element's clicks.
//!
//! What it shows is set only where it differs from what was set before, each
//! of the element's methods checked first to be the one the framework knows
//! for its look, and a fault or a method it does not know is an error. A text
//! box hides by showing no text, since the show and hide methods of the text
//! boxes are not known on this build. The tooltip is kept, and given to the
//! game by [`fill_tooltip`] where the game's tooltip of the element under the
//! pointer is asked for. A clicked button is heard into a bounded queue of
//! its owner's ([`Hears`]), with the keys held down with the click, and taken
//! up at the next frame.
use crate::called_by_game;
use crate::{
    game::{self, Caller, WorldSession},
    game_string,
    memory::Memory,
    objects::gui::{button, element, functions},
    ui::{self, Callback, Subscription},
    unicode::game_text,
    Result,
};
use eu4_harmony_lua::Property;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// What kind of game element it is, which says which methods it is set
/// with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// A text box: its text, which is empty while it is hidden.
    Text,
    /// A button: shown, enabled, its frame, its tooltip.
    Button,
    /// A flag, a button of the game's: shown, its frame, its country.
    Flag,
    /// An icon or a progress bar: shown, its frame, a progress bar's value.
    Icon,
    /// An element the framework only hears or reads, such as a window's
    /// close button: nothing is set on it.
    Plain,
}

/// What is wanted of an element: each property, or none for what is left as
/// it is. Text is game text.
#[derive(Clone, Default, Debug, PartialEq)]
pub(crate) struct Shown {
    pub visible: Option<bool>,
    pub enabled: Option<bool>,
    pub text: Option<Vec<u8>>,
    pub tooltip: Option<Vec<u8>>,
    pub frame: Option<u32>,
    /// A progress bar's, from 0 to 1.
    pub value: Option<f64>,
    /// A flag's country, by its tag; empty for none.
    pub country: Option<String>,
}

impl Shown {
    /// What a game widget's properties, as an extension sets them, want.
    pub(crate) fn of(properties: &BTreeMap<&'static str, Property>) -> Self {
        let mut shown = Self::default();
        for property in properties.values() {
            match property {
                Property::Visible(on) => shown.visible = Some(*on),
                Property::Enabled(on) => shown.enabled = Some(*on),
                Property::Text(words) => shown.text = Some(game_text::from_str(words)),
                Property::Tooltip(words) => shown.tooltip = Some(game_text::from_str(words)),
                Property::Frame(frame) => shown.frame = Some(*frame),
                Property::Value(value) => shown.value = Some(*value),
                Property::Country(tag) => shown.country = Some(tag.clone()),
            }
        }
        shown
    }

    /// The text a text box is to show, if any: none while it is hidden.
    pub(crate) fn text_box(&self) -> Option<Vec<u8>> {
        if self.visible == Some(false) {
            Some(Vec::new())
        } else {
            self.text.clone()
        }
    }
}

/// Whose queue a bound button's clicks are heard into: the elements spliced
/// into vanilla windows' (`crate::ui`), or the extension windows' and
/// pop-ups' (`crate::windows`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hears {
    Panels = 0,
    Windows = 1,
}

/// A click the game reported: the callback's context, the button, whether
/// it was a right click, and the keys held down with it
/// ([`ui::modifiers`]), read as it came, while they still are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Clicked {
    pub context: usize,
    pub button: usize,
    pub right: bool,
    pub modifiers: u32,
}

/// The most clicks a queue keeps until they are taken up; more are lost.
const CLICKS: usize = 64;

/// The clicks heard since they were last taken up, by [`Hears`]. The game
/// calls back while the framework may hold its owner's lock further up the
/// thread, so a click is only put here.
static CLICKED: [Mutex<Vec<Clicked>>; 2] = [Mutex::new(Vec::new()), Mutex::new(Vec::new())];

fn heard<const Q: usize>(context: usize, button: usize, right: bool) {
    let mut clicked = CLICKED[Q].lock().unwrap_or_else(|e| e.into_inner());
    if clicked.len() < CLICKS {
        clicked.push(Clicked { context, button, right, modifiers: ui::modifiers() });
    }
}

#[called_by_game]
extern "system" fn left_click<const Q: usize>(context: usize, button: usize) {
    // The button takes the input whatever the listeners do, so the click
    // does not go on to the map.
    heard::<Q>(context, button, false)
}

#[called_by_game]
extern "system" fn right_click<const Q: usize>(context: usize, button: usize) {
    heard::<Q>(context, button, true)
}

/// The clicks `hears` heard since they were last taken.
pub(crate) fn take_clicks(hears: Hears) -> Vec<Clicked> {
    std::mem::take(&mut *CLICKED[hears as usize].lock().unwrap_or_else(|e| e.into_inner()))
}

/// A listener on the button `object`'s clicks, which hears them into the
/// queue of `hears`; not yet subscribed.
pub(crate) fn listener(m: &Memory, object: usize, hears: Hears) -> Result<Subscription> {
    let (left, right) = match hears {
        Hears::Panels => (left_click::<0> as *const () as usize, right_click::<0> as *const () as usize),
        Hears::Windows => (left_click::<1> as *const () as usize, right_click::<1> as *const () as usize),
    };
    Subscription::new(m, object, left, right)
}

/// A game element the framework has bound.
pub(crate) struct BoundElement {
    pub object: usize,
    look: Look,
    /// For a button: the listener on its clicks.
    subscription: Option<Subscription>,
    /// What was set on it, as [`Shown`] says it; a text box's text as it
    /// shows it.
    applied: Shown,
    /// Its tooltip, game text; empty for none.
    tooltip: Vec<u8>,
}

impl BoundElement {
    /// The element `object`, of `look`, which nothing was set on yet.
    pub(crate) fn new(object: usize, look: Look) -> Self {
        Self { object, look, subscription: None, applied: Shown::default(), tooltip: Vec::new() }
    }

    /// [`BoundElement::new`], hearing its clicks into the queue of `hears`
    /// once [`BoundElement::link`]ed.
    pub(crate) fn listening(m: &Memory, object: usize, look: Look, hears: Hears) -> Result<Self> {
        Ok(Self { subscription: Some(listener(m, object, hears)?), ..Self::new(object, look) })
    }

    /// Subscribes its listener, if it has one.
    pub(crate) fn link(&mut self, m: &Memory) -> Result<()> {
        self.subscription.as_mut().map_or(Ok(()), |s| s.link(m))
    }

    /// Unsubscribes its listener while the element is still alive.
    pub(crate) fn unlink(&mut self, m: &Memory) -> Result<()> {
        self.subscription.as_mut().map_or(Ok(()), |s| s.unlink(m))
    }

    /// Its listener's callback, to keep once it is let go: the game may
    /// still hold a pointer to it.
    pub(crate) fn retire(self) -> Option<Callback> {
        self.subscription.map(|s| s.wrapper)
    }

    /// Whether it listens to its clicks.
    pub(crate) fn listens(&self) -> bool {
        self.subscription.is_some()
    }

    /// Whether `clicked` is a click it heard.
    pub(crate) fn heard(&self, clicked: &Clicked) -> bool {
        clicked.button == self.object && self.subscription.as_ref().is_some_and(|s| s.linked() && s.context() == clicked.context)
    }

    /// Whether it is shown now, as the game has it.
    pub(crate) fn visible(&self, m: &Memory) -> Result<bool> {
        Ok(m.u8(self.object + element::VISIBLE)? != 0)
    }

    /// Its tooltip, if it has one.
    pub(crate) fn tooltip(&self) -> Option<&[u8]> {
        Some(self.tooltip.as_slice()).filter(|t| !t.is_empty())
    }

    /// Makes it show `want`: what differs from what was set before, and
    /// whether it is shown where the game shows it otherwise.
    pub(crate) fn apply(&mut self, m: &Memory, want: &Shown) -> Result<()> {
        if let Some(tooltip) = &want.tooltip {
            self.tooltip.clone_from(tooltip);
        }
        match self.look {
            Look::Plain => Ok(()),
            Look::Text => {
                let Some(text) = want.text_box() else { return Ok(()) };
                if self.applied.text.as_ref() != Some(&text) {
                    write_text(m, self.object, &text)?;
                    self.applied.text = Some(text);
                }
                Ok(())
            }
            Look::Button | Look::Flag | Look::Icon => {
                if let Some(tag) = want.country.as_ref().filter(|_| self.look == Look::Flag) {
                    if self.applied.country.as_ref() != Some(tag) {
                        set_country(m, self.object, tag)?;
                        self.applied.country = Some(tag.clone());
                    }
                }
                if let Some(on) = want.enabled.filter(|_| self.look == Look::Button) {
                    if self.applied.enabled != Some(on) {
                        self.call(m, element::ENABLE, &[ui::SET_ENABLED.rva()], &[on as usize])?;
                        self.applied.enabled = Some(on);
                    }
                }
                if let Some(frame) = want.frame {
                    if self.applied.frame != Some(frame) {
                        let known: &[usize] = if self.look == Look::Icon { &functions::ICON_FRAMES } else { &[functions::BUTTON_FRAME] };
                        self.call(m, element::FRAME, known, &[frame as usize])?;
                        self.applied.frame = Some(frame);
                    }
                }
                if let Some(value) = want.value.filter(|_| self.look == Look::Icon) {
                    if self.applied.value != Some(value) {
                        self.call(m, element::FRAME, &functions::ICON_FRAMES, &[(value.clamp(0.0, 1.0) * 100.0) as usize])?;
                        self.applied.value = Some(value);
                    }
                }
                if let Some(on) = want.visible {
                    if self.visible(m)? != on {
                        let known: &[usize] = match (on, self.look == Look::Icon) {
                            (true, false) => &[ui::SHOW],
                            (false, false) => &[ui::HIDE],
                            (true, true) => &functions::ICON_SHOWS,
                            (false, true) => &functions::ICON_HIDES,
                        };
                        self.call(m, if on { element::SHOW } else { element::HIDE }, known, &[])?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Calls its method in `slot` with `args`, when it is one of `known`.
    fn call(&self, m: &Memory, slot: usize, known: &[usize], args: &[usize]) -> Result<()> {
        let at = m.vslot(self.object, slot)?;
        if !known.iter().any(|&k| m.at(k) == at) {
            return Err(format!("an element's method at {slot:#x} is not one the framework knows"));
        }
        let mut all = vec![self.object];
        all.extend_from_slice(args);
        m.call(at, &all).map(|_| ())
    }
}

/// Builds a game string holding `bytes` with the game's allocator and moves
/// it into the text box `object`. The setter takes the string's contents and
/// leaves it empty (docs/research/interface-bindings.md, "The setter"); it is
/// still the framework's, and destroyed as every `std::string` is, which
/// holds whatever the setter leaves in it.
pub(crate) fn write_text(m: &Memory, object: usize, bytes: &[u8]) -> Result<()> {
    let mut string = game_string::empty();
    let at = string.as_mut_ptr() as usize;
    if !bytes.is_empty() {
        game_string::append_checked(m, at, bytes)?;
    }
    let set = ui::TEXT_SET.call(m, [object, at, 0]);
    game_string::destroy_checked(m, at)?;
    set.map(|_| ())
}

/// Gives a flag its country, which must be one of the game's: the game takes
/// the tag's index into its countries unchecked. Nothing for none.
pub(crate) fn set_country(m: &Memory, object: usize, letters: &str) -> Result<()> {
    if letters.is_empty() {
        return Ok(());
    }
    let session = WorldSession::open(Caller::Remote(game::As::Player)).map_err(|e| e.message)?;
    let bytes: [u8; 3] = letters.as_bytes().try_into().map_err(|_| format!("{letters:?} is not a tag"))?;
    let tag = game::countries::named(session.m, &session.world, eu4_harmony_remote::CountryTag(bytes)).map_err(|e| e.message)?;
    let argument: [u64; 2] = [tag.0, 1];
    for (slot, function) in [(button::SET_COUNTRY, functions::SET_COUNTRY.rva()), (element::INDEX, functions::SET_INDEX.rva())] {
        if m.vslot(object, slot)? != m.at(function) {
            return Err("a flag's method is not the one the framework knows".into());
        }
    }
    functions::SET_COUNTRY.call(m, [object, argument.as_ptr() as usize])?;
    functions::SET_INDEX.call(m, [object, tag.index() as i32 as u32 as usize])?;
    Ok(())
}

/// Fills the game's tooltip at `out`, its main text at `+0` and its delayed
/// text after it, with `text`, only where the game left both empty.
pub(crate) fn fill_tooltip(m: &Memory, out: usize, text: &[u8]) -> Result<()> {
    let empty = |offset: usize| game_string::untouched(m, out + offset);
    if empty(0)? && empty(game_string::OBJECT)? {
        game_string::append_checked(m, out, text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::buffers::{Image, Object};
    use std::cell::RefCell;

    thread_local! {
        /// What the test's functions were called with: which, and the
        /// argument after the element.
        static CALLED: RefCell<Vec<(&'static str, usize)>> = const { RefCell::new(Vec::new()) };
    }

    fn called(what: &'static str, arg: usize) -> usize {
        CALLED.with(|c| c.borrow_mut().push((what, arg)));
        0
    }

    fn taken() -> Vec<(&'static str, usize)> {
        CALLED.with(|c| std::mem::take(&mut *c.borrow_mut()))
    }

    extern "C" fn show(_: usize) -> usize {
        called("show", 0)
    }
    extern "C" fn hide(_: usize) -> usize {
        called("hide", 0)
    }
    extern "C" fn enable(_: usize, on: usize) -> usize {
        called("enable", on)
    }
    extern "C" fn frame(_: usize, n: usize) -> usize {
        called("frame", n)
    }
    extern "C" fn text_set(_: usize, string: usize, _: usize) -> usize {
        // The text given, as its size: none is the empty string.
        called("text", unsafe { *((string + game_string::SIZE) as *const usize) })
    }
    extern "C" fn destroy(_: usize) -> usize {
        called("destroy", 0)
    }

    /// The game's image with the element methods the framework knows, and an
    /// element of `look`'s whose methods are those.
    struct Game {
        image: Image,
        vtable: Object,
        element: Object,
    }

    impl Game {
        fn new(look: Look) -> Self {
            let image = Image::new();
            let m = image.memory();
            let (shows, hides, frames): (usize, usize, usize) = match look {
                Look::Icon => (functions::ICON_SHOWS[0], functions::ICON_HIDES[0], functions::ICON_FRAMES[0]),
                _ => (ui::SHOW, ui::HIDE, functions::BUTTON_FRAME),
            };
            image
                .function(shows, show as *const ())
                .function(hides, hide as *const ())
                .function(ui::SET_ENABLED.rva(), enable as *const ())
                .function(frames, frame as *const ())
                .function(ui::TEXT_SET.rva(), text_set as *const ())
                .function(game_string::DESTROY_AT.rva(), destroy as *const ());
            let vtable = Object::new(0x300)
                .u64(element::SHOW, m.at(shows) as u64)
                .u64(element::HIDE, m.at(hides) as u64)
                .u64(element::ENABLE, m.at(ui::SET_ENABLED.rva()) as u64)
                .u64(element::FRAME, m.at(frames) as u64);
            let element = Object::new(0x200).u64(0, vtable.at() as u64).u8(element::VISIBLE, 1);
            taken();
            Self { image, vtable, element }
        }

        fn bound(&self, look: Look) -> BoundElement {
            BoundElement::new(self.element.at(), look)
        }
    }

    #[test]
    fn what_is_set_again_is_not_set_twice_and_shown_is_read_from_the_game() {
        let game = Game::new(Look::Button);
        let m = game.image.memory();
        let mut b = game.bound(Look::Button);
        let want = Shown { visible: Some(false), enabled: Some(false), frame: Some(2), ..Shown::default() };
        b.apply(m, &want).unwrap();
        assert_eq!(taken(), [("enable", 0), ("frame", 2), ("hide", 0)]);
        // The hide is the game's own here; what the game shows is what counts.
        game.element.set(element::VISIBLE, &[0]);
        b.apply(m, &want).unwrap();
        assert_eq!(taken(), [], "nothing differs");
        game.element.set(element::VISIBLE, &[1]);
        b.apply(m, &want).unwrap();
        assert_eq!(taken(), [("hide", 0)], "the game showed it again");
        b.apply(m, &Shown { enabled: Some(true), ..want }).unwrap();
        assert_eq!(taken(), [("enable", 1), ("hide", 0)]);
    }

    #[test]
    fn a_method_the_framework_does_not_know_is_not_called() {
        let game = Game::new(Look::Button);
        let m = game.image.memory();
        game.vtable.set(element::HIDE, &(m.at(ui::SHOW) as u64).to_le_bytes());
        let mut b = game.bound(Look::Button);
        assert!(b.apply(m, &Shown { visible: Some(false), ..Shown::default() }).unwrap_err().contains("not one the framework knows"));
        assert_eq!(taken(), []);
        // An icon is set with the icons' methods, not the buttons'.
        let game = Game::new(Look::Button);
        let mut icon = game.bound(Look::Icon);
        assert!(icon.apply(game.image.memory(), &Shown { frame: Some(3), ..Shown::default() }).is_err());
        let game = Game::new(Look::Icon);
        let mut icon = game.bound(Look::Icon);
        icon.apply(game.image.memory(), &Shown { frame: Some(3), value: Some(0.5), visible: Some(false), enabled: Some(false), ..Shown::default() }).unwrap();
        assert_eq!(taken(), [("frame", 3), ("frame", 50), ("hide", 0)], "an icon is not enabled");
    }

    #[test]
    fn a_hidden_text_box_shows_no_text_and_its_string_is_let_go() {
        let game = Game::new(Look::Text);
        let m = game.image.memory();
        let mut b = game.bound(Look::Text);
        b.apply(m, &Shown { visible: Some(true), ..Shown::default() }).unwrap();
        assert_eq!(taken(), [], "nothing asked, nothing written");
        b.apply(m, &Shown { visible: Some(false), text: Some(b"12".to_vec()), ..Shown::default() }).unwrap();
        assert_eq!(taken(), [("text", 0), ("destroy", 0)]);
        b.apply(m, &Shown { visible: Some(false), text: Some(b"34".to_vec()), ..Shown::default() }).unwrap();
        assert_eq!(taken(), [], "still hidden, still empty");
    }

    #[test]
    fn a_tooltip_is_kept_and_a_plain_element_is_left_as_it_is() {
        let game = Game::new(Look::Button);
        let m = game.image.memory();
        let mut b = game.bound(Look::Plain);
        b.apply(m, &Shown { visible: Some(false), tooltip: Some(b"Why".to_vec()), ..Shown::default() }).unwrap();
        assert_eq!(taken(), []);
        assert_eq!(b.tooltip(), Some(&b"Why"[..]));
        b.apply(m, &Shown { tooltip: Some(Vec::new()), ..Shown::default() }).unwrap();
        assert_eq!(b.tooltip(), None);
    }

    #[test]
    fn properties_are_what_is_shown() {
        let set = BTreeMap::from([
            ("visible", Property::Visible(true)),
            ("text", Property::Text("Done".into())),
            ("value", Property::Value(0.25)),
            ("country", Property::Country("FRA".into())),
        ]);
        let shown = Shown::of(&set);
        assert_eq!(shown, Shown { visible: Some(true), text: Some(b"Done".to_vec()), value: Some(0.25), country: Some("FRA".into()), ..Shown::default() });
        assert_eq!(Shown { visible: Some(false), ..shown }.text_box(), Some(Vec::new()));
    }

    #[test]
    fn a_queue_keeps_so_many_clicks_until_they_are_taken() {
        take_clicks(Hears::Windows);
        for n in 0..CLICKS + 3 {
            heard::<1>(7, n, n % 2 == 0);
        }
        assert!(take_clicks(Hears::Panels).iter().all(|c| c.context != 7), "the other owner's queue");
        let clicks = take_clicks(Hears::Windows);
        assert_eq!(clicks.len(), CLICKS);
        assert_eq!((clicks[0].button, clicks[0].right, clicks[1].right), (0, true, false));
        assert!(take_clicks(Hears::Windows).is_empty());
    }
}
