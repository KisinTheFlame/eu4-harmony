//! Diplomacy's objects: a diplomatic action, the player's or one made to the
//! player (an offer, C7's), the command that carries one to the game's
//! session, the war goals a declaration of war chooses from, and the peace
//! treaty a peace offer holds (docs/research/diplomacy.md,
//! docs/research/questions-and-offers.md).

/// Globals, by RVA.
pub mod globals {
    /// The war goal of a war declared without a casus belli, a
    /// [`super::war_goal`] the declare war dialog picks when the player
    /// declares without one.
    pub const NO_CASUS_BELLI: usize = 0x2340030;
}

/// Classes, by vtable RVA.
pub mod classes {
    /// The diplomatic actions the framework sends.
    pub const ALLIANCE: usize = 0x1c80720;
    pub const DECLARE_WAR: usize = 0x1c80d90;
    pub const PEACE: usize = 0x1c7b840;
    /// The command that carries a diplomatic action to the session.
    pub const ACTION_COMMAND: usize = 0x1c814b0;
    /// The list of war goals the game offers for a declaration of war: it
    /// owns them.
    pub const WAR_GOALS: usize = 0x1c3d1e0;
}

/// A diplomatic action, the base of every kind, whoever makes it: the player's
/// own, which diplomacy sends, and one made to the player, an offer, which
/// the country it is made to keeps (`super::super::gui::diplomacy_dialog`).
pub mod action {
    /// The type every persistent object of the game has there, u32.
    pub const TYPE: usize = 8;
    /// The type an action has.
    pub const TYPE_ACTION: u32 = 0x165;
    /// The country that makes it, and the one it is made to (tags).
    pub const FROM: usize = 0x10;
    /// The country acting, the maker again in the actions the framework
    /// makes, as in the game's own (tag).
    pub const ACTOR: usize = 0x18;
    pub const TO: usize = 0x20;
    /// When it was made, and when it was sent (i32 dates); the game sets
    /// both to the day.
    pub const MADE: usize = 0x28;
    pub const SENT: usize = 0x2c;
    /// The AI plan it belongs to, -1 for none (i32).
    pub const PLAN: usize = 0x30;
    /// Its answer, i32: [`UNANSWERED`] while it is a proposal, then
    /// [`DECLINED`] or [`ACCEPTED`].
    pub const STATE: usize = 0x34;
    pub const UNANSWERED: i32 = 0;
    pub const DECLINED: i32 = 1;
    pub const ACCEPTED: i32 = 2;
    /// u8, 1 when it starts something (an alliance, a war), 0 when it ends
    /// it; the byte after it, 0.
    pub const OFFER: usize = 0x38;
    /// The id it gets when it reaches a human player, i32; 0 until then (an
    /// offer's id).
    pub const ID: usize = 0x3c;
    /// Its methods, by vtable offset: its destructor, (action, flags), flag
    /// 1 freeing it; making the command that sends a copy of it, (action,
    /// out), the command in `*out`; and the checks, (action, reasons,
    /// skip), each answering whether the game allows it and, when `reasons`
    /// is a `std::string`, appending why not and the action's description
    /// to it: [`VALID`] the action's own conditions, which the command asks
    /// again when the game carries it out; [`ALLOWED`] those and the
    /// player's (diplomats, the wait between two of them), which the dialog
    /// that sends it asks; [`OFFERED`] what the diplomacy panel asks before
    /// it lets the player open that dialog.
    pub const DESTROY: usize = 0;
    pub const MAKE_COMMAND: usize = 0x40;
    pub const VALID: usize = 0x80;
    pub const ALLOWED: usize = 0x88;
    pub const OFFERED: usize = 0x90;
    /// The sizes of the kinds the framework makes.
    pub const ALLIANCE_SIZE: usize = 0x40;
    pub const DECLARE_WAR_SIZE: usize = 0x98;
    pub const PEACE_SIZE: usize = 0x210;
    /// A declaration of war's war goal, its own [`super::war_goal`].
    pub const WAR_GOAL: usize = 0x48;
    /// A peace offer's treaty, [`super::treaty::SIZE`] bytes in the action.
    pub const TREATY: usize = 0x48;
}

/// The command a diplomatic action's [`action::MAKE_COMMAND`] makes.
pub mod command {
    /// The action it carries, its own copy.
    pub const ACTION: usize = 0x50;
}

/// A war goal: what a war is declared for, under a casus belli.
pub mod war_goal {
    /// Its type ([`super::war_goal_type`]).
    pub const TYPE: usize = 0x10;
    /// The province it is about, i32; -1 for none.
    pub const PROVINCE: usize = 0x18;
    /// The country it is about, a tag (unaligned), valid when the byte
    /// [`TARGET_VALID`] is nonzero.
    pub const TARGET: usize = 0x1c;
    pub const TARGET_VALID: usize = 0x23;
    /// Its casus belli's type ([`super::casus_belli`]); null for none.
    pub const CASUS_BELLI: usize = 0x40;
    /// Its method that writes its name, as the game shows it, into a
    /// `std::string`: (war goal, out), by vtable offset.
    pub const NAME: usize = 0x40;
}

/// A type of war goal, from `common/wargoal_types`.
pub mod war_goal_type {
    /// Its key, such as `take_claim`, a `std::string`.
    pub const KEY: usize = 0x18;
}

/// A type of casus belli, from `common/cb_types`.
pub mod casus_belli {
    /// Its key, such as `cb_claim`, a `std::string`.
    pub const KEY: usize = 0x18;
    /// Nonzero for one with `exclusive = yes` (u8): when the player has it,
    /// the game offers its war goals alone, and no declaration without a
    /// casus belli.
    pub const EXCLUSIVE: usize = 0x159;
}

/// The list of [`classes::WAR_GOALS`]: the war goals, pointers, and their
/// count (i32).
pub mod war_goals {
    pub const ITEMS: usize = 8;
    pub const COUNT: usize = 0x14;
    pub const SIZE: usize = 0x18;
}

/// A peace treaty, which a peace offer holds; one with no terms is a white
/// peace.
pub mod treaty {
    pub const SIZE: usize = 0x1c0;
}

/// The world's record of the player's last alliance offer and last peace
/// offer, which the diplomacy dialog writes as it sends one, and the
/// diplomacy panel reads to let the player send no second one on the same
/// day: the player's tag and the day.
pub mod world {
    pub const ALLIANCE_OFFER_FROM: usize = 0x1d94;
    pub const ALLIANCE_OFFER_DATE: usize = 0x1d90;
    pub const PEACE_OFFER_FROM: usize = 0x1da0;
    pub const PEACE_OFFER_DATE: usize = 0x1d9c;
}

use crate::points::Point;

pub fn points() -> Vec<Point> {
    use super::campaign::{globals::WORLD, world::DATE};
    use super::gui::diplomacy_dialog as offer_window;
    use action::*;
    vec![
        // The declaration of war's constructor: the fields of every action,
        // and its class.
        Point::keep(
            0x500492usize,
            "c7 41 08 65 01 00 00 48 89 51 10 48 89 51 18 4c 89 41 20 8b 44 24 50 89 41 28 89 41 2c c7 41 30 ff ff ff ff \
             33 ff 89 79 34 66 c7 41 38 01 00 89 79 3c 48 8d 05 c5 08 78 01 48 89 01",
            "diplomatic action",
        )
        .uses(&[TYPE, TYPE_ACTION as usize, FROM, ACTOR, TO, MADE, SENT, PLAN, STATE, OFFER, ID, classes::DECLARE_WAR]),
        // Then its war goal, a copy of the one it is given.
        Point::keep(0x5004d6usize, "48 8b 4c 24 58 e8 10 a6 89 00 48 89 43 48", "declaration of war's war goal").uses(&[WAR_GOAL]),
        // The peace offer's constructor: its class, then a copy of the
        // treaty, which ends where its last byte is.
        Point::keep(0x598b2fusize, "48 8d 05 0a 2d 6e 01 48 89 01", "peace offer").uses(&[classes::PEACE]),
        Point::keep(0x598b41usize, "48 83 c1 48 48 8b 54 24 50 e8 f1 d3 3d 00 c6 83 08 02 00 00 00", "peace offer's treaty")
            .uses(&[TREATY, TREATY + treaty::SIZE]),
        // The alliance's copy, which sets its class.
        Point::keep(0x5130d4usize, "48 8d 05 45 d6 76 01 49 89 00", "alliance").uses(&[classes::ALLIANCE]),
        // The destructors free this much.
        Point::keep(0x129468usize, "ba 40 00 00 00", "alliance size").uses(&[ALLIANCE_SIZE]),
        Point::keep(0x500539usize, "ba 98 00 00 00", "declaration of war size").uses(&[DECLARE_WAR_SIZE]),
        Point::keep(0x598b87usize, "ba 10 02 00 00", "peace offer size").uses(&[PEACE_SIZE]),
        // Making the command: its class and the action it carries.
        Point::keep(
            0x4e996ausize,
            "48 8d 0d 3f 7b 79 01 33 d2 c7 40 08 65 01 00 00 c6 40 10 00 48 89 50 18 88 50 18 48 89 50 28 48 c7 40 30 0f 00 00 00 48 89 58 50",
            "diplomatic action command",
        )
        .uses(&[classes::ACTION_COMMAND, command::ACTION]),
        // Carrying the command out reads its action, then the action's
        // answer; the AI's answer is 1 + whether it accepts.
        Point::keep(0x4e7c65usize, "48 8b 49 50", "diplomatic action command's action").uses(&[command::ACTION]),
        Point::keep(0x4e7c9ausize, "41 8b 40 34 85 c0", "diplomatic action answer").uses(&[STATE]),
        Point::keep(0x4e81c7usize, "8b cd 84 c0 0f 95 c1 ff c1 89 4b 34 eb 07 c7 43 34 02 00 00 00", "AI answer").uses(&[STATE, ACCEPTED as usize]),
        // The diplomacy dialog sending an action: the world's record of the
        // last alliance or peace offer, the day as when it was made, the
        // session from the map manager, and the command made.
        Point::keep(
            0xedfaafusize,
            "48 8b 87 c0 01 00 00 48 8b 15 bb 03 46 01 48 8b 40 10 8b 8a d0 1d 00 00 48 89 82 94 1d 00 00 89 8a 90 1d 00 00 eb 3c \
             48 8b 8f c0 01 00 00 48 8b 01 ff 90 98 00 00 00 3d 3c 29 00 00 75 25 48 8b 87 c0 01 00 00 48 8b 15 7d 03 46 01 \
             48 8b 40 10 8b 8a d0 1d 00 00 48 89 82 a0 1d 00 00 89 8a 9c 1d 00 00",
            "diplomacy dialog send",
        )
        .uses(&[FROM, WORLD, DATE, world::ALLIANCE_OFFER_FROM, world::ALLIANCE_OFFER_DATE, world::PEACE_OFFER_FROM, world::PEACE_OFFER_DATE]),
        Point::keep(
            0xedfb64usize,
            "48 8b 8f c0 01 00 00 41 8b 80 d0 1d 00 00 89 41 28 48 8b 8f b8 01 00 00 48 8b 01 ff 90 80 00 00 00 48 8b 8f c0 01 00 00 \
             48 8b d8 48 8b 11 4c 8b 42 40",
            "diplomacy dialog command",
        )
        .uses(&[DATE, MADE, super::campaign::map_manager::SESSION, MAKE_COMMAND]),
        // The war goals: the dialog's list, which owns them, its count and
        // its items as the game walks them.
        Point::keep(0xee6d40usize, "48 8d 05 99 64 d5 00", "war goal list").uses(&[classes::WAR_GOALS]),
        Point::keep(0xd9ec33usize, "4d 63 7e 14 4d 85 ff 7e 23 0f 1f 40 00 49 8b 46 08", "war goal list items").uses(&[war_goals::COUNT, war_goals::ITEMS]),
        // A war goal's fields, as the game writes it into a save.
        Point::keep(0xd9bec8usize, "48 8b 73 10 48 83 c6 18", "war goal type").uses(&[war_goal::TYPE, war_goal_type::KEY]),
        Point::keep(0xd9bef8usize, "44 8b 43 18 41 83 f8 ff", "war goal province").uses(&[war_goal::PROVINCE]),
        Point::keep(0xd9bf0fusize, "80 7b 23 00 0f 84 a6 00 00 00 48 8d 53 1c", "war goal country").uses(&[war_goal::TARGET_VALID, war_goal::TARGET]),
        Point::keep(0xd9c036usize, "48 8b 73 40 48 83 c6 18", "war goal casus belli").uses(&[war_goal::CASUS_BELLI, casus_belli::KEY]),
        // The war goal list's builder skips the others for an exclusive
        // casus belli.
        Point::keep(0xd9ec23usize, "45 0f b6 a5 59 01 00 00", "exclusive casus belli").uses(&[casus_belli::EXCLUSIVE]),
        // The dialog: a war goal's name, and the war goal it picks when the
        // game offers none.
        Point::keep(0xee391ausize, "48 8b 4e 58 48 8b 11 4c 8b 42 40 48 8d 54 24 20 41 ff d0", "war goal name").uses(&[war_goal::NAME]),
        Point::keep(0xee70e5usize, "48 8b 15 44 8f 45 01", "no casus belli").uses(&[globals::NO_CASUS_BELLI]),
        // An action made to the player: it gets its id as it reaches the
        // player; the alliance offer's window shows both countries; the
        // offer's window asks whether the action is still unanswered, and
        // whether it was made no more than two months ago (its day, two
        // months on by the game's adding of months, not before today).
        Point::keep(0x4e82a9usize, "89 43 3c", "diplomatic action id").uses(&[ID]),
        Point::keep(0x1067b19usize, "49 8b 46 10 48 c1 e8 20", "diplomatic action maker").uses(&[FROM]),
        Point::keep(0x1067aacusize, "48 8b 96 38 02 00 00 48 8b 52 20", "diplomatic action recipient").uses(&[offer_window::ACTION, TO]),
        Point::keep(0x1066b5dusize, "48 8b 83 38 02 00 00 83 78 34 00 0f 94 c0", "offer's action unanswered").uses(&[offer_window::ACTION, STATE, UNANSWERED as usize]),
        Point::keep(
            0x1066b16usize,
            "48 8b 81 38 02 00 00 48 8b d9 48 8d 4c 24 30 8b 50 28 89 54 24 30 ba 02 00 00 00 e8 fa 14 46 00",
            "offer's action made",
        )
        .uses(&[offer_window::ACTION, MADE, offer_window::ANSWER_MONTHS]),
    ]
}
