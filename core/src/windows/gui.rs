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
    layout::{self, Align, Font, Kind, Placed, Window},
    Popup,
};
use std::fmt::Write as _;

pub const BACKGROUND: &str = "eu4h_background";
pub const TITLE: &str = "eu4h_title";
pub const CLOSE: &str = "eu4h_close";
/// A pop-up's text, and its buttons, `eu4h_button_<n>` from 1.
pub const TEXT: &str = "eu4h_text";

/// The background that stretches to a window's size: the game's dialogs'.
const BACKGROUND_SPRITE: &str = "SandboxFloaterBackground";
const CLOSE_SPRITE: &str = "GFX_closebutton2";

/// A pop-up's size, and where its texts stand.
pub const POPUP_WIDTH: i32 = 460;
pub const POPUP_HEIGHT: i32 = 300;

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

/// The localisation keys a window's definition and its first state take.
pub fn keys(window: &Window, out: &mut Vec<String>) {
    fn of_widgets(widgets: &[Placed], out: &mut Vec<String>) {
        for w in widgets {
            out.extend(w.tooltip.clone());
            match &w.kind {
                Kind::Text { text, .. } | Kind::Button { text, .. } => out.extend(text.clone()),
                Kind::List { row, .. } => of_widgets(&row.widgets, out),
                _ => {}
            }
        }
    }
    out.extend(window.title.clone());
    out.extend(window.entry.as_ref().and_then(|e| e.tooltip.clone()));
    of_widgets(&window.widgets, out);
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
pub fn window(type_name: &str, window: &Window, words: &dyn Fn(&Option<String>) -> String) -> String {
    let mut out = String::new();
    let (w, h) = window.size;
    open_window(&mut out, type_name, (w, h), window.moveable, true);
    if window.title.is_some() {
        let x = layout::MARGIN + layout::CLOSE;
        text_box(&mut out, TITLE, (x, 20), (w - 2 * x, 28), "vic_22", "centre", &words(&window.title));
    }
    if window.close {
        let _ = writeln!(
            out,
            "\t\tguiButtonType = {{ name = \"{CLOSE}\" position = {{ x = {} y = 12 }} quadTextureSprite = \"{CLOSE_SPRITE}\" shortcut = \"ESCAPE\" clicksound = close_window }}",
            w - layout::CLOSE - 12
        );
    }
    let mut rows = String::new();
    for (index, placed) in window.widgets.iter().enumerate() {
        widget(&mut out, placed, index, words);
        if let (Kind::List { row, .. }, Some(name)) = (&placed.kind, &placed.name) {
            open_window(&mut rows, &row_type(type_name, name), row.size, false, false);
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

/// The `windowType` of a pop-up, at the screen's middle: its title, its
/// text and its buttons in a row along its bottom, the last pressed by Esc.
pub fn popup(type_name: &str, popup: &Popup) -> String {
    let mut out = String::new();
    open_window(&mut out, type_name, (POPUP_WIDTH, POPUP_HEIGHT), true, true);
    let margin = layout::MARGIN;
    text_box(&mut out, TITLE, (margin, 24), (POPUP_WIDTH - 2 * margin, 28), "vic_22", "centre", "");
    let _ = writeln!(
        out,
        "\t\tinstantTextBoxType = {{ name = \"{TEXT}\" position = {{ x = {margin} y = 68 }} font = \"vic_18\" borderSize = {{ x = 0 y = 0 }} text = \"\" \
         maxWidth = {} maxHeight = 160 format = centre scrollbartype = \"standardtext_slider\" }}",
        POPUP_WIDTH - 2 * margin
    );
    let count = popup.buttons.len() as i32;
    let width = match count {
        1 | 2 => 149,
        3 => 109,
        _ => 79,
    };
    let gap = 12;
    let total = width * count + gap * (count - 1);
    let mut x = (POPUP_WIDTH - total) / 2;
    for (i, words) in popup.buttons.iter().enumerate() {
        let sprite = button_sprite(width);
        let escape = if i + 1 == popup.buttons.len() { " shortcut = \"ESCAPE\"" } else { "" };
        let _ = writeln!(
            out,
            "\t\tguiButtonType = {{ name = \"eu4h_button_{}\" position = {{ x = {x} y = {} }} quadTextureSprite = \"{sprite}\" buttonText = \"{}\" buttonFont = \"vic_18\"{escape} }}",
            i + 1,
            POPUP_HEIGHT - margin - layout::BUTTON_HEIGHT,
            quoted(words)
        );
        x += width + gap;
    }
    out.push_str("\t}\n");
    out
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
fn open_window(out: &mut String, name: &str, (w, h): (i32, i32), moveable: bool, background: bool) {
    let _ = writeln!(out, "\twindowType = {{");
    let _ = writeln!(out, "\t\tname = \"{name}\"");
    let _ = writeln!(out, "\t\tbackGround = \"{}\"", if background { BACKGROUND } else { "" });
    let _ = writeln!(out, "\t\tposition = {{ x = 0 y = 0 }}");
    let _ = writeln!(out, "\t\tsize = {{ x = {w} y = {h} }}");
    let _ = writeln!(out, "\t\tmoveable = {}", u8::from(moveable));
    let _ = writeln!(out, "\t\tdontRender = \"\"");
    let _ = writeln!(out, "\t\thorizontalBorder = \"\"");
    let _ = writeln!(out, "\t\tverticalBorder = \"\"");
    let _ = writeln!(out, "\t\tfullScreen = no");
    let _ = writeln!(out, "\t\tOrientation = \"UPPER_LEFT\"");
    if background {
        let _ = writeln!(out, "\t\tguiButtonType = {{ name = \"{BACKGROUND}\" quadTextureSprite = \"{BACKGROUND_SPRITE}\" }}");
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

fn widget(out: &mut String, placed: &Placed, index: usize, words: &dyn Fn(&Option<String>) -> String) {
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
            "size": [400, 300], "title": "TITLE",
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
        assert!(text.contains("size = { x = 400 y = 300 }") && text.contains("moveable = 1"));
        assert!(text.contains(&format!("backGround = \"{BACKGROUND}\"")));
        assert!(text.contains(&format!("guiButtonType = {{ name = \"{BACKGROUND}\" quadTextureSprite = \"SandboxFloaterBackground\" }}")));
        assert!(text.contains("name = \"eu4h_title\"") && text.contains("text = \"[TITLE]\""), "{text}");
        assert!(text.contains("name = \"eu4h_close\" position = { x = 352 y = 12 }") && text.contains("shortcut = \"ESCAPE\""));
        assert!(text.contains("name = \"w_status\" position = { x = 24 y = 56 }") && text.contains("format = centre"), "{text}");
        assert!(text.contains("name = \"w_go\"") && text.contains("quadTextureSprite = \"button_type_6\" buttonText = \"[GO]\""), "{text}");
        assert!(text.contains("iconType = { name = \"w_2\""), "an unnamed game widget by its place");
        assert!(text.contains("listboxType = { name = \"w_rows\""));
        assert!(text.contains("\twindowType = {\n\t\tname = \"eu4harmony_m_reforms_rows\"\n\t\tbackGround = \"\"\n"), "its row's type beside it");
        assert_eq!(text.matches('{').count(), text.matches('}').count());
    }

    fn window_text(window: &Window) -> String {
        super::window("eu4harmony_m_reforms", window, &|k| k.as_ref().map(|k| format!("[{k}]")).unwrap_or_default())
    }

    #[test]
    fn a_pop_up_lays_its_buttons_out_along_its_bottom_and_esc_presses_the_last() {
        let popup = Popup { id: None, title: "T".into(), text: "X".into(), buttons: vec!["Yes".into(), "No \"never\"".into(), "Later".into()] };
        let text = super::popup("eu4h_popup_1", &popup);
        assert!(text.contains("name = \"eu4h_button_1\" position = { x = 54 y = 245 } quadTextureSprite = \"button_type_9\" buttonText = \"Yes\""), "{text}");
        assert!(text.contains("buttonText = \"No 'never'\""), "a quote would end the string");
        assert_eq!(text.matches("ESCAPE").count(), 1);
        assert!(text.contains("name = \"eu4h_button_3\"") && text.trim_end().ends_with("shortcut = \"ESCAPE\" }\n\t}"), "{text}");
        let whole = file(&[text]);
        assert_eq!(whole.matches('{').count(), whole.matches('}').count());
    }
}
