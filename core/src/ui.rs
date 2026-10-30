//! Binds spliced elements in the vanilla windows that host them, applies the
//! state extensions ask for, and turns button clicks into events. Ported from
//! eu4-regional-combat's native UI; the ABI evidence is in
//! docs/research/regional-combat/native-{ui,settings}-findings.txt and
//! ui-polish-validation.md (tooltip aggregate). Window teardown and the main
//! menu's windows are in docs/research/notice-window.md.
//!
//! Extensions state what they want (visible, enabled, text, tooltip) and each
//! host's update hook applies it, so state survives the game rebuilding the
//! window and never needs a callback inside the game's UI code.
use crate::{
    game::{self, fail, GResult, Status},
    game_string,
    hooks::HookFrame,
    hosts::{self, Host},
    memory::Memory,
    notice,
    objects::{
        campaign::{self, unit},
        gui::{army_panel, button, callback, classes, element, event, functions, main_menu, signal, window},
        list,
    },
    points::Point,
    splice::{Element, ElementKind},
    Result,
};
use eu4_harmony_abi as abi;
use std::collections::HashSet;
use std::sync::Mutex;

const PANEL_UPDATE: usize = 0x1450660;
const PANEL_DESTROY: usize = 0x144f560;
const PANEL_RECREATE: usize = 0x1450580;
const BUTTON_DESTROY: usize = 0x10b5670;
/// A window's [`window::KILL`]: marks the window and its children dead;
/// memory is freed later in the frame by the GUI manager's sweep.
const WINDOW_KILL: usize = 0x1547520;
/// Root window deleting destructor, reached from that sweep.
const WINDOW_DELETE: usize = 0x1546510;
const MENU_FRAME: usize = 0x10d2e80;
const TOOLTIP: usize = 0x1453b70;
const TEXT_SET: usize = 0x152ae10;
const SUBSCRIBE: usize = 0xfbe3f0;
const UNSUBSCRIBE: usize = 0xfbe370;
const SET_RIGHT_CLICK: usize = 0x1593860;
const SHOW: usize = 0x15aeb30;
const HIDE: usize = 0x15aead0;
const SET_ENABLED: usize = 0xfaab30;

/// A callback the framework subscribes to a button's signal (see
/// [`callback`]).
type Callback = Box<[usize; callback::SIZE / 8]>;

#[derive(Default, Clone)]
struct Desired {
    visible: Option<bool>,
    enabled: Option<bool>,
    text: Option<Vec<u8>>,
    tooltip: Option<Vec<u8>>,
}

struct Declared {
    element: Element,
    host: Host,
    /// Index of the extension that receives this element's clicks.
    extension: Option<usize>,
    desired: Desired,
}

struct Bound {
    index: usize,
    object: usize,
    signal: usize,
    /// Fake callback wrapper the game calls back through; must outlive the subscription.
    wrapper: Option<Callback>,
    linked: bool,
    old_right_click: u8,
    applied_text: Option<Vec<u8>>,
    applied_enabled: Option<bool>,
}

struct Binding {
    host: Host,
    /// The game object that owns the root: the army panel, the top bar
    /// object, or the main menu.
    holder: usize,
    root: usize,
    bound: Vec<Bound>,
}

pub struct Click {
    pub extension: usize,
    pub name: String,
    pub click: u32,
    pub modifiers: u32,
}

#[derive(Default)]
struct Ui {
    declared: Vec<Declared>,
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
/// with takes it only with `try_lock` (clicks, tooltips), except the
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

/// Elements spliced this run, with the extension index that owns each.
pub fn set_elements(elements: Vec<(Element, Option<usize>)>) {
    with_ui(|ui| {
        ui.declared = elements
            .into_iter()
            .filter_map(|(element, extension)| match hosts::window(&element.window).map(|w| w.host) {
                Some(host) => Some(Declared { element, host, extension, desired: Desired::default() }),
                None => {
                    log!("ui: {} is in window {:?}, which is not bound", element.prefixed, element.window);
                    None
                }
            })
            .collect();
    });
}

/// The element of `name` in the window at `root`, through its method at
/// `offset`, which must be `function`.
fn lookup(m: &Memory, root: usize, (offset, function): (usize, usize), name: &str) -> Result<usize> {
    if m.vslot(root, offset)? != m.at(function) {
        return Err("window has an unexpected lookup method".into());
    }
    let c = format!("{name}\0");
    m.call(m.at(function), &[root, c.as_ptr() as usize])
}

/// Name an element reports, through the getter shared by all GUI element classes.
fn element_name(m: &Memory, object: usize) -> Result<Vec<u8>> {
    let getter = m.at(functions::ELEMENT_NAME);
    if m.vslot(object, element::NAME)? != getter {
        return Err("element has an unexpected name getter".into());
    }
    game_string::read(m, m.call(getter, &[object])?)
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
    let mut bound = Bound { index, object, signal: 0, wrapper: None, linked: false, old_right_click: 0, applied_text: None, applied_enabled: None };
    if d.element.kind == ElementKind::Button {
        let signal = object + button::SIGNAL;
        if m.ptr(signal)? != m.at(classes::SIGNAL)
            || m.ptr(signal + signal::AFTER_LISTENERS)? != 0
            || m.vslot(signal, signal::SUBSCRIBE)? != m.at(SUBSCRIBE)
            || m.vslot(signal, signal::UNSUBSCRIBE)? != m.at(UNSUBSCRIBE)
            || m.vslot(object, button::SET_RIGHT_CLICK)? != m.at(SET_RIGHT_CLICK)
        {
            return Err(format!("{} has an unexpected click signal", d.element.prefixed));
        }
        let mut wrapper: Callback = Box::new([0; callback::SIZE / 8]);
        wrapper[0] = m.at(classes::CALLBACK);
        wrapper[callback::CONTEXT / 8] = wrapper.as_ptr() as usize;
        wrapper[callback::RIGHT_CLICK / 8] = right_click as *const () as usize;
        wrapper[callback::LEFT_CLICK / 8] = left_click as *const () as usize;
        bound.signal = signal;
        bound.old_right_click = m.u8(object + button::RIGHT_CLICK)?;
        bound.wrapper = Some(wrapper);
    }
    Ok(Some(bound))
}

fn subscribed(m: &Memory, b: &Bound) -> Result<bool> {
    let Some(wrapper) = &b.wrapper else { return Ok(false) };
    let wrapper = wrapper.as_ptr() as usize;
    let mut node = m.ptr(b.signal + signal::LISTENERS + list::HEAD)?;
    for _ in 0..512 {
        if node == 0 {
            return Ok(false);
        }
        if m.ptr(node + list::ITEM)? == wrapper {
            return Ok(true);
        }
        node = m.ptr(node + list::NEXT)?;
    }
    Err("signal listener list too long".into())
}

/// Detaches every binding `which` selects; their objects must still be alive.
fn detach(m: &Memory, ui: &mut Ui, which: impl Fn(&Binding) -> bool) -> Result<()> {
    let (gone, kept) = std::mem::take(&mut ui.bindings).into_iter().partition(|b| which(b));
    ui.bindings = kept;
    let mut result = Ok(());
    for mut binding in gone {
        let unlinked = (|| {
            for b in &mut binding.bound {
                if !b.linked {
                    continue;
                }
                if m.ptr(b.signal)? != m.at(classes::SIGNAL) || m.u8(b.signal + signal::LISTENERS + list::CHANGING)? != 0 {
                    return Err("cannot unsubscribe from a changed or dispatching signal".to_string());
                }
                m.call(m.at(UNSUBSCRIBE), &[b.signal, b.wrapper.as_ref().unwrap().as_ptr() as usize])?;
                if subscribed(m, b)? {
                    return Err("observer still subscribed after unsubscribe".into());
                }
                b.linked = false;
                m.call(m.at(SET_RIGHT_CLICK), &[b.object, b.old_right_click as usize])?;
            }
            Ok(())
        })();
        ui.retired.extend(binding.bound.into_iter().filter_map(|b| b.wrapper));
        result = result.and(unlinked);
    }
    result
}

fn bind(m: &Memory, ui: &mut Ui, host: Host, holder: usize, root: usize) -> Result<()> {
    if ui.bindings.iter().any(|b| b.host == host && b.holder == holder && b.root == root) {
        return Ok(());
    }
    detach(m, ui, |b| b.host == host)?;
    if root == 0 || !ui.declared.iter().any(|d| d.host == host) {
        return Ok(());
    }
    // Teardown is noticed through the kill hook, so it must be this window's.
    if m.vslot(root, window::KILL)? != m.at(WINDOW_KILL) {
        return Err(format!("{host:?} window has an unexpected kill method"));
    }
    let mut binding = Binding { host, holder, root, bound: Vec::new() };
    for (index, d) in ui.declared.iter().enumerate().filter(|(_, d)| d.host == host) {
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
        if let Some(wrapper) = &b.wrapper {
            b.linked = true;
            m.call(m.at(SUBSCRIBE), &[b.signal, wrapper.as_ptr() as usize])?;
            if !subscribed(m, b)? {
                return Err("subscription did not register".into());
            }
            m.call(m.at(SET_RIGHT_CLICK), &[b.object, 1])?;
        }
    }
    log!("ui: bound {} element(s) in {host:?} {holder:#x}", binding.bound.len());
    Ok(())
}

/// Builds a game string holding `bytes` with the game's allocator and moves
/// it into a text box.
fn write_text(m: &Memory, object: usize, bytes: &[u8]) -> Result<()> {
    let mut string = game_string::empty();
    if !bytes.is_empty() {
        game_string::append_checked(m, string.as_mut_ptr() as usize, bytes)?;
    }
    m.call(m.at(TEXT_SET), &[object, string.as_mut_ptr() as usize, 0])?;
    Ok(())
}

fn apply(m: &Memory, ui: &mut Ui, host: Host) -> Result<()> {
    let Some(binding) = ui.bindings.iter_mut().find(|b| b.host == host) else { return Ok(()) };
    for b in &mut binding.bound {
        let d = &ui.declared[b.index];
        // Text boxes' show/hide methods are unverified on this build; a hidden
        // text box is emulated by an empty text instead.
        if d.element.kind == ElementKind::Text {
            let wanted = if d.desired.visible == Some(false) { Some(Vec::new()) } else { d.desired.text.clone() };
            if let Some(text) = wanted {
                if b.applied_text.as_ref() != Some(&text) {
                    write_text(m, b.object, &text)?;
                    b.applied_text = Some(text);
                }
            }
            continue;
        }
        if let Some(visible) = d.desired.visible {
            if (m.u8(b.object + element::VISIBLE)? != 0) != visible {
                let (slot, target) = if visible { (element::SHOW, SHOW) } else { (element::HIDE, HIDE) };
                let method = m.vslot(b.object, slot)?;
                if d.element.kind == ElementKind::Button && method != m.at(target) {
                    return Err("button show/hide method changed".into());
                }
                m.call(method, &[b.object])?;
            }
        }
        if d.element.kind == ElementKind::Button {
            if let Some(enabled) = d.desired.enabled {
                if b.applied_enabled != Some(enabled) {
                    if m.vslot(b.object, element::ENABLE)? != m.at(SET_ENABLED) {
                        return Err("button enable method changed".into());
                    }
                    m.call(m.at(SET_ENABLED), &[b.object, enabled as usize])?;
                    b.applied_enabled = Some(enabled);
                }
            }
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

extern "C" fn menu_after(frame: *mut HookFrame) {
    crate::guarded("main menu frame", || {
        let frame = unsafe { &*frame };
        let Some(m) = crate::memory().copied() else { return };
        if frame.fault != 0 || crate::thread_id() != crate::main_thread() {
            return;
        }
        crate::safe_point::main_menu_frame(frame.args[0]);
        crate::unicode::log_once();
        match main_menu_page(&m, frame.args[0]) {
            Ok(true) => crate::framework_ui::main_menu_frame(),
            Ok(false) => {}
            // The corner button then shows on every page of the main menu.
            Err(e) => {
                with_ui(|ui| report_once(ui, format!("main menu page unknown: {e}")));
                crate::framework_ui::main_menu_frame();
            }
        }
    });
}

extern "C" fn panel_after(frame: *mut HookFrame) {
    crate::guarded("panel update", || {
        let frame = unsafe { &*frame };
        let Some(m) = crate::memory().copied() else { return };
        if frame.fault != 0 || crate::thread_id() != crate::main_thread() || game::context(&m).is_err() {
            return;
        }
        let panel = frame.args[0];
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
    });
}

/// Detaches the bindings `which` selects while their objects are still alive.
fn retire(which: impl Fn(&Binding) -> bool) {
    let Some(m) = crate::memory().copied() else { return };
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

extern "C" fn panel_destroy(frame: *mut HookFrame) {
    let panel = unsafe { (*frame).args[0] };
    crate::guarded("panel destroy", || retire(|b| b.host == Host::ArmyPanel && b.holder == panel));
}
extern "C" fn panel_recreate(frame: *mut HookFrame) {
    let panel = unsafe { (*frame).args[0] }.checked_sub(army_panel::INTERFACE);
    crate::guarded("panel rebuild", || retire(|b| b.host == Host::ArmyPanel && Some(b.holder) == panel));
}
extern "C" fn button_destroy(frame: *mut HookFrame) {
    let button = unsafe { (*frame).args[0] };
    crate::guarded("button destroy", || retire(|b| b.bound.iter().any(|x| x.object == button)));
}
/// Every teardown path of a root window passes here first (notice-window.md §3).
extern "C" fn window_kill(frame: *mut HookFrame) {
    let window = unsafe { (*frame).args[0] };
    crate::guarded("window kill", || retire(|b| b.root == window));
}
/// Backstop for a root freed without passing the kill hook.
extern "C" fn window_delete(frame: *mut HookFrame) {
    let window = unsafe { (*frame).args[0] };
    crate::guarded("window delete", || retire(|b| b.root == window));
}

fn modifiers() -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
    let mut out = 0;
    if unsafe { GetKeyState(VK_SHIFT as i32) } < 0 {
        out |= abi::MODIFIER_SHIFT;
    }
    if unsafe { GetKeyState(VK_CONTROL as i32) } < 0 {
        out |= abi::MODIFIER_CTRL;
    }
    out
}

fn enqueue(context: usize, button: usize, click: u32) {
    // The game may dispatch while our UI lock is held further up this thread.
    let Ok(mut guard) = UI.try_lock() else { return };
    let Some(ui) = guard.as_mut() else { return };
    let Some(b) = ui
        .bindings
        .iter()
        .flat_map(|binding| &binding.bound)
        .find(|b| b.linked && b.object == button && b.wrapper.as_ref().is_some_and(|w| w.as_ptr() as usize == context))
    else {
        return;
    };
    let d = &ui.declared[b.index];
    let Some(extension) = d.extension else { return };
    if ui.clicks.len() < 64 {
        ui.clicks.push(Click { extension, name: d.element.name.clone(), click, modifiers: modifiers() });
    }
}

extern "system" fn left_click(context: usize, event: usize) {
    crate::guarded("left click", || {
        let Some(m) = crate::memory().copied() else { return };
        if event == 0 {
            return;
        }
        let Ok(button) = m.ptr(event) else { return };
        enqueue(context, button, abi::CLICK_LEFT);
        // Mark the click consumed so it does not fall through to the map.
        let _ = m.write(event + event::CONSUMED, &[1]);
    });
}

extern "system" fn right_click(context: usize, button: usize) {
    crate::guarded("right click", || enqueue(context, button, abi::CLICK_RIGHT));
}

/// Fills the panel's tooltip for our buttons that have one. The game builds
/// the output aggregate: main text at +0, delayed text at +32.
extern "C" fn tooltip_after(frame: *mut HookFrame) {
    crate::guarded("tooltip", || {
        let frame = unsafe { &*frame };
        let Some(m) = crate::memory().copied() else { return };
        if frame.fault != 0 || frame.result != frame.args[1] || crate::thread_id() != crate::main_thread() {
            return;
        }
        let Ok(guard) = UI.try_lock() else { return };
        let Some(ui) = guard.as_ref() else { return };
        let Some(binding) = ui.bindings.iter().find(|b| b.host == Host::ArmyPanel && b.holder == frame.args[0]) else { return };
        let Some(b) = binding.bound.iter().find(|b| b.object == frame.args[2] && b.wrapper.is_some()) else { return };
        let Some(text) = ui.declared[b.index].desired.tooltip.clone().filter(|t| !t.is_empty()) else { return };
        drop(guard);
        let output = frame.args[1];
        let empty = |offset: usize| game_string::untouched(&m, output + offset);
        if let (Ok(true), Ok(true)) = (empty(0), empty(game_string::OBJECT)) {
            if let Err(e) = game_string::append_checked(&m, output, &text) {
                log!("ui: tooltip append failed: {e}");
            }
        }
    });
}

/// The army panel, button and window functions the UI hooks or calls.
pub fn points() -> Vec<Point> {
    vec![
        Point::observe(PANEL_UPDATE, "48 89 5c 24 08 88 54 24 10 55 56 57 41 54 41 55", "army panel update", 2, None, Some(panel_after)),
        Point::observe(PANEL_DESTROY, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 41", "army panel destructor", 2, Some(panel_destroy), None),
        Point::observe(PANEL_RECREATE, "40 53 48 83 ec 40 48 8b d9 48 8b 05 e8 f8 ee 00", "army panel rebuild", 1, Some(panel_recreate), None),
        Point::observe(BUTTON_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8", "button destructor", 2, Some(button_destroy), None),
        Point::observe(WINDOW_KILL, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 c6 81 f4 00 00 00 01", "window kill", 1, Some(window_kill), None),
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
        Point::keep(UNSUBSCRIBE, "40 53 48 83 ec 20 48 8b d9 48 8b 49 08 48 85 c9", "signal unsubscribe"),
        Point::keep(SET_RIGHT_CLICK, "88 91 61 01 00 00 c3", "button right-click enable").uses(&[button::RIGHT_CLICK]),
        Point::keep(SHOW, "40 53 48 83 ec 20 80 b9 00 01 00 00 00 48 8b d9", "element show"),
        Point::keep(HIDE, "40 53 48 83 ec 20 c6 81 c5 00 00 00 00 48 8b d9", "element hide").uses(&[element::VISIBLE]),
        Point::keep(SET_ENABLED, "48 8b 01 84 d2 74 07 48 ff a0 68 02 00 00 48 ff", "button enable"),
        Point::method(classes::WINDOW, window::KILL, WINDOW_KILL, "window kill method"),
        Point::method(classes::BUTTON, element::SHOW, SHOW, "button show method"),
        Point::method(classes::BUTTON, element::HIDE, HIDE, "button hide method"),
        Point::method(classes::BUTTON, element::ENABLE, SET_ENABLED, "button enable method"),
        Point::method(classes::BUTTON, button::SET_RIGHT_CLICK, SET_RIGHT_CLICK, "button right-click method"),
        Point::method(classes::SIGNAL, signal::SUBSCRIBE, SUBSCRIBE, "signal subscribe method"),
        Point::method(classes::SIGNAL, signal::UNSUBSCRIBE, UNSUBSCRIBE, "signal unsubscribe method"),
    ]
}

pub fn take_clicks() -> Vec<Click> {
    with_ui(|ui| std::mem::take(&mut ui.clicks))
}

fn desired(extension: usize, name: &str, f: impl FnOnce(&mut Desired)) -> GResult<()> {
    with_ui(|ui| match ui.declared.iter_mut().find(|d| d.extension == Some(extension) && d.element.name == name) {
        Some(d) => {
            f(&mut d.desired);
            Ok(())
        }
        None => fail(Status::NotFound, format!("no element {name:?} declared by this mod")),
    })
}

pub fn set_visible(extension: usize, name: &str, visible: bool) -> GResult<()> {
    desired(extension, name, |d| d.visible = Some(visible))
}
pub fn set_enabled(extension: usize, name: &str, enabled: bool) -> GResult<()> {
    desired(extension, name, |d| d.enabled = Some(enabled))
}
pub fn set_text(extension: usize, name: &str, text: Vec<u8>) -> GResult<()> {
    desired(extension, name, |d| d.text = Some(text))
}
pub fn set_tooltip(extension: usize, name: &str, text: Vec<u8>) -> GResult<()> {
    desired(extension, name, |d| d.tooltip = Some(text))
}

/// Hides everything a stopped extension added.
pub fn hide_extension(extension: usize) {
    with_ui(|ui| {
        for d in ui.declared.iter_mut().filter(|d| d.extension == Some(extension)) {
            d.desired.visible = Some(false);
        }
    });
}

/// The own land army shown in the open single-army panel.
pub fn selected_army(m: &Memory, ctx: &game::Ctx) -> GResult<u64> {
    let (panel, root) = with_ui(|ui| ui.bindings.iter().find(|b| b.host == Host::ArmyPanel).map(|b| (b.holder, b.root)))
        .ok_or_else(|| game::Fail { status: Status::NotFound, message: "army panel not open".into() })?;
    let ours = m.ptr(panel)? == m.at(classes::ARMY_PANEL) && m.ptr(panel + army_panel::WINDOW)? == root;
    if !ours || m.ptr(root)? != m.at(classes::WINDOW) || m.ptr(root + window::GUI)? != m.at(classes::WINDOW_GUI) {
        return Err("army panel changed".to_string().into());
    }
    if m.u8(root + window::OPEN)? != 1 {
        return fail(Status::NotFound, "army panel not open");
    }
    let army = m.ptr(panel + army_panel::ARMY)?;
    if army == 0 || m.ptr(army)? != m.at(campaign::classes::ARMY) || m.u64(army + unit::OWNER)? != ctx.player.0 {
        return fail(Status::NotFound, "no own land army selected");
    }
    Ok(m.u64(army + unit::ID)?)
}
