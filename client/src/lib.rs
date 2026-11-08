//! Reaching the running game from outside its process: remote control's
//! named pipe, asked in its JSON form or with the request and answer types of
//! `eu4-harmony-remote`; the game's process, started hidden or shown up to
//! its main menu, followed to its exit code and ended; and the backup and
//! restore of the player's files a run may change. The command line tool and
//! the repository's development commands (`cargo xtask`) share it.
pub mod backup;
pub mod game;
pub mod pipe;
pub mod start;

pub type Result<T> = std::result::Result<T, String>;
