//! What the main thread does of the drawing as it prepares a frame: the
//! parts of the game's drawing, and of what the in-game frame does after it,
//! that read the world or draw into a texture of the interface's. Done by the
//! main thread before the frame is handed on, they leave the drawing thread
//! nothing to read that the main thread changes after.
//!
//! - [`ahead`], at every frame, before the drawing: the tooltips' update,
//!   which the frame does after its drawing ([`super::tooltip`]); and the
//!   map's names when they are due, which the 3D map's drawing makes.
//! - [`first_steps`], for a frame the drawing thread draws: the drawing's two
//!   first steps, which read the world (docs/research/drawing-beside-the-day.md,
//!   "The drawing's first steps"), and which the drawing thread skips.
use super::{direct1, direct3, drawing_on, APP, APP_MAP, IDLER_CAMERA, IDLER_MAP, SETTINGS};
use crate::memory::Memory;
use crate::points::{GameFunction, Point};

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::keep(MAKE_NAMES, "48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57", "the map's names made"),
        // cmp byte ptr [rbx + 0x25a0], 0: the names due, in the 3D map's drawing.
        Point::keep(0x10a5b55, "80 bb a0 25 00 00 00", "the map's names due").uses(&[MAP_NAMES_DUE]),
        // mov dword ptr [rbx + 0x638], 15; mov rcx, rbx; call; mov byte ptr [rbx + 0x25a0], 0
        Point::keep(0x10a5b6d, "c7 83 38 06 00 00 0f 00 00 00 48 8b cb e8 61 40 00 00 c6 83 a0 25 00 00 00", "the map's names made in the drawing")
            .uses(&[MAP_NAMES_LEVEL, MAP_NAMES_DUE]),
        // cmp byte ptr [r15 + 0x34], 0; jne; cmp byte ptr [r15 + 0x44], 0; je
        Point::keep(0x818619, "41 80 7f 34 00 75 07 41 80 7f 44 00 74 3a", "the map mode's colors' flags").uses(&[IDLER_COLORS_DUE, IDLER_COLORS_ASKED]),
        // comiss xmm0, [floor]: the ambient sound's floor of the master volume.
        Point::keep(0x8185f6, "0f 2f 05 db 93 5a 01", "the ambient sound's volume floor").uses(&[VOLUME_FLOOR]),
        Point::keep(AMBIENT, "48 8b c4 4c 89 40 18 48 89 50 10 48 89 48 08 55 53 56 57", "the ambient sound"),
        Point::keep(CLOCK, "48 89 5c 24 08 57 48 83 ec 20 48 8b d9", "the colors' clock"),
    ]
}

/// The graphical map's flag that its names are to be made again.
const MAP_NAMES_DUE: usize = 0x25a0;
/// The graphical map's names' level, which the names are made for.
const MAP_NAMES_LEVEL: usize = 0x638;
/// The map's names made again (rcx the graphical map, edx the level, r8b
/// whether even for the level they have): from the countries and the
/// provinces, their meshes through the back end. The 3D map's drawing does
/// it when they are due: the level set to 15, which none is, the names made
/// for the level they had, and the flag cleared (`0x10a5b55`–`0x10a5b7f`).
const MAKE_NAMES: GameFunction<3> = GameFunction::at(0x10a9be0);
/// The settings' master volume, and the floor above which the ambient sound
/// is worked out.
const MASTER_VOLUME: usize = 0xb4;
const VOLUME_FLOOR: usize = 0x1dc19d8;
/// The ambient sound: the idler's sound object, the graphical map, the camera.
const AMBIENT: GameFunction<3> = GameFunction::at(0x6e7a50);
/// The map mode's colors: the graphical map, the map mode, a second value.
const COLORS: GameFunction<3> = GameFunction::at(crate::map::RENDERER);
/// The clock the colors' refresh is timed by: writes the time at its
/// argument and returns it.
const CLOCK: GameFunction<1> = GameFunction::at(0x5bf840);
/// The in-game idler's fields the first steps read.
const IDLER_SOUND: usize = 0x5a0;
const IDLER_COLORS_DUE: usize = 0x34;
const IDLER_COLORS_ASKED: usize = 0x44;
const IDLER_COLORS_AT: usize = 0x38;
const IDLER_MAP_MODE: usize = 0x2c;
const IDLER_MAP_MODE_ARG: usize = 0x40;

/// At every in-game frame, as it calls its drawing: the tooltips updated,
/// and, with the drawing thread on, the map's names made when they are due.
pub(super) fn ahead(m: &Memory, idler: usize) {
    if let Err(e) = super::tooltip::update(m, idler) {
        log!("drawing: the tooltips could not be updated: {e}");
    }
    if drawing_on() {
        if let Err(e) = make_names(m) {
            log!("drawing: the map's names could not be made: {e}");
        }
    }
}

/// Whether the map's names are due, or cannot be read.
pub(super) fn names_due(m: &Memory) -> bool {
    m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_MAP)).and_then(|map| m.u8(map + MAP_NAMES_DUE)).ok().is_none_or(|due| due != 0)
}

/// The map's names made again when they are due, as the 3D map's drawing
/// makes them.
fn make_names(m: &Memory) -> crate::Result<()> {
    let map = m.ptr(m.ptr(m.at(APP))? + APP_MAP)?;
    if map == 0 || m.u8(map + MAP_NAMES_DUE)? == 0 {
        return Ok(());
    }
    let level = m.u32(map + MAP_NAMES_LEVEL)?;
    m.write(map + MAP_NAMES_LEVEL, &15u32.to_le_bytes())?;
    unsafe { direct3(m, MAKE_NAMES)(map, level as usize, 0) };
    m.write(map + MAP_NAMES_DUE, &[0])?;
    Ok(())
}

/// The drawing's first steps, as the drawing does them (`0x8185e7`–`0x81865c`).
pub(super) fn first_steps(m: &Memory, idler: usize) -> crate::Result<()> {
    let settings = m.ptr(m.at(SETTINGS))?;
    let volume = f32::from_bits(m.u32(settings + MASTER_VOLUME)?);
    let floor = f32::from_bits(m.u32(m.at(VOLUME_FLOOR))?);
    if volume > floor {
        let (map, camera) = (m.ptr(idler + IDLER_MAP)?, m.ptr(idler + IDLER_CAMERA)?);
        unsafe { direct3(m, AMBIENT)(idler + IDLER_SOUND, map, camera) };
    }
    if m.u8(idler + IDLER_COLORS_DUE)? != 0 || m.u8(idler + IDLER_COLORS_ASKED)? != 0 {
        let mut now = 0i64;
        let at = unsafe { direct1(m, CLOCK)(&mut now as *mut i64 as usize) };
        let now = if at != 0 { m.u64(at)? as i64 } else { now };
        if now >= m.u64(idler + IDLER_COLORS_AT)? as i64 {
            let map = m.ptr(m.ptr(m.at(APP))? + APP_MAP)?;
            let (mode, arg) = (m.u32(idler + IDLER_MAP_MODE)? as usize, m.u32(idler + IDLER_MAP_MODE_ARG)? as usize);
            unsafe { direct3(m, COLORS)(map, mode, arg) };
            m.write(idler + IDLER_COLORS_DUE, &[0])?;
            m.write(idler + IDLER_COLORS_ASKED, &[0])?;
        }
    }
    Ok(())
}
