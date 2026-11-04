//! What a world reset leaves of a session, which vanilla never meets
//! outside a load, whose save sets it again: put back as a new start has it.
use crate::{
    memory::Memory,
    objects::campaign::{achievements, globals, religions, world},
};
use std::sync::Mutex;

/// The Empire's religion and heretic religion the game's definitions give,
/// as the religions have them at the first main menu of the process, before
/// any session.
static DEFINED_RELIGIONS: Mutex<Option<(usize, usize)>> = Mutex::new(None);

/// The main menu showed: the first time, the Empire's religions as the
/// definitions give them are kept, for [`as_a_new_start`].
pub(super) fn main_menu_shown(m: &Memory) {
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
pub(super) fn as_a_new_start(m: &Memory, world: usize, with_achievements: bool) -> crate::Result<String> {
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
