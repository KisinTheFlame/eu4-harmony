//! The map camera: where the view looks and how high it is, and turning it to
//! a province, to a map position or to another height. A move is made at
//! once: the next frame drawn shows the camera where it was sent, without the
//! glide the game's own moves have.
//!
//! A map position is a pixel of the province map (`map/provinces.bmp`): x
//! from its left edge, y from its bottom edge, as `map/positions.txt` writes
//! them. The camera's height is in the same unit, above the map's base plane
//! (docs/research/camera.md).
use super::{map_manager, with_state, GResult, World, WorldSession};
use crate::{
    memory::Memory,
    objects::{
        camera::{self as layout, camera as fields, grid},
        campaign::globals,
    },
    points::{GameFunction, Point},
};
use eu4_harmony_remote::{camera::args, ProvinceId};

/// The map manager looking at a province (rcx the map manager, rdx the
/// province), at its map data's position, keeping the camera's height.
const LOOK_AT_PROVINCE: GameFunction<2> = GameFunction::at(0x80fe20);
/// The map manager looking at a map position (rcx the map manager, edx x,
/// r8d y), keeping the camera's height; where the map wraps, it takes the
/// copy of the position nearest the camera.
const LOOK_AT_POSITION: GameFunction<3> = GameFunction::at(0x80fd70);
/// The camera's goal set to look at a point from its height (rcx the camera,
/// rdx three f32: x, the height, z), kept on the map; nothing while the
/// camera is locked.
const LOOK_FROM: GameFunction<2> = GameFunction::at(0x109d140);
/// The camera put at its goal at once (rcx the camera).
const SETTLE: GameFunction<1> = GameFunction::at(0x109cd80);

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
pub fn camera(WorldSession { m, world: ctx, .. }: WorldSession) -> GResult<Camera> {
    read(m, find(m, &ctx)?.1)
}

/// Turns the camera to a province, where the game looks at it from, or to a
/// map position, keeping the height.
pub fn look_at(WorldSession { m, world: ctx, .. }: WorldSession, args::LookAt { province, x, y }: args::LookAt) -> GResult<Camera> {
    let camera = match (province, x, y) {
        (Some(ProvinceId(province)), None, None) => {
            if province == 0 {
                return Err(eu4_harmony_checks::provinces::not_found(0).into());
            }
            let (manager, camera) = movable(m, &ctx)?;
            let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
            LOOK_AT_PROVINCE.call(m, [manager, address])?;
            camera
        }
        (None, Some(x), Some(y)) => {
            checks::position(x, y, map_size(m)?)?;
            let (manager, camera) = movable(m, &ctx)?;
            LOOK_AT_POSITION.call(m, [manager, x as usize, y as usize])?;
            camera
        }
        _ => return super::fail(super::Status::InvalidArgument, "look_at takes a province, or x and y"),
    };
    settle(m, camera)
}

/// Raises or lowers the camera to a height, looking at the same position.
pub fn zoom(WorldSession { m, world: ctx, .. }: WorldSession, args::Zoom { height }: args::Zoom) -> GResult<Camera> {
    checks::height(height, limits(m, find(m, &ctx)?.1)?)?;
    let (_, camera) = movable(m, &ctx)?;
    let [x, _, z] = point(m, camera + fields::GOAL_LOOK_AT)?;
    let goal = [x, height as f32, z];
    LOOK_FROM.call(m, [camera, goal.as_ptr() as usize])?;
    settle(m, camera)
}

/// The map manager and its camera, checked by its class.
fn find(m: &Memory, ctx: &World) -> GResult<(usize, usize)> {
    let manager = map_manager(m, ctx)?;
    let camera = m.ptr(manager + layout::IN_MAP_MANAGER)?;
    if camera == 0 || m.ptr(camera)? != m.at(layout::CLASS) {
        return Err("unexpected camera".to_string().into());
    }
    Ok((manager, camera))
}

/// The map manager and its camera, which must not be locked.
fn movable(m: &Memory, ctx: &World) -> GResult<(usize, usize)> {
    let (manager, camera) = find(m, ctx)?;
    if m.u8(camera + fields::LOCKED)? != 0 {
        return Err(checks::locked());
    }
    Ok((manager, camera))
}

/// Puts the camera at its goal at once, and reads it there.
fn settle(m: &Memory, camera: usize) -> GResult<Camera> {
    SETTLE.call(m, [camera])?;
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

/// The refusals the framework words itself of the camera's arguments.
mod checks {
    use crate::game::{Fail, GResult, Status};

    /// Refuses a map position off a map `(width, height)` pixels large.
    pub fn position(x: u32, y: u32, (width, height): (u32, u32)) -> GResult<()> {
        if x >= width || y >= height {
            return Err(Fail::new(Status::InvalidArgument, format!("{x}, {y} is off the map, which is {width} pixels across and {height} up")));
        }
        Ok(())
    }

    /// Refuses a height outside the camera's `(lowest, highest)`.
    pub fn height(height: u32, (lowest, highest): (u32, u32)) -> GResult<()> {
        if !(lowest..=highest).contains(&height) {
            return Err(Fail::new(Status::InvalidArgument, format!("the height is from {lowest} to {highest}")));
        }
        Ok(())
    }

    /// The refusal of a move while the game holds the camera, asked after the
    /// move's arguments are checked.
    pub fn locked() -> Fail {
        Fail::new(Status::Rejected, "the camera is locked")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_position_is_on_the_map_and_a_height_between_the_camera_s() {
            let map = (5632, 2048);
            assert!(position(0, 0, map).is_ok() && position(5631, 2047, map).is_ok());
            let off = position(5632, 10, map).unwrap_err();
            assert_eq!((off.status, off.message.as_str()), (Status::InvalidArgument, "5632, 10 is off the map, which is 5632 pixels across and 2048 up"));
            assert!(position(10, 2048, map).is_err());
            assert!(height(50, (50, 3000)).is_ok() && height(3000, (50, 3000)).is_ok());
            assert_eq!(height(3001, (50, 3000)).unwrap_err().message, "the height is from 50 to 3000");
            assert_eq!(locked().status, Status::Rejected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;
    use crate::game::{buffers::Image, buffers::Object, buffers::Provinces, commands::Commands, player::Player, Caller, Fail, Status, View};
    use crate::objects::campaign::{classes, world};

    /// Where a camera looks, `(x, height, z)`, written as the game keeps it.
    fn put(camera: usize, at: usize, [x, y, z]: [f32; 3]) {
        for (i, value) in [x, y, z].into_iter().enumerate() {
            unsafe { *((camera + at + 4 * i) as *mut u32) = value.to_bits() };
        }
    }

    /// The map manager turning the camera to a province: its position the
    /// province's number across, 0 up.
    extern "C" fn look_at_province(manager: usize, province: usize) -> usize {
        let id = unsafe { *((province + crate::objects::campaign::province::ID) as *const u32) };
        put(unsafe { *((manager + layout::IN_MAP_MANAGER) as *const usize) }, fields::LOOK_AT, [id as f32, 0.0, 0.0]);
        0
    }

    extern "C" fn look_at_position(manager: usize, x: usize, y: usize) -> usize {
        put(unsafe { *((manager + layout::IN_MAP_MANAGER) as *const usize) }, fields::LOOK_AT, [x as f32, 0.0, y as f32]);
        0
    }

    /// The camera's goal set: at once its height, for a test.
    extern "C" fn look_from(camera: usize, goal: *const [f32; 3]) -> usize {
        put(camera, fields::POSITION, unsafe { *goal });
        0
    }

    /// A map 4 pixels across and 2 up, its lower row none, 1, 2 and none, and
    /// its upper row none; the camera looking at 2, 1 from 500, between 100
    /// and 1000, locked or not.
    struct Map {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: WorldSession,
    }

    fn map(locked: bool) -> Map {
        let image = Image::new();
        let m = image.memory();
        image
            .function(LOOK_AT_PROVINCE.rva(), look_at_province as *const ())
            .function(LOOK_AT_POSITION.rva(), look_at_position as *const ())
            .function(LOOK_FROM.rva(), look_from as *const ())
            .function(SETTLE.rva(), crate::game::buffers::no as *const ());
        let provinces = Provinces::new(&image, 3);
        let ids = Object::new(16).u32(0, 0x0001_0000).u32(4, 0x0000_0002);
        provinces.map().set(grid::IDS, &(ids.at() as u64).to_le_bytes());
        provinces.map().set(grid::WIDTH, &4i32.to_le_bytes());
        provinces.map().set(grid::HEIGHT, &2i32.to_le_bytes());
        let camera = Object::new(fields::WRAPS + 8).u64(0, m.at(layout::CLASS) as u64).i32(fields::MIN_HEIGHT, 100).i32(fields::MAX_HEIGHT, 1000).u8(fields::LOCKED, locked.into());
        put(camera.at(), fields::LOOK_AT, [2.0, 0.0, 1.0]);
        put(camera.at(), fields::POSITION, [2.0, 500.0, 1.0]);
        put(camera.at(), fields::GOAL_LOOK_AT, [2.0, 0.0, 1.0]);
        let manager = Object::new(layout::IN_MAP_MANAGER + 8).u64(0, m.at(classes::MAP_MANAGER) as u64).u64(layout::IN_MAP_MANAGER, camera.at() as u64);
        provinces.world_object().set(world::MAP_MANAGER, &(manager.at() as u64).to_le_bytes());
        let world = World { world: provinces.world(), date: 0, countries: 0, player: Player::Plays(tur()) };
        let session = WorldSession { m, world, view: View::Full, caller: Caller::Remote, commands: Commands::Recorded };
        Map { _image: image, _provinces: provinces, _objects: vec![ids, camera, manager], session }
    }

    fn failed(result: GResult<Camera>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was done") };
        (status, message)
    }

    #[test]
    fn the_camera_says_where_it_looks_from_how_high_and_at_which_province() {
        let camera = camera(map(false).session).unwrap();
        assert_eq!(camera, Camera { x: 2, y: 1, height: 500, province: None, min_height: 100, max_height: 1000 });
    }

    #[test]
    fn the_camera_turns_to_a_province_or_a_position_and_to_a_height() {
        let map = map(false);
        let at = look_at(map.session, args::LookAt { province: Some(ProvinceId(1)), x: None, y: None }).unwrap();
        assert_eq!((at.x, at.y, at.province), (1, 0, Some(1)));
        let at = look_at(map.session, args::LookAt { province: None, x: Some(2), y: Some(0) }).unwrap();
        assert_eq!((at.x, at.y, at.province, at.height), (2, 0, Some(2), 500));
        let high = zoom(map.session, args::Zoom { height: 900 }).unwrap();
        assert_eq!(high.height, 900);
    }

    #[test]
    fn a_move_with_wrong_arguments_or_while_the_camera_is_locked_is_refused() {
        let free = map(false);
        let both = args::LookAt { province: Some(ProvinceId(1)), x: Some(1), y: Some(1) };
        assert_eq!(failed(look_at(free.session, both)), (Status::InvalidArgument, "look_at takes a province, or x and y".into()));
        let off = args::LookAt { province: None, x: Some(4), y: Some(0) };
        assert_eq!(failed(look_at(free.session, off.clone())).1, "4, 0 is off the map, which is 4 pixels across and 2 up");
        assert_eq!(failed(zoom(free.session, args::Zoom { height: 50 })), (Status::InvalidArgument, "the height is from 100 to 1000".into()));
        assert_eq!(failed(look_at(free.session, args::LookAt { province: Some(ProvinceId(0)), x: None, y: None })).0, Status::NotFound);
        drop(free);
        // The arguments are checked before the lock is asked.
        let locked = map(true);
        assert_eq!(failed(look_at(locked.session, off)).0, Status::InvalidArgument);
        assert_eq!(failed(zoom(locked.session, args::Zoom { height: 900 })), (Status::Rejected, "the camera is locked".into()));
        assert_eq!(failed(look_at(locked.session, args::LookAt { province: Some(ProvinceId(1)), x: None, y: None })).1, "the camera is locked");
    }

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
