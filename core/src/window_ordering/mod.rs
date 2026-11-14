//! Native window order within existing scopes, with original relative order
//! retained. Native calls run outside state borrows and candidate iteration.
mod course;
mod input;
mod native;
mod placement;
mod registry;

use crate::{hooks::Call, memory::Memory, points::Point, Result};
use input::{Capture, Input};
use registry::Registry;
pub(crate) use registry::{Identity, Owned};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::{Mutex, OnceLock},
};

fn with<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    static STATE: OnceLock<Mutex<Registry>> = OnceLock::new();
    f(&mut STATE.get_or_init(|| Mutex::new(Registry::default())).lock().unwrap_or_else(|e| e.into_inner()))
}

thread_local! {
    static INPUT: RefCell<Input> = RefCell::new(Input::default());
    static CAPTURES: RefCell<Vec<Option<Capture>>> = const { RefCell::new(Vec::new()) };
}

fn checked(result: Result<()>) {
    if let Err(error) = result {
        panic!("native window ordering: {error}");
    }
}

pub(crate) struct Reconciliation;
impl Drop for Reconciliation {
    fn drop(&mut self) {
        with(|state| state.held -= 1);
    }
}

/// Retains logical placement through technical native teardown/reconstruction.
pub(crate) fn begin_owned(wanted: &BTreeSet<Identity>) -> Reconciliation {
    with(|state| state.prepare_owned(wanted));
    Reconciliation
}

pub(crate) fn bind_owned(m: &Memory, seen: &[Owned]) -> Result<()> {
    with(|state| state.owned(m, seen))
}

pub(crate) fn retire(root: usize) {
    with(|state| state.retire(root));
}
pub(crate) fn detach_host(root: usize) {
    with(|state| state.detach_host(root));
}
pub(crate) fn clear() {
    with(Registry::clear);
}

/// Complete attached node groups, from the existing host binding.
pub(crate) fn attach(m: &Memory, host: crate::hosts::Host, root: usize, frame: Option<usize>, controls: &[(usize, crate::bound_element::Look)]) -> Result<()> {
    use crate::{
        bound_element::Look,
        objects::gui::{classes, window},
    };
    let mut nodes = Vec::new();
    if let Some(frame) = frame {
        nodes.push(native::window_node(m, frame)?);
    }
    for &(object, look) in controls {
        let class = m.ptr(object)?;
        match look {
            Look::Button if class == m.at(classes::BUTTON) => {
                nodes.push(m.ptr(object + 0x290)?);
                let extra = m.ptr(object + 0x188)?;
                if extra != 0 {
                    nodes.push(extra);
                }
            }
            Look::Text if class == m.at(classes::TEXT_BOX) => {
                nodes.push(m.ptr(object + 0x118)?);
                let scrollbar = m.ptr(object + 0x1a0)?;
                if scrollbar != 0 {
                    let container = scrollbar + 0x528;
                    if m.ptr(scrollbar)? != m.at(0x1d93748usize)
                        || m.vslot(scrollbar, 0xe8)? != m.at(0x1554660usize)
                        || m.ptr(container)? != m.at(0x1d60238usize)
                    {
                        return Err("unrecognized attached text scrollbar container".into());
                    }
                    nodes.push(container);
                }
            }
            Look::Icon if class == m.at(0x1d96d00usize) => nodes.push(m.ptr(object + 0x168)?),
            _ => return Err("unrecognized attached visual node group".into()),
        }
    }
    let background = m.ptr(root + window::BACKGROUND)?;
    let floor = if background == 0 {
        None
    } else {
        if m.ptr(background)? != m.at(classes::BACKGROUND) {
            return Err("unrecognized host background".into());
        }
        Some(m.ptr(background + 0x168)?)
    };
    with(|state| {
        state.host(m, host as usize, root)?;
        state.attached(m, root, floor, &nodes)
    })
}

/// Called at a frame seam and before/after individual native input walks.
/// Never call from inside a show visitor, a GUI handler, or scene traversal.
pub(crate) fn flush(m: &Memory) -> Result<()> {
    if !crate::drawing::main_owns_presentation() || !INPUT.with_borrow(Input::idle) {
        return Ok(());
    }
    let dirty = with(|state| state.dirty && state.held == 0);
    if !dirty {
        return Ok(());
    }
    let Some((manager, scene)) = native::scene(m)? else {
        return Ok(());
    };
    let changes = with(|state| state.changes(m))?;
    for change in changes {
        if !with(|state| state.current(&change)) || placement::snapshot(m, change.parent)? != change.before {
            return Err("native order changed before placement".into());
        }
        placement::apply(m, change.parent, &change.after, &change.movable)?;
    }
    native::refresh(m, manager, scene)?;
    with(|state| state.dirty = false);
    Ok(())
}

fn mouse_before(call: &mut Call) {
    checked(flush(&call.memory()));
    INPUT.with_borrow_mut(|input| input.begin(call.arg(1)));
}
fn passive_before(call: &mut Call) {
    checked(flush(&call.memory()));
    INPUT.with_borrow_mut(|input| input.begin_passive(call.arg(1)));
}
fn event_after(call: &mut Call) {
    if let Some(tokens) = INPUT.with_borrow_mut(|input| input.end(call.faulted())) {
        with(|state| {
            for token in tokens {
                state.activate(token);
            }
        });
        if !call.faulted() {
            checked(flush(&call.memory()));
        }
    }
}
fn handler_before(call: &mut Call) {
    let m = call.memory();
    let event = call.arg(1);
    let capture = (|| -> Result<Option<Capture>> {
        use crate::objects::gui::mouse_input as fields;
        if m.u32(event + fields::DEVICE)? != fields::MOUSE
            || ![fields::PRESSED, fields::DOUBLE].contains(&m.u32(event + fields::KIND)?)
            || ![1, fields::RIGHT].contains(&m.u32(event + fields::BUTTON)?)
            || m.u16(event + 0x54)? != 0
        {
            return Ok(None);
        }
        if INPUT.with_borrow(|input| input.capture(event, None).is_none()) {
            return Ok(None);
        }
        let target = with(|state| state.target(&m, call.arg(0)))?;
        Ok(INPUT.with_borrow(|input| input.capture(event, target)))
    })()
    .unwrap_or_else(|error| panic!("native window activation: {error}"));
    CAPTURES.with_borrow_mut(|captures| captures.push(capture));
}
fn handler_after(call: &mut Call) {
    let capture = CAPTURES.with_borrow_mut(|captures| captures.pop().expect("native GUI dispatch nesting"));
    let handled = !call.faulted() && capture.is_some() && call.memory().u16(call.arg(1) + 0x54).is_ok_and(|mask| mask != 0);
    INPUT.with_borrow_mut(|input| input.returned(capture, handled));
}

fn gui_visibility_before(call: &mut Call) {
    if crate::thread_id() == crate::main_thread() {
        checked(with(|state| state.before_visibility(&call.memory(), call.arg(0) - crate::objects::gui::window::GUI)));
    }
}
fn root_visibility_before(call: &mut Call) {
    if crate::thread_id() == crate::main_thread() {
        checked(with(|state| state.before_visibility(&call.memory(), call.arg(0))));
    }
}
fn visibility_after(call: &mut Call) {
    if !call.faulted() && crate::thread_id() == crate::main_thread() {
        checked(with(|state| state.visibility(&call.memory())));
    }
}
fn destroyed(call: &mut Call) {
    retire(call.arg(0));
}
fn batch_after(call: &mut Call) {
    if !call.faulted() {
        checked(flush(&call.memory()));
    }
}

pub(crate) fn points() -> Vec<Point> {
    let mut points = native::points();
    points.extend(placement::points());
    points.extend([
        Point::observe(
            0x14ea3d0usize,
            "48 89 5c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 20",
            "native window mouse activation",
            3,
            Some(mouse_before),
            Some(event_after),
        ),
        Point::observe(
            0x14ea150usize,
            "48 89 5c 24 08 48 89 6c 24 10 56 57 41 54 41 56 41 57",
            "native window keyboard order",
            2,
            Some(passive_before),
            Some(event_after),
        ),
        Point::observe(
            0x14ea630usize,
            "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57",
            "native window secondary input walk",
            2,
            Some(passive_before),
            Some(event_after),
        ),
        Point::observe(
            0x1552800usize,
            "48 89 74 24 18 48 89 7c 24 20 41 56 48 83 ec 20 83 7a 50 03",
            "native GUI accepted input",
            3,
            Some(handler_before),
            Some(handler_after),
        ),
        Point::observe(0x14e9a70usize, "4c 8b dc 53 41 56 48 81 ec b8 00 00 00", "native window order after input batch", 4, None, Some(batch_after)),
        Point::observe(
            crate::objects::gui::functions::WINDOW_SHOW,
            "40 53 48 83 ec 30 80 b9 c5 00 00 00 00 48 8b d9",
            "window GUI shown",
            1,
            Some(gui_visibility_before),
            Some(visibility_after),
        ),
        Point::observe(
            crate::objects::gui::functions::WINDOW_HIDE,
            "48 89 5c 24 10 57 48 83 ec 30 48 8b f9 48 8b 89 00 0a 00 00",
            "window GUI hidden",
            1,
            Some(gui_visibility_before),
            Some(visibility_after),
        ),
        Point::observe(
            0x1547cc0usize,
            "40 53 48 83 ec 30 80 b9 db 00 00 00 00 48 8b d9",
            "window root shown",
            1,
            Some(root_visibility_before),
            Some(visibility_after),
        ),
        Point::observe(
            0x1547a50usize,
            "40 53 48 83 ec 20 48 8b d9 c6 81 f5 00 00 00 00",
            "window root hidden",
            1,
            Some(root_visibility_before),
            Some(visibility_after),
        ),
        Point::observe(0x1546730usize, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", "window base destruction retires order", 1, Some(destroyed), None),
        Point::keep(0x15ae9d4usize, "48 8b 89 90 02 00 00", "attached button node").uses(&[0x290]),
        Point::keep(0x15ae9fausize, "48 8b 8b 88 01 00 00", "attached button secondary node").uses(&[0x188]),
        Point::keep(0x1571500usize, "48 8b 89 68 01 00 00", "host background node").uses(&[0x168]),
        Point::keep(0x15861a0usize, "48 8b 89 68 01 00 00", "attached icon node").uses(&[0x168]),
        Point::keep(0x15857fcusize, "48 8d 05 fd 14 81 00", "attached icon class").uses(&[0x1d96d00]),
        Point::keep(0x1552872usize, "66 83 7e 54 00", "native input handled mask").uses(&[0x54]),
        Point::keep(0x1552bfcusize, "48 8d 05 45 0b 84 00 48 89 01", "attached scrollbar class").uses(&[0x1d93748]),
        Point::keep(0x15531f1usize, "49 8d b6 28 05 00 00", "attached scrollbar embedded container").uses(&[0x528]),
        Point::keep(0x155321fusize, "48 8d 05 12 d0 80 00 48 89 06", "attached scrollbar container class").uses(&[0x1d60238]),
        Point::method(0x1d93748, 0xe8, 0x1554660, "attached scrollbar parent method"),
        Point::keep(0x152b0b0usize, "48 8b 89 18 01 00 00", "attached text node").uses(&[0x118]),
        Point::keep(0x152b0d3usize, "48 8b 8b a0 01 00 00 48 85 c9 74 23 48 81 c1 28 05 00 00", "attached text scrollbar group").uses(&[0x1a0]),
    ]);
    points
}
