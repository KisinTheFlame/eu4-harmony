//! Presentation and rendering infrastructure: Direct3D 9 device primitives,
//! the separate drawing thread (presentation lock, crossings, deferred calls),
//! `Present`/`Reset` orchestration, and the Win32 `WndProc` hook.

pub(crate) mod d3d9;
pub(crate) mod drawing;
pub(crate) mod present;
pub(crate) mod window_input;
