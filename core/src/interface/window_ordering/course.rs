//! Scope-local window order, independent of native pointers and event hooks.
//!
//! Sequences run from back to front. The native adapter owns the pointers for
//! each token (and the background-before-controls order inside an attached
//! group). It supplies a complete snapshot of the registered, bound entries
//! in a scope, including hidden entries. Unregistered native content is the
//! adapter's responsibility; a plan does not authorize moving it.
//!
//! Registering an already-present window adopts its state. A genuine later
//! appearance is `visible(key, true)` after registration while hidden or after
//! `visible(key, false)`. Native replacement uses `unbind`/`bind`, without a
//! logical visibility transition. Snapshot reads and plan application belong
//! to the same native synchronization region, outside candidate iteration.
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, &'static str>;
pub(super) type Key = u64;
pub(super) type Scope = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    /// Native relative order is a constraint, supplied by every snapshot.
    Original,
    /// A separately opened framework window or pop-up.
    Owned,
    /// Permanent attached content; never independently activated.
    Attached,
}

/// An incarnation, not a pointer. Reusing a native address cannot revive it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Token {
    pub key: Key,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Plan {
    pub scope: Scope,
    pub before: Vec<Token>,
    pub after: Vec<Token>,
    revision: u64,
}

struct Entry {
    scope: Scope,
    role: Role,
    shown: bool,
    lifetime: u64,
    bound: Option<Token>,
}

#[derive(Default)]
struct Within {
    /// Includes unbound logical windows, preserving their place on rebuild.
    order: Vec<Key>,
    revision: u64,
}

struct Activation {
    key: Key,
    lifetime: u64,
    /// Appearance belongs to the logical window; a click to one incarnation.
    native: Option<Token>,
}

#[derive(Default)]
pub(super) struct Course {
    entries: BTreeMap<Key, Entry>,
    scopes: BTreeMap<Scope, Within>,
    pending: Vec<Activation>,
    serial: u64,
}

impl Course {
    fn next(&mut self) -> u64 {
        self.serial = self.serial.checked_add(1).expect("window ordering identity exhausted");
        self.serial
    }

    fn changed(&mut self, scope: Scope) {
        self.scopes.get_mut(&scope).expect("registered window scope").revision = self.next();
    }

    /// Adopt a logical window. Repeated registration is inert; changing its
    /// scope or role requires retiring that identity first.
    pub fn register(&mut self, key: Key, scope: Scope, role: Role, shown: bool) -> Result<()> {
        if let Some(entry) = self.entries.get(&key) {
            return if entry.scope == scope && entry.role == role { Ok(()) } else { Err("a registered window changed scope or role") };
        }
        let lifetime = self.next();
        self.entries.insert(key, Entry { scope, role, shown, lifetime, bound: None });
        self.scopes.entry(scope).or_default().order.push(key);
        self.changed(scope);
        Ok(())
    }

    /// Replace the native incarnation while keeping logical visibility and
    /// placement. The returned token supersedes every earlier one for `key`.
    pub fn bind(&mut self, key: Key) -> Result<Token> {
        let scope = self.entries.get(&key).ok_or("cannot bind an unregistered window")?.scope;
        let token = Token { key, generation: self.next() };
        self.entries.get_mut(&key).expect("registered window").bound = Some(token);
        self.changed(scope);
        Ok(token)
    }

    /// Observe an actual logical visibility transition, not a repeated show
    /// request, a content refresh or a technical teardown/reconstruction.
    pub fn visible(&mut self, key: Key, shown: bool) -> Result<bool> {
        let entry = self.entries.get_mut(&key).ok_or("cannot show an unregistered window")?;
        if entry.shown == shown {
            return Ok(false);
        }
        entry.shown = shown;
        let scope = entry.scope;
        if shown && entry.role != Role::Attached {
            self.pending.push(Activation { key, lifetime: entry.lifetime, native: None });
        }
        self.changed(scope);
        Ok(true)
    }

    /// Record one accepted mouse activation. The adapter captures the token
    /// before native dispatch; an action destroying that incarnation makes
    /// this call inert. This method neither dispatches nor consumes input.
    pub fn activate(&mut self, token: Token) -> bool {
        let Some(entry) = self.entries.get(&token.key) else { return false };
        if entry.bound != Some(token) || !entry.shown || entry.role == Role::Attached {
            return false;
        }
        let scope = entry.scope;
        self.pending.push(Activation { key: token.key, lifetime: entry.lifetime, native: Some(token) });
        self.changed(scope);
        true
    }

    /// Retire just this native incarnation. An old destruction notification
    /// cannot retire a replacement. Open intent and relative order survive.
    pub fn unbind(&mut self, token: Token) -> bool {
        let Some(entry) = self.entries.get_mut(&token.key) else { return false };
        if entry.bound != Some(token) {
            return false;
        }
        entry.bound = None;
        let scope = entry.scope;
        self.changed(scope);
        true
    }

    /// Forget a logical identity, as on extension unload or session reset.
    pub fn remove(&mut self, key: Key) {
        let Some(entry) = self.entries.remove(&key) else { return };
        let within = self.scopes.get_mut(&entry.scope).expect("registered window scope");
        within.order.retain(|&old| old != key);
        self.pending.retain(|event| event.key != key);
        self.changed(entry.scope);
    }

    /// Clear logical state without recycling incarnation numbers.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.scopes.clear();
        self.pending.clear();
    }

    fn snapshot(&self, scope: Scope, snapshot: &[Token]) -> Result<()> {
        let expected = self.entries.values().filter(|entry| entry.scope == scope && entry.bound.is_some()).count();
        if expected != snapshot.len() {
            return Err("the native window snapshot is incomplete");
        }
        let mut seen = BTreeSet::new();
        for token in snapshot {
            let entry = self.entries.get(&token.key).ok_or("the native snapshot contains an unregistered window")?;
            if entry.scope != scope || entry.bound != Some(*token) || !seen.insert(*token) {
                return Err("the native snapshot contains a stale, duplicate or foreign window");
            }
        }
        Ok(())
    }

    /// Compute a retryable placement. Native original order is authoritative;
    /// pending activation changes only framework-related order. A failed
    /// native application needs no rollback here: obtain its actual snapshot
    /// and compute a fresh plan. No operation is returned for unchanged order.
    pub fn plan(&mut self, scope: Scope, snapshot: &[Token]) -> Result<Option<Plan>> {
        self.snapshot(scope, snapshot)?;
        let Some(within) = self.scopes.get(&scope) else { return Ok(None) };
        let previous = within.order.clone();
        let mut order = previous.clone();

        // Keep the framework entries' positions among original slots while
        // allowing the game to reorder its own entries. Unbound originals
        // retain their logical slots until they can appear in a snapshot.
        let mut originals = snapshot.iter().filter_map(|token| (self.entries[&token.key].role == Role::Original).then_some(token.key));
        for key in &mut order {
            let entry = &self.entries[key];
            if entry.role == Role::Original && entry.bound.is_some() {
                *key = originals.next().expect("complete original snapshot");
            }
        }

        for event in std::mem::take(&mut self.pending) {
            let Some(entry) = self.entries.get(&event.key) else { continue };
            if entry.scope != scope {
                self.pending.push(event);
                continue;
            }
            if entry.lifetime != event.lifetime || !entry.shown || event.native.is_some_and(|token| entry.bound != Some(token)) {
                continue;
            }
            match entry.role {
                Role::Owned => {
                    order.retain(|&key| key != event.key);
                    order.push(event.key);
                }
                Role::Original => {
                    let at = order.iter().position(|&key| key == event.key).expect("registered original");
                    let mut moved = Vec::new();
                    let mut kept = Vec::new();
                    for (index, key) in order.into_iter().enumerate() {
                        if index > at && self.entries[&key].role == Role::Owned {
                            moved.push(key);
                        } else {
                            kept.push(key);
                        }
                    }
                    let at = kept.iter().position(|&key| key == event.key).expect("retained original");
                    kept.splice(at..at, moved);
                    order = kept;
                }
                Role::Attached => {}
            }
        }

        // Stable sorting keeps the group's own order, supplied by registration,
        // and never grants a background or permanent button an activation rank.
        order.sort_by_key(|key| self.entries[key].role != Role::Attached);
        if order != previous {
            self.scopes.get_mut(&scope).expect("registered window scope").order = order.clone();
            self.changed(scope);
        }
        let after: Vec<_> = order.iter().filter_map(|key| self.entries[key].bound).collect();
        if after == snapshot {
            return Ok(None);
        }
        Ok(Some(Plan { scope, before: snapshot.to_vec(), after, revision: self.scopes[&scope].revision }))
    }

    /// Revalidate immediately before touching native order. This also rejects
    /// a native order change since planning, even without a framework event.
    pub fn is_current(&self, plan: &Plan, snapshot: &[Token]) -> bool {
        self.scopes.get(&plan.scope).is_some_and(|within| within.revision == plan.revision)
            && plan.before == snapshot
            && self.snapshot(plan.scope, snapshot).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(course: &mut Course, key: Key, scope: Scope, role: Role, shown: bool) -> Token {
        course.register(key, scope, role, shown).unwrap();
        course.bind(key).unwrap()
    }

    /// The same seam as the native adapter, retaining a changed snapshot when
    /// native application fails instead of acknowledging an intended order.
    fn apply(course: &mut Course, scope: Scope, actual: &mut Vec<Token>) {
        if let Some(plan) = course.plan(scope, actual).unwrap() {
            assert!(course.is_current(&plan, actual));
            *actual = plan.after;
        }
    }

    #[test]
    fn opening_reopening_and_repeated_show() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, false);
        let b = add(&mut course, 2, 0, Role::Owned, false);
        let mut actual = vec![a, b];
        course.visible(2, true).unwrap();
        course.visible(1, true).unwrap();
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [b, a]);
        assert!(!course.visible(2, true).unwrap());
        assert!(course.plan(0, &actual).unwrap().is_none());
        course.visible(2, false).unwrap();
        course.visible(2, true).unwrap();
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [a, b]);
    }

    #[test]
    fn mouse_activation_retains_original_relative_order() {
        let mut course = Course::default();
        let b = add(&mut course, 1, 0, Role::Original, true);
        let x = add(&mut course, 2, 0, Role::Owned, true);
        let a = add(&mut course, 3, 0, Role::Original, true);
        let mut actual = vec![b, x, a];
        assert!(course.activate(b));
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [x, b, a]);
        assert!(course.activate(x));
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [b, a, x]);
    }

    #[test]
    fn original_activation_moves_owned_windows_without_reversing_them() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Original, true);
        let x = add(&mut course, 2, 0, Role::Owned, true);
        let b = add(&mut course, 3, 0, Role::Original, true);
        let y = add(&mut course, 4, 0, Role::Owned, true);
        let mut actual = vec![a, x, b, y];
        course.activate(a);
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [x, y, a, b]);
    }

    #[test]
    fn native_original_reordering_is_authoritative() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Original, true);
        let x = add(&mut course, 2, 0, Role::Owned, true);
        let b = add(&mut course, 3, 0, Role::Original, true);
        let mut actual = vec![b, x, a];
        assert!(course.plan(0, &actual).unwrap().is_none());
        course.activate(b);
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [x, b, a]);
    }

    #[test]
    fn child_activation_never_promotes_its_ancestor() {
        let mut course = Course::default();
        let parent = add(&mut course, 1, 0, Role::Original, true);
        let outside = add(&mut course, 2, 0, Role::Owned, true);
        let child = add(&mut course, 3, 1, Role::Owned, true);
        let peer = add(&mut course, 4, 1, Role::Owned, true);
        let mut children = vec![child, peer];
        course.activate(child);
        assert!(course.plan(0, &[parent, outside]).unwrap().is_none());
        apply(&mut course, 1, &mut children);
        assert_eq!(children, [peer, child]);
        assert!(course.plan(0, &[parent, outside]).unwrap().is_none());
    }

    #[test]
    fn attached_content_stays_behind_picker_and_cannot_activate() {
        let mut course = Course::default();
        let picker = add(&mut course, 1, 0, Role::Original, true);
        let frame = add(&mut course, 2, 0, Role::Attached, true);
        let buttons = add(&mut course, 3, 0, Role::Attached, true);
        let mut actual = vec![picker, frame, buttons];
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [frame, buttons, picker]);
        assert!(!course.activate(frame));
        assert!(!course.activate(buttons));
        course.visible(1, false).unwrap();
        assert!(course.plan(0, &actual).unwrap().is_none());
        course.visible(1, true).unwrap();
        assert!(course.plan(0, &actual).unwrap().is_none());
    }

    #[test]
    fn native_reconstruction_preserves_placement() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, true);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        course.unbind(a);
        let replacement = course.bind(1).unwrap();
        let mut actual = vec![b, replacement];
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [replacement, b]);
        assert!(!course.unbind(a));
        assert!(course.plan(0, &actual).unwrap().is_none());
    }

    #[test]
    fn appearance_before_binding_survives_technical_replacement() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, false);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        course.visible(1, true).unwrap();
        course.unbind(a);
        let replacement = course.bind(1).unwrap();
        let mut actual = vec![replacement, b];
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [b, replacement]);
    }

    #[test]
    fn destroyed_click_target_does_not_activate_replacement() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, true);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        course.activate(a);
        course.unbind(a);
        let replacement = course.bind(1).unwrap();
        assert!(!course.activate(a));
        let mut actual = vec![replacement, b];
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [replacement, b]);
    }

    #[test]
    fn close_during_activation_does_not_raise_hidden_window() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, true);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        course.activate(a);
        course.visible(1, false).unwrap();
        assert!(!course.activate(a));
        assert!(course.plan(0, &[a, b]).unwrap().is_none());
    }

    #[test]
    fn forgotten_identity_does_not_inherit_activation() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, false);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        course.visible(1, true).unwrap();
        course.remove(1);
        let replacement = add(&mut course, 1, 0, Role::Owned, true);
        assert!(!course.activate(a));
        assert!(course.plan(0, &[b, replacement]).unwrap().is_none());
    }

    #[test]
    fn plans_expire_on_lifecycle_or_native_order_change() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, true);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        course.activate(a);
        let plan = course.plan(0, &[a, b]).unwrap().unwrap();
        assert!(course.is_current(&plan, &[a, b]));
        assert!(!course.is_current(&plan, &[b, a]));
        course.unbind(a);
        let replacement = course.bind(1).unwrap();
        assert!(!course.is_current(&plan, &[replacement, b]));
        assert!(!course.is_current(&plan, &[a, b]));
    }

    #[test]
    fn failed_application_can_retry_from_partially_changed_native_order() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, true);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        let c = add(&mut course, 3, 0, Role::Owned, true);
        course.activate(a);
        let plan = course.plan(0, &[a, b, c]).unwrap().unwrap();
        assert_eq!(plan.after, [b, c, a]);
        let mut actual = vec![b, a, c];
        assert!(!course.is_current(&plan, &actual));
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [b, c, a]);
        assert!(course.plan(0, &actual).unwrap().is_none());
    }

    #[test]
    fn incomplete_snapshot_keeps_pending_activation_for_retry() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Owned, true);
        let b = add(&mut course, 2, 0, Role::Owned, true);
        course.activate(a);
        assert!(course.plan(0, &[a]).is_err());
        assert!(course.plan(0, &[a, a]).is_err());
        let mut actual = vec![a, b];
        apply(&mut course, 0, &mut actual);
        assert_eq!(actual, [b, a]);
    }

    #[test]
    fn original_only_scope_is_not_subject_to_activation_policy() {
        let mut course = Course::default();
        let a = add(&mut course, 1, 0, Role::Original, true);
        let b = add(&mut course, 2, 0, Role::Original, true);
        course.activate(a);
        assert!(course.plan(0, &[a, b]).unwrap().is_none());
        assert!(course.plan(0, &[b, a]).unwrap().is_none());
    }

    #[test]
    fn reset_does_not_recycle_tokens() {
        let mut course = Course::default();
        let old = add(&mut course, 1, 0, Role::Owned, true);
        course.clear();
        let new = add(&mut course, 1, 0, Role::Owned, true);
        assert_ne!(old, new);
        assert!(!course.activate(old));
        assert!(!course.unbind(old));
        assert!(course.plan(0, &[new]).unwrap().is_none());
    }
}
