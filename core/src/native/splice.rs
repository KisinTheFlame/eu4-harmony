//! Turns the discovered mods' UI declarations into per-file splice text and
//! the list of elements the UI layer binds at runtime.
use crate::{base::hosts, base::mods::DiscoveredMod, base::pdx, base::Result};
pub use eu4_harmony_manifest::ui::{prefix, ElementKind};
use std::collections::HashMap;

/// A spliced element as the runtime knows it.
#[derive(Clone, Debug)]
pub struct Element {
    /// Index of the owning mod in discovery order.
    pub owner: usize,
    /// The vanilla `windowType` it was spliced into.
    pub window: String,
    /// What binds it there.
    pub host: crate::base::hosts::Host,
    pub name: String,
    pub prefixed: String,
    pub kind: ElementKind,
    /// Whether it is in its window's button area.
    pub in_area: bool,
}

/// The name of the frame the framework puts under a window's button area,
/// and of the button that is its background.
pub const FRAME: &str = "eu4h_button_area";
pub const FRAME_BACKGROUND: &str = "eu4h_button_area_background";

/// A change to one game file.
#[derive(Clone, Debug)]
pub enum Addition {
    /// Text inserted before the closing brace of the named `windowType`.
    Window { window: String, text: String },
    /// Text appended to the end of the file.
    Append(String),
    /// What `edit` makes of the file; `what` names it when it fails.
    Edit { what: &'static str, edit: fn(&str) -> Result<String> },
}

#[derive(Default)]
pub struct Plan {
    /// normalized file path -> additions, applied in order
    by_file: HashMap<String, Vec<Addition>>,
    /// Additions to every file whose normalized path starts and ends so and
    /// whose bytes it is for, applied after those to the file itself.
    everywhere: Vec<Everywhere>,
    pub elements: Vec<Element>,
    pub slots: usize,
    /// The windows whose button area has a frame ([`FRAME`]).
    pub frames: Vec<hosts::Host>,
}

/// An addition to every file of some paths that it is for.
struct Everywhere {
    start: &'static str,
    end: &'static str,
    /// Whether the file's bytes are for it.
    wants: fn(&[u8]) -> bool,
    addition: Addition,
}

impl Everywhere {
    fn has_path(&self, normalized: &str) -> bool {
        normalized.starts_with(self.start) && normalized.ends_with(self.end)
    }
}

impl Plan {
    /// Whether the file may have additions, which its bytes then decide
    /// ([`Plan::for_file`]).
    pub fn may_add_to(&self, normalized: &str) -> bool {
        self.by_file.contains_key(normalized) || self.everywhere.iter().any(|e| e.has_path(normalized))
    }

    /// The additions to the file whose bytes are `data`, in order.
    pub fn for_file(&self, normalized: &str, data: &[u8]) -> Vec<Addition> {
        let mut additions = self.by_file.get(normalized).cloned().unwrap_or_default();
        additions.extend(self.everywhere.iter().filter(|e| e.has_path(normalized) && (e.wants)(data)).map(|e| e.addition.clone()));
        additions
    }

    /// Adds `addition` to every file whose normalized path starts with
    /// `start` and ends with `end` and whose bytes `wants`; a file it does
    /// not want is never decoded for it.
    pub fn add_to_every(&mut self, start: &'static str, end: &'static str, wants: fn(&[u8]) -> bool, addition: Addition) {
        self.everywhere.push(Everywhere { start, end, wants, addition });
    }

    pub fn add(&mut self, normalized: &str, addition: Addition) {
        self.by_file.entry(normalized.to_string()).or_default().push(addition);
    }

    /// Adds `addition` before every other addition to the file.
    fn add_first(&mut self, normalized: &str, addition: Addition) {
        self.by_file.entry(normalized.to_string()).or_default().insert(0, addition);
    }
}

/// Applies `additions` to the text of a game file.
pub fn apply(text: &str, additions: &[Addition]) -> Result<String> {
    let mut text = text.to_string();
    for addition in additions {
        match addition {
            Addition::Window { window, text: added } => text = pdx::splice_window(&text, window, added)?,
            Addition::Append(added) => {
                text.push('\n');
                text.push_str(added);
            }
            Addition::Edit { what, edit } => text = edit(&text).map_err(|e| format!("{what}: {e}"))?,
        }
    }
    Ok(text)
}

/// `include(i)` decides whether mod `i` takes part (its extension loaded, or it has none).
pub fn build(mods: &[DiscoveredMod], include: impl Fn(usize) -> bool, plan: &mut Plan) -> Vec<String> {
    let mut notes = Vec::new();
    // Button-area slots taken, by window.
    let mut slots: HashMap<&'static str, i32> = HashMap::new();
    for (index, m) in mods.iter().enumerate() {
        if !include(index) {
            continue;
        }
        let report = eu4_harmony_manifest::ui::read_fragments(&m.dir, &m.manifest.id, &m.layout.ui, &mut slots);
        for problem in report.problems {
            let detail = problem.strip_prefix("eu4-harmony/").unwrap_or(&problem);
            notes.push(format!("{}: {detail}", m.manifest.id));
        }
        for e in report.elements {
            plan.elements.push(Element { owner: index, window: e.window, host: e.host, name: e.name, prefixed: e.prefixed, kind: e.kind, in_area: e.in_area });
        }
        for fragment in report.fragments {
            plan.add(fragment.window_file, Addition::Window { window: fragment.window_name, text: fragment.text });
        }
    }
    // Each button area's frame, first in its window so that it is drawn
    // under every element spliced there.
    for window in hosts::WINDOWS {
        let (Some(area), Some(&count)) = (window.buttons, slots.get(window.name)) else { continue };
        plan.add_first(window.file, Addition::Window { window: window.name.into(), text: frame(&area, count) });
        plan.frames.push(window.host);
    }
    plan.slots = slots.values().sum::<i32>() as usize;
    notes
}

/// Sprite name for non-moveable framework window frames and extension windows.
pub const FIXED_BACKGROUND_SPRITE: &str = "GFX_eu4harmony_fixed_window_background";

/// The frame under `count` slots of `area`: a window of its own in the
/// framework's window background, as the extension windows are, which the
/// pointer does not light up and whose clicks do not reach the map. A button
/// would light up; an icon is drawn at its sprite's size, not stretched.
fn frame(area: &hosts::ButtonArea, count: i32) -> String {
    let ((x, y), (w, h)) = area.frame(count);
    format!(
        "\n\t\twindowType = {{ name = \"{FRAME}\" backGround = \"{FRAME_BACKGROUND}\" position = {{ x = {x} y = {y} }} size = {{ x = {w} y = {h} }} \
         moveable = 0 dontRender = \"\" horizontalBorder = \"\" verticalBorder = \"\" fullScreen = no Orientation = \"UPPER_LEFT\"\n\
         \t\t\tguiButtonType = {{ name = \"{FRAME_BACKGROUND}\" quadTextureSprite = \"{FIXED_BACKGROUND_SPRITE}\" }}\n\t\t}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::{apply, prefix, Addition, Plan};

    #[test]
    fn additions_to_every_file_follow_the_file_s_own() {
        let mut plan = Plan::default();
        plan.add("interface/core.gui", Addition::Append("own".into()));
        plan.add_to_every("interface/", ".gui", |data| data.starts_with(b"wanted"), Addition::Append("every".into()));
        let appended = |path, data: &[u8]| {
            plan.for_file(path, data).iter().map(|a| if let Addition::Append(t) = a { t.clone() } else { String::new() }).collect::<Vec<_>>()
        };
        assert_eq!(appended("interface/core.gui", b"wanted"), ["own", "every"]);
        assert_eq!(appended("interface/core.gui", b"other"), ["own"]);
        assert_eq!(appended("interface/topbar.gui", b"wanted"), ["every"]);
        assert!(appended("interface/topbar.gui", b"\xff other").is_empty());
        assert!(plan.may_add_to("interface/topbar.gui") && plan.may_add_to("interface/core.gui"));
        assert!(!plan.may_add_to("interface/core.gfx") && !plan.may_add_to("common/x.gui"));
    }

    #[test]
    fn a_button_area_s_frame_comes_first_and_holds_its_slots() {
        let area = crate::base::hosts::window("single_unitpanel").unwrap().buttons.unwrap();
        let mut plan = Plan::default();
        plan.add("interface/unitpanel_main.gui", Addition::Window { window: "single_unitpanel".into(), text: " BUTTON ".into() });
        plan.add_first("interface/unitpanel_main.gui", Addition::Window { window: "single_unitpanel".into(), text: super::frame(&area, 10) });
        let file = "guiTypes = { windowType = { name = \"single_unitpanel\" } }";
        let out = apply(file, &plan.for_file("interface/unitpanel_main.gui", file.as_bytes())).unwrap();
        let (frame, button) = (out.find("eu4h_button_area").unwrap(), out.find("BUTTON").unwrap());
        assert!(frame < button, "{out}");
        assert!(out.contains("position = { x = 504 y = 69 } size = { x = 116 y = 392 }"), "two columns: {out}");
        assert!(
            out.contains("windowType = { name = \"eu4h_button_area\" backGround = \"eu4h_button_area_background\""),
            "a window, which no pointer lights up: {out}"
        );
        assert_eq!(out.matches('{').count(), out.matches('}').count());
    }

    #[test]
    fn prefix_is_identifier_safe() {
        assert_eq!(prefix("regional-combat"), "eu4harmony_regional_combat_");
        assert_eq!(prefix("Hello.World"), "eu4harmony_hello_world_");
    }

    #[test]
    fn appends_follow_window_splices_in_order() {
        let file = "guiTypes = { windowType = { name = \"w\" } }";
        let out = apply(file, &[Addition::Append("spriteTypes = {}".into()), Addition::Window { window: "w".into(), text: " X ".into() }]).unwrap();
        assert!(out.ends_with("\nspriteTypes = {}"));
        assert!(out.contains(" X }"));
        let edited = apply(file, &[Addition::Edit { what: "upper", edit: |t| Ok(t.to_uppercase()) }]).unwrap();
        assert!(edited.starts_with("GUITYPES"));
        let failed = apply(file, &[Addition::Edit { what: "nothing", edit: |_| Err("no".into()) }]);
        assert_eq!(failed.unwrap_err(), "nothing: no");
    }
}
