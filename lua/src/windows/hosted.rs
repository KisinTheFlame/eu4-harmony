//! Desired state of interface elements extensions splice into vanilla host
//! windows (`game.ui.<element>`), retaining semantic [`crate::Text`]
//! descriptions so language changes re-resolve dynamically and tests can verify
//! button pressability.
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
    by_key: BTreeMap<(usize, String), HostedElement>,
}

impl HostedElements {
    pub fn set(&mut self, extension: usize, element: &str, value: Element) {
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
    }

    pub fn get(&self, extension: usize, element: &str) -> Option<&HostedElement> {
        self.by_key.get(&(extension, element.to_string()))
    }

    pub fn tooltip(&self, extension: usize, element: &str) -> Text {
        self.get(extension, element).map(|s| s.tooltip.clone()).unwrap_or_default()
    }

    pub fn check_press(&self, extension: usize, element: &str) -> Result<(), String> {
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
        for ((e, _), state) in &mut self.by_key {
            if *e == extension {
                state.visible = Some(false);
            }
        }
    }
}
