//! A shorter stop for a save, the save the same (docs/game-speed.md, S3 and
//! S8).
//!
//! - `background`: a local save's entries compressed by a thread of the
//!   framework's after the game wrote them, instead of on the main thread.
//! - `stored`: the entries the game then deflates at level 0 copied into the
//!   zip's stored blocks, their CRC taken on the pool.
//! - `parallel`: the countries and provinces of the game state written on
//!   several threads at once, and the pool that takes the other parts.
//! - `sum`: the save checksum's sum taken on the pool.
//! - `update_thread`: the thread the game updates during a save woken when the
//!   save is written.
pub mod background;
pub mod parallel;
pub mod stored;
pub mod sum;
pub mod update_thread;
