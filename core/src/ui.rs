//! Binds spliced elements in the vanilla windows that host them, applies the
//! state extensions ask for, and turns button clicks into signals. Ported from
//! eu4-regional-combat's native UI; the ABI evidence is in
//! docs/research/interface-bindings.md (tooltips included). Window teardown
//! and the main menu's windows are in docs/research/notice-window.md.
//!
//! Extensions state what they want (visible, enabled, text, tooltip) and each
//! host's update hook applies it, so state survives the game rebuilding the
//! window and never needs a callback inside the game's UI code. How a bound
//! element is set and heard is `crate::bound_element`'s, which extension
//! windows share.
use crate::{
    bound_element::{self, BoundElement, Hears, Look, Shown},
    game::{self, fail, GResult, Status},
    game_string,
    hooks::Call,
    hosts::Host,
    memory::Memory,
    notice,
    objects::{
        campaign::unit,
        gui::{army_panel, button, callback, classes, element, functions, main_menu, mouse_input, signal, window},
        list,
    },
    points::{GameFunction, Point},
    splice::{Element, ElementKind},
    Result,
};
use std::collections::HashSet;
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
    desired: Shown,
    text: Option<eu4_harmony_lua::Text>,
    tooltip: eu4_harmony_lua::Text,
}

/// The elements spliced this run and what their extensions want of them,
/// apart from the windows that show them: what the hosts' updates apply.
#[derive(Default)]
struct Elements {
    declared: Vec<Declared>,
    /// The windows whose button area has the framework's frame.
    frames: Vec<Host>,
}

impl Elements {
    /// The elements spliced this run, with the extension that owns each.
    fn declare(&mut self, elements: Vec<(Element, Option<usize>)>, frames: Vec<Host>) {
        self.declared = elements
            .into_iter()
            .map(|(element, extension)| Declared { host: element.host, element, extension, desired: Shown::default(), text: None, tooltip: Default::default() })
            .collect();
        self.frames = frames;
    }

    /// Changes what `extension` wants of its element `name`.
    fn want(&mut self, extension: usize, name: &str, change: impl FnOnce(&mut Shown)) -> GResult<()> {
        match self.declared.iter_mut().find(|d| d.extension == Some(extension) && d.element.name == name) {
            Some(d) => {
                change(&mut d.desired);
                Ok(())
            }
            None => fail(Status::NotFound, format!("no element {name:?} declared by this mod")),
        }
    }

    /// Forgets what `extension` wanted of its elements.
    fn forget(&mut self, extension: usize) {
        for d in self.declared.iter_mut().filter(|d| d.extension == Some(extension)) {
            d.desired = Shown::default();
            d.text = None;
            d.tooltip = Default::default();
        }
    }

    /// Hides every element of `extension`'s.
    fn hide(&mut self, extension: usize) {
        for d in self.declared.iter_mut().filter(|d| d.extension == Some(extension)) {
            d.desired.visible = Some(false);
        }
    }
}

/// The framework's listener on a button's click signal: the callback the
/// game calls back through, which must outlive the subscription, and what
/// the button reported before. Extension windows' buttons subscribe so too
/// (`crate::windows`).
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

    pub(crate) fn subscribed(&self, m: &Memory) -> Result<bool> {
        Ok(crate::containers::items(m, self.signal + signal::LISTENERS, 512)?.contains(&self.context()))
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
        TAKING_RIGHT.lock().unwrap_or_else(|e| e.into_inner()).retain(|&b| b != self.object);
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
/// `crate::bound_element`), except the
/// destruction hooks, which must detach; none of those calls destroys a
/// panel, button or window, so a destruction hook never runs under the lock.
static UI: Mutex<Option<Ui>> = Mutex::new(None);

fn with_ui<T>(f: impl FnOnce(&mut Ui) -> T) -> T {
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Ui::default))
}

fn report_once(ui: &mut Ui, message: String) {
    if ui.reported.insert(message.clone()) {
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

fn bind_one(m: &Memory, root: usize, index: usize, d: &Declared) -> Result<Option<Bound>> {
    let method = match d.element.kind {
        ElementKind::Button => (window::FIND_BUTTON, functions::FIND_BUTTON),
        ElementKind::Text => (window::FIND_TEXT, functions::FIND_TEXT),
        ElementKind::Icon => (window::FIND_ICON, functions::FIND_ICON),
        ElementKind::Other => return Ok(None),
    };
    let object = lookup(m, root, method, &d.element.prefixed)?;
    if object == 0 {
        return Err(format!("{} not found in {:?}", d.element.prefixed, d.element.window));
    }
    let vt = m.ptr(object)?;
    let expected_vt = match d.element.kind {
        ElementKind::Button => Some(classes::BUTTON),
        ElementKind::Text => Some(classes::TEXT_BOX),
        _ => None,
    };
    if expected_vt.is_some_and(|e| vt != m.at(e)) {
        return Err(format!("{} has an unexpected element class", d.element.prefixed));
    }
    // A lookup miss can return a shared fallback object; only accept our own name.
    if element_name(m, object)? != d.element.prefixed.as_bytes() {
        return Err(format!("{} lookup returned another element", d.element.prefixed));
    }
    let element = match d.element.kind {
        ElementKind::Button => BoundElement::listening(m, object, Look::Button, Hears::Panels).map_err(|e| format!("{} has an {e}", d.element.prefixed))?,
        ElementKind::Text => BoundElement::new(object, Look::Text),
        _ => BoundElement::new(object, Look::Icon),
    };
    Ok(Some(Bound { index, element }))
}

/// Detaches every binding `which` selects; their objects must still be alive.
/// The clicks they heard are taken up first.
fn detach(m: &Memory, ui: &mut Ui, which: impl Fn(&Binding) -> bool) -> Result<()> {
    hear_clicks(ui);
    let (gone, kept) = std::mem::take(&mut ui.bindings).into_iter().partition(|b| which(b));
    ui.bindings = kept;
    let mut result = Ok(());
    for mut binding in gone {
        let unlinked = binding.bound.iter_mut().try_for_each(|b| b.element.unlink(m));
        ui.retired.extend(binding.bound.into_iter().filter_map(|b| b.element.retire()));
        result = result.and(unlinked);
    }
    result
}

fn bind(m: &Memory, ui: &mut Ui, host: Host, holder: usize, root: usize) -> Result<()> {
    if ui.bindings.iter().any(|b| b.host == host && b.holder == holder && b.root == root) {
        return Ok(());
    }
    detach(m, ui, |b| b.host == host)?;
    if root == 0 || !ui.elements.declared.iter().any(|d| d.host == host) {
        return Ok(());
    }
    // Teardown is noticed through the kill hook, so it must be this window's.
    if m.vslot(root, window::KILL)? != m.at(WINDOW_KILL) {
        return Err(format!("{host:?} window has an unexpected kill method"));
    }
    let mut binding = Binding { host, holder, root, bound: Vec::new(), frame: None };
    if ui.elements.frames.contains(&host) {
        match bind_frame(m, root) {
            Ok(frame) => binding.frame = Some(frame),
            Err(e) => {
                let message = format!("cannot bind the button area's frame: {e}");
                if ui.reported.insert(message.clone()) {
                    log!("ui: {message}");
                }
            }
        }
    }
    for (index, d) in ui.elements.declared.iter().enumerate().filter(|(_, d)| d.host == host) {
        match bind_one(m, root, index, d) {
            Ok(Some(b)) => binding.bound.push(b),
            Ok(None) => {}
            Err(e) => {
                let message = format!("cannot bind {}: {e}", d.element.prefixed);
                if ui.reported.insert(message.clone()) {
                    log!("ui: {message}");
                }
            }
        }
    }
    // Keep every wrapper reachable before the first subscription.
    ui.bindings.push(binding);
    let binding = ui.bindings.last_mut().unwrap();
    for b in &mut binding.bound {
        b.element.link(m)?;
    }
    log!("ui: bound {} element(s) in {host:?} {holder:#x}", binding.bound.len());
    Ok(())
}

/// The frame under the button area of the window at `root`, a subwindow of
/// it, checked to be a window of the kind the framework knows.
fn bind_frame(m: &Memory, root: usize) -> Result<usize> {
    if m.vslot(root, window::FIND_WINDOW)? != m.at(functions::FIND_WINDOW) {
        return Err("the window's subwindow lookup is not the one the framework knows".into());
    }
    let frame = crate::windows::with_string(m, crate::splice::FRAME.as_bytes(), |name| functions::FIND_WINDOW.call(m, [root, name, 0]))?;
    if frame == 0 || m.ptr(frame)? != m.at(classes::WINDOW) {
        return Err("not found".into());
    }
    keep_still(m, frame, crate::splice::FRAME_BACKGROUND)?;
    Ok(frame)
}

/// Disables the background of the window at `window`, the button `name`,
/// which a window's background has to be: it still takes the clicks on it,
/// so that they do not reach the map, but no press darkens it, as nothing
/// there is to be pressed or dragged (docs/research/extension-windows.md,
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

fn apply(m: &Memory, ui: &mut Ui, host: Host) -> Result<()> {
    let Some(binding) = ui.bindings.iter_mut().find(|b| b.host == host) else { return Ok(()) };
    for b in &mut binding.bound {
        let declared = &mut ui.elements.declared[b.index];
        let owner = declared.extension.and_then(crate::lua::mod_id).unwrap_or_default();
        if let Some(text) = &declared.text {
            declared.desired.text = Some(crate::unicode::game_text::from_str(&text.resolve(|key| crate::loc::mod_text(&owner, key))));
        }
        declared.desired.tooltip = Some(crate::unicode::game_text::from_str(&declared.tooltip.resolve(|key| crate::loc::mod_text(&owner, key))));
        b.element.apply(m, &declared.desired)?;
    }
    if let Some(frame) = binding.frame {
        let mut any = false;
        for b in &binding.bound {
            let d = &ui.elements.declared[b.index];
            if d.element.in_area && matches!(d.element.kind, ElementKind::Button | ElementKind::Icon) && b.element.visible(m)? {
                any = true;
                break;
            }
        }
        if (m.u8(frame + window::OPEN)? != 0) != any {
            crate::windows::show_window(m, frame, any)?;
        }
    }
    Ok(())
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
    if call.faulted() || crate::thread_id() != crate::main_thread() {
        return;
    }
    crate::safe_point::main_menu_frame(call.arg(0));
    crate::unicode::log_once();
    crate::loading_times::log_main_menu();
    match main_menu_page(&m, call.arg(0)) {
        Ok(true) => crate::framework_ui::main_menu_frame(),
        Ok(false) => {}
        // The corner button then shows on every page of the main menu.
        Err(e) => {
            with_ui(|ui| report_once(ui, format!("main menu page unknown: {e}")));
            crate::framework_ui::main_menu_frame();
        }
    }
}

fn panel_after(call: &mut Call) {
    let m = call.memory();
    if call.faulted() || crate::thread_id() != crate::main_thread() || game::context(&m).is_err() {
        return;
    }
    let panel = call.arg(0);
    with_ui(|ui| {
        if ui.fatal.is_some() {
            return;
        }
        let result = (|| -> Result<()> {
            if m.ptr(panel)? != m.at(classes::ARMY_PANEL) {
                return Err("unexpected army panel class".into());
            }
            bind(&m, ui, Host::ArmyPanel, panel, m.ptr(panel + army_panel::WINDOW)?)?;
            apply(&m, ui, Host::ArmyPanel)
        })();
        if let Err(e) = result {
            report_once(ui, e);
        }
    });
}

/// Detaches the bindings `which` selects while their objects are still alive.
fn retire(m: Memory, which: impl Fn(&Binding) -> bool) {
    with_ui(|ui| {
        if !ui.bindings.iter().any(&which) {
            return;
        }
        if let Err(e) = detach(&m, ui, which) {
            log!("ui: binding could not be detached, UI binding stops: {e}");
            ui.fatal = Some(e.clone());
            notice::fall_back(&format!("UI binding stopped: {e}"));
        }
    });
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
    retire(call.memory(), |b| b.root == window);
    crate::windows::killed(&call.memory(), window);
}
/// Backstop for a root freed without passing the kill hook.
fn window_delete(call: &mut Call) {
    let window = call.arg(0);
    retire(call.memory(), |b| b.root == window);
    crate::windows::killed(&call.memory(), window);
}

/// The keys held down now, as [`MODIFIER_SHIFT`] and [`MODIFIER_CTRL`]: the
/// player's, as Windows has handed them to the game's window, and remote
/// control's, which `GetKeyState` does not see
/// ([`game::input::modifiers_held`]).
pub(crate) fn modifiers() -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
    let down = |key: u16| unsafe { GetKeyState(key as i32) } < 0;
    let (shift, ctrl) = game::input::modifiers_held();
    let mut out = 0;
    if shift || down(VK_SHIFT) {
        out |= MODIFIER_SHIFT;
    }
    if ctrl || down(VK_CONTROL) {
        out |= MODIFIER_CTRL;
    }
    out
}

/// The clicks the bound buttons heard, as what their extensions hear; a
/// click of a button no longer bound is let go.
fn hear_clicks(ui: &mut Ui) {
    for clicked in bound_element::take_clicks(Hears::Panels) {
        let Some(b) = ui.bindings.iter().flat_map(|binding| &binding.bound).find(|b| b.element.heard(&clicked)) else { continue };
        let d = &ui.elements.declared[b.index];
        let Some(extension) = d.extension else { continue };
        let click = if clicked.right { CLICK_RIGHT } else { CLICK_LEFT };
        if ui.clicks.len() >= 64 {
            break;
        }
        ui.clicks.push(Click { extension, name: d.element.name.clone(), click, modifiers: clicked.modifiers });
    }
}

/// Before one of the framework's buttons takes the mouse's input: the game
/// takes a right press that soon follows another as a double click, and on
/// every button drops it, so that the second of two quick right clicks does
/// nothing (docs/research/interface-bindings.md, "From input to a click").
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
    if call.result() == Some(call.arg(1)) && crate::thread_id() == crate::main_thread() {
        crate::tooltips::hit(call.arg(2));
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
        hear_clicks(ui);
        std::mem::take(&mut ui.clicks)
    })
}

fn desired(extension: usize, name: &str, f: impl FnOnce(&mut Shown)) -> GResult<()> {
    with_ui(|ui| ui.elements.want(extension, name, f))
}

pub fn set_visible(extension: usize, name: &str, visible: bool) -> GResult<()> {
    desired(extension, name, |d| d.visible = Some(visible))
}
pub fn set_enabled(extension: usize, name: &str, enabled: bool) -> GResult<()> {
    desired(extension, name, |d| d.enabled = Some(enabled))
}
pub fn set_text(extension: usize, name: &str, text: eu4_harmony_lua::Text) -> GResult<()> {
    with_ui(|ui| match ui.elements.declared.iter_mut().find(|d| d.extension == Some(extension) && d.element.name == name) {
        Some(d) => {
            d.text = Some(text);
            Ok(())
        }
        None => fail(Status::NotFound, format!("no element {name:?} declared by this mod")),
    })
}
pub fn set_tooltip(extension: usize, name: &str, text: eu4_harmony_lua::Text) -> GResult<()> {
    with_ui(|ui| match ui.elements.declared.iter_mut().find(|d| d.extension == Some(extension) && d.element.name == name) {
        Some(d) => {
            d.tooltip = text;
            Ok(())
        }
        None => fail(Status::NotFound, format!("no element {name:?} declared by this mod")),
    })
}

pub fn tooltip(extension: usize, name: &str) -> std::result::Result<eu4_harmony_lua::Text, String> {
    with_ui(|ui| {
        ui.elements
            .declared
            .iter()
            .find(|d| d.extension == Some(extension) && d.element.name == name)
            .map(|d| d.tooltip.clone())
            .ok_or_else(|| format!("no element {name:?} declared by this mod"))
    })
}

/// Forgets what a reloaded extension set on its elements: its new code sets
/// them again.
pub fn forget_extension(extension: usize) {
    with_ui(|ui| ui.elements.forget(extension));
}

/// Hides everything a stopped extension added.
pub fn hide_extension(extension: usize) {
    with_ui(|ui| ui.elements.hide(extension));
}

/// The own land army shown in the open single-army panel.
pub fn selected_army(m: &Memory, ctx: &game::Ctx) -> GResult<u64> {
    let (panel, root) = with_ui(|ui| ui.bindings.iter().find(|b| b.host == Host::ArmyPanel).map(|b| (b.holder, b.root)))
        .ok_or_else(|| game::Fail::from(eu4_harmony_checks::armies::panel_not_open()))?;
    let ours = m.ptr(panel)? == m.at(classes::ARMY_PANEL) && m.ptr(panel + army_panel::WINDOW)? == root;
    if !ours || m.ptr(root)? != m.at(classes::WINDOW) || m.ptr(root + window::GUI)? != m.at(classes::WINDOW_GUI) {
        return Err("army panel changed".to_string().into());
    }
    if m.u8(root + window::OPEN)? != 1 {
        return Err(eu4_harmony_checks::armies::panel_not_open().into());
    }
    let army = m.ptr(panel + army_panel::ARMY)?;
    if !game::army::is_own_army(m, ctx.player, army)? {
        return Err(eu4_harmony_checks::armies::none_selected().into());
    }
    Ok(m.u64(army + unit::ID)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(name: &str, kind: ElementKind) -> Element {
        Element { owner: 0, window: "unitpanel".into(), host: Host::ArmyPanel, name: name.into(), prefixed: format!("mod_{name}"), kind, in_area: true }
    }

    #[test]
    fn an_extension_changes_only_its_own_elements() {
        let mut elements = Elements::default();
        elements.declare(vec![(element("go", ElementKind::Button), Some(0)), (element("label", ElementKind::Text), Some(1))], Vec::new());
        elements.want(0, "go", |d| d.enabled = Some(false)).unwrap();
        assert_eq!(elements.want(0, "label", |d| d.visible = Some(true)).unwrap_err().status, Status::NotFound, "extension 1's");
        assert_eq!(elements.want(0, "stop", |_| ()).unwrap_err().status, Status::NotFound, "none declared");
        assert_eq!(elements.declared[0].desired.enabled, Some(false));
        elements.forget(0);
        assert_eq!(elements.declared[0].desired.enabled, None);
        elements.hide(1);
        assert_eq!((elements.declared[0].desired.visible, elements.declared[1].desired.visible), (None, Some(false)));
    }
}
