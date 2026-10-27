//! A button in the army panel's button area that logs its clicks and shows
//! itself only while an own land army is selected. Its tooltip comes from
//! `eu4-harmony/localisation/`, in the player's language.
use eu4_harmony_sdk::{export_extension, log, Click, Extension, Game};

#[derive(Default)]
struct Hello {
    clicks: u32,
}

impl Extension for Hello {
    fn on_session(&mut self, game: &mut Game) {
        let tooltip = game.text("HELLO_TOOLTIP");
        let _ = game.set_tooltip("hello", &tooltip);
        log!("session started: {:?}, language {:?}", game.session(), game.language());
    }

    fn on_frame(&mut self, game: &mut Game) {
        let shown = matches!(game.selected_army(), Ok(Some(_)));
        let _ = game.set_visible("hello", shown);
    }

    fn on_button(&mut self, game: &mut Game, name: &str, click: Click) {
        self.clicks += 1;
        let army = game.selected_army().ok().flatten();
        log!("{name} clicked ({click:?}), click #{}, selected army {army:?}", self.clicks);
    }
}

export_extension!(Hello::default());

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_sdk::{testing::FakeGame, ArmyId};

    #[test]
    fn button_follows_selection() {
        let mut fake = FakeGame::new();
        let mut hello = Hello::default();
        hello.on_frame(&mut fake.game());
        assert_eq!(fake.element("hello").visible, Some(false));
        fake.selected = Some(ArmyId(7));
        hello.on_frame(&mut fake.game());
        assert_eq!(fake.element("hello").visible, Some(true));
    }

    #[test]
    fn tooltip_is_in_the_player_language() {
        let mut fake = FakeGame::new();
        fake.load_localisation(concat!(env!("CARGO_MANIFEST_DIR"), "/.."), "simp_chinese").unwrap();
        Hello::default().on_session(&mut fake.game());
        assert_eq!(fake.element("hello").tooltip.as_deref(), Some("§Y你好§!\n左键或右键点击，在日志里写一行。"));
    }
}
