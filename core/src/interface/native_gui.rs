//! Low-level native EU4 GUI primitives shared by host-window binding (`ui`),
//! extension windows (`windows`), and bound elements (`bound_element`).
//!
//! Holding native method addresses, string/lookup helpers, and button click
//! subscriptions in one leaf module eliminates internal cycles between
//! `bound_element`, `ui`, and `windows`.
use crate::{
    base::Result,
    native::{
        game_string,
        hooks::Call,
        memory::Memory,
        objects::{
            campaign::tag,
            gui::{button, callback, classes, element, functions, mouse_input, signal, window, window_gui},
            list,
        },
        points::{GameFunction, Point},
    },
};
use std::sync::Mutex;

/// A window's [`window::KILL`]: marks the window and its children dead;
/// memory is freed later in the frame by the GUI manager's sweep.
pub(super) const WINDOW_KILL: usize = 0x1547520;
pub(super) const TEXT_SET: GameFunction<3> = GameFunction::at(0x152ae10);
const SUBSCRIBE: GameFunction<2> = GameFunction::at(0xfbe3f0);
const SET_RIGHT_CLICK: GameFunction<2> = GameFunction::at(0x1593860);
const SET_DOUBLE_CLICK: GameFunction<2> = GameFunction::at(0x1593870);
pub(super) const SHOW: usize = 0x15aeb30;
pub(super) const HIDE: usize = 0x15aead0;
pub(super) const SET_ENABLED: GameFunction<2> = GameFunction::at(0xfaab30);
/// A button's [`element::INPUT`] `(button, input, …)`, which hands the input
/// to the elements' base `0x1592bd0` unless the button is disabled.
const BUTTON_INPUT: usize = 0x15aefc0;

/// The keys held down with a click, as bits.
pub const MODIFIER_SHIFT: u32 = 1;
pub const MODIFIER_CTRL: u32 = 2;

/// The buttons the framework's subscriptions take right clicks of
/// ([`Subscription::link`]): on them a quick second right press is a press
/// ([`button_input`]).
static TAKING_RIGHT: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// A callback the framework subscribes to a button's signal (see [`callback`]).
pub(super) type Callback = Box<[usize; callback::SIZE / 8]>;

/// Drops `button` from the right-click double-press override set when it is destroyed.
pub(super) fn drop_taking_right(button: usize) {
    TAKING_RIGHT.lock().unwrap_or_else(|e| e.into_inner()).retain(|&b| b != button);
}

/// The keys held down now, as [`MODIFIER_SHIFT`] and [`MODIFIER_CTRL`]: the
/// player's, as Windows has handed them to the game's window, and remote
/// control's, which `GetKeyState` does not see.
pub(super) fn modifiers() -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
    let down = |key: u16| unsafe { GetKeyState(key as i32) } < 0;
    let (shift, ctrl) = super::modifiers_held();
    let mut out = 0;
    if shift || down(VK_SHIFT) {
        out |= MODIFIER_SHIFT;
    }
    if ctrl || down(VK_CONTROL) {
        out |= MODIFIER_CTRL;
    }
    out
}

/// The framework's listener on a button's click signal: the callback the
/// game calls back through, which must outlive the subscription, and what
/// the button reported before.
pub(super) struct Subscription {
    pub object: usize,
    signal: usize,
    pub wrapper: Callback,
    linked: bool,
    old_right_click: u8,
    old_double_click: u8,
}

impl Subscription {
    /// A listener for the button `object`'s clicks, after checking its
    /// signal is the one the framework knows: `left` and `right` are called
    /// with the wrapper's address and the button. Not yet subscribed.
    pub(super) fn new(m: &Memory, object: usize, left: usize, right: usize) -> Result<Self> {
        let signal = object + button::SIGNAL;
        if m.ptr(signal)? != m.at(classes::SIGNAL)
            || m.ptr(signal + signal::AFTER_LISTENERS)? != 0
            || m.vslot(signal, signal::SUBSCRIBE)? != m.at(SUBSCRIBE)
            || m.vslot(signal, signal::UNSUBSCRIBE)? != m.at(functions::UNSUBSCRIBE)
            || m.vslot(object, button::SET_RIGHT_CLICK)? != m.at(SET_RIGHT_CLICK)
            || m.vslot(object, button::SET_DOUBLE_CLICK)? != m.at(SET_DOUBLE_CLICK)
        {
            return Err("unexpected click signal".into());
        }
        let mut wrapper: Callback = Box::new([0; callback::SIZE / 8]);
        wrapper[0] = m.at(classes::CALLBACK);
        wrapper[callback::CONTEXT / 8] = wrapper.as_ptr() as usize;
        wrapper[callback::RIGHT_CLICK / 8] = right;
        // Called for a quick second press too, which the game sends as a
        // double click, unlike the click function the army panel's own
        // callbacks use.
        wrapper[callback::CLICKED / 8] = left;
        Ok(Self {
            object,
            signal,
            wrapper,
            linked: false,
            old_right_click: m.u8(object + button::RIGHT_CLICK)?,
            old_double_click: m.u8(object + button::DOUBLE_CLICK)?,
        })
    }

    /// The context the game calls `left` and `right` with.
    pub(super) fn context(&self) -> usize {
        self.wrapper.as_ptr() as usize
    }

    /// Whether it is subscribed.
    pub(super) fn linked(&self) -> bool {
        self.linked
    }

    #[cfg(test)]
    pub(super) fn for_test(object: usize) -> Self {
        Self { object, signal: 0, wrapper: Box::new([0; callback::SIZE / 8]), linked: false, old_right_click: 0, old_double_click: 0 }
    }

    #[cfg(test)]
    pub(super) fn set_linked_for_test(&mut self, linked: bool) {
        self.linked = linked;
    }

    pub(super) fn subscribed(&self, m: &Memory) -> Result<bool> {
        Ok(crate::native::containers::items(m, self.signal + signal::LISTENERS, 512)?.contains(&self.context()))
    }

    /// Subscribes it, and has the button report right and double clicks.
    pub(super) fn link(&mut self, m: &Memory) -> Result<()> {
        self.linked = true;
        SUBSCRIBE.call(m, [self.signal, self.context()])?;
        if !self.subscribed(m)? {
            return Err("subscription did not register".into());
        }
        SET_RIGHT_CLICK.call(m, [self.object, 1])?;
        SET_DOUBLE_CLICK.call(m, [self.object, 1])?;
        let mut taking = TAKING_RIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if !taking.contains(&self.object) {
            taking.push(self.object);
        }
        Ok(())
    }

    /// Unsubscribes it while the button is still alive.
    pub(super) fn unlink(&mut self, m: &Memory) -> Result<()> {
        drop_taking_right(self.object);
        if !self.linked {
            return Ok(());
        }
        if m.ptr(self.signal)? != m.at(classes::SIGNAL) || m.u8(self.signal + signal::LISTENERS + list::CHANGING)? != 0 {
            return Err("cannot unsubscribe from a changed or dispatching signal".to_string());
        }
        functions::UNSUBSCRIBE.call(m, [self.signal, self.context()])?;
        if self.subscribed(m)? {
            return Err("observer still subscribed after unsubscribe".into());
        }
        self.linked = false;
        SET_RIGHT_CLICK.call(m, [self.object, self.old_right_click as usize])?;
        SET_DOUBLE_CLICK.call(m, [self.object, self.old_double_click as usize])?;
        Ok(())
    }
}

/// Calls `f` with a game string holding `text`, destroyed after.
pub(super) fn with_string<T>(m: &Memory, text: &[u8], f: impl FnOnce(usize) -> Result<T>) -> Result<T> {
    let mut string = game_string::empty();
    let at = string.as_mut_ptr() as usize;
    if !text.is_empty() {
        game_string::append_checked(m, at, text)?;
    }
    let result = f(at);
    game_string::destroy_checked(m, at)?;
    result
}

/// The element of `name` in the window at `root`, through its method at
/// `offset`, which must be `function`.
pub(super) fn lookup(m: &Memory, root: usize, (offset, function): (usize, GameFunction<2>), name: &str) -> Result<usize> {
    if m.vslot(root, offset)? != m.at(function) {
        return Err("window has an unexpected lookup method".into());
    }
    let c = format!("{name}\0");
    function.call(m, [root, c.as_ptr() as usize])
}

/// Name an element reports, through the getter shared by all GUI element classes.
pub(super) fn element_name(m: &Memory, object: usize) -> Result<Vec<u8>> {
    if m.vslot(object, element::NAME)? != m.at(functions::ELEMENT_NAME) {
        return Err("element has an unexpected name getter".into());
    }
    game_string::read(m, functions::ELEMENT_NAME.call(m, [object])?)
}

/// Shows or hides a native window root through its GUI interface vtable.
pub(super) fn show_window(m: &Memory, root: usize, show: bool) -> Result<()> {
    let gui = root + window::GUI;
    let (slot, function) = if show { (window_gui::SHOW, functions::WINDOW_SHOW) } else { (window_gui::HIDE, functions::WINDOW_HIDE) };
    let at = m.vslot(gui, slot)?;
    if at != m.at(function) {
        return Err("a window's show or hide is not the one the framework knows".into());
    }
    m.call(at, &[gui]).map(|_| ())
}

/// Disables the background of the window at `window`, the button `name`,
/// which a window's background has to be: it still takes the clicks on it,
/// so that they do not reach the map, but it cannot be pressed or dragged.
/// The fixed background sprite keeps the disabled state's original colors
/// (docs/game/interface/extension-windows.md, "A press on a window's background").
pub(super) fn keep_still(m: &Memory, window: usize, name: &str) -> Result<()> {
    let background = m.ptr(window + window::BACKGROUND)?;
    if background == 0 {
        return Err("the window has no background".into());
    }
    if m.ptr(background)? != m.at(classes::BACKGROUND) || element_name(m, background)? != name.as_bytes() {
        return Err(format!("the window's background is not its {name}"));
    }
    if m.vslot(background, element::ENABLE)? != m.at(SET_ENABLED) {
        return Err("button enable method changed".into());
    }
    SET_ENABLED.call(m, [background, 0]).map(|_| ())
}

/// Builds a game string holding `bytes` with the game's allocator and moves
/// it into the text box `object`. The setter takes the string's contents and
/// leaves it empty (docs/game/interface/interface-bindings.md, "The setter"); it is
/// still the framework's, and destroyed as every `std::string` is, which
/// holds whatever the setter leaves in it.
pub(super) fn write_text(m: &Memory, object: usize, bytes: &[u8]) -> Result<()> {
    let mut string = game_string::empty();
    let at = string.as_mut_ptr() as usize;
    if !bytes.is_empty() {
        game_string::append_checked(m, at, bytes)?;
    }
    let set = TEXT_SET.call(m, [object, at, 0]);
    game_string::destroy_checked(m, at)?;
    set.map(|_| ())
}

/// Gives a flag its country, which must be one of the game's: the game takes
/// the tag's index into its countries unchecked. Nothing for none.
pub(super) fn set_country(m: &Memory, object: usize, letters: &str) -> Result<()> {
    if letters.is_empty() {
        return Ok(());
    }
    let Some(raw) = super::resolve_shield_country(m, Some(letters))? else {
        return Err(format!("{letters:?} is not a known country"));
    };
    let index = (raw >> tag::INDEX) as u16 as i16;
    let argument: [u64; 2] = [raw, 1];
    for (slot, function) in [(button::SET_COUNTRY, functions::SET_COUNTRY.rva()), (element::INDEX, functions::SET_INDEX.rva())] {
        if m.vslot(object, slot)? != m.at(function) {
            return Err("a flag's method is not the one the framework knows".into());
        }
    }
    functions::SET_COUNTRY.call(m, [object, argument.as_ptr() as usize])?;
    functions::SET_INDEX.call(m, [object, index as i32 as u32 as usize])?;
    Ok(())
}

/// Before one of the framework's buttons takes the mouse's input: the game
/// takes a right press that soon follows another as a double click, and on
/// every button drops it, so that the second of two quick right clicks does
/// nothing (docs/game/interface/interface-bindings.md, "From input to a click").
/// On the framework's buttons it is made a press, whose release is then a
/// right click as the first's was.
fn button_input(call: &mut Call) {
    let (button, input) = (call.arg(0), call.arg(1));
    let Ok(taking) = TAKING_RIGHT.try_lock() else { return };
    if !taking.contains(&button) {
        return;
    }
    drop(taking);
    let m = call.memory();
    let field = |at: usize| m.u32(input + at).ok();
    if field(mouse_input::KIND) == Some(mouse_input::DOUBLE)
        && field(mouse_input::BUTTON) == Some(mouse_input::RIGHT)
        && field(mouse_input::DEVICE) == Some(mouse_input::MOUSE)
    {
        if let Err(e) = m.write(input + mouse_input::KIND, &mouse_input::PRESSED.to_le_bytes()) {
            log!("ui: a quick second right click stays a double click: {e}");
        }
    }
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(BUTTON_INPUT, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b d9 49 8b f8 48 8b 89", "button input", 3, Some(button_input), None),
        Point::method(classes::BUTTON, element::INPUT, BUTTON_INPUT, "button input method"),
        // The base reads what happened, the device and the mouse button,
        // and drops a right double click (0x1592da5: neither 3 nor 4).
        Point::keep(0x1592c33usize, "8b 4e 50 83 f8 03 75 7c 85 c9 75 3e 39 46 14", "element input's device and kind")
            .uses(&[mouse_input::DEVICE, mouse_input::KIND]),
        Point::keep(0x1592d23usize, "8b 4e 18 83 f9 01 75 7a 83 f8 05", "element input's mouse button").uses(&[mouse_input::BUTTON]),
        Point::keep(0x1592da5usize, "83 f8 03 75 7f 83 f9 02 0f 85 03 01 00 00", "element input's right press"),
        Point::keep(TEXT_SET, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", "text box setter"),
        Point::keep(SUBSCRIBE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 80", "signal subscribe"),
        Point::keep(SET_RIGHT_CLICK, "88 91 61 01 00 00 c3", "button right-click enable").uses(&[button::RIGHT_CLICK]),
        Point::keep(SET_DOUBLE_CLICK, "88 91 62 01 00 00 c3", "button double-click enable").uses(&[button::DOUBLE_CLICK]),
        Point::keep(SHOW, "40 53 48 83 ec 20 80 b9 00 01 00 00 00 48 8b d9", "element show"),
        Point::keep(HIDE, "40 53 48 83 ec 20 c6 81 c5 00 00 00 00 48 8b d9", "element hide").uses(&[element::VISIBLE]),
        Point::keep(SET_ENABLED, "48 8b 01 84 d2 74 07 48 ff a0 68 02 00 00 48 ff", "button enable"),
        Point::method(classes::WINDOW, window::KILL, WINDOW_KILL, "window kill method"),
        Point::method(classes::BUTTON, element::SHOW, SHOW, "button show method"),
        Point::method(classes::BUTTON, element::HIDE, HIDE, "button hide method"),
        Point::method(classes::BUTTON, element::ENABLE, SET_ENABLED.rva(), "button enable method"),
        Point::method(classes::BUTTON, button::SET_RIGHT_CLICK, SET_RIGHT_CLICK.rva(), "button right-click method"),
        Point::method(classes::BUTTON, button::SET_DOUBLE_CLICK, SET_DOUBLE_CLICK.rva(), "button double-click method"),
        Point::method(classes::SIGNAL, signal::SUBSCRIBE, SUBSCRIBE.rva(), "signal subscribe method"),
        Point::method(classes::SIGNAL, signal::UNSUBSCRIBE, functions::UNSUBSCRIBE.rva(), "signal unsubscribe method"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_game::buffers::{Image, Object};
    use std::cell::Cell;

    #[test]
    fn subscriptions_link_unlink_restore_click_flags_and_drop_taking_right_even_on_dispatch_failure() {
        thread_local! {
            static NODE: Cell<usize> = const { Cell::new(0) };
        }
        extern "C" fn subscribe(sig: usize, ctx: usize) -> usize {
            NODE.with(|n| {
                let node = n.get();
                unsafe {
                    *(node as *mut usize) = ctx;
                    *((sig + signal::LISTENERS + list::HEAD) as *mut usize) = node;
                    *((sig + signal::LISTENERS + list::TAIL) as *mut usize) = node;
                    *((sig + signal::LISTENERS + list::COUNT) as *mut u32) = 1;
                }
            });
            0
        }
        extern "C" fn unsubscribe(sig: usize, _: usize) -> usize {
            unsafe {
                *((sig + signal::LISTENERS + list::HEAD) as *mut usize) = 0;
                *((sig + signal::LISTENERS + list::TAIL) as *mut usize) = 0;
                *((sig + signal::LISTENERS + list::COUNT) as *mut u32) = 0;
            }
            0
        }
        extern "C" fn set_right(btn: usize, on: usize) -> usize {
            unsafe { *((btn + button::RIGHT_CLICK) as *mut u8) = on as u8 };
            0
        }
        extern "C" fn set_double(btn: usize, on: usize) -> usize {
            unsafe { *((btn + button::DOUBLE_CLICK) as *mut u8) = on as u8 };
            0
        }

        let image = Image::new();
        let m = image.memory();
        image
            .function(SUBSCRIBE.rva(), subscribe as *const ())
            .function(functions::UNSUBSCRIBE.rva(), unsubscribe as *const ())
            .function(SET_RIGHT_CLICK.rva(), set_right as *const ())
            .function(SET_DOUBLE_CLICK.rva(), set_double as *const ());

        let btn_vt = Object::new(0x200)
            .u64(button::SET_RIGHT_CLICK, m.at(SET_RIGHT_CLICK.rva()) as u64)
            .u64(button::SET_DOUBLE_CLICK, m.at(SET_DOUBLE_CLICK.rva()) as u64);
        let node = Object::new(list::NODE_SIZE);
        NODE.with(|n| n.set(node.at()));
        image
            .u64(classes::SIGNAL + signal::SUBSCRIBE, m.at(SUBSCRIBE.rva()) as u64)
            .u64(classes::SIGNAL + signal::UNSUBSCRIBE, m.at(functions::UNSUBSCRIBE.rva()) as u64);
        let btn = Object::new(0x300)
            .u64(0, btn_vt.at() as u64)
            .u64(button::SIGNAL, m.at(classes::SIGNAL) as u64)
            .u8(button::RIGHT_CLICK, 0)
            .u8(button::DOUBLE_CLICK, 1);

        let mut sub = Subscription::new(m, btn.at(), 0x1111, 0x2222).unwrap();
        sub.link(m).unwrap();
        assert!(sub.linked() && sub.subscribed(m).unwrap());
        assert!(TAKING_RIGHT.lock().unwrap().contains(&btn.at()));
        assert_eq!((m.u8(btn.at() + button::RIGHT_CLICK).unwrap(), m.u8(btn.at() + button::DOUBLE_CLICK).unwrap()), (1, 1));

        // Clean unlink restores original right/double flags and removes TAKING_RIGHT.
        sub.unlink(m).unwrap();
        assert!(!sub.linked() && !sub.subscribed(m).unwrap());
        assert!(!TAKING_RIGHT.lock().unwrap().contains(&btn.at()));
        assert_eq!((m.u8(btn.at() + button::RIGHT_CLICK).unwrap(), m.u8(btn.at() + button::DOUBLE_CLICK).unwrap()), (0, 1));

        // Even if unlink is refused because the signal is dispatching (CHANGING = 1), TAKING_RIGHT drops the dead button.
        sub.link(m).unwrap();
        unsafe { *((btn.at() + button::SIGNAL + signal::LISTENERS + list::CHANGING) as *mut u8) = 1 };
        assert!(sub.unlink(m).is_err());
        assert!(!TAKING_RIGHT.lock().unwrap().contains(&btn.at()));
    }
}
