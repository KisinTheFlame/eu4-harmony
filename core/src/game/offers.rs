//! Offers (GLOSSARY): what other countries proposed to the player that waits
//! for an answer, read from the diplomacy dialogs the game holds for the
//! player, and answered as the player does, with the dialog's accept or
//! decline button. The evidence is in docs/research/questions-and-offers.md.
use super::{context, game, map_manager, questions, Caller, Ctx, GResult, Status, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign,
        diplomacy::action,
        gui::{
            classes as gui_classes, dialog, diplomacy_dialog as diplomacy, element, functions as gui_functions, message_icon as icon, message_icon_holder,
            text_box, window as gui_window,
        },
    },
    unicode::game_text,
};
use eu4_harmony_remote::{Date, MONTH_DAYS};

/// A proposal waiting for the player's answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    pub id: u32,
    pub from: Tag,
    /// When it was made, a day number.
    pub date: i32,
    pub title: String,
    pub description: String,
}

/// The offers waiting for the player's answer, those shown as icons first.
pub fn offers(_: Caller) -> GResult<Vec<Offer>> {
    let m = game()?;
    let ctx = context(m)?;
    listed(m, &ctx)?.iter().map(|h| read(m, h)).collect()
}

/// One offer, by its id.
pub fn offer(_: Caller, id: u32) -> GResult<Offer> {
    let m = game()?;
    let ctx = context(m)?;
    match listed(m, &ctx)?.into_iter().find(|w| w.id == id) {
        Some(w) => read(m, &w),
        None => super::fail(Status::NotFound, format!("no offer {id}")),
    }
}

/// Accepts or declines an offer, as the player does with the buttons of its
/// dialog: the game has the answer once this returns, and applies it a
/// moment later; the dialog, or the icon holding it, goes, so the offer is no
/// longer listed.
///
/// First the dialog's own check ([`dialog::VALID`]) is asked, as the game
/// asks it every frame and as soon as the player opens the dialog: when the
/// game no longer takes an answer (the offer was made more than two months
/// ago, or what it offers is no longer possible), the check lets the offer
/// go, mostly declining it, and the answer is refused. An icon does not ask
/// it while it flashes, the last days before it times out, so the player can
/// see an offer the game would decline at once; such an offer is not listed
/// ([`in_time`]).
pub fn answer(_: Caller, id: u32, accept: bool) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let Some(w) = held(m, &ctx)?.into_iter().find(|w| w.id == id) else {
        return super::fail(Status::NotFound, format!("no offer {id}"));
    };
    let open = m.call(m.vslot(w.dialog, dialog::VALID)?, &[w.dialog])? as u8 != 0;
    if open {
        let method = m.vslot(w.dialog, if accept { dialog::ACCEPT } else { dialog::DECLINE })?;
        m.call(method, &[w.dialog])?;
    } else {
        // The check let the offer go: the dialog must not be asked again, as
        // the game asks one not done every frame.
        m.write(w.dialog + dialog::DONE, &[1])?;
    }
    // An icon whose dialog was answered would stay, and its dialog could be
    // answered again: it goes as when it times out.
    if let Some(icon) = w.icon {
        m.write(icon + icon::DONE, &[1])?;
    }
    if !open {
        return super::fail(Status::Rejected, format!("the game no longer takes an answer to offer {id}, and has let it go"));
    }
    Ok(())
}

/// `date` `months` on, as the game adds months (0x14c8030): the same day of
/// the month, or the month's last when that month is shorter, at the same
/// hour (docs/research/dates.md: hours since 5000 BC, 365-day years).
fn months_after(date: i32, months: usize) -> i32 {
    let (year, month, day) = Date(date).ymd();
    let month = month as usize - 1 + months;
    let (year, month) = (year + (month / 12) as i32, month % 12);
    let day = day.min(MONTH_DAYS[month] as u32);
    Date::from_ymd(year, month as u32 + 1, day).map_or(i32::MAX, |on| on.0 + date.rem_euclid(24))
}

/// Whether the game still takes an answer, on `today`, to an offer made on
/// `made`: until [`diplomacy::ANSWER_MONTHS`] after it, as 0x1066b10 decides,
/// the check every diplomacy dialog about an action ends with.
fn in_time(made: i32, today: i32) -> bool {
    months_after(made, diplomacy::ANSWER_MONTHS) >= today
}

/// A diplomacy dialog waiting for the player's answer, and the icon holding
/// it, if it is not open.
struct Held {
    dialog: usize,
    icon: Option<usize>,
    action: usize,
    id: u32,
    /// When its action was made.
    made: i32,
}

/// The offers the game still takes an answer to, as [`held`] has them.
fn listed(m: &Memory, ctx: &Ctx) -> GResult<Vec<Held>> {
    Ok(held(m, ctx)?.into_iter().filter(|w| in_time(w.made, ctx.date)).collect())
}

/// The diplomacy dialogs whose actions, made to the player, wait for an
/// answer: those the message icons hold, then those open; also those the
/// game no longer takes an answer to but has not let go yet.
fn held(m: &Memory, ctx: &Ctx) -> GResult<Vec<Held>> {
    let manager = map_manager(m, ctx)?;
    let mut dialogs = Vec::new();
    let holder = m.ptr(manager + campaign::map_manager::MESSAGE_ICON_HOLDER)?;
    if holder != 0 {
        for icon in questions::items(m, holder + message_icon_holder::ICONS)? {
            let dialog = m.ptr(icon + icon::DIALOG)?;
            if m.u8(icon + icon::DONE)? == 0 && dialog != 0 {
                dialogs.push((dialog, Some(icon)));
            }
        }
    }
    dialogs.extend(questions::items(m, manager + campaign::map_manager::OPEN_DIALOGS)?.into_iter().map(|dialog| (dialog, None)));
    let mut out: Vec<Held> = Vec::new();
    for (dialog, icon) in dialogs {
        if m.u8(dialog + dialog::DONE)? != 0 || !is_about_action(m, dialog)? {
            continue;
        }
        let action = m.ptr(dialog + diplomacy::ACTION)?;
        if action == 0 || m.i32(action + action::STATE)? != 0 || Tag(m.u64(action + action::TO)?).index() != ctx.player.index() {
            continue;
        }
        let id = m.u32(action + action::ID)?;
        if !out.iter().any(|w| w.id == id) {
            out.push(Held { dialog, icon, action, id, made: m.i32(action + action::MADE)? });
        }
    }
    Ok(out)
}

/// Whether the dialog at `dialog` is a diplomacy dialog, by the mark its
/// constructor leaves, about a diplomatic action, by its accept method.
fn is_about_action(m: &Memory, dialog: usize) -> GResult<bool> {
    Ok(m.ptr(dialog + diplomacy::MARK)? == m.at(gui_classes::CALLBACK)
        && m.ptr(dialog + diplomacy::MARK_CONTEXT)? == dialog
        && m.ptr(dialog + diplomacy::MARK_FUNCTION)? == m.at(gui_functions::DIPLOMACY_POPUP)
        && m.vslot(dialog, dialog::ACCEPT)? == m.at(gui_functions::ACTION_DIALOG_ACCEPT))
}

fn read(m: &Memory, w: &Held) -> GResult<Offer> {
    let window = m.ptr(w.dialog + dialog::WINDOW)?;
    Ok(Offer {
        id: w.id,
        from: Tag(m.u64(w.action + action::FROM)?),
        date: w.made,
        title: text(m, window, "Title")?,
        description: text(m, window, "Description")?,
    })
}

/// The text of the dialog window's text box `name`, as the player sees it;
/// empty when it has none.
fn text(m: &Memory, window: usize, name: &str) -> GResult<String> {
    if window == 0 || m.vslot(window, gui_window::FIND_TEXT)? != m.at(gui_functions::FIND_TEXT) {
        return Err("a dialog window with an unexpected lookup method".to_string().into());
    }
    let c = format!("{name}\0");
    let found = m.call(m.at(gui_functions::FIND_TEXT), &[window, c.as_ptr() as usize])?;
    // A miss can come back as a shared fallback object: only a text box of
    // this name is it.
    if found == 0 || m.ptr(found)? != m.at(gui_classes::TEXT_BOX) || m.vslot(found, element::NAME)? != m.at(gui_functions::ELEMENT_NAME) {
        return Ok(String::new());
    }
    if game_string::read(m, m.call(m.at(gui_functions::ELEMENT_NAME), &[found])?)? != name.as_bytes() {
        return Ok(String::new());
    }
    Ok(game_text::plain(&game_string::read(m, found + text_box::TEXT)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A date as the game keeps it, at midnight.
    fn date(year: i32, month: u32, day: u32) -> i32 {
        Date::from_ymd(year, month, day).unwrap().0
    }

    #[test]
    fn months_on_keep_the_day_or_take_the_month_s_last() {
        assert_eq!(date(1444, 11, 11), 0x035D_7710);
        assert_eq!(months_after(date(1444, 11, 11), 2), date(1445, 1, 11));
        assert_eq!(months_after(date(1444, 12, 31), 2), date(1445, 2, 28));
        assert_eq!(months_after(date(1445, 1, 31), 2), date(1445, 3, 31));
        assert_eq!(months_after(date(1445, 8, 31), 2), date(1445, 10, 31));
        assert_eq!(months_after(date(1445, 10, 31), 2), date(1445, 12, 31));
        assert_eq!(months_after(date(1445, 12, 30), 2), date(1446, 2, 28));
        assert_eq!(months_after(date(1445, 3, 1) + 5, 2), date(1445, 5, 1) + 5);
        assert_eq!(months_after(date(1445, 1, 31), 1), date(1445, 2, 28));
        assert_eq!(months_after(date(1445, 11, 30), 15), date(1447, 2, 28));
        assert_eq!(months_after(date(1445, 5, 9), 0), date(1445, 5, 9));
    }

    /// The game takes an answer up to the day two months on and declines the
    /// offer after it; so an offer is listed until then, and left out after.
    #[test]
    fn an_offer_is_in_time_until_two_months_after_it_was_made() {
        let made = date(1470, 10, 15);
        assert!(in_time(made, made));
        assert!(in_time(made, date(1470, 12, 15)));
        assert!(!in_time(made, date(1470, 12, 16)));
        assert!(in_time(date(1470, 12, 31), date(1471, 2, 28)));
        assert!(!in_time(date(1470, 12, 31), date(1471, 3, 1)));
    }
}
