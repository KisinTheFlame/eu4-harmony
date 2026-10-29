//! The campaign's objects: the world, countries, provinces and their map
//! data, units and their routes, battles, commands, the map manager, and the
//! game's events and diplomatic actions waiting for the player's answer
//! (docs/research/regional-combat/, docs/research/questions-and-offers.md).

/// Globals, by RVA.
pub mod globals {
    /// The world, the campaign's state. It is not null in the main menu
    /// either, so it is no sign of a game (docs/research/safe-points.md).
    pub const WORLD: usize = 0x233fe78;
    /// The country manager.
    pub const COUNTRY_MANAGER: usize = 0x233d8d0;
    /// The map.
    pub const MAP: usize = 0x233fed8;
    /// The indices (i32) of the rebel-like special countries, hostile to
    /// everyone.
    pub const SPECIAL_COUNTRIES: [usize; 3] = [0x233d8a4, 0x233d8d8, 0x233d8dc];
    /// The idle task units without one share, made on first use.
    pub const IDLE_TASK: usize = 0x2340020;
    /// The normal movement policy route queries take (its vtable).
    pub const ROUTE_POLICY: usize = 0x1c3af98;
}

/// Classes, by vtable RVA.
pub mod classes {
    pub const ARMY: usize = 0x1c42540;
    pub const FLEET: usize = 0x1cca7e0;
    pub const MAP_MANAGER: usize = 0x1cb8460;
    pub const IDLE_TASK: usize = 0x1d1e800;
    /// The movement commands the command dispatcher receives.
    pub const MOVE_COMMANDS: [usize; 2] = [0x1d1d3f8, 0x1d1da88];
    /// The recruit command (see [`super::recruit_command`]).
    pub const RECRUIT_COMMAND: usize = 0x1c4dff8;
    /// A kind of regiment or ship (see [`super::unit_type`]); the placeholder
    /// for none, `---`, has a class of its own.
    pub const UNIT_TYPE: usize = 0x1ce3830;
    pub const WAR: usize = 0x1d1f768;
    /// The window of one of the game's events, as the map manager shows it
    /// to the player (see [`super::event_window`]).
    pub const EVENT_WINDOW: usize = 0x1d590f8;
}

/// A country tag, a u64: three letters, then at these bit positions the
/// country's index (i16), its kind (u8) and a byte that is nonzero when the
/// tag is valid.
pub mod tag {
    pub const INDEX: usize = 32;
    pub const KIND: usize = 48;
    pub const VALID: usize = 56;
    /// The kind of rebel factions.
    pub const REBELS: u8 = 7;
}

/// The world, at [`globals::WORLD`].
pub mod world {
    /// The date, i32 (docs/research/dates.md).
    pub const DATE: usize = 0x1dd0;
    /// The map manager.
    pub const MAP_MANAGER: usize = 0x1e00;
    /// Nonzero while the daily update runs, u8.
    pub const DAILY_UPDATE: usize = 0x1e10;
    /// The game speed, i32: 0 for speed 1 to 4 for speed 5.
    pub const SPEED: usize = 0x1e08;
    /// The player's country tag; while [`PLAYER_MODE`] is [`SWITCHED`] and
    /// [`PLAYER_SWITCHED`] is nonzero (u8 each), [`SWITCHED_PLAYER`] instead.
    pub const PLAYER: usize = 0x1e60;
    pub const SWITCHED_PLAYER: usize = 0x1e68;
    pub const PLAYER_MODE: usize = 0x1e66;
    pub const PLAYER_SWITCHED: usize = 0x1e6f;
    pub const SWITCHED: u8 = 7;
    /// The provinces, [`super::province::SIZE`] bytes each, by ID.
    pub const PROVINCES: usize = 0x1ca8;
    /// Nonzero while there is fog of war, u8.
    pub const FOG: usize = 0x23a6;
    /// The wars going on: pointers to [`super::war`]s, and their count (i32).
    pub const WARS: usize = 0x1e30;
    pub const WAR_COUNT: usize = 0x1e3c;
    /// The events shown to players and not answered yet: an array of
    /// [`super::pending_event`]s, its data at [`PENDING_EVENTS`] and its
    /// length (i32) at [`PENDING_EVENT_COUNT`].
    pub const PENDING_EVENTS: usize = 0x1ea0;
    pub const PENDING_EVENT_COUNT: usize = 0x1eac;
}

/// The country manager, at [`globals::COUNTRY_MANAGER`].
pub mod countries {
    /// The countries: an array whose data, pointers by tag index, is at
    /// [`LIST`] and whose length (i32) is at [`COUNT`].
    pub const ARRAY: usize = 0x110;
    pub const LIST: usize = ARRAY + 8;
    pub const COUNT: usize = ARRAY + 0x14;
}

pub mod country {
    /// Its tag.
    pub const TAG: usize = 0x20;
    /// Its armies and fleets, a [`super::list`].
    pub const UNITS: usize = 0xd18;
    /// The countries it is at war with: a pointer to [`HOSTILE_SIZE`]-byte
    /// records whose first i32 is the other country's index, and their count
    /// (i32).
    pub const HOSTILE: usize = 0x1548;
    pub const HOSTILE_COUNT: usize = 0x1554;
    pub const HOSTILE_SIZE: usize = 16;
    /// The regiments and ships it recruits now: [`UNIT_KINDS`] pointers to
    /// [`super::unit_type`]s, by kind (infantry, cavalry, artillery, heavy
    /// ship, light ship, galley, transport).
    pub const UNIT_TYPES: usize = 0xd48;
    pub const UNIT_KINDS: usize = 7;
    /// Fixed-point numbers, i32 thousandths, as the game keeps its numbers:
    /// the treasury in ducats, stability, prestige, and manpower and its
    /// maximum in thousands of men (so in men).
    pub const TREASURY: usize = 0xbb0;
    pub const STABILITY: usize = 0xbd8;
    pub const PRESTIGE: usize = 0xcf8;
    pub const MANPOWER: usize = 0xdb8;
    pub const MAX_MANPOWER: usize = 0xdbc;
    /// Monarch points, an i32 each: administrative, diplomatic, military.
    pub const POWER: usize = 0x260;
    pub const POWER_DIP: usize = POWER + 4;
    pub const POWER_MIL: usize = POWER + 8;
    /// Its overlord's tag, not valid when it has none.
    pub const OVERLORD: usize = 0x18f0;
    /// Its allies, a [`super::list`] whose items are the allies' tags.
    pub const ALLIES: usize = 0x19b8;
    /// Its subjects' tags, a vector (begin, end).
    pub const SUBJECTS: usize = 0x1a18;
    /// The countries it rivals: the first of a chain of nodes, each with the
    /// rival's tag at [`RIVAL_TAG`] and the next node at [`RIVAL_NEXT`].
    pub const RIVALS: usize = 0x2248;
    pub const RIVAL_TAG: usize = 0x10;
    pub const RIVAL_NEXT: usize = 0x28;
}

/// A war going on, of [`classes::WAR`].
pub mod war {
    /// Its name, a `std::string` (`crate::game_string`).
    pub const NAME: usize = 0x28;
    /// The tags of its attackers and of its defenders, each a vector (begin,
    /// end); the first of each side is the side's leader.
    pub const ATTACKERS: usize = 0x48;
    pub const DEFENDERS: usize = 0x78;
}

pub mod province {
    pub const SIZE: usize = 0x2e10;
    /// Its ID, i32.
    pub const ID: usize = 0x20;
    /// Its map data (see [`super::geometry`]).
    pub const GEOMETRY: usize = 0x28;
    /// The units in it, a [`super::list`].
    pub const UNITS: usize = 0x350;
    /// The tags of its owner and its controller.
    pub const OWNER: usize = 0x468;
    pub const CONTROLLER: usize = 0x470;
    /// Fog of war hides it from the player when [`FOGGED_BIT`] of the byte
    /// at [`FOGGED`] and [`HIDDEN_BIT`] of the byte at [`HIDDEN`] are set.
    pub const HIDDEN: usize = 0x958;
    pub const HIDDEN_BIT: u8 = 2;
    pub const FOGGED: usize = 0x959;
    pub const FOGGED_BIT: u8 = 0x20;
    /// Its base tax, production and manpower, i32 in thousandths. The game's
    /// own getters add a native country's tribal development to its capital
    /// (docs/research/provinces.md).
    pub const BASE_MANPOWER: usize = 0x3e0;
    pub const BASE_TAX: usize = 0x3e4;
    pub const BASE_PRODUCTION: usize = 0x3e8;
    /// Its buildings: a vector (begin, end) of [`BUILDING_SLOT`]-byte slots,
    /// one per kind of building by its [`super::building::INDEX`], each
    /// holding the building there (the placeholder, index 0, where there is
    /// none) and the tag of who built it.
    pub const BUILDINGS: usize = 0x3b0;
    pub const BUILDING_SLOT: usize = 0x10;
    /// Its fort's state, u8 ([`MOTHBALLED`] when mothballed), and its fort
    /// levels (i32): [`FORT_LEVEL`], and [`FORT_LEVEL_MOTHBALLED`], what is
    /// left of it while mothballed (a capital's own).
    pub const MOTHBALL: usize = 0x8f4;
    pub const MOTHBALLED: u8 = 2;
    pub const FORT_LEVEL_MOTHBALLED: usize = 0x2ddc;
    pub const FORT_LEVEL: usize = 0x2de0;
}

/// A province's map data, at [`province::GEOMETRY`].
pub mod geometry {
    /// The province's ID again, i32.
    pub const ID: usize = 0x150;
    /// Its neighbors: a vector (begin, end) of [`super::adjacency`] records.
    pub const ADJACENT: usize = 0x168;
    /// Land and lake bits, u8.
    pub const TERRAIN: usize = 0x228;
    pub const LAND: u8 = 8;
    pub const LAKE: u8 = 0x10;
    /// The impassable bit, u8.
    pub const CLIMATE: usize = 0x229;
    pub const IMPASSABLE: u8 = 8;
    /// The province's name, a `std::string` of game text: the localisation
    /// of `PROV<id>`, or what it was renamed to.
    pub const NAME: usize = 0x10;
    /// Its terrain (see [`super::terrain`]), such as farmlands: not the land
    /// and lake bits of [`TERRAIN`].
    pub const TERRAIN_TYPE: usize = 0x30;
}

/// A kind of terrain, one of the map's [`map::TERRAINS`].
pub mod terrain {
    /// Its key as in map/terrain.txt (`farmlands`), a `std::string`.
    pub const NAME: usize = 0x118;
}

/// A kind of building.
pub mod building {
    /// Its index in the building database, i32; 0 for the placeholder that
    /// stands for none.
    pub const INDEX: usize = 0x14;
    /// Its key as in common/buildings (`marketplace`), a `std::string`.
    pub const NAME: usize = 0x28;
}

/// A kind of regiment or ship, of class [`classes::UNIT_TYPE`].
pub mod unit_type {
    /// Which of a country's [`super::country::UNIT_KINDS`] it is, i32.
    pub const KIND: usize = 0x10;
    /// Its key as in common/units (`western_medieval_infantry`), a
    /// `std::string`.
    pub const NAME: usize = 0x60;
}

/// A connection to a neighbor, [`SIZE`] bytes: the neighbor's ID, the
/// province's own, the sea zone a strait crosses (i32 each) and the kind.
pub mod adjacency {
    pub const TO: usize = 0;
    pub const FROM: usize = 4;
    pub const THROUGH: usize = 8;
    pub const KIND: usize = 0x1c;
    pub const SIZE: usize = 0x24;
    /// Kinds: plain land, then `sea` (a strait), `river`, `impassable`, `canal`.
    pub const LAND: i32 = 0;
    pub const STRAIT: i32 = 1;
    pub const RIVER: i32 = 2;
    pub const CANAL: i32 = 4;
}

/// The map, at [`globals::MAP`].
pub mod map {
    /// The provinces: a vector (begin, end) of [`super::province::SIZE`]-byte
    /// records, the same the world's [`super::world::PROVINCES`] points to.
    pub const PROVINCES: usize = 0x6f0;
    /// The kinds of terrain: a vector (begin, end) of pointers to
    /// [`super::terrain`]s.
    pub const TERRAINS: usize = 0x758;
}

/// An army or a fleet.
pub mod unit {
    /// Its ID, u64, kept in saves.
    pub const ID: usize = 0x20;
    /// Its regiments: a vector (begin, end) of pointers.
    pub const REGIMENTS: usize = 0x30;
    /// The army it is attached to, and the [`super::list`] of those
    /// attached to it.
    pub const ATTACHED_TO: usize = 0x50;
    pub const FOLLOWERS: usize = 0x58;
    /// Two pointers and a vector (begin, end) of what it is busy with besides
    /// its task; all empty when it is idle.
    pub const CONTEXTS: [usize; 2] = [0x98, 0xa0];
    pub const CONTEXT_VECTOR: usize = 0xa8;
    /// The battle and the siege it is in.
    pub const BATTLE: usize = 0xc0;
    pub const SIEGE: usize = 0xc8;
    /// Its owner's tag.
    pub const OWNER: usize = 0xd0;
    /// The province it is in.
    pub const LOCATION: usize = 0xe8;
    /// Its route, a [`super::list`] of province IDs (i32).
    pub const PATH: usize = 0x100;
    /// Its task; [`super::globals::IDLE_TASK`] or null when idle.
    pub const TASK: usize = 0x1f8;
    /// u8 each.
    pub const MOVEMENT_LOCKED: usize = 0x202;
    pub const RETREATING: [usize; 2] = [0x203, 0x204];
    pub const EXILED: usize = 0x207;
    /// Methods of [`super::classes::ARMY`], by vtable offset.
    pub const CAN_ENTER: usize = 0x120;
    pub const MILITARY_TAG: usize = 0x138;
}

/// A task's methods an idle task has as [`super::classes::IDLE_TASK`] does,
/// by vtable offset.
pub mod task {
    pub const IDLE_METHODS: [usize; 3] = [0x50, 0x78, 0x80];
}

/// What a route query is told, as the game's own callers fill it: flags at
/// [`ONE_HOP`], an i32 at [`LIMIT`] and a tag at [`TAG`].
pub mod route_options {
    pub const SIZE: usize = 0x48;
    pub const ONE_HOP: usize = 8;
    pub const LIMIT: usize = 0xc;
    pub const TAG: usize = 0x14;
    /// The tag that names no country, `---`.
    pub const NO_TAG: u64 = 0x2d2d2d;
}

pub mod battle {
    /// The two sides, winner first once it is decided.
    pub const SIDES: [usize; 2] = [0x20, 0x28];
    /// Nonzero once the result is in, u8.
    pub const SETTLED: usize = 0x44;
}

/// A side in a battle.
pub mod side {
    /// Its units, a [`super::list`], and their count (i32).
    pub const UNITS: usize = 0xe8;
    pub const COUNT: usize = 0xf8;
}

/// A movement command.
pub mod command {
    /// The ID of the unit it moves.
    pub const UNIT: usize = 0x58;
}

/// The recruit command, of class [`classes::RECRUIT_COMMAND`]
/// (docs/research/provinces.md).
pub mod recruit_command {
    /// Its size, as the recruit button allocates it.
    pub const SIZE: usize = 0xa0;
    /// Its methods, by vtable offset: the destructor (command, flags) and the
    /// check (command).
    pub const DESTROY: usize = 0;
    pub const CHECK: usize = 0x40;
}

/// The map manager, at [`world::MAP_MANAGER`].
pub mod map_manager {
    /// The current map mode, i32.
    pub const MODE: usize = 0x2c;
    /// Its pause state (see [`super::pause`]).
    pub const PAUSE: usize = 0x418;
    /// Its methods, by vtable offset: the refresh, and the one that answers
    /// its session (`objects::session`).
    pub const REFRESH: usize = 0xc8;
    pub const SESSION: usize = 0x80;
    /// Its method that shows one of the game's events to the player, by
    /// vtable offset: it makes an event window ([`super::classes::EVENT_WINDOW`])
    /// and adds it to [`POPUPS`].
    pub const SHOW_EVENT: usize = 0x108;
    /// The popup windows open on the map, a [`super::list`] of windows, each
    /// asked every frame whether it stays ([`super::popup::UPDATE`]); a node
    /// whose window closed while the list was walked is marked
    /// [`super::list::REMOVED`] and goes later.
    pub const POPUPS: usize = 0x78;
    /// What holds the diplomatic message icons, `gui::message_icon_holder`.
    pub const MESSAGE_ICON_HOLDER: usize = 0x1400;
    /// The dialogs open from those icons, a [`super::list`].
    pub const OPEN_DIALOGS: usize = 0x370;
}

/// The map manager's pause state, at [`map_manager::PAUSE`].
pub mod pause {
    /// The map manager it belongs to.
    pub const MANAGER: usize = 0x50;
    /// The player's pause, u8: nonzero once the player paused the game (or
    /// remote control or an extension did); when it is zero and the game is
    /// paused, windows hold it paused.
    pub const PLAYER_PAUSED: usize = 0x48;
}

/// A window among the map manager's [`map_manager::POPUPS`].
pub mod popup {
    /// Its update, by vtable offset: whether it stays, u8.
    pub const UPDATE: usize = 0x18;
}

/// One of the game's events shown to a player and not answered yet, one of
/// the world's [`world::PENDING_EVENTS`], [`SIZE`] bytes.
pub mod pending_event {
    /// The event (see [`super::event`]).
    pub const EVENT: usize = 0x10;
    /// The last day to answer, i32; later, the event window picks an option.
    pub const DEADLINE: usize = 0x20;
    /// Its id, i32, unique in the save.
    pub const ID: usize = 0x24;
    pub const SIZE: usize = 0x28;
}

/// One of the game's events, as its files define it.
pub mod event {
    /// Its id in the game's files (`flavor_fra.1`), a `std::string`.
    pub const NAME: usize = 0x20;
    /// Its options: a vector (begin, end) of pointers to
    /// [`super::event_option`]s.
    pub const OPTIONS: usize = 0x790;
    /// The localisation key of its title, a `std::string`.
    pub const TITLE: usize = 0x7e0;
}

pub mod event_option {
    /// The localisation key of its text, a `std::string`.
    pub const NAME: usize = 0x88;
}

/// An event window, of [`classes::EVENT_WINDOW`], among the map manager's
/// popups.
pub mod event_window {
    /// The event, the scope it happens in (an object of the window's own,
    /// copied from the pending event's) and the pending event's id (i32).
    pub const EVENT: usize = 0x48;
    pub const SCOPE: usize = 0x50;
    pub const PENDING: usize = 0x120;
    /// Nonzero while the window is open, u8; choosing an option clears it.
    pub const OPEN: usize = 0x124;
}

/// A diplomatic action, as the country it is made to keeps it.
pub mod diplomatic_action {
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

pub use super::list;
use crate::points::Point;

pub fn points() -> Vec<Point> {
    use self::{globals::*, *};
    vec![
        // The player's country, as the game's own getter reads it.
        Point::keep(
            0x811b20usize,
            "48 8b 05 51 e3 b2 01 80 b8 66 1e 00 00 07 75 17 80 b8 6f 1e 00 00 00 74 0e 48 8b 80 68 1e 00 00 48 89 02 48 8b c2 c3 \
             48 8b 80 60 1e 00 00 48 89 02 48 8b c2 c3",
            "player country",
        )
        .uses(&[WORLD, world::PLAYER_MODE, world::SWITCHED as usize, world::PLAYER_SWITCHED, world::SWITCHED_PLAYER, world::PLAYER]),
        Point::keep(0x9c376usize, "48 8b 0d fb 3a 2a 02 8b 44 24 60 39 81 d0 1d 00 00", "world date").uses(&[WORLD, world::DATE]),
        Point::keep(0x10a5213usize, "48 8b 05 5e ac 29 01 80 b8 10 1e 00 00 00", "daily update flag").uses(&[WORLD, world::DAILY_UPDATE]),
        Point::keep(0xa3595usize, "48 8b 05 dc c8 29 02 48 8b 88 00 1e 00 00", "world map manager").uses(&[WORLD, world::MAP_MANAGER]),
        Point::keep(0x9b811usize, "4c 8b 25 60 46 2a 02 48 8b bb 88 00 00 00 48 63 07 48 69 c0 10 2e 00 00 49 03 84 24 a8 1c 00 00", "province by ID")
            .uses(&[WORLD, province::SIZE, world::PROVINCES]),
        // The fog of war refresh reads the player and whether there is fog.
        Point::keep(
            0x827610usize,
            "80 bb 66 1e 00 00 07 75 12 80 bb 6f 1e 00 00 00 74 09 48 8b bb 68 1e 00 00 eb 07 48 8b bb 60 1e 00 00 80 bb a6 23 00 00 00",
            "world fog",
        )
        .uses(&[world::PLAYER, world::FOG]),
        // A country finding itself by its tag.
        Point::keep(0xb239cusize, "4c 8b 05 2d b5 28 02 48 8b 41 20 48 c1 e8 20 48 0f bf d0 49 8b 80 18 01 00 00 48 8b 1c d0", "country by tag")
            .uses(&[COUNTRY_MANAGER, country::TAG, tag::INDEX, countries::LIST]),
        // The country manager, made, hands its country array to 0xb510f0,
        // which walks it to its length, reading each country's tag.
        Point::keep(0xb510c6usize, "48 89 05 03 c8 7e 01 48 8d 90 10 01 00 00", "country array").uses(&[COUNTRY_MANAGER, countries::ARRAY]),
        Point::keep(
            0xb5113fusize,
            "41 8b 46 14 4d 8b 76 08 48 98 49 8d 2c c6 4c 3b f5 0f 84 1c 01 00 00 48 89 74 24 50 4c 89 64 24 58 4c 8d 25 39 ea 15 01              4c 89 7c 24 60 45 33 ff 90 8b 57 14 49 8b 1e 8b 77 10 8d 42 01 48 8b 5b 20",
            "country array walk",
        )
        .uses(&[countries::COUNT - countries::ARRAY, countries::LIST - countries::ARRAY, country::TAG]),
        Point::keep(0xb252eusize, "49 8b 89 48 15 00 00 4d 63 81 54 15 00 00 49 c1 e0 04 4c 03 c1", "country wars")
            .uses(&[country::HOSTILE, country::HOSTILE_COUNT, 4]),
        // The country snapshot (docs/research/countries.md). The country
        // writer in saves: each field, then its key's token.
        Point::keep(
            0x2df116usize,
            "49 8d 97 f8 0c 00 00 48 8d 8d 00 11 00 00 e8 c7 65 db ff 4c 8b c0 ba a1 29 00 00 48 8b ce e8 a7 89 42 01 \
             49 8d 97 d8 0b 00 00 48 8d 8d 10 11 00 00 e8 a4 65 db ff 4c 8b c0 ba 86 28 00 00 48 8b ce e8 84 89 42 01 \
             49 8d 97 b0 0b 00 00 48 8d 8d 18 11 00 00 e8 81 65 db ff 4c 8b c0 ba 76 28 00 00",
            "country prestige, stability and treasury",
        )
        .uses(&[country::PRESTIGE, country::STABILITY, country::TREASURY]),
        Point::keep(
            0x2e00a9usize,
            "49 8d 97 b8 0d 00 00 48 8d 8d 00 11 00 00 e8 34 56 db ff 4c 8b c0 ba 3f 28 00 00 48 8b ce e8 14 7a 42 01 \
             49 8d 97 bc 0d 00 00 48 8d 8d 10 11 00 00 e8 11 56 db ff 4c 8b c0 ba 73 29 00 00",
            "country manpower",
        )
        .uses(&[country::MANPOWER, country::MAX_MANPOWER]),
        Point::keep(0x2e0b23usize, "4d 8d 87 60 02 00 00 ba b2 2a 00 00", "country monarch points").uses(&[country::POWER]),
        Point::keep(
            0x2ddee6usize,
            "49 8d 8f f0 18 00 00 e8 ce a6 db ff 84 c0 74 1c 49 8d 8f f0 18 00 00 e8 1e 70 db ff 4c 8b c0 ba b6 2c 00 00",
            "country overlord",
        )
        .uses(&[country::OVERLORD]),
        Point::keep(
            0x2def0cusize,
            "49 8d 97 18 1a 00 00 41 b8 db 2e 00 00 48 8b ce e8 9f 20 14 00 49 8d 97 b8 19 00 00 41 b8 9b 2e 00 00",
            "country subjects and allies",
        )
        .uses(&[country::SUBJECTS, country::ALLIES]),
        // The triggers on monarch points, one for each kind: the country by
        // its tag, then its points of that kind.
        Point::keep(
            0xcb4197usize,
            "48 8b 4f 48 48 8b 05 2e 97 68 01 48 c1 e9 20 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 14 c8 8b 8a 60 02 00 00",
            "administrative points",
        )
        .uses(&[COUNTRY_MANAGER, tag::INDEX, countries::LIST, country::POWER]),
        Point::keep(0xcb4c0dusize, "48 8b 05 bc 8c 68 01 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 34 c8 48 81 c6 64 02 00 00", "diplomatic points")
            .uses(&[COUNTRY_MANAGER, countries::LIST, country::POWER_DIP]),
        Point::keep(0xcb566dusize, "48 8b 05 5c 82 68 01 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 34 c8 48 81 c6 68 02 00 00", "military points")
            .uses(&[COUNTRY_MANAGER, countries::LIST, country::POWER_MIL]),
        // Walking a country's allies, as the AI does.
        Point::keep(0x3a37a7usize, "48 8b bf b8 19 00 00 48 85 ff 0f 84 19 07 00 00 44 8b 45 d8 0f 1f 44 00 00 48 8b 0f 48 8b 7f 10", "country allies")
            .uses(&[country::ALLIES, list::ITEM, list::NEXT]),
        // A country's diplomacy cleared: no subjects, their storage kept.
        Point::keep(0x2e2ca7usize, "48 8b 87 18 1a 00 00 48 89 87 20 1a 00 00", "country subjects").uses(&[country::SUBJECTS, country::SUBJECTS + 8]),
        // The `is_rival` trigger walking the country's rivals.
        Point::keep(
            0xc848d6usize,
            "48 8b 83 48 22 00 00 48 85 c0 74 aa 48 c1 ea 20 66 66 0f 1f 84 00 00 00 00 00 48 8b 48 10 48 c1 e9 20 66 3b ca 74 0b 48 8b 40 28",
            "country rivals",
        )
        .uses(&[country::RIVALS, country::RIVAL_TAG, country::RIVAL_NEXT]),
        // The wars going on, walked; a war made: its class, its name, its
        // sides; the leaders of the sides, the first of each.
        Point::keep(0x291997usize, "48 8b 05 da e4 0a 02 48 8b b8 30 1e 00 00 48 63 80 3c 1e 00 00", "wars").uses(&[WORLD, world::WARS, world::WAR_COUNT]),
        Point::keep(
            0xd856ceusize,
            "48 8d 05 93 a0 f9 00 48 89 01 66 c7 41 10 00 00 33 f6 48 89 71 18 40 88 71 20 c7 41 24 f8 77 9c 02 48 89 71 28 48 89 71 38 \
             48 c7 41 40 0f 00 00 00 40 88 71 28 48 89 71 48 48 89 71 50 48 89 71 58 48 89 71 60 48 89 71 68 48 89 71 70 48 89 71 78 \
             48 89 b1 80 00 00 00 48 89 b1 88 00 00 00",
            "war",
        )
        .uses(&[
            classes::WAR,
            war::NAME,
            war::NAME + crate::game_string::SIZE,
            war::NAME + crate::game_string::CAPACITY,
            war::ATTACKERS,
            war::ATTACKERS + 8,
            war::DEFENDERS,
            war::DEFENDERS + 8,
        ]),
        Point::keep(
            0xd8950ausize,
            "48 8b 41 48 48 8b fa 48 8b da 48 c1 ef 20 41 0f b6 e8 48 8b f1 48 8b 10 48 c1 ea 20 66 3b d7 0f 84 b1 01 00 00 48 8b 41 78",
            "war leaders",
        )
        .uses(&[war::ATTACKERS, war::DEFENDERS]),
        // The fog of war refresh walks a country's units.
        Point::keep(
            0x827c59usize,
            "48 8b 9d 18 0d 00 00 48 85 db 0f 84 0f 01 00 00 0f 1f 80 00 00 00 00 48 8b 33 48 8b 5b 10 48 8b 06 48 8b ce ff 90 80 00 00 00 \
             84 c0 74 49 48 8b 8e d0 00 00 00 48 8b c1",
            "country units",
        )
        .uses(&[country::UNITS, list::ITEM, list::NEXT, unit::OWNER]),
        Point::keep(0x15d443usize, "4c 8b 35 8e 2a 1e 02 49 8b 8e f0 06 00 00 49 8b 86 f8 06 00 00", "map provinces").uses(&[MAP, map::PROVINCES, map::PROVINCES + 8]),
        // Whether a unit is exiled: its owner's tag, the special countries,
        // the flag.
        Point::keep(
            0x99cb6usize,
            "48 8b 93 d0 00 00 00 48 8b c2 48 c1 e8 38 84 c0 74 37 48 8b c2 48 c1 e8 30 3c 07 74 2c 48 c1 ea 20 0f bf c2 \
             3b 05 c4 3b 2a 02 74 1d 3b 05 f0 3b 2a 02 74 15 3b 05 ec 3b 2a 02 74 0d 0f b6 83 07 02 00 00",
            "unit exiled",
        )
        .uses(&[unit::OWNER, tag::VALID, tag::KIND, tag::REBELS as usize, tag::INDEX, SPECIAL_COUNTRIES[0], SPECIAL_COUNTRIES[1], SPECIAL_COUNTRIES[2], unit::EXILED]),
        // The unit constructor.
        Point::keep(0xd3ace9usize, "48 89 69 30 48 89 69 38", "unit regiments").uses(&[unit::REGIMENTS, unit::REGIMENTS + 8]),
        Point::keep(0xd3ad0busize, "48 8b 05 96 51 60 01 48 89 41 20", "unit ID").uses(&[unit::ID]),
        Point::keep(0xd3ad1eusize, "48 89 69 50", "unit attached to").uses(&[unit::ATTACHED_TO]),
        Point::keep(0xd3ad6eusize, "48 89 a9 98 00 00 00 48 89 a9 a0 00 00 00 48 89 a9 a8 00 00 00 48 89 a9 b0 00 00 00", "unit contexts")
            .uses(&[unit::CONTEXTS[0], unit::CONTEXTS[1], unit::CONTEXT_VECTOR, unit::CONTEXT_VECTOR + 8]),
        Point::keep(0xd3ae28usize, "48 89 af 00 01 00 00 48 89 af 08 01 00 00 89 af 10 01 00 00 c6 87 14 01 00 00 00", "unit route")
            .uses(&[unit::PATH + list::HEAD, unit::PATH + list::TAIL, unit::PATH + list::COUNT, unit::PATH + list::CHANGING]),
        // Stopping an army: its followers, where they are and what they are
        // in, its lock and its route.
        Point::keep(0xd4fcebusize, "48 8b 59 58 4c 8d 3d a2 b2 ee 00 45 33 f6 48 85 db 0f 84 9b 00 00 00 48 8b 3b 48 8b 5b 10", "army followers")
            .uses(&[unit::FOLLOWERS, ROUTE_POLICY, list::ITEM, list::NEXT]),
        Point::keep(
            0xd4fd28usize,
            "4c 8b bf e8 00 00 00 4d 85 ff 74 59 4c 8b b6 e8 00 00 00 4d 85 f6 74 4a 80 bf 04 02 00 00 00 75 41 80 bf 03 02 00 00 00 \
             0f 85 24 02 00 00 48 8b 8f c8 00 00 00 48 85 c9 74 0a 48 8b 01 ff 50 70 84 c0 74 1e 48 8b 8f c0 00 00 00",
            "army state",
        )
        .uses(&[unit::LOCATION, unit::RETREATING[1], unit::RETREATING[0], unit::SIEGE, unit::BATTLE]),
        Point::keep(0xd4fda4usize, "c6 86 02 02 00 00 00", "army movement lock").uses(&[unit::MOVEMENT_LOCKED]),
        Point::keep(
            0xd4fdf2usize,
            "48 8b 8e 00 01 00 00 48 85 c9 74 18 66 90 48 8b 59 10 ba 20 00 00 00 e8 e2 96 9e 00 48 8b cb 48 85 db 75 ea \
             4c 89 b6 00 01 00 00 4c 89 b6 08 01 00 00 44 89 b6 10 01 00 00",
            "army route clear",
        )
        .uses(&[unit::PATH + list::HEAD, list::NEXT, list::NODE_SIZE, unit::PATH + list::TAIL, unit::PATH + list::COUNT]),
        // Giving a unit the idle task.
        Point::keep(
            0x2dc549usize,
            "48 8b 05 d0 3a 06 02 48 85 c0 75 20 8d 48 10 e8 77 6d 75 01 c7 40 08 65 01 00 00 48 8d 0d 95 22 a4 01 48 89 08 48 89 05 ab 3a 06 02 \
             48 8b 8f f8 01 00 00",
            "idle task",
        )
        .uses(&[IDLE_TASK, classes::IDLE_TASK, unit::TASK]),
        // Moving an army: where it is, and a province added to its route.
        Point::keep(0xd5060dusize, "49 8b 87 e8 00 00 00 8b 58 20", "army location").uses(&[unit::LOCATION, province::ID]),
        Point::keep(
            0xd50733usize,
            "49 8b 9f 08 01 00 00 b9 20 00 00 00 e8 90 2b ce 00 89 30 48 89 58 08 48 c7 40 10 00 00 00 00 c6 40 18 00 49 89 87 08 01 00 00 \
             41 8b 97 10 01 00 00 8d 4a 01 41 89 8f 10 01 00 00 85 d2 74 06 48 89 43 10 eb 07 49 89 87 00 01 00 00",
            "route node",
        )
        .uses(&[
            unit::PATH + list::TAIL,
            list::NODE_SIZE,
            list::ITEM,
            list::PREVIOUS,
            list::NEXT,
            list::REMOVED,
            unit::PATH + list::COUNT,
            unit::PATH + list::HEAD,
        ]),
        // A route query's options, as the rebel hunt fills them.
        Point::keep(
            0x1c878eusize,
            "48 c7 45 c7 00 00 00 00 66 c7 45 cf 01 01 c7 45 d3 ff ff ff ff 44 89 65 d7 66 c7 45 db 2d 2d c7 45 dd 2d 00 00 00",
            "route options",
        ),
        // Fog of war on a province: its map data's bit and its own.
        Point::keep(0xa0b15ausize, "4c 8b 41 28 0f b6 c2 c0 e0 02 0f b6 fa 48 8b d9 41 80 a0 28 02 00 00 fb 41 08 80 28 02 00 00", "province map data")
            .uses(&[province::GEOMETRY, geometry::TERRAIN]),
        Point::keep(0xa075d1usize, "f6 81 59 09 00 00 20 74 23 f6 81 58 09 00 00 02", "province fog")
            .uses(&[province::FOGGED, province::FOGGED_BIT as usize, province::HIDDEN, province::HIDDEN_BIT as usize]),
        // A strait's sea zone and the units in it.
        Point::keep(
            0x96f691usize,
            "48 63 43 08 85 c0 0f 84 0f 01 00 00 48 8b 0d 34 08 9d 01 48 8b d0 e8 d4 b3 72 ff 48 8b 98 50 03 00 00",
            "strait units",
        )
        .uses(&[adjacency::THROUGH, province::UNITS]),
        // The province writer in saves.
        Point::keep(0x9d70fbusize, "49 8d 97 68 04 00 00", "province owner").uses(&[province::OWNER]),
        Point::keep(0x9d71adusize, "49 8d 97 70 04 00 00", "province controller").uses(&[province::CONTROLLER]),
        Point::keep(0x9d7d67usize, "45 8b 67 20", "province ID").uses(&[province::ID]),
        // The route edge check: impassable, the kind of connection, the
        // neighbors, land.
        Point::keep(0x96f88dusize, "49 8b 47 28 f6 80 29 02 00 00 08 0f 85 81 03 00 00 48 8b 9c 24 a8 00 00 00 8b 43 1c", "province impassable")
            .uses(&[province::GEOMETRY, geometry::CLIMATE, geometry::IMPASSABLE as usize, adjacency::KIND]),
        Point::keep(0x96fa7busize, "4d 8b a6 70 01 00 00 4d 2b a6 68 01 00 00", "province neighbors").uses(&[geometry::ADJACENT + 8, geometry::ADJACENT]),
        // The map's set-up: the ID, land and not lake.
        Point::keep(0x87bd78usize, "89 bb 50 01 00 00", "map data ID").uses(&[geometry::ID]),
        Point::keep(0x87be67usize, "80 a3 28 02 00 00 ef 80 8b 28 02 00 00 08", "map data land").uses(&[geometry::TERRAIN, geometry::LAND as usize]),
        // Reading adjacencies.csv: a new connection.
        Point::keep(
            0x8802d6usize,
            "49 8b 57 08 49 3b 57 10 0f 84 dc 01 00 00 44 89 1a 44 89 72 04 8b 8d a0 00 00 00 89 4a 08 4c 89 52 0c 8b 4c 24 5c 89 4a 14 \
             89 42 18 44 89 62 1c 44 88 52 20 49 83 47 08 24",
            "adjacency record",
        )
        .uses(&[adjacency::TO, adjacency::FROM, adjacency::THROUGH, adjacency::KIND, adjacency::SIZE]),
        // Settling a battle; a side losing a unit.
        Point::keep(
            0x23a066usize,
            "80 7f 44 00 0f 85 e1 04 00 00 c6 47 44 01 4c 8b 05 fd 5d 10 02 45 84 f6 0f 85 e6 01 00 00 48 8b 57 20",
            "battle result",
        )
        .uses(&[battle::SETTLED, WORLD, battle::SIDES[0]]),
        Point::keep(0x23a0e3usize, "4c 8d 67 28", "battle loser").uses(&[battle::SIDES[1]]),
        Point::keep(0x21eb42usize, "48 8b b9 e8 00 00 00", "battle side units").uses(&[side::UNITS]),
        Point::keep(0x21ec87usize, "41 ff 8f f8 00 00 00", "battle side count").uses(&[side::COUNT]),
        Point::keep(0xd5cf2cusize, "48 8b e9 0f b6 99 98 00 00 00 0f b6 b9 91 00 00 00 48 83 c1 58", "move command unit").uses(&[command::UNIT]),
        // The map manager.
        Point::keep(0xb7f874usize, "49 8b 8e 18 04 00 00 e8 30 09 c6 ff", "map manager pause").uses(&[map_manager::PAUSE]),
        Point::keep(0x7e01cdusize, "48 8b 49 50", "pause owner").uses(&[pause::MANAGER]),
        Point::keep(0x82205cusize, "8b 5e 2c 8d 45 fe 83 f8 41 77 60", "map mode").uses(&[map_manager::MODE]),
        // Classes, where their constructors set them.
        Point::keep(0x1a20aeusize, "48 8d 05 8b 04 aa 01 48 89 03", "army class").uses(&[classes::ARMY]),
        Point::keep(0x909600usize, "48 8d 05 d9 11 3c 01 48 89 03", "fleet class").uses(&[classes::FLEET]),
        Point::keep(0x8032deusize, "48 8d 05 7b 51 4b 01 48 89 06", "map manager class").uses(&[classes::MAP_MANAGER]),
        Point::keep(0xe5180usize, "48 8d 05 71 82 c3 01", "move command class").uses(&[classes::MOVE_COMMANDS[0]]),
        Point::keep(0xd4c3ausize, "48 8d 05 47 8e c4 01", "move command class").uses(&[classes::MOVE_COMMANDS[1]]),
        Point::keep(0x26ce8eusize, "48 8d 05 63 11 9e 01 48 89 01", "recruit command class").uses(&[classes::RECRUIT_COMMAND]),
        // A province's snapshot (docs/research/provinces.md). The province
        // writer in saves: its name.
        Point::keep(0x9d6fb0usize, "49 8b 5f 28 48 83 c3 10", "province name").uses(&[province::GEOMETRY, geometry::NAME]),
        // The has_terrain trigger compares the province's terrain; the
        // map finds a terrain by its key among its terrains.
        Point::keep(0xcd915eusize, "48 8b 54 0a 28 48 8b 47 68 48 39 42 30", "province terrain").uses(&[province::GEOMETRY, geometry::TERRAIN_TYPE]),
        Point::keep(0x88a10ausize, "4c 8b b1 58 07 00 00 33 db 48 8b a9 60 07 00 00", "map terrains").uses(&[map::TERRAINS, map::TERRAINS + 8]),
        Point::keep(0x88a139usize, "48 81 c1 18 01 00 00", "terrain name").uses(&[terrain::NAME]),
        // The fort's defense: the level left while mothballed, or the
        // level.
        Point::keep(0xa0e890usize, "80 b9 f4 08 00 00 02 75 08 8b 81 dc 2d 00 00 eb 06 8b 81 e0 2d 00 00", "province fort level")
            .uses(&[province::MOTHBALL, province::MOTHBALLED as usize, province::FORT_LEVEL_MOTHBALLED, province::FORT_LEVEL]),
        // Whether a province has a building: the slot at the building's
        // index holds a building whose index is not 0.
        Point::keep(
            0x26a3dfusize,
            "4c 8b 8c 0a b0 03 00 00 48 8b 84 0a b8 03 00 00 49 2b c1 48 c1 f8 04 44 3b c0 7d 13 41 8b c0 48 03 c0 49 8b 04 c1 83 78 14 00",
            "province buildings",
        )
        .uses(&[province::BUILDINGS, province::BUILDINGS + 8, building::INDEX]),
        // The building database finds a building by its key.
        Point::keep(0x1e5ea0usize, "48 8b 33 48 8b d7 48 8d 4e 28", "building name").uses(&[building::NAME]),
        // The country writer in saves: each kind's unit type by its key,
        // for the seven kinds, in the order of the kinds' tokens.
        Point::keep(
            0x2e0191usize,
            "8b d7 49 8d 8f 48 0d 00 00 e8 81 de e3 ff 48 8b 08 e8 99 79 f8 ff 4c 8b c0 45 33 c9 41 8b d5 48 8b ce e8 08 7f 42 01 ff c7 83 ff 07",
            "country unit types",
        )
        .uses(&[country::UNIT_TYPES, country::UNIT_KINDS]),
        Point::keep(0x267b40usize, "48 8d 41 60 c3", "unit type name").uses(&[unit_type::NAME]),
        // infantry, cavalry, artillery, heavy_ship (these four in data),
        // light_ship, galley, transport.
        Point::keep(0x1dc2380usize, "81 27 00 00 80 27 00 00 82 27 00 00 84 27 00 00", "unit kind tokens"),
        Point::keep(
            0x2c7341usize,
            "66 0f 6f 05 37 b0 af 01 48 8d 0d 8c 39 19 02 f3 0f 7f 05 88 39 19 02 c7 05 8e 39 19 02 85 27 00 00 \
             c7 05 88 39 19 02 87 27 00 00 c7 05 82 39 19 02 86 27 00 00",
            "unit kind tokens",
        ),
        Point::keep(0xad8c5eusize, "48 8d 05 cb ab 20 01 48 89 01", "unit type class").uses(&[classes::UNIT_TYPE]),
        // Questions and offers (docs/research/questions-and-offers.md).
        // Removing a pending event: the world's array, each record's id.
        Point::keep(
            0x7570b8usize,
            "4c 63 81 ac 1e 00 00 33 ed 4c 8b f1 45 85 c0 0f 8e 82 01 00 00 4c 8b 91 a0 1e 00 00 45 33 c9 49 8d 42 24 0f 1f 44 00 00 \
             39 10 74 16 ff c5 49 ff c1 48 83 c0 28",
            "pending events",
        )
        .uses(&[world::PENDING_EVENT_COUNT, world::PENDING_EVENTS, pending_event::ID, pending_event::SIZE]),
        // Adding one: the event, its scope, the deadline and the id.
        Point::keep(
            0x757058usize,
            "49 63 c0 48 8d 0c 80 48 8b 45 08 48 8d 14 c8 41 8d 40 01 89 45 14 8b 44 24 28 89 42 08 4c 89 22 48 8b 44 24 30 48 89 42 10 \
             4c 89 72 18 8b 44 24 40 89 42 20 89 7a 24",
            "pending event record",
        )
        .uses(&[pending_event::EVENT, pending_event::DEADLINE, pending_event::ID]),
        // The map manager asking each popup whether it stays.
        Point::keep(0x6ea4e7usize, "49 8b 5c 24 78 48 85 db 74 58 48 8b 75 f7 48 8b 0b 48 8b 01 ff 50 18", "popups").uses(&[map_manager::POPUPS, popup::UPDATE]),
        // Adding a popup walks the list for one of its kind.
        Point::keep(
            0x6ecb14usize,
            "48 8b 5e 78 48 85 db 74 1c 0f 1f 00 48 8b 0b 48 8b 01 ff 50 08 84 c0 75 60 48 8b 43 10 48 8b d8 48 85 c0 75 e7",
            "map manager popups",
        )
        .uses(&[map_manager::POPUPS + list::HEAD, list::ITEM, list::NEXT]),
        // A popup that closes while the list is being walked is marked to go
        // later.
        Point::keep(0x6ea583usize, "41 80 bc 24 8c 00 00 00 00 74 06 c6 41 18 01", "popup gone").uses(&[map_manager::POPUPS + list::CHANGING, list::REMOVED]),
        // The event window's constructor: its class, the event, the scope,
        // the pending event's id, open.
        Point::keep(
            0x10b9b3busize,
            "48 8d 05 b6 f5 c9 00 48 89 03 48 8d 05 94 f5 c9 00 48 89 43 08 c7 43 30 ff ff ff ff 33 f6 48 89 73 38 48 89 7b 40 \
             4c 89 7b 48 48 8d 4b 50 49 8b d6 e8 2f 5b 01 ff 90 8b 45 60 89 83 20 01 00 00 40 88 b3 24 01 00 00",
            "event window",
        )
        .uses(&[classes::EVENT_WINDOW, event_window::EVENT, event_window::SCOPE, event_window::PENDING, event_window::OPEN]),
        // Choosing an option logs the event's id, and closes the window.
        Point::keep(0x10bc44cusize, "48 8b 56 48 48 83 c2 20", "event id").uses(&[event_window::EVENT, event::NAME]),
        Point::keep(0x10bc54busize, "c6 86 24 01 00 00 00", "event window closed").uses(&[event_window::OPEN]),
        // The event window's title and options.
        Point::keep(0x10ba2bfusize, "49 8b 57 48 48 81 c2 e0 07 00 00", "event title").uses(&[event_window::EVENT, event::TITLE]),
        Point::keep(0x6f2778usize, "48 8b 80 98 07 00 00 48 2b 87 90 07 00 00 48 c1 f8 03", "event options").uses(&[event::OPTIONS + 8, event::OPTIONS]),
        Point::keep(0x10bb6a8usize, "48 8d 97 88 00 00 00 48 8d 4d b8 e8 c8 9b fd fe", "event option text").uses(&[event_option::NAME]),
        // A popup that goes leaves the open dialogs too.
        Point::keep(0x6ea5deusize, "49 8b 8c 24 70 03 00 00 48 85 c9 74 79 0f 1f 44 00 00 48 39 39 74 0e", "open dialogs").uses(&[map_manager::OPEN_DIALOGS]),
        // The alliance dialog showing both countries of its action.
        Point::keep(0x1067b19usize, "49 8b 46 10 48 c1 e8 20", "diplomatic action maker").uses(&[diplomatic_action::FROM]),
        // An action reaching the player gets its id.
        Point::keep(0x4e82a9usize, "89 43 3c", "diplomatic action id").uses(&[diplomatic_action::ID]),
    ]
}
