//! Offers (GLOSSARY): what other countries proposed to the player that waits
//! for an answer, read from the diplomacy dialogs the game holds for the
//! player, and answered as the player does, with the dialog's accept or
//! decline button. The evidence is in docs/research/questions-and-offers.md.
use super::{map_manager, Ctx, GResult, Session, Status, Tag};
use crate::containers;
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
};
use eu4_harmony_remote::{
    offers::{args, Offer},
    CountryTag, Date,
};

/// The offers waiting for the player's answer, those shown as icons first.
pub fn offers(Session { m, ctx, .. }: Session) -> GResult<Vec<Offer>> {
    listed(m, &ctx)?.iter().map(|h| read(m, h)).collect()
}

/// Accepts an offer, as its dialog's accept button does ([`answer`]).
pub fn accept(session: Session, args::Accept { offer }: args::Accept) -> GResult<()> {
    answer(session, offer, true)
}

/// Declines an offer, as its dialog's decline button does ([`answer`]).
pub fn decline(session: Session, args::Decline { offer }: args::Decline) -> GResult<()> {
    answer(session, offer, false)
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
pub fn answer(Session { m, ctx, .. }: Session, id: u32, accept: bool) -> GResult<()> {
    let Some(w) = held(m, &ctx)?.into_iter().find(|w| w.id == id) else {
        return Err(checks::not_found(id));
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

/// Whether the game still takes an answer, on `today`, to an offer made on
/// `made`: until [`diplomacy::ANSWER_MONTHS`] after it, as 0x1066b10 decides,
/// the check every diplomacy dialog about an action ends with.
fn in_time(made: i32, today: i32) -> bool {
    Date(made).months_later(diplomacy::ANSWER_MONTHS as u32).is_none_or(|until| until.0 >= today)
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
        for icon in containers::items(m, holder + message_icon_holder::ICONS, containers::MOST)? {
            let dialog = m.ptr(icon + icon::DIALOG)?;
            if m.u8(icon + icon::DONE)? == 0 && dialog != 0 {
                dialogs.push((dialog, Some(icon)));
            }
        }
    }
    dialogs.extend(containers::items(m, manager + campaign::map_manager::OPEN_DIALOGS, containers::MOST)?.into_iter().map(|dialog| (dialog, None)));
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
        from: CountryTag(Tag(m.u64(w.action + action::FROM)?).letters()),
        date: Date(w.made),

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
    let found = gui_functions::FIND_TEXT.call(m, [window, c.as_ptr() as usize])?;
    // A miss can come back as a shared fallback object: only a text box of
    // this name is it.
    if found == 0 || m.ptr(found)? != m.at(gui_classes::TEXT_BOX) || m.vslot(found, element::NAME)? != m.at(gui_functions::ELEMENT_NAME) {
        return Ok(String::new());
    }
    if game_string::read(m, gui_functions::ELEMENT_NAME.call(m, [found])?)? != name.as_bytes() {
        return Ok(String::new());
    }
    Ok(game_string::shown(m, found + text_box::TEXT)?)
}

/// The refusals the framework words itself of an offer.
mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of a number that names no offer waiting.
    pub fn not_found(offer: u32) -> Fail {
        Fail::new(Status::NotFound, format!("no offer {offer}"))
    }
}

/// The group's dependency points: none of its own.
pub fn points() -> Vec<crate::points::Point> {
    Vec::new()
}

crate::catalogue::bind! {
    eu4_harmony_remote::offers: OfferRequest => OfferAnswer;
    List => offers;
    Accept => accept;
    Decline => decline;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A date as the game keeps it, at midnight.
    fn date(year: i32, month: u32, day: u32) -> i32 {
        Date::from_ymd(year, month, day).unwrap().0
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

    use crate::game::{
        buffers::{self, Image, Object},
        Caller, Fail,
    };
    use crate::objects::gui::message_icon_holder;
    use std::cell::RefCell;

    /// A dialog the game's own check no longer lets be answered, in a byte
    /// it does not use, read by the check below.
    const STALE: usize = 8;

    thread_local! {
        /// The dialogs answered, each with whether it was accepted.
        static ANSWERED: RefCell<Vec<(usize, bool)>> = const { RefCell::new(Vec::new()) };
    }

    extern "C" fn valid(dialog: usize) -> usize {
        (unsafe { *((dialog + STALE) as *const u8) } == 0).into()
    }

    extern "C" fn accepted(dialog: usize) -> usize {
        ANSWERED.with(|a| a.borrow_mut().push((dialog, true)));
        0
    }

    extern "C" fn declined(dialog: usize) -> usize {
        ANSWERED.with(|a| a.borrow_mut().push((dialog, false)));
        0
    }

    /// Byzantium, to which the Ottomans' offers 7, open in its dialog, and
    /// 8, held by a message icon, wait for an answer, and 9 the game no
    /// longer takes; offer 10 was made to Venice.
    struct Byzantium {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
        dialogs: Vec<usize>,
        icon: usize,
    }

    fn byzantium() -> Byzantium {
        use crate::catalogue::fake::{byz, tur};
        let image = Image::new();
        let m = image.memory();
        image.function(gui_functions::ACTION_DIALOG_ACCEPT, accepted as *const ());
        let vtable = Object::new(dialog::VALID + 8)
            .u64(dialog::ACCEPT, m.at(gui_functions::ACTION_DIALOG_ACCEPT) as u64)
            .u64(dialog::DECLINE, declined as *const () as u64)
            .u64(dialog::VALID, valid as *const () as u64);
        let ven = Tag(u64::from_le_bytes(*b"VEN\0\0\0\0\0"));
        let dialog = |id: u32, to: Tag, stale: bool| {
            let action = Object::new(action::ID + 4).u64(action::FROM, tur().0).u64(action::TO, to.0).u32(action::ID, id);
            let dialog = Object::new(diplomacy::ACTION + 8).u64(0, vtable.at() as u64).u8(STALE, stale.into());
            let at = dialog.at() as u64;
            dialog
                .u64(diplomacy::MARK, m.at(gui_classes::CALLBACK) as u64)
                .u64(diplomacy::MARK_CONTEXT, at)
                .u64(diplomacy::MARK_FUNCTION, m.at(gui_functions::DIPLOMACY_POPUP) as u64)
                .points_to(diplomacy::ACTION, action)
        };
        let (open, held, stale, elsewhere) = (dialog(7, byz(), false), dialog(8, byz(), false), dialog(9, byz(), true), dialog(10, ven, false));
        let dialogs = vec![open.at(), held.at(), stale.at()];
        let icon = Object::new(icon::DONE + 8).points_to(icon::DIALOG, held);
        let icon_at = icon.at();
        let holder = Object::new(message_icon_holder::ICONS + 0x18).list(message_icon_holder::ICONS, vec![(icon, false)]);
        let manager = Object::new(campaign::map_manager::MESSAGE_ICON_HOLDER + 8)
            .u64(0, m.at(campaign::classes::MAP_MANAGER) as u64)
            .points_to(campaign::map_manager::MESSAGE_ICON_HOLDER, holder)
            .list(campaign::map_manager::OPEN_DIALOGS, vec![(open, false), (stale, false), (elsewhere, false)]);
        let world = Object::new(campaign::world::MAP_MANAGER + 8).points_to(campaign::world::MAP_MANAGER, manager);
        let country = Object::country(byz(), 0);
        let countries = Object::countries(&[&country]);
        let session = buffers::session(m, world.at(), byz(), country.at(), countries.at(), Caller::Remote);
        Byzantium { _image: image, _objects: vec![vtable, world, country, countries], session, dialogs, icon: icon_at }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the offer was answered") };
        (status, message)
    }

    #[test]
    fn an_offer_is_answered_with_its_dialog_s_buttons_and_its_icon_goes() {
        let byzantium = byzantium();
        answer(byzantium.session, 7, true).unwrap();
        answer(byzantium.session, 8, false).unwrap();
        assert_eq!(ANSWERED.with(|a| a.take()), [(byzantium.dialogs[0], true), (byzantium.dialogs[1], false)]);
        assert_eq!(unsafe { *((byzantium.icon + icon::DONE) as *const u8) }, 1);
    }

    #[test]
    fn an_offer_the_game_no_longer_takes_or_not_to_the_player_is_refused() {
        let byzantium = byzantium();
        assert_eq!(refused(answer(byzantium.session, 9, true)), (Status::Rejected, "the game no longer takes an answer to offer 9, and has let it go".into()));
        // The game's check let it go: its dialog is not asked again.
        assert_eq!(unsafe { *((byzantium.dialogs[2] + dialog::DONE) as *const u8) }, 1);
        assert_eq!(refused(answer(byzantium.session, 9, true)), (Status::NotFound, "no offer 9".into()));
        assert_eq!(refused(answer(byzantium.session, 10, true)), (Status::NotFound, "no offer 10".into()));
        assert!(ANSWERED.with(|a| a.take()).is_empty());
    }
}
