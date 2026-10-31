//! The map camera: where the view looks and how high it is, and turning it to
//! a province, to a map position or to another height. A move is made at
//! once: the next frame drawn shows the camera where it was sent, without the
//! glide the game's own moves have.
//!
//! A map position is a pixel of the province map (`map/provinces.bmp`): x
//! from its left edge, y from its bottom edge, as `map/positions.txt` writes
//! them. The camera's height is in the same unit, above the map's base plane
//! (docs/research/camera.md).
use super::{context, fail, game, map_manager, with_state, Caller, Ctx, GResult, Status};
use crate::{
    memory::Memory,
    objects::{
        camera::{self as layout, camera as fields, grid},
        campaign::globals,
    },
    points::Point,
};
use eu4_harmony_checks::camera as checks;

/// The map manager looking at a province (rcx the map manager, rdx the
/// province), at its map data's position, keeping the camera's height.
const LOOK_AT_PROVINCE: usize = 0x80fe20;
/// The map manager looking at a map position (rcx the map manager, edx x,
/// r8d y), keeping the camera's height; where the map wraps, it takes the
/// copy of the position nearest the camera.
const LOOK_AT_POSITION: usize = 0x80fd70;
/// The camera's goal set to look at a point from its height (rcx the camera,
/// rdx three f32: x, the height, z), kept on the map; nothing while the
/// camera is locked.
const LOOK_FROM: usize = 0x109d140;
/// The camera put at its goal at once (rcx the camera).
const SETTLE: usize = 0x109cd80;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(LOOK_AT_PROVINCE, "48 83 ec 38 83 7a 20 00 0f 8e a9 00 00 00 48 8b 42 28 4c 8b 89 00 12 00 00 8b 50 7c", "camera to a province"),
        Point::keep(LOOK_AT_POSITION, "48 83 ec 38 4c 8b 91 00 12 00 00 44 8b ca 41 80 ba 78 07 00 00 00", "camera to a position")
            .uses(&[layout::IN_MAP_MANAGER, fields::WRAPS]),
        Point::keep(LOOK_FROM, "80 b9 58 07 00 00 00 75 6b 8b 02 89 81 94 06 00 00", "camera goal").uses(&[fields::LOCKED, fields::GOAL_POSITION]),
        Point::keep(SETTLE, "48 89 5c 24 08 57 48 83 ec 20 8b 81 94 06 00 00 48 8b f9 89 81 7c 06 00 00", "camera to its goal")
            .uses(&[fields::GOAL_POSITION, fields::POSITION]),
    ]
}

/// Where the camera looks and how high it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Camera {
    /// The map position at the middle of the view: where the line of sight
    /// meets the map's base plane.
    pub x: u32,
    pub y: u32,
    /// The camera's height above the map's base plane, in pixels of the map.
    pub height: u32,
    /// The province at that position; none where there is no province.
    pub province: Option<u32>,
    /// The lowest and the highest the camera may be; the game keeps it higher
    /// than the lowest over high ground.
    pub min_height: u32,
    pub max_height: u32,
}

/// Where the camera looks now.
pub fn camera(_: Caller) -> GResult<Camera> {
    let m = game()?;
    let ctx = context(m)?;
    read(m, find(m, &ctx)?.1)
}

/// Turns the camera to a province, where the game looks at it from, keeping
/// the height.
pub fn look_at_province(_: Caller, province: u32) -> GResult<Camera> {
    let m = game()?;
    let ctx = context(m)?;
    if province == 0 {
        return fail(Status::NotFound, "no province 0");
    }
    let (manager, camera) = movable(m, &ctx)?;
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    m.call(m.at(LOOK_AT_PROVINCE), &[manager, address])?;
    settle(m, camera)
}

/// Turns the camera to a map position, keeping the height.
pub fn look_at(_: Caller, x: u32, y: u32) -> GResult<Camera> {
    let m = game()?;
    let ctx = context(m)?;
    checks::position(x, y, map_size(m)?)?;
    let (manager, camera) = movable(m, &ctx)?;
    m.call(m.at(LOOK_AT_POSITION), &[manager, x as usize, y as usize])?;
    settle(m, camera)
}

/// Raises or lowers the camera to a height, looking at the same position.
pub fn zoom(_: Caller, height: u32) -> GResult<Camera> {
    let m = game()?;
    let ctx = context(m)?;
    checks::height(height, limits(m, find(m, &ctx)?.1)?)?;
    let (_, camera) = movable(m, &ctx)?;
    let [x, _, z] = point(m, camera + fields::GOAL_LOOK_AT)?;
    let goal = [x, height as f32, z];
    m.call(m.at(LOOK_FROM), &[camera, goal.as_ptr() as usize])?;
    settle(m, camera)
}

/// The map manager and its camera, checked by its class.
fn find(m: &Memory, ctx: &Ctx) -> GResult<(usize, usize)> {
    let manager = map_manager(m, ctx)?;
    let camera = m.ptr(manager + layout::IN_MAP_MANAGER)?;
    if camera == 0 || m.ptr(camera)? != m.at(layout::CLASS) {
        return Err("unexpected camera".to_string().into());
    }
    Ok((manager, camera))
}

/// The map manager and its camera, which must not be locked.
fn movable(m: &Memory, ctx: &Ctx) -> GResult<(usize, usize)> {
    let (manager, camera) = find(m, ctx)?;
    if m.u8(camera + fields::LOCKED)? != 0 {
        return Err(checks::locked().into());
    }
    Ok((manager, camera))
}

/// Puts the camera at its goal at once, and reads it there.
fn settle(m: &Memory, camera: usize) -> GResult<Camera> {
    m.call(m.at(SETTLE), &[camera])?;
    read(m, camera)
}

fn read(m: &Memory, camera: usize) -> GResult<Camera> {
    let [x, _, z] = point(m, camera + fields::LOOK_AT)?;
    let [_, height, _] = point(m, camera + fields::POSITION)?;
    let (width, map_height) = map_size(m)?;
    let (min_height, max_height) = limits(m, camera)?;
    let (x, y) = position(x, z, width, map_height, m.u8(camera + fields::WRAPS)? != 0);
    Ok(Camera { x, y, height: height.round().max(0.0) as u32, province: province_at(m, x, y)?, min_height, max_height })
}

/// The map position of a point the camera looks at: rounded to a pixel, x
/// brought onto the map across its edges when it `wraps`, as the game's map
/// does, kept on it otherwise, as y always is.
fn position(x: f32, z: f32, width: u32, height: u32, wraps: bool) -> (u32, u32) {
    let x = x.round() as i64;
    let x = if wraps { x.rem_euclid(width as i64) } else { x.clamp(0, width as i64 - 1) } as u32;
    let y = (z.round() as i64).clamp(0, height as i64 - 1) as u32;
    (x, y)
}

/// A point of the camera's: x, the height, z.
fn point(m: &Memory, at: usize) -> GResult<[f32; 3]> {
    let mut out = [0.0; 3];
    for (i, value) in out.iter_mut().enumerate() {
        *value = f32::from_bits(m.u32(at + 4 * i)?);
        if !value.is_finite() || value.abs() > 1e6 {
            return Err(format!("camera point {value}").into());
        }
    }
    Ok(out)
}

/// The lowest and the highest the camera may be.
fn limits(m: &Memory, camera: usize) -> GResult<(u32, u32)> {
    let lowest = m.i32(camera + fields::MIN_HEIGHT)?;
    let highest = m.i32(camera + fields::MAX_HEIGHT)?;
    if lowest < 0 || highest < lowest || highest > 1_000_000 {
        return Err(format!("camera heights {lowest} to {highest}").into());
    }
    Ok((lowest as u32, highest as u32))
}

/// The map's size in pixels, across and up.
fn map_size(m: &Memory) -> GResult<(u32, u32)> {
    let map = m.ptr(m.at(globals::MAP))?;
    let width = m.i32(map + grid::WIDTH)?;
    let height = m.i32(map + grid::HEIGHT)?;
    if !(1..=65_536).contains(&width) || !(1..=65_536).contains(&height) {
        return Err(format!("map size {width} by {height}").into());
    }
    Ok((width as u32, height as u32))
}

/// The province at a map position on the map.
fn province_at(m: &Memory, x: u32, y: u32) -> GResult<Option<u32>> {
    let map = m.ptr(m.at(globals::MAP))?;
    let (width, _) = map_size(m)?;
    let ids = m.ptr(map + grid::IDS)?;
    let id = m.u16(ids + 2 * (y as usize * width as usize + x as usize))? as i16;
    Ok((id > 0).then_some(id as u32))
}

#[cfg(test)]
mod tests {
    use super::position;

    #[test]
    fn a_point_comes_onto_the_map() {
        assert_eq!(position(2842.4, 1493.6, 5632, 2048, true), (2842, 1494));
        // Where the map wraps, a point past an edge is the same as one on it.
        assert_eq!(position(-10.0, 5.0, 5632, 2048, true), (5622, 5));
        assert_eq!(position(5632.2, 5.0, 5632, 2048, true), (0, 5));
        // A map that does not wrap keeps it on the map, as up and down.
        assert_eq!(position(-10.0, 5.0, 5632, 2048, false), (0, 5));
        assert_eq!(position(5632.2, 5.0, 5632, 2048, false), (5631, 5));
        assert_eq!(position(1.0, 2048.0, 5632, 2048, true), (1, 2047));
        assert_eq!(position(1.0, -3.0, 5632, 2048, true), (1, 0));
    }
}
