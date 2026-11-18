//! Desired state of interface elements extensions splice into vanilla host
//! windows (`game.ui.<element>`), retaining semantic [`crate::Text`]
//! descriptions so language changes re-resolve dynamically and tests can verify
//! element declarations and button pressability.
use crate::{Element, Text};
use std::collections::BTreeMap;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct HostedElement {
    pub visible: Option<bool>,
    pub enabled: Option<bool>,
    pub text: Option<Text>,
    pub tooltip: Text,
    pub tooltip_set: bool,
}

#[derive(Clone, Default, Debug)]
pub struct HostedElements {
    strict_default: bool,
    /// Declared elements per extension (`name -> is_button`).
    /// When present for an extension (or when `strict_default` is true),
    /// undeclared elements are refused with the same error as the running game.
    declared: BTreeMap<usize, BTreeMap<String, bool>>,
    by_key: BTreeMap<(usize, String), HostedElement>,
}

impl HostedElements {
    /// Enforce declared-element checks for every extension, even before any
    /// element is declared (used by the running game).
    pub fn strict() -> Self {
        Self { strict_default: true, ..Self::default() }
    }

    /// Registers the elements spliced for `extension`, each as `(name, is_button)`.
    pub fn declare(&mut self, extension: usize, elements: impl IntoIterator<Item = (String, bool)>) {
        self.declared.insert(extension, elements.into_iter().collect());
    }

    pub fn clear(&mut self) {
        self.declared.clear();
        self.by_key.clear();
    }

    fn is_declared(&self, extension: usize, element: &str) -> Result<bool, String> {
        if let Some(elements) = self.declared.get(&extension) {
            return elements.get(element).copied().ok_or_else(|| format!("no element {element:?} declared by this mod"));
        }
        if self.strict_default {
            return Err(format!("no element {element:?} declared by this mod"));
        }
        Ok(true)
    }

    pub fn set(&mut self, extension: usize, element: &str, value: Element) -> Result<(), String> {
        self.is_declared(extension, element)?;
        let state = self.by_key.entry((extension, element.to_string())).or_default();
        match value {
            Element::Visible(on) => state.visible = Some(on),
            Element::Enabled(on) => state.enabled = Some(on),
            Element::Text(text) => state.text = Some(text),
            Element::Tooltip(text) => {
                state.tooltip = text;
                state.tooltip_set = true;
            }
        }
        Ok(())
    }

    pub fn get(&self, extension: usize, element: &str) -> Option<&HostedElement> {
        self.by_key.get(&(extension, element.to_string()))
    }

    pub fn tooltip(&self, extension: usize, element: &str) -> Result<Text, String> {
        self.is_declared(extension, element)?;
        Ok(self.get(extension, element).map(|s| s.tooltip.clone()).unwrap_or_default())
    }

    pub fn check_press(&self, extension: usize, element: &str) -> Result<(), String> {
        if !self.is_declared(extension, element)? {
            return Err(format!("the element {element:?} is not a button"));
        }
        if let Some(state) = self.get(extension, element) {
            if state.visible == Some(false) {
                return Err(format!("the element {element:?} is hidden"));
            }
            if state.enabled == Some(false) {
                return Err(format!("the element {element:?} is disabled"));
            }
        }
        Ok(())
    }

    pub fn forget(&mut self, extension: usize) {
        self.by_key.retain(|(e, _), _| *e != extension);
    }

    pub fn hide(&mut self, extension: usize) {
        if let Some(names) = self.declared.get(&extension).map(|m| m.keys().cloned().collect::<Vec<_>>()) {
            for name in names {
                self.by_key.entry((extension, name)).or_default().visible = Some(false);
            }
        }
        for ((e, _), state) in &mut self.by_key {
            if *e == extension {
                state.visible = Some(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_elements_enforce_declarations_and_pressability() {
        let mut hosted = HostedElements::strict();
        hosted.declare(0, [("go".into(), true), ("label".into(), false)]);
        hosted.declare(1, [("other".into(), true)]);

        hosted.set(0, "go", Element::Enabled(false)).unwrap();
        assert!(hosted.set(0, "other", Element::Visible(true)).unwrap_err().contains("no element \"other\" declared"));
        assert!(hosted.set(0, "missing", Element::Visible(true)).unwrap_err().contains("no element \"missing\" declared"));
        assert!(hosted.tooltip(0, "missing").unwrap_err().contains("no element \"missing\" declared"));
        assert!(hosted.check_press(0, "label").unwrap_err().contains("is not a button"));
        assert!(hosted.check_press(0, "go").unwrap_err().contains("disabled"));

        hosted.set(0, "go", Element::Enabled(true)).unwrap();
        hosted.check_press(0, "go").unwrap();
        hosted.set(0, "go", Element::Tooltip("tip".into())).unwrap();
        assert_eq!(hosted.tooltip(0, "go").unwrap(), Text::from("tip"));

        hosted.forget(0);
        assert!(hosted.get(0, "go").is_none(), "forget clears desired state");
        hosted.check_press(0, "go").unwrap();

        hosted.hide(1);
        assert_eq!(hosted.get(1, "other").and_then(|s| s.visible), Some(false), "hide covers never-mutated declared elements");
    }
}
