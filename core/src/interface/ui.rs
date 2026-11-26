//! Binds spliced elements in the vanilla windows that host them, applies the
//! state extensions ask for, and turns button clicks into signals. Ported from
//! eu4-regional-combat's native UI; the ABI evidence is in
//! docs/game/interface/interface-bindings.md (tooltips included). Window teardown
//! and the main menu's windows are in docs/game/interface/window-construction.md.
//!
//! Extensions state what they want (visible, enabled, text, tooltip) and each
//! host's update hook applies it, so state survives the game rebuilding the
//! window and never needs a callback inside the game's UI code. How a bound
//! element is set and heard is `crate::native::bound_element`'s, which extension
//! windows share.
use crate::{
    base::{hosts::Host, notice, Result},
    native::bound_element::{self, BoundElement, Hears, Look, Shown},
    native::game_string,
    native::hooks::Call,
    native::memory::Memory,
    native::objects::{
        gui::{army_panel, button, callback, classes, element, functions, main_menu, mouse_input, signal, window},
        list,
    },
    native::points::{GameFunction, Point},
    native::splice::{Element, ElementKind},
};
use eu4_harmony_lua::windows::HostedElements;
use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

const PANEL_UPDATE: usize = 0x1450660;
const PANEL_DESTROY: usize = 0x144f560;
const PANEL_RECREATE: usize = 0x1450580;
const BUTTON_DESTROY: usize = 0x10b5670;
/// A window's [`window::KILL`]: marks the window and its children dead;
/// memory is freed later in the frame by the GUI manager's sweep.
pub(crate) const WINDOW_KILL: usize = 0x1547520;
/// Root window deleting destructor, reached from that sweep.
const WINDOW_DELETE: usize = 0x1546510;
const MENU_FRAME: usize = 0x10d2e80;
const TOOLTIP: usize = 0x1453b70;
pub(crate) const TEXT_SET: GameFunction<3> = GameFunction::at(0x152ae10);
const SUBSCRIBE: GameFunction<2> = GameFunction::at(0xfbe3f0);
const SET_RIGHT_CLICK: GameFunction<2> = GameFunction::at(0x1593860);
const SET_DOUBLE_CLICK: GameFunction<2> = GameFunction::at(0x1593870);
pub(crate) const SHOW: usize = 0x15aeb30;
pub(crate) const HIDE: usize = 0x15aead0;
pub(crate) const SET_ENABLED: GameFunction<2> = GameFunction::at(0xfaab30);
/// A button's [`element::INPUT`] `(button, input, …)`, which hands the input
/// to the elements' base `0x1592bd0` unless the button is disabled.
const BUTTON_INPUT: usize = 0x15aefc0;

/// The buttons the framework's subscriptions take right clicks of
/// ([`Subscription::link`]): on them a quick second right press is a press
/// ([`button_input`]).
static TAKING_RIGHT: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// A callback the framework subscribes to a button's signal (see
/// [`callback`]).
pub(crate) type Callback = Box<[usize; callback::SIZE / 8]>;

struct Declared {
    element: Element,
    host: Host,
    /// Index of the extension that receives this element's clicks.
    extension: Option<usize>,
}

/// The elements spliced this run and what their extensions want of them,
/// apart from the windows that show them: what the hosts' updates apply.
struct Elements {
    declared: Vec<Declared>,
    /// The windows whose button area has the framework's frame.
    frames: Vec<Host>,
    hosted: HostedElements,
}

impl Default for Elements {
    fn default() -> Self {
        Self { declared: Vec::new(), frames: Vec::new(), hosted: HostedElements::strict() }
    }
}

impl Elements {
    /// The elements spliced this run, with the extension that owns each.
    fn declare(&mut self, elements: Vec<(Element, Option<usize>)>, frames: Vec<Host>) {
        self.hosted.clear();
        let mut by_extension: BTreeMap<usize, Vec<(String, bool)>> = BTreeMap::new();
        self.declared = elements
            .into_iter()
            .map(|(element, extension)| {
                if let Some(ext) = extension {
                    by_extension.entry(ext).or_default().push((element.name.clone(), element.kind.is_button()));
                }
                Declared { host: element.host, element, extension }
            })
            .collect();
        for (ext, items) in by_extension {
            self.hosted.declare(ext, items);
        }
        self.frames = frames;
    }

    fn shown_for(&self, declared: &Declared, resolve_text: &impl Fn(&str, &eu4_harmony_lua::Text) -> String) -> Shown {
        let Some(extension) = declared.extension else { return Shown::default() };
        let owner = super::mod_id(extension).unwrap_or_default();
        let Some(state) = self.hosted.get(extension, &declared.element.name) else {
            return Shown { tooltip: Some(Vec::new()), ..Shown::default() };
        };
        Shown {
            visible: state.visible,
            enabled: state.enabled,
            text: state.text.as_ref().map(|text| crate::text::unicode::game_text::from_str(&resolve_text(&owner, text))),
            tooltip: Some(crate::text::unicode::game_text::from_str(&resolve_text(&owner, &state.tooltip))),
            ..Shown::default()
        }
    }
}

/// The framework's listener on a button's click signal: the callback the
/// game calls back through, which must outlive the subscription, and what
/// the button reported before. Extension windows' buttons subscribe so too
/// (`crate::interface::windows`).
pub(crate) struct Subscription {
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
    pub(crate) fn new(m: &Memory, object: usize, left: usize, right: usize) -> Result<Self> {
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
    pub(crate) fn context(&self) -> usize {
        self.wrapper.as_ptr() as usize
    }

    /// Whether it is subscribed.
    pub(crate) fn linked(&self) -> bool {
        self.linked
    }

    #[cfg(test)]
    pub(crate) fn for_test(object: usize) -> Self {
        Self { object, signal: 0, wrapper: Box::new([0; callback::SIZE / 8]), linked: false, old_right_click: 0, old_double_click: 0 }
    }

    #[cfg(test)]
    pub(crate) fn set_linked_for_test(&mut self, linked: bool) {
        self.linked = linked;
    }

    pub(crate) fn subscribed(&self, m: &Memory) -> Result<bool> {
        Ok(crate::native::containers::items(m, self.signal + signal::LISTENERS, 512)?.contains(&self.context()))
    }

    /// Subscribes it, and has the button report right and double clicks.
    pub(crate) fn link(&mut self, m: &Memory) -> Result<()> {
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
    pub(crate) fn unlink(&mut self, m: &Memory) -> Result<()> {
        TAKING_RIGHT.lock().unwrap_or_else(|e| e.into_inner()).retain(|&b| b != self.object);
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

struct Bound {
    /// Its index among the [`Elements`] declared.
    index: usize,
    element: BoundElement,
}

struct Binding {
    host: Host,
    /// The game object that owns the root: the army panel, the top bar
    /// object, or the main menu.
    holder: usize,
    root: usize,
    bound: Vec<Bound>,
    /// The frame under its button area, which shows while any of the
    /// area's buttons and icons does.
    frame: Option<usize>,
}

/// Which button clicked a mod's button.
pub const CLICK_LEFT: u32 = 1;
pub const CLICK_RIGHT: u32 = 2;
/// The keys held down with a click, as bits.
pub const MODIFIER_SHIFT: u32 = 1;
pub const MODIFIER_CTRL: u32 = 2;

pub struct Click {
    pub extension: usize,
    pub name: String,
    pub click: u32,
    pub modifiers: u32,
}

#[derive(Default)]
struct Ui {
    elements: Elements,
    bindings: Vec<Binding>,
    /// Wrappers of detached bindings; the game may still hold pointers to them.
    retired: Vec<Callback>,
    clicks: Vec<Click>,
    reported: HashSet<String>,
    /// Set when a binding could not be detached safely; binding stops.
    fatal: Option<String>,
}

/// Code that holds this calls the game: to look elements up, subscribe and
/// unsubscribe, show, hide, enable and set text. What the game calls back
/// with takes it only with `try_lock` (tooltips; clicks are only queued,
/// `crate::native::bound_element`), except the
/// destruction hooks, which must detach; none of those calls destroys a
/// panel, button or window, so a destruction hook never runs under the lock.
static UI: Mutex<Option<Ui>> = Mutex::new(None);

fn with_ui<T>(f: impl FnOnce(&mut Ui) -> T) -> T {
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Ui::default))
}

fn report_once(reported: &mut HashSet<String>, message: String) {
    if reported.insert(message.clone()) {
        log!("ui: {message}");
    }
}

/// Elements spliced this run, with the extension index that owns each, and
/// the windows whose button area has a frame.
pub fn set_elements(elements: Vec<(Element, Option<usize>)>, frames: Vec<Host>) {
    with_ui(|ui| ui.elements.declare(elements, frames));
}

/// The element of `name` in the window at `root`, through its method at
/// `offset`, which must be `function`.
pub(crate) fn lookup(m: &Memory, root: usize, (offset, function): (usize, GameFunction<2>), name: &str) -> Result<usize> {
    if m.vslot(root, offset)? != m.at(function) {
        return Err("window has an unexpected lookup method".into());
    }
    let c = format!("{name}\0");
    function.call(m, [root, c.as_ptr() as usize])
}

/// Name an element reports, through the getter shared by all GUI element classes.
pub(crate) fn element_name(m: &Memory, object: usize) -> Result<Vec<u8>> {
    if m.vslot(object, element::NAME)? != m.at(functions::ELEMENT_NAME) {
        return Err("element has an unexpected name getter".into());
    }
    game_string::read(m, functions::ELEMENT_NAME.call(m, [object])?)
}

trait HostNative {
    fn check_window_kill(&self, root: usize) -> Result<()>;
    fn bind_frame(&self, root: usize) -> Result<usize>;
    fn bind_element(&self, root: usize, declared: &Element) -> Result<Option<BoundElement>>;
    fn link(&mut self, element: &mut BoundElement) -> Result<()>;
    fn unlink(&mut self, element: &mut BoundElement) -> Result<()>;
    fn attach_ordering(&mut self, host: Host, root: usize, frame: Option<usize>, controls: &[(usize, Look)]) -> Result<()>;
    fn detach_ordering(&mut self, root: usize);
    fn apply_element(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()>;
    fn element_visible(&self, element: &BoundElement) -> Result<bool>;
    fn frame_open(&self, frame: usize) -> Result<bool>;
    fn show_frame(&mut self, frame: usize, open: bool) -> Result<()>;
    fn resolve_text(&self, owner: &str, text: &eu4_harmony_lua::Text) -> String;
    fn take_clicks(&mut self) -> Vec<bound_element::Clicked>;
    fn fall_back_notice(&mut self, message: &str);
}

struct GameHost<'a>(&'a Memory);

impl HostNative for GameHost<'_> {
    fn check_window_kill(&self, root: usize) -> Result<()> {
        if self.0.vslot(root, window::KILL)? != self.0.at(WINDOW_KILL) {
            return Err("window has an unexpected kill method".into());
        }
        Ok(())
    }

    fn bind_frame(&self, root: usize) -> Result<usize> {
        bind_frame(self.0, root)
    }

    fn bind_element(&self, root: usize, declared: &Element) -> Result<Option<BoundElement>> {
        bind_one(self.0, root, declared)
    }

    fn link(&mut self, element: &mut BoundElement) -> Result<()> {
        element.link(self.0)
    }

    fn unlink(&mut self, element: &mut BoundElement) -> Result<()> {
        element.unlink(self.0)
    }

    fn attach_ordering(&mut self, host: Host, root: usize, frame: Option<usize>, controls: &[(usize, Look)]) -> Result<()> {
        crate::interface::window_ordering::attach(self.0, host, root, frame, controls)
    }

    fn detach_ordering(&mut self, root: usize) {
        crate::interface::window_ordering::detach_host(root);
    }

    fn apply_element(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()> {
        element.apply(self.0, shown)
    }

    fn element_visible(&self, element: &BoundElement) -> Result<bool> {
        element.visible(self.0)
    }

    fn frame_open(&self, frame: usize) -> Result<bool> {
        Ok(self.0.u8(frame + window::OPEN)? != 0)
    }

    fn show_frame(&mut self, frame: usize, open: bool) -> Result<()> {
        crate::interface::windows::show_window(self.0, frame, open)
    }

    fn resolve_text(&self, owner: &str, text: &eu4_harmony_lua::Text) -> String {
        text.resolve(|key| crate::base::loc::mod_text(owner, key))
    }

    fn take_clicks(&mut self) -> Vec<bound_element::Clicked> {
        bound_element::take_clicks(Hears::Panels)
    }

    fn fall_back_notice(&mut self, message: &str) {
        notice::fall_back(message);
    }
}

fn bind_one(m: &Memory, root: usize, element: &Element) -> Result<Option<BoundElement>> {
    let method = match element.kind {
        ElementKind::Button => (window::FIND_BUTTON, functions::FIND_BUTTON),
        ElementKind::Text => (window::FIND_TEXT, functions::FIND_TEXT),
        ElementKind::Icon => (window::FIND_ICON, functions::FIND_ICON),
        ElementKind::Other => return Ok(None),
    };
    let object = lookup(m, root, method, &element.prefixed)?;
    if object == 0 {
        return Err(format!("{} not found in {:?}", element.prefixed, element.window));
    }
    let vt = m.ptr(object)?;
    let expected_vt = match element.kind {
        ElementKind::Button => Some(classes::BUTTON),
        ElementKind::Text => Some(classes::TEXT_BOX),
        _ => None,
    };
    if expected_vt.is_some_and(|e| vt != m.at(e)) {
        return Err(format!("{} has an unexpected element class", element.prefixed));
    }
    // A lookup miss can return a shared fallback object; only accept our own name.
    if element_name(m, object)? != element.prefixed.as_bytes() {
        return Err(format!("{} lookup returned another element", element.prefixed));
    }
    let bound = match element.kind {
        ElementKind::Button => BoundElement::listening(m, object, Look::Button, Hears::Panels).map_err(|e| format!("{} has an {e}", element.prefixed))?,
        ElementKind::Text => BoundElement::new(object, Look::Text),
        _ => BoundElement::new(object, Look::Icon),
    };
    Ok(Some(bound))
}

impl Ui {
    /// Detaches every binding `which` selects; their objects must still be alive.
    /// The clicks they heard are taken up first.
    fn detach(&mut self, native: &mut impl HostNative, which: impl Fn(&Binding) -> bool) -> Result<()> {
        self.hear_clicks(native);
        let (gone, kept) = std::mem::take(&mut self.bindings).into_iter().partition(|b| which(b));
        self.bindings = kept;
        let mut result = Ok(());
        for mut binding in gone {
            native.detach_ordering(binding.root);
            let unlinked = binding.bound.iter_mut().try_for_each(|b| native.unlink(&mut b.element));
            self.retired.extend(binding.bound.into_iter().filter_map(|b| b.element.retire()));
            result = result.and(unlinked);
        }
        result
    }

    fn bind(&mut self, native: &mut impl HostNative, host: Host, holder: usize, root: usize) -> Result<()> {
        if self.bindings.iter().any(|b| b.host == host && b.holder == holder && b.root == root) {
            return Ok(());
        }
        self.detach(native, |b| b.host == host)?;
        if root == 0 || !self.elements.declared.iter().any(|d| d.host == host) {
            return Ok(());
        }
        // Teardown is noticed through the kill hook, so it must be this window's.
        native.check_window_kill(root).map_err(|e| format!("{host:?} {e}"))?;
        let mut binding = Binding { host, holder, root, bound: Vec::new(), frame: None };
        if self.elements.frames.contains(&host) {
            match native.bind_frame(root) {
                Ok(frame) => binding.frame = Some(frame),
                Err(e) => report_once(&mut self.reported, format!("cannot bind the button area's frame: {e}")),
            }
        }
        for (index, d) in self.elements.declared.iter().enumerate().filter(|(_, d)| d.host == host) {
            match native.bind_element(root, &d.element) {
                Ok(Some(element)) => binding.bound.push(Bound { index, element }),
                Ok(None) => {}
                Err(e) => report_once(&mut self.reported, format!("cannot bind {}: {e}", d.element.prefixed)),
            }
        }
        // Keep every wrapper reachable before the first subscription; if a later
        // button fails to link, detach and retire the partial binding rather than
        // leaving a half-linked entry in `self.bindings` that blocks future retries.
        self.bindings.push(binding);
        let link_result = (|| -> Result<()> {
            let binding = self.bindings.last_mut().unwrap();
            for b in &mut binding.bound {
                native.link(&mut b.element)?;
            }
            let controls = binding
                .bound
                .iter()
                .map(|bound| {
                    let look = match self.elements.declared[bound.index].element.kind {
                        ElementKind::Button => Look::Button,
                        ElementKind::Text => Look::Text,
                        ElementKind::Icon => Look::Icon,
                        ElementKind::Other => Look::Plain,
                    };
                    (bound.element.object, look)
                })
                .collect::<Vec<_>>();
            native.attach_ordering(host, binding.root, binding.frame, &controls)
        })();
        if let Err(e) = link_result {
            if let Some(failed) = self.bindings.pop() {
                native.detach_ordering(failed.root);
                for mut b in failed.bound {
                    let _ = native.unlink(&mut b.element);
                    self.retired.extend(b.element.retire());
                }
            }
            return Err(e);
        }
        let bound_count = self.bindings.last().map_or(0, |b| b.bound.len());
        log!("ui: bound {bound_count} element(s) in {host:?} {holder:#x}");
        Ok(())
    }

    fn apply(&mut self, native: &mut impl HostNative, host: Host) -> Result<()> {
        let Some(binding) = self.bindings.iter_mut().find(|b| b.host == host) else { return Ok(()) };
        for b in &mut binding.bound {
            let declared = &self.elements.declared[b.index];
            let desired = self.elements.shown_for(declared, &|owner, text| native.resolve_text(owner, text));
            native.apply_element(&mut b.element, &desired)?;
        }
        if let Some(frame) = binding.frame {
            let mut any = false;
            for b in &binding.bound {
                let d = &self.elements.declared[b.index];
                if d.element.in_area && matches!(d.element.kind, ElementKind::Button | ElementKind::Icon) && native.element_visible(&b.element)? {
                    any = true;
                    break;
                }
            }
            if native.frame_open(frame)? != any {
                native.show_frame(frame, any)?;
            }
        }
        Ok(())
    }

    fn retire(&mut self, native: &mut impl HostNative, which: impl Fn(&Binding) -> bool) {
        if !self.bindings.iter().any(&which) {
            return;
        }
        if let Err(e) = self.detach(native, which) {
            log!("ui: binding could not be detached, UI binding stops: {e}");
            self.fatal = Some(e.clone());
            native.fall_back_notice(&format!("UI binding stopped: {e}"));
        }
    }

    /// The clicks the bound buttons heard, as what their extensions hear; a
    /// click of a button no longer bound or no longer pressable is let go.
    fn hear_clicks(&mut self, native: &mut impl HostNative) {
        for clicked in native.take_clicks() {
            let Some(b) = self.bindings.iter().flat_map(|binding| &binding.bound).find(|b| b.element.heard(&clicked)) else { continue };
            let d = &self.elements.declared[b.index];
            let Some(extension) = d.extension else { continue };
            if self.elements.hosted.check_press(extension, &d.element.name).is_err() {
                continue;
            }
            let click = if clicked.right { CLICK_RIGHT } else { CLICK_LEFT };
            if self.clicks.len() >= 64 {
                break;
            }
            self.clicks.push(Click { extension, name: d.element.name.clone(), click, modifiers: clicked.modifiers });
        }
    }
}

/// The frame under the button area of the window at `root`, a subwindow of
/// it, checked to be a window of the kind the framework knows.
fn bind_frame(m: &Memory, root: usize) -> Result<usize> {
    if m.vslot(root, window::FIND_WINDOW)? != m.at(functions::FIND_WINDOW) {
        return Err("the window's subwindow lookup is not the one the framework knows".into());
    }
    let frame = crate::interface::windows::with_string(m, crate::native::splice::FRAME.as_bytes(), |name| functions::FIND_WINDOW.call(m, [root, name, 0]))?;
    if frame == 0 || m.ptr(frame)? != m.at(classes::WINDOW) {
        return Err("not found".into());
    }
    keep_still(m, frame, crate::native::splice::FRAME_BACKGROUND)?;
    Ok(frame)
}

/// Disables the background of the window at `window`, the button `name`,
/// which a window's background has to be: it still takes the clicks on it,
/// so that they do not reach the map, but it cannot be pressed or dragged.
/// The fixed background sprite keeps the disabled state's original colors
/// (docs/game/interface/extension-windows.md,
/// "A press on a window's background").
pub(crate) fn keep_still(m: &Memory, window: usize, name: &str) -> Result<()> {
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

fn panel_after(call: &mut Call) {
    let m = call.memory();
    if call.faulted() || crate::base::thread_id() != crate::base::main_thread() || !super::in_game(&m) {
        return;
    }
    let panel = call.arg(0);
    with_ui(|ui| {
        if ui.fatal.is_some() {
            return;
        }
        let mut native = GameHost(&m);
        let result = (|| -> Result<()> {
            if m.ptr(panel)? != m.at(classes::ARMY_PANEL) {
                return Err("unexpected army panel class".into());
            }
            let root = m.ptr(panel + army_panel::WINDOW)?;
            ui.bind(&mut native, Host::ArmyPanel, panel, root)?;
            ui.apply(&mut native, Host::ArmyPanel)
        })();
        if let Err(e) = result {
            report_once(&mut ui.reported, e);
        }
    });
}

/// Detaches the bindings `which` selects while their objects are still alive.
fn retire(m: Memory, which: impl Fn(&Binding) -> bool) {
    with_ui(|ui| ui.retire(&mut GameHost(&m), which));
}

/// Whether the main menu shows its first page ([`main_menu::PAGE`];
/// 0x10daf60 switches pages).
fn main_menu_page(m: &Memory, frontend: usize) -> Result<bool> {
    if m.ptr(frontend)? != m.at(classes::MAIN_MENU) {
        return Err("unexpected main menu object".into());
    }
    Ok(m.u32(frontend + main_menu::PAGE)? == 0)
}

fn menu_after(call: &mut Call) {
    let m = call.memory();
    if call.faulted() || crate::base::thread_id() != crate::base::main_thread() {
        return;
    }
    crate::text::unicode::log_once();
    let show_corner = match main_menu_page(&m, call.arg(0)) {
        Ok(page) => page,
        // The corner button then shows on every page of the main menu.
        Err(e) => {
            with_ui(|ui| report_once(&mut ui.reported, format!("main menu page unknown: {e}")));
            true
        }
    };
    super::on_main_menu_frame(call.arg(0), show_corner);
}

fn panel_destroy(call: &mut Call) {
    let panel = call.arg(0);
    retire(call.memory(), |b| b.host == Host::ArmyPanel && b.holder == panel);
}
fn panel_recreate(call: &mut Call) {
    let panel = call.arg(0).checked_sub(army_panel::INTERFACE);
    retire(call.memory(), |b| b.host == Host::ArmyPanel && Some(b.holder) == panel);
}
fn button_destroy(call: &mut Call) {
    let button = call.arg(0);
    TAKING_RIGHT.lock().unwrap_or_else(|e| e.into_inner()).retain(|&b| b != button);
    retire(call.memory(), |b| b.bound.iter().any(|x| x.element.object == button));
}
/// Every teardown path of a root window passes here first (notice-window.md §3).
fn window_kill(call: &mut Call) {
    let window = call.arg(0);
    crate::interface::window_ordering::retire(window);
    retire(call.memory(), |b| b.root == window);
    crate::interface::windows::killed(&call.memory(), window);
}
/// Backstop for a root freed without passing the kill hook.
fn window_delete(call: &mut Call) {
    let window = call.arg(0);
    crate::interface::window_ordering::retire(window);
    retire(call.memory(), |b| b.root == window);
    crate::interface::windows::killed(&call.memory(), window);
}

/// The keys held down now, as [`MODIFIER_SHIFT`] and [`MODIFIER_CTRL`]: the
/// player's, as Windows has handed them to the game's window, and remote
/// control's, which `GetKeyState` does not see
/// ([`game::input::modifiers_held`]).
pub(crate) fn modifiers() -> u32 {
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

/// The actual native hit, for the shared presentation adapter.
fn tooltip_after(call: &mut Call) {
    if call.result() == Some(call.arg(1)) && crate::base::thread_id() == crate::base::main_thread() {
        crate::interface::tooltips::hit(call.arg(2));
    }
}

/// Every bound visual element participates, whether it listens for clicks or
/// not. An empty managed value still replaces the native content.
pub(crate) fn managed_tooltip(element: usize) -> Option<Vec<u8>> {
    let guard = UI.try_lock().ok()?;
    let ui = guard.as_ref()?;
    let bound = ui.bindings.iter().flat_map(|b| &b.bound).find(|b| b.element.object == element)?;
    Some(bound.element.tooltip().unwrap_or_default().to_vec())
}

/// The army panel, button and window functions the UI hooks or calls.
pub fn points() -> Vec<Point> {
    vec![
        Point::observe(PANEL_UPDATE, "48 89 5c 24 08 88 54 24 10 55 56 57 41 54 41 55", "army panel update", 2, None, Some(panel_after)),
        Point::observe(PANEL_DESTROY, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 41", "army panel destructor", 2, Some(panel_destroy), None),
        Point::observe(PANEL_RECREATE, "40 53 48 83 ec 40 48 8b d9 48 8b 05 e8 f8 ee 00", "army panel rebuild", 1, Some(panel_recreate), None),
        Point::observe(BUTTON_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8", "button destructor", 2, Some(button_destroy), None),
        Point::observe(BUTTON_INPUT, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b d9 49 8b f8 48 8b 89", "button input", 3, Some(button_input), None),
        Point::method(classes::BUTTON, element::INPUT, BUTTON_INPUT, "button input method"),
        // The base reads what happened, the device and the mouse button,
        // and drops a right double click (0x1592da5: neither 3 nor 4).
        Point::keep(0x1592c33usize, "8b 4e 50 83 f8 03 75 7c 85 c9 75 3e 39 46 14", "element input's device and kind")
            .uses(&[mouse_input::DEVICE, mouse_input::KIND]),
        Point::keep(0x1592d23usize, "8b 4e 18 83 f9 01 75 7a 83 f8 05", "element input's mouse button").uses(&[mouse_input::BUTTON]),
        Point::keep(0x1592da5usize, "83 f8 03 75 7f 83 f9 02 0f 85 03 01 00 00", "element input's right press"),
        Point::observe(
            WINDOW_KILL,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 c6 81 f4 00 00 00 01",
            "window kill",
            1,
            Some(window_kill),
            None,
        ),
        Point::observe(
            WINDOW_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 0c 02 00 00 f6 c3 01 74 0d ba 80 0a 00 00",
            "window destructor",
            2,
            Some(window_delete),
            None,
        ),
        Point::observe(MENU_FRAME, "48 8b c4 48 89 58 10 55 56 57 41 54 41 55 41 56", "main menu frame", 2, None, Some(menu_after)),
        Point::keep(0x10db037usize, "41 8b 8e 00 09 00 00 49 8b 86 e8 08 00 00 48 8b 0c c8 48 8b 01 ff 50 18 41 89 be 00 09 00 00", "main menu page switch")
            .uses(&[main_menu::PAGE]),
        Point::observe(TOOLTIP, "48 89 5c 24 08 48 89 74 24 10 55 57 41 54 41 56", "army panel tooltip", 3, None, Some(tooltip_after)),
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

pub fn take_clicks() -> Vec<Click> {
    with_ui(|ui| {
        if let Some(m) = crate::native::memory() {
            ui.hear_clicks(&mut GameHost(m));
        }
        std::mem::take(&mut ui.clicks)
    })
}

pub fn set_element(extension: usize, name: &str, value: eu4_harmony_lua::Element) -> Result<()> {
    with_ui(|ui| ui.elements.hosted.set(extension, name, value))
}

pub fn tooltip(extension: usize, name: &str) -> Result<eu4_harmony_lua::Text> {
    with_ui(|ui| ui.elements.hosted.tooltip(extension, name))
}

/// Forgets what a reloaded extension set on its elements: its new code sets
/// them again.
pub fn forget_extension(extension: usize) {
    with_ui(|ui| ui.elements.hosted.forget(extension));
}

/// Hides everything a stopped extension added.
pub fn hide_extension(extension: usize) {
    with_ui(|ui| ui.elements.hosted.hide(extension));
}

/// Outcome of reading the native army pointer shown in the open single-army panel.
pub(crate) enum SelectedArmyPanel {
    Open(usize),
    NotOpen,
}

/// The native army object shown in the open single-army panel, if open.
pub(crate) fn selected_army_ptr(m: &Memory) -> Result<SelectedArmyPanel> {
    let Some((panel, root)) = with_ui(|ui| ui.bindings.iter().find(|b| b.host == Host::ArmyPanel).map(|b| (b.holder, b.root))) else {
        return Ok(SelectedArmyPanel::NotOpen);
    };
    let ours = m.ptr(panel)? == m.at(classes::ARMY_PANEL) && m.ptr(panel + army_panel::WINDOW)? == root;
    if !ours || m.ptr(root)? != m.at(classes::WINDOW) || m.ptr(root + window::GUI)? != m.at(classes::WINDOW_GUI) {
        return Err("army panel changed".to_string());
    }
    if m.u8(root + window::OPEN)? != 1 {
        return Ok(SelectedArmyPanel::NotOpen);
    }
    Ok(SelectedArmyPanel::Open(m.ptr(panel + army_panel::ARMY)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn element(name: &str, kind: ElementKind, in_area: bool) -> Element {
        Element {
            owner: 0,
            window: "single_unitpanel".into(),
            host: Host::ArmyPanel,
            name: name.into(),
            prefixed: format!("eu4harmony_m_{name}"),
            kind,
            in_area,
        }
    }

    #[derive(Default)]
    struct RecordingHost {
        next_object: usize,
        objects: BTreeMap<String, usize>,
        linked: Vec<usize>,
        unlinked: Vec<usize>,
        attached_roots: Vec<(usize, Option<usize>, usize)>,
        detached_roots: Vec<usize>,
        frame_open: bool,
        frame_shows: Vec<bool>,
        fail_link_on: Option<usize>,
        fail_attach: Option<String>,
        fail_unlink_on: Option<usize>,
        clicks: Vec<bound_element::Clicked>,
        notices: Vec<String>,
        texts: BTreeMap<String, String>,
    }

    impl HostNative for RecordingHost {
        fn check_window_kill(&self, _root: usize) -> Result<()> {
            Ok(())
        }

        fn bind_frame(&self, root: usize) -> Result<usize> {
            Ok(root + 0x10)
        }

        fn bind_element(&self, _root: usize, declared: &Element) -> Result<Option<BoundElement>> {
            let object = *self.objects.get(&declared.prefixed).ok_or_else(|| format!("{} missing", declared.prefixed))?;
            let bound = match declared.kind {
                ElementKind::Button => BoundElement::test_listening(object, Look::Button),
                ElementKind::Text => BoundElement::new(object, Look::Text),
                ElementKind::Icon => BoundElement::new(object, Look::Icon),
                ElementKind::Other => return Ok(None),
            };
            Ok(Some(bound))
        }

        fn link(&mut self, element: &mut BoundElement) -> Result<()> {
            if self.fail_link_on == Some(element.object) {
                return Err(format!("link failed at {:#x}", element.object));
            }
            element.set_linked_for_test(true);
            self.linked.push(element.object);
            Ok(())
        }

        fn unlink(&mut self, element: &mut BoundElement) -> Result<()> {
            if self.fail_unlink_on == Some(element.object) {
                return Err(format!("unlink failed at {:#x}", element.object));
            }
            element.set_linked_for_test(false);
            self.unlinked.push(element.object);
            Ok(())
        }

        fn attach_ordering(&mut self, _host: Host, root: usize, frame: Option<usize>, controls: &[(usize, Look)]) -> Result<()> {
            if let Some(error) = self.fail_attach.take() {
                return Err(error);
            }
            self.attached_roots.push((root, frame, controls.len()));
            Ok(())
        }

        fn detach_ordering(&mut self, root: usize) {
            self.detached_roots.push(root);
        }

        fn apply_element(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()> {
            element.record_for_test(shown);
            Ok(())
        }

        fn element_visible(&self, element: &BoundElement) -> Result<bool> {
            Ok(element.wanted_visible())
        }

        fn frame_open(&self, _frame: usize) -> Result<bool> {
            Ok(self.frame_open)
        }

        fn show_frame(&mut self, _frame: usize, open: bool) -> Result<()> {
            self.frame_open = open;
            self.frame_shows.push(open);
            Ok(())
        }

        fn resolve_text(&self, _owner: &str, text: &eu4_harmony_lua::Text) -> String {
            text.resolve(|key| self.texts.get(key).cloned())
        }

        fn take_clicks(&mut self) -> Vec<bound_element::Clicked> {
            std::mem::take(&mut self.clicks)
        }

        fn fall_back_notice(&mut self, message: &str) {
            self.notices.push(message.to_string());
        }
    }

    impl RecordingHost {
        fn with_elements(names: &[&str]) -> Self {
            let mut host = Self { next_object: 0x1000, ..Default::default() };
            for name in names {
                let obj = host.next_object;
                host.next_object += 0x100;
                host.objects.insert(format!("eu4harmony_m_{name}"), obj);
            }
            host
        }
    }

    #[test]
    fn an_extension_changes_only_its_own_elements() {
        let mut elements = Elements::default();
        elements.declare(vec![(element("go", ElementKind::Button, true), Some(0)), (element("label", ElementKind::Text, false), Some(1))], Vec::new());
        elements.hosted.set(0, "go", eu4_harmony_lua::Element::Enabled(false)).unwrap();
        assert!(elements.hosted.set(0, "label", eu4_harmony_lua::Element::Visible(true)).is_err(), "extension 1's");
        assert!(elements.hosted.set(0, "stop", eu4_harmony_lua::Element::Visible(true)).is_err(), "none declared");
        assert_eq!(elements.hosted.get(0, "go").unwrap().enabled, Some(false));
        elements.hosted.forget(0);
        assert!(elements.hosted.get(0, "go").is_none());
        elements.hosted.hide(1);
        assert_eq!((elements.hosted.get(0, "go").and_then(|s| s.visible), elements.hosted.get(1, "label").and_then(|s| s.visible)), (None, Some(false)));
    }

    #[test]
    fn host_binding_lifecycle_recovers_from_partial_failures_and_tracks_frame_and_clicks() {
        let mut ui = Ui::default();
        ui.elements.declare(
            vec![
                (element("go", ElementKind::Button, true), Some(0)),
                (element("glow", ElementKind::Icon, true), Some(0)),
                (element("note", ElementKind::Text, false), Some(0)),
            ],
            vec![Host::ArmyPanel],
        );

        let mut host = RecordingHost::with_elements(&["go", "glow", "note"]);
        let go_obj = host.objects["eu4harmony_m_go"];
        let glow_obj = host.objects["eu4harmony_m_glow"];
        let note_obj = host.objects["eu4harmony_m_note"];
        // Failure during attach rolls back ordering and all bound elements, leaving bindings empty for retry.
        host.fail_attach = Some("ordering busy".into());
        assert_eq!(ui.bind(&mut host, Host::ArmyPanel, 0x500, 0x600).unwrap_err(), "ordering busy");
        assert!(ui.bindings.is_empty());
        assert_eq!(host.detached_roots, [0x600]);
        assert_eq!(host.unlinked, [go_obj, glow_obj, note_obj]);
        assert_eq!(ui.retired.len(), 1);

        // Next update succeeds cleanly and is idempotent on identical (holder, root).
        ui.bind(&mut host, Host::ArmyPanel, 0x500, 0x600).unwrap();
        assert_eq!(ui.bindings.len(), 1);
        ui.bind(&mut host, Host::ArmyPanel, 0x500, 0x600).unwrap();
        assert_eq!(host.attached_roots.len(), 1);

        // Frame visibility follows area buttons/icons while excluding non-area elements.
        ui.apply(&mut host, Host::ArmyPanel).unwrap();
        assert!(host.frame_open);
        ui.elements.hosted.set(0, "go", eu4_harmony_lua::Element::Visible(false)).unwrap();
        ui.elements.hosted.set(0, "glow", eu4_harmony_lua::Element::Visible(false)).unwrap();
        ui.apply(&mut host, Host::ArmyPanel).unwrap();
        assert!(!host.frame_open);
        ui.elements.hosted.set(0, "go", eu4_harmony_lua::Element::Visible(true)).unwrap();
        ui.apply(&mut host, Host::ArmyPanel).unwrap();
        assert!(host.frame_open);

        // Queued click is dropped when the extension disables or hides the element before take_clicks.
        let ctx = ui.bindings[0].bound[0].element.context_for_test().unwrap();
        host.clicks.push(bound_element::Clicked { context: ctx, button: go_obj, right: false, modifiers: MODIFIER_SHIFT });
        ui.elements.hosted.set(0, "go", eu4_harmony_lua::Element::Enabled(false)).unwrap();
        ui.hear_clicks(&mut host);
        assert!(ui.clicks.is_empty(), "queued click must not outlive disabling");

        ui.elements.hosted.set(0, "go", eu4_harmony_lua::Element::Enabled(true)).unwrap();
        host.clicks.push(bound_element::Clicked { context: ctx, button: go_obj, right: true, modifiers: MODIFIER_CTRL });
        ui.hear_clicks(&mut host);
        assert_eq!(ui.clicks.len(), 1);
        assert_eq!((ui.clicks[0].extension, ui.clicks[0].name.as_str(), ui.clicks[0].click, ui.clicks[0].modifiers), (0, "go", CLICK_RIGHT, MODIFIER_CTRL));

        // Failing to unlink on retire latches fatal and triggers notice fallback.
        host.fail_unlink_on = Some(go_obj);
        ui.retire(&mut host, |b| b.root == 0x600);
        assert!(ui.fatal.as_ref().is_some_and(|e| e.contains("unlink failed")));
        assert_eq!(host.notices.len(), 1);
    }

    #[test]
    fn subscriptions_link_unlink_restore_click_flags_and_drop_taking_right_even_on_dispatch_failure() {
        use crate::game::buffers::{Image, Object};
        use std::cell::Cell;

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
