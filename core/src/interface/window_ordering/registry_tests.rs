use super::*;
use crate::{native::bound_element::Look, capabilities::game::buffers::{Image, Object}, native::objects::gui::{classes, globals, mouse_input, window}};
use std::rc::Rc;

struct Tree {
    image: Image,
    scene: Object,
    _manager: Object,
    _gate: Object,
    roots: Vec<Object>,
    nodes: Vec<Object>,
}

impl Tree {
    fn new(count: usize) -> Self {
        let image = Image::new();
        let m = image.memory();
        let gate = Object::new(0x278).u64(0x270, m.at(0x8f440usize) as u64);
        let roots: Vec<_> = (0..count).map(|_| Object::new(0xa38).u64(0x30, m.at(classes::WINDOW_GUI) as u64).u8(0xf5, 1)).collect();
        let nodes: Vec<_> = roots.iter().map(|root| Object::new(0x180).u64(0, gate.at() as u64).u64(0x118, (root.at() + 0x30) as u64)).collect();
        for (root, node) in roots.iter().zip(&nodes) {
            root.set(0xa30, &(node.at() as u64).to_le_bytes());
        }
        let scene = Object::new(0x500).u64(0x110, gate.at() as u64).vector(0x58, &nodes.iter().map(|node| node.at() as u64).collect::<Vec<_>>());
        let manager = Object::new(0xc8).u64(0xc0, scene.at() as u64);
        image.u64(globals::MANAGER, manager.at() as u64);
        let tree = Self { image, scene, _manager: manager, _gate: gate, roots, nodes };
        tree.link(tree.parent(), &(0..count).map(|n| tree.nodes[n].at()).collect::<Vec<_>>());
        tree
    }

    fn parent(&self) -> usize {
        self.scene.at() + native::ROOT
    }

    fn memory(&self) -> &'static Memory {
        self.image.memory()
    }

    fn link(&self, parent: usize, nodes: &[usize]) {
        Self::link_mem(self.memory(), parent, nodes);
    }

    fn link_mem(m: &Memory, parent: usize, nodes: &[usize]) {
        m.write(parent + 0x100, &nodes.first().copied().unwrap_or(0).to_le_bytes()).unwrap();
        m.write(parent + 0x108, &nodes.last().copied().unwrap_or(0).to_le_bytes()).unwrap();
        for (i, &node) in nodes.iter().enumerate() {
            m.write(node + 0x110, &parent.to_le_bytes()).unwrap();
            m.write(node + 0xf0, &i.checked_sub(1).map_or(0, |i| nodes[i]).to_le_bytes()).unwrap();
            m.write(node + 0xf8, &nodes.get(i + 1).copied().unwrap_or(0).to_le_bytes()).unwrap();
        }
    }

    fn owned(&self, index: usize, appearance: u64) -> Owned {
        Owned { identity: Identity::Window(0, "test".into()), root: self.roots[index].at(), appearance }
    }

    fn scope_order(&self) -> Vec<usize> {
        placement::snapshot(self.memory(), self.parent()).unwrap()
    }
}

#[derive(Default)]
struct LocalAccess {
    registry: Rc<RefCell<Registry>>,
    input: RefCell<Input>,
}

impl OrderAccess for LocalAccess {
    fn with_registry<T>(&self, f: impl FnOnce(&mut Registry) -> T) -> T {
        f(&mut self.registry.borrow_mut())
    }

    fn with_input<T>(&self, f: impl FnOnce(&mut Input) -> T) -> T {
        f(&mut self.input.borrow_mut())
    }

    fn hold(&self) -> Hold {
        Hold::Local(self.registry.clone())
    }
}

struct RecordingScene {
    owns_presentation: bool,
    apply_calls: usize,
    refresh_calls: usize,
    tamper_snapshot_once: bool,
    fail_apply: Option<String>,
    fail_refresh: Option<String>,
}

impl Default for RecordingScene {
    fn default() -> Self {
        Self { owns_presentation: true, apply_calls: 0, refresh_calls: 0, tamper_snapshot_once: false, fail_apply: None, fail_refresh: None }
    }
}

impl SceneOps for RecordingScene {
    fn owns_presentation(&self) -> bool {
        self.owns_presentation
    }

    fn snapshot(&self, m: &Memory, parent: usize) -> Result<Vec<usize>> {
        let mut nodes = placement::snapshot(m, parent)?;
        if self.tamper_snapshot_once && nodes.len() >= 2 {
            nodes.swap(0, 1);
        }
        Ok(nodes)
    }

    fn apply(&mut self, m: &Memory, parent: usize, after: &[usize], movable: &BTreeSet<usize>) -> Result<()> {
        self.apply_calls += 1;
        if let Some(error) = self.fail_apply.take() {
            return Err(error);
        }
        let current = placement::snapshot(m, parent)?;
        let fixed_before: Vec<_> = current.iter().copied().filter(|n| !movable.contains(n)).collect();
        let fixed_after: Vec<_> = after.iter().copied().filter(|n| !movable.contains(n)).collect();
        assert_eq!(fixed_before, fixed_after, "fixed siblings must keep their relative order");
        Tree::link_mem(m, parent, after);
        Ok(())
    }

    fn refresh(&mut self, _m: &Memory, _manager: usize, _scene: usize) -> Result<()> {
        self.refresh_calls += 1;
        if let Some(error) = self.fail_refresh.take() {
            return Err(error);
        }
        Ok(())
    }
}

fn mouse_event(kind: u32, button: u32) -> Object {
    let event = Object::new(0x60).u32(mouse_input::DEVICE, mouse_input::MOUSE).u32(mouse_input::KIND, kind).u32(mouse_input::BUTTON, button);
    event.set(native::HANDLED, &0u16.to_le_bytes());
    event
}

fn click_window(orderer: &Orderer<LocalAccess>, scene: &mut RecordingScene, m: &Memory, gui: usize, handled: bool) {
    let event = mouse_event(mouse_input::PRESSED, 1);
    orderer.mouse_before(m, event.at(), scene).unwrap();
    orderer.handler_before(m, gui, event.at()).unwrap();
    if handled {
        event.set(native::HANDLED, &1u16.to_le_bytes());
    }
    orderer.handler_after(m, event.at(), false);
    orderer.event_after(m, false, scene).unwrap();
}

#[test]
fn request_order_and_accepted_original_click_share_the_real_registry() {
    let tree = Tree::new(3);
    let orderer = Orderer(LocalAccess::default());
    let mut scene = RecordingScene::default();
    orderer.bind_owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(tree.scope_order(), [tree.nodes[0].at(), tree.nodes[2].at(), tree.nodes[1].at()]);
    assert_eq!(scene.refresh_calls, 1);

    click_window(&orderer, &mut scene, tree.memory(), tree.roots[2].at() + window::GUI, true);
    assert_eq!(tree.scope_order(), [tree.nodes[0].at(), tree.nodes[1].at(), tree.nodes[2].at()]);
    assert_eq!(scene.refresh_calls, 2);

    // Clicking the already-frontmost window marks clean without walking the scene.
    click_window(&orderer, &mut scene, tree.memory(), tree.roots[2].at() + window::GUI, true);
    assert_eq!(scene.apply_calls, 2);
    assert_eq!(scene.refresh_calls, 2);
    assert!(!orderer.0.with_registry(|r| r.is_dirty()));

    orderer.visibility_after(tree.memory()).unwrap();
    assert!(!orderer.0.with_registry(|r| r.is_dirty()), "a repeated native show is inert");
    orderer.bind_owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    assert!(!orderer.0.with_registry(|r| r.is_dirty()), "an unchanged open request is inert");
    orderer.bind_owned(tree.memory(), &[tree.owned(1, 2)]).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(tree.scope_order(), [tree.nodes[0].at(), tree.nodes[2].at(), tree.nodes[1].at()]);
    assert_eq!(scene.refresh_calls, 3);
}

#[test]
fn reconstruction_retains_order_and_retirement_prevents_reading_the_old_root() {
    let tree = Tree::new(4);
    let orderer = Orderer(LocalAccess::default());
    let mut scene = RecordingScene::default();
    orderer.bind_owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    click_window(&orderer, &mut scene, tree.memory(), tree.roots[2].at() + window::GUI, true);
    // Node 3 will become the replacement; remove its temporary native identity.
    orderer.retire(tree.roots[3].at());
    orderer.retire(tree.roots[1].at());
    tree.roots[1].set(0x30, &0usize.to_le_bytes());
    tree.link(tree.parent(), &[tree.nodes[0].at(), tree.nodes[2].at(), tree.nodes[3].at()]);
    orderer.visibility_after(tree.memory()).unwrap();
    orderer.bind_owned(tree.memory(), &[tree.owned(3, 1)]).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(tree.scope_order(), [tree.nodes[0].at(), tree.nodes[3].at(), tree.nodes[2].at()]);
}

#[test]
fn attached_group_stays_after_the_host_background_and_before_the_picker() {
    let tree = Tree::new(4);
    // Host 0 owns a raw background, picker 1, then two attached visual nodes.
    let background = Object::new(0x180);
    let parent = tree.nodes[0].at();
    tree.link(tree.parent(), &[parent]);
    tree.link(parent, &[background.at(), tree.nodes[1].at(), tree.nodes[2].at(), tree.nodes[3].at()]);
    let orderer = Orderer(LocalAccess::default());
    orderer.0.with_registry(|r| r.attached(tree.memory(), tree.roots[0].at(), Some(background.at()), &[tree.nodes[2].at(), tree.nodes[3].at()])).unwrap();
    let changes = orderer.0.with_registry(|r| r.changes(tree.memory())).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].after, [background.at(), tree.nodes[2].at(), tree.nodes[3].at(), tree.nodes[1].at()]);
    assert_eq!(changes[0].movable, [tree.nodes[2].at(), tree.nodes[3].at()].into_iter().collect());
    orderer.retire(tree.roots[0].at());
    assert_eq!(orderer.0.with_registry(|r| r.scope_count()), 0);
    assert_eq!(orderer.0.with_registry(|r| r.entry_count()), 0);
}

#[test]
fn rebuilding_a_host_keeps_its_relation_to_an_independent_window() {
    let tree = Tree::new(3);
    tree.link(tree.parent(), &[tree.nodes[0].at(), tree.nodes[1].at()]);
    let orderer = Orderer(LocalAccess::default());
    let mut scene = RecordingScene::default();
    orderer.0.with_registry(|r| r.host(tree.memory(), 0, tree.roots[0].at())).unwrap();
    orderer.bind_owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    orderer.retire(tree.roots[0].at());
    tree.roots[0].set(0x30, &0usize.to_le_bytes());
    tree.link(tree.parent(), &[tree.nodes[1].at(), tree.nodes[2].at()]);
    orderer.0.with_registry(|r| r.host(tree.memory(), 0, tree.roots[2].at())).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(tree.scope_order(), [tree.nodes[2].at(), tree.nodes[1].at()]);
    let hold = orderer.begin_owned(&BTreeSet::new());
    assert_eq!(orderer.0.with_registry(|r| r.scope_count()), 0, "without related framework windows, vanilla ordering is no longer observed");
    drop(hold);
}

#[test]
fn shared_visibility_methods_ignore_a_derived_window_outside_the_related_scope() {
    let tree = Tree::new(2);
    tree.link(tree.parent(), &[tree.nodes[0].at()]);
    tree.link(tree.nodes[0].at(), &[tree.nodes[1].at()]);
    let orderer = Orderer(LocalAccess::default());
    let mut scene = RecordingScene::default();
    orderer.0.with_registry(|r| r.attached(tree.memory(), tree.roots[0].at(), None, &[tree.nodes[1].at()])).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    let entries = orderer.0.with_registry(|r| r.entry_count());
    // The observed derived class shares root show/hide with ordinary windows.
    // Classification must not read the ordinary node field at root+0xa30.
    let derived = Object::new(0x38).u64(0, (tree.memory().base + 0x1d57768) as u64).u64(0x30, (tree.memory().base + 0x1d57510) as u64);
    orderer.before_visibility(tree.memory(), derived.at()).unwrap();
    orderer.visibility_after(tree.memory()).unwrap();
    assert_eq!(orderer.0.with_registry(|r| r.entry_count()), entries);
    assert!(!orderer.0.with_registry(|r| r.is_dirty()));
}

#[test]
fn shared_visibility_methods_still_adopt_a_hidden_ordinary_window() {
    let tree = Tree::new(2);
    tree.link(tree.parent(), &[tree.nodes[0].at()]);
    let orderer = Orderer(LocalAccess::default());
    let mut scene = RecordingScene::default();
    orderer.bind_owned(tree.memory(), &[tree.owned(0, 1)]).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    tree.roots[1].set(0xf5, &[0]);
    tree.link(tree.parent(), &[tree.nodes[0].at(), tree.nodes[1].at()]);
    orderer.before_visibility(tree.memory(), tree.roots[1].at()).unwrap();
    let (root, role) = orderer.0.with_registry(|r| r.node_entry(tree.nodes[1].at())).unwrap();
    assert_eq!(root, Some(tree.roots[1].at()));
    assert_eq!(role, course::Role::Original);
    tree.roots[1].set(0xf5, &[1]);
    orderer.visibility_after(tree.memory()).unwrap();
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(tree.scope_order(), [tree.nodes[0].at(), tree.nodes[1].at()]);
}

#[test]
fn shared_visibility_methods_reject_broken_ordinary_window_ownership() {
    let tree = Tree::new(2);
    let orderer = Orderer(LocalAccess::default());
    orderer.bind_owned(tree.memory(), &[tree.owned(0, 1)]).unwrap();
    tree.nodes[1].set(0x118, &0usize.to_le_bytes());
    let error = orderer.before_visibility(tree.memory(), tree.roots[1].at()).unwrap_err();
    assert!(error.contains("window container ownership changed"));
}

#[test]
fn ancestor_gate_visibility_changes_without_changing_the_child_open_byte() {
    let tree = Tree::new(2);
    let gate = Object::new(0x278).u64(0x270, tree.memory().at(0x11483a0usize) as u64);
    tree.nodes[0].set(0, &(gate.at() as u64).to_le_bytes());
    tree.nodes[0].set(0x178, &[1]);
    tree.link(tree.parent(), &[tree.nodes[0].at()]);
    tree.link(tree.nodes[0].at(), &[tree.nodes[1].at()]);
    assert!(native::visible(tree.memory(), tree.roots[1].at(), tree.parent()).unwrap());
    tree.nodes[0].set(0x178, &[0]);
    assert!(!native::visible(tree.memory(), tree.roots[1].at(), tree.parent()).unwrap());
    assert_eq!(tree.memory().u8(tree.roots[1].at() + 0xf5).unwrap(), 1);
    tree.nodes[0].set(0x178, &[1]);
    assert!(native::visible(tree.memory(), tree.roots[1].at(), tree.parent()).unwrap());
}

#[test]
fn coordinator_defers_placement_across_holds_nested_walks_and_faults() {
    let tree = Tree::new(3);
    let orderer = Orderer(LocalAccess::default());
    let mut scene = RecordingScene::default();
    let hold = orderer.begin_owned(&[tree.owned(1, 1).identity].into_iter().collect());
    orderer.bind_owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    // Suppressed while Reconciliation guard is held and while presentation is unowned.
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(scene.apply_calls, 0);
    drop(hold);

    scene.owns_presentation = false;
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(scene.apply_calls, 0);

    scene.owns_presentation = true;
    orderer.flush(tree.memory(), &mut scene).unwrap();
    assert_eq!(scene.apply_calls, 1);
    assert_eq!(scene.refresh_calls, 1);

    // A mouse press nested inside a passive walk defers flush until the outer walk returns.
    let key_event = mouse_event(mouse_input::PRESSED, 1);
    let click_ev = mouse_event(mouse_input::PRESSED, 1);
    orderer.passive_before(tree.memory(), key_event.at(), &mut scene).unwrap();
    orderer.mouse_before(tree.memory(), click_ev.at(), &mut scene).unwrap();
    orderer.handler_before(tree.memory(), tree.roots[2].at() + window::GUI, click_ev.at()).unwrap();
    click_ev.set(native::HANDLED, &1u16.to_le_bytes());
    orderer.handler_after(tree.memory(), click_ev.at(), false);
    orderer.event_after(tree.memory(), false, &mut scene).unwrap();
    assert_eq!(scene.apply_calls, 1, "inner walk must not flush inside outer passive walk");
    orderer.event_after(tree.memory(), false, &mut scene).unwrap();
    assert_eq!(scene.apply_calls, 2);
    assert_eq!(tree.scope_order(), [tree.nodes[0].at(), tree.nodes[1].at(), tree.nodes[2].at()]);

    // A faulted outer walk discards pending activations without reordering.
    let faulted_ev = mouse_event(mouse_input::PRESSED, 1);
    orderer.mouse_before(tree.memory(), faulted_ev.at(), &mut scene).unwrap();
    orderer.handler_before(tree.memory(), tree.roots[0].at() + window::GUI, faulted_ev.at()).unwrap();
    faulted_ev.set(native::HANDLED, &1u16.to_le_bytes());
    orderer.handler_after(tree.memory(), faulted_ev.at(), false);
    orderer.event_after(tree.memory(), true, &mut scene).unwrap();
    assert_eq!(scene.apply_calls, 2);
    assert_eq!(tree.scope_order(), [tree.nodes[0].at(), tree.nodes[1].at(), tree.nodes[2].at()]);
}

#[test]
fn failed_snapshot_apply_or_refresh_keeps_dirty_for_retry() {
    let tree = Tree::new(2);
    let orderer = Orderer(LocalAccess::default());
    orderer.bind_owned(tree.memory(), &[tree.owned(0, 1)]).unwrap();

    let mut stale = RecordingScene { tamper_snapshot_once: true, ..Default::default() };
    assert!(orderer.flush(tree.memory(), &mut stale).unwrap_err().contains("native order changed before placement"));
    assert!(orderer.0.with_registry(|r| r.is_dirty()));

    let mut fail_apply = RecordingScene { fail_apply: Some("apply failed".into()), ..Default::default() };
    assert_eq!(orderer.flush(tree.memory(), &mut fail_apply).unwrap_err(), "apply failed");
    assert!(orderer.0.with_registry(|r| r.is_dirty()));

    let mut fail_refresh = RecordingScene { fail_refresh: Some("refresh failed".into()), ..Default::default() };
    assert_eq!(orderer.flush(tree.memory(), &mut fail_refresh).unwrap_err(), "refresh failed");
    assert!(orderer.0.with_registry(|r| r.is_dirty()));

    let mut ok = RecordingScene::default();
    orderer.flush(tree.memory(), &mut ok).unwrap();
    assert!(!orderer.0.with_registry(|r| r.is_dirty()));
    assert_eq!(tree.scope_order(), [tree.nodes[1].at(), tree.nodes[0].at()]);
}

#[test]
fn host_nodes_validates_controls_scrollbars_icons_and_background() {
    let image = Image::new();
    let m = image.memory();
    image.u64(native::SCROLLBAR_CLASS + native::PARENT_SLOT, m.at(native::SCROLLBAR_PARENT) as u64);
    image.u64(native::BASE_ICON_CLASS + native::PARENT_SLOT, m.at(native::ICON_PARENT) as u64);
    image.u64(native::GAME_ICON_CLASS + native::PARENT_SLOT, m.at(native::ICON_PARENT) as u64);

    let host_root = Object::new(0xa38);
    let bg = Object::new(0x180).u64(0, m.at(classes::BACKGROUND) as u64).u64(native::ATTACHED_NODE, 0x9001);
    host_root.set(window::BACKGROUND, &(bg.at() as u64).to_le_bytes());

    let frame = Object::new(0xa38).u64(window::GUI, m.at(classes::WINDOW_GUI) as u64);
    let frame_node = Object::new(0x180).u64(0x118, (frame.at() + window::GUI) as u64);
    frame.set(0xa30, &(frame_node.at() as u64).to_le_bytes());

    let btn = Object::new(0x2a0).u64(0, m.at(classes::BUTTON) as u64).u64(native::BUTTON_NODE, 0x1001).u64(native::BUTTON_EXTRA, 0x1002);

    let scrollbar =
        Object::new(0x540).u64(0, m.at(native::SCROLLBAR_CLASS) as u64).u64(native::SCROLLBAR_CONTAINER, m.at(native::SCROLLBAR_CONTAINER_CLASS) as u64);
    let text_with_bar =
        Object::new(0x1b0).u64(0, m.at(classes::TEXT_BOX) as u64).u64(native::TEXT_NODE, 0x2001).u64(native::TEXT_SCROLLBAR, scrollbar.at() as u64);
    let text_plain = Object::new(0x1b0).u64(0, m.at(classes::TEXT_BOX) as u64).u64(native::TEXT_NODE, 0x2002);

    let base_icon = Object::new(0x180).u64(0, m.at(native::BASE_ICON_CLASS) as u64).u64(native::ATTACHED_NODE, 0x3001);
    let game_icon = Object::new(0x180).u64(0, m.at(native::GAME_ICON_CLASS) as u64).u64(native::ATTACHED_NODE, 0x3002);

    let (floor, nodes) = native::host_nodes(
        m,
        host_root.at(),
        Some(frame.at()),
        &[
            (btn.at(), Look::Button),
            (text_with_bar.at(), Look::Text),
            (text_plain.at(), Look::Text),
            (base_icon.at(), Look::Icon),
            (game_icon.at(), Look::Icon),
        ],
    )
    .unwrap();
    assert_eq!(floor, Some(0x9001));
    assert_eq!(nodes, [frame_node.at(), 0x1001, 0x1002, 0x2001, scrollbar.at() + native::SCROLLBAR_CONTAINER, 0x2002, 0x3001, 0x3002]);

    let bad_scrollbar = Object::new(0x540).u64(0, m.at(native::SCROLLBAR_CLASS) as u64);
    let bad_text =
        Object::new(0x1b0).u64(0, m.at(classes::TEXT_BOX) as u64).u64(native::TEXT_NODE, 0x2003).u64(native::TEXT_SCROLLBAR, bad_scrollbar.at() as u64);
    assert!(native::host_nodes(m, host_root.at(), None, &[(bad_text.at(), Look::Text)]).unwrap_err().contains("scrollbar container"));

    let wrong_class = Object::new(0x40).u64(0, m.at(classes::BUTTON) as u64);
    let err = native::host_nodes(m, host_root.at(), None, &[(wrong_class.at(), Look::Icon)]).unwrap_err();
    assert!(err.contains("unrecognized attached visual node group"));
}
