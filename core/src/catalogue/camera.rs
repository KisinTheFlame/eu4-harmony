//! The map camera.
use eu4_harmony_remote::camera::args;
use eu4_harmony_remote::camera::{Camera, CameraAnswer, CameraRequest};

layer! {
    run CameraRequest => CameraAnswer;
    Get => fn camera -> Camera = crate::game::camera::camera;
    LookAt => fn look_at -> Camera = crate::game::camera::look_at;
    Zoom => fn zoom -> Camera = crate::game::camera::zoom;
}
