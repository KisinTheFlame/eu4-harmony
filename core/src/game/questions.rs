//! Questions (GLOSSARY): the game's own events waiting for the player's
//! answer, read from the event windows the game shows the player, and
//! answered as the player does, by choosing an option in the window.
//! [`count`] says how many wait, for advancing time too. The evidence is in
//! docs/research/questions-and-offers.md.
use super::{map_manager, Ctx, GResult, Session};
use crate::containers;
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{self, classes, event, event_option, event_window as window, pending_event as pending, route_options::NO_TAG, world},
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::{
    questions::{args, Question, QuestionOption},
    Date,
};

/// `(window, option)`: what clicking an option's button in an event window
/// does: sends the game the command that chooses it and closes the window.
const CHOOSE: GameFunction<2> = GameFunction::at(0x10bc2d0);
/// `(result, scope, key, tag)`: the text of a localisation key, with the
/// scope's names filled in, as the event window shows it. Returns a
/// `std::string` in `result`; takes `key`, a `std::string`, by value and
/// destroys it; `tag` is [`NO_TAG`] wherever the game calls it.
const LOCALIZE: GameFunction<4> = GameFunction::at(0x10bc860);
/// `(event, result, scope)`: the localisation key of the event's description
/// in the scope, a `std::string` in `result`.
const DESCRIPTION_KEY: GameFunction<3> = GameFunction::at(0x6edfb0);
/// `(option, scope)`: whether the player may choose the option, u8.
const OPTION_OPEN: GameFunction<2> = GameFunction::at(0x6f6b30);
/// The map manager's method that shows one of the game's events to the
/// player ([`campaign::map_manager::SHOW_EVENT`]): the window it makes is
/// added to the popups, where questions are read from.
const SHOW_EVENT: usize = 0x828c00;

/// More than any game shows.
const MOST: usize = 10_000;
const MOST_OPTIONS: usize = 64;

/// The questions waiting for the player's answer, oldest first.
pub fn questions(Session { m, ctx, .. }: Session) -> GResult<Vec<Question>> {
    open(m, &ctx)?.iter().map(|w| read(m, w)).collect()
}

/// Answers a question with one of its options, as the player does in its
/// event window: the game has the answer once this returns, and applies it a
/// moment later; the window is closed, so the question is no longer listed.
pub fn answer(Session { m, ctx, .. }: Session, args::Answer { question, option }: args::Answer) -> GResult<()> {
    let w = find(m, &ctx, question)?;
    let options = options(m, w.event)?;
    let Some(&chosen) = options.get(option as usize) else {
        return Err(checks::no_option(question, option));
    };
    if !may_choose(m, chosen, w.scope)? {
        return Err(checks::not_allowed(question, option));
    }
    CHOOSE.call(m, [w.address, option as usize])?;
    if m.u8(w.address + window::OPEN)? != 0 {
        return Err(format!("the event window of question {question} stayed open").into());
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
    for address in containers::items(m, manager + campaign::map_manager::POPUPS, MOST)? {
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
        None => Err(checks::not_found(id)),
    }
}

fn options(m: &Memory, event: usize) -> GResult<Vec<usize>> {
    let begin = m.ptr(event + event::OPTIONS)?;
    let count = containers::span(begin, m.ptr(event + event::OPTIONS + 8)?, 8, MOST_OPTIONS)?;
    (0..count).map(|i| m.ptr(begin + i * 8)).collect::<crate::Result<_>>().map_err(Into::into)
}

fn may_choose(m: &Memory, option: usize, scope: usize) -> GResult<bool> {
    Ok(OPTION_OPEN.call(m, [option, scope])? as u8 != 0)
}

fn read(m: &Memory, w: &Window) -> GResult<Question> {
    let mut open = Vec::new();
    for (index, option) in options(m, w.event)?.into_iter().enumerate() {
        if may_choose(m, option, w.scope)? {
            open.push(QuestionOption { option: index as u32, text: localize(m, w.scope, &game_string::read(m, option + event_option::NAME)?)? });
        }
    }
    Ok(Question {
        id: w.id,
        event: game_string::key(m, w.event + event::NAME)?,
        title: localize(m, w.scope, &game_string::read(m, w.event + event::TITLE)?)?,
        description: description(m, w)?,
        deadline: Date(w.deadline),

        options: open,
    })
}

fn description(m: &Memory, w: &Window) -> GResult<String> {
    let (_, key) = game_string::written(m, |slot| DESCRIPTION_KEY.call(m, [w.event, slot, w.scope]))?;
    localize(m, w.scope, &key)
}

/// The text of the localisation key `key` in `scope`, as the event window
/// shows it, in plain text (`game_text::plain`).
fn localize(m: &Memory, scope: usize, key: &[u8]) -> GResult<String> {
    // The key goes by value: a string of the game's own, which it destroys.
    let mut argument = game_string::empty();
    if !key.is_empty() {
        game_string::append_checked(m, argument.as_mut_ptr() as usize, key)?;
    }
    let (_, text) = game_string::written(m, |slot| LOCALIZE.call(m, [slot, scope, argument.as_mut_ptr() as usize, NO_TAG as usize]))?;
    Ok(game_text::plain(&text))
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
    use crate::game::{
        buffers::{self, Image, Object},
        Caller, Fail, Status, Tag,
    };
    use std::cell::RefCell;

    /// An option the game does not let the player choose, and an event
    /// window that stays open when an option is chosen in it, in bytes
    /// neither uses, read by the game's functions below.
    const CLOSED: usize = 8;
    const STICKS: usize = 8;

    thread_local! {
        /// The options chosen, each with its window.
        static CHOSEN: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
    }

    extern "C" fn choose(window: usize, option: usize) -> usize {
        CHOSEN.with(|c| c.borrow_mut().push((window, option)));
        if unsafe { *((window + STICKS) as *const u8) } == 0 {
            unsafe { *((window + window::OPEN) as *mut u8) = 0 };
        }
        0
    }

    extern "C" fn option_open(option: usize) -> usize {
        (unsafe { *((option + CLOSED) as *const u8) } == 0).into()
    }

    /// France's questions: 3 (`flavor.1`, whose second option is closed)
    /// and 5 (its window stays open when chosen in), shown in event windows;
    /// and the window of 4, whose pending event no longer waits.
    struct France {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
        windows: [usize; 3],
    }

    fn france() -> France {
        let image = Image::new();
        let m = image.memory();
        image.function(CHOOSE.rva(), choose as *const ()).function(OPTION_OPEN.rva(), option_open as *const ());
        let options = [Object::new(CLOSED + 8), Object::new(CLOSED + 8).u8(CLOSED, 1)];
        let event = Object::new(event::OPTIONS + 0x10).vector(event::OPTIONS, &options.iter().map(|o| o.at() as u64).collect::<Vec<_>>());
        let window = |pending: u32, sticks: bool| {
            Object::new(window::OPEN + 8)
                .u64(0, m.at(classes::EVENT_WINDOW) as u64)
                .u64(window::EVENT, event.at() as u64)
                .u32(window::PENDING, pending)
                .u8(window::OPEN, 1)
                .u8(STICKS, sticks.into())
        };
        let windows = [window(3, false), window(4, false), window(5, true)];
        let at = [windows[0].at(), windows[1].at(), windows[2].at()];
        let manager = Object::new(campaign::map_manager::POPUPS + 0x18)
            .u64(0, m.at(classes::MAP_MANAGER) as u64)
            .list(campaign::map_manager::POPUPS, windows.into_iter().map(|w| (w, false)).collect());
        let records = Object::new(2 * pending::SIZE);
        for (i, id) in [3u32, 5].into_iter().enumerate() {
            let at = i * pending::SIZE;
            records.set(at + pending::EVENT, &(event.at() as u64).to_le_bytes());
            records.set(at + pending::ID, &id.to_le_bytes());
            records.set(at + pending::DEADLINE, &100i32.to_le_bytes());
        }
        let world = Object::new(world::PENDING_EVENT_COUNT + 4)
            .points_to(world::MAP_MANAGER, manager)
            .u64(world::PENDING_EVENTS, records.at() as u64)
            .i32(world::PENDING_EVENT_COUNT, 2);
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        let country = Object::country(fra, 0);
        let countries = Object::countries(&[&country]);
        let session = buffers::session(m, world.at(), fra, country.at(), countries.at(), Caller::Remote);
        let mut objects = vec![event, records, world, country, countries];
        objects.extend(options);
        France { _image: image, _objects: objects, session, windows: at }
    }

    fn answering(france: &France, question: u32, option: u32) -> GResult<()> {
        answer(france.session, args::Answer { question, option })
    }

    #[test]
    fn a_question_is_answered_by_choosing_the_option_in_its_window() {
        let france = france();
        answering(&france, 3, 0).unwrap();
        assert_eq!(CHOSEN.with(|c| c.take()), [(france.windows[0], 0)]);
        // Its window is closed: the question is no longer listed.
        assert_eq!(count(france.session.m, &france.session.ctx).unwrap(), 1);
    }

    #[test]
    fn a_question_or_an_option_the_window_does_not_offer_is_refused() {
        let france = france();
        let refused = |question, option| {
            let Err(Fail { status, message }) = answering(&france, question, option) else { panic!("question {question} was answered") };
            (status, message)
        };
        assert_eq!(refused(3, 1), (Status::Rejected, "the player may not choose option 1 of question 3".into()));
        assert_eq!(refused(3, 2), (Status::InvalidArgument, "question 3 has no option 2".into()));
        // Something else answered 4: its window is left alone.
        assert_eq!(refused(4, 0), (Status::NotFound, "no question 4".into()));
        assert_eq!(refused(9, 0), (Status::NotFound, "no question 9".into()));
        assert!(CHOSEN.with(|c| c.take()).is_empty());
        assert_eq!(refused(5, 0), (Status::Internal, "the event window of question 5 stayed open".into()));
    }
}

/// The refusals the framework words itself of a question and its options.
mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of a number that names no question waiting.
    pub fn not_found(question: u32) -> Fail {
        Fail::new(Status::NotFound, format!("no question {question}"))
    }

    /// The refusal of an option a question does not have.
    pub fn no_option(question: u32, option: u32) -> Fail {
        Fail::new(Status::InvalidArgument, format!("question {question} has no option {option}"))
    }

    /// The refusal of an option of a question the player may not choose.
    pub fn not_allowed(question: u32, option: u32) -> Fail {
        Fail::new(Status::Rejected, format!("the player may not choose option {option} of question {question}"))
    }
}
