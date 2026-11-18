//! Interface fragment reading, host-window validation, and element declarations
//! shared across startup splicing, `eu4-harmony check`, and the fake game.
use crate::{
    hosts::{self, Host},
    pdx, UiFragment,
};
use std::{collections::HashMap, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    Button,
    Text,
    Icon,
    /// Spliced but not bound (windows, backgrounds of other types).
    Other,
}

impl ElementKind {
    pub fn from_key(key: &str) -> Self {
        match key.to_ascii_lowercase().as_str() {
            "guibuttontype" => Self::Button,
            "instanttextboxtype" => Self::Text,
            "icontype" => Self::Icon,
            _ => Self::Other,
        }
    }

    pub fn is_button(self) -> bool {
        self == Self::Button
    }
}

/// An element declared by one interface fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredElement {
    pub window: String,
    pub host: Host,
    pub name: String,
    pub prefixed: String,
    pub kind: ElementKind,
    pub in_area: bool,
}

/// One validated and rendered fragment ready to splice into a vanilla window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedFragment {
    pub window_name: String,
    pub window_file: &'static str,
    pub text: String,
}

/// Result of reading and validating all interface fragments of one mod.
#[derive(Default)]
pub struct FragmentReport {
    pub fragments: Vec<RenderedFragment>,
    pub elements: Vec<DeclaredElement>,
    pub problems: Vec<String>,
}

/// Element-name prefix for a mod: `eu4harmony_<id>_` with non-identifier characters replaced.
pub fn prefix(id: &str) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    format!("eu4harmony_{safe}_")
}

/// Reads and validates a mod's interface fragments against `hosts::WINDOWS` and
/// `.gui` syntax rules, allocating button-area slots in `slots` as each valid
/// button-area fragment is accepted.
pub fn read_fragments(root: &Path, id: &str, ui: &[UiFragment], slots: &mut HashMap<&'static str, i32>) -> FragmentReport {
    let mut report = FragmentReport::default();
    let prefix = prefix(id);
    for fragment in ui {
        let Some(window) = hosts::window(&fragment.into) else {
            report.problems.push(format!("eu4-harmony/{}: window {:?} is not supported", fragment.file, fragment.into));
            continue;
        };
        let path = root.join("eu4-harmony").join(&fragment.file);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                report.problems.push(format!("eu4-harmony/{}: cannot read {}: {e}", fragment.file, path.display()));
                continue;
            }
        };
        let rendered = match fragment.area.as_deref() {
            Some("buttons") => match window.buttons {
                Some(area) => pdx::render_fragment(&text, &prefix, Some(area.slot(slots.get(window.name).copied().unwrap_or(0)))),
                None => Err(format!("window {:?} has no button area", window.name)),
            },
            None => pdx::render_fragment(&text, &prefix, None),
            Some(other) => Err(format!("unknown area {other:?}")),
        };
        let rendered = match rendered {
            Ok(r) => r,
            Err(e) => {
                report.problems.push(format!("eu4-harmony/{}: {e}", fragment.file));
                continue;
            }
        };
        if let Some(clash) = rendered.elements.iter().find(|e| report.elements.iter().any(|p| p.name == e.name)) {
            report.problems.push(format!("eu4-harmony/{}: element {:?} already declared", fragment.file, clash.name));
            continue;
        }
        if fragment.area.is_some() {
            *slots.entry(window.name).or_default() += 1;
        }
        for e in rendered.elements {
            report.elements.push(DeclaredElement {
                window: fragment.into.clone(),
                host: window.host,
                prefixed: format!("{prefix}{}", e.name),
                name: e.name,
                kind: ElementKind::from_key(&e.key),
                in_area: fragment.area.is_some(),
            });
        }
        report.fragments.push(RenderedFragment { window_name: fragment.into.clone(), window_file: window.file, text: rendered.text });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_identifier_safe() {
        assert_eq!(prefix("regional-combat"), "eu4harmony_regional_combat_");
        assert_eq!(prefix("Hello.World"), "eu4harmony_hello_world_");
    }

    #[test]
    fn read_fragments_checks_hosts_syntax_and_duplicate_names() {
        let root = std::env::temp_dir().join(format!("eu4h-fragments-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |file: &str, text: &str| {
            let path = root.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("ui/single_unitpanel/buttons/a.gui", "guiButtonType = { name = \"go\" }\n");
        write("ui/single_unitpanel/buttons/z.gui", "instantTextBoxType = { name = \"go\" }\n");
        write("ui/single_unitpanel/bad.gui", "guiButtonType = { name = \"bad-name\" }\n");
        write("ui/topbar/x.gui", "guiButtonType = { name = \"top\" }\n");
        let layout = crate::layout(&root.join("eu4-harmony"));
        let mut slots = HashMap::new();
        let report = read_fragments(&root, "my-mod", &layout.ui, &mut slots);
        assert_eq!(report.elements.len(), 1);
        assert_eq!(report.elements[0].name, "go");
        assert_eq!(report.elements[0].kind, ElementKind::Button);
        assert_eq!(slots.get("single_unitpanel"), Some(&1));
        assert_eq!(report.problems.len(), 3, "{:?}", report.problems);
        assert!(report.problems.iter().any(|p| p.contains("element \"go\" already declared")));
        assert!(report.problems.iter().any(|p| p.contains("bad.gui: element name \"bad-name\"")));
        assert!(report.problems.iter().any(|p| p.contains("topbar/x.gui: window \"topbar\" is not supported")));
        let _ = std::fs::remove_dir_all(root);
    }
}
