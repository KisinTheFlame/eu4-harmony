//! Logs every command the game executes, in its written form, for finding out
//! what a command carries (the `dev-command-log` feature: a player's build has
//! none of this). dev.toml's `[command-log]` can name the types to log, by the
//! game's names (`types = ["buildunit", "move_order"]`); every type is logged
//! when it names none. It listens to the command layer
//! ([`crate::game::commands`]), which writes a command out only when asked.
use crate::game::commands::{self, Heard};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

static TYPES: OnceLock<Vec<String>> = OnceLock::new();
static LOGGED: AtomicUsize = AtomicUsize::new(0);

pub fn start() {
    let types = TYPES.get_or_init(|| crate::dev::options().command_log.types.clone());
    log!("commands: logging {}", if types.is_empty() { "every executed command".to_string() } else { types.join(", ") });
    commands::listen(heard);
}

fn heard(command: &Heard) {
    let types = TYPES.get().map_or(&[][..], Vec::as_slice);
    if !types.is_empty() && !types.contains(&command.kind) {
        return;
    }
    if let Some(text) = command.text() {
        let n = LOGGED.fetch_add(1, Ordering::Relaxed);
        log!("command #{n} id {} {}: {text}", command.id, command.kind);
    }
}
