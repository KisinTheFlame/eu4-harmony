//! Province coloring on the map. The core switches to a native map mode whose
//! color pass it can observe (mode 12, option 26) and overwrites the colors of
//! the requested provinces; clearing restores the mode that was active before.
//! Ported from eu4-regional-combat's range preview (docs/research/map-modes.md).
use crate::{
    game::{self, fail, map_coloring::MapColor, GResult, Status},
    hooks::Call,
    memory::Memory,
    objects::campaign::{classes, globals, map, map_manager, province},
    points::{GameFunction, Point},
    Result,
};
use std::{cell::RefCell, collections::BTreeMap, sync::Mutex};

const FILL: usize = 0x12fd9b0;
const SECONDARY: usize = 0x12fdcb0;
const RENDERER: usize = 0x10a8c30;
const REFRESH: GameFunction<4> = GameFunction::at(0x821ff0);
const PAINT_MODE: i32 = 12;
const PAINT_OPTION: u32 = 26;

/// Proof collected by the hooks during one refresh.
#[derive(Clone, Default)]
struct Ticket {
    mode: i32,
    validated: bool,
    rendered: bool,
    applied: bool,
    error: Option<String>,
}

/// Who colors the map and how, apart from the game: the extension whose
/// coloring shows, the provinces' colors, and the extensions whose
/// coloring another replaced, to be told.
#[derive(Default)]
struct Coloring {
    owner: Option<usize>,
    /// province -> bytes in memory order (B, G, R, A)
    colors: BTreeMap<u32, [u8; 4]>,
    lost: Vec<usize>,
}

impl Coloring {
    /// `extension`'s coloring replaces whichever showed; the extension it
    /// replaces, another, is to be told.
    fn paint(&mut self, extension: usize, colors: &[MapColor]) {
        if let Some(previous) = self.owner.filter(|&p| p != extension) {
            self.lost.push(previous);
        }
        self.owner = Some(extension);
        self.colors = colors.iter().map(|c| (c.province, [c.rgba[2], c.rgba[1], c.rgba[0], c.rgba[3]])).collect();
    }

    /// No coloring shows any more.
    fn clear(&mut self) {
        self.colors.clear();
        self.owner = None;
    }

    /// Whether `extension`'s coloring shows.
    fn shows(&self, extension: usize) -> bool {
        self.owner == Some(extension)
    }

    /// The extensions whose coloring another replaced since the last time.
    fn take_lost(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.lost)
    }
}

#[derive(Default)]
struct State {
    coloring: Coloring,
    /// (manager, mode) to restore
    saved: Option<(usize, i32)>,
    ticket: Option<Ticket>,
}

#[derive(Clone, Copy)]
struct Fill {
    data: usize,
    capacity: usize,
    mode: i32,
    option: u32,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
thread_local! { static FILLS: RefCell<Vec<Option<Fill>>> = const { RefCell::new(Vec::new()) }; }

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

fn fill_before(call: &mut Call) {
    let m = &call.memory();
    let a = [call.arg(0), call.arg(1), call.arg(2)];
    let fill = (|| -> Result<Fill> {
        let d0 = m.i32(a[0] + 8)?;
        let d1 = m.i32(a[0] + 12)?;
        let capacity = d0 as i64 * d1 as i64;
        if d0 <= 0 || d1 <= 0 || capacity > 1_000_000 {
            return Err("color buffer size".into());
        }
        Ok(Fill { data: m.ptr(a[0])?, capacity: capacity as usize, mode: a[1] as i32, option: a[2] as u32 })
    })();
    FILLS.with(|s| s.borrow_mut().push(fill.ok()));
}

fn fill_after(call: &mut Call) {
    FILLS.with(|s| s.borrow_mut().pop());
    if call.faulted() {
        with_state(|s| {
            if let Some(t) = s.ticket.as_mut() {
                t.error = Some("color fill unwound".into());
            }
        });
    }
}

/// After the game's secondary color pass for the paint mode, overwrite our provinces.
fn secondary_after(call: &mut Call) {
    let m = &call.memory();
    let Ok(mut guard) = STATE.try_lock() else { return };
    let Some(s) = guard.as_mut() else { return };
    if s.ticket.is_none() && s.coloring.colors.is_empty() {
        return;
    }
    let result = (|| -> Result<()> {
        if call.faulted() {
            return Err("secondary color pass unwound".into());
        }
        let fill = FILLS.with(|f| f.borrow().last().copied().flatten()).ok_or("no enclosing color fill")?;
        if fill.option != call.arg(1) as u32 || fill.data.checked_add(fill.capacity * 4) != Some(call.arg(0)) {
            return Err("secondary buffer does not follow the fill buffer".into());
        }
        let map = m.ptr(m.at(globals::MAP))?;
        let first = m.ptr(map + map::PROVINCES)?;
        let end = m.ptr(map + map::PROVINCES + 8)?;
        let bytes = end.checked_sub(first).ok_or("reversed province vector")?;
        if first == 0 || bytes % province::SIZE != 0 || bytes / province::SIZE > fill.capacity {
            return Err("province vector exceeds color capacity".into());
        }
        if let Some(t) = s.ticket.as_mut() {
            if t.mode == fill.mode {
                t.validated = true;
            }
        }
        if fill.mode != PAINT_MODE || fill.option != PAINT_OPTION || s.coloring.colors.is_empty() {
            return Ok(());
        }
        let count = bytes / province::SIZE;
        for (&id, color) in &s.coloring.colors {
            if id as usize >= count || m.i32(first + id as usize * province::SIZE + province::ID)? != id as i32 {
                return Err(format!("province {id} not in the color vector"));
            }
            m.write(call.arg(0) + id as usize * 4, color)?;
        }
        if let Some(t) = s.ticket.as_mut() {
            t.applied = true;
        }
        Ok(())
    })();
    if let Err(e) = result {
        match s.ticket.as_mut() {
            Some(t) => t.error = Some(e),
            None => log!("map: {e}"),
        }
    }
}

fn renderer_after(call: &mut Call) {
    let Ok(mut guard) = STATE.try_lock() else { return };
    if let Some(t) = guard.as_mut().and_then(|s| s.ticket.as_mut()) {
        if call.faulted() {
            t.error = Some("renderer unwound".into());
        } else if t.mode == call.arg_as::<i32>(1) {
            t.rendered = true;
        }
    }
}

/// The color pass the core observes, and the refresh it calls.
pub fn points() -> Vec<Point> {
    vec![
        Point::observe(FILL, "48 89 5c 24 08 48 89 6c 24 18 56 57 41 54 41 56", "map color fill", 3, Some(fill_before), Some(fill_after)),
        Point::observe(SECONDARY, "48 8b c4 89 50 10 55 53 56 57 41 54 41 55 41 56", "map secondary colors", 2, None, Some(secondary_after)),
        Point::observe(RENDERER, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "map renderer", 3, None, Some(renderer_after)),
        Point::keep(REFRESH, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "map mode refresh"),
        Point::method(classes::MAP_MANAGER, map_manager::REFRESH, REFRESH.rva(), "map mode refresh method"),
    ]
}

fn manager(m: &Memory, ctx: &game::Ctx) -> GResult<usize> {
    let object = game::map_manager(m, ctx)?;
    if m.vslot(object, map_manager::REFRESH)? != m.at(REFRESH) {
        return Err("unexpected map manager".to_string().into());
    }
    Ok(object)
}

/// Switches the map mode and confirms the hooks saw the full color pass.
fn refresh(m: &Memory, manager: usize, mode: i32, require_color: bool) -> GResult<()> {
    with_state(|s| s.ticket = Some(Ticket { mode, ..Default::default() }));
    let call = REFRESH.call(m, [manager, mode as usize, 0, 1]);
    let ticket = with_state(|s| s.ticket.take()).unwrap_or_default();
    call?;
    if m.i32(manager + map_manager::MODE)? != mode || !ticket.validated || !ticket.rendered || (require_color && !ticket.applied) || ticket.error.is_some() {
        return Err(format!(
            "map refresh not confirmed: validated={} rendered={} applied={} error={:?}",
            ticket.validated, ticket.rendered, ticket.applied, ticket.error
        )
        .into());
    }
    Ok(())
}

/// Replaces `extension`'s coloring (memory order B, G, R, A per province).
pub fn paint(m: &Memory, ctx: &game::Ctx, extension: usize, colors: &[MapColor]) -> GResult<()> {
    if colors.is_empty() || colors.len() > game::MAX_PROVINCES {
        return fail(Status::InvalidArgument, "color list is empty or too long");
    }
    let manager = manager(m, ctx)?;
    with_state(|s| -> GResult<()> {
        if let Some((old, _)) = s.saved {
            if old != manager {
                return Err("map manager changed during coloring".to_string().into());
            }
        } else {
            let mode = m.i32(manager + map_manager::MODE)?;
            if !(0..=0x4c).contains(&mode) {
                return Err(format!("unexpected map mode {mode}").into());
            }
            s.saved = Some((manager, mode));
        }
        s.coloring.paint(extension, colors);
        Ok(())
    })?;
    if let Err(e) = refresh(m, manager, PAINT_MODE, true) {
        let restored = restore(m, ctx);
        return Err(format!("{}; restore: {:?}", e.message, restored.err().map(|r| r.message)).into());
    }
    Ok(())
}

fn restore(m: &Memory, ctx: &game::Ctx) -> GResult<()> {
    let saved = with_state(|s| {
        s.coloring.clear();
        s.saved
    });
    if let Some((object, mode)) = saved {
        if manager(m, ctx)? != object {
            with_state(|s| s.saved = None);
            return Err("map manager changed before restoring".to_string().into());
        }
        refresh(m, object, mode, false)?;
        with_state(|s| s.saved = None);
    }
    Ok(())
}

pub fn clear(m: &Memory, ctx: &game::Ctx, extension: usize) -> GResult<()> {
    if !with_state(|s| s.coloring.shows(extension)) {
        return Ok(());
    }
    restore(m, ctx)
}

/// Drops coloring state at a session change, restoring the mode when the
/// same map is still there. Told by the sessions (`crate::sessions`) at the
/// new session's first frame.
pub fn reset_session() {
    let saved = with_state(|s| {
        s.coloring = Coloring::default();
        s.saved.take()
    });
    let Some(m) = crate::memory() else { return };
    if let (Some((old, old_mode)), Ok(ctx)) = (saved, game::context(m)) {
        if let Ok(current) = manager(m, &ctx) {
            let current_mode = m.i32(current + map_manager::MODE).unwrap_or(-1);
            let target = if current == old && current_mode == PAINT_MODE { old_mode } else { current_mode };
            if (0..=0x4c).contains(&target) {
                if let Err(e) = refresh(m, current, target, false) {
                    log!("map: restoring after session change failed: {}", e.message);
                }
            }
        }
    }
}

/// Extensions whose coloring another extension replaced since the last call.
pub fn take_lost() -> Vec<usize> {
    with_state(|s| s.coloring.take_lost())
}

/// Releases a stopped extension's coloring.
pub fn release(m: &Memory, extension: usize) {
    if let Ok(ctx) = game::context(m) {
        if let Err(e) = clear(m, &ctx, extension) {
            log!("map: release failed: {}", e.message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(province: u32) -> MapColor {
        MapColor { province, rgba: [1, 2, 3, 4] }
    }

    #[test]
    fn the_last_extension_to_color_the_map_shows_and_the_one_it_replaced_is_told() {
        let mut coloring = Coloring::default();
        coloring.paint(1, &[color(151)]);
        assert!(coloring.shows(1));
        assert_eq!(coloring.colors.get(&151), Some(&[3, 2, 1, 4]), "kept in memory order, B, G, R, A");
        coloring.paint(1, &[color(152)]);
        assert!(coloring.take_lost().is_empty(), "an extension replacing its own coloring loses nothing");
        assert_eq!(coloring.colors.keys().collect::<Vec<_>>(), [&152]);
        coloring.paint(2, &[color(151)]);
        assert!(coloring.shows(2) && !coloring.shows(1));
        assert_eq!(coloring.take_lost(), [1]);
        assert!(coloring.take_lost().is_empty());
        coloring.clear();
        assert!(!coloring.shows(2) && coloring.colors.is_empty());
    }
}
