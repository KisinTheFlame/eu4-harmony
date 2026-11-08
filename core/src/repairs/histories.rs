//! What the records of the histories leave as they go: the monarchs, heirs
//! and queens a country's history holds (docs/research/main-menu.md, "The
//! monarchs a history keeps"), and the battles a war's history holds.
//!
//! A person is let go by the characters' manager, which deletes every one on
//! the vector of all persons, but only those made while it reads characters
//! are put there; a history's reader makes the others, and keeps each in a
//! record of the history, by pointer. A record's destructor leaves its
//! persons, and nothing else deletes them. Several records may hold one
//! person (the reader takes the one already registered under an ID; a
//! record that sets an heir or a queen keeps the one before), and a world
//! reset still uses them after their records are gone, so a person cannot
//! go with its record.
//!
//! So a person a record held as it went, and that is on no vector, waits:
//! once a world reset is over, at the first frame after it, those that
//! waited from before its end are deleted, unless the game deleted them
//! itself in the meantime, put them on the vector, still finds them by their
//! ID, or has them as a country's monarch, heir or queen. Those that began
//! to wait after the reset's end, as a load replaces the main menu's
//! histories with the save's, wait for the next reset: until then a record of
//! the session may still keep one as the person before it.
//!
//! A war's history (`warhistory.cpp`) keeps each battle it reads in a
//! record, by pointer, and the game deletes no battle it made so: a battle
//! waits as its record goes, as a person does, and goes as a person does,
//! with nothing else to keep it.
//!
//! A diplomatic history (`diplomatichistory.cpp`) keeps each relation it
//! reads in a record, and in a second one where it ends; one that takes
//! effect goes to the diplomacy, which marks it and deletes it itself, and
//! the others nothing deletes. A relation that was not given to the
//! diplomacy waits as its records go, and is deleted as the world's clear
//! begins once a return has taken the session's interface down after it
//! began to wait: a relation's destructor updates its country unless the
//! world is being cleared, and the interface may show it until it goes. A relation the game deleted while a record still
//! held it, as it does with one the diplomacy would not take, is not waited
//! for.
use super::waiting::{self, Moment, Waiting};
use crate::{game::world::FIND_ID, hooks::Call, memory::Memory, points::Point};
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

/// `(record, flags)`: the deleting destructor of a monarch's record, the
/// first slot of [`MONARCH_RECORD`]; and that of the 0x50-byte records,
/// which heirs' and queens' records share with other kinds of a history's
/// records.
const MONARCH_RECORD_DELETE: usize = 0x29bdc0;
const SMALL_RECORD_DELETE: usize = 0x29a7d0;
/// The records' vtables: a monarch's holds four persons ([`MONARCH_PERSONS`]),
/// an heir's and a queen's two ([`SMALL_PERSONS`]).
const MONARCH_RECORD: usize = 0x1c51630;
const HEIR_RECORD: usize = 0x1c51220;
const QUEEN_RECORD: usize = 0x1c512f0;
const MONARCH_PERSONS: [usize; 4] = [0x40, 0x48, 0x50, 0x58];
const SMALL_PERSONS: [usize; 2] = [0x40, 0x48];
/// `(person)`: the destructor every monarch, heir and queen goes through:
/// it takes the person off the vector of all persons and off the registry.
const PERSON_DESTRUCTOR: usize = 0xa48fa0;
/// The persons' classes and their deleting destructors.
const MONARCH: usize = 0x1cdcdd8;
const HEIR: usize = 0x1c52240;
const QUEEN: usize = 0x1c522f8;
const MONARCH_DELETE: usize = 0xa48a30;
const HEIR_DELETE: usize = 0x296940;
const QUEEN_DELETE: usize = 0x296a40;
/// `(record, flags)`: the deleting destructor of a battle's record of a
/// war's history, which the 0x48-byte records of the war history share; a
/// battle's record ([`BATTLE_RECORD`]) holds its battle at [`BATTLE`].
const BATTLE_RECORD_DELETE: usize = 0x4e2020;
const BATTLE_RECORD: usize = 0x1d210b8;
const BATTLE: usize = 0x40;
/// `(battle, flags)`: a battle's class and its deleting destructor.
const BATTLE_CLASS: usize = 0x1c448d0;
const BATTLE_DELETE: usize = 0x1dd060;
/// The records of a diplomatic history, which share [`BATTLE_RECORD_DELETE`]
/// and hold their relation at [`RELATION`].
const DIPLOMACY_RECORDS: [usize; 6] = [0x1c7a960, 0x1c7ab00, 0x1c7abd0, 0x1c7a890, 0x1c7aa30, 0x1c7a7c0];
const RELATION: usize = 0x40;
/// `(kind) -> relation`: makes a relation of a kind, as the diplomatic
/// history's reader does; `(relation)`: every relation's base destructor.
const MAKE_RELATION: usize = 0xa5c220;
const RELATION_DESTRUCTOR: usize = 0xa5bb00;
/// A relation's flags, u8: [`PERMITTED`] lets its destructor go without an
/// error, [`GIVEN`] is set as the diplomacy takes it.
const PERMITTED: usize = 0x28;
const GIVEN: usize = 0x29;
/// The vector of all persons (begin, end), which the characters' manager's
/// clear (`0x7e9820`) deletes; and the null monarch, heir and queen, made
/// once, which records hold until they are given a person.
const ALL_PERSONS: usize = 0x23581a0;
const NULL_PERSONS: [usize; 3] = [0x233ffa0, 0x233ffa8, 0x233ffb0];
/// A person's ID, as the registry finds it.
const ID: usize = 0x10;
/// A country's monarch, heir and queen.
const COUNTRY_PERSONS: [usize; 3] = [0x1280, 0x1298, 0x12b0];

/// What a record held as it went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Left {
    Person,
    Battle,
    Relation,
}

impl Left {
    /// What it waits for: a relation, which the session's interface may
    /// show, a return; a person or a battle, which a world reset may still
    /// use, the reset's end.
    fn waits_for(self) -> Moment {
        match self {
            Left::Relation => Moment::Return,
            Left::Person | Left::Battle => Moment::Reset,
        }
    }
}

/// What the records left, which waits.
static WAITING: Waiting<Left> = Waiting::new();
/// What the repair deleted, and what it kept waiting, for the log.
static PERSONS: AtomicUsize = AtomicUsize::new(0);
static PERSONS_KEPT: AtomicUsize = AtomicUsize::new(0);
static BATTLES: AtomicUsize = AtomicUsize::new(0);
static RELATIONS: AtomicUsize = AtomicUsize::new(0);
static RELATIONS_KEPT: AtomicUsize = AtomicUsize::new(0);
/// The relations the game deleted, until a relation is made at their place:
/// a record may still hold one.
static GONE: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair {
    points,
    since_reset: Some(since_reset),
    world_clearing: Some(world_clearing),
    first_frame_after_reset: Some(let_go),
    ..super::Repair::NONE
};

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!(
        "{} monarchs, heirs and queens the histories left were deleted, {} kept as the game still had them; {} battles the wars' histories left were deleted; {} relations the diplomatic histories left were deleted, {} kept as the diplomacy was given them",
        PERSONS.swap(0, Ordering::Relaxed),
        PERSONS_KEPT.swap(0, Ordering::Relaxed),
        BATTLES.swap(0, Ordering::Relaxed),
        RELATIONS.swap(0, Ordering::Relaxed),
        RELATIONS_KEPT.swap(0, Ordering::Relaxed)
    )
}

/// A record of a history is being deleted: the persons it holds that are not
/// null wait, and so does a battle.
fn record_going(call: &mut Call) {
    let (record, flags) = (call.arg(0), call.arg(1));
    if flags & 1 == 0 {
        return;
    }
    let m = call.memory();
    let kept = (|| {
        let class = m.ptr(record)?;
        if class == m.at(BATTLE_RECORD) {
            let battle = m.ptr(record + BATTLE)?;
            return Ok(if battle == 0 { Vec::new() } else { vec![(battle, Left::Battle)] });
        }
        if DIPLOMACY_RECORDS.iter().any(|&c| class == m.at(c)) {
            let relation = m.ptr(record + RELATION)?;
            let gone = GONE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|g| g.contains(&relation));
            return Ok(if relation == 0 || gone { Vec::new() } else { vec![(relation, Left::Relation)] });
        }
        let fields: &[usize] = if class == m.at(MONARCH_RECORD) {
            &MONARCH_PERSONS
        } else if class == m.at(HEIR_RECORD) || class == m.at(QUEEN_RECORD) {
            &SMALL_PERSONS
        } else {
            return Ok(Vec::new());
        };
        let nulls = null_persons(&m)?;
        let mut persons = Vec::new();
        for field in fields {
            let person = m.ptr(record + field)?;
            if person != 0 && !nulls.contains(&person) {
                persons.push((person, Left::Person));
            }
        }
        crate::Result::Ok(persons)
    })();
    match kept {
        Ok(left) => {
            for (object, kind) in left {
                WAITING.wait(object, kind, kind.waits_for());
            }
        }
        Err(e) => log!("repair: what a history record holds was not read: {e}"),
    }
}

/// A person or a battle is destroyed, by the game or the repair: it waits
/// no more.
fn going(call: &mut Call) {
    WAITING.withdraw(call.arg(0));
}

/// A relation is destroyed, by the game or the repair: it waits no more, and
/// a record that still holds it does not make it wait.
fn relation_going(call: &mut Call) {
    let relation = call.arg(0);
    WAITING.withdraw(relation);
    GONE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).insert(relation);
}

/// A relation is made where one the game deleted was.
fn relation_made(call: &mut Call) {
    let relation = call.result().unwrap_or(0);
    if relation != 0 {
        if let Some(gone) = GONE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            gone.remove(&relation);
        }
    }
}

/// The world's clear begins: the relations that waited from before the last
/// return are deleted, unless the diplomacy was given one: the session's
/// interface may show them until it goes.
fn world_clearing(m: &Memory) {
    let due = WAITING.take_due(Moment::Return);
    let deleted = (|| {
        for (relation, _) in due {
            if m.u8(relation + GIVEN)? != 0 {
                RELATIONS_KEPT.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            m.write(relation + PERMITTED, &[1u8])?;
            waiting::delete_by_its_class(m, relation, "diplomatic history's relation")?;
            RELATIONS.fetch_add(1, Ordering::Relaxed);
        }
        crate::Result::Ok(())
    })();
    if let Err(e) = deleted {
        log!("repair: the relations the diplomatic histories left were not all deleted: {e}");
    }
}

/// The first frame after a world reset: what waited from before its end is
/// deleted, unless the game still has a person; relations wait for the
/// world's next clear.
fn let_go(m: &Memory) {
    let due = WAITING.take_due(Moment::Reset);
    if due.is_empty() {
        return;
    }
    let deleted = (|| {
        let in_use = in_use(m)?;
        for (object, kind) in due {
            if kind == Left::Battle {
                waiting::delete_by_its_class(m, object, "war history's battle")?;
                BATTLES.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let person = object;
            if in_use.contains(&person) {
                PERSONS_KEPT.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let id = m.u64(person + ID)?;
            if FIND_ID.call(m, [&id as *const u64 as usize])? == person {
                PERSONS_KEPT.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            waiting::delete_by_its_class(m, person, "history's person")?;
            PERSONS.fetch_add(1, Ordering::Relaxed);
        }
        crate::Result::Ok(())
    })();
    if let Err(e) = deleted {
        log!("repair: what the histories left was not all deleted: {e}");
    }
}

/// The game's null persons.
fn null_persons(m: &Memory) -> crate::Result<[usize; 3]> {
    Ok([m.ptr(m.at(NULL_PERSONS[0]))?, m.ptr(m.at(NULL_PERSONS[1]))?, m.ptr(m.at(NULL_PERSONS[2]))?])
}

/// The persons the game still has by more than their records: those on the
/// vector of all persons, the null ones, and every country's monarch, heir
/// and queen.
fn in_use(m: &Memory) -> crate::Result<HashSet<usize>> {
    let mut found: HashSet<usize> = null_persons(m)?.into_iter().collect();
    let (begin, end) = (m.ptr(m.at(ALL_PERSONS))?, m.ptr(m.at(ALL_PERSONS) + 8)?);
    for at in (begin..end).step_by(8) {
        found.insert(m.ptr(at)?);
    }
    for country in crate::containers::countries(m)? {
        for field in COUNTRY_PERSONS {
            found.insert(m.ptr(country + field)?);
        }
    }
    Ok(found)
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(
            MONARCH_RECORD_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8d 05 1f 62 9b 01 8b da 48 89 01 48 8b f9 e8 32 be 54 00",
            "monarch record deleting destructor",
            2,
            Some(record_going),
            None,
        ),
        Point::observe(
            SMALL_RECORD_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8d 05 0f 78 9b 01 8b da 48 89 01 48 8b f9 e8 22 d4 54 00",
            "history record deleting destructor",
            2,
            Some(record_going),
            None,
        ),
        Point::observe(
            PERSON_DESTRUCTOR,
            "40 53 48 83 ec 20 48 8d 05 2b 3e 29 01 48 89 4c 24 30 48 8d 54 24 30 48 89 01 48 8b d9 e8 fe 37 da ff",
            "person destructor",
            1,
            Some(going),
            None,
        ),
        Point::observe(
            BATTLE_RECORD_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 dc 5b 30 00",
            "war history record deleting destructor",
            2,
            Some(record_going),
            None,
        ),
        Point::observe(
            BATTLE_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 8b da 48 81 c1 18 01 00 00 e8 e5 85 eb ff",
            "battle deleting destructor",
            2,
            Some(going),
            None,
        ),
        Point::method(BATTLE_RECORD, 0, BATTLE_RECORD_DELETE, "battle record deleting destructor method"),
        Point::observe(
            RELATION_DESTRUCTOR,
            "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 48 8d 6c 24 c0",
            "relation destructor",
            1,
            Some(relation_going),
            None,
        ),
        Point::observe(MAKE_RELATION, "48 89 5c 24 08 48 89 7c 24 10 55 48 8b ec 48 83 ec 40", "relation made", 1, None, Some(relation_made)),
        // The diplomatic history's reader gives a record its relation, the
        // diplomacy marks one it takes, and a relation's destructor lets one
        // go without an error when it is permitted or was given.
        Point::keep(0x4e2f72usize, "e8 69 0c 00 00 48 8b d8 48 89 78 40", "diplomacy record's relation").uses(&[RELATION]),
        Point::keep(0x4e1504usize, "c6 47 29 01", "relation given to the diplomacy").uses(&[GIVEN]),
        Point::keep(0xa5bb61usize, "80 7f 28 00 0f 85 12 02 00 00 80 7f 29 00", "relation deleted where permitted").uses(&[PERMITTED, GIVEN]),
        Point::method(DIPLOMACY_RECORDS[0], 0, BATTLE_RECORD_DELETE, "diplomacy record deleting destructor method"),
        Point::method(DIPLOMACY_RECORDS[1], 0, BATTLE_RECORD_DELETE, "diplomacy record deleting destructor method"),
        Point::method(DIPLOMACY_RECORDS[2], 0, BATTLE_RECORD_DELETE, "diplomacy record deleting destructor method"),
        Point::method(DIPLOMACY_RECORDS[3], 0, BATTLE_RECORD_DELETE, "diplomacy record deleting destructor method"),
        Point::method(DIPLOMACY_RECORDS[4], 0, BATTLE_RECORD_DELETE, "diplomacy record deleting destructor method"),
        Point::method(DIPLOMACY_RECORDS[5], 0, BATTLE_RECORD_DELETE, "diplomacy record deleting destructor method"),
        Point::method(BATTLE_CLASS, 0, BATTLE_DELETE, "battle deleting destructor method"),
        // The war history's reader gives a battle's record its battle.
        Point::keep(0xda6575usize, "4c 89 72 40", "battle record's battle").uses(&[BATTLE]),
        Point::method(MONARCH_RECORD, 0, MONARCH_RECORD_DELETE, "monarch record deleting destructor method"),
        Point::method(HEIR_RECORD, 0, SMALL_RECORD_DELETE, "heir record deleting destructor method"),
        Point::method(QUEEN_RECORD, 0, SMALL_RECORD_DELETE, "queen record deleting destructor method"),
        Point::method(MONARCH, 0, MONARCH_DELETE, "monarch deleting destructor"),
        Point::method(HEIR, 0, HEIR_DELETE, "heir deleting destructor"),
        Point::method(QUEEN, 0, QUEEN_DELETE, "queen deleting destructor"),
        // The records' persons, as their constructors set them.
        Point::keep(
            0x29bd8dusize,
            "e8 de cc 7a 00 48 89 43 40 e8 d5 cc 7a 00 48 89 43 48 e8 7c ab 7b 00 48 89 43 50 e8 c3 bd 7b 00 48 89 43 58",
            "monarch record persons",
        )
        .uses(&MONARCH_PERSONS),
        Point::keep(0x29c17dusize, "e8 9e a7 7b 00 48 89 43 40 e8 95 a7 7b 00 48 89 43 48", "heir record persons").uses(&SMALL_PERSONS),
        Point::keep(0x29c2bdusize, "e8 ae b8 7b 00 48 89 43 40 e8 a5 b8 7b 00 48 89 43 48", "queen record persons").uses(&SMALL_PERSONS),
        // A person goes on the vector of all persons as it is made, if at all.
        Point::keep(0xa48c76usize, "48 8b 15 2b f5 90 01 48 3b 15 2c f5 90 01 74 0d 48 89 1a 48 83 05 17 f5 90 01 08", "person on the vector of all persons")
            .uses(&[ALL_PERSONS + 8]),
        // The characters' manager's clear reads the vector, end and begin.
        Point::keep(0x7e996dusize, "4c 8b 35 34 e8 b6 01 48 8b 3d 25 e8 b6 01", "the vector of all persons cleared").uses(&[ALL_PERSONS, ALL_PERSONS + 8]),
        Point::keep(0xa48a76usize, "48 8b 05 23 75 8f 01", "null monarch").uses(&[NULL_PERSONS[0]]),
        Point::keep(0xa56926usize, "48 8b 05 7b 96 8e 01", "null heir").uses(&[NULL_PERSONS[1]]),
        Point::keep(0xa57b7ausize, "48 8b 05 2f 84 8e 01", "null queen").uses(&[NULL_PERSONS[2]]),
        // The reader finds a person by its ID as the registry keeps it.
        Point::keep(0x299e38usize, "49 8d 4c 24 10", "history person's ID").uses(&[ID]),
        // A country's monarch, as it is set; its heir and queen, as a record
        // keeps the one before.
        Point::keep(0x311a7fusize, "4d 89 af 80 12 00 00", "country monarch").uses(&[COUNTRY_PERSONS[0]]),
        Point::keep(0x29c1dbusize, "49 8b 82 98 12 00 00", "country heir").uses(&[COUNTRY_PERSONS[1]]),
        Point::keep(0x29c30dusize, "49 8b 82 b0 12 00 00", "country queen").uses(&[COUNTRY_PERSONS[2]]),
    ]
}
