//! Offers (GLOSSARY): what other countries proposed to the player that waits
//! for an answer, read from the diplomacy dialogs the game holds for the
//! player, and answered as the player does, with the dialog's accept or
//! decline button. The evidence is in docs/research/questions-and-offers.md.
use super::{context, game, map_manager, questions, Caller, Ctx, GResult, Status, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::gui::{classes as gui_classes, element, functions as gui_functions, window as gui_window},
    points::Point,
    unicode::game_text,
};

/// The map manager's holder of the diplomatic message icons, which keeps
/// them in a list at [`ICONS`].
const ICON_HOLDER: usize = 0x1400;
const ICONS: usize = 0x4c78;
/// The dialogs the map manager has open from those icons, a list.
const OPEN_DIALOGS: usize = 0x370;

/// A diplomatic message icon.
mod icon {
    /// The dialog it opens when clicked; null once it has.
    pub const DIALOG: usize = 0x20;
    /// Nonzero once it is done with and goes, u8.
    pub const DONE: usize = 0x140;
}

/// A dialog (the game's base of every dialog with an agree and a decline
/// button).
mod dialog {
    /// Its window.
    pub const WINDOW: usize = 8;
    /// Nonzero once answered, u8: it goes.
    pub const DONE: usize = 0x114;
    /// Its methods, by vtable offset: what its agree and decline buttons
    /// call.
    pub const ACCEPT: usize = 0x50;
    pub const DECLINE: usize = 0x58;
    /// Whether it is still open to an answer, which its update asks.
    pub const VALID: usize = 0x60;
}

/// A diplomacy dialog: a dialog about a diplomatic action, whose own
/// constructor sets [`MARK`]: a callback object (its class, the dialog, a
/// function) none other has.
mod diplomacy {
    pub const MARK: usize = 0x1c0;
    pub const MARK_CONTEXT: usize = 0x1c8;
    pub const MARK_FUNCTION: usize = 0x1d0;
    /// What it calls when its popup button is clicked.
    pub const FUNCTION: usize = 0x1079f90;
    /// Its own copy of the diplomatic action.
    pub const ACTION: usize = 0x238;
}

/// A diplomatic action, as the country it is made to keeps it.
mod action {
    /// The country that made it, and the one it is made to (tags).
    pub const FROM: usize = 0x10;
    pub const TO: usize = 0x20;
    /// When it was made, i32.
    pub const DATE: usize = 0x28;
    /// 0 while unanswered, 1 declined, 2 accepted (i32).
    pub const STATE: usize = 0x34;
    /// Its id, i32, given when it reaches the player.
    pub const ID: usize = 0x3c;
}

/// A text box's text, a `std::string`.
const TEXT: usize = 0x170;

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
    held(m, &ctx)?.iter().map(|h| read(m, h)).collect()
}

/// One offer, by its id.
pub fn offer(_: Caller, id: u32) -> GResult<Offer> {
    let m = game()?;
    let ctx = context(m)?;
    read(m, &find(m, &ctx, id)?)
}

/// Accepts or declines an offer, as the player does with the buttons of its
/// dialog: the game has the answer once this returns, and applies it a
/// moment later; the dialog, or the icon holding it, goes, so the offer is no
/// longer listed.
pub fn answer(_: Caller, id: u32, accept: bool) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let w = find(m, &ctx, id)?;
    let method = m.vslot(w.dialog, if accept { dialog::ACCEPT } else { dialog::DECLINE })?;
    m.call(method, &[w.dialog])?;
    // An icon whose dialog was answered would stay, and its dialog could be
    // answered again: it goes as when it times out.
    if let Some(icon) = w.icon {
        m.write(icon + icon::DONE, &[1])?;
    }
    Ok(())
}

/// How many offers wait, for [`questions::waiting`].
pub(super) fn count(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    Ok(held(m, ctx)?.len())
}

/// A diplomacy dialog waiting for the player's answer, and the icon holding
/// it, if it is not open.
struct Held {
    dialog: usize,
    icon: Option<usize>,
    action: usize,
    id: u32,
}

/// The diplomacy dialogs whose actions, made to the player, wait for an
/// answer: those the message icons hold, then those open.
fn held(m: &Memory, ctx: &Ctx) -> GResult<Vec<Held>> {
    let manager = map_manager(m, ctx)?;
    let mut dialogs = Vec::new();
    let holder = m.ptr(manager + ICON_HOLDER)?;
    if holder != 0 {
        for icon in questions::items(m, holder + ICONS)? {
            let dialog = m.ptr(icon + icon::DIALOG)?;
            if m.u8(icon + icon::DONE)? == 0 && dialog != 0 {
                dialogs.push((dialog, Some(icon)));
            }
        }
    }
    dialogs.extend(questions::items(m, manager + OPEN_DIALOGS)?.into_iter().map(|dialog| (dialog, None)));
    let mut out: Vec<Held> = Vec::new();
    for (dialog, icon) in dialogs {
        if m.u8(dialog + dialog::DONE)? != 0 || !is_diplomacy(m, dialog)? {
            continue;
        }
        let action = m.ptr(dialog + diplomacy::ACTION)?;
        if action == 0 || m.i32(action + action::STATE)? != 0 || Tag(m.u64(action + action::TO)?).index() != ctx.player.index() {
            continue;
        }
        let id = m.u32(action + action::ID)?;
        if !out.iter().any(|w| w.id == id) {
            out.push(Held { dialog, icon, action, id });
        }
    }
    Ok(out)
}

/// Whether the dialog at `dialog` is a diplomacy dialog, by the mark its
/// constructor leaves.
fn is_diplomacy(m: &Memory, dialog: usize) -> GResult<bool> {
    Ok(m.ptr(dialog + diplomacy::MARK)? == m.at(gui_classes::CALLBACK)
        && m.ptr(dialog + diplomacy::MARK_CONTEXT)? == dialog
        && m.ptr(dialog + diplomacy::MARK_FUNCTION)? == m.at(diplomacy::FUNCTION))
}

fn find(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Held> {
    match held(m, ctx)?.into_iter().find(|w| w.id == id) {
        Some(w) => Ok(w),
        None => super::fail(Status::NotFound, format!("no offer {id}")),
    }
}

fn read(m: &Memory, w: &Held) -> GResult<Offer> {
    let window = m.ptr(w.dialog + dialog::WINDOW)?;
    Ok(Offer {
        id: w.id,
        from: Tag(m.u64(w.action + action::FROM)?),
        date: m.i32(w.action + action::DATE)?,
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
    Ok(game_text::plain(&game_string::read(m, found + TEXT)?))
}

/// The message icons, the open dialogs, the diplomacy dialogs and their
/// actions, and the text boxes the offers are read from.
pub fn points() -> Vec<Point> {
    vec![
        // Switching map modes hides every message icon.
        Point::keep(0x8221aausize, "48 8b 9e 00 14 00 00 48 8b 9b 78 4c 00 00", "message icons").uses(&[ICON_HOLDER, ICONS]),
        // A clicked icon hands its dialog to the open dialogs and is done.
        Point::keep(
            0x10807a6usize,
            "48 8b 89 00 1e 00 00 e8 5e c5 66 ff 48 c7 43 20 00 00 00 00 c6 83 40 01 00 00 01",
            "message icon clicked",
        )
        .uses(&[icon::DIALOG, icon::DONE]),
        // A popup that goes leaves the open dialogs too.
        Point::keep(0x6ea5deusize, "49 8b 8c 24 70 03 00 00 48 85 c9 74 79 0f 1f 44 00 00 48 39 39 74 0e", "open dialogs").uses(&[OPEN_DIALOGS]),
        // The diplomacy dialog's constructor leaves its mark.
        Point::keep(
            0x1066368usize,
            "48 8d 05 59 1d c5 00 48 89 83 c0 01 00 00 48 89 9b c8 01 00 00 48 8d 05 0c 3c 01 00 48 89 83 d0 01 00 00",
            "diplomacy dialog",
        )
        .uses(&[gui_classes::CALLBACK, diplomacy::MARK, diplomacy::MARK_CONTEXT, diplomacy::FUNCTION, diplomacy::MARK_FUNCTION]),
        // A dialog's update: its window, and whether it stays.
        Point::keep(0x10a090busize, "48 8b 4b 08", "dialog window").uses(&[dialog::WINDOW]),
        Point::keep(0x10a0941usize, "80 bb 14 01 00 00 00 75 15 48 8b 03 48 8b cb ff 50 60", "dialog done").uses(&[dialog::DONE, dialog::VALID]),
        // What the decline and agree buttons call.
        Point::keep(0x10a0f40usize, "48 8b 01 ff 60 58 cc cc 48 8b 01 ff 60 50", "dialog buttons").uses(&[dialog::DECLINE, dialog::ACCEPT]),
        // A diplomacy dialog asking whether its action is still unanswered,
        // and when it was made.
        Point::keep(0x1066b5dusize, "48 8b 83 38 02 00 00 83 78 34 00 0f 94 c0", "diplomacy dialog action").uses(&[diplomacy::ACTION, action::STATE]),
        Point::keep(0x1066b16usize, "48 8b 81 38 02 00 00 48 8b d9 48 8d 4c 24 30 8b 50 28", "diplomatic action date").uses(&[diplomacy::ACTION, action::DATE]),
        // The alliance dialog showing both countries.
        Point::keep(0x1067aacusize, "48 8b 96 38 02 00 00 48 8b 52 20", "diplomatic action recipient").uses(&[diplomacy::ACTION, action::TO]),
        Point::keep(0x1067b19usize, "49 8b 46 10 48 c1 e8 20", "diplomatic action maker").uses(&[action::FROM]),
        // An action reaching the player gets its id.
        Point::keep(0x4e82a9usize, "89 43 3c", "diplomatic action id").uses(&[action::ID]),
        // A dialog's description: its text box, by name, and the text.
        Point::keep(0x10a286fusize, "48 8b 01 48 8b da 48 8d 15 84 31 bc 00 ff 90 98 00 00 00 48 8b f0 48 8d b8 70 01 00 00", "dialog description")
            .uses(&[gui_window::FIND_TEXT, TEXT]),
    ]
}
