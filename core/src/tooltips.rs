//! One native tooltip presentation seam. The game still picks its element,
//! times delayed text, lays out colors and line breaks, rebuilds only changed
//! text, and places the tooltip within the screen. Only a framework-owned
//! target replaces the final content (research/panel-entry-text.md).
use crate::{game_string, hooks::Call, memory::Memory, points::Point, Result};
use std::cell::Cell;
mod drawing;
pub(crate) use drawing::{discard_late, draw_late};
pub(crate) fn clear() {
    drawing::clear();
    drawing::discard_late();
}

const PRESENT: usize = 0x1415390;
const HIT: usize = 0x14debe0;
const HIT_RETURN: usize = 0x141548f;
const CONTENT: usize = 0x1415bf0;
const HIT_ELEMENT: usize = 8;

thread_local! {
    // A native pointer never survives the current presentation call. In
    // particular, a destroyed or rebuilt row cannot supply next frame's text.
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static TARGET: Cell<Option<usize>> = const { Cell::new(None) };
    static SELF_DRAWN: Cell<bool> = const { Cell::new(false) };
}

fn before(_: &mut Call) {
    if crate::thread_id() == crate::main_thread() {
        ACTIVE.set(true);
        TARGET.set(None);
        SELF_DRAWN.set(false);
    }
}

fn after(call: &mut Call) {
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    if call.faulted() || !SELF_DRAWN.replace(false) {
        drawing::clear();
    }
    TARGET.set(None);
    ACTIVE.set(false);
}

/// The view callback also supplies the actual hit in the alternative camera
/// path, whose scene lookup differs from the normal cursor lookup.
pub(crate) fn hit(element: usize) {
    if ACTIVE.get() {
        TARGET.set(Some(element));
    }
}

fn hit_after(call: &mut Call) {
    if ACTIVE.get() && !call.faulted() && call.caller() == call.memory().at(HIT_RETURN) {
        if let Ok(element) = call.memory().ptr(call.arg(1) + HIT_ELEMENT) {
            hit(element);
        }
    }
}

fn content_before(call: &mut Call) {
    if !ACTIVE.get() || crate::thread_id() != crate::main_thread() {
        return;
    }
    let text = if let Some(text) = crate::framework_ui::tooltip_at() {
        SELF_DRAWN.set(true);
        if let Err(e) = drawing::showing(&call.memory(), call.arg(0), &text) {
            drawing::clear();
            log!("native self-drawn tooltip binding: {e}");
        }
        Some(text)
    } else {
        TARGET.get().and_then(|target| crate::windows::managed_tooltip(target).or_else(|| crate::ui::managed_tooltip(target)))
    };
    // None is an unmanaged hit: preserve both native strings and every flag.
    // Some(empty) is an explicitly empty or hidden managed target.
    let Some(text) = text else { return };
    if let Err(e) = replace(&call.memory(), call.arg(1), &text) {
        log!("tooltip content replacement failed: {e}");
    }
}

fn replace(m: &Memory, aggregate: usize, text: &[u8]) -> Result<()> {
    let empty = game_string::empty().map(usize::to_le_bytes);
    for offset in [0, game_string::OBJECT] {
        let string = aggregate + offset;
        game_string::destroy_checked(m, string)?;
        m.write(string, empty.as_flattened())?;
    }
    if !text.is_empty() {
        game_string::append_checked(m, aggregate, text)?;
    }
    Ok(())
}

pub fn points() -> Vec<Point> {
    let mut points = vec![
        Point::observe(PRESENT, "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57", "native tooltip presentation", 2, Some(before), Some(after)),
        Point::observe(HIT, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55", "native tooltip cursor hit", 3, None, Some(hit_after)),
        Point::keep(0x1415483usize, "48 8b 88 c0 00 00 00 e8 51 97 0c 00 4c 8b 74 24 38", "tooltip uses the scene cursor hit"),
        Point::keep(0x14dedfbusize, "48 8b 46 08 49 89 44 24 08", "tooltip hit element pair").uses(&[HIT_ELEMENT]),
        Point::keep(0x81f4e5usize, "48 8d 73 20 48 8d 6f 20", "tooltip delayed string").uses(&[game_string::OBJECT]),
        Point::observe(CONTENT, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55", "native tooltip final content", 2, Some(content_before), None),
    ];
    points.extend(drawing::points());
    points
}
