//! The map camera.
use crate::game::{self};
use eu4_harmony_remote::{
    camera::{Camera, CameraAnswer, CameraRequest},
    Error, ProvinceId,
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
        CameraRequest::LookAtProvince { province } => CameraAnswer::LookAtProvince(camera(game.look_at_province(province.0)?)),
        CameraRequest::LookAt { x, y } => CameraAnswer::LookAt(camera(game.look_at(x, y)?)),
        CameraRequest::Zoom { height } => CameraAnswer::Zoom(camera(game.zoom(height)?)),
    })
}
