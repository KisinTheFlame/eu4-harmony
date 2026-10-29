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
        campaign::{globals, province, world},
        session::{self as layout, session},
    },
    points::Point,
};

/// The game's `operator new`.
const NEW: usize = 0x1a332d4;
/// The recruit command's constructor, as the recruit button calls it:
/// (command, country, unit type, province ID, mercenary company), no company
/// for a regular unit.
const COMMAND: usize = 0x26ce40;
/// The command's size and class.
const COMMAND_SIZE: usize = 0xa0;
const COMMAND_CLASS: usize = 0x1c4dff8;
/// The command's check, its vtable's `+0x40`: (command), whether the game
/// would carry it out now.
const CHECK: usize = 0x26e080;
const CHECK_SLOT: usize = 0x40;
/// The command's destructor, its vtable's `+0`: (command, flags), flag 1
/// freeing it.
const DESTROY: usize = 0x26cda0;
const FREE: usize = 1;
/// The recruit button in the province view: it makes the command and posts
/// it to the session.
const BUTTON: usize = 0xe2151f;

pub fn points() -> Vec<Point> {
    use world::*;
    vec![
        Point::keep(NEW, "40 53 48 83 ec 20 48 8b d9 eb 0f 48 8b cb e8 95", "game allocator"),
        Point::keep(COMMAND, "48 89 4c 24 08 53 48 83 ec 20 48 8b d9 c7 41 08 65 01 00 00", "recruit command"),
        Point::keep(0x26ce8eusize, "48 8d 05 63 11 9e 01 48 89 01", "recruit command class").uses(&[COMMAND_CLASS]),
        Point::method(COMMAND_CLASS, CHECK_SLOT, CHECK, "recruit command check"),
        Point::keep(CHECK, "40 57 48 83 ec 50 0f bf 41 54 48 8b f9", "recruit command check"),
        Point::method(COMMAND_CLASS, 0, DESTROY, "recruit command destructor"),
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
            COMMAND_SIZE,
            PLAYER_MODE,
            SWITCHED as usize,
            PLAYER_SWITCHED,
            SWITCHED_PLAYER,
            PLAYER,
            province::ID,
            MAP_MANAGER,
            layout::MAP_MANAGER_SLOT,
        ]),
    ]
}

/// Recruits one regiment or ship of `kind` in `province` for the player, the
/// way the recruit button in the province view does. Refused
/// ([`Status::Rejected`]) when the game would not let the player recruit it
/// there now, with the game's reasons, one a line; [`Status::NotFound`] for a
/// number that names no province. Done once the game's session has the
/// order: the game carries it out at its next frame, paying for it and
/// putting it in the province's recruit queue.
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
    let command = m.call(m.at(NEW), &[COMMAND_SIZE])?;
    if command == 0 {
        return Err("the game could not allocate a recruit command".to_string().into());
    }
    m.call(m.at(COMMAND), &[command, ctx.player.0 as usize, unit_type, id as usize, 0])?;
    // The game owns the command once its session took it; until then, it is
    // the framework's to free. A command a call into the game failed on is
    // left alone.
    let checked = m.call(m.at(CHECK), &[command])? as u8 != 0;
    let taken = if checked { post(m, &ctx, command)? } else { false };
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

/// Posts a command to the game's session, as the player's click does inside
/// the map manager's frame. The framework's safe point comes after that
/// frame, so the session is told it is in the frame, as the frame tells it,
/// for as long as the post takes. Whether the session took the command.
fn post(m: &Memory, ctx: &Ctx, command: usize) -> GResult<bool> {
    let session = m.call(m.at(layout::OF_MAP_MANAGER), &[map_manager(m, ctx)?])?;
    if session == 0 || m.ptr(session)? != m.at(layout::CLASS) {
        return Err("the map manager has no session".to_string().into());
    }
    let flag = session + session::IN_FRAME;
    let was = m.u8(flag)?;
    let in_frame = m.u8(ctx.world + world::PLAYER_MODE)? != world::SWITCHED;
    m.write(flag, &[in_frame as u8])?;
    let posted = m.call(m.at(events::DISPATCH), &[session, command]);
    m.write(flag, &[was])?;
    Ok(posted? as u8 != 0)
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
