//! Binds the spliced elements of the single-army panel each time the game
//! builds that panel, applies the state extensions ask for, and turns button
//! clicks into events. Ported from eu4-regional-combat's native UI; the ABI
//! evidence is in that repository's docs/native-{ui,settings}-findings.txt.
//!
//! Extensions state what they want (visible, enabled, text, tooltip) and the
//! panel hook applies it every update, so state survives the game rebuilding
//! the panel and never needs a callback inside the game's UI code.
use crate::{
    baseline::Dependency,
    game::{self, fail, GResult, ARMY_VT},
    hooks::{self, HookFrame},
    memory::Memory,
    splice::{Element, ElementKind},
    Result,
};
use eu4_harmony_abi as abi;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

const BUTTON_VT: usize = 0x1d58218;
const SIGNAL_VT: usize = 0x1d581f8;
const PANEL_VT: usize = 0x1d87088;
const ROOT_VT: usize = 0x1d926b0;
const ROOT_GUI_VT: usize = 0x1d929d0;
const TEXT_VT: usize = 0x1d8fd10;
/// Callback wrapper class the panel uses for its own buttons.
const WRAPPER_VT: usize = 0x1cb80c8;

const PANEL_UPDATE: usize = 0x1450660;
const PANEL_DESTROY: usize = 0x144f560;
const PANEL_RECREATE: usize = 0x1450580;
const BUTTON_DESTROY: usize = 0x10b5670;
const TOOLTIP: usize = 0x1453b70;
const STRING_APPEND: usize = 0x932f0;
const TEXT_SET: usize = 0x152ae10;
const SUBSCRIBE: usize = 0xfbe3f0;
const UNSUBSCRIBE: usize = 0xfbe370;
const SET_RIGHT_CLICK: usize = 0x1593860;
const SHOW: usize = 0x15aeb30;
const HIDE: usize = 0x15aead0;
const SET_ENABLED: usize = 0xfaab30;

pub const DEPENDENCIES: &[Dependency] = &[
    Dependency { rva: PANEL_UPDATE, bytes: "48 89 5c 24 08 88 54 24 10 55 56 57 41 54 41 55", what: "army panel update" },
    Dependency { rva: PANEL_DESTROY, bytes: "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 41", what: "army panel destructor" },
    Dependency { rva: PANEL_RECREATE, bytes: "40 53 48 83 ec 40 48 8b d9 48 8b 05 e8 f8 ee 00", what: "army panel rebuild" },
    Dependency { rva: BUTTON_DESTROY, bytes: "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8", what: "button destructor" },
    Dependency { rva: TOOLTIP, bytes: "48 89 5c 24 08 48 89 74 24 10 55 57 41 54 41 56", what: "army panel tooltip" },
    Dependency { rva: STRING_APPEND, bytes: "48 89 5c 24 18 55 56 57 41 55 41 56 48 83 ec 20", what: "string append" },
    Dependency { rva: TEXT_SET, bytes: "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", what: "text box setter" },
    Dependency { rva: SUBSCRIBE, bytes: "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 80", what: "signal subscribe" },
    Dependency { rva: UNSUBSCRIBE, bytes: "40 53 48 83 ec 20 48 8b d9 48 8b 49 08 48 85 c9", what: "signal unsubscribe" },
    Dependency { rva: SET_RIGHT_CLICK, bytes: "88 91 61 01 00 00 c3", what: "button right-click enable" },
    Dependency { rva: SHOW, bytes: "40 53 48 83 ec 20 80 b9 00 01 00 00 00 48 8b d9", what: "element show" },
    Dependency { rva: HIDE, bytes: "40 53 48 83 ec 20 c6 81 c5 00 00 00 00 48 8b d9", what: "element hide" },
    Dependency { rva: SET_ENABLED, bytes: "48 8b 01 84 d2 74 07 48 ff a0 68 02 00 00 48 ff", what: "button enable" },
];

#[derive(Default, Clone)]
struct Desired {
    visible: Option<bool>,
    enabled: Option<bool>,
    text: Option<Vec<u8>>,
    tooltip: Option<Vec<u8>>,
}

struct Declared {
    element: Element,
    /// Index of the extension that receives this element's clicks.
    extension: Option<usize>,
    desired: Desired,
}

struct Bound {
    index: usize,
    object: usize,
    signal: usize,
    /// Fake callback wrapper the game calls back through; must outlive the subscription.
    wrapper: Option<Box<[usize; 15]>>,
    linked: bool,
    old_right_click: u8,
    applied_text: Option<Vec<u8>>,
    applied_enabled: Option<bool>,
}

struct Binding {
    panel: usize,
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
    binding: Option<Binding>,
    /// Wrappers of detached bindings; the game may still hold pointers to them.
    retired: Vec<Box<[usize; 15]>>,
    clicks: Vec<Click>,
    reported: HashSet<String>,
    /// Set when a binding could not be detached safely; binding stops.
    fatal: Option<String>,
}

static MEMORY: OnceLock<Memory> = OnceLock::new();
static UI: Mutex<Option<Ui>> = Mutex::new(None);

fn with_ui<T>(f: impl FnOnce(&mut Ui) -> T) -> T {
    let mut guard = UI.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Ui::default))
}

fn memory() -> Option<Memory> {
    MEMORY.get().copied()
}

fn report_once(ui: &mut Ui, message: String) {
    if ui.reported.insert(message.clone()) {
        log!("ui: {message}");
    }
}

/// Elements spliced this run, with the extension index that owns each.
pub fn set_elements(elements: Vec<(Element, Option<usize>)>) {
    with_ui(|ui| {
        ui.declared = elements.into_iter().map(|(element, extension)| Declared { element, extension, desired: Desired::default() }).collect();
    });
}

fn lookup(m: &Memory, root: usize, slot: usize, name: &str) -> Result<usize> {
    let c = format!("{name}\0");
    m.call(m.vslot(root, slot)?, &[root, c.as_ptr() as usize])
}

/// Name an element reports, through the getter shared by all GUI element classes.
fn element_name(m: &Memory, object: usize) -> Result<Vec<u8>> {
    let getter = m.ptr(m.at(BUTTON_VT) + 0xa0)?;
    if m.vslot(object, 0xa0)? != getter {
        return Err("element has an unexpected name getter".into());
    }
    m.msvc_string(m.call(getter, &[object])?)
}

fn bind_one(m: &Memory, root: usize, index: usize, d: &Declared) -> Result<Option<Bound>> {
    let slot = match d.element.kind {
        ElementKind::Button => 0x68,
        ElementKind::Text => 0x98,
        ElementKind::Icon => 0xc8,
        ElementKind::Other => return Ok(None),
    };
    let object = lookup(m, root, slot, &d.element.prefixed)?;
    if object == 0 {
        return Err(format!("{} not found in the panel", d.element.prefixed));
    }
    let vt = m.ptr(object)?;
    let expected_vt = match d.element.kind {
        ElementKind::Button => Some(BUTTON_VT),
        ElementKind::Text => Some(TEXT_VT),
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
        let signal = object + 0xc8;
        if m.ptr(signal)? != m.at(SIGNAL_VT)
            || m.ptr(signal + 0x20)? != 0
            || m.vslot(signal, 8)? != m.at(SUBSCRIBE)
            || m.vslot(signal, 0x10)? != m.at(UNSUBSCRIBE)
            || m.vslot(object, 0x170)? != m.at(SET_RIGHT_CLICK)
        {
            return Err(format!("{} has an unexpected click signal", d.element.prefixed));
        }
        let mut wrapper = Box::new([0usize; 15]);
        wrapper[0] = m.at(WRAPPER_VT);
        wrapper[1] = wrapper.as_ptr() as usize;
        wrapper[9] = right_click as *const () as usize;
        wrapper[14] = left_click as *const () as usize;
        bound.signal = signal;
        bound.old_right_click = m.u8(object + 0x161)?;
        bound.wrapper = Some(wrapper);
    }
    Ok(Some(bound))
}

fn subscribed(m: &Memory, b: &Bound) -> Result<bool> {
    let Some(wrapper) = &b.wrapper else { return Ok(false) };
    let wrapper = wrapper.as_ptr() as usize;
    let mut node = m.ptr(b.signal + 8)?;
    for _ in 0..512 {
        if node == 0 {
            return Ok(false);
        }
        if m.ptr(node)? == wrapper {
            return Ok(true);
        }
        node = m.ptr(node + 0x10)?;
    }
    Err("signal listener list too long".into())
}

fn detach(m: &Memory, ui: &mut Ui) -> Result<()> {
    let Some(mut binding) = ui.binding.take() else { return Ok(()) };
    let result = (|| {
        for b in &mut binding.bound {
            if !b.linked {
                continue;
            }
            if m.ptr(b.signal)? != m.at(SIGNAL_VT) || m.u8(b.signal + 0x1c)? != 0 {
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
    result
}

fn bind(m: &Memory, ui: &mut Ui, panel: usize) -> Result<()> {
    let root = m.ptr(panel + 0x38)?;
    if ui.binding.as_ref().is_some_and(|b| b.panel == panel && b.root == root) {
        return Ok(());
    }
    detach(m, ui)?;
    if root == 0 || ui.declared.is_empty() {
        return Ok(());
    }
    let mut binding = Binding { panel, root, bound: Vec::new() };
    for (index, d) in ui.declared.iter().enumerate() {
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
    ui.binding = Some(binding);
    let binding = ui.binding.as_mut().unwrap();
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
    log!("ui: bound {} element(s) on panel {panel:#x}", binding.bound.len());
    Ok(())
}

/// Builds an MSVC string holding `bytes` with the game's allocator and moves
/// it into a text box.
fn write_text(m: &Memory, object: usize, bytes: &[u8]) -> Result<()> {
    let mut string = [0u64; 4];
    string[3] = 15;
    if !bytes.is_empty() {
        m.call(m.at(STRING_APPEND), &[string.as_mut_ptr() as usize, bytes.as_ptr() as usize, bytes.len()])?;
    }
    m.call(m.at(TEXT_SET), &[object, string.as_mut_ptr() as usize, 0])?;
    Ok(())
}

fn apply(m: &Memory, ui: &mut Ui) -> Result<()> {
    let Some(binding) = ui.binding.as_mut() else { return Ok(()) };
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
            if (m.u8(b.object + 0xc5)? != 0) != visible {
                let (slot, target) = if visible { (0x88, SHOW) } else { (0x90, HIDE) };
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
                    if m.vslot(b.object, 0x80)? != m.at(SET_ENABLED) {
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

extern "C" fn panel_after(frame: *mut HookFrame) {
    crate::guarded("panel update", || {
        let frame = unsafe { &*frame };
        let Some(m) = memory() else { return };
        if frame.fault != 0 || crate::thread_id() != crate::main_thread() || game::context(&m).is_err() {
            return;
        }
        let panel = frame.args[0];
        with_ui(|ui| {
            if ui.fatal.is_some() {
                return;
            }
            let result = (|| -> Result<()> {
                if m.ptr(panel)? != m.at(PANEL_VT) {
                    return Err("unexpected army panel class".into());
                }
                bind(&m, ui, panel)?;
                apply(&m, ui)
            })();
            if let Err(e) = result {
                report_once(ui, e);
            }
        });
    });
}

fn retire(panel: Option<usize>, button: Option<usize>) {
    let Some(m) = memory() else { return };
    with_ui(|ui| {
        let matches = ui
            .binding
            .as_ref()
            .is_some_and(|b| panel == Some(b.panel) || button.is_some_and(|p| b.bound.iter().any(|x| x.object == p)));
        if matches {
            if let Err(e) = detach(&m, ui) {
                log!("ui: binding could not be detached, UI binding stops: {e}");
                ui.fatal = Some(e);
            }
        }
    });
}

extern "C" fn panel_destroy(frame: *mut HookFrame) {
    crate::guarded("panel destroy", || retire(Some(unsafe { (*frame).args[0] }), None));
}
extern "C" fn button_destroy(frame: *mut HookFrame) {
    crate::guarded("button destroy", || retire(None, Some(unsafe { (*frame).args[0] })));
}
extern "C" fn panel_recreate(frame: *mut HookFrame) {
    crate::guarded("panel rebuild", || retire(unsafe { (*frame).args[0] }.checked_sub(0x10), None));
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
    let Some(binding) = &ui.binding else { return };
    let Some(b) = binding.bound.iter().find(|b| b.linked && b.object == button && b.wrapper.as_ref().is_some_and(|w| w.as_ptr() as usize == context)) else {
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
        let Some(m) = memory() else { return };
        if event == 0 {
            return;
        }
        let Ok(button) = m.ptr(event) else { return };
        enqueue(context, button, abi::CLICK_LEFT);
        // Mark the click consumed so it does not fall through to the map.
        let _ = m.write(event + 0x10, &[1]);
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
        let Some(m) = memory() else { return };
        if frame.fault != 0 || frame.result != frame.args[1] || crate::thread_id() != crate::main_thread() {
            return;
        }
        let Ok(guard) = UI.try_lock() else { return };
        let Some(ui) = guard.as_ref() else { return };
        let Some(binding) = &ui.binding else { return };
        if binding.panel != frame.args[0] {
            return;
        }
        let Some(b) = binding.bound.iter().find(|b| b.object == frame.args[2] && b.wrapper.is_some()) else { return };
        let Some(text) = ui.declared[b.index].desired.tooltip.clone().filter(|t| !t.is_empty()) else { return };
        drop(guard);
        let output = frame.args[1];
        let empty = |offset: usize| -> Result<bool> { Ok(m.u64(output + offset + 16)? == 0 && m.u64(output + offset + 24)? == 15) };
        match (empty(0), empty(32)) {
            (Ok(true), Ok(true)) => {
                if let Err(e) = m.call(m.at(STRING_APPEND), &[output, text.as_ptr() as usize, text.len()]) {
                    log!("ui: tooltip append failed: {e}");
                }
            }
            _ => {}
        }
    });
}

pub fn install(m: Memory) -> Result<()> {
    MEMORY.set(m).map_err(|_| "UI already installed".to_string())?;
    hooks::observe(m.at(PANEL_UPDATE), 2, None, Some(panel_after))?;
    hooks::observe(m.at(PANEL_DESTROY), 2, Some(panel_destroy), None)?;
    hooks::observe(m.at(BUTTON_DESTROY), 2, Some(button_destroy), None)?;
    hooks::observe(m.at(PANEL_RECREATE), 1, Some(panel_recreate), None)?;
    hooks::observe(m.at(TOOLTIP), 3, None, Some(tooltip_after))?;
    Ok(())
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
        None => fail(abi::NOT_FOUND, format!("no element {name:?} declared by this mod")),
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
    let (panel, root) = with_ui(|ui| ui.binding.as_ref().map(|b| (b.panel, b.root))).ok_or_else(|| game::Fail { code: abi::NOT_FOUND, message: "army panel not open".into() })?;
    if m.ptr(panel)? != m.at(PANEL_VT) || m.ptr(panel + 0x38)? != root || m.ptr(root)? != m.at(ROOT_VT) || m.ptr(root + 0x30)? != m.at(ROOT_GUI_VT) {
        return Err("army panel changed".to_string().into());
    }
    if m.u8(root + 0xf5)? != 1 {
        return fail(abi::NOT_FOUND, "army panel not open");
    }
    let army = m.ptr(panel + 0x40)?;
    if army == 0 || m.ptr(army)? != m.at(ARMY_VT) || m.u64(army + 0xd0)? != ctx.player.0 {
        return fail(abi::NOT_FOUND, "no own land army selected");
    }
    Ok(m.u64(army + 0x20)?)
}
