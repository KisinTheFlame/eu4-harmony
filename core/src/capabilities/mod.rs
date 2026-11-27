//! Unified capability layer and capability catalogue (ADR one-capability-catalogue):
//! all 56 capability groups, map coloring, stateful group adapters, and the
//! `Catalogue` seam shared by Lua extensions and remote control.

pub(crate) mod catalogue;
pub(crate) mod game;

use crate::native::points::Point;
use std::sync::OnceLock;

/// Top-level callbacks wired by `app` so `capabilities` never depends on
/// `lua`, `framework_ui`, `dev`, or `app` safe-point orchestration.
pub(crate) struct CapabilityHooks {
    pub(crate) on_safe_frame: fn(),
    pub(crate) reload_extension:
        fn(crate::world::Caller, eu4_harmony_remote::extensions::args::Reload) -> crate::world::GResult<eu4_harmony_remote::extensions::Reloaded>,
    #[cfg(feature = "dev")]
    pub(crate) answer_memory: fn(eu4_harmony_remote::memory::MemoryRequest) -> Result<eu4_harmony_remote::memory::MemoryAnswer, eu4_harmony_remote::Error>,
    #[cfg(feature = "dev")]
    pub(crate) answer_performance:
        fn(eu4_harmony_remote::performance::PerformanceRequest) -> crate::world::GResult<eu4_harmony_remote::performance::PerformanceAnswer>,
}

static HOOKS: OnceLock<CapabilityHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: CapabilityHooks) {
    let _ = HOOKS.set(hooks);
}

pub(super) fn hooks() -> Option<&'static CapabilityHooks> {
    HOOKS.get()
}

pub(crate) fn points() -> Vec<Point> {
    game::points()
}
