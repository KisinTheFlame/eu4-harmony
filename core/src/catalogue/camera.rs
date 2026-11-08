//! The map camera.
use super::Convert;
use crate::game::{self};
use eu4_harmony_remote::{
    camera::{Camera, CameraAnswer, CameraRequest},
    ProvinceId,
};
use eu4_harmony_remote::camera::args;

layer! {
    run CameraRequest => CameraAnswer;
    Get => fn camera -> game::camera::Camera = game::camera::camera;
    LookAt => fn look_at -> game::camera::Camera = game::camera::look_at;
    Zoom => fn zoom -> game::camera::Camera = game::camera::zoom;
}

impl Convert<Camera> for game::camera::Camera {
    fn convert(self) -> Camera {
        Camera { x: self.x, y: self.y, height: self.height, province: self.province.map(ProvinceId), min_height: self.min_height, max_height: self.max_height }
    }
}
