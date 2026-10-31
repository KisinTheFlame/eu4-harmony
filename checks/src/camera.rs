//! The map camera.
use crate::{Checked, Refusal, Status};

/// Refuses a map position off a map `(width, height)` pixels large.
pub fn position(x: u32, y: u32, (width, height): (u32, u32)) -> Checked {
    if x >= width || y >= height {
        return Err(Refusal::new(Status::InvalidArgument, format!("{x}, {y} is off the map, which is {width} pixels across and {height} up")));
    }
    Ok(())
}

/// Refuses a height outside the camera's `(lowest, highest)`.
pub fn height(height: u32, (lowest, highest): (u32, u32)) -> Checked {
    if !(lowest..=highest).contains(&height) {
        return Err(Refusal::new(Status::InvalidArgument, format!("the height is from {lowest} to {highest}")));
    }
    Ok(())
}

/// The refusal of a move while the game holds the camera, asked after the
/// move's arguments are checked.
pub fn locked() -> Refusal {
    Refusal::new(Status::Rejected, "the camera is locked")
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
