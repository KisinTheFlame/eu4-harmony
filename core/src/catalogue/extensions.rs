//! The mods' extensions, remote control only: reloading one.
use super::Convert;
use eu4_harmony_remote::extensions::{ExtensionAnswer, ExtensionRequest, Reloaded};
use eu4_harmony_remote::extensions::args;

layer! {
    run ExtensionRequest => ExtensionAnswer;
    Reload => fn reload -> crate::lua::Reloaded = caller crate::lua::reload;
}

impl Convert<Reloaded> for crate::lua::Reloaded {
    fn convert(self) -> Reloaded {
        Reloaded { session: self.session, lost: self.lost }
    }
}
