//! What a world reset leaks: a destructor or a clear that lets an object go
//! without what it owns. A country's array of bytes, a mercenary company's
//! leader, a country's rebel factions and the agents its AI started are let
//! go with them.
use super::LettingGo;
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::{
        campaign::{ai_king, classes, countries as country_list, country, functions, globals, leader, mercenary_company, world},
        list,
    },
    points::Point,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

/// `(country)`: a country's destructor, which every world reset runs for
/// every country, as the country tags are set up again.
const COUNTRY_DESTRUCTOR: usize = 0x2cbf20;
/// Where the world's own reset begins to clear the world (`0x74a260`), as
/// it marks it as being cleared (`mov byte ptr [rsi + world::CLEARING], 1`);
/// and what follows. Its clear then clears every country.
const WORLD_CLEARING: usize = 0x74a289;
const WORLD_CLEARING_DONE: usize = 0x74a290;
/// `(faction, flags)`, `(leader, flags)`: the deleting destructors of a rebel
/// faction and of a leader, their vtables' first slots.
const FACTION_DELETE: usize = 0xa94480;
const LEADER_DELETE: usize = 0x860f60;
/// `(company, flags)`: a mercenary company's destructor, which its vtable's
/// first slot is; a country lets the companies it can hire go by it as it
/// changes them, and in its clear.
const COMPANY_DESTRUCTOR: usize = 0x8fdb50;
/// `(agent, flags)`: the deleting destructor of the agent at the top of a
/// country's AI, its vtable's first slot, which the country's AI calls as
/// it goes; and the destructor it calls, which the game also runs alone for
/// agents it makes for a moment on the stack.
const AI_KING_DELETE: usize = 0x148260;

/// What the repair let go since the last world reset, for the log:
/// mercenary companies' leaders, rebel factions and the AI's agents.
pub(super) static LEADERS: AtomicUsize = AtomicUsize::new(0);
pub(super) static FACTIONS: AtomicUsize = AtomicUsize::new(0);
pub(super) static AGENTS: AtomicUsize = AtomicUsize::new(0);

/// The agents of the AI whose top agent is being deleted, by that agent:
/// they are deleted once it is gone.
static AGENTS_LEFT: Mutex<Vec<(usize, Vec<usize>)>> = Mutex::new(Vec::new());

/// A country's destructor begins: its array of bytes
/// ([`country::ENTRY_BYTES`]), which the destructor leaves, is freed.
extern "C" fn country_going(frame: *mut HookFrame) {
    crate::guarded("country destructor", || {
        let m = crate::hooked_memory();
        let country = unsafe { (*frame).args[0] };
        let freed = (|| {
            let bytes = m.ptr(country + country::ENTRY_BYTES)?;
            if bytes != 0 {
                m.write(country + country::ENTRY_BYTES, &0usize.to_le_bytes())?;
                let _letting_go = LettingGo::new("country's array of bytes");
                crate::game_heap::delete(&m, bytes, 0)?;
            }
            crate::Result::Ok(())
        })();
        if let Err(e) = freed {
            log!("repair: a country's array of bytes was not freed: {e}");
        }
    });
}

/// A mercenary company's destructor begins: its leader, which the
/// destructor leaves, is deleted, unless it is the game's null leader. The
/// leader's own destructor takes it off its army and its country. Where the
/// game hands a company's leader on, it gives the company the null leader
/// in its place.
extern "C" fn company_going(frame: *mut HookFrame) {
    crate::guarded("mercenary company destructor", || {
        let m = crate::hooked_memory();
        let company = unsafe { (*frame).args[0] };
        let deleted = (|| {
            let leader = m.ptr(company + mercenary_company::LEADER)?;
            if leader == 0 || m.call(m.vslot(leader, leader::REAL)?, &[leader])? as u8 == 0 {
                return Ok(());
            }
            m.write(company + mercenary_company::LEADER, &0usize.to_le_bytes())?;
            let _letting_go = LettingGo::new("mercenary company's leader");
            m.call(m.vslot(leader, 0)?, &[leader, 1])?;
            LEADERS.fetch_add(1, Ordering::Relaxed);
            crate::Result::Ok(())
        })();
        if let Err(e) = deleted {
            log!("repair: a mercenary company's leader was not deleted: {e}");
        }
    });
}

/// The world's own reset begins to clear the world: every country's rebel
/// factions, whose vector a country's clear (`0x2e1a30`) lets go without
/// them, are deleted first, before any country is cleared. Not as each
/// country is: a country's clear destroys its history, whose records may
/// hold leaders of another country's factions (rebels that went over to it,
/// `0xa9e160`), and a faction's destructor deletes its leaders whoever holds
/// them. A faction's destructor takes it off its countries' vectors and,
/// with the world being cleared, leaves its provinces be.
extern "C" fn world_clearing() {
    crate::guarded("world clear", || {
        let m = crate::hooked_memory();
        let deleted = (|| {
            let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
            let list = m.ptr(manager + country_list::LIST)?;
            for index in 0..m.i32(manager + country_list::COUNT)?.max(0) as usize {
                let country = m.ptr(list + index * 8)?;
                if country == 0 {
                    continue;
                }
                let vector = country + country::REBEL_FACTIONS;
                let (begin, end) = (m.ptr(vector)?, m.ptr(vector + 8)?);
                let factions = (begin..end).step_by(8).map(|at| m.ptr(at)).collect::<crate::Result<Vec<_>>>()?;
                for faction in factions.into_iter().filter(|&f| f != 0) {
                    let _letting_go = LettingGo::new("rebel faction");
                    m.call(m.vslot(faction, 0)?, &[faction, 1])?;
                    FACTIONS.fetch_add(1, Ordering::Relaxed);
                }
                // Each took itself off; none is left for the clear.
                m.write(vector + 8, &m.ptr(vector)?.to_le_bytes())?;
            }
            crate::Result::Ok(())
        })();
        if let Err(e) = deleted {
            log!("repair: the rebel factions were not deleted: {e}");
        }
        super::histories::world_clearing(&m);
    });
}

/// The items of one of the game's lists.
fn list_items(m: &Memory, header: usize) -> crate::Result<Vec<usize>> {
    let mut items = Vec::new();
    let mut node = m.ptr(header + list::HEAD)?;
    while node != 0 {
        items.push(m.ptr(node + list::ITEM)?);
        node = m.ptr(node + list::NEXT)?;
    }
    Ok(items)
}

/// The agent at the top of a country's AI is being deleted: the agents it
/// started, and those that retired since its last update, which its
/// destructor leaves, are kept to be deleted once it is gone
/// ([`ai_king_gone`]), when nothing of it can use them any more.
extern "C" fn ai_king_going(frame: *mut HookFrame) {
    crate::guarded("AI king delete", || {
        let (king, flags) = unsafe { ((*frame).args[0], (*frame).args[1]) };
        if flags & 1 == 0 {
            return;
        }
        let m = crate::hooked_memory();
        let kept = (|| {
            let mut agents = list_items(&m, king + ai_king::AGENTS)?;
            agents.extend(list_items(&m, king + ai_king::RETIRED)?);
            agents.retain(|&a| a != 0);
            crate::Result::Ok(agents)
        })();
        match kept {
            Ok(agents) => AGENTS_LEFT.lock().unwrap_or_else(|e| e.into_inner()).push((king, agents)),
            Err(e) => log!("repair: the agents of a country's AI were not read: {e}"),
        }
    });
}

/// The agent at the top of a country's AI is gone: the agents it left are
/// deleted, as its update deletes those that retire.
extern "C" fn ai_king_gone(frame: *mut HookFrame) {
    crate::guarded("AI king deleted", || {
        let king = unsafe { (*frame).args[0] };
        let agents = {
            let mut left = AGENTS_LEFT.lock().unwrap_or_else(|e| e.into_inner());
            match left.iter().rposition(|(k, _)| *k == king) {
                Some(at) => left.swap_remove(at).1,
                None => return,
            }
        };
        let m = crate::hooked_memory();
        for agent in agents {
            let _letting_go = LettingGo::new("agent of a country's AI");
            match m.vslot(agent, 0).and_then(|delete| m.call(delete, &[agent, 1])) {
                Ok(_) => {
                    AGENTS.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => log!("repair: an agent of a country's AI was not deleted: {e}"),
            }
        }
    });
}

crate::resume!(WORLD_CLEARING_RESUME: WORLD_CLEARING_NEXT = WORLD_CLEARING_DONE);

/// Replaces the world's clear marking the world as being cleared: marks it,
/// then [`world_clearing`].
#[unsafe(naked)]
extern "C" fn world_clearing_stub() {
    std::arch::naked_asm!(
        "mov byte ptr [rsi + {clearing}], 1",
        crate::call_rust!(call clearing_fn),
        "jmp qword ptr [rip + {next}]",
        clearing = const world::CLEARING,
        clearing_fn = sym world_clearing,
        next = sym WORLD_CLEARING_NEXT,
    );
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(
            COUNTRY_DESTRUCTOR,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 ec 20 48 8b d9",
            "country destructor",
            1,
            Some(country_going),
            None,
        ),
        // The country's deleting destructor.
        Point::keep(0x28651fusize, "e8 fc 59 04 00 f6 c3 01 74 0d ba 78 9b 00 00", "country deleting destructor"),
        Point::jump(WORLD_CLEARING, "c6 86 85 74 00 00 01", "world clearing", world_clearing_stub as *const () as usize, &WORLD_CLEARING_RESUME)
            .uses(&[world::CLEARING]),
        Point::keep(WORLD_CLEARING_DONE, "c7 44 24 50 65 01 00 00", "world clearing marked"),
        // The world's own reset clears every country.
        Point::keep(0x74ad92usize, "48 8b 8e e8 1c 00 00 48 8b 0c c1 e8 8e 6c b9 ff", "world reset country clear"),
        // A rebel faction and a leader go by their first slots; a leader
        // answers that it is one, the game's null leader that it is not.
        Point::method(classes::REBEL_FACTION, 0, FACTION_DELETE, "rebel faction deleting destructor"),
        Point::method(classes::LEADER, 0, LEADER_DELETE, "leader deleting destructor"),
        Point::method(classes::LEADER, leader::REAL, functions::YES, "a leader is one"),
        Point::method(classes::NULL_LEADER, leader::REAL, functions::NO, "the null leader is none"),
        // A mercenary company's destructor, which leaves its leader: it only
        // takes the company's ID off and frees it.
        Point::observe(
            COMPANY_DESTRUCTOR,
            "48 89 5c 24 08 57 48 83 ec 20 48 8d 05 77 ee 48 01 8b da 48 89 01",
            "mercenary company destructor",
            2,
            Some(company_going),
            None,
        ),
        Point::keep(0x8fdb69usize, "e8 d2 19 bd 00 f6 c3 01 74 0d ba 68 00 00 00", "mercenary company destructor end"),
        // The deleting destructor of the agent at the top of a country's AI,
        // which leaves its agents; its update deletes those that retire by
        // their first slots.
        Point::observe(
            AI_KING_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 2c 00 00 00 f6 c3 01 74 0d ba 58 0d 00 00",
            "AI king deleting destructor",
            2,
            Some(ai_king_going),
            Some(ai_king_gone),
        ),
        Point::method(classes::AI_KING, 0, AI_KING_DELETE, "AI king deleting destructor method"),
        Point::keep(0x14b2f1usize, "48 8b 0b 48 8b 5b 10 48 85 c9 74 0a 48 8b 01 ba 01 00 00 00 ff 10", "AI retired agents deleted by their first slots"),
    ]
}
