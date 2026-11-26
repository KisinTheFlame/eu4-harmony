//! Capability group wrapper for remote input; the virtual cursor and Win32
//! input playback engine lives in `crate::render::input`.
pub use crate::render::input::*;

pub fn points() -> Vec<crate::native::points::Point> {
    Vec::new()
}
