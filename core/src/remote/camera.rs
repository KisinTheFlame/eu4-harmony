//! The map camera.
use super::Live;
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    camera::{Camera, CameraAnswer, CameraRequest},
    Error, ProvinceId,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn camera(&mut self) -> GResult<game::camera::Camera>;
    fn look_at_province(&mut self, province: u32) -> GResult<game::camera::Camera>;
    fn look_at(&mut self, x: u32, y: u32) -> GResult<game::camera::Camera>;
    fn zoom(&mut self, height: u32) -> GResult<game::camera::Camera>;
}

impl Layer for Live {
    fn camera(&mut self) -> GResult<game::camera::Camera> {
        game::camera::camera(Caller::Remote)
    }

    fn look_at_province(&mut self, province: u32) -> GResult<game::camera::Camera> {
        game::camera::look_at_province(Caller::Remote, province)
    }

    fn look_at(&mut self, x: u32, y: u32) -> GResult<game::camera::Camera> {
        game::camera::look_at(Caller::Remote, x, y)
    }

    fn zoom(&mut self, height: u32) -> GResult<game::camera::Camera> {
        game::camera::zoom(Caller::Remote, height)
    }
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
