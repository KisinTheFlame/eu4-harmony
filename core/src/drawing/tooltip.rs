//! The tooltip with the drawing thread. The in-game frame updates the
//! tooltips after its drawing (`0x6eb6e0` → `0x1414ef0`): the element under
//! the cursor, its tooltip built again when its text changed, which draws the
//! text into a texture in a scene of the renderer's, and the tooltip put
//! where the cursor is. The main thread does that update before the frame is
//! prepared instead ([`update`]), so that its scene comes before the frame is
//! handed on; and between updates, which at speed 5 come once a day, the
//! drawing thread keeps the tooltip at the cursor ([`follow_cursor`]), as it
//! does the map icons at the camera (docs/research/drawing-beside-the-day.md,
//! "Frames late at speed 5").
use super::{drawing_on, on_main, IDLER_MAP, OTHER_CAMERA};
use crate::called_by_game;
use crate::memory::Memory;
use crate::points::Point;
use std::sync::Mutex;

pub(super) fn points() -> Vec<Point> {
    vec![
        // mov rax, [r13 + 0x11c0]; cmp byte ptr [rax + 0x530], 0; je;
        // movss xmm1, [r13 + 0x310]; mov rcx, r13: the tooltips' update's
        // condition and arguments, after the drawing.
        Point::keep(
            0x81592d,
            "49 8b 85 c0 11 00 00 80 b8 30 05 00 00 00 74 11 f3 41 0f 10 8d 10 03 00 00 49 8b cd",
            "the tooltips updated after the drawing, the condition",
        )
        .uses(&[IDLER_MAP, MAP_TOOLTIPS_ON, IDLER_TOOLTIPS_TIME]),
        // call 0x6eb6e0 (UPDATE), its target in the bytes kept.
        Point::write(UPDATE_AFTER_DRAWING, "e8 92 5d ed ff", "the tooltips updated after the drawing", "0f 1f 44 00 00"),
        // mov rcx, [rcx + 0x3b0]; test rcx, rcx: the idler's tooltips.
        Point::keep(UPDATE, "48 8b 89 b0 03 00 00 48 85 c9", "the tooltips' update"),
        Point::jump(PLACED, "4c 8b 5e 08 48 8b 85 a0 00 00 00", "the tooltip's place worked out", placed_stub as *const () as usize, &PLACED_RESUME),
        // mov [rbp + 0xa0], rax; lea r8, [r11 + 0x16c]; lea r9, [rbp + 0xa0];
        // mov edx, [r11 + 0x168]; lea rcx, [rbp + 0xa8]; call ORIENTED;
        // mov rdx, [rax]; mov [r11 + 0x154], rdx; mov rcx, [r11 + 0xd0];
        // mov rax, [rcx]; mov r8, [rax + 0x168]: the place turned into the
        // window's by its orientation, kept, and set.
        Point::keep(
            PLACED_GOES_ON,
            "48 89 85 a0 00 00 00 4d 8d 83 6c 01 00 00 4c 8d 8d a0 00 00 00 41 8b 93 68 01 00 00 48 8d 8d a8 00 00 00 e8 db 82 11 00 48 8b 10 49 89 93 54 01 00 00 49 8b 8b d0 00 00 00 48 8b 01 4c 8b 80 68 01 00 00",
            "the tooltip's place set",
        )
        .uses(&[WINDOW_ANCHOR, WINDOW_ORIENTATION, WINDOW_PLACE, WINDOW_GUI, GUI_SET_PLACE]),
        Point::keep(ORIENTED, "33 c0 48 89 01 83 fa 08", "a place turned by an orientation"),
        // mov rcx, [rsi + 0x20]; mov rax, [rcx]; call [rax + 0x78]; ...;
        // mulss xmm1, [rax + 0x4dc]; ...; mulss xmm0, [rax + 0x4d8]: the cursor
        // in GUI units.
        Point::keep(
            0x1415781,
            "48 8b 4e 20 48 8b 01 ff 50 78 66 41 0f 6e 4c 24 04 0f 5b c9 f3 0f 59 88 dc 04 00 00 66 41 0f 6e 04 24 0f 5b c0 f3 0f 59 80 d8 04 00 00",
            "the tooltip's cursor in GUI units",
        )
        .uses(&[TOOLTIPS_MANAGER, MANAGER_GRAPHICS, GRAPHICS_TO_GUI, GRAPHICS_TO_GUI + 4]),
        // mov rcx, [rsi + 0x20]; mov rax, [rcx]; call [rax + 0x78];
        // mov rcx, [rax + 0x490]: the screen's size in GUI units.
        Point::keep(0x1415744, "48 8b 4e 20 48 8b 01 ff 50 78 48 8b 88 90 04 00 00", "the tooltip's screen").uses(&[GRAPHICS_SCREEN]),
        // mov rax, [rsi + 8]; mov rcx, [rax + 0xd0]; mov rax, [rcx];
        // lea rdx, [rbp + 0xa8]; call [rax + 0x180]: the tooltip's size.
        Point::keep(0x14157cb, "48 8b 46 08 48 8b 88 d0 00 00 00 48 8b 01 48 8d 95 a8 00 00 00 ff 90 80 01 00 00", "the tooltip's size")
            .uses(&[TOOLTIPS_WINDOW, GUI_SIZE]),
    ]
}

/// The tooltips' update (`0x6eb6e0`: the idler's tooltips at its `+0x3b0`,
/// `0x1414ef0`), given the idler and a time (`+0x310`, written each frame at
/// `0x813e75`): the tooltip of the element under the cursor is built again
/// when its text changed, the text drawn into a texture of its own in a
/// scene of the renderer's (`0x1415bf0` → `0x1550e90` → `0x1596660`). The
/// in-game frame calls it after its drawing, while the graphical map's
/// `+0x530` is set (`0x81592d`–`0x815949`), so a tooltip built again began a
/// scene after the frame was prepared, and held the drawing thread back to
/// the next update; at speed 5 the text under the cursor changes every day.
/// It is called before the frame is prepared instead, and the call after the
/// drawing is taken out: nothing the drawing does is read by it (the drawing
/// reads `+0x310`, never writes it), and its only caller is that one.
const UPDATE: usize = 0x6eb6e0;
/// The idler's tooltips, which the update reads first (`0x6eb6e0`).
const IDLER_TOOLTIPS: usize = 0x3b0;
const UPDATE_AFTER_DRAWING: usize = 0x815949;
const IDLER_TOOLTIPS_TIME: usize = 0x310;
const MAP_TOOLTIPS_ON: usize = 0x530;
/// In the tooltip's build and placing (`0x1415390`, `rsi` the tooltips,
/// `r12` the cursor in pixels of the frame): `mov r11, [rsi + 8]; mov rax,
/// [rbp + 0xa0]`, with the tooltip's place in GUI units at `[rbp + 0xa0]`
/// (the cursor in GUI units, moved by the tooltip offset, kept on the screen
/// by `0x14164a0`), before it is turned into the window's place.
const PLACED: usize = 0x14159d2;
const PLACED_GOES_ON: usize = 0x14159dd;
crate::resume!(PLACED_RESUME: PLACED_GOES_ON_AT = PLACED_GOES_ON);
/// The tooltips' window holder, and in it the window's orientation, the
/// anchor its place is turned by, the window's place kept, and the GUI object
/// with the slots that set its place and give its size.
const TOOLTIPS_WINDOW: usize = 0x8;
const WINDOW_ORIENTATION: usize = 0x168;
const WINDOW_ANCHOR: usize = 0x16c;
const WINDOW_PLACE: usize = 0x154;
const WINDOW_GUI: usize = 0xd0;
const GUI_SET_PLACE: usize = 0x168;
const GUI_SIZE: usize = 0x180;
/// A place turned by an orientation (`0x152dce0`: out, the orientation, the
/// anchor, the place): the place plus what of the anchor the orientation
/// takes. It writes only its out.
const ORIENTED: usize = 0x152dce0;
/// The tooltips' GUI manager, its slot that gives the graphics object, and in
/// that the factors from pixels to GUI units (two f32) and the screen's size
/// in GUI units (two i32).
const TOOLTIPS_MANAGER: usize = 0x20;
const MANAGER_GRAPHICS: usize = 0x78;
const GRAPHICS_TO_GUI: usize = 0x4d8;
const GRAPHICS_SCREEN: usize = 0x490;

/// The tooltip as the main thread last placed it: the tooltips, the cursor
/// in pixels, the place in GUI units.
#[derive(Clone, Copy)]
struct Placed {
    tooltips: usize,
    cursor: [i32; 2],
    place: [i32; 2],
}

/// The last placing, and the place the drawing thread last set from it.
static PLACING: Mutex<Option<(Placed, [i32; 2])>> = Mutex::new(None);

/// At every in-game frame, as it calls its drawing, by the main thread: the
/// tooltips updated as the frame would after its drawing ([`UPDATE`]).
pub(super) fn update(m: &Memory, idler: usize) -> crate::Result<()> {
    let map = m.ptr(idler + IDLER_MAP)?;
    if map == 0 || m.u8(map + MAP_TOOLTIPS_ON)? == 0 {
        return Ok(());
    }
    let time = f32::from_bits(m.u32(idler + IDLER_TOOLTIPS_TIME)?);
    let update: unsafe extern "C" fn(usize, f32) = unsafe { std::mem::transmute(m.at(UPDATE)) };
    unsafe { update(idler, time) };
    Ok(())
}

#[unsafe(naked)]
extern "C" fn placed_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rsi", "mov rdx, r12", "mov r8, qword ptr [rbp]", "add r8, 0xa0", call placed),
        "mov r11, qword ptr [rsi + 8]",
        "mov rax, qword ptr [rbp + 0xa0]",
        "jmp qword ptr [rip + {goes_on}]",
        placed = sym placed,
        goes_on = sym PLACED_GOES_ON_AT,
    );
}

/// The game works the tooltip's place out from the cursor at `cursor`, the
/// place at `place_at`; the stub finds it in the game's frame through the
/// `rbp` that `call_rust!` keeps at `[rbp]`.
#[called_by_game]
extern "C" fn placed(tooltips: usize, cursor: usize, place_at: usize) {
    if !on_main() || !drawing_on() {
        return;
    }
    let m = crate::hooked_memory();
    let pair = |at: usize| -> crate::Result<[i32; 2]> { Ok([m.u32(at)? as i32, m.u32(at + 4)? as i32]) };
    match (pair(cursor), pair(place_at)) {
        (Ok(cursor), Ok(place)) => *PLACING.lock().unwrap_or_else(|e| e.into_inner()) = Some((Placed { tooltips, cursor, place }, place)),
        _ => log!("drawing: the tooltip's place could not be read"),
    }
}

/// Before the drawing thread draws a frame, under the lock: the tooltip
/// moved by what the cursor moved since the main thread placed it, kept on
/// the screen as the game keeps it, and set as the game sets it.
pub(super) fn follow_cursor(m: &Memory, idler: usize) -> crate::Result<()> {
    let mut placing = PLACING.lock().unwrap_or_else(|e| e.into_inner());
    let Some((placed, set)) = placing.as_mut() else {
        return Ok(());
    };
    // A tooltip placed by another idler's tooltips (the main menu's, before
    // a game was loaded) is not this frame's, and they may be gone.
    if m.ptr(idler + IDLER_TOOLTIPS)? != placed.tooltips {
        *placing = None;
        return Ok(());
    }
    // Another camera's tooltip is placed otherwise (`0x1415816`).
    if m.u8(m.at(OTHER_CAMERA))? != 0 {
        return Ok(());
    }
    let Some(now) = crate::game::input::cursor_now() else {
        return Ok(());
    };
    let manager = m.ptr(placed.tooltips + TOOLTIPS_MANAGER)?;
    let graphics_of: unsafe extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(m.ptr(m.ptr(manager)? + MANAGER_GRAPHICS)?) };
    let graphics = unsafe { graphics_of(manager) };
    let to_gui = [f32::from_bits(m.u32(graphics + GRAPHICS_TO_GUI)?), f32::from_bits(m.u32(graphics + GRAPHICS_TO_GUI + 4)?)];
    let screen = [m.u32(graphics + GRAPHICS_SCREEN)? as i32, m.u32(graphics + GRAPHICS_SCREEN + 4)? as i32];
    let window = m.ptr(placed.tooltips + TOOLTIPS_WINDOW)?;
    let gui = m.ptr(window + WINDOW_GUI)?;
    let mut size = [0i32; 2];
    let size_of: unsafe extern "C" fn(usize, *mut [i32; 2]) -> usize = unsafe { std::mem::transmute(m.ptr(m.ptr(gui)? + GUI_SIZE)?) };
    unsafe { size_of(gui, &mut size) };
    let place = followed(placed.place, placed.cursor, now, to_gui, screen, size);
    if place == *set {
        return Ok(());
    }
    *set = place;
    let mut out = 0u64;
    let oriented: unsafe extern "C" fn(*mut u64, u32, usize, *const [i32; 2]) -> *const u64 = unsafe { std::mem::transmute(m.at(ORIENTED)) };
    let window_place = unsafe { *oriented(&mut out, m.u32(window + WINDOW_ORIENTATION)?, window + WINDOW_ANCHOR, &place) };
    m.write(window + WINDOW_PLACE, &window_place.to_le_bytes())?;
    let pair = [window_place as u32 as i32, (window_place >> 32) as u32 as i32];
    let set_place: unsafe extern "C" fn(usize, *const [i32; 2]) -> usize = unsafe { std::mem::transmute(m.ptr(m.ptr(gui)? + GUI_SET_PLACE)?) };
    unsafe { set_place(gui, &pair) };
    Ok(())
}

/// Where a tooltip placed at `place` for the cursor at `was` goes for the
/// cursor at `now` (pixels), `to_gui` the factors from pixels to GUI units:
/// moved by what the cursor moved in GUI units, as the game works them out,
/// and kept on a screen of `screen` whole, as `0x14164a0` keeps it.
fn followed(place: [i32; 2], was: [i32; 2], now: [i32; 2], to_gui: [f32; 2], screen: [i32; 2], size: [i32; 2]) -> [i32; 2] {
    std::array::from_fn(|i| {
        let moved = place[i] + ((now[i] as f32 * to_gui[i]) as i32 - (was[i] as f32 * to_gui[i]) as i32);
        if moved + size[i] > screen[i] {
            screen[i] - size[i]
        } else {
            moved.max(0)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::followed;

    #[test]
    fn a_tooltip_moves_with_the_cursor_and_stays_on_the_screen() {
        let screen = [1920, 1080];
        let size = [300, 100];
        assert_eq!(followed([500, 400], [1000, 800], [1000, 800], [0.5, 0.5], screen, size), [500, 400]);
        assert_eq!(followed([500, 400], [1000, 800], [1100, 760], [0.5, 0.5], screen, size), [550, 380]);
        assert_eq!(followed([1600, 400], [1000, 800], [1200, 800], [1.0, 1.0], screen, size), [1620, 400]);
        assert_eq!(followed([10, 10], [100, 100], [0, 0], [1.0, 1.0], screen, size), [0, 0]);
    }
}
