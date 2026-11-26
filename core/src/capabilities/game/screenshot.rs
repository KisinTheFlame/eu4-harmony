//! Capability group wrapper for screenshots; the D3D9 GPU frame readback and
//! PNG encoder live in `crate::render::screenshot`.
pub use crate::render::screenshot::*;

pub fn points() -> Vec<crate::native::points::Point> {
    Vec::new()
}
