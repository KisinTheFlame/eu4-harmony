//! The mods' extensions, remote control only: reloading one, wired to the Lua
//! host through `capabilities::CapabilityHooks`.
use crate::world::{fail, Caller, GResult, Status};
use eu4_harmony_remote::extensions::Reloaded;

/// The group's dependency points: none of its own.
pub fn points() -> Vec<crate::native::points::Point> {
    Vec::new()
}

fn reload(caller: Caller, args: eu4_harmony_remote::extensions::args::Reload) -> GResult<Reloaded> {
    match crate::capabilities::hooks() {
        Some(h) => (h.reload_extension)(caller, args),
        None => fail(Status::Unavailable, "extension host unavailable"),
    }
}

crate::capabilities::catalogue::bind! {
    eu4_harmony_remote::extensions: ExtensionRequest => ExtensionAnswer;
    Reload => caller reload;
}
