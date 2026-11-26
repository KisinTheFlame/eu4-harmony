//! The game on the player's desktop, for a game started hidden
//! (`crate::tweaks::hidden`).
use super::{fail, Caller, Fail, GResult, Status};
use eu4_harmony_remote::desktop::Desktop;

fn now() -> Desktop {
    Desktop { hidden: crate::tweaks::hidden::hidden(), hidden_start: crate::tweaks::hidden::hidden_start(), sound: crate::tweaks::hidden::sound() }
}

fn refused(refused: crate::tweaks::hidden::Refused) -> Fail {
    use crate::tweaks::hidden::Refused;
    match refused {
        Refused::NotControllable => Fail { status: Status::InvalidArgument, message: "the game was not started hidden (-harmony_hidden) or as a test game".into() },
        Refused::Fullscreen => Fail { status: Status::InvalidArgument, message: "a fullscreen game window cannot be hidden".into() },
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
    crate::tweaks::hidden::show().map_err(refused)?;
    Ok(now())
}

fn hide(caller: Caller) -> GResult<Desktop> {
    remote_only(caller)?;
    crate::tweaks::hidden::hide().map_err(refused)?;
    Ok(now())
}

/// The group's dependency points: none of its own.
pub fn points() -> Vec<crate::native::points::Point> {
    Vec::new()
}

crate::capabilities::catalogue::bind! {
    eu4_harmony_remote::desktop: DesktopRequest => DesktopAnswer;
    Get => caller desktop;
    Show => caller show;
    Hide => caller hide;
}
