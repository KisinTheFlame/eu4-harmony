use super::*;
use eu4_harmony_lua::windows::Wanted;
use serde_json::json;
use std::collections::HashMap;

#[derive(Default)]
struct Recording {
    next: usize,
    // Every object is owned by a root, a list, or another row window.
    owners: HashMap<usize, usize>,
    alive: HashSet<usize>,
    linked: HashSet<usize>,
    found: HashMap<(usize, String), usize>,
    positions: HashMap<usize, (i32, i32)>,
    applied: HashMap<usize, BTreeMap<&'static str, Property>>,
    calls: Vec<(&'static str, usize)>,
    fail: Option<usize>,
    fail_named: Option<&'static str>,
    lost: Vec<u32>,
    dragged: Vec<(usize, String, (i32, i32))>,
    available: bool,
    rows: HashSet<usize>,
    fail_row_apply: bool,
}
impl Recording {
    fn new() -> Self {
        Self { available: true, ..Self::default() }
    }
    fn call(&mut self, what: &'static str, object: usize) -> Result<()> {
        self.calls.push((what, object));
        if self.fail == Some(self.calls.len()) || self.fail_named == Some(what) {
            self.fail = None;
            self.fail_named = None;
            Err(format!("injected {what}"))
        } else {
            Ok(())
        }
    }
    fn allocate(&mut self, parent: usize) -> usize {
        self.next += 1;
        let object = self.next;
        self.owners.insert(object, parent);
        self.alive.insert(object);
        object
    }
    fn beneath(&self, mut object: usize, ancestor: usize) -> bool {
        loop {
            if object == ancestor {
                return true;
            }
            object = self.owners.get(&object).copied().unwrap_or(0);
            if object == 0 {
                return false;
            }
        }
    }
    fn destroy(&mut self, root: usize, include_root: bool) {
        let objects: Vec<_> = self.alive.iter().copied().filter(|&object| self.beneath(object, root) && (include_root || object != root)).collect();
        for object in objects {
            assert!(!self.linked.contains(&object), "destroyed a subscribed object {object}");
            self.alive.remove(&object);
        }
    }
    fn occurrences(&self, what: &str) -> usize {
        self.calls.iter().filter(|(name, _)| *name == what).count()
    }
}
impl Native for Recording {
    fn available(&mut self) -> Result<bool> {
        self.call("available", 0)?;
        Ok(self.available)
    }
    fn screen(&mut self) -> Result<(i32, i32)> {
        self.call("screen", 0)?;
        Ok((1200, 900))
    }
    fn read(&mut self, path: &str, _: String) -> Result<()> {
        self.call(if path == live::WINDOWS_FILE { "definitions" } else { "popup definitions" }, 0)
    }
    fn create(&mut self, _: &str) -> Result<usize> {
        self.call("create", 0)?;
        Ok(self.allocate(0))
    }
    fn close_panels(&mut self) -> Result<()> {
        self.call("close panels", 0)
    }
    fn position(&mut self, root: usize, at: (i32, i32)) -> Result<()> {
        self.call("position", root)?;
        self.positions.insert(root, at);
        Ok(())
    }
    fn placed(&mut self, root: usize) -> Result<(i32, i32)> {
        self.call("placed", root)?;
        Ok(self.positions[&root])
    }
    fn find(&mut self, root: usize, _: Option<&Kind>, name: &str) -> Result<usize> {
        self.call("find", root)?;
        assert!(self.alive.contains(&root));
        let key = (root, name.to_string());
        if let Some(&object) = self.found.get(&key).filter(|object| self.alive.contains(object)) {
            return Ok(object);
        }
        let object = self.allocate(root);
        self.found.insert(key, object);
        Ok(object)
    }
    fn listener(&mut self, object: usize, look: Look) -> Result<BoundElement> {
        self.call("listener", object)?;
        Ok(BoundElement::new(object, look))
    }
    fn link(&mut self, element: &mut BoundElement) -> Result<()> {
        assert!(self.alive.contains(&element.object));
        // A native call can retain the callback and then fail.
        self.linked.insert(element.object);
        self.call("link", element.object)
    }
    fn unlink(&mut self, element: &mut BoundElement) -> Result<()> {
        assert!(self.alive.contains(&element.object), "unsubscribed after native destruction");
        self.call("unlink", element.object)?;
        self.linked.remove(&element.object);
        Ok(())
    }
    fn apply(&mut self, element: &mut BoundElement, want: &BTreeMap<&'static str, Property>) -> Result<()> {
        assert!(self.alive.contains(&element.object));
        self.call("apply", element.object)?;
        if self.fail_row_apply && self.owners.get(&element.object).is_some_and(|parent| self.rows.contains(parent)) {
            self.fail_row_apply = false;
            return Err("injected row apply".into());
        }
        self.applied.insert(element.object, want.clone());
        Ok(())
    }
    fn keep_still(&mut self, root: usize) -> Result<()> {
        self.call("keep still", root)
    }
    fn show(&mut self, root: usize, shown: bool) -> Result<()> {
        assert!(self.alive.contains(&root));
        self.call(if shown { "show" } else { "hide" }, root)
    }
    fn kill(&mut self, root: usize) -> Kill {
        if let Err(e) = self.call("kill check", root) {
            return Kill::NotCalled(e);
        }
        let result = self.call("kill", root);
        // A failed return does not prove that the root survived. Model the
        // native side effect happening before the fault and the sweep.
        self.destroy(root, true);
        Kill::Attempted(result)
    }
    fn clear_rows(&mut self, list: usize) -> Result<()> {
        self.call("clear rows", list)?;
        self.destroy(list, false);
        Ok(())
    }
    fn row(&mut self, list: usize, _: &str) -> Result<usize> {
        self.call("row", list)?;
        let row = self.allocate(list);
        self.rows.insert(row);
        Ok(row)
    }
    fn popup_text(&mut self, root: usize, _: &str) -> Result<()> {
        self.call("popup text", root)
    }
    fn country(&mut self, object: usize, _: &str) -> Result<()> {
        self.call("country", object)
    }
    fn player(&mut self) -> Option<String> {
        Some("FRA".into())
    }
    fn dragged(&mut self, extension: usize, name: &str, at: (i32, i32)) {
        self.dragged.push((extension, name.into(), at));
    }
    fn lost_popup(&mut self, number: u32) {
        self.lost.push(number);
    }
}

fn wants() -> Wants {
    let dictionary = BTreeMap::from([
        ("PRESS".to_string(), "Press".to_string()),
        ("ROW".to_string(), "Row".to_string()),
        ("LABEL".to_string(), "Default $N$".to_string()),
        ("WHY".to_string(), "Reason".to_string()),
    ]);
    let windows = layout::read(
        &json!({ "tools": { "size": [400, 300], "panel": true, "items": [
            { "type": "button", "name": "press", "at": [5, 5], "size": [80, 20], "text": "PRESS" },
            { "type": "list", "name": "results", "at": [5, 40], "size": [300, 120], "row": { "size": [280, 40], "items": [
                { "type": "button", "name": "row_press", "size": [80, 20], "text": "ROW" },
                { "type": "text", "name": "label", "at": [90, 0], "size": [160, 20], "text": { "key": "LABEL", "values": { "N": 2 } }, "tooltip": "WHY" }
            ]}}
        ]}}),
        &|_| true,
    )
    .unwrap();
    let wanted =
        Wanted { open: true, rows: BTreeMap::from([("results".into(), vec![vec![("label".into(), Property::Text("First".into()))]])]), ..Wanted::default() };
    Wants {
        generation: 1,
        definitions: vec!["definition".into()],
        windows: vec![WantWindow {
            extension: 4,
            type_name: "tools_1".into(),
            layout: windows[0].clone(),
            words: std::sync::Arc::new(move |key| dictionary.get(key).cloned()),
            wanted,
        }],
        popups: vec![],
    }
}
fn popup() -> Popup {
    Popup { header: None, title: "Title".into(), text: "Text".into(), buttons: vec!["Yes".into(), "No".into()], shields: [Some("FRA".into()), None] }
}

#[test]
fn repeated_frames_keep_objects_and_localized_rows_then_replace_them_before_destroying() {
    let (mut native, mut live, mut wants) = (Recording::new(), Live::default(), wants());
    wants.windows[0].wanted.rows.get_mut("results").unwrap().push(vec![]);
    reconcile(&mut native, &mut live, &wants).unwrap();
    let root = live.built[0].root;
    let creates = native.occurrences("create");
    let clears = native.occurrences("clear rows");
    let row = live.built[0].elements.iter().find(|e| e.name == "label").unwrap();
    assert_eq!(native.applied[&row.element.object]["text"], Property::Text("First".into()));
    assert_eq!(native.applied[&row.element.object]["tooltip"], Property::Tooltip("Reason".into()));
    let old_row = row.element.object;
    let default = live.built[0].elements.iter().find(|e| e.name == "label" && e.row.as_ref().is_some_and(|(_, number)| *number == 2)).unwrap();
    assert_eq!(native.applied[&default.element.object]["text"], Property::Text("Default 2".into()));
    assert_eq!(native.applied[&default.element.object]["tooltip"], Property::Tooltip("Reason".into()));
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_eq!(native.occurrences("create"), creates);
    assert_eq!(native.occurrences("clear rows"), clears);
    wants.windows[0].wanted.rows.insert("results".into(), vec![vec![("label".into(), Property::Text("Second".into()))]]);
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_eq!(live.built[0].root, root);
    assert!(!native.alive.contains(&old_row));
    assert!(live.built[0].elements.iter().all(|e| e.element.object != old_row));
    assert_eq!(native.occurrences("clear rows"), clears + 1);
    end_session(&mut native, &mut live).unwrap();
    assert!(native.alive.is_empty());
    assert!(native.linked.is_empty());
}

#[test]
fn every_fallible_window_build_step_preserves_ownership_and_can_be_cleaned() {
    let wants = wants();
    let (mut baseline, mut live) = (Recording::new(), Live::default());
    reconcile(&mut baseline, &mut live, &wants).unwrap();
    let operations = baseline.calls.clone();
    for (index, (operation, _)) in operations.iter().enumerate() {
        let (mut native, mut live) = (Recording::new(), Live::default());
        native.fail = Some(index + 1);
        let _ = reconcile(&mut native, &mut live, &wants);
        assert!(native.fail.is_none(), "{operation} was not reached");
        end_session(&mut native, &mut live).unwrap();
        assert!(native.alive.is_empty(), "objects after failed {operation}");
        assert!(native.linked.is_empty(), "subscriptions after failed {operation}");
    }
}

#[test]
fn a_failed_row_update_is_not_committed_and_the_earlier_contents_can_be_requested_again() {
    let (mut native, mut live, mut wants) = (Recording::new(), Live::default(), wants());
    reconcile(&mut native, &mut live, &wants).unwrap();
    let original = wants.windows[0].wanted.rows.clone();
    wants.windows[0].wanted.rows.insert("results".into(), vec![vec![("label".into(), Property::Text("Changed".into()))]]);
    // Fail after the replacement row has been allocated and subscribed.
    let row_calls = native.occurrences("row");
    native.fail_row_apply = true;
    assert!(reconcile(&mut native, &mut live, &wants).is_err());
    assert!(!native.fail_row_apply);
    assert!(!live.built[0].rows.contains_key("results"));
    assert!(live.built[0].elements.iter().all(|e| e.row.is_none()));
    wants.windows[0].wanted.rows = original;
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert!(native.occurrences("row") > row_calls);
    let row = live.built[0].elements.iter().find(|e| e.name == "label").unwrap();
    assert_eq!(native.applied[&row.element.object]["text"], Property::Text("First".into()));
    end_session(&mut native, &mut live).unwrap();
    assert!(native.alive.is_empty());
}

#[test]
fn definition_failures_are_retried_and_a_new_generation_retires_old_roots() {
    let (mut native, mut live, mut wants) = (Recording::new(), Live::default(), wants());
    native.fail_named = Some("definitions");
    assert!(reconcile(&mut native, &mut live, &wants).is_err());
    assert_eq!(live.read, None);
    reconcile(&mut native, &mut live, &wants).unwrap();
    let old = live.built[0].root;
    wants.generation = 2;
    wants.windows[0].type_name = "tools_2".into();
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert!(!native.alive.contains(&old));
    assert_ne!(live.built[0].root, old);
    assert_eq!(live.read, Some(2));
    end_session(&mut native, &mut live).unwrap();
}

#[test]
fn a_failed_build_waits_for_fresh_definitions_and_session_end_forgets_failure() {
    let (mut native, mut live, mut wants) = (Recording::new(), Live::default(), wants());
    native.fail_named = Some("create");
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert!(live.built.is_empty());
    let attempted = native.occurrences("create");
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_eq!(native.occurrences("create"), attempted);
    wants.generation += 1;
    wants.windows[0].type_name = "tools_2".into();
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_eq!(live.built.len(), 1);
    end_session(&mut native, &mut live).unwrap();
    assert!(live.failed.is_empty());
    assert!(live.popups_read.is_empty());
    assert_eq!(live.read, None);
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_eq!(live.built.len(), 1);
    end_session(&mut native, &mut live).unwrap();
}

#[test]
fn native_destruction_is_distinct_from_ours_and_duplicate_notifications_are_harmless() {
    let (mut native, mut live, wants) = (Recording::new(), Live::default(), wants());
    reconcile(&mut native, &mut live, &wants).unwrap();
    let root = live.built[0].root;
    native_killed(&mut native, &mut live, root).unwrap();
    assert!(live.built.is_empty());
    let calls = native.calls.len();
    native_killed(&mut native, &mut live, root).unwrap();
    assert_eq!(native.calls.len(), calls);
    assert_eq!(native.occurrences("kill"), 0, "the game's own Kill must not be called recursively");
    native.destroy(root, true);
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_ne!(live.built[0].root, root);
    end_session(&mut native, &mut live).unwrap();
}

#[test]
fn popup_definitions_commit_only_after_reading_and_every_build_stage_rolls_back() {
    let (mut native, mut live, mut wants) = (Recording::new(), Live::default(), wants());
    wants.windows.clear();
    wants.popups.push((1, popup()));
    native.fail_named = Some("popup definitions");
    assert!(reconcile(&mut native, &mut live, &wants).is_err());
    assert!(!live.popups_read.contains(&1));
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert!(live.popups_read.contains(&1));
    assert_eq!(live.built.len(), 1);
    let calls = native.calls.clone();
    end_session(&mut native, &mut live).unwrap();
    let (mut baseline, mut baseline_live) = (Recording::new(), Live::default());
    reconcile(&mut baseline, &mut baseline_live, &wants).unwrap();
    for failure in 1..=baseline.calls.len() {
        let (mut native, mut live) = (Recording::new(), Live::default());
        native.fail = Some(failure);
        let _ = reconcile(&mut native, &mut live, &wants);
        end_session(&mut native, &mut live).unwrap();
        assert!(native.alive.is_empty(), "objects after {:?}", baseline.calls[failure - 1]);
        assert!(native.linked.is_empty());
    }
    assert!(calls.iter().any(|(name, _)| *name == "popup text"));
}

#[test]
fn dragged_position_errors_keep_ownership_and_only_an_uncalled_kill_may_be_retried() {
    let (mut native, mut live, mut wants) = (Recording::new(), Live::default(), wants());
    wants.windows[0].layout.moveable = true;
    reconcile(&mut native, &mut live, &wants).unwrap();
    let root = live.built[0].root;
    native.positions.insert(root, (25, 35));
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_eq!(native.dragged, [(4, "tools".into(), (25, 35))]);
    native.fail_named = Some("placed");
    assert!(reconcile(&mut native, &mut live, &wants).is_err());
    assert_eq!(live.built[0].root, root);
    native.fail_named = Some("kill check");
    assert!(end_session(&mut native, &mut live).is_err());
    assert_eq!(live.closing.len(), 1);
    assert!(native.alive.contains(&root));
    end_session(&mut native, &mut live).unwrap();
    assert!(live.closing.is_empty());
    assert!(native.alive.is_empty());
}

#[test]
fn an_entered_kill_is_never_retried_even_if_its_failure_already_freed_the_root() {
    let (mut native, mut live, wants) = (Recording::new(), Live::default(), wants());
    reconcile(&mut native, &mut live, &wants).unwrap();
    let old_root = live.built[0].root;
    native.fail_named = Some("kill");
    end_session(&mut native, &mut live).unwrap();
    assert!(live.built.is_empty());
    assert!(live.closing.is_empty());
    assert!(!native.alive.contains(&old_root));
    assert!(live.reported.iter().any(|message| message.contains("ownership is uncertain")));
    let after_fault = native.calls.len();
    end_session(&mut native, &mut live).unwrap();
    reconcile(&mut native, &mut live, &wants).unwrap();
    assert_ne!(live.built[0].root, old_root);
    assert!(native.calls[after_fault..].iter().all(|(_, object)| *object != old_root));
    end_session(&mut native, &mut live).unwrap();
}

#[test]
fn opening_suppression_is_nested_and_is_restored_after_a_failure() {
    assert!(!live::OPENING.load(std::sync::atomic::Ordering::Relaxed));
    {
        let _opening = Opening::new();
        assert!(live::OPENING.load(std::sync::atomic::Ordering::Relaxed));
        {
            let _nested = Opening::new();
        }
        assert!(live::OPENING.load(std::sync::atomic::Ordering::Relaxed));
    }
    assert!(!live::OPENING.load(std::sync::atomic::Ordering::Relaxed));
}
