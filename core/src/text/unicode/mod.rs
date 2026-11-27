//! The Unicode module (GLOSSARY, ADR built-in-unicode-module): the game keeps,
//! draws and takes input of text as UTF-8. Its dependency points install with
//! the rest of the framework, all or none
//! (ADR framework-succeeds-or-fails-as-a-whole).
pub(crate) mod atlas;
mod breaks;
mod buttons;
mod costs;
mod distance;
mod dxt;
mod edit_box;
mod fill;
mod fitted_text;
mod floating_text;
mod font_load;
mod framework_fonts;
pub(crate) use crate::native::game_text;
mod glyphs;
mod ingest;
mod input;
mod main_text;
mod map_field;
mod map_names;
mod outline;
mod pages;
mod pixels;
mod strings;
pub(crate) mod system_font;
mod text_height;
mod text_loops;
mod text_width;
pub(crate) mod tone;

use crate::native::{physfs::File, points::Point, splice::Plan};

/// Framework font files mounted into the game's file system at startup.
pub(crate) fn files() -> Vec<File> {
    framework_fonts::files()
}

/// Adds Unicode module splices (framework fonts and map distance-field shader)
/// to the startup file-splice plan.
pub(crate) fn plan(plan: &mut Plan) {
    framework_fonts::plan(plan);
    map_field::plan(plan);
}

/// Grows any font pages that asked for room, called on the main thread before Present.
pub(crate) fn grow_pages() {
    fill::grow_pages();
}

/// Registers a callback invoked when a font page replaces `*wrapper` directly.
pub(crate) fn set_on_texture_replaced(f: fn(usize, usize)) {
    fill::set_on_texture_replaced(f);
}

/// Logs Unicode text-rendering timing costs when the reporting interval elapses.
pub(crate) fn log_costs_when_due() {
    costs::log_when_due();
}

/// Forces every font page to request growth on the next main-thread Present (dev switch).
#[cfg(feature = "dev")]
pub(crate) fn want_room_everywhere() {
    fill::want_room_everywhere();
}

pub fn points() -> Vec<Point> {
    let mut points = font_load::points();
    points.extend(fill::points());
    points.extend(ingest::points());
    points.extend(main_text::points());
    points.extend(fitted_text::points());
    points.extend(buttons::points());
    points.extend(floating_text::points());
    points.extend(text_height::points());
    points.extend(strings::points());
    points.extend(map_names::points());
    points.extend(map_field::points());
    points.extend(text_width::points());
    points.extend(input::points());
    points.extend(edit_box::points());
    points.extend(pixels::points());
    points
}

/// Logs what the module did while the game loaded, once, from the main menu.
pub fn log_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let (kept, legacy, left) = ingest::localisation_counts();
        let (loaded, duplicates) = font_load::loaded();
        let (sets, shared) = glyphs::shared_sets();
        log!("unicode: {kept} localisation entries kept ({legacy} with legacy escapes, {left} left to the game); {loaded} font glyphs above 0xFF ({duplicates} duplicates), kept as {sets} shared sets of {shared} in all; {} script and save tokens had legacy escapes", ingest::decoded_tokens());
        costs::log_now();
        log!("memory at the main menu: {}", crate::base::heap::summary());
    });
}
