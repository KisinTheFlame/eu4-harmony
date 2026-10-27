//! The Unicode module (GLOSSARY, ADR 0010): the game keeps, draws and takes
//! input of text as UTF-8. Its dependency points install with the rest of the
//! framework, all or none (ADR 0012).
pub mod atlas;
pub mod breaks;
pub mod encoding;
pub mod font_load;
pub mod glyphs;
pub mod legacy;
pub mod localisation;
pub mod main_text;
pub mod scalar;
pub mod system_font;

use crate::points::Point;

pub fn points() -> Vec<Point> {
    let mut points = font_load::points();
    points.extend(localisation::points());
    points.extend(main_text::points());
    points
}

/// Logs what the module did while the game loaded, once, from the main menu.
pub fn log_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let (kept, legacy, left) = localisation::counts();
        let (loaded, duplicates) = font_load::loaded();
        log!("unicode: {kept} localisation entries kept ({legacy} with legacy escapes, {left} left to the game); {loaded} font glyphs above 0xFF ({duplicates} duplicates)");
    });
}
