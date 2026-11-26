//! EU4 Harmony core. Loaded by the `version.dll` proxy during process start;
//! `eu4_harmony_bootstrap` runs under the loader lock, so it only checks the
//! executable and installs every dependency point, all or none
//! (ADR framework-succeeds-or-fails-as-a-whole). Everything that needs the
//! game's own setup (its file system, the user directory) waits for the game's
//! first `PHYSFS_init`, on the game thread. When startup fails nothing is
//! hooked; a thread of its own writes the report and tells the player.
// The capability groups' list declares every group's modules inside its
// macro, so a module's own macros, such as a test's large `json!`, expand
// deeper with each group the list holds.
#![recursion_limit = "256"]

#[macro_use]
mod base;
#[macro_use]
mod native;
mod text;
mod render;
mod repairs;
mod speed;
mod tweaks;
mod session;
mod interface;
mod world;
mod capabilities;
mod lua;
mod remote;
mod framework_ui;
mod dev;
mod app;
#[cfg(test)]
mod fake_game;

/// Called by the proxy from its DllMain. Returns 0 on success.
#[no_mangle]
pub extern "system" fn eu4_harmony_bootstrap() -> u32 {
    app::bootstrap_entry()
}
