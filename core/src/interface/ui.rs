//! Binds spliced elements in the vanilla windows that host them, applies the
//! state extensions ask for, and turns button clicks into signals. Ported from
//! eu4-regional-combat's native UI; the ABI evidence is in
//! docs/game/interface/interface-bindings.md (tooltips included). Window teardown
//! is in docs/game/interface/window-construction.md.
//!
//! Extensions state what they want (visible, enabled, text, tooltip) and each
//! host's update hook applies it, so state survives the game rebuilding the
//! window and never needs a callback inside the game's UI code. Low-level
//! native GUI primitives live in `crate::interface::native_gui` and element
//! state/click handling in `crate::interface::bound_element`.
use super::{
    bound_element::{self, BoundElement, ElementOps, Hears, LiveElements, Look, Shown},
    native_gui::{self, Callback},
};
use crate::{
    base::{
        hosts::{self, Host},
        notice, Result,
    },
    native::{
        hooks::Call,
        memory::Memory,
        objects::gui::{army_panel, classes, functions, window},
        points::Point,
        splice::{Addition, Element, ElementKind, Plan},
    },
};
use eu4_harmony_lua::windows::HostedElements;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

pub use super::native_gui::{MODIFIER_CTRL, MODIFIER_SHIFT};

const PANEL_UPDATE: usize = 0x1450660;
const PANEL_DESTROY: usize = 0x144f560;
const PANEL_RECREATE: usize = 0x1450580;
const TOOLTIP: usize = 0x1453b70;

/// The name of the frame the framework puts under a window's button area,
/// and of the button that is its background.
const FRAME: &str = "eu4h_button_area";
const FRAME_BACKGROUND: &str = "eu4h_button_area_background";

/// The frame under `count` slots of `area`: a window of its own in the
/// framework's window background, as the extension windows are, which the
/// pointer does not light up and whose clicks do not reach the map. A button
/// would light up; an icon is drawn at its sprite's size, not stretched.
fn button_area_frame(area: &hosts::ButtonArea, count: i32) -> String {
    let ((x, y), (w, h)) = area.frame(count);
    let sprite = super::FIXED_BACKGROUND_SPRITE;
    format!(
        "\n\t\twindowType = {{ name = \"{FRAME}\" backGround = \"{FRAME_BACKGROUND}\" position = {{ x = {x} y = {y} }} size = {{ x = {w} y = {h} }} \
         moveable = 0 dontRender = \"\" horizontalBorder = \"\" verticalBorder = \"\" fullScreen = no Orientation = \"UPPER_LEFT\"\n\
         \t\t\tguiButtonType = {{ name = \"{FRAME_BACKGROUND}\" quadTextureSprite = \"{sprite}\" }}\n\t\t}}\n"
    )
}

/// Inserts each host button area's background frame first in its window so it
/// draws under every spliced element, returning the hosts that received a frame.
pub(crate) fn splice_frames(plan: &mut Plan, slots: &HashMap<&'static str, i32>) -> Vec<Host> {
    let mut frames = Vec::new();
    for window in hosts::WINDOWS {
        let (Some(area), Some(&count)) = (window.buttons, slots.get(window.name)) else { continue };
        plan.add_first(window.file, Addition::Window { window: window.name.into(), text: button_area_frame(&area, count) });
        frames.push(window.host);
    }
    frames
}

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
/// `crate::interface::bound_element`), except the destruction hooks, which
/// must detach; none of those calls destroys a panel, button or window, so
/// a destruction hook never runs under the lock.
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

trait HostNative: ElementOps {
    fn check_window_kill(&self, root: usize) -> Result<()>;
    fn bind_frame(&self, root: usize) -> Result<usize>;
    fn bind_element(&self, root: usize, declared: &Element) -> Result<Option<BoundElement>>;
    fn attach_ordering(&mut self, host: Host, root: usize, frame: Option<usize>, controls: &[(usize, Look)]) -> Result<()>;
    fn detach_ordering(&mut self, root: usize);
    fn element_visible(&self, element: &BoundElement) -> Result<bool>;
    fn frame_open(&self, frame: usize) -> Result<bool>;
    fn show_frame(&mut self, frame: usize, open: bool) -> Result<()>;
    fn resolve_text(&self, owner: &str, text: &eu4_harmony_lua::Text) -> String;
    fn take_clicks(&mut self) -> Vec<bound_element::Clicked>;
    fn fall_back_notice(&mut self, message: &str);
}

struct GameHost<'a>(&'a Memory);

impl ElementOps for GameHost<'_> {
    fn listener(&mut self, object: usize, look: Look) -> Result<BoundElement> {
        LiveElements(self.0, Hears::Panels).listener(object, look)
    }
    fn subscribed(&mut self, element: &BoundElement) -> bool {
        LiveElements(self.0, Hears::Panels).subscribed(element)
    }
    fn link(&mut self, element: &mut BoundElement) -> Result<()> {
        LiveElements(self.0, Hears::Panels).link(element)
    }
    fn unlink(&mut self, element: &mut BoundElement) -> Result<()> {
        LiveElements(self.0, Hears::Panels).unlink(element)
    }
    fn apply(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()> {
        LiveElements(self.0, Hears::Panels).apply(element, shown)
    }
}

impl HostNative for GameHost<'_> {
    fn check_window_kill(&self, root: usize) -> Result<()> {
        if self.0.vslot(root, window::KILL)? != self.0.at(native_gui::WINDOW_KILL) {
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

    fn attach_ordering(&mut self, host: Host, root: usize, frame: Option<usize>, controls: &[(usize, Look)]) -> Result<()> {
        crate::interface::window_ordering::attach(self.0, host, root, frame, controls)
    }

    fn detach_ordering(&mut self, root: usize) {
        crate::interface::window_ordering::detach_host(root);
    }

    fn element_visible(&self, element: &BoundElement) -> Result<bool> {
        element.visible(self.0)
    }

    fn frame_open(&self, frame: usize) -> Result<bool> {
        Ok(self.0.u8(frame + window::OPEN)? != 0)
    }

    fn show_frame(&mut self, frame: usize, open: bool) -> Result<()> {
        native_gui::show_window(self.0, frame, open)
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
    let object = native_gui::lookup(m, root, method, &element.prefixed)?;
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
    if native_gui::element_name(m, object)? != element.prefixed.as_bytes() {
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
            native.apply(&mut b.element, &desired)?;
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
    let frame = native_gui::with_string(m, FRAME.as_bytes(), |name| functions::FIND_WINDOW.call(m, [root, name, 0]))?;
    if frame == 0 || m.ptr(frame)? != m.at(classes::WINDOW) {
        return Err("not found".into());
    }
    native_gui::keep_still(m, frame, FRAME_BACKGROUND)?;
    Ok(frame)
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

fn panel_destroy(call: &mut Call) {
    let panel = call.arg(0);
    retire(call.memory(), |b| b.host == Host::ArmyPanel && b.holder == panel);
}

fn panel_recreate(call: &mut Call) {
    let panel = call.arg(0).checked_sub(army_panel::INTERFACE);
    retire(call.memory(), |b| b.host == Host::ArmyPanel && Some(b.holder) == panel);
}

pub(super) fn button_gone(m: Memory, button: usize) {
    retire(m, |b| b.bound.iter().any(|x| x.element.object == button));
}

pub(super) fn window_gone(m: Memory, window: usize) {
    retire(m, |b| b.root == window);
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

/// The army panel functions the UI hooks.
pub fn points() -> Vec<Point> {
    vec![
        Point::observe(PANEL_UPDATE, "48 89 5c 24 08 88 54 24 10 55 56 57 41 54 41 55", "army panel update", 2, None, Some(panel_after)),
        Point::observe(PANEL_DESTROY, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 41", "army panel destructor", 2, Some(panel_destroy), None),
        Point::observe(PANEL_RECREATE, "40 53 48 83 ec 40 48 8b d9 48 8b 05 e8 f8 ee 00", "army panel rebuild", 1, Some(panel_recreate), None),
        Point::observe(TOOLTIP, "48 89 5c 24 08 48 89 74 24 10 55 57 41 54 41 56", "army panel tooltip", 3, None, Some(tooltip_after)),
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

    impl ElementOps for RecordingHost {
        fn listener(&mut self, object: usize, look: Look) -> Result<BoundElement> {
            Ok(BoundElement::test_listening(object, look))
        }

        fn subscribed(&mut self, element: &BoundElement) -> bool {
            self.linked.contains(&element.object)
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

        fn apply(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()> {
            element.record_for_test(shown);
            Ok(())
        }
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
    fn a_button_area_s_frame_comes_first_and_holds_its_slots() {
        let mut plan = Plan::default();
        plan.add("interface/unitpanel_main.gui", Addition::Window { window: "single_unitpanel".into(), text: " BUTTON ".into() });
        let slots = HashMap::from([("single_unitpanel", 10)]);
        let frames = splice_frames(&mut plan, &slots);
        assert_eq!(frames, [Host::ArmyPanel]);
        let file = "guiTypes = { windowType = { name = \"single_unitpanel\" } }";
        let out = crate::native::splice::apply(file, &plan.for_file("interface/unitpanel_main.gui", file.as_bytes())).unwrap();
        let (frame, button) = (out.find("eu4h_button_area").unwrap(), out.find("BUTTON").unwrap());
        assert!(frame < button, "{out}");
        assert!(out.contains("position = { x = 504 y = 69 } size = { x = 116 y = 392 }"), "two columns: {out}");
        assert!(
            out.contains("windowType = { name = \"eu4h_button_area\" backGround = \"eu4h_button_area_background\""),
            "a window, which no pointer lights up: {out}"
        );
        assert_eq!(out.matches('{').count(), out.matches('}').count());
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
}
