//! Template Mod: an EU4 Harmony extension.
//!
//! Its button sits in the army panel's button area. It shows while an own
//! land army is selected and logs each click to
//! `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\template-mod.log`.
//! Its texts are in `eu4-harmony/localisation/`, one file per language.
use eu4_harmony_sdk::{export_extension, log, Click, Extension, Game};

#[derive(Default)]
pub struct Mod {
    clicks: u32,
}

impl Extension for Mod {
    fn on_session(&mut self, game: &mut Game) {
        let tooltip = game.text("TEMPLATE_MOD_TOOLTIP");
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
