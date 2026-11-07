//! The map camera.
use super::Convert;
use crate::game::{self};
use eu4_harmony_remote::{
    camera::{Camera, CameraAnswer, CameraRequest},
    Code, Error, ProvinceId,
};

layer! {
    run CameraRequest => CameraAnswer;
    Get => fn camera() -> game::camera::Camera = world game::camera::camera;
    fn look_at_province(province: u32) -> game::camera::Camera = world game::camera::look_at_province;
    fn look_at(x: u32, y: u32) -> game::camera::Camera = world game::camera::look_at;
    LookAt { province, x, y } => look(game, province, x, y);
    Zoom => fn zoom(height: u32) -> game::camera::Camera = world game::camera::zoom;
}

/// Looks at a province, or at a point of the map.
fn look(game: &mut (impl Layer + ?Sized), province: Option<ProvinceId>, x: Option<u32>, y: Option<u32>) -> Result<game::camera::Camera, Error> {
    match (province, x, y) {
        (Some(province), None, None) => Ok(game.look_at_province(province.0)?),
        (None, Some(x), Some(y)) => Ok(game.look_at(x, y)?),
        _ => Err(Error::new(Code::InvalidArgument, "look_at takes a province, or x and y")),
    }
}

impl Convert<Camera> for game::camera::Camera {
    fn convert(self) -> Camera {
        Camera { x: self.x, y: self.y, height: self.height, province: self.province.map(ProvinceId), min_height: self.min_height, max_height: self.max_height }
    }
}
