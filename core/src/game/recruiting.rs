//! Recruiting: one regiment or ship of a kind in a province, ordered for the
//! player through the game's own recruit command, as the recruit button in
//! the province view orders it, so that the game's checks, costs and
//! multiplayer rules apply (docs/research/recruiting.md).
use super::{
    context, events, fail, game, map_manager,
    provinces::{can_recruit, unit_type, UnitKind},
    with_state, Caller, Ctx, GResult, Status,
};
use crate::{
    memory::Memory,
    objects::{
        campaign::{self, classes, globals, province, recruit_command, world},
        session as layout,
    },
    points::Point,
    unicode::fill::ALLOCATE,
};

/// The recruit command's constructor, as the recruit button calls it:
/// (command, country, unit type, province ID, mercenary company), no company
/// for a regular unit.
const COMMAND: usize = 0x26ce40;
/// The command's check, its vtable's [`recruit_command::CHECK`]: (command),
/// whether the game would carry it out now.
const CHECK: usize = 0x26e080;
/// The command's destructor, its vtable's [`recruit_command::DESTROY`]:
/// (command, flags), flag 1 freeing it.
const DESTROY: usize = 0x26cda0;
const FREE: usize = 1;
/// The recruit button in the province view: it makes the command and posts
/// it to the session.
const BUTTON: usize = 0xe2151f;

pub fn points() -> Vec<Point> {
    use world::*;
    vec![
        Point::keep(COMMAND, "48 89 4c 24 08 53 48 83 ec 20 48 8b d9 c7 41 08 65 01 00 00", "recruit command"),
        Point::method(classes::RECRUIT_COMMAND, recruit_command::CHECK, CHECK, "recruit command check"),
        Point::keep(CHECK, "40 57 48 83 ec 50 0f bf 41 54 48 8b f9", "recruit command check"),
        Point::method(classes::RECRUIT_COMMAND, recruit_command::DESTROY, DESTROY, "recruit command destructor"),
        Point::keep(DESTROY, "48 89 5c 24 08 57 48 83 ec 20 48 8b d9 8b fa 48 8b 89 98 00 00 00", "recruit command destructor"),
        // The button: the command's size, the player, the province's ID,
        // no mercenary company, the constructor; the session from the map
        // manager, and the post.
        Point::keep(
            BUTTON,
            "48 8b 3d 52 e9 51 01 b9 a0 00 00 00 e8 a4 1d c1 00 48 89 44 24 68 48 8b 4b 08 80 bf 66 1e 00 00 07 75 12 \
             80 bf 6f 1e 00 00 00 74 09 48 8b 97 68 1e 00 00 eb 07 48 8b 97 60 1e 00 00 48 c7 44 24 20 00 00 00 00 \
             44 8b 49 20 4c 8b c6 48 8b c8 e8 cd b8 44 ff 48 8b d8 48 8b 8f 00 1e 00 00 48 8b 11 ff 92 80 00 00 00 \
             48 8b d3 48 8b c8 e8 3f 8d 6a 00",
            "recruit button",
        )
        .uses(&[
            globals::WORLD,
            recruit_command::SIZE,
            PLAYER_MODE,
            SWITCHED as usize,
            PLAYER_SWITCHED,
            SWITCHED_PLAYER,
            PLAYER,
            province::ID,
            MAP_MANAGER,
            campaign::map_manager::SESSION,
        ]),
    ]
}

/// Recruits one regiment or ship of `kind` in `province` for the player, the
/// way the recruit button in the province view does. Refused
/// ([`Status::Rejected`]) when the game would not let the player recruit it
/// there now, with the game's reasons, one a line; [`Status::NotFound`] for a
/// number that names no province. Done once the game's session has the
/// order: the game carries it out at its next frame, paying for it and
/// putting it in the province's recruit queue. It checks the order again
/// then and drops one that no longer passes with only a line in its own log,
/// so orders given at one safe point or in one callback are each checked
/// here against the same treasury, manpower and sailors, and those the first
/// ones leave too little for are dropped at that frame.
pub fn recruit(_: Caller, id: u32, kind: UnitKind) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    if id == 0 {
        return fail(Status::NotFound, "no province 0");
    }
    let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
    let Some(unit_type) = unit_type(m, ctx.country, kind)? else {
        return fail(Status::Rejected, format!("the player has no unit type of the kind {}", kind.key()));
    };
    let (allowed, reasons) = can_recruit(m, &ctx, address, unit_type)?;
    if !allowed {
        return fail(Status::Rejected, refusal(&reasons));
    }
    // The session before the command: once the command is made, only a call
    // into the game can fail before it is posted or destroyed. The game owns
    // it once its session took it; until then, it is the framework's to
    // free. A command a call into the game failed on is left alone.
    let session = session(m, &ctx)?;
    let command = m.call(m.at(ALLOCATE), &[recruit_command::SIZE])?;
    if command == 0 {
        return Err("the game could not allocate a recruit command".to_string().into());
    }
    m.call(m.at(COMMAND), &[command, ctx.player.0 as usize, unit_type, id as usize, 0])?;
    let checked = m.call(m.at(CHECK), &[command])? as u8 != 0;
    let taken = checked && post(m, session, command)?;
    if !taken {
        m.call(m.at(DESTROY), &[command, FREE])?;
    }
    match (checked, taken) {
        (true, true) => Ok(()),
        // What the snapshot leaves out: the command's own rule for ships.
        (false, _) => fail(Status::Rejected, "the game's recruit command refuses it, and gives no reason"),
        (true, false) => fail(Status::Rejected, "the game takes no orders from the player now"),
    }
}

/// The game's reasons as one message.
fn refusal(reasons: &[String]) -> String {
    if reasons.is_empty() {
        "the game does not let the player recruit it there, and gives no reason".into()
    } else {
        reasons.join("\n")
    }
}

/// The game's session, as the recruit button finds it: the map manager's
/// method answers it.
fn session(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    let session = m.call(m.at(layout::OF_MAP_MANAGER), &[map_manager(m, ctx)?])?;
    if session == 0 || m.ptr(session)? != m.at(layout::CLASS) {
        return Err("the map manager has no session".to_string().into());
    }
    Ok(session)
}

/// Posts a command to the game's session, as the recruit button does;
/// whether the session took it. A single player game's session takes it at
/// any time; one that is not a client takes it only while the map manager's
/// frame runs (docs/research/recruiting.md).
fn post(m: &Memory, session: usize, command: usize) -> GResult<bool> {
    Ok(m.call(m.at(events::DISPATCH), &[session, command])? as u8 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_says_the_game_s_reasons_one_a_line() {
        assert_eq!(refusal(&["没有足够的manpower。".into(), "没有足够的ducats。".into()]), "没有足够的manpower。\n没有足够的ducats。");
        assert!(refusal(&[]).contains("no reason"));
    }
}
