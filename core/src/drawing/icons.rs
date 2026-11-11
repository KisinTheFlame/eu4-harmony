//! The map icons follow the drawing's camera (docs/plans/drawing-thread.md,
//! T3a). The interface places each map icon's window (a unit's flag and
//! number, a trade node's, a siege's and the rest) where its point on the
//! map shows with the camera of its update (`0x132e300`,
//! docs/research/drawing-beside-the-day.md, "Where the map icons are
//! placed"); the drawing thread moves the camera between updates, and the
//! icons stayed behind the map. Each placing is kept as the game makes it,
//! and handed on with the frame prepared; before it draws, the drawing
//! thread works each point out again with the camera as it is then, and
//! moves the window by the difference.
use crate::called_by_game;
use crate::memory::Memory;
use crate::objects::gui::globals::SCALE as GRAPHICS;
use crate::points::Point;
use std::sync::Mutex;

/// In the placing, the window put at its place: `mov rdx, [rsp + 0xb8]`, the
/// place's two i32, before `call r8`, the window's slot `+0x30`. Here
/// `r14` is the window, `[rsp + 0xc0]` the point's place on the screen in
/// pixels (two f32), `[rsp + 0xd8]` the point on the map (three f32) and
/// `[rsp + 0xe0]` what the matrix it was projected with is in: the camera's
/// view (the camera's `+0x30`, seen in the game), which the drawing thread's
/// camera moves.
const PLACED: usize = 0x132e5c4;
const PLACED_GOES_ON: usize = 0x132e5cc;
crate::resume!(PLACED_RESUME: PLACED_GOES_ON_AT = PLACED_GOES_ON);
/// The projection (`0x132e210`): the point times the 4×4 matrix at `+0x80`
/// of the object it is given, the rows at `+0x80`, `+0x90`, `+0xa0` and
/// `+0xb0`, the last times the constant at `0x1dc25c0` (all ones); x and y
/// over w.
const PROJECTION: usize = 0x132e210;
const MATRIX: usize = 0x80;
/// The screen's place from the projection: x × 0.5 + 0.5 and y × −0.5 + 0.5
/// (`0x1d99d94`, `0x1dc19fc`), times the screen's size in pixels at the
/// graphics object's `+0x490` (two i32), the object that also scales the
/// GUI's units ([`GRAPHICS`]).
const HALF: usize = 0x1d99d94;
const MINUS_HALF: usize = 0x1dc19fc;
const SCREEN_SIZE: usize = 0x490;
/// The window's slot that puts it at a place (`0x132e58c`).
const SET_PLACE: usize = 0x30;

/// One icon placed: its window, where the window was put, where the point
/// showed on the screen, the point on the map, and what the matrix is in.
#[derive(Clone, Copy, Debug)]
struct Icon {
    window: usize,
    put: [i32; 2],
    screen: [f32; 2],
    point: [f32; 3],
    matrix: usize,
}

/// The icons the current update placed, and those of the frame prepared.
static PLACING: Mutex<Vec<Icon>> = Mutex::new(Vec::new());
static PREPARED: Mutex<Vec<Icon>> = Mutex::new(Vec::new());

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::jump(PLACED, "48 8b 94 24 b8 00 00 00", "a map icon's window put at its place", placed_stub as *const () as usize, &PLACED_RESUME),
        Point::keep(0x132e58c, "4c 8b 40 30", "a map icon's window's slot that puts it").uses(&[SET_PLACE]),
        Point::keep(PROJECTION, "48 83 ec 28 0f 10 81 90 00 00 00", "a map icon's projection"),
        // movups xmm0, [rcx + 0xb0]; mulps xmm0, [ones]: the matrix's last row.
        Point::keep(0x132e25f, "0f 10 81 b0 00 00 00 0f 59 05 53 43 a9 00", "a map icon's projection, the matrix's last row").uses(&[MATRIX + 0x30]),
        Point::keep(0x132e3ee, "f3 0f 59 1d 06 36 a9 00", "a map icon's place, y").uses(&[MINUS_HALF]),
        Point::keep(0x132e40e, "f3 44 0f 10 0d 7d b9 a6 00", "a map icon's place, the half").uses(&[HALF]),
        // mov rax, [graphics]; ...; mov rbp, [rax + 0x490]: the screen's size.
        Point::keep(0x132e3d5, "48 8b 05 dc b0 01 01", "a map icon's place, the graphics").uses(&[GRAPHICS]),
        Point::keep(0x132e3fe, "48 8b a8 90 04 00 00", "a map icon's place, the screen's size").uses(&[SCREEN_SIZE]),
    ]
}

#[unsafe(naked)]
extern "C" fn placed_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, r14", "lea rdx, [rbp + 8]", call placed),
        "mov rdx, qword ptr [rsp + 0xb8]",
        "jmp qword ptr [rip + {goes_on}]",
        placed = sym placed,
        goes_on = sym PLACED_GOES_ON_AT,
    );
}

/// A map icon's window is put at its place, on the main thread; `frame` is
/// the placing's stack.
#[called_by_game]
extern "C" fn placed(window: usize, frame: usize) {
    if !super::on_main() || !super::drawing_on() {
        return;
    }
    let m = crate::hooked_memory();
    let read = || -> crate::Result<Icon> {
        let put = m.u64(frame + 0xb8)?;
        let point_at = m.ptr(frame + 0xd8)?;
        Ok(Icon {
            window,
            put: [put as u32 as i32, (put >> 32) as u32 as i32],
            screen: [f32::from_bits(m.u32(frame + 0xc0)?), f32::from_bits(m.u32(frame + 0xc4)?)],
            point: [f32::from_bits(m.u32(point_at)?), f32::from_bits(m.u32(point_at + 4)?), f32::from_bits(m.u32(point_at + 8)?)],
            matrix: m.ptr(frame + 0xe0)?,
        })
    };
    match read() {
        Ok(icon) => PLACING.lock().unwrap_or_else(|e| e.into_inner()).push(icon),
        Err(e) => log!("drawing: a map icon's placing could not be read: {e}"),
    }
}

/// The frame is prepared: the icons its update placed go with it.
pub(super) fn prepared() {
    let placed = std::mem::take(&mut *PLACING.lock().unwrap_or_else(|e| e.into_inner()));
    *PREPARED.lock().unwrap_or_else(|e| e.into_inner()) = placed;
}

/// Before the drawing thread draws a frame, under the lock: each icon of the
/// frame prepared moved to where its point shows now.
pub(super) fn follow_camera(m: &Memory) -> crate::Result<()> {
    let icons = PREPARED.lock().unwrap_or_else(|e| e.into_inner());
    if icons.is_empty() {
        return Ok(());
    }
    let f32_at = |at: usize| m.u32(at).map(f32::from_bits);
    let (half, minus_half) = (f32_at(m.at(HALF))?, f32_at(m.at(MINUS_HALF))?);
    let size = m.u64(m.ptr(m.at(GRAPHICS))? + SCREEN_SIZE)?;
    let (width, height) = (size as u32 as i32 as f32, (size >> 32) as u32 as i32 as f32);
    for icon in icons.iter() {
        let mut row = [[0f32; 4]; 4];
        for (r, row) in row.iter_mut().enumerate() {
            for (c, v) in row.iter_mut().enumerate() {
                *v = f32_at(icon.matrix + MATRIX + r * 16 + c * 4)?;
            }
        }
        let [x, y, z] = icon.point;
        let v: [f32; 4] = std::array::from_fn(|c| x * row[0][c] + y * row[1][c] + z * row[2][c] + row[3][c]);
        if v[3] == 0.0 {
            continue;
        }
        let screen = [(v[0] / v[3] * half + half) * width, (v[1] / v[3] * minus_half + half) * height];
        let put = [moved(icon.put[0], icon.screen[0], screen[0]), moved(icon.put[1], icon.screen[1], screen[1])];
        if put == icon.put {
            continue;
        }
        let set: unsafe extern "C" fn(usize, u64) = unsafe { std::mem::transmute(m.ptr(m.ptr(icon.window)? + SET_PLACE)?) };
        unsafe { set(icon.window, put[0] as u32 as u64 | (put[1] as u32 as u64) << 32) };
    }
    Ok(())
}

/// Where a window put at `put` for a point shown at `was` goes for the point
/// shown at `now`.
fn moved(put: i32, was: f32, now: f32) -> i32 {
    (put as f32 + (now - was)).round() as i32
}

#[cfg(test)]
mod tests {
    use super::moved;

    #[test]
    fn a_window_moves_by_what_its_point_moved() {
        assert_eq!(moved(100, 140.0, 140.0), 100);
        assert_eq!(moved(100, 140.0, 152.4), 112);
        assert_eq!(moved(-5, 10.0, 0.0), -15);
    }
}
