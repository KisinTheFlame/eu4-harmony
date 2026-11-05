//! The mods' extensions, remote control only: reloading one.
use super::Convert;
use eu4_harmony_remote::extensions::{ExtensionAnswer, ExtensionRequest, Reloaded};

layer! {
    run ExtensionRequest => ExtensionAnswer;
    Reload => fn reload(id: String) -> crate::lua::Reloaded = crate::lua::reload;
}

impl Convert<Reloaded> for crate::lua::Reloaded {
    fn convert(self) -> Reloaded {
        Reloaded { session: self.session, lost: self.lost }
    }
}
