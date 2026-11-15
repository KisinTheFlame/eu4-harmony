use super::*;
use crate::{
    game::buffers::{Image, Object},
    objects::gui::{classes, globals},
};

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
        self.scene.at() + 0x110
    }
    fn memory(&self) -> &'static Memory {
        self.image.memory()
    }
    fn link(&self, parent: usize, nodes: &[usize]) {
        let m = self.memory();
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
    fn commit(&self, registry: &mut Registry) -> Vec<usize> {
        let changes = registry.changes(self.memory()).unwrap();
        for change in changes {
            assert!(registry.current(&change));
            self.link(change.parent, &change.after);
        }
        registry.dirty = false;
        placement::snapshot(self.memory(), self.parent()).unwrap()
    }
}

#[test]
fn request_order_and_accepted_original_click_share_the_real_registry() {
    let tree = Tree::new(3);
    let mut registry = Registry::default();
    registry.owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    assert_eq!(tree.commit(&mut registry), [tree.nodes[0].at(), tree.nodes[2].at(), tree.nodes[1].at()]);
    let token = registry.target(tree.memory(), tree.roots[2].at() + 0x30).unwrap().unwrap();
    registry.activate(token);
    assert_eq!(tree.commit(&mut registry), [tree.nodes[0].at(), tree.nodes[1].at(), tree.nodes[2].at()]);
    registry.visibility(tree.memory()).unwrap();
    assert!(!registry.dirty, "a repeated native show is inert");
    registry.owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    assert!(!registry.dirty, "an unchanged open request is inert");
    registry.owned(tree.memory(), &[tree.owned(1, 2)]).unwrap();
    assert_eq!(tree.commit(&mut registry), [tree.nodes[0].at(), tree.nodes[2].at(), tree.nodes[1].at()]);
}

#[test]
fn reconstruction_retains_order_and_retirement_prevents_reading_the_old_root() {
    let tree = Tree::new(4);
    let mut registry = Registry::default();
    registry.owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    tree.commit(&mut registry);
    let original = registry.target(tree.memory(), tree.roots[2].at() + 0x30).unwrap().unwrap();
    registry.activate(original);
    tree.commit(&mut registry);
    // Node 3 will become the replacement; remove its temporary native identity.
    registry.retire(tree.roots[3].at());
    registry.retire(tree.roots[1].at());
    tree.roots[1].set(0x30, &0usize.to_le_bytes());
    tree.link(tree.parent(), &[tree.nodes[0].at(), tree.nodes[2].at(), tree.nodes[3].at()]);
    registry.visibility(tree.memory()).unwrap();
    registry.owned(tree.memory(), &[tree.owned(3, 1)]).unwrap();
    assert_eq!(tree.commit(&mut registry), [tree.nodes[0].at(), tree.nodes[3].at(), tree.nodes[2].at()]);
}

#[test]
fn attached_group_stays_after_the_host_background_and_before_the_picker() {
    let tree = Tree::new(4);
    // Host 0 owns a raw background, picker 1, then two attached visual nodes.
    let background = Object::new(0x180);
    let parent = tree.nodes[0].at();
    tree.link(tree.parent(), &[parent]);
    tree.link(parent, &[background.at(), tree.nodes[1].at(), tree.nodes[2].at(), tree.nodes[3].at()]);
    let mut registry = Registry::default();
    registry.attached(tree.memory(), tree.roots[0].at(), Some(background.at()), &[tree.nodes[2].at(), tree.nodes[3].at()]).unwrap();
    let changes = registry.changes(tree.memory()).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].after, [background.at(), tree.nodes[2].at(), tree.nodes[3].at(), tree.nodes[1].at()]);
    assert_eq!(changes[0].movable, [tree.nodes[2].at(), tree.nodes[3].at()].into_iter().collect());
    registry.retire(tree.roots[0].at());
    assert!(registry.scopes.is_empty());
    assert!(registry.entries.is_empty());
}

#[test]
fn rebuilding_a_host_keeps_its_relation_to_an_independent_window() {
    let tree = Tree::new(3);
    tree.link(tree.parent(), &[tree.nodes[0].at(), tree.nodes[1].at()]);
    let mut registry = Registry::default();
    registry.host(tree.memory(), 0, tree.roots[0].at()).unwrap();
    registry.owned(tree.memory(), &[tree.owned(1, 1)]).unwrap();
    tree.commit(&mut registry);
    registry.retire(tree.roots[0].at());
    tree.roots[0].set(0x30, &0usize.to_le_bytes());
    tree.link(tree.parent(), &[tree.nodes[1].at(), tree.nodes[2].at()]);
    registry.host(tree.memory(), 0, tree.roots[2].at()).unwrap();
    assert_eq!(tree.commit(&mut registry), [tree.nodes[2].at(), tree.nodes[1].at()]);
    registry.prepare_owned(&BTreeSet::new());
    assert!(registry.scopes.is_empty(), "without related framework windows, vanilla ordering is no longer observed");
    registry.held -= 1;
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
