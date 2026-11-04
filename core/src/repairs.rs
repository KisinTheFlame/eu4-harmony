//! Repairs (GLOSSARY): the game's own defects the framework puts right, each
//! where the game goes wrong, so that they hold in vanilla's paths too
//! (docs/research/main-menu.md).
//!
//! - What a world reset leaks: a destructor or a clear that lets an object
//!   go without what it owns. A country's array of bytes, a mercenary
//!   company's leader, a country's rebel factions and the agents its AI
//!   started are let go with them.
//! - What a country's modifiers leak as they go or are made again, in play
//!   too: the sets of modifiers they copy, which they alone own. Those they
//!   are copied over from their own set still lists, and go with them.
//! - What a world reset leaves of a session, which vanilla never meets
//!   outside a load, whose save sets it again: put back as a new start has
//!   it.
//! - What of the session's interface the game never lets go, which vanilla
//!   never meets, as it restarts instead: the popups its message windows
//!   hold, let go before an in-process return takes the interface down, and
//!   what the map manager's destructor leaves, let go as it goes.
//! - What of its interface the game leaves as it lets a part of it go, at
//!   every interface rebuild in vanilla too: the map modes' window and the
//!   bar of the units selected, which it never kills, and the outliner's
//!   entries, which it never deletes.
//! - What a text box leaves as it goes, wherever the game lets one go:
//!   its animation, which its element deletes only while it is on the
//!   element's animations, and the text box takes off them while it does
//!   not play. It is put back on them, owned, as the text box goes.
//! - What the game unloads while it is shown, which vanilla never meets, as
//!   it makes the main menu only at startup: a sprite type shown sprites
//!   still count, which the main menu, made again, would unload with the
//!   game's graphics, stays loaded.
//! - What the game's heap keeps of the sessions it let go: free pages, which
//!   the blocks a session leaves among them keep committed. The first frame
//!   after a world reset, of the main menu or of the session it began, has
//!   the heaps give them back.
//! - What the textures' transparency masks keep, in vanilla too: a texture
//!   ID's mask keeps the buffer of the largest texture the ID was ever
//!   lent to, and IDs pass from texture to texture as they load again. The
//!   buffer is let go before a texture smaller than it is written to it.
//! - What the game's AI changes on several worker threads at once without a
//!   lock, in vanilla too: a country's coalition, which the AI of the
//!   countries in it leaves in parallel. Leaving and joining a coalition
//!   take turns.
//! - What the game's pick of a name from a list weighed for both sexes
//!   overflows, in vanilla too: the random number, which it multiplies by
//!   1000 before taking it modulo the list's weight, so that about half the
//!   draws come out negative and pick the list's first name. The number is
//!   taken modulo the weight as it is.
//! - What the save checksum (GLOSSARY) of a save loaded in a session reads,
//!   in vanilla too: the in-game load reads the save from disk, and a file's
//!   stream sums its bytes otherwise than the memory stream the save was
//!   written from, so such a save never matched and turned achievements off.
//!   The file is summed as the save was.
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::{
        campaign::{
            achievements, ai_king, classes, countries as country_list, country, globals, leader, map_manager as manager_fields, map_modes, mercenary_company,
            message_window, modifiers, outliner, religions, unit_view, world,
        },
        gui::{classes as gui_classes, element, sprite_type, text_box, transparency_mask, window as gui_window},
        list,
    },
    points::{Function, Point},
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

/// `(block, size)`: the game's `operator delete`.
const DELETE: usize = 0x17394f0;
/// In the save checksum (`0x14e3390(out, stream, cut, salt, salt length)`),
/// the stream's sum of its bytes: `call r12`, the stream in `rbx`, how many
/// bytes in `edx` (-1 for all of them).
const SAVE_CHECKSUM_SUM: usize = 0x14e33f9;
const SAVE_CHECKSUM_SUM_DONE: usize = 0x14e3403;
/// A file's stream (vtable [`FILE_STREAM`]): its sum of its bytes, its
/// method at `+0x98`, and where it keeps the file.
const FILE_STREAM: usize = 0x1dbfa98;
const FILE_STREAM_SUM: usize = 0x1710c00;
const FILE_STREAM_SUM_METHOD: usize = 0x98;
const FILE_STREAM_FILE: usize = 0x38;
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

/// `(unit view, flags)`: the unit view's deleting destructor, its vtable's
/// first slot, which the interface rebuild calls before it makes it again,
/// and the framework as the map manager goes. `(map manager, window)`:
/// takes a window off the map manager's windows.
const UNIT_VIEW_DELETE: usize = 0x14719b0;
const TAKE_WINDOW_OFF: usize = 0x6ecc80;
/// Where the unit view, making its selection's bar again, lets the old one
/// go: off the map manager's windows, then killed.
const SELECTION_BAR_MADE_AGAIN: usize = 0x1471540;

/// `(text box)`: a text box's destructor, which its deleting destructor
/// (`0x152aab0`) and its subclass's (`0x15f2d20`) call first; it ends in the
/// element's (`0x1552680`), which deletes the animations the element owns
/// by their first slots. `(element, animation)`: puts an animation on an
/// element's animations, owned by it, or marks it owned there.
const TEXT_BOX_DESTRUCTOR: usize = 0x152aaf0;
const OWN_ANIMATION: usize = 0x15529e0;

/// `(bits, count)`: a `std::vector<bool>`'s resize; and where the texture
/// loader resizes a transparency mask to its texture's pixels, the return
/// address of that call. `(words)`: the destructor of a vector of u32, which
/// frees its buffer and leaves it empty.
const BIT_VECTOR_RESIZE: usize = 0x6e05c0;
const MASK_RESIZED: usize = 0x16c302d;
const WORDS_DESTRUCTOR: usize = 0x943e0;

/// `(country, …, other)`: a country's AI weighs leaving the coalition
/// against `other` (a tag) and the coalitions its members are in, and takes
/// countries off them; on worker threads. `(country)`: a country joins the
/// coalition against the country it targets, on the main thread.
const COALITION_LEAVE: usize = 0x3b0220;
const COALITION_JOIN: usize = 0x3e3940;

/// Where the pick of a name from a list (`0xda9880`), weighing its male and
/// female names each by a factor, turns its random number (`esi`, 0 to
/// 2^31 - 1) into fixed point, `imul eax, esi, 1000`, before it takes it
/// modulo the list's weight in fixed point. A new heir is named by it from
/// the country's ruler names (`0xa55cf0`), unless its sex is already set;
/// the female names weigh by the country's female heir chance.
const NAME_DRAW: usize = 0xda9981;

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
/// The bytes of transparency masks let go since the last world reset.
static MASK_BYTES: AtomicUsize = AtomicUsize::new(0);
static MODIFIER_SETS: AtomicUsize = AtomicUsize::new(0);
static MODIFIER_SETS_WAITING_COUNT: AtomicUsize = AtomicUsize::new(0);
static MODIFIER_SETS_WAITED: AtomicUsize = AtomicUsize::new(0);
static TEXT_BOX_ANIMATIONS: AtomicUsize = AtomicUsize::new(0);

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

/// The unit view is being deleted: the bar of the units selected, which its
/// destructor leaves, would stay in the GUI, shown in the next session when
/// units were selected; it goes first, as the unit view lets it go when it
/// makes it again: off the map manager's windows, then killed.
extern "C" fn unit_view_going(frame: *mut HookFrame) {
    crate::guarded("unit view delete", || {
        let m = crate::hooked_memory();
        let view = unsafe { (*frame).args[0] };
        let killed = (|| {
            let bar = m.ptr(view + unit_view::SELECTION_BAR)?;
            if bar == 0 {
                return Ok(());
            }
            m.write(view + unit_view::SELECTION_BAR, &0usize.to_le_bytes())?;
            m.call(m.at(TAKE_WINDOW_OFF), &[m.ptr(view + unit_view::MAP_MANAGER)?, bar])?;
            m.call(m.vslot(bar, gui_window::KILL)?, &[bar])?;
            crate::Result::Ok(())
        })();
        if let Err(e) = killed {
            log!("repair: the unit view's selection bar was not killed: {e}");
        }
    });
}

/// A text box's destructor begins: its animation, which its element deletes
/// only from its animations, is put on them owned, as the text box puts it
/// there when it plays; the element's destructor then deletes it with the
/// others, the text box still whole, as it deletes a playing one.
extern "C" fn text_box_going(frame: *mut HookFrame) {
    crate::guarded("text box destructor", || {
        let m = crate::hooked_memory();
        let boxed = unsafe { (*frame).args[0] };
        let owned = (|| {
            let animation = m.ptr(boxed + text_box::ANIMATION)?;
            if animation != 0 {
                m.call(m.at(OWN_ANIMATION), &[boxed, animation])?;
                TEXT_BOX_ANIMATIONS.fetch_add(1, Ordering::Relaxed);
            }
            crate::Result::Ok(())
        })();
        if let Err(e) = owned {
            log!("repair: a text box's animation was not handed to its element: {e}");
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

/// The sets of modifiers to delete once a country's modifiers have gone, or
/// to wait for them once they are copied over, by those modifiers.
static MODIFIER_SETS_LEFT: Mutex<Vec<(usize, Vec<usize>)>> = Mutex::new(Vec::new());

/// The sets of modifiers a country's modifiers had before they were copied
/// over, which their own set still lists: deleted when they go, by them.
static MODIFIER_SETS_WAITING: Mutex<Vec<(usize, Vec<usize>)>> = Mutex::new(Vec::new());

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

fn take_modifier_sets(list: &Mutex<Vec<(usize, Vec<usize>)>>, modifiers: usize) -> Vec<usize> {
    let mut left = list.lock().unwrap_or_else(|e| e.into_inner());
    match left.iter().rposition(|(k, _)| *k == modifiers) {
        Some(at) => left.swap_remove(at).1,
        None => Vec::new(),
    }
}

fn kept_modifier_sets(modifiers: usize) -> Vec<usize> {
    take_modifier_sets(&MODIFIER_SETS_LEFT, modifiers)
}

/// A country's modifiers go, or are made again in place: the sets they
/// copied, which their destructor leaves, and those they had before they
/// were copied over, which their own set listed until now, are kept to be
/// deleted once they are gone ([`modifiers_gone`]) with what of them points
/// to the sets.
extern "C" fn modifiers_going(frame: *mut HookFrame) {
    crate::guarded("modifiers destructor", || {
        let m = crate::hooked_memory();
        let modifiers = unsafe { (*frame).args[0] };
        let waiting = take_modifier_sets(&MODIFIER_SETS_WAITING, modifiers);
        MODIFIER_SETS_WAITING_COUNT.fetch_sub(waiting.len(), Ordering::Relaxed);
        MODIFIER_SETS_WAITED.fetch_add(waiting.len(), Ordering::Relaxed);
        let sets = modifier_copies(&m, modifiers).map(|mut sets| {
            for set in waiting {
                if !sets.contains(&set) {
                    sets.push(set);
                }
            }
            sets
        });
        keep_modifier_sets(modifiers, sets);
    });
}

extern "C" fn modifiers_gone(frame: *mut HookFrame) {
    crate::guarded("modifiers destructor", || {
        let m = crate::hooked_memory();
        delete_modifier_sets(&m, kept_modifier_sets(unsafe { (*frame).args[0] }));
    });
}

/// A country's modifiers are copied over from other modifiers: the sets
/// they copied that the others do not hold would be lost, and wait for them
/// to go once the copy is made ([`modifiers_copied`]).
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

/// The copy is made: their own set still lists the lost sets, as a set's
/// copy leaves the list it has, so the lost sets wait for the modifiers to
/// go; and the other modifiers no longer point to the sets they now share,
/// so that the sets go with these alone: the copy moves them.
extern "C" fn modifiers_copied(frame: *mut HookFrame) {
    crate::guarded("modifiers copy", || {
        let m = crate::hooked_memory();
        let (modifiers, other) = unsafe { ((*frame).args[0], (*frame).args[1]) };
        let lost = kept_modifier_sets(modifiers);
        if !lost.is_empty() {
            #[cfg(feature = "dev-heap-census")]
            check_listed(&m, modifiers, &lost);
            MODIFIER_SETS_WAITING_COUNT.fetch_add(lost.len(), Ordering::Relaxed);
            let mut waiting = MODIFIER_SETS_WAITING.lock().unwrap_or_else(|e| e.into_inner());
            match waiting.iter_mut().find(|(k, _)| *k == modifiers) {
                Some((_, sets)) => sets.extend(lost),
                None => waiting.push((modifiers, lost)),
            }
        }
        let moved = m.ptr(other + modifiers::COPIES).and_then(|begin| m.write(other + modifiers::COPIES + 8, &begin.to_le_bytes()));
        if let Err(e) = moved {
            log!("repair: the sets of modifiers copied were not moved: {e}");
        }
    });
}

/// Logs the lost sets the modifiers' own set does not list: a check, in
/// builds with the heap census, of what the repair takes the game to do.
#[cfg(feature = "dev-heap-census")]
fn check_listed(m: &Memory, modifiers: usize, lost: &[usize]) {
    let listed = (|| {
        let added = modifiers + modifiers::OWN + crate::objects::campaign::modifier_set::ADDED;
        let (begin, end) = (m.ptr(added)?, m.ptr(added + 8)?);
        let mut sets = Vec::new();
        if end > begin {
            for at in (begin..end).step_by(8) {
                sets.push(m.ptr(at)?);
            }
        }
        crate::Result::Ok(sets)
    })();
    match listed {
        Ok(listed) => {
            let unlisted = lost.iter().filter(|s| !listed.contains(s)).count();
            if unlisted > 0 {
                log!("repair: {unlisted} of {} sets of modifiers copied over are not listed by the modifiers' own set", lost.len());
            }
        }
        Err(e) => log!("repair: the modifiers' own set's list was not read: {e}"),
    }
}

/// A vector of bits is resized: when it is a transparency mask the texture
/// loader is about to write a texture to, and its buffer holds more than the
/// texture needs, the buffer is let go first, so that the resize makes one
/// as large as the texture. The loader writes every bit the texture has, so
/// nothing of the old mask is needed.
extern "C" fn mask_resizing(frame: *mut HookFrame) {
    crate::guarded("transparency mask resize", || {
        let m = crate::hooked_memory();
        let h = unsafe { &*frame };
        if h.return_address != m.at(MASK_RESIZED) {
            return;
        }
        let (mask, bits) = (h.args[0], h.args[1]);
        let let_go = (|| {
            let (begin, capacity) = (m.ptr(mask + transparency_mask::WORDS)?, m.ptr(mask + transparency_mask::CAPACITY)?);
            let needed = bits.div_ceil(32) * 4;
            if begin == 0 || capacity <= begin + needed {
                return Ok(0);
            }
            m.call(m.at(WORDS_DESTRUCTOR), &[mask + transparency_mask::WORDS])?;
            m.write(mask + transparency_mask::BITS, &0usize.to_le_bytes())?;
            crate::Result::Ok(capacity - begin)
        })();
        match let_go {
            Ok(bytes) => {
                MASK_BYTES.fetch_add(bytes, Ordering::Relaxed);
            }
            Err(e) => log!("repair: a transparency mask was not let go: {e}"),
        }
    });
}

/// The turns leaving and joining a coalition take: a lock, and the thread
/// that holds it and how many times, so that it may take its turn again.
struct Turns {
    lock: std::cell::UnsafeCell<windows_sys::Win32::System::Threading::SRWLOCK>,
    holder: std::sync::atomic::AtomicU32,
    depth: std::cell::Cell<u32>,
}
// The lock is the SRW lock's; `depth` is touched only by its holder.
unsafe impl Sync for Turns {}

static COALITION_TURNS: Turns = Turns {
    lock: std::cell::UnsafeCell::new(windows_sys::Win32::System::Threading::SRWLOCK_INIT),
    holder: std::sync::atomic::AtomicU32::new(0),
    depth: std::cell::Cell::new(0),
};

/// A country's coalition is about to change: wait for the turn. Several
/// countries' AI take countries off the same coalition at once, on worker
/// threads, and its list is not made for it.
extern "C" fn coalition_changing(_frame: *mut HookFrame) {
    let turns = &COALITION_TURNS;
    let me = crate::thread_id();
    if turns.holder.load(Ordering::Acquire) != me {
        unsafe { windows_sys::Win32::System::Threading::AcquireSRWLockExclusive(turns.lock.get()) };
        turns.holder.store(me, Ordering::Release);
    }
    turns.depth.set(turns.depth.get() + 1);
}

/// The change is made, or the function faulted: the turn is over.
extern "C" fn coalition_changed(_frame: *mut HookFrame) {
    let turns = &COALITION_TURNS;
    if turns.holder.load(Ordering::Acquire) != crate::thread_id() {
        return;
    }
    let depth = turns.depth.get() - 1;
    turns.depth.set(depth);
    if depth == 0 {
        turns.holder.store(0, Ordering::Release);
        unsafe { windows_sys::Win32::System::Threading::ReleaseSRWLockExclusive(turns.lock.get()) };
    }
}

/// The game's file stream's sum, for the stub to tell a file's stream by.
static FILE_STREAM_SUM_AT: AtomicUsize = AtomicUsize::new(0);

/// The sum of bytes the save checksum is taken of, as a memory stream sums
/// them (`0x171f3f0`): a 16-bit sum and a 16-bit sum of sums, each shifted
/// right by 16 when it passes 0xffff. A file's stream (`0x170f9b0`) takes
/// 0xffff off instead.
#[derive(Default)]
struct SaveSum {
    a: u32,
    b: u32,
}

impl SaveSum {
    fn add(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.a += u32::from(byte);
            if self.a > 0xffff {
                self.a >>= 16;
            }
            self.b += self.a;
            if self.b > 0xffff {
                self.b >>= 16;
            }
        }
    }

    fn value(&self) -> u32 {
        self.a << 16 | self.b
    }
}

/// A file's stream's sum of `count` of its bytes (-1 for all), as the memory
/// stream the save was written from summed them; the game's own sum when
/// the file cannot be read.
unsafe extern "C" fn file_stream_sum(stream: usize, count: i32) -> u32 {
    crate::guarded("save checksum of a file", || {
        let m = crate::hooked_memory();
        let mut sum = SaveSum::default();
        let read = m
            .ptr(stream + FILE_STREAM_FILE)
            .and_then(|holder| m.ptr(holder))
            .and_then(|file| crate::physfs::read_from_start(file, u64::try_from(count).ok(), |bytes| sum.add(bytes)));
        match read {
            Ok(()) => sum.value(),
            Err(e) => {
                log!("repairs: the save checksum of a file is summed by the game, which never matches the save's: {e}");
                m.call(FILE_STREAM_SUM_AT.load(Ordering::Relaxed), &[stream, count as u32 as usize]).map_or(0, |v| v as u32)
            }
        }
    })
}

crate::resume!(SAVE_CHECKSUM_SUM_RESUME: SAVE_CHECKSUM_SUM_NEXT = SAVE_CHECKSUM_SUM_DONE);

/// Replaces the save checksum's sum of the stream's bytes: a file's stream
/// is summed by [`file_stream_sum`], any other by its own method.
#[unsafe(naked)]
extern "C" fn save_checksum_sum_stub() {
    std::arch::naked_asm!(
        "mov rcx, rbx",
        "cmp r12, qword ptr [rip + {file_sum}]",
        "jne 2f",
        crate::call_rust!(call sum, "mov eax, dword ptr [rsp + 0x48]"),
        "jmp 3f",
        "2:",
        "call r12",
        "3:",
        "mov byte ptr [rbp - 0x38], 0",
        "jmp qword ptr [rip + {next}]",
        file_sum = sym FILE_STREAM_SUM_AT,
        sum = sym file_stream_sum,
        next = sym SAVE_CHECKSUM_SUM_NEXT,
    );
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

/// A world reset was made, and no frame has run since: the heaps have free
/// pages to give back.
static PAGES_TO_GIVE_BACK: AtomicBool = AtomicBool::new(false);

/// The first frame after a world reset: the heaps give back the free pages
/// the sessions they held leave, which they would keep committed. A heap
/// keeps a page while a block on it lives, and what a session leaves is
/// strewn among what it let go.
fn give_back_pages() {
    if !PAGES_TO_GIVE_BACK.swap(false, Ordering::Relaxed) {
        return;
    }
    #[repr(C)]
    struct Optimize {
        version: u32,
        flags: u32,
    }
    let started = std::time::Instant::now();
    let before = crate::heap::private_bytes();
    let optimize = Optimize { version: 1, flags: 0 };
    let ok = unsafe {
        windows_sys::Win32::System::Memory::HeapSetInformation(
            std::ptr::null_mut(),
            windows_sys::Win32::System::Memory::HeapOptimizeResources,
            &optimize as *const Optimize as *const _,
            std::mem::size_of::<Optimize>(),
        )
    };
    if ok == 0 {
        log!("repair: the heaps did not give back their free pages: {}", std::io::Error::last_os_error());
        return;
    }
    let after = crate::heap::private_bytes();
    log!("repair: the heaps gave back {:.0} MB of free pages in {} ms", before.saturating_sub(after) as f64 / (1024.0 * 1024.0), started.elapsed().as_millis());
}

/// A session's frame ran: the first after a world reset gives back free
/// pages ([`give_back_pages`]).
pub fn session_frame() {
    give_back_pages();
}

/// The main menu showed: the first time, the Empire's religions as the
/// definitions give them are kept, for [`as_a_new_start`]; the first frame
/// after a world reset gives back free pages ([`give_back_pages`]).
pub fn main_menu_shown(m: &Memory) {
    give_back_pages();
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
        PAGES_TO_GIVE_BACK.store(true, Ordering::Relaxed);
        let m = crate::hooked_memory();
        let world = unsafe { (*frame).args[0] };
        match as_a_new_start(&m, world, true) {
            Ok(found) if found.is_empty() => {}
            Ok(found) => log!("repair: the world reset left {found}; put back as a new start has them"),
            Err(e) => log!("repair: what the world reset leaves was not put back: {e}"),
        }
        log!(
            "repair: since the last world reset, {} mercenary companies' leaders, {} rebel factions, {} agents of the AI and {} sets of modifiers ({} of them once the modifiers that listed them went) and {} text boxes' animations were deleted with what owned them; {} sets of modifiers wait for the modifiers that list them; {:.1} MB of transparency masks were let go",
            LEADERS.swap(0, Ordering::Relaxed),
            FACTIONS.swap(0, Ordering::Relaxed),
            AGENTS.swap(0, Ordering::Relaxed),
            MODIFIER_SETS.swap(0, Ordering::Relaxed),
            MODIFIER_SETS_WAITED.swap(0, Ordering::Relaxed),
            TEXT_BOX_ANIMATIONS.swap(0, Ordering::Relaxed),
            MODIFIER_SETS_WAITING_COUNT.load(Ordering::Relaxed),
            MASK_BYTES.swap(0, Ordering::Relaxed) as f64 / (1024.0 * 1024.0)
        );
        #[cfg(feature = "dev-heap-census")]
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
        // The save checksum calls the stream's sum, its method at +0x98; a
        // file's stream sums its file, which it keeps at +0x38, from a holder.
        Point::keep(0x14e33d5usize, "4c 8b a0 98 00 00 00", "save checksum stream sum method").uses(&[FILE_STREAM_SUM_METHOD]),
        Point::jump(
            SAVE_CHECKSUM_SUM,
            "48 8b cb 41 ff d4 c6 45 c8 00",
            "save checksum stream sum",
            save_checksum_sum_stub as *const () as usize,
            &SAVE_CHECKSUM_SUM_RESUME,
        ),
        Point::keep(SAVE_CHECKSUM_SUM_DONE, "45 33 e4 4c 89 64 24 70", "save checksum after the sum"),
        Point::method(FILE_STREAM, FILE_STREAM_SUM_METHOD, FILE_STREAM_SUM, "file stream sum method"),
        Point::keep(FILE_STREAM_SUM, "48 8b 49 38 e9 a7 ed ff ff", "file stream sum").uses(&[FILE_STREAM_FILE]).recorded(&FILE_STREAM_SUM_AT),
        Point::keep(0x170f9e0usize, "48 8b 09 e8 d8 74 2e 00", "file stream sum's file"),
        // The texture loader resizes the transparency mask of the texture's
        // ID to its pixels, then writes every bit.
        Point::observe(BIT_VECTOR_RESIZE, "48 89 5c 24 10 57 48 83 ec 50 4c 8b 41 18 48 8b da 48 8b f9", "vector of bits resize", 2, Some(mask_resizing), None)
            .uses(&[transparency_mask::BITS]),
        Point::keep(0x16c3025usize, "49 8b cf e8 93 d5 01 ff 4d 8b 6e 18", "transparency mask resized for a texture"),
        Point::keep(0x16c3079usize, "49 8b 0f 8b c2 48 c1 e8 05 4c 8d 14 81", "transparency mask's bits written"),
        Point::keep(WORDS_DESTRUCTOR, "40 53 48 83 ec 20 48 8b d9 48 8b 09 48 85 c9 74 3e 48 8b 53 10 48 2b d1", "vector of u32 destructor")
            .uses(&[transparency_mask::CAPACITY]),
        // Leaving and joining a coalition change its list; the AI leaves on
        // worker threads.
        Point::observe(
            COALITION_LEAVE,
            "4c 89 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55 53 56 57 41 54 41 55 41 56 41 57",
            "AI leaving a coalition",
            4,
            Some(coalition_changing),
            Some(coalition_changed),
        ),
        Point::keep(0x3b0c33usize, "48 8b 8b 28 19 00 00", "coalition taken off by the AI").uses(&[country::COALITION]),
        Point::observe(
            COALITION_JOIN,
            "48 89 5c 24 20 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 20 48 8b e9",
            "joining a coalition",
            1,
            Some(coalition_changing),
            Some(coalition_changed),
        ),
        Point::keep(0x3e3c31usize, "49 8b 81 28 19 00 00", "coalition joined").uses(&[country::COALITION]),
        // The random number goes into the division as it is (`mov eax,
        // esi`); the remainder of it by the weight is where the walk down the
        // names begins, each name's weight taken off until it goes below 0.
        Point::write(NAME_DRAW, "69 c6 e8 03 00 00", "name pick's draw in fixed point", "8b c6 0f 1f 40 00"),
        Point::keep(0xda9995usize, "41 03 c9 99 4d 8b 4a 18 f7 f9 8b ca", "name pick's draw modulo the weight"),
        Point::keep(0xda99dcusize, "2b ca 78 06", "name pick's walk down the names"),
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
        Point::observe(
            UNIT_VIEW_DELETE,
            "48 89 5c 24 08 57 48 83 ec 20 48 8d 05 7f 5b 91 00 8b fa 48 89 01 48 8b d9",
            "unit view deleting destructor",
            2,
            Some(unit_view_going),
            None,
        ),
        Point::method(classes::UNIT_VIEW, 0, UNIT_VIEW_DELETE, "unit view deleting destructor method"),
        // The rebuild deletes the unit view by its first slot.
        Point::keep(0x81cdbcusize, "49 8b 8e a0 5b 00 00 48 85 c9 74 0a 48 8b 01 ba 01 00 00 00 ff 10", "unit view deleted by the rebuild"),
        Point::keep(TAKE_WINDOW_OFF, "40 53 48 83 ec 20 48 8b d9 48 8b 89 90 00 00 00 48 85 c9", "take a window off the map manager's windows"),
        Point::keep(
            SELECTION_BAR_MADE_AGAIN,
            "48 8b 91 a0 01 00 00 48 85 d2 74 18 48 8b cf e8 2c b7 27 ff 48 8b 8b a0 01 00 00 48 8b 01 ff 90 30 01 00 00",
            "selection bar let go",
        )
        .uses(&[gui_window::KILL]),
        // A text box's destructor, which leaves its animation, and ends in
        // the element's; that deletes the animations the element owns, and
        // the text box makes its own with it.
        Point::observe(TEXT_BOX_DESTRUCTOR, "40 53 48 83 ec 20 48 8b d9 48 8d 05 10 52 86 00 48 89 01", "text box destructor", 1, Some(text_box_going), None)
            .uses(&[gui_classes::TEXT_BOX]),
        Point::keep(0x152ab58usize, "48 8b cb 48 83 c4 20 5b e9 1b 7b 02 00", "text box destructor ends in the element's"),
        Point::keep(OWN_ANIMATION, "48 85 d2 0f 84 a9 00 00 00 53 48 83 ec 30 4c 8b 81 90 00 00 00", "own an animation").uses(&[element::ANIMATIONS]),
        Point::keep(0x1552a22usize, "49 8b 92 98 00 00 00 48 89 5c 24 20 c6 44 24 28 01", "animation put on owned"),
        Point::keep(0x155269ausize, "48 8d b9 90 00 00 00", "element destructor's animations").uses(&[element::ANIMATIONS]),
        Point::keep(
            0x15526c0usize,
            "48 8b 17 80 7c 13 08 00 74 16 48 8b 0c 13 48 85 c9 74 0d 48 8b 01 ba 01 00 00 00 ff 10",
            "element deletes its owned animations",
        )
        .uses(&[element::ANIMATION_OWNED]),
        Point::keep(0x152a036usize, "48 89 9e 10 02 00 00", "text box's animation made").uses(&[text_box::ANIMATION]),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_is_summed_as_a_memory_stream_sums_it_in_any_pieces() {
        let bytes = [&[0xff; 300][..], b"checksum="].concat();
        let mut whole = SaveSum::default();
        whole.add(&bytes);
        // Past 0xffff a memory stream shifts right by 16, where a file's
        // stream would give 0x2e6541f9.
        assert_eq!(whole.value(), 0x2d67b397);
        let mut pieces = SaveSum::default();
        for piece in bytes.chunks(7) {
            pieces.add(piece);
        }
        assert_eq!(pieces.value(), whole.value());
    }
}
