//! Presentation and rendering infrastructure: the separate drawing thread
//! (presentation lock, crossings, deferred calls) and the low-level Direct3D 9
//! `Present`/`Reset` and Win32 `WndProc` hook shells.

pub(crate) mod drawing;
pub(crate) mod present;
pub(crate) mod window_input;
