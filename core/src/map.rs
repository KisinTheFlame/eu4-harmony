//! Province coloring on the map. The core switches to a native map mode whose
//! color pass it can observe (mode 12, option 26) and overwrites the colors of
//! the requested provinces; clearing restores the mode that was active before.
//! Ported from eu4-regional-combat's range preview
//! (docs/research/regional-combat/native-range-preview-findings.txt).
use crate::{
    game::{self, fail, GResult, MAP, MAP_MANAGER_VT, PROVINCE_STRIDE},
    hooks::HookFrame,
    memory::Memory,
    points::Point,
    Result,
};
use eu4_harmony_abi as abi;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::Mutex,
};

const FILL: usize = 0x12fd9b0;
const SECONDARY: usize = 0x12fdcb0;
const RENDERER: usize = 0x10a8c30;
const REFRESH: usize = 0x821ff0;
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

#[derive(Default)]
struct State {
    owner: Option<usize>,
    /// province -> bytes in memory order (B, G, R, A)
    colors: BTreeMap<u32, [u8; 4]>,
    /// (manager, mode) to restore
    saved: Option<(usize, i32)>,
    ticket: Option<Ticket>,
    /// Extensions whose coloring was taken over, to notify.
    lost: Vec<usize>,
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

extern "C" fn fill_before(frame: *mut HookFrame) {
    crate::guarded("map fill", || {
        let Some(m) = crate::memory() else { return };
        let a = unsafe { &(*frame).args };
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
    });
}

extern "C" fn fill_after(frame: *mut HookFrame) {
    crate::guarded("map fill", || {
        FILLS.with(|s| s.borrow_mut().pop());
        if unsafe { (*frame).fault } != 0 {
            with_state(|s| {
                if let Some(t) = s.ticket.as_mut() {
                    t.error = Some("color fill unwound".into());
                }
            });
        }
    });
}

/// After the game's secondary color pass for the paint mode, overwrite our provinces.
extern "C" fn secondary_after(frame: *mut HookFrame) {
    crate::guarded("map colors", || {
        let Some(m) = crate::memory() else { return };
        let frame = unsafe { &*frame };
        let Ok(mut guard) = STATE.try_lock() else { return };
        let Some(s) = guard.as_mut() else { return };
        if s.ticket.is_none() && s.colors.is_empty() {
            return;
        }
        let result = (|| -> Result<()> {
            if frame.fault != 0 {
                return Err("secondary color pass unwound".into());
            }
            let fill = FILLS.with(|f| f.borrow().last().copied().flatten()).ok_or("no enclosing color fill")?;
            if fill.option != frame.args[1] as u32 || fill.data.checked_add(fill.capacity * 4) != Some(frame.args[0]) {
                return Err("secondary buffer does not follow the fill buffer".into());
            }
            let map = m.ptr(m.at(MAP))?;
            let first = m.ptr(map + 0x6f0)?;
            let end = m.ptr(map + 0x6f8)?;
            let bytes = end.checked_sub(first).ok_or("reversed province vector")?;
            if first == 0 || bytes % PROVINCE_STRIDE != 0 || bytes / PROVINCE_STRIDE > fill.capacity {
                return Err("province vector exceeds color capacity".into());
            }
            if let Some(t) = s.ticket.as_mut() {
                if t.mode == fill.mode {
                    t.validated = true;
                }
            }
            if fill.mode != PAINT_MODE || fill.option != PAINT_OPTION || s.colors.is_empty() {
                return Ok(());
            }
            let count = bytes / PROVINCE_STRIDE;
            for (&id, color) in &s.colors {
                if id as usize >= count || m.i32(first + id as usize * PROVINCE_STRIDE + 0x20)? != id as i32 {
                    return Err(format!("province {id} not in the color vector"));
                }
                m.write(frame.args[0] + id as usize * 4, color)?;
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
    });
}

extern "C" fn renderer_after(frame: *mut HookFrame) {
    crate::guarded("map renderer", || {
        let frame = unsafe { &*frame };
        let Ok(mut guard) = STATE.try_lock() else { return };
        if let Some(t) = guard.as_mut().and_then(|s| s.ticket.as_mut()) {
            if frame.fault != 0 {
                t.error = Some("renderer unwound".into());
            } else if t.mode == frame.args[1] as i32 {
                t.rendered = true;
            }
        }
    });
}

/// The color pass the core observes, and the refresh it calls.
pub fn points() -> Vec<Point> {
    vec![
        Point::observe(FILL, "48 89 5c 24 08 48 89 6c 24 18 56 57 41 54 41 56", "map color fill", 3, Some(fill_before), Some(fill_after)),
        Point::observe(SECONDARY, "48 8b c4 89 50 10 55 53 56 57 41 54 41 55 41 56", "map secondary colors", 2, None, Some(secondary_after)),
        Point::observe(RENDERER, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "map renderer", 3, None, Some(renderer_after)),
        Point::keep(REFRESH, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "map mode refresh"),
    ]
}

fn manager(m: &Memory, ctx: &game::Ctx) -> GResult<usize> {
    let object = m.ptr(ctx.world + 0x1e00)?;
    if m.ptr(object)? != m.at(MAP_MANAGER_VT) || m.vslot(object, 0xc8)? != m.at(REFRESH) {
        return Err("unexpected map manager".to_string().into());
    }
    Ok(object)
}

/// Switches the map mode and confirms the hooks saw the full color pass.
fn refresh(m: &Memory, manager: usize, mode: i32, require_color: bool) -> GResult<()> {
    with_state(|s| s.ticket = Some(Ticket { mode, ..Default::default() }));
    let call = m.call(m.at(REFRESH), &[manager, mode as usize, 0, 1]);
    let ticket = with_state(|s| s.ticket.take()).unwrap_or_default();
    call?;
    if m.i32(manager + 0x2c)? != mode || !ticket.validated || !ticket.rendered || (require_color && !ticket.applied) || ticket.error.is_some() {
        return Err(format!(
            "map refresh not confirmed: validated={} rendered={} applied={} error={:?}",
            ticket.validated, ticket.rendered, ticket.applied, ticket.error
        )
        .into());
    }
    Ok(())
}

/// Replaces `extension`'s coloring (memory order B, G, R, A per province).
pub fn paint(m: &Memory, ctx: &game::Ctx, extension: usize, colors: &[abi::MapColor]) -> GResult<()> {
    if colors.is_empty() || colors.len() > game::MAX_PROVINCES {
        return fail(abi::INVALID_ARGUMENT, "color list is empty or too long");
    }
    let manager = manager(m, ctx)?;
    with_state(|s| -> GResult<()> {
        if let Some((old, _)) = s.saved {
            if old != manager {
                return Err("map manager changed during coloring".to_string().into());
            }
        } else {
            let mode = m.i32(manager + 0x2c)?;
            if !(0..=0x4c).contains(&mode) {
                return Err(format!("unexpected map mode {mode}").into());
            }
            s.saved = Some((manager, mode));
        }
        if let Some(previous) = s.owner.filter(|&p| p != extension) {
            s.lost.push(previous);
        }
        s.owner = Some(extension);
        s.colors = colors.iter().map(|c| (c.province, [c.rgba[2], c.rgba[1], c.rgba[0], c.rgba[3]])).collect();
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
        s.colors.clear();
        s.owner = None;
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
    if with_state(|s| s.owner) != Some(extension) {
        return Ok(());
    }
    restore(m, ctx)
}

/// Drops coloring state at a session change, restoring the mode when the
/// same map is still there.
pub fn reset_session(m: &Memory, ctx: &game::Ctx) {
    let saved = with_state(|s| {
        s.colors.clear();
        s.owner = None;
        s.lost.clear();
        s.saved.take()
    });
    if let Some((old, old_mode)) = saved {
        if let Ok(current) = manager(m, ctx) {
            let current_mode = m.i32(current + 0x2c).unwrap_or(-1);
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
    with_state(|s| std::mem::take(&mut s.lost))
}

/// Releases a stopped extension's coloring.
pub fn release(m: &Memory, extension: usize) {
    if let Ok(ctx) = game::context(m) {
        if let Err(e) = clear(m, &ctx, extension) {
            log!("map: release failed: {}", e.message);
        }
    }
}
