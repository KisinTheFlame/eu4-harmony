//! Turns the discovered mods' UI declarations into per-file splice text and
//! the list of elements the UI layer binds at runtime.
use crate::{hosts, mods::DiscoveredMod, pdx, Result};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    Button,
    Text,
    Icon,
    /// Spliced but not bound (windows, backgrounds of other types).
    Other,
}

impl ElementKind {
    fn from_key(key: &str) -> Self {
        match key.to_ascii_lowercase().as_str() {
            "guibuttontype" => Self::Button,
            "instanttextboxtype" => Self::Text,
            "icontype" => Self::Icon,
            _ => Self::Other,
        }
    }
}

/// A spliced element as the runtime knows it.
#[derive(Clone, Debug)]
pub struct Element {
    /// Index of the owning mod in discovery order.
    pub owner: usize,
    /// The vanilla `windowType` it was spliced into.
    pub window: String,
    /// What binds it there.
    pub host: crate::hosts::Host,
    pub name: String,
    pub prefixed: String,
    pub kind: ElementKind,
}

/// A change to one game file.
#[derive(Clone, Debug, PartialEq)]
pub enum Addition {
    /// Text inserted before the closing brace of the named `windowType`.
    Window { window: String, text: String },
    /// Text appended to the end of the file.
    Append(String),
}

#[derive(Default)]
pub struct Plan {
    /// normalized file path -> additions, applied in order
    by_file: HashMap<String, Vec<Addition>>,
    pub elements: Vec<Element>,
    pub slots: usize,
}

impl Plan {
    pub fn for_file(&self, normalized: &str) -> Option<Vec<Addition>> {
        self.by_file.get(normalized).cloned()
    }

    pub fn add(&mut self, normalized: &str, addition: Addition) {
        self.by_file.entry(normalized.to_string()).or_default().push(addition);
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
        }
    }
    Ok(text)
}

/// Element-name prefix for a mod: `eu4harmony_<id>_` with non-identifier characters replaced.
pub fn prefix(id: &str) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    format!("eu4harmony_{safe}_")
}

/// `include(i)` decides whether mod `i` takes part (its extension loaded, or it has none).
pub fn build(mods: &[DiscoveredMod], include: impl Fn(usize) -> bool) -> (Plan, Vec<String>) {
    let mut plan = Plan::default();
    let mut notes = Vec::new();
    // Button-area slots taken, by window.
    let mut slots: HashMap<&str, i32> = HashMap::new();
    for (index, m) in mods.iter().enumerate() {
        if !include(index) {
            continue;
        }
        let prefix = prefix(&m.manifest.id);
        for ui in &m.manifest.ui {
            let Some(window) = hosts::window(&ui.into) else {
                notes.push(format!("{}: window {:?} is not supported", m.manifest.id, ui.into));
                continue;
            };
            let path = m.dir.join("eu4-harmony").join(&ui.file);
            let fragment = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    notes.push(format!("{}: cannot read {}: {e}", m.manifest.id, path.display()));
                    continue;
                }
            };
            let rendered: Result<pdx::Rendered> = match ui.area.as_deref() {
                Some("buttons") => match window.buttons {
                    Some(area) => pdx::render_fragment(&fragment, &prefix, Some(area.slot(slots.get(window.name).copied().unwrap_or(0)))),
                    None => Err(format!("window {:?} has no button area", window.name)),
                },
                None => pdx::render_fragment(&fragment, &prefix, None),
                Some(other) => Err(format!("unknown area {other:?}")),
            };
            let rendered = match rendered {
                Ok(r) => r,
                Err(e) => {
                    notes.push(format!("{}: {}: {e}", m.manifest.id, ui.file));
                    continue;
                }
            };
            if let Some(clash) = rendered.elements.iter().find(|e| plan.elements.iter().any(|p| p.owner == index && p.name == e.name)) {
                notes.push(format!("{}: {}: element {:?} already declared", m.manifest.id, ui.file, clash.name));
                continue;
            }
            if ui.area.is_some() {
                *slots.entry(window.name).or_default() += 1;
            }
            for e in rendered.elements {
                plan.elements.push(Element {
                    owner: index,
                    window: ui.into.clone(),
                    host: window.host,
                    prefixed: format!("{prefix}{}", e.name),
                    name: e.name,
                    kind: ElementKind::from_key(&e.key),
                });
            }
            plan.add(window.file, Addition::Window { window: ui.into.clone(), text: rendered.text });
        }
    }
    plan.slots = slots.values().sum::<i32>() as usize;
    (plan, notes)
}

#[cfg(test)]
mod tests {
    use super::{apply, prefix, Addition};

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
    }
}
