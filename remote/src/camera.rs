//! The map camera: where the view looks and how high it is, and moving it.
//!
//! A map position is a pixel of the province map (`map/provinces.bmp`,
//! 5632 by 2048 in the unmodded game): `x` from its left edge, `y` from its
//! bottom edge, as `map/positions.txt` writes them. A height is in the same
//! unit, above the map's base plane; the higher the camera, the more of the
//! map the view shows. Every answer says which province is at the position
//! the camera looks at, so looking at a province gives its position and
//! looking at a position gives its province.
use crate::ProvinceId;
use serde::{Deserialize, Serialize};

group! {
    /// The map camera.
    "camera": CameraRequest, CameraAnswer;

    /// Where the camera looks and how high it is.
    world Get "get" {} -> Camera, "the camera: x and y, the map position it looks at; height; province, the province there (null where the map has none); min_height and max_height";
    /// Turns the camera to a province, at the position the game itself
    /// looks at it from, or to a map position, keeping the height: a
    /// province, or x and y. At once, the next frame showing it, or gliding
    /// there over some frames as the game's own moves do.
    world LookAt "look_at" {
        /// The province.
        province: Option<ProvinceId>,
        /// Pixels from the map's left edge.
        x: Option<u32>,
        /// Pixels from the map's bottom edge.
        y: Option<u32>,
        /// Whether the camera glides there, as the game's own moves do;
        /// false when left out, at once.
        glide: Option<bool>,
    } -> Camera, "the camera afterwards, as camera.get answers it; where it is going when it glides";
    /// Raises or lowers the camera, looking at the same position. At once:
    /// the next frame shows it.
    world Zoom "zoom" {
        /// The height above the map, in pixels of the map, from the camera's
        /// min_height to its max_height.
        height: u32,
    } -> Camera, "the camera afterwards, as camera.get answers it";
}

/// Where the camera looks and how high it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Camera {
    /// The map position at the middle of the view, where the line of sight
    /// meets the map's base plane: pixels from the left edge, and from the
    /// bottom edge.
    pub x: u32,
    pub y: u32,
    /// The camera's height above the map's base plane, in pixels of the map.
    pub height: u32,
    /// The province at that position; none where there is none.
    pub province: Option<ProvinceId>,
    /// The lowest and the highest the camera may be (the defines
    /// `NFrontend.CAMERA_MIN_HEIGHT` and `CAMERA_MAX_HEIGHT`); over high
    /// ground the game keeps it higher than the lowest.
    pub min_height: u32,
    pub max_height: u32,
}
