//! The national focus: which kind of monarch power the player's country
//! favors, as the court view's focus buttons set it
//! (docs/research/power-spending.md, "National focus").
use super::{action, power, technology::key, GResult, Session};
use crate::{
    game_string,
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::focus::args;
use eu4_harmony_remote::{
    choices::{Choice, PowerKind},
    focus::{Focus, FocusChoice},
};

/// A country's national focus: 0 to 2 a kind of power, 3 none.
const FOCUS: usize = 0x2868;
const NONE: i32 = 3;
/// Whether the country may change its focus now: `(country, text)`, the
/// text, when not null, given the line saying when it may.
const MAY_CHANGE: GameFunction<2> = GameFunction::at(0x3db490);

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_CHANGE, "40 55 53 56 57 41 56 48 8d 6c 24 90 48 81 ec 70 01 00 00 4c 8b f2 48 8b", "national focus may change"),
        // A focus button asks whether the focus may change, then reads the
        // focus to tell setting it from removing it.
        Point::keep(0xed663fusize, "48 8b cf e8 49 4e 50 ff 84 c0 0f 84 0a 02 00 00 8b 87 68 28 00 00", "focus button").uses(&[FOCUS]),
    ]
}

/// The focus of the country at `country`, None when none is set.
fn focus_of(m: &Memory, country: usize) -> GResult<Option<PowerKind>> {
    Ok(match m.i32(country + FOCUS)? {
        0 => Some(PowerKind::Adm),
        1 => Some(PowerKind::Dip),
        2 => Some(PowerKind::Mil),
        _ => None,
    })
}

/// Whether the focus may change now, and when not, the game's line saying
/// when it may.
fn may_change(m: &Memory, country: usize) -> GResult<(bool, Vec<String>)> {
    let (may, text) = game_string::written(m, |out| MAY_CHANGE.call(m, [country, out]))?;
    Ok((may & 0xff != 0, if may & 0xff != 0 { Vec::new() } else { power::plain_lines(&text) }))
}

fn choice((may, why): &(bool, Vec<String>)) -> Choice {
    if *may {
        Choice::can(Vec::new())
    } else {
        Choice::cannot(why.clone(), Vec::new())
    }
}

/// The player's focus and what the focus buttons offer now.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Focus> {
    let focus = focus_of(m, ctx.country)?;
    let may = may_change(m, ctx.country)?;
    let choose = PowerKind::ALL
        .into_iter()
        .map(|kind| FocusChoice { focus: kind, choice: if focus == Some(kind) { Choice::cannot(Vec::new(), Vec::new()) } else { choice(&may) } })
        .collect();
    let cancel = if focus.is_none() { Choice::cannot(Vec::new(), Vec::new()) } else { choice(&may) };
    Ok(Focus { focus, choose, cancel })
}

/// The focus command in its written form: the country by its tag and the
/// focus as a number, 3 for none, as the focus buttons make it.
fn written(country: [u8; 3], focus: i32) -> String {
    format!("set_national_focus_command={{ country=\"{}\" power={focus} }}", String::from_utf8_lossy(&country))
}

/// Sets the focus on `kind`, as its button does.
pub fn choose(Session { m, ctx, commands, .. }: Session, args::Choose { focus: kind }: args::Choose) -> GResult<()> {
    if focus_of(m, ctx.country)? == Some(kind) {
        return Err(checks::already(key(kind)));
    }
    let choice = choice(&may_change(m, ctx.country)?);
    action::carry_out(commands, &choice, "change the national focus", || Ok(written(ctx.player.letters(), kind as i32)))
}

/// Removes the focus, as pressing the button of the focus set does.
pub fn cancel(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    if focus_of(m, ctx.country)?.is_none() {
        return Err(checks::none_set());
    }
    let choice = choice(&may_change(m, ctx.country)?);
    action::carry_out(commands, &choice, "remove the national focus", || Ok(written(ctx.player.letters(), NONE)))
}

/// The refusals the framework words itself of the focus buttons.
mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of a focus on the kind of power (`adm`, `dip`, `mil`) it is
    /// on already.
    pub fn already(focus: &str) -> Fail {
        Fail::new(Status::Rejected, format!("the national focus is on {focus} already"))
    }

    /// The refusal of cancelling the focus when none is set.
    pub fn none_set() -> Fail {
        Fail::new(Status::Rejected, "no national focus is set")
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn a_focus_set_already_or_none_set_is_refused() {
            assert_eq!(super::already("adm").message, "the national focus is on adm already");
            assert_eq!(super::none_set().message, "no national focus is set");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;
    use crate::game::{
        buffers::{self, write_reason, Image, Object},
        commands::record,
        Caller, Fail, Status,
    };

    #[test]
    fn the_command_names_the_country_and_the_focus() {
        assert_eq!(super::written(*b"FRA", 1), r#"set_national_focus_command={ country="FRA" power=1 }"#);
        assert_eq!(super::written(*b"FRA", super::NONE), r#"set_national_focus_command={ country="FRA" power=3 }"#);
    }

    /// The focus may change.
    extern "C" fn may(_: usize, _: usize) -> usize {
        1
    }

    /// The focus may not change yet, as the game's line says.
    extern "C" fn may_not(_: usize, text: usize) -> usize {
        unsafe { write_reason(text, b"Wait until 1450") };
        0
    }

    struct Ottomans {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    /// The Ottomans, their focus `focus` (3 none), the game answering
    /// whether it may change with `check`.
    fn ottomans(focus: i32, check: extern "C" fn(usize, usize) -> usize) -> Ottomans {
        let image = Image::new();
        image.strings().function(MAY_CHANGE.rva(), check as *const ());
        let player = Object::country(tur(), FOCUS + 4).i32(FOCUS, focus);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), 0, tur(), player.at(), countries.at(), Caller::Remote);
        Ottomans { _image: image, _objects: vec![player, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_options_offer_every_other_focus_and_cancelling_the_one_set() {
        let focus = options(ottomans(1, may).session).unwrap();
        assert_eq!(focus.focus, Some(PowerKind::Dip));
        assert_eq!(focus.choose.iter().map(|c| (c.focus, c.choice.can)).collect::<Vec<_>>(), [(PowerKind::Adm, true), (PowerKind::Dip, false), (PowerKind::Mil, true)]);
        assert!(focus.cancel.can);
        let none = options(ottomans(NONE, may_not).session).unwrap();
        assert_eq!((none.focus, none.cancel.can), (None, false));
        assert_eq!(none.choose[0].choice.reasons, ["Wait until 1450"]);
    }

    #[test]
    fn a_focus_is_set_and_removed_by_the_buttons_command() {
        let ottomans = ottomans(1, may);
        choose(ottomans.session, args::Choose { focus: PowerKind::Mil }).unwrap();
        cancel(ottomans.session).unwrap();
        assert_eq!(record::posted(), [r#"set_national_focus_command={ country="TUR" power=2 }"#, r#"set_national_focus_command={ country="TUR" power=3 }"#]);
    }

    #[test]
    fn the_focus_set_already_none_to_cancel_or_a_change_too_soon_is_refused() {
        assert_eq!(refused(choose(ottomans(1, may).session, args::Choose { focus: PowerKind::Dip })), (Status::Rejected, "the national focus is on dip already".into()));
        assert_eq!(refused(cancel(ottomans(NONE, may).session)), (Status::Rejected, "no national focus is set".into()));
        assert_eq!(refused(choose(ottomans(NONE, may_not).session, args::Choose { focus: PowerKind::Adm })), (Status::Rejected, "Wait until 1450".into()));
        assert_eq!(refused(cancel(ottomans(0, may_not).session)), (Status::Rejected, "Wait until 1450".into()));
        assert!(record::posted().is_empty());
    }
}
