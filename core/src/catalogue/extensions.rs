//! The mods' extensions, remote control only: reloading one.
use eu4_harmony_remote::{
    extensions::{ExtensionAnswer, ExtensionRequest, Reloaded},
    Error,
};

layer! {
    fn reload(id: String) -> crate::lua::Reloaded = crate::lua::reload;
}

pub fn run(request: ExtensionRequest, game: &mut (impl Layer + ?Sized)) -> Result<ExtensionAnswer, Error> {
    match request {
        ExtensionRequest::Reload { id } => {
            let r = game.reload(id)?;
            Ok(ExtensionAnswer::Reload(Reloaded { session: r.session, lost: r.lost }))
        }
    }
}
