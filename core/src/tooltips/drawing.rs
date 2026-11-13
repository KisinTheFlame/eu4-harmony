//! A self-drawn control's tooltip remains the game's single ToolTip texture
//! node. Filter only that ready node from the current native draw list, then
//! draw it once after the framework UI, before the presentation lock opens.
//! See research/panel-entry-text.md and research/gui-drawing.md.
use super::handoff::{Current, Display, Filter, Frame, Handoff, Native};
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

static HANDOFF: Mutex<Handoff> = Mutex::new(Handoff::new());

pub(super) fn with_handoff<T>(f: impl FnOnce(&mut Handoff) -> T) -> T {
    f(&mut HANDOFF.lock().unwrap_or_else(|e| e.into_inner()))
}

pub(super) fn clear() {
    with_handoff(Handoff::clear);
}

pub(super) fn showing(m: &Memory, owner: usize, text: &[u8]) -> Result<()> {
    let holder = m.ptr(owner + OWNER_HOLDER)?;
    let node = if holder != 0 && !text.is_empty() { m.ptr(holder + HOLDER_NODE)? } else { 0 };
    with_handoff(|handoff| handoff.showing(owner, holder, node, text));
    Ok(())
}

fn current() -> Current {
    // Release the handoff mutex before asking the framework UI for its text.
    // With no prepared presentation this does not visit the UI at all.
    with_handoff(|handoff| handoff.current(None)).with_text(crate::framework_ui::tooltip_at)
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

fn without_node(m: &Memory, header: usize, node: usize) -> Result<Option<Filter>> {
    let nodes = crate::containers::pointers(m, header, 16384)?;
    if !nodes.contains(&node) {
        return Ok(None);
    }
    Ok(Some(Filter::new(nodes.into_iter().filter(|item| *item != node).collect())))
}

struct Game<'a>(&'a Memory);

impl Native for Game<'_> {
    fn ready(&self, display: &Display) -> Result<bool> {
        ready(self.0, display)
    }

    fn without_node(&self, list: usize, node: usize) -> Result<Option<Filter>> {
        without_node(self.0, list, node)
    }

    fn renderer(&self, graphics: usize) -> Result<usize> {
        self.0.ptr(graphics + GRAPHICS_RENDERER)
    }

    fn begin(&mut self, renderer: usize) -> Result<bool> {
        // The native entry also resets the renderer's declaration cache.
        // Both scene functions return a Boolean in AL, not all of RAX.
        BEGIN.call(self.0, [renderer]).map(|value| value & 0xff != 0)
    }

    fn setup(&mut self, graphics: usize) -> Result<()> {
        SETUP.call(self.0, [graphics]).map(|_| ())
    }

    fn draw(&mut self, graphics: usize, node: usize) -> Result<()> {
        let filter = Filter::new(vec![node]);
        DRAW.call(self.0, [graphics, filter.header()]).map(|_| ())
    }

    fn end(&mut self, renderer: usize) -> Result<bool> {
        END.call(self.0, [renderer]).map(|value| value & 0xff != 0)
    }
}

thread_local! {
    static FRAME: RefCell<Frame> = const { RefCell::new(Frame::new()) };
    // Native drawing reenters the draw-list observers while FRAME is borrowed.
    // The handoff's scene guard sets this before calling it and clears it on
    // every return or unwind, after ending the scene.
    static IN_LATE: Cell<bool> = const { Cell::new(false) };
}

fn frame_before(_: &mut Call) {
    discard_late();
}

fn filter_before(call: &mut Call) {
    if IN_LATE.get() {
        return;
    }
    let m = call.memory();
    let eligible = (crate::thread_id() == crate::main_thread() || crate::drawing::is_drawing_thread())
        && [m.at(DRAW_FIRST_RETURN), m.at(DRAW_SECOND_RETURN)].contains(&call.caller());
    let filtered = FRAME.with_borrow_mut(|frame| frame.filter(|| eligible.then(current), &Game(&m), call.arg(0), call.arg(1)));
    if let Some(header) = filtered {
        call.set_arg(1, header);
    }
}

fn filter_after(_: &mut Call) {
    if !IN_LATE.get() {
        FRAME.with_borrow_mut(Frame::returned);
    }
}

pub(crate) fn discard_late() {
    FRAME.with_borrow_mut(Frame::discard);
}

/// Finish the current frame after the UI mutex and its D3D state block have
/// been released, before screenshot/Present release the presentation lock.
pub(crate) fn draw_late(renderer: usize) {
    let Some(m) = crate::memory() else {
        discard_late();
        return;
    };
    let result = IN_LATE.with(|drawing| FRAME.with_borrow_mut(|frame| frame.finish(current, &mut Game(m), renderer, drawing)));
    if let Err(e) = result {
        log!("late native tooltip: {e}");
    }
}

fn owner_invalidated(call: &mut Call) {
    with_handoff(|handoff| handoff.owner_invalidated(call.arg(0)));
}

fn holder_deleted(call: &mut Call) {
    with_handoff(|handoff| handoff.holder_deleted(call.arg(0)));
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
        assert_eq!(crate::containers::pointers(&m, filtered.header(), 10).unwrap(), [11, 33]);
        assert_eq!(crate::containers::pointers(&m, original.at(), 10).unwrap(), [11, 22, 33]);
        assert!(without_node(&m, original.at(), 44).unwrap().is_none());
    }

    #[test]
    fn the_native_node_class_and_draw_returns_are_checked_before_runtime_use() {
        for rva in [NODE_CLASS, DRAW_FIRST_RETURN, DRAW_SECOND_RETURN] {
            assert!(crate::points::checks(rva), "unchecked tooltip address {rva:#x}");
        }
    }
}
