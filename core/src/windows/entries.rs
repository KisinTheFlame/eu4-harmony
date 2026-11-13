//! Native lifetime and slot layout of extension entry buttons on the panel
//! button strip (`floating_buttons`), behind an internal seam shared by the
//! running game and failure-injecting process tests.
use super::{Entry, Live, STRIP_BACKGROUND, STRIP_FIRST, STRIP_SLOTS, STRIP_STEP, TEXT_HIDE, TEXT_PLACE, TEXT_PLACE_SLOT, TEXT_SHOW};
use crate::{
    bound_element::{self, BoundElement, Look, Shown},
    memory::Memory,
    objects::{
        campaign::map_manager,
        gui::{classes, element, functions, window},
    },
    ui,
    unicode::game_text,
    Result,
};
use eu4_harmony_lua::windows::ResolvedEntry;

pub(super) struct EntrySpec {
    pub extension: usize,
    pub id: String,
    pub window: String,
    pub resolved: ResolvedEntry,
}

pub(super) trait StripNative {
    fn strip(&mut self, idler: usize) -> Result<Option<usize>>;
    fn vanilla_shown(&mut self, strip: usize, index: usize) -> Result<bool>;
    fn find_entry(&mut self, strip: usize, button_name: &str, background_name: &str) -> Result<Option<(usize, usize, usize)>>;
    fn listener(&mut self, button: usize) -> Result<BoundElement>;
    fn subscribed(&mut self, element: &BoundElement) -> bool;
    fn link(&mut self, element: &mut BoundElement) -> Result<()>;
    fn unlink(&mut self, element: &mut BoundElement) -> Result<()>;
    fn apply(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()>;
    fn show_number(&mut self, number: usize, previous: &mut Option<String>, text: String, at: (i32, i32)) -> Result<()>;
    fn place(&mut self, button: usize, background: usize, at: (i32, i32)) -> Result<()>;
}

struct GameStrip<'a> {
    m: &'a Memory,
}

impl StripNative for GameStrip<'_> {
    fn strip(&mut self, idler: usize) -> Result<Option<usize>> {
        let strip = self.m.ptr(idler + map_manager::PANEL_BUTTONS)?;
        if strip == 0 || self.m.ptr(strip)? != self.m.at(classes::WINDOW) {
            return Ok(None);
        }
        Ok(Some(strip))
    }

    fn vanilla_shown(&mut self, strip: usize, index: usize) -> Result<bool> {
        let button = ui::lookup(self.m, strip, (window::FIND_BUTTON, functions::FIND_BUTTON), &format!("f_button_{index}"))?;
        Ok(button != 0 && self.m.u8(button + element::VISIBLE)? != 0)
    }

    fn find_entry(&mut self, strip: usize, button_name: &str, background_name: &str) -> Result<Option<(usize, usize, usize)>> {
        let button = ui::lookup(self.m, strip, (window::FIND_BUTTON, functions::FIND_BUTTON), button_name)?;
        if button == 0 || ui::element_name(self.m, button)? != button_name.as_bytes() || self.m.ptr(button)? != self.m.at(classes::BUTTON) {
            return Ok(None);
        }
        let background = ui::lookup(self.m, strip, (window::FIND_ICON, functions::FIND_ICON), background_name)?;
        let number_name = format!("{button_name}_number");
        let number = ui::lookup(self.m, strip, (window::FIND_TEXT, functions::FIND_TEXT), &number_name)?;
        if number == 0 || self.m.ptr(number)? != self.m.at(classes::TEXT_BOX) || ui::element_name(self.m, number)? != number_name.as_bytes() {
            return Err(format!("the entry {button_name} has no independent number"));
        }
        Ok(Some((button, background, number)))
    }

    fn listener(&mut self, button: usize) -> Result<BoundElement> {
        BoundElement::listening(self.m, button, Look::Button, bound_element::Hears::Windows)
    }

    fn subscribed(&mut self, element: &BoundElement) -> bool {
        element.subscribed(self.m).unwrap_or(false)
    }

    fn link(&mut self, element: &mut BoundElement) -> Result<()> {
        element.link(self.m)
    }

    fn unlink(&mut self, element: &mut BoundElement) -> Result<()> {
        element.unlink(self.m)
    }

    fn apply(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()> {
        element.apply(self.m, shown)
    }

    fn show_number(&mut self, number: usize, previous: &mut Option<String>, text: String, at: (i32, i32)) -> Result<()> {
        let visible = !text.is_empty();
        if previous.as_ref() != Some(&text) {
            bound_element::write_text(self.m, number, &game_text::from_str(&text))?;
            *previous = Some(text);
        }
        if (self.m.u8(number + element::VISIBLE)? != 0) != visible {
            let (slot, function) = if visible { (element::SHOW, TEXT_SHOW) } else { (element::HIDE, TEXT_HIDE) };
            if self.m.vslot(number, slot)? != self.m.at(function.rva()) {
                return Err("entry number visibility method changed".into());
            }
            function.call(self.m, [number])?;
        }
        if visible {
            if self.m.vslot(number, TEXT_PLACE_SLOT)? != self.m.at(TEXT_PLACE.rva()) {
                return Err("entry number position method changed".into());
            }
            let position = [at.0, at.1];
            TEXT_PLACE.call(self.m, [number, position.as_ptr() as usize])?;
        }
        Ok(())
    }

    fn place(&mut self, button: usize, background: usize, at: (i32, i32)) -> Result<()> {
        if self.m.vslot(button, element::PLACE)? == self.m.at(functions::BUTTON_PLACE.rva()) {
            functions::BUTTON_PLACE.call(self.m, [button, super::pack(at)])?;
        }
        let place = if background != 0 { self.m.vslot(background, element::PLACE)? } else { 0 };
        if background != 0 && functions::ICON_PLACES.iter().any(|&k| self.m.at(k) == place) {
            let mut resolved = 0u64;
            functions::PLACED.call(self.m, [button, &mut resolved as *mut u64 as usize])?;
            let (x, y) = super::unpack(resolved);
            self.m.call(place, &[background, super::pack((x + STRIP_BACKGROUND.0, y + STRIP_BACKGROUND.1))])?;
        }
        Ok(())
    }
}

pub(super) fn sync(m: &Memory, live: &mut Live, idler: usize, specs: &[EntrySpec]) {
    reconcile(&mut GameStrip { m }, live, idler, specs);
}

pub(super) fn retire(m: &Memory, live: &mut Live, gone: impl Fn(&Entry) -> bool) -> Result<()> {
    retire_with(&mut GameStrip { m }, live, gone)
}

pub(super) fn reconcile(native: &mut impl StripNative, live: &mut Live, idler: usize, specs: &[EntrySpec]) {
    let strip = match native.strip(idler) {
        Ok(Some(strip)) => strip,
        Ok(None) => return,
        Err(e) => {
            live.report(format!("entry buttons: {e}"));
            return;
        }
    };
    let mut used = 0;
    for k in 1..=STRIP_SLOTS {
        match native.vanilla_shown(strip, k) {
            Ok(true) => used = k,
            Ok(false) => break,
            Err(e) => {
                live.report(format!("entry buttons: {e}"));
                return;
            }
        }
    }
    let mut slot = used as i32;
    for spec in specs {
        match place_one(native, live, strip, spec, slot) {
            Ok(placed_in_slot) => {
                if placed_in_slot {
                    slot += 1;
                }
            }
            Err(e) => live.report(format!("entry button {}.{}: {e}", spec.id, spec.window)),
        }
    }
}

fn place_one(native: &mut impl StripNative, live: &mut Live, strip: usize, spec: &EntrySpec, slot: i32) -> Result<bool> {
    let (button_name, background_name) = super::entry_names(&spec.id, &spec.window);
    let Some((button, background, number)) = native.find_entry(strip, &button_name, &background_name)? else {
        return Ok(false);
    };
    let known = live.entries.iter().position(|e| e.extension == spec.extension && e.window == spec.window);
    let fresh = match known {
        Some(i) => live.entries[i].strip != strip || live.entries[i].element.object != button || !native.subscribed(&live.entries[i].element),
        None => true,
    };
    if fresh {
        if let Some(i) = known {
            let mut old = live.entries.remove(i);
            if old.strip == strip && old.element.object == button {
                let _ = native.unlink(&mut old.element);
            }
            live.retired.extend(old.element.retire());
        }
        let element = native.listener(button)?;
        // Retain the entry before native `link`, matching `windows::lifecycle`,
        // so a failure after native SUBSCRIBE never frees a live callback.
        live.entries.push(Entry {
            extension: spec.extension,
            window: spec.window.clone(),
            strip,
            element,
            background: BoundElement::new(background, Look::Icon),
            number,
            number_text: None,
        });
        let index = live.entries.len() - 1;
        if let Err(e) = native.link(&mut live.entries[index].element) {
            let mut failed = live.entries.pop().expect("entry retained before link");
            let _ = native.unlink(&mut failed.element);
            live.retired.extend(failed.element.retire());
            return Err(e);
        }
    }
    let bound = live.entries.iter_mut().find(|e| e.extension == spec.extension && e.window == spec.window).expect("entry exists");
    let shown = Shown {
        visible: Some(spec.resolved.visible),
        enabled: Some(spec.resolved.enabled),
        frame: Some(spec.resolved.frame),
        tooltip: Some(game_text::from_str(&spec.resolved.tooltip)),
        ..Default::default()
    };
    native.apply(&mut bound.element, &shown)?;
    if background != 0 {
        native.apply(&mut bound.background, &Shown { visible: Some(spec.resolved.visible), ..Default::default() })?;
    }
    let number_text = if spec.resolved.visible { spec.resolved.text.clone() } else { String::new() };
    let number_at = (STRIP_FIRST.0 - STRIP_STEP * slot - 4, STRIP_FIRST.1 + 40);
    native.show_number(bound.number, &mut bound.number_text, number_text, number_at)?;
    if !spec.resolved.visible {
        return Ok(false);
    }
    let at = (STRIP_FIRST.0 - STRIP_STEP * slot, STRIP_FIRST.1);
    native.place(button, background, at)?;
    Ok(true)
}

pub(super) fn retire_with(native: &mut impl StripNative, live: &mut Live, gone: impl Fn(&Entry) -> bool) -> Result<()> {
    let (old, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut live.entries).into_iter().partition(gone);
    live.entries = kept;
    let mut result = Ok(());
    for mut entry in old {
        result = result.and(native.unlink(&mut entry.element));
        live.retired.extend(entry.element.retire());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct RecordingStrip {
        strip: usize,
        vanilla_used: usize,
        missing_number: HashSet<String>,
        objects: HashMap<(usize, String), (usize, usize, usize)>,
        next_object: usize,
        linked: HashSet<usize>,
        applied: HashMap<usize, Shown>,
        numbers: HashMap<usize, (String, (i32, i32))>,
        placed: HashMap<usize, ((i32, i32), usize)>,
        calls: Vec<&'static str>,
        fail_step: Option<usize>,
    }

    impl RecordingStrip {
        fn new(strip: usize, vanilla_used: usize) -> Self {
            Self { strip, vanilla_used, next_object: 100, ..Default::default() }
        }

        fn step(&mut self, name: &'static str) -> Result<()> {
            self.calls.push(name);
            if self.fail_step == Some(self.calls.len()) {
                self.fail_step = None;
                return Err(format!("injected {name}"));
            }
            Ok(())
        }
    }

    impl StripNative for RecordingStrip {
        fn strip(&mut self, _: usize) -> Result<Option<usize>> {
            self.step("strip")?;
            Ok((self.strip != 0).then_some(self.strip))
        }

        fn vanilla_shown(&mut self, _: usize, index: usize) -> Result<bool> {
            self.step("vanilla")?;
            Ok(index <= self.vanilla_used)
        }

        fn find_entry(&mut self, strip: usize, button_name: &str, _: &str) -> Result<Option<(usize, usize, usize)>> {
            self.step("find")?;
            if self.missing_number.contains(button_name) {
                return Err(format!("the entry {button_name} has no independent number"));
            }
            let entry = *self.objects.entry((strip, button_name.to_string())).or_insert_with(|| {
                let base = self.next_object;
                self.next_object += 3;
                (base, base + 1, base + 2)
            });
            Ok(Some(entry))
        }

        fn listener(&mut self, button: usize) -> Result<BoundElement> {
            self.step("listener")?;
            Ok(BoundElement::new(button, Look::Button))
        }

        fn subscribed(&mut self, element: &BoundElement) -> bool {
            self.linked.contains(&element.object)
        }

        fn link(&mut self, element: &mut BoundElement) -> Result<()> {
            // Retain subscription before checking failure, modeling native SUBSCRIBE
            // registering the callback prior to a subsequent setter failure.
            self.linked.insert(element.object);
            self.step("link")
        }

        fn unlink(&mut self, element: &mut BoundElement) -> Result<()> {
            self.step("unlink")?;
            self.linked.remove(&element.object);
            Ok(())
        }

        fn apply(&mut self, element: &mut BoundElement, shown: &Shown) -> Result<()> {
            self.step("apply")?;
            element.record_for_test(shown);
            self.applied.insert(element.object, shown.clone());
            Ok(())
        }

        fn show_number(&mut self, number: usize, previous: &mut Option<String>, text: String, at: (i32, i32)) -> Result<()> {
            self.step("number")?;
            *previous = Some(text.clone());
            self.numbers.insert(number, (text, at));
            Ok(())
        }

        fn place(&mut self, button: usize, background: usize, at: (i32, i32)) -> Result<()> {
            self.step("place")?;
            self.placed.insert(button, (at, background));
            Ok(())
        }
    }

    fn spec(extension: usize, id: &str, window: &str, visible: bool, enabled: bool, text: &str, tooltip: &str) -> EntrySpec {
        EntrySpec {
            extension,
            id: id.into(),
            window: window.into(),
            resolved: ResolvedEntry {
                sprite: "GFX_entry".into(),
                visible,
                enabled,
                frame: 1,
                value: (!text.is_empty()).then_some(12.0),
                text: text.into(),
                tooltip: tooltip.into(),
            },
        }
    }

    #[test]
    fn entries_compact_slots_after_vanilla_buttons_and_rebind_cleanly_across_strip_rebuilds() {
        let mut native = RecordingStrip::new(10, 2);
        let mut live = Live::default();
        let mut specs = vec![spec(0, "alpha", "tools", true, true, "12", "Alpha Tools"), spec(1, "beta", "ledger", true, false, "0", "Beta Disabled")];
        reconcile(&mut native, &mut live, 1, &specs);
        assert_eq!(live.entries.len(), 2);
        let (first_btn, second_btn) = (live.entries[0].element.object, live.entries[1].element.object);
        assert_eq!(native.placed[&first_btn].0, (STRIP_FIRST.0 - STRIP_STEP * 2, STRIP_FIRST.1));
        assert_eq!(native.placed[&second_btn].0, (STRIP_FIRST.0 - STRIP_STEP * 3, STRIP_FIRST.1));
        assert_eq!(native.numbers[&live.entries[0].number], ("12".into(), (STRIP_FIRST.0 - STRIP_STEP * 2 - 4, STRIP_FIRST.1 + 40)));
        assert_eq!(native.applied[&second_btn].enabled, Some(false));
        assert_eq!(live.entries[1].element.tooltip(), Some(b"Beta Disabled".as_slice()));

        // Hiding the first entry closes the gap and suppresses its number and tooltip.
        specs[0].resolved.visible = false;
        native.placed.clear();
        reconcile(&mut native, &mut live, 1, &specs);
        assert!(!native.placed.contains_key(&first_btn));
        assert_eq!(live.entries[0].element.tooltip(), None);
        assert_eq!(native.numbers[&live.entries[0].number].0, "");
        assert_eq!(native.placed[&second_btn].0, (STRIP_FIRST.0 - STRIP_STEP * 2, STRIP_FIRST.1));

        // Strip destruction unsubscribes only that strip's entries; rebuilding rebinds.
        retire_with(&mut native, &mut live, |e| e.strip == 10).unwrap();
        assert!(live.entries.is_empty() && native.linked.is_empty());
        native.strip = 20;
        specs[0].resolved.visible = true;
        reconcile(&mut native, &mut live, 1, &specs);
        assert_eq!(live.entries.len(), 2);
        assert!(live.entries.iter().all(|e| e.strip == 20));
        retire_with(&mut native, &mut live, |_| true).unwrap();
        assert!(native.linked.is_empty());
    }

    #[test]
    fn failed_entry_steps_isolate_other_entries_and_clean_up_partial_subscriptions() {
        let specs = vec![spec(0, "alpha", "tools", true, true, "5", "Alpha"), spec(1, "beta", "ledger", true, true, "9", "Beta")];
        // A broken first entry does not prevent the second entry from placing in slot 0.
        let mut broken = RecordingStrip::new(10, 0);
        let (bad_btn, _) = super::super::entry_names("alpha", "tools");
        broken.missing_number.insert(bad_btn);
        let mut live = Live::default();
        reconcile(&mut broken, &mut live, 1, &specs);
        assert_eq!(live.entries.len(), 1);
        assert_eq!(live.entries[0].window, "ledger");
        assert_eq!(broken.placed[&live.entries[0].element.object].0, STRIP_FIRST);

        // Inject failure at every step (including `link` after native retention) and verify zero leaked subscriptions.
        let mut baseline = RecordingStrip::new(10, 1);
        reconcile(&mut baseline, &mut Live::default(), 1, &specs[..1]);
        let total = baseline.calls.len();
        for step in 1..=total {
            let mut injected = RecordingStrip::new(10, 1);
            injected.fail_step = Some(step);
            let mut state = Live::default();
            reconcile(&mut injected, &mut state, 1, &specs[..1]);
            retire_with(&mut injected, &mut state, |_| true).unwrap();
            assert!(injected.linked.is_empty(), "subscription leaked after failure at step {step} ({})", baseline.calls[step - 1]);
        }
    }
}
