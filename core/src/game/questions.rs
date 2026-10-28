//! Questions (GLOSSARY): the game's own events waiting for the player's
//! answer, read from the event windows the game shows the player, and
//! answered as the player does, by choosing an option in the window.
//! [`waiting`] says whether a question or an offer ([`super::offers`]) waits.
//! The evidence is in docs/research/questions-and-offers.md.
use super::{context, game, map_manager, offers, span, Caller, Ctx, GResult, Status};
use crate::{
    game_string,
    memory::Memory,
    objects::{campaign::route_options::NO_TAG, list},
    points::Point,
    unicode::game_text,
};
use std::collections::BTreeSet;

/// The world's pending events: an array of [`pending::SIZE`]-byte records,
/// its data at [`PENDING`] and its length (i32) at [`PENDING_COUNT`].
const PENDING: usize = 0x1ea0;
const PENDING_COUNT: usize = 0x1eac;

/// A pending event: one shown to a player, which waits for the answer.
mod pending {
    /// The event.
    pub const EVENT: usize = 0x10;
    /// The last day to answer, i32; later, the event window picks an option.
    pub const DEADLINE: usize = 0x20;
    /// Its id, i32, unique in the save.
    pub const ID: usize = 0x24;
    pub const SIZE: usize = 0x28;
}

/// The map manager's popups: a [`list`] of the windows it shows, each asked
/// every frame, through [`UPDATE`] in its vtable, whether it stays.
pub(super) const POPUPS: usize = 0x78;
const UPDATE: usize = 0x18;

/// An event window, by its vtable.
const EVENT_WINDOW: usize = 0x1d590f8;

/// An event window's fields.
mod window {
    /// The event, the scope it happens in (an object of the window's own,
    /// copied from the pending event's) and the pending event's id (i32).
    pub const EVENT: usize = 0x48;
    pub const SCOPE: usize = 0x50;
    pub const PENDING: usize = 0x120;
    /// Nonzero while the window is open, u8; choosing an option clears it.
    pub const OPEN: usize = 0x124;
}

/// An event's fields.
mod event {
    /// Its id in the game's files (`flavor_fra.1`), a `std::string`.
    pub const NAME: usize = 0x20;
    /// Its options: a vector (begin, end) of pointers.
    pub const OPTIONS: usize = 0x790;
    /// The localisation key of its title, a `std::string`.
    pub const TITLE: usize = 0x7e0;
    /// The localisation key of an option's text, a `std::string`, in the
    /// option.
    pub const OPTION_NAME: usize = 0x88;
}

/// `(window, option)`: what clicking an option's button in an event window
/// does: sends the game the command that chooses it and closes the window.
const CHOOSE: usize = 0x10bc2d0;
/// `(result, scope, key, tag)`: the text of a localisation key, with the
/// scope's names filled in, as the event window shows it. Returns a
/// `std::string` in `result`; takes `key`, a `std::string`, by value and
/// destroys it; `tag` is [`NO_TAG`] wherever the game calls it.
const LOCALISE: usize = 0x10bc860;
/// `(event, result, scope)`: the localisation key of the event's description
/// in the scope, a `std::string` in `result`.
const DESCRIPTION_KEY: usize = 0x6edfb0;
/// `(option, scope)`: whether the player may choose the option, u8.
const OPTION_OPEN: usize = 0x6f6b30;

/// More than any game shows.
const MOST: usize = 10_000;
const MOST_OPTIONS: usize = 64;

/// A game event waiting for the player's answer.
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

/// What waits for the player's answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Waiting {
    pub questions: u32,
    pub offers: u32,
}

impl Waiting {
    /// Anything at all.
    pub fn any(&self) -> bool {
        self.questions + self.offers > 0
    }
}

/// How many questions and offers wait for the player's answer. Cheap: it
/// reads no texts. Advancing time stops on it.
pub fn waiting(_: Caller) -> GResult<Waiting> {
    let m = game()?;
    let ctx = context(m)?;
    Ok(Waiting { questions: open(m, &ctx)?.len() as u32, offers: offers::count(m, &ctx)? as u32 })
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
    let count = m.i32(ctx.world + PENDING_COUNT)?;
    if !(0..=MOST as i32).contains(&count) {
        return Err(format!("{count} pending events").into());
    }
    let data = m.ptr(ctx.world + PENDING)?;
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
    for address in items(m, manager + POPUPS)? {
        if m.ptr(address)? != m.at(EVENT_WINDOW) || m.u8(address + window::OPEN)? == 0 {
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
            open.push(QuestionOption { index: index as u32, text: localise(m, w.scope, &game_string::read(m, option + event::OPTION_NAME)?)? });
        }
    }
    Ok(Question {
        id: w.id,
        event: game_text::to_utf8(&game_string::read(m, w.event + event::NAME)?),
        title: localise(m, w.scope, &game_string::read(m, w.event + event::TITLE)?)?,
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
    localise(m, w.scope, &read?)
}

/// The text of the localisation key `key` in `scope`, as the event window
/// shows it, without color codes.
fn localise(m: &Memory, scope: usize, key: &[u8]) -> GResult<String> {
    // The key goes by value: a string of the game's own, which it destroys.
    let mut argument = game_string::empty();
    if !key.is_empty() {
        game_string::append_checked(m, argument.as_mut_ptr() as usize, key)?;
    }
    let mut result = game_string::empty();
    let slot = result.as_mut_ptr() as usize;
    m.call(m.at(LOCALISE), &[slot, scope, argument.as_mut_ptr() as usize, NO_TAG as usize])?;
    let text = game_string::read(m, slot);
    game_string::destroy_checked(m, slot)?;
    Ok(plain(&text?))
}

/// Game text as Unicode, without the game's color codes (`§` and the letter
/// after it), which the player never sees.
pub(super) fn plain(text: &[u8]) -> String {
    let unicode = game_text::to_utf8(text);
    let mut out = String::with_capacity(unicode.len());
    let mut chars = unicode.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// The pending events, the event windows and the functions answering and
/// reading questions use.
pub fn points() -> Vec<Point> {
    vec![
        // Removing a pending event: the world's array, each record's id.
        Point::keep(
            0x7570b8usize,
            "4c 63 81 ac 1e 00 00 33 ed 4c 8b f1 45 85 c0 0f 8e 82 01 00 00 4c 8b 91 a0 1e 00 00 45 33 c9 49 8d 42 24 0f 1f 44 00 00 \
             39 10 74 16 ff c5 49 ff c1 48 83 c0 28",
            "pending events",
        )
        .uses(&[PENDING_COUNT, PENDING, pending::ID, pending::SIZE]),
        // Adding one: the event, its scope, the deadline and the id.
        Point::keep(
            0x757058usize,
            "49 63 c0 48 8d 0c 80 48 8b 45 08 48 8d 14 c8 41 8d 40 01 89 45 14 8b 44 24 28 89 42 08 4c 89 22 48 8b 44 24 30 48 89 42 10 \
             4c 89 72 18 8b 44 24 40 89 42 20 89 7a 24",
            "pending event record",
        )
        .uses(&[pending::EVENT, pending::DEADLINE, pending::ID]),
        // The map manager asking each popup whether it stays.
        Point::keep(0x6ea4e7usize, "49 8b 5c 24 78 48 85 db 74 58 48 8b 75 f7 48 8b 0b 48 8b 01 ff 50 18", "popups").uses(&[POPUPS, UPDATE]),
        Point::keep(0x6ea53dusize, "48 8b 43 10", "next popup").uses(&[list::NEXT]),
        // The event window's constructor: its class, the event, the scope,
        // the pending event's id, open.
        Point::keep(
            0x10b9b3busize,
            "48 8d 05 b6 f5 c9 00 48 89 03 48 8d 05 94 f5 c9 00 48 89 43 08 c7 43 30 ff ff ff ff 33 f6 48 89 73 38 48 89 7b 40 \
             4c 89 7b 48 48 8d 4b 50 49 8b d6 e8 2f 5b 01 ff 90 8b 45 60 89 83 20 01 00 00 40 88 b3 24 01 00 00",
            "event window",
        )
        .uses(&[EVENT_WINDOW, window::EVENT, window::SCOPE, window::PENDING, window::OPEN]),
        Point::keep(CHOOSE, "48 89 5c 24 08 48 89 74 24 10 55 57 41 56 48 8d 6c 24 c0", "event option chosen"),
        // Choosing logs the event's id, and closes the window.
        Point::keep(0x10bc44cusize, "48 8b 56 48 48 83 c2 20", "event id").uses(&[window::EVENT, event::NAME]),
        Point::keep(0x10bc54busize, "c6 86 24 01 00 00 00", "event window closed").uses(&[window::OPEN]),
        Point::keep(LOCALISE, "48 8b c4 4c 89 48 20 4c 89 40 18 48 89 50 10 48 89 48 08 55 53 56 57 41 54 41 55 41 56 41 57", "event text"),
        Point::keep(0x487b4dusize, "41 b9 2d 2d 2d 00 4c 8b c0 48 8b d3 48 8d 4c 24 20 e8 fd 4c c3 00", "event text call").uses(&[NO_TAG as usize]),
        // The event window's title, description and options.
        Point::keep(0x10ba2bfusize, "49 8b 57 48 48 81 c2 e0 07 00 00", "event title").uses(&[window::EVENT, event::TITLE]),
        Point::keep(DESCRIPTION_KEY, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 57 41 54 41", "event description key"),
        Point::keep(0x10bac67usize, "4d 8b c4 48 8d 55 f0 49 8b 4f 48 e8 39 33 63 ff", "event description").uses(&[window::EVENT]),
        Point::keep(0x6f2778usize, "48 8b 80 98 07 00 00 48 2b 87 90 07 00 00 48 c1 f8 03", "event options").uses(&[event::OPTIONS + 8, event::OPTIONS]),
        Point::keep(OPTION_OPEN, "48 81 c1 a8 00 00 00 e9 c4 16 4a 00", "event option open"),
        Point::keep(0x10bb5bdusize, "49 8b d4 48 8b cf e8 68 b5 63 ff", "event option shown"),
        Point::keep(0x10bb6a8usize, "48 8d 97 88 00 00 00 48 8d 4d b8 e8 c8 9b fd fe", "event option text").uses(&[event::OPTION_NAME]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_codes_are_left_out_and_game_text_becomes_unicode() {
        assert_eq!(plain(b"\xa7YParis\xa7! is ours"), "Paris is ours");
        assert_eq!(plain("中国\u{a7}R！".as_bytes()), "中国！");
        // A lone 1252 byte is its character.
        assert_eq!(plain(b"caf\xe9 \x93quoted\x94"), "café \u{201c}quoted\u{201d}");
        assert_eq!(plain(b"ends with \xa7"), "ends with ");
    }

    #[test]
    fn a_list_s_removed_nodes_are_skipped() {
        let m = Memory { base: 0 };
        let things = [11u64, 22, 33];
        let mut nodes = [[0u64; 4]; 3];
        for (i, node) in nodes.iter_mut().enumerate() {
            node[0] = &things[i] as *const u64 as u64;
        }
        nodes[1][3] = 1;
        let addresses: Vec<u64> = nodes.iter().map(|n| n.as_ptr() as u64).collect();
        nodes[0][2] = addresses[1];
        nodes[1][2] = addresses[2];
        let header = [addresses[0], addresses[2], 3];
        let got = items(&m, header.as_ptr() as usize).unwrap();
        assert_eq!(got, [&things[0] as *const u64 as usize, &things[2] as *const u64 as usize]);
        // A cycle is an error, not a hang.
        nodes[2][2] = addresses[0];
        assert!(items(&m, header.as_ptr() as usize).is_err());
    }
}
