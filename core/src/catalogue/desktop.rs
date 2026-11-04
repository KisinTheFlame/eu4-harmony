//! The game on the player's desktop, for a game started hidden
//! (`crate::hidden`).
use crate::game;
use eu4_harmony_remote::{
    desktop::{Desktop, DesktopAnswer, DesktopRequest},
    Error,
};

layer! {
    fn desktop() -> Desktop = desktop;
    fn show() -> Desktop = show;
    fn hide() -> Desktop = hide;
}

fn now() -> Desktop {
    Desktop { hidden: crate::hidden::hidden(), hidden_start: crate::hidden::hidden_start(), sound: crate::hidden::sound() }
}

fn refused(refused: crate::hidden::Refused) -> game::Fail {
    use crate::hidden::Refused;
    match refused {
        Refused::NotHiddenStart => game::Fail { status: game::Status::InvalidArgument, message: "the game was not started hidden (-harmony_hidden)".into() },
        Refused::NoWindow => game::Fail { status: game::Status::NotReady, message: "the game has no window yet".into() },
    }
}

fn remote_only(caller: game::Caller) -> game::GResult<()> {
    match caller {
        game::Caller::Extension(_) => game::fail(game::Status::Unavailable, "only remote control shows and hides the game"),
        _ => Ok(()),
    }
}

fn desktop(caller: game::Caller) -> game::GResult<Desktop> {
    remote_only(caller)?;
    Ok(now())
}

fn show(caller: game::Caller) -> game::GResult<Desktop> {
    remote_only(caller)?;
    crate::hidden::show().map_err(refused)?;
    Ok(now())
}

fn hide(caller: game::Caller) -> game::GResult<Desktop> {
    remote_only(caller)?;
    crate::hidden::hide().map_err(refused)?;
    Ok(now())
}

pub fn run(request: DesktopRequest, game: &mut (impl Layer + ?Sized)) -> Result<DesktopAnswer, Error> {
    Ok(match request {
        DesktopRequest::Get {} => DesktopAnswer::Get(game.desktop()?),
        DesktopRequest::Show {} => DesktopAnswer::Show(game.show()?),
        DesktopRequest::Hide {} => DesktopAnswer::Hide(game.hide()?),
    })
}
