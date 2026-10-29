//! Questions (GLOSSARY): the game's own events waiting for the player's
//! answer, read from the event windows the game shows the player, and
//! answered as the player does, by choosing an option in the window.
//! [`count`] says how many wait, for advancing time too. The evidence is in
//! docs/research/questions-and-offers.md.
use super::{context, game, map_manager, span, Caller, Ctx, GResult, Status};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign::{self, classes, event, event_option, event_window as window, pending_event as pending, route_options::NO_TAG, world},
        list,
    },
    points::Point,
    unicode::game_text,
};
use std::collections::BTreeSet;

/// `(window, option)`: what clicking an option's button in an event window
/// does: sends the game the command that chooses it and closes the window.
const CHOOSE: usize = 0x10bc2d0;
/// `(result, scope, key, tag)`: the text of a localisation key, with the
/// scope's names filled in, as the event window shows it. Returns a
/// `std::string` in `result`; takes `key`, a `std::string`, by value and
/// destroys it; `tag` is [`NO_TAG`] wherever the game calls it.
const LOCALIZE: usize = 0x10bc860;
/// `(event, result, scope)`: the localisation key of the event's description
/// in the scope, a `std::string` in `result`.
const DESCRIPTION_KEY: usize = 0x6edfb0;
/// `(option, scope)`: whether the player may choose the option, u8.
const OPTION_OPEN: usize = 0x6f6b30;
/// The map manager's method that shows one of the game's events to the
/// player ([`campaign::map_manager::SHOW_EVENT`]): the window it makes is
/// added to the popups, where questions are read from.
const SHOW_EVENT: usize = 0x828c00;

/// More than any game shows.
const MOST: usize = 10_000;
const MOST_OPTIONS: usize = 64;

/// A question: one of the game's events waiting for the player's answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Question {
    pub id: u32,
    /// The event's id in the game's files, such as `flavor_fra.1`.
    pub event: String,
    pub title: String,
    pub description: String,
    /// The last day to answer, a day number; after it the game picks an
    /// option itself.
    pub deadline: i32,
    /// The options the player may choose, in the game's order.
    pub options: Vec<QuestionOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestionOption {
    /// Its number among all the event's options.
    pub index: u32,
    pub text: String,
}

/// The questions waiting for the player's answer, oldest first.
pub fn questions(_: Caller) -> GResult<Vec<Question>> {
    let m = game()?;
    let ctx = context(m)?;
    open(m, &ctx)?.iter().map(|w| read(m, w)).collect()
}

/// One question, by its id.
pub fn question(_: Caller, id: u32) -> GResult<Question> {
    let m = game()?;
    let ctx = context(m)?;
    read(m, &find(m, &ctx, id)?)
}

/// Answers a question with one of its options, as the player does in its
/// event window: the game has the answer once this returns, and applies it a
/// moment later; the window is closed, so the question is no longer listed.
pub fn answer(_: Caller, id: u32, option: u32) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let w = find(m, &ctx, id)?;
    let options = options(m, w.event)?;
    let Some(&chosen) = options.get(option as usize) else {
        return super::fail(Status::InvalidArgument, format!("question {id} has no option {option}"));
    };
    if !may_choose(m, chosen, w.scope)? {
        return super::fail(Status::Rejected, format!("the player may not choose option {option} of question {id}"));
    }
    m.call(m.at(CHOOSE), &[w.address, option as usize])?;
    if m.u8(w.address + window::OPEN)? != 0 {
        return Err(format!("the event window of question {id} stayed open").into());
    }
    Ok(())
}

/// How many questions wait in the campaign `ctx` was read from, also during
/// the daily update: it only reads memory, and calls neither the game nor
/// [`context`], which refuses then. Cheap: it reads no texts. Advancing time
/// stops on it (`super::time`), so on exactly the questions listed.
pub fn count(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    Ok(open(m, ctx)?.len())
}

/// An open event window whose pending event still waits.
struct Window {
    address: usize,
    id: u32,
    event: usize,
    scope: usize,
    deadline: i32,
}

/// The items of the game's list whose header is at `header`, skipping nodes
/// removed while the list was being walked.
pub(super) fn items(m: &Memory, header: usize) -> GResult<Vec<usize>> {
    let mut node = m.ptr(header + list::HEAD)?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    while node != 0 {
        if seen.len() >= MOST || !seen.insert(node) {
            return Err("list cycle".to_string().into());
        }
        let item = m.ptr(node + list::ITEM)?;
        if m.u8(node + list::REMOVED)? == 0 && item != 0 {
            out.push(item);
        }
        node = m.ptr(node + list::NEXT)?;
    }
    Ok(out)
}

/// The world's pending events: id, event, deadline.
fn pending(m: &Memory, ctx: &Ctx) -> GResult<Vec<(u32, usize, i32)>> {
    let count = m.i32(ctx.world + world::PENDING_EVENT_COUNT)?;
    if !(0..=MOST as i32).contains(&count) {
        return Err(format!("{count} pending events").into());
    }
    let data = m.ptr(ctx.world + world::PENDING_EVENTS)?;
    (0..count as usize)
        .map(|i| {
            let record = data + i * pending::SIZE;
            Ok((m.u32(record + pending::ID)?, m.ptr(record + pending::EVENT)?, m.i32(record + pending::DEADLINE)?))
        })
        .collect()
}

/// The open event windows among the map manager's popups whose pending event
/// still waits, oldest first. A window outlives its pending event when
/// something else answered it; choosing in it then would apply an option
/// again, so it is left out.
fn open(m: &Memory, ctx: &Ctx) -> GResult<Vec<Window>> {
    let manager = map_manager(m, ctx)?;
    let pending = pending(m, ctx)?;
    let mut out = Vec::new();
    for address in items(m, manager + campaign::map_manager::POPUPS)? {
        if m.ptr(address)? != m.at(classes::EVENT_WINDOW) || m.u8(address + window::OPEN)? == 0 {
            continue;
        }
        let id = m.u32(address + window::PENDING)?;
        let event = m.ptr(address + window::EVENT)?;
        if let Some(&(_, _, deadline)) = pending.iter().find(|&&(p, e, _)| p == id && e == event) {
            out.push(Window { address, id, event, scope: address + window::SCOPE, deadline });
        }
    }
    Ok(out)
}

fn find(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Window> {
    match open(m, ctx)?.into_iter().find(|w| w.id == id) {
        Some(w) => Ok(w),
        None => super::fail(Status::NotFound, format!("no question {id}")),
    }
}

fn options(m: &Memory, event: usize) -> GResult<Vec<usize>> {
    let begin = m.ptr(event + event::OPTIONS)?;
    let count = span(begin, m.ptr(event + event::OPTIONS + 8)?, 8, MOST_OPTIONS)?;
    (0..count).map(|i| m.ptr(begin + i * 8)).collect::<crate::Result<_>>().map_err(Into::into)
}

fn may_choose(m: &Memory, option: usize, scope: usize) -> GResult<bool> {
    Ok(m.call(m.at(OPTION_OPEN), &[option, scope])? as u8 != 0)
}

fn read(m: &Memory, w: &Window) -> GResult<Question> {
    let mut open = Vec::new();
    for (index, option) in options(m, w.event)?.into_iter().enumerate() {
        if may_choose(m, option, w.scope)? {
            open.push(QuestionOption { index: index as u32, text: localize(m, w.scope, &game_string::read(m, option + event_option::NAME)?)? });
        }
    }
    Ok(Question {
        id: w.id,
        event: game_text::to_utf8(&game_string::read(m, w.event + event::NAME)?),
        title: localize(m, w.scope, &game_string::read(m, w.event + event::TITLE)?)?,
        description: description(m, w)?,
        deadline: w.deadline,
        options: open,
    })
}

fn description(m: &Memory, w: &Window) -> GResult<String> {
    let mut key = game_string::empty();
    let slot = key.as_mut_ptr() as usize;
    m.call(m.at(DESCRIPTION_KEY), &[w.event, slot, w.scope])?;
    let read = game_string::read(m, slot);
    game_string::destroy_checked(m, slot)?;
    localize(m, w.scope, &read?)
}

/// The text of the localisation key `key` in `scope`, as the event window
/// shows it, in plain text (`game_text::plain`).
fn localize(m: &Memory, scope: usize, key: &[u8]) -> GResult<String> {
    // The key goes by value: a string of the game's own, which it destroys.
    let mut argument = game_string::empty();
    if !key.is_empty() {
        game_string::append_checked(m, argument.as_mut_ptr() as usize, key)?;
    }
    let mut result = game_string::empty();
    let slot = result.as_mut_ptr() as usize;
    m.call(m.at(LOCALIZE), &[slot, scope, argument.as_mut_ptr() as usize, NO_TAG as usize])?;
    let text = game_string::read(m, slot);
    game_string::destroy_checked(m, slot)?;
    Ok(game_text::plain(&text?))
}

/// How the game shows a question, the functions answering and reading
/// questions call, and how the event window calls them; the objects they
/// read are in `objects::campaign`.
pub fn points() -> Vec<Point> {
    vec![
        Point::method(classes::MAP_MANAGER, campaign::map_manager::SHOW_EVENT, SHOW_EVENT, "map manager shows one of the game's events"),
        // It makes the event window (0x10b9a50) and adds it to its own
        // popups (0x6ecae0, this in rcx).
        Point::keep(0x828c43usize, "4c 8b ce 4c 8b c5 48 8b d3 48 8b c8 e8 fc 0d 89 00 90 48 8b d0 48 8b cb e8 80 3e ec ff", "event window shown"),
        Point::keep(CHOOSE, "48 89 5c 24 08 48 89 74 24 10 55 57 41 56 48 8d 6c 24 c0", "event option chosen"),
        Point::keep(LOCALIZE, "48 8b c4 4c 89 48 20 4c 89 40 18 48 89 50 10 48 89 48 08 55 53 56 57 41 54 41 55 41 56 41 57", "event text"),
        Point::keep(0x487b4dusize, "41 b9 2d 2d 2d 00 4c 8b c0 48 8b d3 48 8d 4c 24 20 e8 fd 4c c3 00", "event text call").uses(&[NO_TAG as usize]),
        // The event window's description and options.
        Point::keep(DESCRIPTION_KEY, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 57 41 54 41", "event description key"),
        Point::keep(0x10bac67usize, "4d 8b c4 48 8d 55 f0 49 8b 4f 48 e8 39 33 63 ff", "event description").uses(&[window::EVENT]),
        Point::keep(OPTION_OPEN, "48 81 c1 a8 00 00 00 e9 c4 16 4a 00", "event option open"),
        Point::keep(0x10bb5bdusize, "49 8b d4 48 8b cf e8 68 b5 63 ff", "event option shown"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_s_removed_nodes_are_skipped() {
        let m = Memory { base: 0 };
        let things = [11u64, 22, 33];
        let mut nodes = [[0u64; 4]; 3];
        for (i, node) in nodes.iter_mut().enumerate() {
            node[0] = &things[i] as *const u64 as u64;
        }
        nodes[1][3] = 1;
        let addresses: Vec<u64> = nodes.iter_mut().map(|n| n.as_mut_ptr() as u64).collect();
        // The list is read through `m`, so its links are written through it.
        let link = |from: usize, to: usize| m.write(addresses[from] as usize + list::NEXT, &addresses[to].to_le_bytes()).unwrap();
        link(0, 1);
        link(1, 2);
        let header = [addresses[0], addresses[2], 3];
        let got = items(&m, header.as_ptr() as usize).unwrap();
        assert_eq!(got, [&things[0] as *const u64 as usize, &things[2] as *const u64 as usize]);
        // A cycle is an error, not a hang.
        link(2, 0);
        assert!(items(&m, header.as_ptr() as usize).is_err());
    }
}
