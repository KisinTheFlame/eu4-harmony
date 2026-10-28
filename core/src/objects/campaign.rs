//! The campaign's objects: the world, countries, provinces and their map
//! data, units and their routes, battles, commands and the map manager
//! (docs/research/regional-combat/).

/// Globals, by RVA.
pub mod globals {
    /// The world, the campaign's state; null when no campaign runs.
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
}

/// The country manager, at [`globals::COUNTRY_MANAGER`].
pub mod countries {
    /// The countries, pointers by tag index.
    pub const LIST: usize = 0x118;
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
}

/// A node of the game's linked lists: the item, then the next node.
pub mod list {
    pub const ITEM: usize = 0;
    pub const NEXT: usize = 0x10;
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
    /// Its route, a [`super::path`].
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

/// A route: a doubly linked list of provinces.
pub mod path {
    pub const HEAD: usize = 0;
    pub const TAIL: usize = 8;
    /// i32, and a byte that is nonzero while it is being changed.
    pub const COUNT: usize = 0x10;
    pub const CHANGING: usize = 0x14;
    pub const SIZE: usize = 0x18;
    /// A node: the province's ID (i32), the previous and next node, and a
    /// byte set when it is removed.
    pub const NODE_SIZE: usize = 0x20;
    pub const PROVINCE: usize = 0;
    pub const PREVIOUS: usize = 8;
    pub const NEXT: usize = 0x10;
    pub const REMOVED: usize = 0x18;
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

/// The map manager, at [`world::MAP_MANAGER`].
pub mod map_manager {
    /// The current map mode, i32.
    pub const MODE: usize = 0x2c;
    /// Its pause state (see [`super::pause`]).
    pub const PAUSE: usize = 0x418;
    /// Its refresh method, by vtable offset.
    pub const REFRESH: usize = 0xc8;
}

pub mod pause {
    /// The map manager it belongs to.
    pub const MANAGER: usize = 0x50;
}

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
        Point::keep(0xb252eusize, "49 8b 89 48 15 00 00 4d 63 81 54 15 00 00 49 c1 e0 04 4c 03 c1", "country wars")
            .uses(&[country::HOSTILE, country::HOSTILE_COUNT, 4]),
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
            .uses(&[unit::PATH + path::HEAD, unit::PATH + path::TAIL, unit::PATH + path::COUNT, unit::PATH + path::CHANGING]),
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
        .uses(&[unit::PATH + path::HEAD, path::NEXT, path::NODE_SIZE, unit::PATH + path::TAIL, unit::PATH + path::COUNT]),
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
            unit::PATH + path::TAIL,
            path::NODE_SIZE,
            path::PROVINCE,
            path::PREVIOUS,
            path::NEXT,
            path::REMOVED,
            unit::PATH + path::COUNT,
            unit::PATH + path::HEAD,
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
    ]
}
