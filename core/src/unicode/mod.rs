//! The Unicode module (GLOSSARY, ADR 0010): the game keeps, draws and takes
//! input of text as UTF-8. Its dependency points install with the rest of the
//! framework, all or none (ADR 0012).
pub mod atlas;
pub mod breaks;
pub mod buttons;
pub mod dxt;
pub mod edit_box;
pub mod fill;
pub mod fitted_text;
pub mod floating_text;
pub mod font_load;
pub mod game_text;
pub mod glyphs;
pub mod input;
pub mod legacy;
pub mod localisation;
pub mod main_text;
pub mod map_names;
pub mod objects;
pub mod save_names;
pub mod scripts;
pub mod search;
pub mod system_font;
pub mod text_height;
pub mod text_loops;
pub mod text_width;

use crate::points::Point;

const STRING_APPEND_BYTES: &str = "48 89 5c 24 18 55 56 57 41 55 41 56 48 83 ec 20";

pub fn points() -> Vec<Point> {
    let mut points = vec![Point::keep(text_loops::STRING_APPEND, STRING_APPEND_BYTES, "string append").recorded(&text_loops::APPEND)];
    points.extend(objects::points());
    points.extend(font_load::points());
    points.extend(fill::points());
    points.extend(localisation::points());
    points.extend(main_text::points());
    points.extend(fitted_text::points());
    points.extend(scripts::points());
    points.extend(buttons::points());
    points.extend(floating_text::points());
    points.extend(text_height::points());
    points.extend(search::points());
    points.extend(map_names::points());
    points.extend(text_width::points());
    points.extend(input::points());
    points.extend(edit_box::points());
    points.extend(save_names::points());
    points
}

/// Logs what the module did while the game loaded, once, from the main menu.
pub fn log_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let (kept, legacy, left) = localisation::counts();
        let (loaded, duplicates) = font_load::loaded();
        log!("unicode: {kept} localisation entries kept ({legacy} with legacy escapes, {left} left to the game); {loaded} font glyphs above 0xFF ({duplicates} duplicates); {} script and save tokens had legacy escapes", scripts::decoded());
    });
}
