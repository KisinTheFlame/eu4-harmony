//! Achievement rules (ADR achievement-rules-can-be-relaxed): the framework
//! settings that let the game give achievements outside ironman and with a game
//! checksum (GLOSSARY) that is not vanilla's. Every other rule stays the game's
//! own. How the game decides is in docs/research/achievements.md.
//!
//! - Outside ironman: the game's check is always asked to ignore ironman, as
//!   the single player setup already asks it, and that setup's text about
//!   achievements stops wanting its own ironman checkbox.
//! - With a changed game checksum: the game is told its game checksum is
//!   vanilla's wherever it keeps the answer, which the check, the save
//!   checksum's salt and the ironman load in a modded game all read.
//!
//! The settings are followed as they are loaded and saved; the settings window
//! opens only in the main menu, so the next session is the first to see a
//! change.
use crate::called_by_game;
use crate::{
    objects::campaign::achievements,
    points::{Function, Point},
    settings::Settings,
};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

/// `(achievements, reasons, ignore ironman, ignore player) -> bool`: the
/// game's check of whether the current game can earn achievements, which
/// fills in the world's part and asks `0x96900`; `reasons`, when not null,
/// receives why not.
const CHECK: usize = 0x96810;
/// `(achievements, vanilla)`: stores whether the game checksum is vanilla's,
/// once a process, as startup has taken it.
const CHECKSUM_KNOWN: usize = 0x97770;
/// In the single player setup's update of its text about achievements
/// (`0x13b6cc0`, the panel in `rsi`): the panel's ironman checkbox wanted
/// before the text says achievements can be earned, and before it lists why
/// not; each goes on where the checkbox is set or where it is not.
const CAN_BE_EARNED: usize = 0x13b6db7;
const CAN_BE_EARNED_IRONMAN: usize = 0x13b6dc0;
const CAN_BE_EARNED_NOT_IRONMAN: usize = 0x13b6e36;
const REASONS: usize = 0x13b6eec;
const REASONS_IRONMAN: usize = 0x13b6ef5;
const REASONS_NOT_IRONMAN: usize = 0x13b6f22;
/// The setup panel's ironman checkbox (u8).
const PANEL_IRONMAN: usize = 0x280;

/// Achievements outside ironman, as the settings say.
static OUTSIDE_IRONMAN: AtomicBool = AtomicBool::new(false);
/// Achievements with a changed game checksum, as the settings say.
static CHANGED_CHECKSUM: AtomicBool = AtomicBool::new(false);
/// What startup found of the game checksum: 0 until it is known, then
/// [`NOT_VANILLA`] or [`VANILLA`].
static GAME_CHECKSUM: AtomicU8 = AtomicU8::new(0);
const NOT_VANILLA: u8 = 1;
const VANILLA: u8 = 2;
/// The campaign's achievements, where the game keeps the answer; set before
/// [`GAME_CHECKSUM`].
static KEPT_IN: AtomicUsize = AtomicUsize::new(0);

/// Takes up the achievement rules of `settings`, at startup and whenever the
/// player saves them.
pub fn follow(settings: &Settings) {
    OUTSIDE_IRONMAN.store(settings.achievements_outside_ironman, Ordering::Relaxed);
    CHANGED_CHECKSUM.store(settings.achievements_with_changed_checksum, Ordering::Relaxed);
    // Once startup has taken the game checksum, the game keeps the answer
    // and reads it again only from where it keeps it.
    let found = GAME_CHECKSUM.load(Ordering::Acquire);
    let Some(m) = crate::memory().filter(|_| found != 0) else { return };
    let answer = taken_as_vanilla(found == VANILLA);
    let at = KEPT_IN.load(Ordering::Acquire) + achievements::VANILLA_GAME_CHECKSUM;
    if let Err(e) = m.write(at, &[answer as u8]) {
        log!("achievements: the game checksum's answer was not changed: {e}");
    }
}

/// Whether the game takes its game checksum as vanilla's, when it is so or
/// not.
fn taken_as_vanilla(vanilla: bool) -> bool {
    vanilla || CHANGED_CHECKSUM.load(Ordering::Relaxed)
}

type Check = unsafe extern "C" fn(usize, usize, u8, u8) -> u8;
static ORIGINAL_CHECK: Function<Check> = Function::new();

#[called_by_game]
unsafe extern "C" fn check(achievements: usize, reasons: usize, ignore_ironman: u8, ignore_player: u8) -> u8 {
    let ignore_ironman = ignore_ironman | OUTSIDE_IRONMAN.load(Ordering::Relaxed) as u8;
    unsafe { ORIGINAL_CHECK.get()(achievements, reasons, ignore_ironman, ignore_player) }
}

type ChecksumKnown = unsafe extern "C" fn(usize, u8) -> usize;
static ORIGINAL_CHECKSUM_KNOWN: Function<ChecksumKnown> = Function::new();

#[called_by_game]
unsafe extern "C" fn checksum_known(achievements: usize, vanilla: u8) -> usize {
    let vanilla = vanilla != 0;
    KEPT_IN.store(achievements, Ordering::Release);
    GAME_CHECKSUM.store(if vanilla { VANILLA } else { NOT_VANILLA }, Ordering::Release);
    let answer = taken_as_vanilla(vanilla);
    log!("achievements: the game checksum is {}vanilla's, taken as {}vanilla's", if vanilla { "" } else { "not " }, if answer { "" } else { "not " });
    unsafe { ORIGINAL_CHECKSUM_KNOWN.get()(achievements, answer as u8) }
}

crate::resume!(CAN_BE_EARNED_RESUME: CAN_BE_EARNED_YES = CAN_BE_EARNED_IRONMAN, CAN_BE_EARNED_NO = CAN_BE_EARNED_NOT_IRONMAN);

/// The setup's `cmp byte ptr [rsi + 0x280], 0; je`, with achievements
/// outside ironman taken as the checkbox set.
#[unsafe(naked)]
extern "C" fn can_be_earned_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rip + {outside}], 0",
        "jne 2f",
        "cmp byte ptr [rsi + {ironman}], 0",
        "jne 2f",
        "jmp qword ptr [rip + {no}]",
        "2:",
        "jmp qword ptr [rip + {yes}]",
        outside = sym OUTSIDE_IRONMAN,
        ironman = const PANEL_IRONMAN,
        yes = sym CAN_BE_EARNED_YES,
        no = sym CAN_BE_EARNED_NO,
    );
}

crate::resume!(REASONS_RESUME: REASONS_YES = REASONS_IRONMAN, REASONS_NO = REASONS_NOT_IRONMAN);

/// The same, before the setup lists why achievements cannot be earned.
#[unsafe(naked)]
extern "C" fn reasons_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rip + {outside}], 0",
        "jne 2f",
        "cmp byte ptr [rsi + {ironman}], 0",
        "jne 2f",
        "jmp qword ptr [rip + {no}]",
        "2:",
        "jmp qword ptr [rip + {yes}]",
        outside = sym OUTSIDE_IRONMAN,
        ironman = const PANEL_IRONMAN,
        yes = sym REASONS_YES,
        no = sym REASONS_NO,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(CHECK, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", "achievement check", check as Check, &ORIGINAL_CHECK),
        // The check hands its third argument on as `0x96900`'s fourth, which
        // passes ironman or not when it is set.
        Point::keep(0x96837usize, "45 0f b6 f8", "achievement check keeps ignore ironman"),
        Point::keep(0x96898usize, "45 0f b6 cf", "achievement check hands on ignore ironman"),
        Point::keep(0x9698busize, "45 84 c9 75 0a 40 38 7a 09 0f 84 dc 00 00 00", "achievements ignore ironman"),
        Point::replace(
            CHECKSUM_KNOWN,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20",
            "game checksum known",
            checksum_known as ChecksumKnown,
            &ORIGINAL_CHECKSUM_KNOWN,
        ),
        Point::keep(0x97786usize, "0f b6 fa", "game checksum answer"),
        Point::keep(0x977aeusize, "40 88 7e 02", "game checksum answer kept").uses(&[achievements::VANILLA_GAME_CHECKSUM]),
        Point::keep(0x977ccusize, "40 88 79 02", "game checksum answer kept without the platform").uses(&[achievements::VANILLA_GAME_CHECKSUM]),
        // The save checksum's salt follows the same answer.
        Point::keep(0x5ce788usize, "80 78 03 00 75 1b 80 78 02 00 74 06 80 78 01 00 75 0f", "save checksum salt").uses(&[achievements::VANILLA_GAME_CHECKSUM]),
        Point::jump(CAN_BE_EARNED, "80 be 80 02 00 00 00 74 76", "setup achievements text", can_be_earned_stub as *const () as usize, &CAN_BE_EARNED_RESUME)
            .uses(&[PANEL_IRONMAN]),
        Point::keep(CAN_BE_EARNED_IRONMAN, "84 c0 74 72", "setup achievements can be earned"),
        Point::keep(CAN_BE_EARNED_NOT_IRONMAN, "45 33 c9 45 33 c0", "setup achievements cannot be earned"),
        Point::jump(REASONS, "80 be 80 02 00 00 00 74 2d", "setup achievement reasons", reasons_stub as *const () as usize, &REASONS_RESUME)
            .uses(&[PANEL_IRONMAN]),
        Point::keep(REASONS_IRONMAN, "e8 86 f1 cd fe", "setup achievement reasons listed"),
        Point::keep(REASONS_NOT_IRONMAN, "48 8b 8e 90 02 00 00", "setup achievement reasons skipped"),
    ]
}
