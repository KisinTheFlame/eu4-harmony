//! Native window order within existing scopes, with original relative order
//! retained. Native calls run outside state borrows and candidate iteration.
mod course;
mod input;
mod native;
mod placement;
mod registry;

use crate::{base::Result, native::hooks::Call, native::memory::Memory, native::points::Point};
use input::Input;
use registry::Registry;
pub(crate) use registry::{Identity, Owned};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, OnceLock,
    },
};

static ORDERING_BROKEN_LOGGED: AtomicBool = AtomicBool::new(false);
static ACTIVATION_BROKEN_LOGGED: AtomicBool = AtomicBool::new(false);

fn checked(result: Result<()>) {
    if let Err(error) = result {
        crate::base::broken_invariant(&ORDERING_BROKEN_LOGGED, format_args!("native window ordering: {error}"));
    }
}

pub(crate) struct Reconciliation(Arc<Mutex<Registry>>);
impl Drop for Reconciliation {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).release_hold();
    }
}

trait SceneOps {
    fn owns_presentation(&self) -> bool;
    fn snapshot(&self, m: &Memory, parent: usize) -> Result<Vec<usize>>;
    fn apply(&mut self, m: &Memory, parent: usize, after: &[usize], movable: &BTreeSet<usize>) -> Result<()>;
    fn refresh(&mut self, m: &Memory, manager: usize, scene: usize) -> Result<()>;
}

struct LiveScene;
impl SceneOps for LiveScene {
    fn owns_presentation(&self) -> bool {
        crate::render::drawing::main_owns_presentation()
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

#[derive(Default)]
struct Orderer {
    registry: Arc<Mutex<Registry>>,
    input: Mutex<Input>,
    map_markers: AtomicU32,
}

impl Orderer {
    fn with_registry<T>(&self, f: impl FnOnce(&mut Registry) -> T) -> T {
        f(&mut self.registry.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn with_input<T>(&self, f: impl FnOnce(&mut Input) -> T) -> T {
        f(&mut self.input.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn begin_owned(&self, wanted: &BTreeSet<Identity>) -> Reconciliation {
        self.with_registry(|state| state.prepare_owned(wanted));
        Reconciliation(Arc::clone(&self.registry))
    }

    fn bind_owned(&self, m: &Memory, seen: &[Owned]) -> Result<()> {
        self.with_registry(|state| state.owned(m, seen))
    }

    fn retire(&self, root: usize) {
        self.with_registry(|state| state.retire(root));
    }

    fn detach_host(&self, root: usize) {
        self.with_registry(|state| state.detach_host(root));
    }

    fn clear(&self) {
        self.with_registry(Registry::clear);
    }

    fn attach(
        &self,
        m: &Memory,
        host: crate::base::hosts::Host,
        root: usize,
        frame: Option<usize>,
        controls: &[(usize, crate::interface::bound_element::Look)],
    ) -> Result<()> {
        let (floor, nodes) = native::host_nodes(m, root, frame, controls)?;
        self.with_registry(|state| {
            state.host(m, host as usize, root)?;
            state.attached(m, root, floor, &nodes)
        })
    }

    fn flush(&self, m: &Memory, ops: &mut impl SceneOps) -> Result<()> {
        if !ops.owns_presentation() || !self.with_input(|input| input.idle()) || !self.with_registry(|state| state.needs_flush()) {
            return Ok(());
        }
        let Some((manager, scene)) = native::scene(m)? else {
            return Ok(());
        };
        let changes = self.with_registry(|state| state.changes(m))?;
        if changes.is_empty() {
            self.with_registry(Registry::mark_clean);
            return Ok(());
        }
        for change in changes {
            if !self.with_registry(|state| state.current(&change)) || ops.snapshot(m, change.parent)? != change.before {
                return Err("native order changed before placement".into());
            }
            ops.apply(m, change.parent, &change.after, &change.movable)?;
        }
        ops.refresh(m, manager, scene)?;
        self.with_registry(Registry::mark_clean);
        Ok(())
    }

    fn mouse_before(&self, m: &Memory, event: usize, ops: &mut impl SceneOps) -> Result<()> {
        let flushed = self.flush(m, ops);
        self.with_input(|input| input.begin(event));
        flushed
    }

    fn passive_before(&self, m: &Memory, event: usize, ops: &mut impl SceneOps) -> Result<()> {
        let flushed = self.flush(m, ops);
        self.with_input(|input| input.begin_passive(event));
        flushed
    }

    fn event_after(&self, m: &Memory, faulted: bool, ops: &mut impl SceneOps) -> Result<()> {
        if let Some(tokens) = self.with_input(|input| input.end(faulted)) {
            self.with_registry(|state| {
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
        let resolved = (|| -> Result<(bool, Option<course::Token>)> {
            let eligible = native::press_unhandled(m, event)? && self.with_input(|input| input.accepts(event));
            let target = if eligible { self.with_registry(|state| state.target(m, gui))? } else { None };
            Ok((eligible, target))
        })();
        match resolved {
            Ok((eligible, target)) => {
                self.with_input(|input| input.push_capture(event, eligible, target));
                Ok(())
            }
            Err(error) => {
                // Keep the capture stack paired with `handler_after` even on error.
                self.with_input(|input| input.push_capture(event, false, None));
                Err(error)
            }
        }
    }

    fn handler_after(&self, m: &Memory, event: usize, faulted: bool) {
        let handled = !faulted && self.with_input(|input| input.top_captured()) && native::event_handled(m, event);
        self.with_input(|input| input.pop_capture(handled));
    }

    fn in_map_markers(&self) -> bool {
        self.map_markers.load(Ordering::Acquire) != 0
    }

    fn map_markers_before(&self) {
        self.map_markers.fetch_add(1, Ordering::AcqRel);
    }

    fn map_markers_after(&self) {
        self.map_markers.fetch_update(Ordering::AcqRel, Ordering::Acquire, |d| Some(d.saturating_sub(1))).ok();
    }

    fn prepend_backdrop(&self, m: &Memory, root: usize) -> Result<()> {
        if self.in_map_markers() {
            self.with_registry(|state| state.mark_backdrop(m, root))?;
        }
        Ok(())
    }

    fn before_visibility(&self, m: &Memory, root: usize) -> Result<()> {
        if self.in_map_markers() {
            return self.with_registry(|state| state.mark_backdrop(m, root));
        }
        self.with_registry(|state| state.before_visibility(m, root))
    }

    fn visibility_after(&self, m: &Memory) -> Result<()> {
        if self.in_map_markers() {
            return Ok(());
        }
        self.with_registry(|state| state.visibility(m))
    }

    fn batch_after(&self, m: &Memory, faulted: bool, ops: &mut impl SceneOps) -> Result<()> {
        if !faulted {
            self.flush(m, ops)?;
        }
        Ok(())
    }
}

fn live() -> &'static Orderer {
    static LIVE: OnceLock<Orderer> = OnceLock::new();
    LIVE.get_or_init(Orderer::default)
}

/// Retains logical placement through technical native teardown/reconstruction.
pub(crate) fn begin_owned(wanted: &BTreeSet<Identity>) -> Reconciliation {
    live().begin_owned(wanted)
}

pub(crate) fn bind_owned(m: &Memory, seen: &[Owned]) -> Result<()> {
    live().bind_owned(m, seen)
}

pub(crate) fn retire(root: usize) {
    live().retire(root);
}
pub(crate) fn detach_host(root: usize) {
    live().detach_host(root);
}
pub(crate) fn clear() {
    live().clear();
}

/// Complete attached node groups, from the existing host binding.
pub(in crate::interface) fn attach(
    m: &Memory,
    host: crate::base::hosts::Host,
    root: usize,
    frame: Option<usize>,
    controls: &[(usize, crate::interface::bound_element::Look)],
) -> Result<()> {
    live().attach(m, host, root, frame, controls)
}

/// Called at a frame seam and before/after individual native input walks.
/// Never call from inside a show visitor, a GUI handler, or scene traversal.
pub(crate) fn flush(m: &Memory) -> Result<()> {
    live().flush(m, &mut LiveScene)
}

/// Rebuilds the first scene's prepared `DRAW_LIST`, `HIT_LIST`, and `OWNER_LIST`
/// from live tree nodes under presentation ownership without resetting
/// hover/input ranks.
pub(crate) fn prepare_scene(m: &Memory) -> Result<()> {
    native::prepare_lists(m)
}

fn mouse_before(call: &mut Call) {
    checked(live().mouse_before(&call.memory(), call.arg(1), &mut LiveScene));
}
fn passive_before(call: &mut Call) {
    checked(live().passive_before(&call.memory(), call.arg(1), &mut LiveScene));
}
fn event_after(call: &mut Call) {
    checked(live().event_after(&call.memory(), call.faulted(), &mut LiveScene));
}
fn handler_before(call: &mut Call) {
    if let Err(error) = live().handler_before(&call.memory(), call.arg(0), call.arg(1)) {
        crate::base::broken_invariant(&ACTIVATION_BROKEN_LOGGED, format_args!("native window activation: {error}"));
    }
}
fn handler_after(call: &mut Call) {
    live().handler_after(&call.memory(), call.arg(1), call.faulted());
}

fn gui_visibility_before(call: &mut Call) {
    if crate::base::thread_id() == crate::base::main_thread() {
        if let Some(root) = call.arg(0).checked_sub(crate::native::objects::gui::window::GUI) {
            checked(live().before_visibility(&call.memory(), root));
        }
    }
}
fn root_visibility_before(call: &mut Call) {
    if crate::base::thread_id() == crate::base::main_thread() {
        checked(live().before_visibility(&call.memory(), call.arg(0)));
    }
}
fn visibility_after(call: &mut Call) {
    if !call.faulted() && crate::base::thread_id() == crate::base::main_thread() {
        checked(live().visibility_after(&call.memory()));
    }
}
fn destroyed(call: &mut Call) {
    retire(call.arg(0));
}
fn map_markers_before(_: &mut Call) {
    if crate::base::thread_id() == crate::base::main_thread() {
        live().map_markers_before();
    }
}
fn map_markers_after(_: &mut Call) {
    if crate::base::thread_id() == crate::base::main_thread() {
        live().map_markers_after();
    }
}
fn prepend_to_first_root_before(call: &mut Call) {
    if crate::base::thread_id() == crate::base::main_thread() {
        if let Some(root) = call.arg(0).checked_sub(crate::native::objects::gui::window::GUI) {
            checked(live().prepend_backdrop(&call.memory(), root));
        }
    }
}
fn batch_after(call: &mut Call) {
    checked(live().batch_after(&call.memory(), call.faulted(), &mut LiveScene));
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
        Point::observe(
            0x10a6d00usize,
            "48 89 5c 24 10 48 89 6c 24 18 56 48 83 ec 30 33",
            "map markers update suppresses map icon activation",
            4,
            Some(map_markers_before),
            Some(map_markers_after),
        ),
        Point::observe(
            0x15466d0usize,
            "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8b 89 00 0a 00 00",
            "map icon prepended to first GUI root",
            1,
            Some(prepend_to_first_root_before),
            None,
        ),
    ]);
    points
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
