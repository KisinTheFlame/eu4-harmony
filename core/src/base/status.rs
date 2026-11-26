//! Capability call identity, view, status, and error primitives shared across
//! `render`, `world`, `capabilities`, `lua`, and `remote`.
pub(crate) use eu4_harmony_remote::As;

/// How much of the game a read gives (GLOSSARY: Player's view, Full view;
/// ADR requests-act-as-the-player-or-as-the-game).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// What the player's interface shows: the map as fog of war leaves it,
    /// and of other countries what their panels show.
    Player,
    /// Everything the game keeps.
    Full,
}

/// The view `caller` reads: the full view for an extension and a request as
/// the game, the player's view for a request as the player.
pub fn view(caller: Caller) -> View {
    match caller {
        Caller::Extension(_) | Caller::Remote(As::Game) => View::Full,
        Caller::Remote(As::Player) => View::Player,
    }
}

/// Who calls a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    /// The extension with this index, from one of its callbacks. It may do
    /// what the game does too (ADR requests-act-as-the-player-or-as-the-game).
    Extension(usize),
    /// A remote client's request, as the player or as the game. Its orders
    /// are the player's.
    Remote(As),
}

impl Caller {
    /// The calling extension, for what only an extension has (`what`, such as
    /// "interface elements").
    pub fn extension(self, what: &str) -> GResult<usize> {
        match self {
            Caller::Extension(index) => Ok(index),
            Caller::Remote(_) => fail(Status::Unavailable, format!("only extensions have {what}")),
        }
    }
}

/// Why a capability failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// No campaign is running, or one is being loaded.
    NotInGame,
    /// The player observes with no country in view, so there is no country
    /// to act as (ADR observe-mode-acts-as-the-country-in-view).
    NoCountry,
    /// The game state cannot be read right now; a later frame may succeed.
    NotReady,
    NotFound,
    /// A command was refused; the message says why.
    Rejected,
    InvalidArgument,
    /// Not available in this state, to this caller, or after an unexpected
    /// game-side failure.
    Unavailable,
    /// Called off the game's main thread.
    WrongContext,
    Internal,
}

/// A failed capability call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail {
    pub status: Status,
    pub message: String,
}
pub type GResult<T> = std::result::Result<T, Fail>;

impl Fail {
    /// A failure with `status`, in the words `message`: a refusal the
    /// capability words itself.
    pub fn new(status: Status, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self { status: Status::Internal, message }
    }
}

/// A refusal of the framework's own checks (`eu4_harmony_checks`).
impl From<eu4_harmony_checks::Refusal> for Fail {
    fn from(refusal: eu4_harmony_checks::Refusal) -> Self {
        use eu4_harmony_checks::Status as Checked;
        let status = match refusal.status {
            Checked::NotReady => Status::NotReady,
            Checked::NotFound => Status::NotFound,
            Checked::Rejected => Status::Rejected,
            Checked::InvalidArgument => Status::InvalidArgument,
        };
        Self { status, message: refusal.message }
    }
}

pub fn fail<T>(status: Status, message: impl Into<String>) -> GResult<T> {
    Err(Fail { status, message: message.into() })
}
