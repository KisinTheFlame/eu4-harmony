//! The mods' extensions, remote control only: reloading one, which the Lua
//! host does (`crate::lua::reload`).
use crate::catalogue::Convert;
use eu4_harmony_remote::extensions::Reloaded;

/// The group's dependency points: none of its own.
pub fn points() -> Vec<crate::points::Point> {
    Vec::new()
}

crate::catalogue::bind! {
    eu4_harmony_remote::extensions: ExtensionRequest => ExtensionAnswer;
    Reload => caller crate::lua::reload;
}

impl Convert<Reloaded> for crate::lua::Reloaded {
    fn convert(self) -> Reloaded {
        Reloaded { session: self.session, lost: self.lost }
    }
}
