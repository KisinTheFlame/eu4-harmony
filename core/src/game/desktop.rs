//! The game on the player's desktop, for a game started hidden
//! (`crate::hidden`).
use super::{fail, Caller, Fail, GResult, Status};
use eu4_harmony_remote::desktop::Desktop;

fn now() -> Desktop {
    Desktop { hidden: crate::hidden::hidden(), hidden_start: crate::hidden::hidden_start(), sound: crate::hidden::sound() }
}

fn refused(refused: crate::hidden::Refused) -> Fail {
    use crate::hidden::Refused;
    match refused {
        Refused::NotHiddenStart => Fail { status: Status::InvalidArgument, message: "the game was not started hidden (-harmony_hidden)".into() },
        Refused::NoWindow => Fail { status: Status::NotReady, message: "the game has no window yet".into() },
    }
}

fn remote_only(caller: Caller) -> GResult<()> {
    match caller {
        Caller::Extension(_) => fail(Status::Unavailable, "only remote control shows and hides the game"),
        _ => Ok(()),
    }
}

fn desktop(caller: Caller) -> GResult<Desktop> {
    remote_only(caller)?;
    Ok(now())
}

fn show(caller: Caller) -> GResult<Desktop> {
    remote_only(caller)?;
    crate::hidden::show().map_err(refused)?;
    Ok(now())
}

fn hide(caller: Caller) -> GResult<Desktop> {
    remote_only(caller)?;
    crate::hidden::hide().map_err(refused)?;
    Ok(now())
}

/// The group's dependency points: none of its own.
pub fn points() -> Vec<crate::points::Point> {
    Vec::new()
}

crate::catalogue::bind! {
    eu4_harmony_remote::desktop: DesktopRequest => DesktopAnswer;
    Get => caller desktop;
    Show => caller show;
    Hide => caller hide;
}
