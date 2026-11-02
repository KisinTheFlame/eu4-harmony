//! Repairs (GLOSSARY): the game's own defects the framework puts right, each
//! where the game goes wrong, so that they hold in vanilla's paths too
//! (docs/research/main-menu.md).
//!
//! - What a world reset leaks: a destructor or a clear that lets an object
//!   go without what it owns. A country's array of bytes, a mercenary
//!   company's leader, a country's rebel factions and the agents its AI
//!   started are let go with them.
//! - What a country's modifiers leak as they go or are made again, in play
//!   too: the sets of modifiers they copy, which they alone point to.
//! - What a world reset leaves of a session, which vanilla never meets
//!   outside a load, whose save sets it again: put back as a new start has
//!   it.
//! - What of the session's interface the game never lets go, which vanilla
//!   never meets, as it restarts instead: the popups its message windows
//!   hold, let go before an in-process return takes the interface down, and
//!   what the map manager's destructor leaves, let go as it goes.
//! - What of its interface the game leaves as it lets a part of it go, at
//!   every interface rebuild in vanilla too: the map modes' window, which it
//!   never kills, and the outliner's entries, which it never deletes.
//! - What the game unloads while it is shown, which vanilla never meets, as
//!   it makes the main menu only at startup: a sprite type shown sprites
//!   still count, which the main menu, made again, would unload with the
//!   game's graphics, stays loaded.
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::{
        campaign::{
            achievements, ai_king, classes, countries as country_list, country, globals, leader, map_manager as manager_fields, map_modes, mercenary_company,
            message_window, modifiers, outliner, religions, world,
        },
        gui::{classes as gui_classes, sprite_type, window as gui_window},
        list,
    },
    points::{Function, Point},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

/// `(block, size)`: the game's `operator delete`.
const DELETE: usize = 0x17394f0;
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
/// `()`: answers yes; answers no.
const YES: usize = 0x8f440;
const NO: usize = 0x8f450;
/// `(company, flags)`: a mercenary company's destructor, which its vtable's
/// first slot is; a country lets the companies it can hire go by it as it
/// changes them, and in its clear.
const COMPANY_DESTRUCTOR: usize = 0x8fdb50;
/// `(world)`: the world's own reset, inside the game's and at startup.
const WORLD_RESET: usize = 0x749020;
/// `(map manager)`: the map manager's destructor, which its deleting
/// destructor (its vtable's first slot) calls as the application's idler
/// switch lets it go.
const MAP_MANAGER_DESTRUCTOR: usize = 0x80b570;
/// `(message icon holder)`: empties it of its icons.
const EMPTY_MESSAGE_ICONS: usize = 0xdffca0;
/// `(sprite type)`: unloads a sprite type with its group of graphics
/// (`0x14dc240`), letting its textures go; its [`sprite_type::UNLOAD`].
const SPRITE_TYPE_UNLOAD: usize = 0x156d730;
/// `(agent, flags)`: the deleting destructor of the agent at the top of a
/// country's AI, its vtable's first slot, which the country's AI calls as
/// it goes; and the destructor it calls, which the game also runs alone for
/// agents it makes for a moment on the stack.
const AI_KING_DELETE: usize = 0x148260;

/// `(map modes' panel)`, `(outliner)`: let the map manager's map modes'
/// panel and its outliner go, which its interface rebuild does before it
/// makes them again, and the framework as the map manager goes.
const LET_MAP_MODES_GO: usize = 0x81de20;
const LET_OUTLINER_GO: usize = 0x81dc30;

/// `(modifiers)`: the destructor of a country's modifiers; `(modifiers,
/// other)`: their copy from another's.
const MODIFIERS_DESTRUCTOR: usize = 0x93ae10;
const MODIFIERS_COPY: usize = 0x3296b0;
/// `(set, flags)`: a set of modifiers' deleting destructor, its first slot.
const MODIFIER_SET_DELETE: usize = 0x1b2290;

/// What the repairs let go since the last world reset, for the log:
/// mercenary companies' leaders, rebel factions and the AI's agents.
static LEADERS: AtomicUsize = AtomicUsize::new(0);
static FACTIONS: AtomicUsize = AtomicUsize::new(0);
static AGENTS: AtomicUsize = AtomicUsize::new(0);
static MODIFIER_SETS: AtomicUsize = AtomicUsize::new(0);

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
                m.call(m.at(DELETE), &[bytes, 0])?;
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
            match m.vslot(agent, 0).and_then(|delete| m.call(delete, &[agent, 1])) {
                Ok(_) => {
                    AGENTS.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => log!("repair: an agent of a country's AI was not deleted: {e}"),
            }
        }
    });
}

/// The window of the map modes' panel being let go, killed once it is.
static MAP_MODES_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// The map modes' panel is being let go: its window, which the game hides
/// and takes off the map manager's windows without killing it, would stay in
/// the GUI with the map modes' entries; it is kept to be killed once the
/// panel is gone ([`map_modes_gone`]), as the game kills its other windows.
extern "C" fn map_modes_going(frame: *mut HookFrame) {
    crate::guarded("map modes let go", || {
        let m = crate::hooked_memory();
        match m.ptr(unsafe { (*frame).args[0] } + map_modes::WINDOW) {
            Ok(window) => MAP_MODES_WINDOW.store(window, Ordering::Relaxed),
            Err(e) => log!("repair: the map modes' window was not read: {e}"),
        }
    });
}

/// The map modes' panel is gone: its window is killed, and the GUI lets it
/// go with what it holds at its next sweep.
extern "C" fn map_modes_gone(_: *mut HookFrame) {
    crate::guarded("map modes let go", || {
        let window = MAP_MODES_WINDOW.swap(0, Ordering::Relaxed);
        if window == 0 {
            return;
        }
        let m = crate::hooked_memory();
        if let Err(e) = m.vslot(window, gui_window::KILL).and_then(|kill| m.call(kill, &[window])) {
            log!("repair: the map modes' window was not killed: {e}");
        }
    });
}

/// The outliner is being let go: its categories' entries, which the
/// function letting it go leaves (its headers with their windows among
/// them), are deleted first as its refresh deletes them: what shows them is
/// told first, then each goes by its first slot.
extern "C" fn outliner_going(frame: *mut HookFrame) {
    crate::guarded("outliner let go", || {
        let m = crate::hooked_memory();
        let outliner = unsafe { (*frame).args[0] };
        let deleted = (|| {
            let view = m.ptr(outliner + outliner::VIEW)?;
            if view != 0 {
                m.call(m.vslot(view, outliner::VIEW_EMPTIED)?, &[view])?;
            }
            let mut deleted = 0;
            for category in 0..outliner::CATEGORY_COUNT {
                let entries = outliner + outliner::CATEGORIES + category * outliner::CATEGORY_SIZE + outliner::ENTRIES;
                let mut node = m.ptr(entries + list::HEAD)?;
                while node != 0 {
                    let entry = m.ptr(node + list::ITEM)?;
                    if entry != 0 {
                        m.write(node + list::ITEM, &0usize.to_le_bytes())?;
                        m.call(m.vslot(entry, 0)?, &[entry, 1])?;
                        deleted += 1;
                    }
                    node = m.ptr(node + list::NEXT)?;
                }
            }
            crate::Result::Ok(deleted)
        })();
        match deleted {
            Ok(n) => log!("repair: {n} entries of the outliner were deleted as it went"),
            Err(e) => log!("repair: the outliner's entries were not deleted: {e}"),
        }
    });
}

/// The sets of modifiers to delete once a country's modifiers have gone or
/// been copied over, by those modifiers.
static MODIFIER_SETS_LEFT: Mutex<Vec<(usize, Vec<usize>)>> = Mutex::new(Vec::new());

/// The sets a country's modifiers copied, which nothing else points to.
fn modifier_copies(m: &Memory, modifiers: usize) -> crate::Result<Vec<usize>> {
    let (begin, end) = (m.ptr(modifiers + modifiers::COPIES)?, m.ptr(modifiers + modifiers::COPIES + 8)?);
    let mut sets = Vec::new();
    if end > begin {
        for at in (begin..end).step_by(8) {
            let set = m.ptr(at)?;
            if set != 0 && !sets.contains(&set) {
                sets.push(set);
            }
        }
    }
    Ok(sets)
}

/// Deletes sets of modifiers by their first slots, those of their class.
fn delete_modifier_sets(m: &Memory, sets: Vec<usize>) {
    for set in sets {
        let deleted = (|| {
            if m.ptr(set)? != m.at(classes::MODIFIER_SET) {
                return Err(format!("{set:#x} is not a set of modifiers"));
            }
            m.call(m.vslot(set, 0)?, &[set, 1])
        })();
        match deleted {
            Ok(_) => {
                MODIFIER_SETS.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => log!("repair: a set of modifiers was not deleted: {e}"),
        }
    }
}

fn keep_modifier_sets(modifiers: usize, sets: crate::Result<Vec<usize>>) {
    match sets {
        Ok(sets) if sets.is_empty() => {}
        Ok(sets) => MODIFIER_SETS_LEFT.lock().unwrap_or_else(|e| e.into_inner()).push((modifiers, sets)),
        Err(e) => log!("repair: a country's sets of modifiers were not read: {e}"),
    }
}

fn kept_modifier_sets(modifiers: usize) -> Vec<usize> {
    let mut left = MODIFIER_SETS_LEFT.lock().unwrap_or_else(|e| e.into_inner());
    match left.iter().rposition(|(k, _)| *k == modifiers) {
        Some(at) => left.swap_remove(at).1,
        None => Vec::new(),
    }
}

/// A country's modifiers go, or are made again in place: the sets they
/// copied, which their destructor leaves, are kept to be deleted once they
/// are gone ([`modifiers_gone`]) with what of them points to the sets.
extern "C" fn modifiers_going(frame: *mut HookFrame) {
    crate::guarded("modifiers destructor", || {
        let m = crate::hooked_memory();
        let modifiers = unsafe { (*frame).args[0] };
        keep_modifier_sets(modifiers, modifier_copies(&m, modifiers));
    });
}

extern "C" fn modifiers_gone(frame: *mut HookFrame) {
    crate::guarded("modifiers destructor", || {
        let m = crate::hooked_memory();
        delete_modifier_sets(&m, kept_modifier_sets(unsafe { (*frame).args[0] }));
    });
}

/// A country's modifiers are copied over from other modifiers: the sets
/// they copied that the others do not hold would be lost, and are deleted
/// once the copy is made ([`modifiers_copied`]).
extern "C" fn modifiers_copying(frame: *mut HookFrame) {
    crate::guarded("modifiers copy", || {
        let m = crate::hooked_memory();
        let (modifiers, other) = unsafe { ((*frame).args[0], (*frame).args[1]) };
        let lost = (|| {
            let kept = modifier_copies(&m, other)?;
            Ok(modifier_copies(&m, modifiers)?.into_iter().filter(|s| !kept.contains(s)).collect())
        })();
        keep_modifier_sets(modifiers, lost);
    });
}

/// The copy is made: the lost sets are deleted, and the other modifiers no
/// longer point to the sets they now share, so that the sets go with these
/// alone: the copy moves them.
extern "C" fn modifiers_copied(frame: *mut HookFrame) {
    crate::guarded("modifiers copy", || {
        let m = crate::hooked_memory();
        let (modifiers, other) = unsafe { ((*frame).args[0], (*frame).args[1]) };
        delete_modifier_sets(&m, kept_modifier_sets(modifiers));
        let moved = m.ptr(other + modifiers::COPIES).and_then(|begin| m.write(other + modifiers::COPIES + 8, &begin.to_le_bytes()));
        if let Err(e) = moved {
            log!("repair: the sets of modifiers copied were not moved: {e}");
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

type SpriteTypeUnload = unsafe extern "C" fn(usize);
static ORIGINAL_SPRITE_TYPE_UNLOAD: Function<SpriteTypeUnload> = Function::new();

/// A sprite type is unloaded with its group of graphics, as the main menu,
/// being made, unloads the game's, whatever its shown sprites. A sprite type
/// loaded lazily that shown sprites still show stays loaded: so it does until
/// the last of them hides, which unloads it. Vanilla makes the main menu
/// once, at startup, when no sprite of the game shows; after an in-process
/// return, sprites the session left still count, and a sprite of the type
/// shown later, finding the count above 0, does not load the texture, and is
/// drawn from one let go (docs/research/main-menu.md).
unsafe extern "C" fn sprite_type_unloading(kind: usize) {
    let stays = crate::guarded("sprite type unload", || {
        let m = crate::hooked_memory();
        let kept = (|| -> crate::Result<u32> {
            let shown = if m.u8(kind + sprite_type::LAZY)? != 0 { m.u32(kind + sprite_type::SHOWN)? } else { 0 };
            if shown != 0 {
                m.write(kind + sprite_type::LOADED, &[1])?;
            }
            Ok(shown)
        })();
        match kept {
            Ok(0) => false,
            Ok(shown) => {
                let name = crate::game_string::read(&m, kind + sprite_type::NAME).map(|b| crate::unicode::game_text::to_utf8(&b)).unwrap_or_default();
                log!("repair: the sprite type {name} stays loaded with its group: {shown} shown sprite(s) show it");
                true
            }
            Err(e) => {
                log!("repair: a sprite type was unloaded with its group unread: {e}");
                false
            }
        }
    });
    if !stays {
        unsafe { ORIGINAL_SPRITE_TYPE_UNLOAD.get()(kind) }
    }
}

/// The Empire's religion and heretic religion the game's definitions give,
/// as the religions have them at the first main menu of the process, before
/// any session.
static DEFINED_RELIGIONS: Mutex<Option<(usize, usize)>> = Mutex::new(None);

/// The main menu showed: the first time, the Empire's religions as the
/// definitions give them are kept, for [`as_a_new_start`].
pub fn main_menu_shown(m: &Memory) {
    let mut defined = DEFINED_RELIGIONS.lock().unwrap_or_else(|e| e.into_inner());
    if defined.is_some() {
        return;
    }
    let read = (|| {
        let manager = m.ptr(m.at(globals::RELIGIONS))?;
        if manager == 0 {
            return Err("no religions yet".to_string());
        }
        Ok((m.ptr(manager + religions::ORIGINAL_HRE)?, m.ptr(manager + religions::ORIGINAL_HRE_HERETIC)?))
    })();
    if let Ok((hre, heretic)) = read {
        if hre != 0 && heretic != 0 {
            *defined = Some((hre, heretic));
        }
    }
}

/// Puts back what a world reset leaves of a session as a new start has it:
/// the Empire's religious status and its religions; and, when `achievements`,
/// the campaign's achievements. Answers what it found otherwise.
fn as_a_new_start(m: &Memory, world: usize, with_achievements: bool) -> crate::Result<String> {
    let mut found = Vec::new();
    let status = m.u32(world + world::HRE_RELIGION_STATUS)?;
    if status != 0 {
        m.write(world + world::HRE_RELIGION_STATUS, &0u32.to_le_bytes())?;
        found.push(format!("the Empire's religious status {status}"));
    }
    let defined = *DEFINED_RELIGIONS.lock().unwrap_or_else(|e| e.into_inner());
    let manager = m.ptr(m.at(globals::RELIGIONS))?;
    if let (Some((hre, heretic)), true) = (defined, manager != 0) {
        for (field, religion) in
            [(religions::HRE, hre), (religions::ORIGINAL_HRE, hre), (religions::HRE_HERETIC, heretic), (religions::ORIGINAL_HRE_HERETIC, heretic)]
        {
            if m.ptr(manager + field)? != religion {
                m.write(manager + field, &religion.to_le_bytes())?;
                found.push(format!("the Empire's religion at {field:#x}"));
            }
        }
    }
    let manager = m.ptr(m.at(globals::ACHIEVEMENTS))?;
    if with_achievements && manager != 0 {
        let mut cleared = 0;
        for list in achievements::LISTS {
            let (begin, end) = (m.ptr(manager + list)?, m.ptr(manager + list + 8)?);
            cleared += end.saturating_sub(begin) / 2;
            m.write(manager + list + 8, &begin.to_le_bytes())?;
        }
        if cleared != 0 {
            found.push(format!("{cleared} achievement(s)"));
        }
    }
    Ok(found.join(", "))
}

/// After a world reset that read the game's history: the Empire's history
/// sets its emperors on countries that still have the old session's
/// religions, and the game then takes the Empire's religious status and
/// religions as changed. They are put back as a new start has them.
pub fn after_history(m: &Memory) {
    let fixed = m.ptr(m.at(globals::WORLD)).and_then(|world| as_a_new_start(m, world, false));
    match fixed {
        Ok(found) if found.is_empty() => {}
        Ok(found) => log!("repair: the history left {found}; put back as a new start has them"),
        Err(e) => log!("repair: what the history leaves was not put back: {e}"),
    }
}

/// After every world reset: what it leaves of the session is put back as a
/// new start has it ([`as_a_new_start`]). A load sets it again from its save.
extern "C" fn world_reset(frame: *mut HookFrame) {
    crate::guarded("world reset", || {
        let m = crate::hooked_memory();
        let world = unsafe { (*frame).args[0] };
        match as_a_new_start(&m, world, true) {
            Ok(found) if found.is_empty() => {}
            Ok(found) => log!("repair: the world reset left {found}; put back as a new start has them"),
            Err(e) => log!("repair: what the world reset leaves was not put back: {e}"),
        }
        log!(
            "repair: since the last world reset, {} mercenary companies' leaders, {} rebel factions, {} agents of the AI and {} sets of modifiers were deleted with what owned them",
            LEADERS.swap(0, Ordering::Relaxed),
            FACTIONS.swap(0, Ordering::Relaxed),
            AGENTS.swap(0, Ordering::Relaxed),
            MODIFIER_SETS.swap(0, Ordering::Relaxed)
        );
        #[cfg(feature = "trace")]
        crate::census::world_reset();
    });
}

/// The session's interface the map manager's destructor leaves, as an
/// in-process return takes the session down (ADR 0022).
pub struct SessionInterface {
    manager: usize,
    /// Those of the session's windows open as popups, the end-of-game
    /// summary the player left from say, which the world reset lets go.
    popups: Vec<usize>,
}

/// The map manager an in-process return lets go at the idler switch.
static LEAVING: AtomicUsize = AtomicUsize::new(0);

impl SessionInterface {
    /// Before an in-process return takes the session's interface down. A
    /// message window holds the popup it was made with and never lets it go,
    /// nor does anything else: the popup, its window and the sprites it
    /// shows, which keep counting as shown, would stay for the process. So
    /// the popups of the message windows that are not closed, which the
    /// in-game load marks as closed, go first, unless open, which go with
    /// the other popups.
    pub fn before_reset(m: &Memory, manager: usize) -> crate::Result<Self> {
        let popups = crate::game::questions::items(m, manager + manager_fields::POPUPS).map_err(|e| e.message)?;
        let mut node = m.ptr(manager + manager_fields::MESSAGE_WINDOWS + list::HEAD)?;
        while node != 0 {
            let window = m.ptr(node + list::ITEM)?;
            if window != 0 && m.u8(node + list::REMOVED)? == 0 && m.u8(window + message_window::CLOSING)? == 0 {
                let popup = m.ptr(window + message_window::POPUP)?;
                if popup != 0 && !popups.contains(&popup) {
                    m.write(window + message_window::POPUP, &0usize.to_le_bytes())?;
                    m.call(m.vslot(popup, 0)?, &[popup, 1])?;
                }
            }
            node = m.ptr(node + list::NEXT)?;
        }
        Ok(Self { manager, popups })
    }

    /// After that reset: the interface is let go as the map manager goes
    /// ([`let_session_interface_go`]), but for the popups the reset let go.
    pub fn let_go_with_map_manager(self, m: &Memory) -> crate::Result<()> {
        for field in manager_fields::SESSION_WINDOWS {
            if self.popups.contains(&m.ptr(self.manager + field)?) {
                log!("repair: the reset let the popup at {field:#x} go");
                m.write(self.manager + field, &0usize.to_le_bytes())?;
            }
        }
        LEAVING.store(self.manager, Ordering::Relaxed);
        Ok(())
    }
}

/// The map manager's destructor begins: after an in-process return, what
/// it never lets go of the session's interface goes first
/// ([`let_session_interface_go`]). Not before: until the idler switch has
/// switched the map manager out, it uses that interface.
extern "C" fn map_manager_going(frame: *mut HookFrame) {
    crate::guarded("map manager destructor", || {
        let manager = unsafe { (*frame).args[0] };
        if LEAVING.compare_exchange(manager, 0, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            return;
        }
        let m = crate::hooked_memory();
        match let_session_interface_go(&m, manager) {
            Ok(gone) => log!("repair: {gone} parts of the session's interface the map manager leaves were let go"),
            Err(e) => log!("repair: the session's interface was not let go: {e}"),
        }
    });
}

/// The map manager lets some of the session's interface go as it goes, and
/// the rest stays in the game's GUI, drawn, updated and under the mouse in
/// the main menu, its buttons calling into the map manager that went: what
/// it never lets go is let go first, the last made first, as the game lets
/// its windows go.
fn let_session_interface_go(m: &Memory, manager: usize) -> crate::Result<usize> {
    let mut gone = 0;
    let mut take = |field: usize| -> crate::Result<usize> {
        let object = m.ptr(manager + field)?;
        if object != 0 {
            m.write(manager + field, &0usize.to_le_bytes())?;
            gone += 1;
        }
        Ok(object)
    };
    let icons = take(manager_fields::MESSAGE_ICON_HOLDER)?;
    if icons != 0 {
        m.call(m.at(EMPTY_MESSAGE_ICONS), &[icons])?;
        m.call(m.at(DELETE), &[icons, manager_fields::MESSAGE_ICON_HOLDER_SIZE])?;
    }
    for field in manager_fields::SESSION_WINDOWS.iter().rev() {
        let window = take(*field)?;
        if window != 0 {
            m.call(m.vslot(window, 0)?, &[window, 1])?;
        }
    }
    for (field, let_go) in manager_fields::SESSION_OBJECTS.iter().rev() {
        let object = take(*field)?;
        if object != 0 {
            m.call(m.at(*let_go), &[object])?;
        }
    }
    // Its minimap, which its destructor leaves too.
    let minimap = take(manager_fields::MINIMAP)?;
    if minimap != 0 {
        m.call(m.vslot(minimap, 0)?, &[minimap, 1])?;
    }
    Ok(gone)
}

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(DELETE, "e9 2b 45 02 00", "operator delete"),
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
        Point::method(classes::LEADER, leader::REAL, YES, "a leader is one"),
        Point::method(classes::NULL_LEADER, leader::REAL, NO, "the null leader is none"),
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
        // A country's modifiers' destructor and copy, which leave the sets
        // they copied; a set goes by its first slot.
        Point::observe(
            MODIFIERS_DESTRUCTOR,
            "40 53 48 83 ec 20 48 8b d9 48 81 c1 78 6c 00 00 e8 8b 96 75 ff",
            "modifiers destructor",
            1,
            Some(modifiers_going),
            Some(modifiers_gone),
        ),
        Point::observe(
            MODIFIERS_COPY,
            "48 89 5c 24 10 48 89 6c 24 18 56 57 41 56 48 83 ec 20 8b 02 48 8b f2 89 01",
            "modifiers copy",
            2,
            Some(modifiers_copying),
            Some(modifiers_copied),
        ),
        Point::method(classes::MODIFIER_SET, 0, MODIFIER_SET_DELETE, "modifier set deleting destructor"),
        Point::observe(WORLD_RESET, "48 89 5c 24 10 48 89 74 24 18 55 57 41 56 48 8b ec 48 83 ec 60 48 8b f1", "world's own reset", 1, None, Some(world_reset)),
        Point::observe(
            MAP_MANAGER_DESTRUCTOR,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 ec 40 48 8b d9",
            "map manager destructor",
            1,
            Some(map_manager_going),
            None,
        ),
        // Its deleting destructor, which the idler switch calls.
        Point::keep(0x80b42fusize, "e8 3c 01 00 00 f6 c3 01 74 0d ba c8 5b 00 00", "map manager deleting destructor"),
        Point::keep(EMPTY_MESSAGE_ICONS, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b f1 48 8d 59 40", "empty message icons"),
        // The rebuild empties the message icons, then makes a holder of this size.
        Point::keep(0x81d542usize, "e8 59 27 5e 00 b9 00 70 00 00", "message icon holder size"),
        Point::observe(LET_OUTLINER_GO, "48 89 5c 24 08 57 48 83 ec 20 48 8b d9 48 8b 89 b0 04 00 00", "let the outliner go", 1, Some(outliner_going), None),
        Point::observe(
            LET_MAP_MODES_GO,
            "40 53 48 83 ec 20 48 8b d9 48 8b 49 08 48 83 c1 30",
            "let the map modes' panel go",
            1,
            Some(map_modes_going),
            Some(map_modes_gone),
        )
        .uses(&[map_modes::WINDOW]),
        // A sprite type unloaded with its group of graphics, which the main
        // menu's interface rebuild does for the game's (group 3); a sprite
        // shown loads its lazily loaded type only when none showed it.
        Point::replace(
            SPRITE_TYPE_UNLOAD,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 ec 40 4c 8b f9",
            "sprite type unload",
            sprite_type_unloading as SpriteTypeUnload,
            &ORIGINAL_SPRITE_TYPE_UNLOAD,
        ),
        Point::method(gui_classes::SPRITE_TYPE, sprite_type::UNLOAD, SPRITE_TYPE_UNLOAD, "sprite type unload method"),
        Point::keep(0x14dc2a5usize, "80 79 44 00 74 0d 48 8b 01 48 8b d7 c6 41 44 00 ff 50 48", "graphics group unload")
            .uses(&[sprite_type::LOADED, sprite_type::UNLOAD]),
        Point::keep(0x10d5ca9usize, "ba 03 00 00 00 48 8b c8 e8 8a 65 40 00", "main menu unloads the game's graphics"),
        Point::keep(0x15bd47fusize, "8b 87 88 02 00 00 85 c0 75 29 80 bf 8c 02 00 00 01 75 20 83 bf 58 02 00 00 ff", "sprite shown loads its type").uses(&[
            sprite_type::SHOWN,
            sprite_type::LAZY,
            sprite_type::TEXTURE,
        ]),
    ]
}
