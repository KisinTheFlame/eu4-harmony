//! The main menu, remote control only (ADR remote-control-uses-capabilities):
//! where the game is, the saves, loading one, starting a new game and returning
//! to the main menu.
//!
//! Returning to the main menu starts `eu4.exe` again: the process that was
//! asked answers, then ends, and a new one takes the pipe over a few seconds
//! later. Every answer here says which process gave it.
use crate::{session::Session, CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The main menu: where the game is, the saves, loading one, starting a
    /// game and returning to the main menu.
    "menu": MenuRequest, MenuAnswer;

    /// Where the game is, and which process answers: a game started again
    /// (after returning to the main menu, say) is a new process, whose
    /// sessions and events are numbered from 1 again.
    remote_anywhere State "" {} -> Menu, "process (when the game process started, in milliseconds since 1970), and place: main_menu, starting (a game is being started from the main menu, set up or loaded, and at startup until the main menu shows) or game";
    /// The saves in the save games folder, newest first.
    remote_anywhere Saves "saves" {} -> Vec<Save>, "the saves: name (to load it by), file, bytes, modified (seconds since 1970), player and date as the save's header says them (null when it cannot be read), and missing_mods, the mods it was made with that the game did not start with (menu.load refuses it while there are any)";
    /// Loads a save and plays it as the country it was saved as: from the
    /// main menu's first page (at startup, once menu says main_menu) as the
    /// continue box does, and from a game as the game's own load does,
    /// without the main menu between. A game left so is not saved, but an
    /// ironman game is, first, unless the framework settings turn that off;
    /// rejected, the game going on, when that save fails. Answers once the
    /// game is ready, however long the save takes to load. Sent while neither
    /// the main menu nor a game's frame shows (a game being started, a frame
    /// the game is not ready in), it waits for one. Rejected, before anything
    /// happens, when the save was made with mods the game did not start
    /// with, any one of them, with no way past it: one such save crashed the
    /// game as it loaded, and which mods change the game cannot be told, so
    /// add them to the playset and start the game again; and rejected when
    /// the game does not take the load up, cannot read the save (it then
    /// starts a game from what it read), begins another game than the
    /// save's, or begins one the player cannot play. A save of observe mode
    /// loads into observe mode.
    remote_anywhere Load "load" {
        /// The save's name, as menu.saves lists it.
        save: String,
    } -> Session, "the session of the loaded game, as session answers it, its player and date the save's";
    /// Starts a new game at the default start date as a country, or in
    /// observe mode with no country given, as the single player setup and
    /// its observe button do: from the main menu's first page (at
    /// startup, once menu says main_menu), or from a game, which is left
    /// first as menu.return leaves it (an ironman game saved), when the
    /// framework settings return without restarting. Answers once the game is ready.
    /// Rejected, before anything happens, for a country with no land at the
    /// start date, which the setup does not offer the player (the game would
    /// only be observed); and rejected when the game does not take the start
    /// up or begins a game the player cannot play.
    remote_anywhere Start "start" {
        /// The country to play; none for observe mode, the AI playing every
        /// country and none in view, rejected with ironman chosen in the
        /// single player setup, which the game does not let observe.
        country: Option<CountryTag>,
    } -> Session, "the session of the new game, as session answers it";
    /// Returns to the main menu, without the game's question or its summary,
    /// as the player would; asked while time runs, the game is paused first
    /// and the session left once it is still, a few frames later, as a new
    /// game and an in-game load from a session are; an ironman game is saved
    /// first, and rejected,
    /// the game going on, when that save fails. Whenever a game is going on,
    /// also where capabilities that need a game are refused; not_in_game
    /// where none is. With the framework
    /// settings returning without restarting (the default), this process
    /// answers once its main menu is up, place main_menu. Otherwise the game
    /// returns by starting eu4.exe again, and the answer comes from this
    /// process at once, place ending; once the game starts again, every
    /// request to this process, waiting or sent, is answered not_ready (an
    /// advance waiting, main_menu). The new process opens the pipe a few
    /// seconds later and answers once it presents its first frame: ask menu
    /// again until its process differs and its place is main_menu.
    remote_anywhere Return "return" {} -> Menu, "this process, and place main_menu once its main menu is up, or ending when the game starts again";
}

impl MenuRequest {
    /// Whether it does one thing in the main menu and another in a game, so
    /// that it runs only where it is known which: loading a save, starting a
    /// game, returning to the main menu.
    pub fn needs_place(&self) -> bool {
        matches!(self, Self::Load { .. } | Self::Start { .. } | Self::Return {})
    }
}

/// Where the game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// The main menu shows, on any of its pages, and no game is being
    /// started from it.
    MainMenu,
    /// A game is being set up or loaded: started from the main menu (by a
    /// remote client or by the player), or neither the main menu nor a
    /// game's frame runs, as at startup until the main menu shows.
    Starting,
    /// A game is going on (also in observe mode).
    Game,
    /// The game is returning to the main menu: this process ends.
    Ending,
}

/// Which process answers, and where the game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Menu {
    /// When the game process answering started, in milliseconds since 1970:
    /// a different number once the game has started again.
    pub process: u64,
    pub place: Place,
}

/// A save in the save games folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Save {
    /// The file's name without `.eu4`, which menu.load takes.
    pub name: String,
    pub file: String,
    pub bytes: u64,
    /// When it was last written, in seconds since 1970.
    pub modified: u64,
    /// The country it is played as, as its header says; null when that
    /// cannot be read, or for a save of observe mode.
    pub player: Option<CountryTag>,
    /// Its date, as its header says.
    pub date: Option<Date>,
    /// The mods it was made with that the game did not start with (the
    /// playset it started with lacks them), by name: menu.load refuses the
    /// save while there are any.
    pub missing_mods: Vec<String>,
}
