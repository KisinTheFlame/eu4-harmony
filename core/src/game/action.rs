//! What the player does in a panel, carried out as the panel carries it out
//! (docs/player-actions.md): refused with the game's reasons when the game
//! does not let the player do it, and otherwise the command the button
//! posts, posted through the session's commands ([`Commands`]). Every action
//! refuses in the same words here, named by what the player does, as "raise
//! stability" or "send a missionary there".
use super::{
    commands::{Commands, Refused},
    Fail, GResult, Status,
};
use eu4_harmony_remote::choices::Choice;

/// Carries out the action `doing` the panel offers as `choice` says: refused
/// with its reasons when the player cannot, otherwise the command `text`
/// writes posted.
pub fn carry_out(commands: Commands, choice: &Choice, doing: &str, text: impl FnOnce() -> GResult<String>) -> GResult<()> {
    allowed(choice, doing)?;
    post(commands, &text()?, doing)
}

/// Whether the game lets the player do `doing`, as the panel's `choice`
/// says, or its refusal with the choice's reasons.
pub fn allowed(choice: &Choice, doing: &str) -> GResult<()> {
    match choice.can {
        true => Ok(()),
        false => Err(not_allowed(doing, &choice.reasons)),
    }
}

/// The refusal of `doing`, which the game does not let the player do now:
/// the game's `reasons`, one a line, or, when it gives none, that it gives
/// none.
pub fn not_allowed(doing: &str, reasons: &[String]) -> Fail {
    let message = match reasons.is_empty() {
        true => format!("the game does not let the player {doing} now, and gives no reason"),
        false => reasons.join("\n"),
    };
    Fail { status: Status::Rejected, message }
}

/// Posts the command `text` writes for the action `doing`, the game having
/// let the player do it: refused, when the command layer does not post it,
/// as [`refused`] words it.
pub fn post(commands: Commands, text: &str, doing: &str) -> GResult<()> {
    commands.post(text).map_err(|why| refused(why, doing))
}

/// Why the command for `doing` was not posted, in the layer's words: the
/// command's own check, which gives no reason, and a session that takes no
/// orders, [`Status::Rejected`]; a text the game read otherwise than the
/// framework wrote it, [`Status::Internal`].
pub fn refused(why: Refused, doing: &str) -> Fail {
    let (status, message) = match why {
        Refused::Game(fail) => return fail,
        Refused::Check => (Status::Rejected, format!("the game's command refuses to {doing}, and gives no reason")),
        Refused::NotTaken => (Status::Rejected, "the game takes no orders from the player now".to_string()),
        Refused::Unread(how) => (Status::Internal, format!("the game read the command to {doing} otherwise: {how}")),
    };
    Fail { status, message }
}

#[cfg(test)]
mod tests {
    use super::super::commands::record;
    use super::*;

    fn failed(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was carried out") };
        (status, message)
    }

    #[test]
    fn an_action_the_player_cannot_take_is_refused_with_the_game_s_reasons_and_not_posted() {
        let cannot = Choice::cannot(vec!["没有足够的sailors。".into(), "法兰西不是托雷多的控制者。".into()], Vec::new());
        let text = || -> GResult<String> { panic!("the command was written") };
        assert_eq!(
            failed(carry_out(Commands::Recorded, &cannot, "recruit it there", text)),
            (Status::Rejected, "没有足够的sailors。\n法兰西不是托雷多的控制者。".into())
        );
        let silent = Choice::cannot(Vec::new(), Vec::new());
        assert_eq!(
            failed(carry_out(Commands::Recorded, &silent, "raise stability", text)),
            (Status::Rejected, "the game does not let the player raise stability now, and gives no reason".into())
        );
        assert!(record::posted().is_empty());
    }

    #[test]
    fn an_action_the_player_can_take_posts_its_command() {
        let can = Choice::can(Vec::new());
        carry_out(Commands::Recorded, &can, "raise stability", || Ok("increase_stability={ }".into())).unwrap();
        assert_eq!(record::posted(), ["increase_stability={ }"]);
    }

    #[test]
    fn a_command_the_layer_does_not_post_is_refused_in_one_wording() {
        let cases = [
            (Refused::Check, Status::Rejected, "the game's command refuses to hire them there, and gives no reason"),
            (Refused::NotTaken, Status::Rejected, "the game takes no orders from the player now"),
            (Refused::Unread("keys left out".into()), Status::Internal, "the game read the command to hire them there otherwise: keys left out"),
            (Refused::Game(Fail { status: Status::NotReady, message: "later".into() }), Status::NotReady, "later"),
        ];
        for (why, status, message) in cases {
            record::refuse_next(why);
            assert_eq!(failed(post(Commands::Recorded, "hire_mercenary={ }", "hire them there")), (status, message.to_string()));
        }
        assert!(record::posted().is_empty());
    }
}
