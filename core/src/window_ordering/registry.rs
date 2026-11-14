//! Logical identities and native incarnations, confined to related scopes.
//! No native function is called while the registry is borrowed.
use super::{
    course::{Course, Key, Role, Token},
    native, placement,
};
use crate::{memory::Memory, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Identity {
    Window(usize, String),
    Popup(u32),
}

pub(crate) struct Owned {
    pub identity: Identity,
    pub root: usize,
    pub appearance: u64,
}

struct Entry {
    scope: usize,
    role: Role,
    root: Option<usize>,
    nodes: Vec<usize>,
    token: Option<Token>,
    appearance: u64,
}

struct Scope {
    /// The host's special background and anything before it stay fixed.
    floor: Option<usize>,
    host: Option<usize>,
}

pub(super) struct Change {
    pub parent: usize,
    pub before: Vec<usize>,
    pub after: Vec<usize>,
    pub movable: BTreeSet<usize>,
    tokens: Vec<Token>,
    plan: Option<super::course::Plan>,
}

#[derive(Default)]
pub(super) struct Registry {
    course: Course,
    next: u64,
    entries: BTreeMap<Key, Entry>,
    owned: BTreeMap<Identity, Key>,
    scopes: BTreeMap<usize, Scope>,
    nodes: BTreeMap<usize, Key>,
    hosts: BTreeMap<usize, Key>,
    host_roots: BTreeMap<usize, usize>,
    pub dirty: bool,
    pub held: usize,
}

impl Registry {
    fn key(&mut self) -> Key {
        self.next = self.next.checked_add(1).expect("window identity exhausted");
        self.next
    }

    fn remove(&mut self, key: Key) {
        if let Some(entry) = self.entries.remove(&key) {
            for node in entry.nodes {
                self.nodes.remove(&node);
            }
            self.course.remove(key);
            self.dirty = true;
        }
    }

    fn unbind(&mut self, key: Key) {
        if let Some(entry) = self.entries.get_mut(&key) {
            if let Some(token) = entry.token.take() {
                self.course.unbind(token);
            }
            for node in entry.nodes.drain(..) {
                self.nodes.remove(&node);
            }
            entry.root = None;
            self.dirty = true;
        }
    }

    fn add(&mut self, scope: usize, role: Role, root: Option<usize>, nodes: Vec<usize>, shown: bool) -> Result<Key> {
        let key = self.key();
        self.course.register(key, scope as u64, role, shown)?;
        let token = self.course.bind(key)?;
        for &node in &nodes {
            if self.nodes.insert(node, key).is_some() {
                return Err("a scene node belongs to two ordering entries".into());
            }
        }
        self.entries.insert(key, Entry { scope, role, root, nodes, token: Some(token), appearance: 0 });
        self.dirty = true;
        Ok(key)
    }

    /// Close intent retires logical identities before their native teardown.
    /// Surviving identities remain through definition-generation rebuilds.
    pub fn prepare_owned(&mut self, wanted: &BTreeSet<Identity>) {
        let gone: Vec<_> = self.owned.keys().filter(|id| !wanted.contains(*id)).cloned().collect();
        for identity in gone {
            let key = self.owned.remove(&identity).expect("known owned window");
            self.remove(key);
        }
        let unused: Vec<_> = self
            .scopes
            .iter()
            .filter_map(|(&parent, scope)| {
                (scope.host.is_none() && !self.entries.values().any(|entry| entry.scope == parent && entry.role == Role::Owned)).then_some(parent)
            })
            .collect();
        for parent in unused {
            self.retire_scope(parent);
        }
        self.held += 1;
    }

    /// Bind in request order, not the construction order of the Lua maps.
    pub fn owned(&mut self, m: &Memory, seen: &[Owned]) -> Result<()> {
        let Some((_, scene)) = native::scene(m)? else {
            return Ok(());
        };
        let mut ordered: Vec<_> = seen.iter().collect();
        ordered.sort_by_key(|item| item.appearance);
        for item in ordered {
            let node = native::window_node(m, item.root)?;
            let parent = m.ptr(node + native::PARENT)?;
            if parent != scene + 0x110 {
                return Err("an independent window changed its native scope".into());
            }
            self.scopes.entry(parent).or_insert(Scope { floor: None, host: None });
            let key = if let Some(&key) = self.owned.get(&item.identity) {
                key
            } else {
                // A show observer may have adopted it before reconciliation
                // supplied its logical identity. Forget that temporary entry.
                if let Some(&temporary) = self.nodes.get(&node) {
                    self.remove(temporary);
                }
                let key = self.add(parent, Role::Owned, None, Vec::new(), false)?;
                self.owned.insert(item.identity.clone(), key);
                key
            };
            if self.entries[&key].scope != parent {
                return Err("an owned window changed scope across reconstruction".into());
            }
            if self.entries[&key].nodes != [node] {
                if let Some(&temporary) = self.nodes.get(&node) {
                    if temporary != key {
                        self.remove(temporary);
                    }
                }
                self.unbind(key);
                let token = self.course.bind(key)?;
                let entry = self.entries.get_mut(&key).expect("owned entry");
                entry.nodes = vec![node];
                entry.root = Some(item.root);
                entry.token = Some(token);
                self.nodes.insert(node, key);
            }
            let shown = native::visible(m, item.root, scene + 0x110)?;
            if self.entries[&key].appearance != item.appearance {
                self.course.visible(key, false)?;
                self.entries.get_mut(&key).expect("owned entry").appearance = item.appearance;
            }
            self.dirty |= self.course.visible(key, shown)?;
        }
        Ok(())
    }

    /// Bind the original host's logical identity across native reconstruction.
    /// It participates at top level only while an owned window needs that scope.
    pub fn host(&mut self, m: &Memory, identity: usize, root: usize) -> Result<()> {
        self.host_roots.insert(identity, root);
        let node = native::window_node(m, root)?;
        let parent = m.ptr(node + native::PARENT)?;
        if !self.scopes.contains_key(&parent) {
            return Ok(());
        }
        let Some((_, scene)) = native::scene(m)? else {
            return Ok(());
        };
        let key = self.hosts.get(&identity).copied().filter(|key| self.entries.contains_key(key));
        if let Some(key) = key {
            if self.entries[&key].scope != parent {
                return Err("a host changed its containing scope during reconstruction".into());
            }
            if self.entries[&key].nodes != [node] {
                if let Some(&temporary) = self.nodes.get(&node) {
                    if temporary != key {
                        self.remove(temporary);
                    }
                }
                self.unbind(key);
                let token = self.course.bind(key)?;
                let entry = self.entries.get_mut(&key).expect("host entry");
                entry.root = Some(root);
                entry.nodes = vec![node];
                entry.token = Some(token);
                self.nodes.insert(node, key);
            }
            // Rebinding is not an appearance. A later native display
            // transition is observed separately through the shared methods.
        } else {
            let key = match self.nodes.get(&node).copied() {
                Some(key) => key,
                None => self.add(parent, Role::Original, Some(root), vec![node], native::visible(m, root, scene + 0x110)?)?,
            };
            self.hosts.insert(identity, key);
        }
        Ok(())
    }

    /// Register a complete attached group. The caller supplies all visual
    /// nodes, including a button's optional second node, in native order.
    pub fn attached(&mut self, m: &Memory, host: usize, floor: Option<usize>, nodes: &[usize]) -> Result<()> {
        let parent = native::window_node(m, host)?;
        self.retire_scope(parent);
        if nodes.is_empty() {
            return Ok(());
        }
        let full = placement::snapshot(m, parent)?;
        let requested: BTreeSet<_> = nodes.iter().copied().collect();
        if requested.len() != nodes.len() || !requested.iter().all(|node| full.contains(node)) {
            return Err("attached content has missing or repeated scene nodes".into());
        }
        if floor.is_some_and(|node| requested.contains(&node) || !full.contains(&node)) {
            return Err("the host background is not a fixed scene anchor".into());
        }
        let ordered: Vec<_> = full.into_iter().filter(|node| requested.contains(node)).collect();
        self.scopes.insert(parent, Scope { floor, host: Some(host) });
        self.add(parent, Role::Attached, None, ordered, true)?;
        Ok(())
    }

    fn retire_scope(&mut self, parent: usize) {
        if self.scopes.remove(&parent).is_none() {
            return;
        }
        let gone: Vec<_> = self.entries.iter().filter_map(|(&key, entry)| (entry.scope == parent).then_some(key)).collect();
        for key in gone {
            self.remove(key);
        }
    }

    pub fn detach_host(&mut self, root: usize) {
        let gone: Vec<_> = self.scopes.iter().filter_map(|(&parent, scope)| (scope.host == Some(root)).then_some(parent)).collect();
        for parent in gone {
            self.retire_scope(parent);
        }
    }

    /// Called before Kill/destruction, never after its memory was released.
    /// Framework-owned logical order survives a native incarnation's death.
    pub fn retire(&mut self, root: usize) {
        self.detach_host(root);
        let gone: Vec<_> = self.entries.iter().filter_map(|(&key, entry)| (entry.root == Some(root)).then_some((key, entry.role))).collect();
        for (key, role) in gone {
            if role == Role::Owned || self.hosts.values().any(|host| *host == key) {
                self.unbind(key);
            } else {
                self.remove(key);
            }
        }
        self.host_roots.retain(|_, old| *old != root);
    }

    /// Refresh only registered scopes. Other scene trees remain outside policy.
    fn adopt(&mut self, m: &Memory, parent: usize, first_root: usize) -> Result<Vec<usize>> {
        let full = placement::snapshot(m, parent)?;
        let prefix = match self.scopes[&parent].floor {
            Some(floor) => full.iter().position(|&node| node == floor).ok_or("host background disappeared")? + 1,
            None => 0,
        };
        let included = &full[prefix..];
        let absent: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&key, entry)| {
                (entry.scope == parent && entry.role == Role::Original && entry.nodes.iter().any(|node| !included.contains(node))).then_some(key)
            })
            .collect();
        for key in absent {
            self.remove(key);
        }
        for &node in included {
            if self.nodes.contains_key(&node) {
                continue;
            }
            let root = native::window_of(m, node)?;
            let shown = root.map(|root| native::visible(m, root, first_root)).transpose()?.unwrap_or(false);
            let key = self.add(parent, Role::Original, root, vec![node], shown)?;
            if let Some(root) = root {
                for (&identity, &host_root) in &self.host_roots {
                    if host_root == root {
                        self.hosts.insert(identity, key);
                    }
                }
            }
        }
        // Missing owned nodes are a failed lifetime contract, never an excuse
        // to dereference their old root or reattach a detached native object.
        for entry in self.entries.values().filter(|entry| entry.scope == parent && entry.token.is_some()) {
            if !entry.nodes.iter().all(|node| included.contains(node)) {
                return Err("registered window nodes disappeared without retirement".into());
            }
        }
        Ok(full)
    }

    /// Before a display operation, adopt its still-hidden ordinary window if
    /// it belongs to an already-related scope. No tree mutation happens here.
    pub fn before_visibility(&mut self, m: &Memory, root: usize) -> Result<()> {
        if self.scopes.is_empty() {
            return Ok(());
        }
        let node = native::window_node(m, root)?;
        let parent = m.ptr(node + native::PARENT)?;
        if self.scopes.contains_key(&parent) && !self.nodes.contains_key(&node) {
            let Some((_, scene)) = native::scene(m)? else {
                return Ok(());
            };
            self.adopt(m, parent, scene + 0x110)?;
        }
        Ok(())
    }

    /// Reevaluating registered descendants also covers a parent's root hide,
    /// which suppresses traversal without changing their local open flags.
    pub fn visibility(&mut self, m: &Memory) -> Result<()> {
        if self.scopes.is_empty() {
            return Ok(());
        }
        let Some((_, scene)) = native::scene(m)? else {
            return Ok(());
        };
        for (&key, entry) in &self.entries {
            if entry.role == Role::Owned && self.held != 0 {
                continue;
            }
            if let Some(root) = entry.root {
                self.dirty |= self.course.visible(key, native::visible(m, root, scene + 0x110)?)?;
            }
        }
        Ok(())
    }

    pub fn target(&self, m: &Memory, gui: usize) -> Result<Option<Token>> {
        if self.scopes.is_empty() {
            return Ok(None);
        }
        let Some((_, scene)) = native::scene(m)? else {
            return Ok(None);
        };
        let Some(mut node) = native::target_node(m, scene, gui)? else {
            return Ok(None);
        };
        let mut visited = BTreeSet::new();
        while node != 0 && visited.len() < 1024 && visited.insert(node) {
            if let Some(entry) = self.nodes.get(&node).and_then(|key| self.entries.get(key)) {
                if entry.root.is_some() {
                    return Ok(entry.token);
                }
            }
            if node == scene + 0x110 {
                return Ok(None);
            }
            node = m.ptr(node + native::PARENT)?;
        }
        if node == 0 {
            Ok(None)
        } else {
            Err("input target ancestor chain loops or exceeds its bound".into())
        }
    }

    pub fn activate(&mut self, token: Token) {
        self.dirty |= self.course.activate(token);
    }

    pub fn changes(&mut self, m: &Memory) -> Result<Vec<Change>> {
        if !self.dirty || self.held != 0 {
            return Ok(Vec::new());
        }
        let Some((_, scene)) = native::scene(m)? else {
            return Ok(Vec::new());
        };
        let mut changes = Vec::new();
        for parent in self.scopes.keys().copied().collect::<Vec<_>>() {
            let full = self.adopt(m, parent, scene + 0x110)?;
            let mut keys = BTreeSet::new();
            let tokens = full
                .iter()
                .filter_map(|node| {
                    let key = *self.nodes.get(node)?;
                    if self.entries[&key].scope != parent || !keys.insert(key) {
                        return None;
                    }
                    self.entries[&key].token
                })
                .collect::<Vec<_>>();
            let plan = self.course.plan(parent as u64, &tokens)?;
            let order = plan.as_ref().map_or(tokens.as_slice(), |plan| plan.after.as_slice());
            let prefix = match self.scopes[&parent].floor {
                Some(floor) => full.iter().position(|&node| node == floor).ok_or("host background disappeared")? + 1,
                None => 0,
            };
            let mut after = full[..prefix].to_vec();
            let mut movable = BTreeSet::new();
            for token in order {
                let entry = &self.entries[&token.key];
                after.extend(&entry.nodes);
                if entry.role != Role::Original {
                    movable.extend(&entry.nodes);
                }
            }
            if full != after {
                changes.push(Change { parent, before: full, after, movable, tokens, plan });
            }
        }
        Ok(changes)
    }

    pub fn current(&self, change: &Change) -> bool {
        change.plan.as_ref().is_none_or(|plan| self.course.is_current(plan, &change.tokens))
            && change.tokens.iter().all(|token| self.entries.get(&token.key).is_some_and(|entry| entry.token == Some(*token)))
    }

    pub fn clear(&mut self) {
        self.course.clear();
        self.entries.clear();
        self.owned.clear();
        self.scopes.clear();
        self.nodes.clear();
        self.hosts.clear();
        self.host_roots.clear();
        self.dirty = false;
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
