//! The native lifetime of extension windows, pop-ups and their list rows.
//!
//! The same reconciliation runs against the game and a recording native
//! adapter in tests. A created root is owned before any further fallible
//! operation, subscriptions are retired before their objects die, and an
//! applied generation or list is committed only after it succeeds.
use super::gui;
use super::{self as live, Bound, Built, Key, Live, WantWindow, Wants};
use crate::{
    bound_element::{BoundElement, Hears, Look, Shown},
    game_heap,
    memory::Memory,
    objects::gui::{functions, globals, list_box, list_entry, window},
    ui, Result,
};
use eu4_harmony_lua::{
    layout::{self, Kind, Placed},
    Popup, Property,
};
use std::collections::{BTreeMap, HashSet};

/// Whether native Kill was entered matters more than whether it returned:
/// an entered call may mark the root dead before it faults. No pointer from
/// an attempted Kill may be read or retried by the framework afterwards.
enum Kill {
    NotCalled(String),
    Attempted(Result<()>),
}

/// Native operations behind the lifetime protocol; neither the extension
/// state nor a caller decides when these operations happen.
trait Native {
    fn available(&mut self) -> Result<bool>;
    fn screen(&mut self) -> Result<(i32, i32)>;
    fn read(&mut self, path: &str, text: String) -> Result<()>;
    fn create(&mut self, name: &str) -> Result<usize>;
    fn close_panels(&mut self) -> Result<()>;
    fn position(&mut self, root: usize, at: (i32, i32)) -> Result<()>;
    fn placed(&mut self, root: usize) -> Result<(i32, i32)>;
    fn find(&mut self, root: usize, kind: Option<&Kind>, name: &str) -> Result<usize>;
    fn listener(&mut self, object: usize, look: Look) -> Result<BoundElement>;
    fn link(&mut self, element: &mut BoundElement) -> Result<()>;
    fn unlink(&mut self, element: &mut BoundElement) -> Result<()>;
    fn apply(&mut self, element: &mut BoundElement, want: &BTreeMap<&'static str, Property>) -> Result<()>;
    fn keep_still(&mut self, root: usize) -> Result<()>;
    fn show(&mut self, root: usize, shown: bool) -> Result<()>;
    fn kill(&mut self, root: usize) -> Kill;
    fn clear_rows(&mut self, list: usize) -> Result<()>;
    fn row(&mut self, list: usize, name: &str) -> Result<usize>;
    fn popup_text(&mut self, root: usize, text: &str) -> Result<()>;
    fn country(&mut self, object: usize, tag: &str) -> Result<()>;
    fn player(&mut self) -> Option<String>;
    fn dragged(&mut self, extension: usize, name: &str, at: (i32, i32));
    fn lost_popup(&mut self, number: u32);
}

struct Game<'a> {
    m: &'a Memory,
    manager: usize,
}

impl Native for Game<'_> {
    fn available(&mut self) -> Result<bool> {
        self.manager = self.m.ptr(self.m.at(globals::MANAGER))?;
        Ok(self.manager != 0)
    }
    fn screen(&mut self) -> Result<(i32, i32)> {
        live::screen(self.m, self.manager)
    }
    fn read(&mut self, path: &str, text: String) -> Result<()> {
        live::read_definitions(self.m, self.manager, path, text)
    }
    fn create(&mut self, name: &str) -> Result<usize> {
        live::create(self.m, self.manager, name)
    }
    fn close_panels(&mut self) -> Result<()> {
        let idler = live::idler(self.m)?;
        let _opening = Opening::new();
        live::CLOSE_VIEWS.call(self.m, [idler]).and_then(|_| live::CLOSE_PANELS.call(self.m, [idler])).map(|_| ())
    }
    fn position(&mut self, root: usize, at: (i32, i32)) -> Result<()> {
        live::set_position(self.m, root, at)
    }
    fn placed(&mut self, root: usize) -> Result<(i32, i32)> {
        self.m.u64(root + window::POSITION).map(live::unpack)
    }
    fn find(&mut self, root: usize, kind: Option<&Kind>, name: &str) -> Result<usize> {
        live::find(self.m, root, kind, name)
    }
    fn listener(&mut self, object: usize, look: Look) -> Result<BoundElement> {
        BoundElement::listening(self.m, object, look, Hears::Windows)
    }
    fn link(&mut self, element: &mut BoundElement) -> Result<()> {
        element.link(self.m)
    }
    fn unlink(&mut self, element: &mut BoundElement) -> Result<()> {
        element.unlink(self.m)
    }
    fn apply(&mut self, element: &mut BoundElement, want: &BTreeMap<&'static str, Property>) -> Result<()> {
        element.apply(self.m, &Shown::of(want))
    }
    fn keep_still(&mut self, root: usize) -> Result<()> {
        ui::keep_still(self.m, root, gui::BACKGROUND)
    }
    fn show(&mut self, root: usize, shown: bool) -> Result<()> {
        live::show_window(self.m, root, shown)
    }
    fn kill(&mut self, root: usize) -> Kill {
        let checked =
            self.m.vslot(root, window::KILL).and_then(
                |kill| {
                    if kill == self.m.at(ui::WINDOW_KILL) {
                        Ok(kill)
                    } else {
                        Err("a window's kill method changed".into())
                    }
                },
            );
        match checked {
            Err(e) => Kill::NotCalled(e),
            Ok(kill) => Kill::Attempted(self.m.call(kill, &[root]).map(|_| ())),
        }
    }
    fn clear_rows(&mut self, list: usize) -> Result<()> {
        for (slot, function) in [(list_box::CLEAR, functions::LIST_CLEAR.rva()), (list_box::ADD, functions::LIST_ADD.rva())] {
            if self.m.vslot(list, slot)? != self.m.at(function) {
                return Err("a list box's method is not the one the framework knows".into());
            }
        }
        functions::LIST_CLEAR.call(self.m, [list]).map(|_| ())
    }
    fn row(&mut self, list: usize, name: &str) -> Result<usize> {
        let entry = game_heap::new(self.m, list_entry::SIZE)?;
        if entry == 0 {
            return Err("the game allocated no list entry".into());
        }
        live::with_string(self.m, name.as_bytes(), |t| functions::LIST_ENTRY.call(self.m, [entry, self.manager, t]))?;
        let root = self.m.ptr(entry + list_entry::WINDOW)?;
        if root == 0 {
            game_heap::delete(self.m, entry, list_entry::SIZE)?;
            return Err(format!("the game built no row {name}"));
        }
        // The list owns the entry from here, including when later binding
        // fails. Reconciliation then retires its listeners and clears it.
        functions::LIST_ADD.call(self.m, [list, entry, 1])?;
        Ok(root)
    }
    fn popup_text(&mut self, root: usize, text: &str) -> Result<()> {
        let text_box = live::find(self.m, root, Some(&Kind::Text { text: None, font: layout::Font::Body, align: layout::Align::Start }), gui::TEXT)?;
        crate::bound_element::write_text(self.m, text_box, &crate::unicode::game_text::from_str(text))
    }
    fn country(&mut self, object: usize, tag: &str) -> Result<()> {
        crate::bound_element::set_country(self.m, object, tag)
    }
    fn player(&mut self) -> Option<String> {
        live::player_letters(self.m)
    }
    fn dragged(&mut self, extension: usize, name: &str, at: (i32, i32)) {
        super::with(|all| all.wanted.dragged(extension, name, at));
    }
    fn lost_popup(&mut self, number: u32) {
        super::with(|all| all.wanted.lost_popup(number));
    }
}

/// Native closing notifications during our own opening are ignored even
/// when an operation fails or unwinds through the framework's guard.
struct Opening(bool);
impl Opening {
    fn new() -> Self {
        Self(live::OPENING.swap(true, std::sync::atomic::Ordering::Relaxed))
    }
}
impl Drop for Opening {
    fn drop(&mut self) {
        live::OPENING.store(self.0, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(super) fn frame(m: &Memory, live: &mut Live, wants: &Wants) -> Result<()> {
    reconcile(&mut Game { m, manager: 0 }, live, wants)
}

fn reconcile(native: &mut impl Native, live: &mut Live, wants: &Wants) -> Result<()> {
    if !native.available()? {
        return Ok(());
    }
    retry_closing(native, live)?;
    if live.read != Some(wants.generation) {
        remove(native, live, |b| matches!(b.key, Key::Window(..)))?;
        if !wants.definitions.is_empty() {
            native.read(live::WINDOWS_FILE, gui::file(&wants.definitions))?;
        }
        live.read = Some(wants.generation);
        live.failed.clear();
    }
    let open_popups: HashSet<u32> = wants.popups.iter().map(|(n, _)| *n).collect();
    remove(native, live, |b| match &b.key {
        Key::Popup(n) => !open_popups.contains(n),
        Key::Window(e, w) => !wants.windows.iter().any(|ww| ww.extension == *e && &ww.layout.name == w && ww.wanted.open),
    })?;
    let screen = native.screen()?;
    for want in &wants.windows {
        let key = Key::Window(want.extension, want.layout.name.clone());
        if want.wanted.open && !live.built.iter().any(|b| b.key == key) && !live.failed.contains(&want.type_name) {
            match build_window(native, live, key.clone(), want, screen) {
                Ok(built) => live.built.push(built),
                Err(e) => {
                    live.failed.insert(want.type_name.clone());
                    live.report(format!("{} not built: {e}", want.layout.name));
                }
            }
        }
        let Some(at) = live.built.iter().position(|b| b.key == key) else { continue };
        let mut built = live.built.remove(at);
        let result: Result<()> = (|| {
            apply_window(native, live, &mut built, want)?;
            if built.moveable {
                let now = native.placed(built.root)?;
                if now != built.placed {
                    built.placed = now;
                    native.dragged(want.extension, &want.layout.name, now);
                }
            }
            Ok(())
        })();
        // An error never loses the root or the partially applied bindings.
        live.built.push(built);
        result?;
    }
    for (number, popup) in &wants.popups {
        if live.built.iter().any(|b| b.key == Key::Popup(*number)) {
            continue;
        }
        let name = format!("eu4h_popup_{number}");
        if !live.popups_read.contains(number) {
            native.read(live::POPUPS_FILE, gui::file(&[gui::popup(&name, popup)]))?;
            live.popups_read.insert(*number);
        }
        let shown = live.built.iter().filter(|b| matches!(b.key, Key::Popup(_))).count() as i32 % 8;
        match build_popup(native, live, *number, popup, &name, screen, shown) {
            Ok(built) => live.built.push(built),
            Err(e) => {
                live.report(format!("pop-up {number} not built: {e}"));
                native.lost_popup(*number);
            }
        }
    }
    Ok(())
}

fn unbind(native: &mut impl Native, live: &mut Live, elements: &mut Vec<Bound>) -> Result<()> {
    let mut result = Ok(());
    for mut bound in elements.drain(..) {
        let unlinked = native.unlink(&mut bound.element);
        live.retired.extend(bound.element.retire());
        result = result.and(unlinked);
    }
    result
}

fn tear_down(native: &mut impl Native, live: &mut Live, built: &mut Built) -> Result<()> {
    if let Err(e) = unbind(native, live, &mut built.elements) {
        live.report(format!("window subscriptions at teardown: {e}"));
    }
    // Hiding is best effort: a failed hide must not prevent Kill. Retired
    // callbacks remain alive even when native unsubscription failed.
    if let Err(e) = native.show(built.root, false) {
        live.report(format!("window hide at teardown: {e}"));
    }
    match native.kill(built.root) {
        Kill::NotCalled(e) => Err(e),
        Kill::Attempted(result) => {
            if let Err(e) = result {
                live.report(format!("window Kill faulted; native ownership is uncertain and the root will not be retried: {e}"));
            }
            // Even a failed native call may have marked it dead. The game's
            // sweep may free it before the next safe frame, so relinquish
            // the pointer rather than trying to prove it readable again.
            Ok(())
        }
    }
}

fn remove(native: &mut impl Native, live: &mut Live, gone: impl Fn(&Built) -> bool) -> Result<()> {
    let (old, kept) = std::mem::take(&mut live.built).into_iter().partition(gone);
    live.built = kept;
    live.closing.extend(old);
    retry_closing(native, live)
}

fn retry_closing(native: &mut impl Native, live: &mut Live) -> Result<()> {
    let mut result = Ok(());
    for mut built in std::mem::take(&mut live.closing) {
        if let Err(e) = tear_down(native, live, &mut built) {
            live.closing.push(built);
            result = result.and(Err(e));
        }
    }
    result
}

fn bind_one(native: &mut impl Native, built: &mut Built, name: String, object: usize, look: Look, listens: bool, row: Option<(String, u32)>) -> Result<()> {
    let element = if listens { native.listener(object, look)? } else { BoundElement::new(object, look) };
    built.elements.push(Bound { name, tooltip_object: object + if look == Look::List { list_box::ELEMENT } else { 0 }, element, row });
    // Link may fail after native code has retained the callback: ownership
    // is already in Built, so rollback will always retire it.
    if listens {
        native.link(&mut built.elements.last_mut().expect("just inserted").element)?;
    }
    Ok(())
}

fn bind(native: &mut impl Native, built: &mut Built, root: usize, widgets: &[Placed], row: Option<(String, u32)>) -> Result<()> {
    for (index, placed) in widgets.iter().enumerate() {
        let name = gui::element(placed, index);
        let object = native.find(root, Some(&placed.kind), &name)?;
        bind_one(
            native,
            built,
            placed.name.clone().unwrap_or(name),
            object,
            live::look(&placed.kind),
            matches!(placed.kind, Kind::Button { .. }),
            row.clone(),
        )?;
    }
    Ok(())
}

fn failed_build(native: &mut impl Native, live: &mut Live, mut built: Built, error: String) -> String {
    if let Err(e) = tear_down(native, live, &mut built) {
        live.report(format!("failed window cleanup: {e}"));
        live.closing.push(built);
    }
    error
}

fn build_window(native: &mut impl Native, live: &mut Live, key: Key, want: &WantWindow, screen: (i32, i32)) -> Result<Built> {
    let layout = &want.layout;
    let root = native.create(&want.type_name)?;
    let at = want.wanted.dragged.unwrap_or_else(|| live::place(layout, screen));
    let mut built = Built { key, root, placed: at, moveable: layout.moveable, elements: Vec::new(), rows: BTreeMap::new() };
    let result = (|| {
        if layout.panel {
            native.close_panels()?;
        }
        native.position(root, at)?;
        if layout.close {
            let close = native.find(root, None, gui::CLOSE)?;
            bind_one(native, &mut built, gui::CLOSE.into(), close, Look::Plain, true, None)?;
        }
        if !layout.moveable {
            native.keep_still(root)?;
        }
        bind(native, &mut built, root, &layout.widgets, None)?;
        native.show(root, true)
    })();
    match result {
        Ok(()) => Ok(built),
        Err(e) => Err(failed_build(native, live, built, e)),
    }
}

fn apply_window(native: &mut impl Native, live: &mut Live, built: &mut Built, want: &WantWindow) -> Result<()> {
    for (index, placed) in want.layout.widgets.iter().enumerate() {
        let name = placed.name.clone().unwrap_or_else(|| gui::element(placed, index));
        let Some(at) = built.elements.iter().position(|b| b.name == name && b.row.is_none()) else { continue };
        if matches!(placed.kind, Kind::List { .. }) {
            let rows = want.wanted.rows.get(&name).cloned().unwrap_or_default();
            if built.rows.get(&name) != Some(&rows) {
                fill_rows(native, live, built, &name, want, &rows)?;
            }
        }
        let mut wanted = live::initial(placed, &want.words);
        if let Some(set) = want.wanted.set.get(&name) {
            wanted.extend(set.iter().map(|(key, value)| (*key, value.clone())));
        }
        native.apply(&mut built.elements[at].element, &wanted)?;
    }
    Ok(())
}

fn fill_rows(native: &mut impl Native, live: &mut Live, built: &mut Built, list: &str, want: &WantWindow, rows: &[Vec<(String, Property)>]) -> Result<()> {
    let Some(Placed { kind: Kind::List { row, .. }, .. }) = want.layout.widget(list) else { return Ok(()) };
    let Some(list_index) = built.elements.iter().position(|b| b.name == list && b.row.is_none()) else { return Ok(()) };
    let list_box = built.elements[list_index].element.object;
    // Invalidate the cache before any partial replacement. Retrying the
    // earlier rows must rebuild them after a failed update, too.
    built.rows.remove(list);
    let (mut old, kept) = std::mem::take(&mut built.elements).into_iter().partition(|b: &Bound| b.row.as_ref().is_some_and(|(l, _)| l == list));
    built.elements = kept;
    unbind(native, live, &mut old)?;
    native.clear_rows(list_box)?;
    let row_type = gui::row_type(&want.type_name, list);
    let result = (|| {
        for (n, cells) in rows.iter().enumerate() {
            let row_window = native.row(list_box, &row_type)?;
            let first = built.elements.len();
            bind(native, built, row_window, &row.widgets, Some((list.to_string(), n as u32 + 1)))?;
            for (i, placed) in row.widgets.iter().enumerate() {
                let mut wanted = live::initial(placed, &want.words);
                for (_, property) in cells.iter().filter(|(name, _)| Some(name) == placed.name.as_ref()) {
                    wanted.insert(property.name(), property.clone());
                }
                native.apply(&mut built.elements[first + i].element, &wanted)?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        let (mut partial, kept) = std::mem::take(&mut built.elements).into_iter().partition(|b: &Bound| b.row.as_ref().is_some_and(|(l, _)| l == list));
        built.elements = kept;
        if let Err(e) = unbind(native, live, &mut partial) {
            live.report(format!("failed list subscriptions: {e}"));
        }
        if let Err(e) = native.clear_rows(list_box) {
            live.report(format!("failed list cleanup: {e}"));
        }
        return Err(error);
    }
    built.rows.insert(list.to_string(), rows.to_vec());
    Ok(())
}

fn build_popup(native: &mut impl Native, live: &mut Live, number: u32, popup: &Popup, name: &str, (sw, sh): (i32, i32), shown: i32) -> Result<Built> {
    let root = native.create(name)?;
    let at = ((sw - gui::POPUP_WIDTH) / 2 + live::CASCADE * shown, (sh - gui::popup_height(popup)) / 2 + live::CASCADE * shown);
    let mut built = Built { key: Key::Popup(number), root, placed: at, moveable: true, elements: Vec::new(), rows: BTreeMap::new() };
    let result = (|| {
        native.position(root, at)?;
        native.popup_text(root, &popup.text)?;
        for (element, tag) in gui::SHIELDS.iter().zip(&popup.shields) {
            let Some(tag) = tag.clone().or_else(|| native.player()) else { continue };
            let shield = native.find(root, None, element)?;
            if let Err(e) = native.country(shield, &tag) {
                live.report(format!("pop-up {number}'s shield {tag}: {e}"));
                continue;
            }
            bind_one(native, &mut built, element.to_string(), shield, Look::Plain, true, None)?;
        }
        for (name, _) in gui::shown(popup) {
            let object = native.find(root, None, &name)?;
            bind_one(native, &mut built, name, object, Look::Plain, true, None)?;
        }
        native.show(root, true)
    })();
    match result {
        Ok(()) => Ok(built),
        Err(e) => Err(failed_build(native, live, built, e)),
    }
}

pub(super) fn killed(m: &Memory, live: &mut Live, root: usize) -> Result<()> {
    native_killed(&mut Game { m, manager: 0 }, live, root)
}
fn native_killed(native: &mut impl Native, live: &mut Live, root: usize) -> Result<()> {
    let mut result = Ok(());
    for closing in [false, true] {
        let built = if closing { &mut live.closing } else { &mut live.built };
        if let Some(at) = built.iter().position(|b| b.root == root) {
            let mut built = built.remove(at);
            result = result.and(unbind(native, live, &mut built.elements));
        }
    }
    result
}

pub(super) fn session_ended(m: &Memory, live: &mut Live) -> Result<()> {
    end_session(&mut Game { m, manager: 0 }, live)
}
fn end_session(native: &mut impl Native, live: &mut Live) -> Result<()> {
    let result = remove(native, live, |_| true);
    // A new session may use the same names and pop-up numbers. Old native
    // definitions and failed builds must not stand in for the new ones.
    live.read = None;
    live.popups_read.clear();
    live.failed.clear();
    result
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
