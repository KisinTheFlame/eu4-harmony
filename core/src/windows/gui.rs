//! The game's interface definitions of extension windows and pop-ups: a
//! `windowType` written out of a laid out window ([`window`]), or of a
//! pop-up's words ([`popup`]), made of the game's own sprites in the look
//! of its dialogs. Pure, so that what is written is tested without the game.
//!
//! Every element is named so that [`super::live`] finds it: [`BACKGROUND`],
//! [`TITLE`], [`CLOSE`], and each game widget `w_<name>` (`w_<n>` for one
//! with no name, by its place in the layout); a list's row is a window type
//! of its own, `<window>_<list>`.
use eu4_harmony_lua::{
    layout::{self, Align, Font, Kind, Placed, Ribbon, Window},
    Popup,
};
use std::fmt::Write as _;

pub const BACKGROUND: &str = "eu4h_background";
pub const TITLE: &str = "eu4h_title";
pub const CLOSE: &str = "eu4h_close";
/// A pop-up's text.
pub const TEXT: &str = "eu4h_text";

/// The background that stretches to a window's size: the framework's own
/// sprite ([`SPRITES`]) of the game's blue panel, the religion view's, its
/// gold edge kept at every size.
pub const BACKGROUND_SPRITE: &str = "GFX_eu4harmony_window_background";

/// The framework's sprites the definitions use, a `.gfx` the game reads
/// at startup among the framework files.
pub const SPRITES: &str = "spriteTypes = {
	corneredTileSpriteType = {
		name = \"GFX_eu4harmony_window_background\"
		size = { x = 531 y = 310 }
		textureFile = \"gfx/interface/country_religion_view_allreligion_bg_large.dds\"
		borderSize = { x = 12 y = 12 }
	}
	corneredTileSpriteType = {
		name = \"GFX_eu4harmony_ribbon_red\"
		size = { x = 321 y = 42 }
		textureFile = \"gfx/interface/band_reorg_red.dds\"
		borderSize = { x = 32 y = 1 }
	}
	corneredTileSpriteType = {
		name = \"GFX_eu4harmony_ribbon_blue\"
		size = { x = 321 y = 42 }
		textureFile = \"gfx/interface/band_reorg_blue.dds\"
		borderSize = { x = 32 y = 1 }
	}
}
";

/// A title's ribbon: its window, where it stands and how high it is; the
/// ribbon runs from one side of the window to the other, but this.
pub const RIBBON: &str = "eu4h_ribbon";
const RIBBON_TOP: i32 = 14;
const RIBBON_HEIGHT: i32 = 42;
const RIBBON_INSET: i32 = 14;

/// A pop-up's parts, as the game's message pop-up has them
/// (`DefaultPopup` in `interface/eu3dialog.gui`): its small header line,
/// its title on the green band, its shields, its buttons.
pub const HEADER: &str = "eu4h_header";
pub const SHIELDS: [&str; 2] = ["eu4h_shield_left", "eu4h_shield_right"];
/// A pop-up's buttons' names, left to right.
pub const POPUP_BUTTONS: [&str; 2] = ["eu4h_button_1", "eu4h_button_2"];
/// The key of the game's own words for its OK, a pop-up's button when the
/// mod gives none.
pub const OK: &str = "OK";
const POPUP_BACKGROUND: &str = "SandboxFloaterBackground";

/// A pop-up's width, the game's message pop-up's; its height follows its
/// text, as the game's does, from the least the game's has.
pub const POPUP_WIDTH: i32 = 500;
const POPUP_LEAST: i32 = 262;
const POPUP_MOST: i32 = 480;
/// Where its text begins, how wide it runs, how high a line of it is, and
/// how far its buttons stand above its bottom.
const POPUP_TEXT_TOP: i32 = 84;
const POPUP_TEXT_WIDTH: i32 = 440;
const POPUP_LINE: i32 = 20;
const POPUP_BUTTONS_ABOVE: i32 = 19 + 31;

/// A pop-up's height for its text: the lines it takes, a full-width
/// character such as a Chinese one about twice a letter, under its buttons.
pub fn popup_height(popup: &Popup) -> i32 {
    let per_line = 60.0;
    let lines: f64 = popup
        .text
        .split('\n')
        .map(|paragraph| {
            let width: f64 = paragraph.chars().map(|c| if (c as u32) < 0x2e80 { 1.0 } else { 2.0 }).sum();
            (width / per_line).ceil().max(1.0)
        })
        .sum();
    let height = POPUP_TEXT_TOP + (lines as i32) * POPUP_LINE + 24 + POPUP_BUTTONS_ABOVE;
    height.clamp(POPUP_LEAST, POPUP_MOST)
}

/// The name of the element a game widget is, `index` its place among its
/// window's or its row's.
pub fn element(placed: &Placed, index: usize) -> String {
    match &placed.name {
        Some(name) => format!("w_{name}"),
        None => format!("w_{index}"),
    }
}

/// The name of the window type a list's rows are built from.
pub fn row_type(window_type: &str, list: &str) -> String {
    format!("{window_type}_{list}")
}

/// Words as a `.gui` string holds them: no quotes or line breaks, which the
/// game's script reading would take for the string's end.
fn quoted(words: &str) -> String {
    words
        .chars()
        .map(|c| {
            if c == '"' {
                '\''
            } else if c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// The `windowType` of an extension window, and one for each of its lists'
/// rows; `words` puts a key of the mod's into the player language.
pub fn window(type_name: &str, window: &Window, words: &dyn Fn(&Option<eu4_harmony_lua::Text>) -> String) -> String {
    let mut out = String::new();
    let (w, h) = window.size;
    open_window(&mut out, type_name, (w, h), window.moveable, Some(BACKGROUND_SPRITE));
    let close_top = h - layout::CLOSE_ABOVE - layout::BUTTON_HEIGHT;
    // The title on its ribbon, a window of its own the ribbon stretches to.
    if window.title.is_some() {
        let ribbon = match window.ribbon {
            Ribbon::Red => "GFX_eu4harmony_ribbon_red",
            Ribbon::Blue => "GFX_eu4harmony_ribbon_blue",
        };
        let width = w - 2 * RIBBON_INSET;
        let mut band = String::new();
        open_window(&mut band, RIBBON, (width, RIBBON_HEIGHT), false, Some(ribbon));
        out.push_str(&band.replacen("position = { x = 0 y = 0 }", &format!("position = {{ x = {RIBBON_INSET} y = {RIBBON_TOP} }}"), 1));
        let side = 24;
        text_box(&mut out, TITLE, (side, 7), (width - 2 * side, 28), "vic_22", "centre", &words(&window.title));
        out.push_str("\t}\n");
    }
    // The game's long Close button, centered along the bottom, in the
    // game's words, which Esc presses.
    if window.close {
        let _ = writeln!(
            out,
            "\t\tguiButtonType = {{ name = \"{CLOSE}\" position = {{ x = {} y = {close_top} }} quadTextureSprite = \"button_type_1\" buttonText = \"CLOSE\" buttonFont = \"vic_18\" shortcut = \"ESCAPE\" clicksound = close_window }}",
            (w - 149) / 2
        );
    }
    let mut rows = String::new();
    for (index, placed) in window.widgets.iter().enumerate() {
        widget(&mut out, placed, index, words);
        if let (Kind::List { row, .. }, Some(name)) = (&placed.kind, &placed.name) {
            open_window(&mut rows, &row_type(type_name, name), row.size, false, None);
            for (i, p) in row.widgets.iter().enumerate() {
                widget(&mut rows, p, i, words);
            }
            rows.push_str("\t}\n");
        }
    }
    out.push_str("\t}\n");
    out.push_str(&rows);
    out
}

/// The `windowType` of a pop-up, as the game's message pop-up is laid out:
/// its header, its title on the green band, the shields at its corners,
/// its text, and its buttons along its bottom. Its words are written into it, its text alone set as it is
/// built, for the game's formatting and line breaks.
pub fn popup(type_name: &str, popup: &Popup) -> String {
    let mut out = String::new();
    let height = popup_height(popup);
    open_window(&mut out, type_name, (POPUP_WIDTH, height), true, Some(POPUP_BACKGROUND));
    let _ = writeln!(out, "\t\ticonType = {{ name = \"eu4h_band\" spriteType = \"GFX_message_band\" position = {{ x = -15 y = 18 }} }}");
    if let Some(header) = &popup.header {
        text_box(&mut out, HEADER, (59, 19), (380, 20), "vic_18", "centre", header);
    }
    text_box(&mut out, TITLE, (59, 41), (380, 24), "vic_22", "centre", &popup.title);
    let _ = writeln!(
        out,
        "\t\tinstantTextBoxType = {{ name = \"{TEXT}\" position = {{ x = {} y = {POPUP_TEXT_TOP} }} font = \"vic_18\" borderSize = {{ x = 0 y = 0 }} text = \"\" \
         maxWidth = {POPUP_TEXT_WIDTH} maxHeight = {} format = centre scrollbartype = \"standardtext_slider\" }}",
        (POPUP_WIDTH - POPUP_TEXT_WIDTH) / 2,
        height - POPUP_TEXT_TOP - POPUP_BUTTONS_ABOVE - 12
    );
    for (name, x) in SHIELDS.iter().zip([-1, 407]) {
        let _ = writeln!(out, "\t\tguiButtonType = {{ name = \"{name}\" position = {{ x = {x} y = 30 }} quadTextureSprite = \"standarddialogshield\" }}");
    }
    // The game's dialog's places: two at its sides, or one in the middle.
    // Its keys go with the places, as the game's dialogs have them: c and
    // Return the right one, or the only one; z and Esc the left one.
    let shown = shown(popup);
    let places: &[i32] = if shown.len() == 1 { &[175] } else { &[77, 274] };
    for (i, ((name, text), x)) in shown.iter().zip(places).enumerate() {
        let keys = if i + 1 == shown.len() {
            " shortcut = \"c\" extra_shortcut = \"RETURN\" clicksound = ok_click"
        } else {
            " shortcut = \"z\" extra_shortcut = \"ESCAPE\""
        };
        let _ = writeln!(
            out,
            "\t\tguiButtonType = {{ name = \"{name}\" position = {{ x = {x} y = {} }} quadTextureSprite = \"largefloaterbutton\" buttonText = \"{}\" buttonFont = \"vic_18\"{keys} }}",
            height - POPUP_BUTTONS_ABOVE,
            quoted(text)
        );
    }
    out.push_str("\t}\n");
    out
}

/// A pop-up's buttons as they stand, left to right: each its element's
/// name and its words, or the key of the game's own OK, which the game puts
/// into its language, when the mod gave none.
pub fn shown(popup: &Popup) -> Vec<(String, String)> {
    POPUP_BUTTONS.iter().zip(popup.shown_buttons(OK)).map(|(name, words)| ((*name).to_string(), words)).collect()
}

/// The button, from 1, of a pop-up's element `name`.
pub fn popup_button(name: &str) -> Option<u32> {
    POPUP_BUTTONS.iter().position(|b| *b == name).map(|i| i as u32 + 1)
}

/// A definitions file of window types.
pub fn file(types: &[String]) -> String {
    let mut out = String::from("guiTypes = {\n");
    for t in types {
        out.push_str(t);
    }
    out.push_str("}\n");
    out
}

/// Opens a `windowType`; one with a background has it stretched to its
/// size, as the game's dialogs do: a button of their sprite, which the
/// window's `backGround` names.
fn open_window(out: &mut String, name: &str, (w, h): (i32, i32), moveable: bool, background: Option<&str>) {
    let _ = writeln!(out, "\twindowType = {{");
    let _ = writeln!(out, "\t\tname = \"{name}\"");
    let _ = writeln!(out, "\t\tbackGround = \"{}\"", if background.is_some() { BACKGROUND } else { "" });
    let _ = writeln!(out, "\t\tposition = {{ x = 0 y = 0 }}");
    let _ = writeln!(out, "\t\tsize = {{ x = {w} y = {h} }}");
    let _ = writeln!(out, "\t\tmoveable = {}", u8::from(moveable));
    let _ = writeln!(out, "\t\tdontRender = \"\"");
    let _ = writeln!(out, "\t\thorizontalBorder = \"\"");
    let _ = writeln!(out, "\t\tverticalBorder = \"\"");
    let _ = writeln!(out, "\t\tfullScreen = no");
    let _ = writeln!(out, "\t\tOrientation = \"UPPER_LEFT\"");
    // A title's ribbon only shows: clicks pass through it, so that it is
    // never pressed.
    if let Some(sprite) = background {
        let shows_only = if name == RIBBON { " alwaystransparent = yes" } else { "" };
        let _ = writeln!(out, "\t\tguiButtonType = {{ name = \"{BACKGROUND}\" quadTextureSprite = \"{sprite}\"{shows_only} }}");
    }
}

fn text_box(out: &mut String, name: &str, (x, y): (i32, i32), (w, h): (i32, i32), font: &str, format: &str, words: &str) {
    let _ = writeln!(
        out,
        "\t\tinstantTextBoxType = {{ name = \"{name}\" position = {{ x = {x} y = {y} }} font = \"{font}\" borderSize = {{ x = 0 y = 0 }} text = \"{}\" \
         maxWidth = {w} maxHeight = {h} fixedsize = yes format = {format} }}",
        quoted(words)
    );
}

/// The game's text button sprite `width` wide ([`layout::BUTTON_WIDTHS`]).
fn button_sprite(width: i32) -> &'static str {
    match width {
        79 => "button_type_6",
        109 => "button_type_9",
        189 => "button_type_8",
        224 => "button_type_4",
        _ => "button_type_1",
    }
}

fn widget(out: &mut String, placed: &Placed, index: usize, words: &dyn Fn(&Option<eu4_harmony_lua::Text>) -> String) {
    let name = element(placed, index);
    let (x, y) = (placed.x, placed.y);
    match &placed.kind {
        Kind::Text { text, font, align } => {
            let font = match font {
                Font::Body => "vic_18",
                Font::Heading => "vic_22",
            };
            let format = match align {
                Align::Start => "left",
                Align::Center => "centre",
                Align::End => "right",
            };
            text_box(out, &name, (x, y), (placed.width, placed.height), font, format, &words(text));
        }
        Kind::Icon { sprite, frame } => {
            let _ =
                writeln!(out, "\t\ticonType = {{ name = \"{name}\" position = {{ x = {x} y = {y} }} spriteType = \"{}\" frame = {frame} }}", quoted(sprite));
        }
        Kind::Button { text: Some(text), .. } => {
            let _ = writeln!(
                out,
                "\t\tguiButtonType = {{ name = \"{name}\" position = {{ x = {x} y = {y} }} quadTextureSprite = \"{}\" buttonText = \"{}\" buttonFont = \"vic_18\" }}",
                button_sprite(placed.width),
                quoted(&words(&Some(text.clone())))
            );
        }
        Kind::Button { sprite, frame, .. } => {
            let sprite = sprite.as_deref().unwrap_or_default();
            let _ = writeln!(
                out,
                "\t\tguiButtonType = {{ name = \"{name}\" position = {{ x = {x} y = {y} }} quadTextureSprite = \"{}\" frame = {frame} }}",
                quoted(sprite)
            );
        }
        Kind::Progress { sprite, .. } => {
            let _ = writeln!(out, "\t\ticonType = {{ name = \"{name}\" position = {{ x = {x} y = {y} }} spriteType = \"{}\" }}", quoted(sprite));
        }
        Kind::Flag { sprite, .. } => {
            let _ = writeln!(out, "\t\tguiButtonType = {{ name = \"{name}\" position = {{ x = {x} y = {y} }} quadTextureSprite = \"{}\" }}", quoted(sprite));
        }
        Kind::List { spacing, .. } => {
            let _ = writeln!(
                out,
                "\t\tlistboxType = {{ name = \"{name}\" position = {{ x = {x} y = {y} }} size = {{ x = {} y = {} }} spacing = {spacing} \
                 borderSize = {{ x = 0 y = 0 }} scrollbartype = \"standardlistbox_slider\" }}",
                placed.width, placed.height
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_window_is_the_game_s_window_type_with_its_rows_beside_it() {
        let layouts = json!({ "reforms": {
            "size": [400, 360], "title": "TITLE",
            "items": [
                { "type": "text", "name": "status", "size": [200, 40], "text": "STATUS", "align": "center" },
                { "type": "button", "name": "go", "text": "GO", "size": [100, 31] },
                { "type": "icon", "sprite": "GFX_seal", "size": [32, 32] },
                { "type": "list", "name": "rows", "size": [300, 100], "row": { "size": [280, 30], "items": [
                    { "type": "text", "name": "name", "size": [100, 20] },
                ]}},
            ],
        }});
        let window = &layout::read(&layouts, &|_| true).unwrap()[0];
        let text = window_text(window);
        assert!(text.starts_with("\twindowType = {\n\t\tname = \"eu4harmony_m_reforms\"\n\t\tbackGround = \"eu4h_background\"\n"), "{text}");
        assert!(text.contains("size = { x = 400 y = 360 }") && text.contains("moveable = 1"));
        assert!(text.contains(&format!("backGround = \"{BACKGROUND}\"")));
        assert!(text.contains(&format!("guiButtonType = {{ name = \"{BACKGROUND}\" quadTextureSprite = \"GFX_eu4harmony_window_background\" }}")));
        assert!(text.contains("name = \"eu4h_title\"") && text.contains("text = \"[TITLE]\""), "{text}");
        assert!(
            text.contains("name = \"eu4h_close\" position = { x = 125 y = 315 } quadTextureSprite = \"button_type_1\" buttonText = \"CLOSE\""),
            "the game's long Close, centered at the bottom: {text}"
        );
        assert!(text.contains("shortcut = \"ESCAPE\""));
        assert!(text.contains("name = \"w_status\" position = { x = 24 y = 66 }") && text.contains("format = centre"), "{text}");
        assert!(
            text.contains("name = \"eu4h_ribbon\"") && text.contains("position = { x = 14 y = 14 }") && text.contains("GFX_eu4harmony_ribbon_red"),
            "the title on its ribbon: {text}"
        );
        assert!(!text.contains("gfx_footer_decoration") && !text.contains("gfx_header_decoration"), "no gold line, no crest");
        assert!(text.contains("name = \"w_go\"") && text.contains("quadTextureSprite = \"button_type_6\" buttonText = \"[GO]\""), "{text}");
        assert!(text.contains("iconType = { name = \"w_2\""), "an unnamed game widget by its place");
        assert!(text.contains("listboxType = { name = \"w_rows\""));
        assert!(text.contains("\twindowType = {\n\t\tname = \"eu4harmony_m_reforms_rows\"\n\t\tbackGround = \"\"\n"), "its row's type beside it");
        assert_eq!(text.matches('{').count(), text.matches('}').count());
    }

    fn window_text(window: &Window) -> String {
        super::window("eu4harmony_m_reforms", window, &|k| k.as_ref().map(|text| text.resolve(|key| Some(format!("[{key}]")))).unwrap_or_default())
    }

    #[test]
    fn a_pop_up_is_the_game_s_message_pop_up_its_keys_going_with_its_buttons_places() {
        let mut popup = Popup { header: Some("Our steward".into()), title: "T".into(), text: "X".into(), buttons: Vec::new(), shields: [None, None] };
        let alone = super::popup("eu4h_popup_1", &popup);
        assert!(alone.contains("quadTextureSprite = \"SandboxFloaterBackground\""), "the game's dialogs' black");
        assert!(alone.contains("spriteType = \"GFX_message_band\"") && alone.contains("text = \"Our steward\""), "{alone}");
        assert_eq!(alone.matches("standarddialogshield").count(), 2);
        assert!(
            alone.contains("name = \"eu4h_button_1\" position = { x = 175 y = 212 }") && alone.contains("buttonText = \"OK\""),
            "the game's own OK in the middle: {alone}"
        );
        assert!(
            alone.contains("shortcut = \"c\" extra_shortcut = \"RETURN\"") && !alone.contains("ESCAPE"),
            "one button: c and Return, as the game's: {alone}"
        );
        popup.buttons = vec!["Look".into(), "No \"never\"".into()];
        let two = super::popup("eu4h_popup_2", &popup);
        assert!(
            two.contains("name = \"eu4h_button_1\" position = { x = 77 y = 212 }")
                && two.contains("buttonText = \"Look\" buttonFont = \"vic_18\" shortcut = \"z\" extra_shortcut = \"ESCAPE\""),
            "{two}"
        );
        assert!(
            two.contains("name = \"eu4h_button_2\" position = { x = 274 y = 212 }")
                && two.contains("buttonText = \"No 'never'\" buttonFont = \"vic_18\" shortcut = \"c\" extra_shortcut = \"RETURN\""),
            "{two}"
        );
        assert_eq!((popup_button("eu4h_button_2"), popup_button("eu4h_close")), (Some(2), None));
        let whole = file(&[two]);
        assert_eq!(whole.matches('{').count(), whole.matches('}').count());
    }
}
