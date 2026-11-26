//! Process-level foundation with zero dependencies on any other core node:
//! logging, paths, settings persistence, localisation, mod discovery, notices,
//! troubleshooting archive collection, zip utilities, developer build options,
//! and core runtime primitives.

#[macro_use]
pub(crate) mod log;
pub(crate) mod dev_options;
pub(crate) mod heap;
pub(crate) mod icu;
pub(crate) mod lang;
pub(crate) mod loc;
pub(crate) mod mods;
pub(crate) mod notice;
pub(crate) mod paths;
pub(crate) mod report;
pub(crate) mod runtime;
pub(crate) mod save_sum;
pub(crate) mod settings;
pub(crate) mod stack;
pub(crate) mod troubleshooting;
pub(crate) mod zip;

pub(crate) use eu4_harmony_manifest::{hosts, pdx};
pub(crate) use runtime::{
    called_by_game, fatal, game_dir, game_window, guarded, hidden_start, log_panics, main_thread, mark_bootstrap, open_fallback_log, open_logs, reopen_log,
    set_game_dir, set_game_window, set_hidden, set_hidden_start, set_session_number, since_bootstrap, thread_id, Result, MAIN_THREAD,
};
