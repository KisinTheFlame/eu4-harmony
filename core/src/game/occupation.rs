//! Occupation: handing a province the player occupies over to an ally, as
//! the province view's transfer occupation button and window do
//! (docs/research/peace.md, "Handing over occupied provinces").
//!
//! The button asks the game's check for the player and the province, which
//! says why not, or lists the allies that may take it; the window lists
//! those, and picking one posts the transfer command for it, which asks the
//! same check before the game carries it out.
use super::{action, diplomacy, with_state, GResult, Session, Tag};
use crate::{game_string, memory::Memory, objects::campaign::province, points::Point};
use eu4_harmony_checks::peace as checks;
use eu4_harmony_remote::choices::Choice;

/// The check: (country, province, allies, reasons), the allies a list the
/// check fills ([`receivers`]), `reasons` a `std::string` or null, which
/// gets the button's tooltip when the check says no.
const CHECK: usize = 0x3dbce0;

/// The list of allies the check fills: the first node, and in each node the
/// ally's tag and the next node; a node is [`NODE_SIZE`] bytes.
mod receivers {
    pub const FIRST: usize = 0;
    pub const SIZE: usize = 0x18;
    pub const TAG: usize = 0;
    pub const NEXT: usize = 0x10;
    pub const NODE_SIZE: usize = 0x20;
}

/// The most allies a list is believed to hold.
const MOST: usize = 1_000;

/// Whether the player can hand a province over, and the allies the window
/// lists to take it.
#[derive(Clone, Debug, PartialEq)]
pub struct Transfers {
    pub choice: Choice,
    pub allies: Vec<Tag>,
}

/// Whether the player can hand `province` over, and to which allies, as the
/// transfer occupation button and window tell the player.
pub fn options(Session { m, ctx, .. }: Session, province: u32) -> GResult<Transfers> {
    let at = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let (allies, reasons) = check(m, ctx.country, at)?;
    Ok(match allies {
        Some(allies) => Transfers { choice: Choice::can(Vec::new()), allies },
        None => Transfers { choice: Choice::cannot(reasons, Vec::new()), allies: Vec::new() },
    })
}

/// Hands `province` over to `ally`, as picking it in the window does; the
/// country that occupies it after.
pub fn transfer(Session { m, ctx, commands, .. }: Session, province: u32, ally: Tag) -> GResult<Option<Tag>> {
    diplomacy::other(m, &ctx, ally)?;
    let at = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let (allies, reasons) = check(m, ctx.country, at)?;
    let Some(allies) = allies else {
        return Err(action::not_allowed("hand it over", &reasons));
    };
    if !allies.iter().any(|a| a.index() == ally.index()) {
        return Err(checks::not_a_receiver(diplomacy::letters(ally), province).into());
    }
    action::post(commands, &command(ally, province, ctx.player), "hand it over")?;
    let controller = Tag(m.u64(at + province::CONTROLLER)?);
    Ok((controller.valid() && controller.normal()).then_some(controller))
}

/// The transfer command's written form: the ally to take the province, and
/// the player handing it over.
fn command(ally: Tag, province: u32, player: Tag) -> String {
    format!("transfer_controller_command={{ country=\"{}\" province={province} commander=\"{}\" }}", diplomacy::letters(ally), diplomacy::letters(player))
}

/// What the check says of the player's country at `country` and the
/// province at `province`: the allies that may take it, or None and why
/// not, the button's tooltip's lines.
fn check(m: &Memory, country: usize, province: usize) -> GResult<(Option<Vec<super::Tag>>, Vec<String>)> {
    let mut list = [0u64; receivers::SIZE / 8];
    let list_at = list.as_mut_ptr() as usize;
    let checked = game_string::written(m, |s| m.call(m.at(CHECK), &[country, province, list_at, s]));
    let first = unsafe { std::ptr::read_volatile(&list) }[receivers::FIRST / 8] as usize;
    let allies = nodes(m, first);
    let (can, text) = checked?;
    let allies = allies?;
    if can as u8 == 0 {
        return Ok((None, diplomacy::text_lines(&text)));
    }
    Ok((Some(allies), Vec::new()))
}

/// The allies in the list from `first`, its nodes freed as the game frees
/// them.
fn nodes(m: &Memory, mut node: usize) -> GResult<Vec<Tag>> {
    let mut allies = Vec::new();
    while node != 0 {
        if allies.len() > MOST {
            return Err("the allies to take a province are out of bounds".to_string().into());
        }
        let next = m.ptr(node + receivers::NEXT)?;
        allies.push(Tag(m.u64(node + receivers::TAG)?));
        crate::game_heap::delete(m, node, receivers::NODE_SIZE)?;
        node = next;
    }
    Ok(allies)
}

/// The check, and the command's: its check asks the check for the
/// commander and finds the country among the allies, freeing the nodes.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CHECK, "48 89 5c 24 18 4c 89 4c 24 20 48 89 54 24 10 55 56 57 41 54 41 55 41 56", "transfer occupation check"),
        Point::keep(0x2a59c8usize, "4c 8b 43 50 49 c1 e8 20 48 8b 10 48 c1 ea 20 66 41 3b d0 74 0c 48 8b 50 10", "transfer occupation allies")
            .uses(&[receivers::TAG, receivers::NEXT]),
        Point::keep(0x2a5a00usize, "48 8b 59 10 ba 20 00 00 00 e8 e2 3a 49 01", "transfer occupation allies freed")
            .uses(&[receivers::NEXT, receivers::NODE_SIZE]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::game_tag;

    #[test]
    fn the_command_names_the_ally_to_take_it_and_the_player() {
        assert_eq!(command(game_tag(b"DUL", 2), 151, game_tag(b"TUR", 1)), "transfer_controller_command={ country=\"DUL\" province=151 commander=\"TUR\" }");
    }
}
