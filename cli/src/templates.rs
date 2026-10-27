//! Files `eu4-harmony new` writes. `@@id@@`, `@@name@@`, `@@crate@@`,
//! `@@lib@@`, `@@key@@`, `@@sdk@@` and `@@api@@` are substituted.

pub const DESCRIPTOR: &str = r#"version="0.1.0"
tags={
	"Gameplay"
}
name="@@name@@"
supported_version="v1.37.*"
"#;

pub const MANIFEST: &str = r#"{
  "id": "@@id@@",
  "api_version": "@@api@@",
  "extension": "win64/@@lib@@.dll",
  "ui": [
    { "file": "ui/button.gui", "into": "single_unitpanel", "area": "buttons" }
  ]
}
"#;

pub const BUTTON_GUI: &str = r#"# One slot of the army panel's button area. EU4 Harmony positions it and
# prefixes the element names; refer to them by the names written here.
guiButtonType = {
	name = "button"
	quadTextureSprite = "GFX_unitpanel_button_hunt_rebels"
}
"#;

pub const LOC_ENGLISH: &str = r#"l_english:
 @@key@@_TOOLTIP:0 "§Y@@name@@§!\nClick to write a line to the log."
"#;

pub const LOC_SIMP_CHINESE: &str = r#"l_simp_chinese:
 @@key@@_TOOLTIP:0 "§Y@@name@@§!\n点击后在日志里写一行。"
"#;

pub const GITIGNORE: &str = "/extension/target/\n/eu4-harmony/win64/\n";

pub const CARGO_TOML: &str = r#"[package]
name = "@@crate@@"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "@@lib@@"
crate-type = ["cdylib"]

[dependencies]
eu4-harmony-sdk = { path = "@@sdk@@" }

[profile.release]
lto = "thin"
codegen-units = 1

# Not part of any parent workspace.
[workspace]
"#;

pub const CARGO_CONFIG: &str = r#"[build]
target = "x86_64-pc-windows-msvc"

[target.x86_64-pc-windows-msvc]
# Extensions run inside eu4.exe; a static CRT keeps the DLL self-contained.
rustflags = ["-C", "target-feature=+crt-static"]
"#;

pub const LIB_RS: &str = r#"//! @@name@@: an EU4 Harmony extension.
//!
//! Its button sits in the army panel's button area. It shows while an own
//! land army is selected and logs each click to
//! `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\@@id@@.log`.
//! Its texts are in `eu4-harmony/localisation/`, one file per language.
use eu4_harmony_sdk::{export_extension, log, Click, Extension, Game};

#[derive(Default)]
pub struct Mod {
    clicks: u32,
}

impl Extension for Mod {
    fn on_session(&mut self, game: &mut Game) {
        let tooltip = game.text("@@key@@_TOOLTIP");
        let _ = game.set_tooltip("button", &tooltip);
    }

    fn on_frame(&mut self, game: &mut Game) {
        let selected = matches!(game.selected_army(), Ok(Some(_)));
        let _ = game.set_visible("button", selected);
    }

    fn on_button(&mut self, game: &mut Game, name: &str, click: Click) {
        self.clicks += 1;
        let army = game.selected_army().ok().flatten();
        log!("{name} clicked ({click:?}), click #{}, army {army:?}", self.clicks);
    }
}

export_extension!(Mod::default());

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_sdk::{testing::FakeGame, ArmyId};

    #[test]
    fn button_shows_only_with_an_army_selected() {
        let mut fake = FakeGame::new();
        let mut m = Mod::default();
        m.on_frame(&mut fake.game());
        assert_eq!(fake.element("button").visible, Some(false));
        fake.selected = Some(ArmyId(1));
        m.on_frame(&mut fake.game());
        assert_eq!(fake.element("button").visible, Some(true));
    }

    #[test]
    fn tooltip_comes_from_the_localisation() {
        let mut fake = FakeGame::new();
        fake.load_localisation(concat!(env!("CARGO_MANIFEST_DIR"), "/.."), "simp_chinese").unwrap();
        Mod::default().on_session(&mut fake.game());
        assert!(fake.element("button").tooltip.unwrap().contains("日志"));
    }
}
"#;

pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(template.to_string(), |text, (key, value)| text.replace(&format!("@@{key}@@"), value))
}
