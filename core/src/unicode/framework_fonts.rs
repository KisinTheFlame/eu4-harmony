//! Framework fonts (GLOSSARY, ADR framework-files-in-the-game-file-system):
//! fonts the framework defines in its own files, for text of the game's that
//! draws in a system font of its own, such as the game console's. Each is a
//! bitmap font as the game's are: a definition in a `.gfx` file, a `.fnt` file
//! with its lines and no characters, and a page, all among the framework's
//! files (`crate::files`). So it has a page of its own, which `fill` draws in
//! the system font of its [`Kind`], whatever the game's and the mods' fonts
//! are.
//!
//! The windows that draw in a framework font say so in [`FONTS`]: as the game
//! reads any `.gui` file under `interface/`, every `font` in those windows
//! becomes the framework font, once the font has its page of system glyphs.
//! The `.gfx` files, and with them the fonts, load before the `.gui` files;
//! a window whose font has no such page, on a device `fill` cannot fill, keeps
//! the game's font, which draws its own glyphs.
use super::fill::Kind;
use crate::files::File;
use crate::pdx;
use crate::splice::{Addition, Plan};
use crate::Result;
use std::collections::HashSet;
use std::sync::Mutex;

pub struct FrameworkFont {
    /// Its name in the `.gfx` files, and the name of its files.
    pub name: &'static str,
    /// What it draws, which picks its system font.
    pub kind: Kind,
    /// How tall its lines are and where its baseline lies, in GUI units.
    line: u32,
    base: u32,
    /// The game's windows whose text draws in it.
    windows: &'static [&'static str],
}

pub const FONTS: &[FrameworkFont] = &[FrameworkFont {
    name: "eu4harmony_console",
    kind: Kind::Console,
    // As the game's `Arial12`, which the console's 16-unit lines were made
    // for.
    line: 14,
    base: 11,
    // The console and each of its lines.
    windows: &["console_wnd", "console_entry_wnd"],
}];

/// The colors that `§` codes pick in text of the framework fonts, as the
/// game's own fonts have them.
const TEXT_COLORS: &str = "G = { 44 171 50 } R = { 181 61 61 } Y = { 219 203 71 }";

/// The `.gfx` file that defines the framework fonts.
const DEFINITIONS: &str = "interface/eu4harmony_fonts.gfx";

/// The framework fonts that have a page of system glyphs.
static DRAWN: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);

/// The framework font named `name`.
pub fn named(name: &[u8]) -> Option<&'static FrameworkFont> {
    FONTS.iter().find(|f| f.name.as_bytes() == name)
}

/// `font` has its page of system glyphs.
pub fn drawn(font: &FrameworkFont) {
    DRAWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).insert(font.name);
}

fn is_drawn(font: &FrameworkFont) -> bool {
    DRAWN.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|d| d.contains(font.name))
}

/// The framework fonts' files: their definitions, and for each its `.fnt`
/// file and its page, a small empty one that `fill` replaces.
pub fn files() -> Vec<File> {
    let mut definitions = String::from("bitmapfonts = {\n");
    let mut files = Vec::new();
    for font in FONTS {
        let path = format!("gfx/fonts/{}", font.name);
        definitions.push_str(&format!(
            "\tbitmapfont = {{\n\t\tname = \"{}\"\n\t\tpath = \"{path}\"\n\t\tcolor = 0xffffffff\n\t\ttextcolors = {{ {TEXT_COLORS} }}\n\t}}\n",
            font.name
        ));
        let fnt = format!(
            "info face=\"{0}\" size={1} bold=0 italic=0 charset=\"ANSI\" stretchH=100 smooth=0 aa=1 padding=0,0,0,0 spacing=0,0\ncommon lineHeight={1} base={2} scaleW={3} scaleH={3} pages=1\n",
            font.name, font.line, font.base, PAGE
        );
        files.push(File { path: format!("{path}.fnt"), data: fnt.into_bytes() });
        files.push(File { path: format!("{path}.tga"), data: page() });
    }
    definitions.push_str("}\n");
    files.push(File { path: DEFINITIONS.to_string(), data: definitions.into_bytes() });
    files
}

/// How wide and tall a framework font's own page is, in pixels.
const PAGE: u16 = 8;

/// An empty 32-bit TGA page.
fn page() -> Vec<u8> {
    let mut tga = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    tga.extend(PAGE.to_le_bytes());
    tga.extend(PAGE.to_le_bytes());
    // 32 bits a pixel, 8 of them alpha.
    tga.extend([32, 8]);
    tga.resize(tga.len() + PAGE as usize * PAGE as usize * 4, 0);
    tga
}

pub fn plan(plan: &mut Plan) {
    plan.add_to_every("interface/", ".gui", mentions_a_window, Addition::Edit { what: "the windows that draw in framework fonts", edit: use_fonts });
}

/// Whether a file's bytes name one of the windows of [`FONTS`].
fn mentions_a_window(data: &[u8]) -> bool {
    FONTS.iter().flat_map(|f| f.windows).any(|w| data.windows(w.len()).any(|bytes| bytes == w.as_bytes()))
}

/// A `.gui` file with the windows of [`FONTS`] that it defines in their
/// framework font; one that defines none of them, as it was.
fn use_fonts(text: &str) -> Result<String> {
    in_fonts(text, is_drawn)
}

fn in_fonts(text: &str, drawn: impl Fn(&FrameworkFont) -> bool) -> Result<String> {
    let mut text = text.to_string();
    for font in FONTS {
        for &window in font.windows {
            if !text.contains(window) || pdx::find_window(&text, &pdx::tokenize(&text), window).is_none() {
                continue;
            }
            if !drawn(font) {
                log!("framework fonts: {} has no page of system glyphs; {window} keeps its fonts", font.name);
                continue;
            }
            text = pdx::set_in_window(&text, window, "font", font.name)?;
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{files, in_fonts, mentions_a_window, FONTS};
    use crate::pdx;

    #[test]
    fn each_font_is_defined_with_its_files() {
        let files = files();
        let definitions = files.iter().find(|f| f.path == "interface/eu4harmony_fonts.gfx").unwrap();
        let text = std::str::from_utf8(&definitions.data).unwrap();
        let tokens = pdx::tokenize(text);
        assert_eq!(pdx::child_blocks(&tokens, 0, tokens.len()).len(), 1);
        for font in FONTS {
            assert!(text.contains(&format!("name = \"{}\"\n\t\tpath = \"gfx/fonts/{}\"", font.name, font.name)));
            let fnt = files.iter().find(|f| f.path == format!("gfx/fonts/{}.fnt", font.name)).unwrap();
            assert!(std::str::from_utf8(&fnt.data)
                .unwrap()
                .contains(&format!("\ncommon lineHeight={} base={} scaleW=8 scaleH=8 pages=1\n", font.line, font.base)));
            let tga = files.iter().find(|f| f.path == format!("gfx/fonts/{}.tga", font.name)).unwrap();
            assert_eq!(tga.data.len(), 18 + 8 * 8 * 4);
            assert_eq!(&tga.data[12..18], &[8, 0, 8, 0, 32, 8]);
        }
    }

    const GUI: &str = "guiTypes = {\n\twindowType = { name = \"console_wnd\" editBoxType = { name = \"console_edit\" font = \"Arial12\" } }\n\twindowType = { name = \"console_entry_wnd\" instantTextBoxType = { font = \"Arial12\" } }\n\ttextBoxType = { name = \"ChatMemberText\" font = \"Main_14_eu\" }\n}\n";

    #[test]
    fn the_console_s_windows_draw_in_its_font_once_it_is_drawn() {
        let out = in_fonts(GUI, |_| true).unwrap();
        assert_eq!(out.matches("font = \"eu4harmony_console\"").count(), 2);
        assert!(out.contains("font = \"Main_14_eu\""));
        assert_eq!(in_fonts(GUI, |_| false).unwrap(), GUI);
        // Files without the windows are left as they are, and so is a
        // mention of one that is not its window.
        let other = "guiTypes = { windowType = { name = \"topbar\" font = \"vic_18\" tooltip = \"console_wnd\" } }";
        assert_eq!(in_fonts(other, |_| true).unwrap(), other);
        // A window of a framework font without a font fails the file, which
        // the game then reads as it was.
        assert!(in_fonts("windowType = { name = \"console_wnd\" }", |_| true).is_err());
        assert!(mentions_a_window(GUI.as_bytes()) && mentions_a_window(b"\xff name = \"console_entry_wnd\""));
        assert!(!mentions_a_window(b"\xff windowType = { name = \"end_of_combat\" }"));
    }
}
