//! The map camera.
use crate::game::{self};
use eu4_harmony_remote::{
    camera::{Camera, CameraAnswer, CameraRequest},
    Code, Error, ProvinceId,
};

layer! {
    fn camera() -> game::camera::Camera = game::camera::camera;
    fn look_at_province(province: u32) -> game::camera::Camera = game::camera::look_at_province;
    fn look_at(x: u32, y: u32) -> game::camera::Camera = game::camera::look_at;
    fn zoom(height: u32) -> game::camera::Camera = game::camera::zoom;
}

fn camera(c: game::camera::Camera) -> Camera {
    Camera { x: c.x, y: c.y, height: c.height, province: c.province.map(ProvinceId), min_height: c.min_height, max_height: c.max_height }
}

pub fn run(request: CameraRequest, game: &mut (impl Layer + ?Sized)) -> Result<CameraAnswer, Error> {
    Ok(match request {
        CameraRequest::Get {} => CameraAnswer::Get(camera(game.camera()?)),
        CameraRequest::LookAt { province, x, y } => CameraAnswer::LookAt(camera(match (province, x, y) {
            (Some(province), None, None) => game.look_at_province(province.0)?,
            (None, Some(x), Some(y)) => game.look_at(x, y)?,
            _ => return Err(Error::new(Code::InvalidArgument, "look_at takes a province, or x and y")),
        })),
        CameraRequest::Zoom { height } => CameraAnswer::Zoom(camera(game.zoom(height)?)),
    })
}
