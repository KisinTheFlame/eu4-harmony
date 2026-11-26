//! Native window order within existing scopes, with original relative order
//! retained. Native calls run outside state borrows and candidate iteration.
mod course;
mod input;
mod native;
mod placement;
mod registry;

use crate::{native::hooks::Call, native::memory::Memory, native::points::Point, base::Result};
use input::Input;
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
}

fn checked(result: Result<()>) {
    if let Err(error) = result {
        panic!("native window ordering: {error}");
    }
}

enum Hold {
    Live,
    #[cfg(test)]
    Local(std::rc::Rc<RefCell<Registry>>),
}

pub(crate) struct Reconciliation(Hold);
impl Drop for Reconciliation {
    fn drop(&mut self) {
        match &self.0 {
            Hold::Live => with(Registry::release_hold),
            #[cfg(test)]
            Hold::Local(state) => state.borrow_mut().release_hold(),
        }
    }
}

trait OrderAccess {
    fn with_registry<T>(&self, f: impl FnOnce(&mut Registry) -> T) -> T;
    fn with_input<T>(&self, f: impl FnOnce(&mut Input) -> T) -> T;
    fn hold(&self) -> Hold;
}

trait SceneOps {
    fn owns_presentation(&self) -> bool;
    fn snapshot(&self, m: &Memory, parent: usize) -> Result<Vec<usize>>;
    fn apply(&mut self, m: &Memory, parent: usize, after: &[usize], movable: &BTreeSet<usize>) -> Result<()>;
    fn refresh(&mut self, m: &Memory, manager: usize, scene: usize) -> Result<()>;
}

struct LiveAccess;
impl OrderAccess for LiveAccess {
    fn with_registry<T>(&self, f: impl FnOnce(&mut Registry) -> T) -> T {
        with(f)
    }

    fn with_input<T>(&self, f: impl FnOnce(&mut Input) -> T) -> T {
        INPUT.with_borrow_mut(f)
    }

    fn hold(&self) -> Hold {
        Hold::Live
    }
}

struct LiveScene;
impl SceneOps for LiveScene {
    fn owns_presentation(&self) -> bool {
        crate::drawing::main_owns_presentation()
    }

    fn snapshot(&self, m: &Memory, parent: usize) -> Result<Vec<usize>> {
        placement::snapshot(m, parent)
    }

    fn apply(&mut self, m: &Memory, parent: usize, after: &[usize], movable: &BTreeSet<usize>) -> Result<()> {
        placement::apply(m, parent, after, movable)
    }

    fn refresh(&mut self, m: &Memory, manager: usize, scene: usize) -> Result<()> {
        native::refresh(m, manager, scene)
    }
}

struct Orderer<A>(A);

impl<A: OrderAccess> Orderer<A> {
    fn begin_owned(&self, wanted: &BTreeSet<Identity>) -> Reconciliation {
        self.0.with_registry(|state| state.prepare_owned(wanted));
        Reconciliation(self.0.hold())
    }

    fn bind_owned(&self, m: &Memory, seen: &[Owned]) -> Result<()> {
        self.0.with_registry(|state| state.owned(m, seen))
    }

    fn retire(&self, root: usize) {
        self.0.with_registry(|state| state.retire(root));
    }

    fn detach_host(&self, root: usize) {
        self.0.with_registry(|state| state.detach_host(root));
    }

    fn clear(&self) {
        self.0.with_registry(Registry::clear);
    }

    fn attach(&self, m: &Memory, host: crate::base::hosts::Host, root: usize, frame: Option<usize>, controls: &[(usize, crate::native::bound_element::Look)]) -> Result<()> {
        let (floor, nodes) = native::host_nodes(m, root, frame, controls)?;
        self.0.with_registry(|state| {
            state.host(m, host as usize, root)?;
            state.attached(m, root, floor, &nodes)
        })
    }

    fn flush(&self, m: &Memory, ops: &mut impl SceneOps) -> Result<()> {
        if !ops.owns_presentation() || !self.0.with_input(|input| input.idle()) || !self.0.with_registry(|state| state.needs_flush()) {
            return Ok(());
        }
        let Some((manager, scene)) = native::scene(m)? else {
            return Ok(());
        };
        let changes = self.0.with_registry(|state| state.changes(m))?;
        if changes.is_empty() {
            self.0.with_registry(Registry::mark_clean);
            return Ok(());
        }
        for change in changes {
            if !self.0.with_registry(|state| state.current(&change)) || ops.snapshot(m, change.parent)? != change.before {
                return Err("native order changed before placement".into());
            }
            ops.apply(m, change.parent, &change.after, &change.movable)?;
        }
        ops.refresh(m, manager, scene)?;
        self.0.with_registry(Registry::mark_clean);
        Ok(())
    }

    fn mouse_before(&self, m: &Memory, event: usize, ops: &mut impl SceneOps) -> Result<()> {
        self.flush(m, ops)?;
        self.0.with_input(|input| input.begin(event));
        Ok(())
    }

    fn passive_before(&self, m: &Memory, event: usize, ops: &mut impl SceneOps) -> Result<()> {
        self.flush(m, ops)?;
        self.0.with_input(|input| input.begin_passive(event));
        Ok(())
    }

    fn event_after(&self, m: &Memory, faulted: bool, ops: &mut impl SceneOps) -> Result<()> {
        if let Some(tokens) = self.0.with_input(|input| input.end(faulted)) {
            self.0.with_registry(|state| {
                for token in tokens {
                    state.activate(token);
                }
            });
            if !faulted {
                self.flush(m, ops)?;
            }
        }
        Ok(())
    }

    fn handler_before(&self, m: &Memory, gui: usize, event: usize) -> Result<()> {
        let eligible = native::press_unhandled(m, event)? && self.0.with_input(|input| input.accepts(event));
        let target = if eligible { self.0.with_registry(|state| state.target(m, gui))? } else { None };
        self.0.with_input(|input| input.push_capture(event, eligible, target));
        Ok(())
    }

    fn handler_after(&self, m: &Memory, event: usize, faulted: bool) {
        let handled = !faulted && self.0.with_input(|input| input.top_captured()) && native::event_handled(m, event);
        self.0.with_input(|input| input.pop_capture(handled));
    }

    fn before_visibility(&self, m: &Memory, root: usize) -> Result<()> {
        self.0.with_registry(|state| state.before_visibility(m, root))
    }

    fn visibility_after(&self, m: &Memory) -> Result<()> {
        self.0.with_registry(|state| state.visibility(m))
    }

    fn batch_after(&self, m: &Memory, faulted: bool, ops: &mut impl SceneOps) -> Result<()> {
        if !faulted {
            self.flush(m, ops)?;
        }
        Ok(())
    }
}

const LIVE: Orderer<LiveAccess> = Orderer(LiveAccess);

/// Retains logical placement through technical native teardown/reconstruction.
pub(crate) fn begin_owned(wanted: &BTreeSet<Identity>) -> Reconciliation {
    LIVE.begin_owned(wanted)
}

pub(crate) fn bind_owned(m: &Memory, seen: &[Owned]) -> Result<()> {
    LIVE.bind_owned(m, seen)
}

pub(crate) fn retire(root: usize) {
    LIVE.retire(root);
}
pub(crate) fn detach_host(root: usize) {
    LIVE.detach_host(root);
}
pub(crate) fn clear() {
    LIVE.clear();
}

/// Complete attached node groups, from the existing host binding.
pub(crate) fn attach(m: &Memory, host: crate::base::hosts::Host, root: usize, frame: Option<usize>, controls: &[(usize, crate::native::bound_element::Look)]) -> Result<()> {
    LIVE.attach(m, host, root, frame, controls)
}

/// Called at a frame seam and before/after individual native input walks.
/// Never call from inside a show visitor, a GUI handler, or scene traversal.
pub(crate) fn flush(m: &Memory) -> Result<()> {
    LIVE.flush(m, &mut LiveScene)
}

/// Rebuilds the first scene's prepared `DRAW_LIST`, `HIT_LIST`, and `OWNER_LIST`
/// from live tree nodes under presentation ownership without resetting
/// hover/input ranks.
pub(crate) fn prepare_scene(m: &Memory) -> Result<()> {
    native::prepare_lists(m)
}

fn mouse_before(call: &mut Call) {
    checked(LIVE.mouse_before(&call.memory(), call.arg(1), &mut LiveScene));
}
fn passive_before(call: &mut Call) {
    checked(LIVE.passive_before(&call.memory(), call.arg(1), &mut LiveScene));
}
fn event_after(call: &mut Call) {
    checked(LIVE.event_after(&call.memory(), call.faulted(), &mut LiveScene));
}
fn handler_before(call: &mut Call) {
    if let Err(error) = LIVE.handler_before(&call.memory(), call.arg(0), call.arg(1)) {
        panic!("native window activation: {error}");
    }
}
fn handler_after(call: &mut Call) {
    LIVE.handler_after(&call.memory(), call.arg(1), call.faulted());
}

fn gui_visibility_before(call: &mut Call) {
    if crate::base::thread_id() == crate::base::main_thread() {
        checked(LIVE.before_visibility(&call.memory(), call.arg(0) - crate::native::objects::gui::window::GUI));
    }
}
fn root_visibility_before(call: &mut Call) {
    if crate::base::thread_id() == crate::base::main_thread() {
        checked(LIVE.before_visibility(&call.memory(), call.arg(0)));
    }
}
fn visibility_after(call: &mut Call) {
    if !call.faulted() && crate::base::thread_id() == crate::base::main_thread() {
        checked(LIVE.visibility_after(&call.memory()));
    }
}
fn destroyed(call: &mut Call) {
    retire(call.arg(0));
}
fn batch_after(call: &mut Call) {
    checked(LIVE.batch_after(&call.memory(), call.faulted(), &mut LiveScene));
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
            crate::native::objects::gui::functions::WINDOW_SHOW,
            "40 53 48 83 ec 30 80 b9 c5 00 00 00 00 48 8b d9",
            "window GUI shown",
            1,
            Some(gui_visibility_before),
            Some(visibility_after),
        ),
        Point::observe(
            crate::native::objects::gui::functions::WINDOW_HIDE,
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
    ]);
    points
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
