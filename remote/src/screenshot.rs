//! The frame the player sees, as an image. Remote control only.
use crate::Binary;
use serde::{Deserialize, Serialize};

group! {
    /// The frame the player sees, as an image.
    "screenshot": ScreenshotRequest, ScreenshotAnswer;

    /// The next frame the game shows, as the player sees it, the framework
    /// UI included, as a PNG: in the main menu and in a game. The game
    /// shows no frame of its own while a save loads, and the answer then
    /// waits for the first one after the load.
    anywhere Take "" {
        /// The image's size in percent of the frame's, 1 to 100; absent for
        /// 100.
        scale: Option<u32>,
    } -> Screenshot, "the PNG as bytes, its width and height, and the frame's width and height, in pixels";
}

/// A frame, as an image.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screenshot {
    /// The image, `image/png`.
    pub image: Binary,
    /// The image's size in pixels.
    pub width: u32,
    pub height: u32,
    /// The size in pixels of the frame it was taken from, which a pixel of
    /// the image scales to.
    pub frame_width: u32,
    pub frame_height: u32,
}
