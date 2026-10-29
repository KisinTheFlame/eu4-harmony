//! The main menu, remote control only (ADR 0015): where the game is, the
//! saves, loading one, starting a new game and returning to the main menu.
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
    anywhere State "" {} -> Menu, "process (when the game process started, in milliseconds since 1970), and place: main_menu, starting (a game is being set up or loaded) or game";
    /// The saves in the save games folder, newest first.
    anywhere Saves "saves" {} -> Vec<Save>, "the saves: name (to load it by), file, bytes, modified (seconds since 1970), and player and date as the save's header says them (null when it cannot be read)";
    /// Loads a save from the main menu and plays it as the country it was
    /// saved as, as the continue box does. Only from the main menu's first
    /// page (at startup, once menu says main_menu). Answers once the game is
    /// ready, however long the save takes to load.
    anywhere Load "load" {
        /// The save's name, as menu.saves lists it.
        save: String,
    } -> Session, "the session of the loaded game";
    /// Starts a new game at the default start date as a country, as the
    /// single player setup does. Only from the main menu's first page (at
    /// startup, once menu says main_menu). Answers once the game is ready.
    anywhere Start "start" {
        /// The country to play.
        country: CountryTag,
    } -> Session, "the session of the new game";
    /// Returns to the main menu, without the game's question or its summary,
    /// as the player would. The game does that by starting eu4.exe again, so
    /// the answer comes from this process before it ends; after it every
    /// request to this process is answered not_ready. The new process opens
    /// the pipe a few seconds later and answers once it presents its first
    /// frame: ask menu again until its process differs and its place is
    /// main_menu.
    game Return "return" {} -> Menu, "this process, and place ending";
}

/// Where the game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// The main menu shows, on any of its pages, and no game is being
    /// started from it.
    MainMenu,
    /// A game is being set up or loaded: started from the main menu, or
    /// neither the main menu nor a game's frame runs.
    Starting,
    /// A game is going on (also while the player only observes).
    Game,
    /// The game is returning to the main menu: this process ends.
    Ending,
}

/// Which process answers, and where the game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Menu {
    /// When the game process answering started, in milliseconds since 1970:
    /// a different number once the game has started again.
    pub process: u64,
    pub place: Place,
}

/// A save in the save games folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Save {
    /// The file's name without `.eu4`, which menu.load takes.
    pub name: String,
    pub file: String,
    pub bytes: u64,
    /// When it was last written, in seconds since 1970.
    pub modified: u64,
    /// The country it is played as, as its header says; null when that
    /// cannot be read, or for an observer's save.
    pub player: Option<CountryTag>,
    /// Its date, as its header says.
    pub date: Option<Date>,
}
