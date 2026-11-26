//! Low-level binary interface to `eu4.exe`: memory access, baseline check,
//! hooks, jump stubs, dependency point definitions and installation, speed-up
//! switches, native game object layouts, strings/text codec, containers,
//! framework files, splice planning, PhysFS hooks, and developer probe stubs.

#[macro_use]
pub(crate) mod stub;
#[macro_use]
pub(crate) mod points;
pub(crate) mod baseline;
pub(crate) mod containers;
pub(crate) mod files;
pub(crate) mod game_heap;
pub(crate) mod game_string;
pub(crate) mod game_text;
pub(crate) mod hooks;
pub(crate) mod main_thread;
pub(crate) mod memory;
pub(crate) mod objects;
pub(crate) mod physfs;
pub(crate) mod probes;
pub(crate) mod speed_up;
pub(crate) mod splice;

pub(crate) use memory::{hooked_memory, memory, set_memory};
