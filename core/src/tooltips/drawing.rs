//! A self-drawn control's tooltip remains the game's single ToolTip texture
//! node. Filter only that ready node from the current native draw list, then
//! draw it once after the framework UI, before the presentation lock opens.
//! See research/panel-entry-text.md and research/gui-drawing.md.
use crate::{
    hooks::Call,
    memory::Memory,
    objects::{gui::element, text::text_texture},
    points::{GameFunction, Point},
    Result,
};
use std::{
    cell::{Cell, RefCell},
    sync::Mutex,
};

const DRAW: GameFunction<2> = GameFunction::at(0x14dd350);
const DRAW_FIRST_RETURN: usize = 0x14dd2bd;
const DRAW_SECOND_RETURN: usize = 0x14dd742;
const SETUP: GameFunction<1> = GameFunction::at(0x14dcf90);
const BEGIN: GameFunction<1> = GameFunction::at(crate::drawing::SCENE_BEGINS);
const END: GameFunction<1> = GameFunction::at(0x16d4e50);
const INVALIDATE: usize = 0x1414e50;
const DELETE_OWNER: usize = 0x1414b60;
const DELETE_HOLDER: usize = 0x15a1240;
const OWNER_HOLDER: usize = 8;
const HOLDER_NODE: usize = 0xd0;
const NODE_DIRTY: usize = 0x2db;
const NODE_CLASS: usize = 0x1d933b0;
const DEAD: usize = 0xc4;
const GRAPHICS_RENDERER: usize = 0x88;

#[derive(Clone)]
struct Display {
    owner: usize,
    holder: usize,
    node: usize,
    text: Vec<u8>,
    generation: u64,
}
#[derive(Default)]
struct Shared {
    generation: u64,
    display: Option<Display>,
}
impl Shared {
    fn showing(&mut self, owner: usize, holder: usize, node: usize, text: &[u8]) {
        let unchanged = self.display.as_ref().is_some_and(|old| old.owner == owner && old.holder == holder && old.node == node && old.text == text);
        if !unchanged {
            self.generation = self.generation.wrapping_add(1);
        }
        self.display = Some(Display { owner, holder, node, text: text.to_vec(), generation: self.generation });
    }
}
static SHARED: Mutex<Shared> = Mutex::new(Shared { generation: 0, display: None });

pub(super) fn clear() {
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    shared.display = None;
    shared.generation = shared.generation.wrapping_add(1);
}

pub(super) fn showing(m: &Memory, owner: usize, text: &[u8]) -> Result<()> {
    let holder = m.ptr(owner + OWNER_HOLDER)?;
    if holder == 0 || text.is_empty() {
        clear();
        return Ok(());
    }
    let node = m.ptr(holder + HOLDER_NODE)?;
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    shared.showing(owner, holder, node, text);
    Ok(())
}

fn display() -> Option<Display> {
    SHARED.lock().unwrap_or_else(|e| e.into_inner()).display.clone()
}

fn current(display: &Display) -> bool {
    SHARED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .display
        .as_ref()
        .is_some_and(|now| now.generation == display.generation && now.owner == display.owner && now.holder == display.holder && now.node == display.node)
}

fn ready(m: &Memory, display: &Display) -> Result<bool> {
    Ok(display.node != 0
        && m.ptr(display.owner + OWNER_HOLDER)? == display.holder
        && m.ptr(display.holder + HOLDER_NODE)? == display.node
        && m.u8(display.holder + element::VISIBLE)? != 0
        && m.u8(display.holder + DEAD)? == 0
        && m.ptr(display.node)? == m.at(NODE_CLASS)
        && m.u8(display.node + NODE_DIRTY)? == 0
        && m.ptr(display.node + text_texture::TEXTURE)? != 0)
}

struct Filter {
    // The private vector's allocation and header stay alive until the native
    // helper returns; the actual game's vector is never changed.
    _nodes: Vec<usize>,
    header: [usize; 3],
}
impl Filter {
    fn new(nodes: Vec<usize>) -> Self {
        let begin = nodes.as_ptr() as usize;
        let end = begin + nodes.len() * size_of::<usize>();
        Self { _nodes: nodes, header: [begin, end, end] }
    }
}

fn without_node(m: &Memory, header: usize, node: usize) -> Result<Option<Filter>> {
    let nodes = crate::containers::pointers(m, header, 16384)?;
    if !nodes.contains(&node) {
        return Ok(None);
    }
    Ok(Some(Filter::new(nodes.into_iter().filter(|item| *item != node).collect())))
}

#[derive(Clone)]
struct Late {
    display: Display,
    graphics: usize,
}
thread_local! {
    static FILTERS: RefCell<Vec<Option<Box<Filter>>>> = const { RefCell::new(Vec::new()) };
    static LATE: RefCell<Option<Late>> = const { RefCell::new(None) };
    static IN_LATE: Cell<bool> = const { Cell::new(false) };
}

fn frame_before(_: &mut Call) {
    discard_late();
}

fn filter_before(call: &mut Call) {
    if IN_LATE.get() {
        return;
    }
    FILTERS.with_borrow_mut(|filters| filters.push(None));
    if crate::thread_id() != crate::main_thread() && !crate::drawing::is_drawing_thread() {
        return;
    }
    let Some(display) = display() else { return };
    if crate::framework_ui::tooltip_at().as_deref() != Some(display.text.as_slice()) {
        return;
    }
    let m = call.memory();
    if ![m.at(DRAW_FIRST_RETURN), m.at(DRAW_SECOND_RETURN)].contains(&call.caller()) {
        return;
    }
    if ready(&m, &display) != Ok(true) {
        return;
    }
    let Ok(Some(filtered)) = without_node(&m, call.arg(1), display.node) else { return };
    let filtered = Box::new(filtered);
    call.set_arg(1, filtered.header.as_ptr() as usize);
    FILTERS.with_borrow_mut(|filters| *filters.last_mut().expect("this draw has a frame") = Some(filtered));
    LATE.with_borrow_mut(|late| *late = Some(Late { display, graphics: call.arg(0) }));
}

fn filter_after(_: &mut Call) {
    if !IN_LATE.get() {
        FILTERS.with_borrow_mut(|filters| {
            filters.pop();
        });
    }
}

pub(crate) fn discard_late() {
    LATE.with_borrow_mut(|late| *late = None);
}

/// Called after the UI mutex and its D3D state block have been released, and
/// before screenshot/Present release the presentation lock. No node survives
/// this frame's call. Unready nodes always retain their original draw.
pub(crate) fn draw_late(renderer: usize) {
    let Some(late) = LATE.with_borrow_mut(Option::take) else { return };
    if !current(&late.display) || crate::framework_ui::tooltip_at().as_deref() != Some(late.display.text.as_slice()) {
        return;
    }
    let Some(m) = crate::memory() else { return };
    let result = (|| -> Result<()> {
        if !ready(m, &late.display)? || m.ptr(late.graphics + GRAPHICS_RENDERER)? != renderer {
            return Ok(());
        }
        let filter = Filter::new(vec![late.display.node]);
        // Use native scene entry: it resets the native renderer's declaration
        // cache as well as beginning the actual D3D scene. Test AL, not RAX.
        if BEGIN.call(m, [renderer])? & 0xff == 0 {
            return Err("native tooltip scene did not begin".into());
        }
        IN_LATE.set(true);
        let drawn = SETUP.call(m, [late.graphics]).and_then(|_| DRAW.call(m, [late.graphics, filter.header.as_ptr() as usize]));
        let ended = END.call(m, [renderer]);
        IN_LATE.set(false);
        drawn?;
        if ended? & 0xff == 0 {
            return Err("native tooltip scene did not end".into());
        }
        Ok(())
    })();
    if let Err(e) = result {
        log!("late native tooltip: {e}");
    }
}

fn owner_invalidated(call: &mut Call) {
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if shared.display.as_ref().is_some_and(|display| display.owner == call.arg(0)) {
        shared.display = None;
        shared.generation = shared.generation.wrapping_add(1);
    }
}
fn holder_deleted(call: &mut Call) {
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if shared.display.as_ref().is_some_and(|display| display.holder == call.arg(0)) {
        shared.display = None;
        shared.generation = shared.generation.wrapping_add(1);
    }
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(0x14dd010usize, "48 8b c4 55 53 41 56 48 8d 68 a1 48 81 ec d0 00 00 00", "native tooltip frame starts", 1, Some(frame_before), None),
        Point::keep(0x14dd2b8usize, "e8 93 00 00 00 e8 4e ab", "native tooltip first GUI draw scope"),
        Point::keep(0x14dd73dusize, "e8 0e fc ff ff 48 8b 05", "native tooltip second GUI draw scope"),
        Point::observe(
            DRAW.rva(),
            "48 89 54 24 10 53 41 55 41 56 41 57 48 83 ec 58",
            "native tooltip node draw list",
            2,
            Some(filter_before),
            Some(filter_after),
        ),
        Point::keep(SETUP, "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8d 91 bc 25 03 00", "native tooltip GUI state"),
        Point::keep(BEGIN, "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8b 49 08", "native tooltip scene begin"),
        Point::keep(END, "48 83 ec 28 48 8b 49 08 48 8b 01 ff 90 50 01 00 00", "native tooltip scene end"),
        Point::observe(INVALIDATE, "48 89 5c 24 08 57 48 83 ec 20 48 8b 79 08", "native tooltip holder invalidation", 1, Some(owner_invalidated), None),
        Point::observe(DELETE_OWNER, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20", "native tooltip owner deletion", 2, Some(owner_invalidated), None),
        Point::observe(DELETE_HOLDER, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9", "native tooltip holder deletion", 2, Some(holder_deleted), None),
        Point::method(NODE_CLASS, 0x58, 0x1550950, "native tooltip text node draw"),
        Point::keep(0x155072cusize, "48 8d 05 7d 2c 84 00 48 89 03", "native tooltip text node class").uses(&[NODE_CLASS]),
        Point::keep(0x1414f03usize, "48 8b 49 08", "native tooltip current holder").uses(&[OWNER_HOLDER]),
        Point::keep(0x15a10afusize, "48 89 86 d0 00 00 00", "native tooltip holder node").uses(&[HOLDER_NODE]),
        Point::keep(0x14dec9busize, "48 85 c0 74 16 80 b8 c5 00 00 00 00 74 0d 80 b8 c4 00 00 00 00", "native tooltip hit visibility and death")
            .uses(&[element::VISIBLE, DEAD]),
        Point::keep(0x155096ausize, "80 b9 db 02 00 00 00", "native tooltip layout ready").uses(&[NODE_DIRTY]),
        Point::keep(0x15509b6usize, "48 83 bf 78 02 00 00 00", "native tooltip texture ready").uses(&[text_texture::TEXTURE]),
        Point::keep(0x14dd36ausize, "48 8b 89 88 00 00 00", "native tooltip graphics renderer").uses(&[GRAPHICS_RENDERER]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ready_tooltip_needs_no_hit_target_but_must_have_its_current_holder_and_texture() {
        use crate::game::buffers::Object;
        let m = Memory { base: 0 };
        let texture = Object::new(8);
        // The tooltip owner deliberately clears the node's hit-target pointer.
        let node = Object::new(0x2e0).u64(0, m.at(NODE_CLASS) as u64).u64(text_texture::TEXTURE, texture.at() as u64);
        let holder = Object::new(0x1f0).u64(HOLDER_NODE, node.at() as u64).u8(element::VISIBLE, 1);
        let owner = Object::new(0xd0).u64(OWNER_HOLDER, holder.at() as u64);
        let display = Display { owner: owner.at(), holder: holder.at(), node: node.at(), text: b"Save".to_vec(), generation: 1 };
        assert_eq!(ready(&m, &display), Ok(true));
        node.set(NODE_DIRTY, &[1]);
        assert_eq!(ready(&m, &display), Ok(false));
        node.set(NODE_DIRTY, &[0]);
        holder.set(DEAD, &[1]);
        assert_eq!(ready(&m, &display), Ok(false));
        holder.set(DEAD, &[0]);
        node.set(text_texture::TEXTURE, &0u64.to_le_bytes());
        assert_eq!(ready(&m, &display), Ok(false));
        node.set(text_texture::TEXTURE, &(texture.at() as u64).to_le_bytes());
        owner.set(OWNER_HOLDER, &0u64.to_le_bytes());
        assert_eq!(ready(&m, &display), Ok(false));
    }

    #[test]
    fn the_native_vector_is_filtered_without_changing_its_storage_or_other_nodes() {
        let m = Memory { base: 0 };
        let original = crate::game::buffers::Object::new(24).vector(0, &[11, 22, 33]);
        let filtered = without_node(&m, original.at(), 22).unwrap().unwrap();
        assert_eq!(crate::containers::pointers(&m, filtered.header.as_ptr() as usize, 10).unwrap(), [11, 33]);
        assert_eq!(crate::containers::pointers(&m, original.at(), 10).unwrap(), [11, 22, 33]);
        assert!(without_node(&m, original.at(), 44).unwrap().is_none());
    }

    #[test]
    fn refreshing_unchanged_content_keeps_the_in_flight_generation() {
        let mut shared = Shared::default();
        shared.showing(1, 2, 3, b"Save");
        let prepared = shared.generation;
        shared.showing(1, 2, 3, b"Save");
        assert_eq!(shared.generation, prepared, "a presentation between ordinary drawing and Present must not invalidate the filtered node");
        shared.showing(1, 2, 3, b"Cancel");
        assert_ne!(shared.generation, prepared, "changed content cannot reuse the old frame's presentation");
        let changed = shared.generation;
        shared.showing(1, 4, 5, b"Cancel");
        assert_ne!(shared.generation, changed, "a rebuilt holder cannot reuse the old node");
    }

    #[test]
    fn the_native_node_class_and_draw_returns_are_checked_before_runtime_use() {
        for rva in [NODE_CLASS, DRAW_FIRST_RETURN, DRAW_SECOND_RETURN] {
            assert!(crate::points::checks(rva), "unchecked tooltip address {rva:#x}");
        }
    }
}
