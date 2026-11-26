//! What the game's timeline (`0x25` map mode) misattributes, drops, or delays
//! when tags are formed, territories shift, or playback pauses
//! (docs/game/interface/timeline.md):
//!
//! Native defects repaired:
//!
//! 1. **Same-day event ordering (`0x13c2980`)**: `CTimelineData::Build` gathers
//!    province ownership events forward and country history backward via
//!    `0x13c2220`, runs `std::stable_sort` descending (`>`), then calls
//!    `std::reverse`. Same-day `ChangeTag` thus runs before same-day
//!    `ProvinceOwner` changes and swaps conquered provinces back to the old
//!    tag.
//! 2. **Formation-day map color (`0x76491b`, `0x13c2220`)**: `ChangeTag`
//!    logs `changed_country_mapcolor_from` right before `changed_tag_from` in
//!    the same date block. Walking child entries backward switches the tag at
//!    `changed_tag_from` first, then assigns the new tag's color to the old
//!    tag.
//! 3. **Swapped-out dead-tag history (`0x76416c`, `0x13c2220`)**: `ChangeTag`
//!    swaps the two `CCountry*` objects without re-attributing pre-swap history
//!    on the swapped-out object (or fixing undated top-level items away from
//!    the final tag).
//! 4. **Day-0 live-game events (`0x13c2ff5`–`0x13c3011`)**: `Build` applies
//!    `base_timeline` up to `start_date` and then calls `ApplyEventsUntil`
//!    starting past `snap->date == start_date`, skipping live-game events on
//!    `start_date`.
//! 5. **Pause and end-of-timeline name refresh (`0x8153fd`–`0x815497`)**:
//!    the checks that pause, exceed `24 / speed` months, or reach
//!    `world->date` live inside `if (CTimeline::Update(dt))`, so pausing or
//!    ending during a month with no events never flags map names (`map+0x25a0`)
//!    for rebuilding.
//! 6. **Timeline borders (`0x12dc261`, `0x12dc4a4`)**: border classification
//!    receives `(mode != 0x25)` at `[rsp + 0x118]`, yet still reads live
//!    `province+0x468` (`owner`) and `+0x488` (`tribal_owner`) instead of the
//!    active `CTimelineState`'s `owners[]`.
//!
//! Playback responsiveness improvements:
//!
//! 7. **Immediate timeline color updates (`0x6ea0b0`, `0x815405`)**: in map
//!    mode `0x25`, schedule the color texture update immediately (`+0x38 = 0`)
//!    instead of waiting out the native `MAX_MAPMODE_COLOR_UPDATE_DELAY_MS`
//!    (200 ms) batching delay.
//! 8. **Responsive playback map names (`0x8153fd`)**: rebuild map names on the
//!    frame a `ChangeTag`, `CountryName`, or `CountryAdjective` event applies,
//!    and cap periodic name rebuilds during territory playback at `0.25 s`
//!    (`min(0.25, 24.0 / speed)`) rather than waiting `24 / speed` real
//!    seconds.

use crate::{
    base::{called_by_game, Result},
    game_heap::{self, delete},
    game_string,
    hooks::Call,
    memory::Memory,
    objects::{
        application,
        campaign::{country, globals, map_manager, province, tag, world},
        text,
    },
    points::{Function, GameFunction, Point},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        Mutex,
    },
};

// --- Native functions, globals, and vtables ---

const BUILD: usize = 0x13c2980;
const STOP: usize = 0x13c3540;
const APPLY_EVENTS_UNTIL: GameFunction<3> = GameFunction::at(0x13c3030);
const STATE_CTOR: GameFunction<1> = GameFunction::at(0x13c36b0);
const VECTOR_ALLOC: GameFunction<1> = GameFunction::at(0x950b0);

const TIMELINE_SINGLETON: usize = 0x2340118;
const NAME_TIMER: usize = 0x245b48c;
const NAME_REFRESH_MONTHS: usize = 0x1dc1848;

const VTABLE_CHANGE_TAG: usize = 0x1d80988;
const VTABLE_COUNTRY_MAPCOLOR: usize = 0x1d80998;
const VTABLE_PROVINCE_OWNER: usize = 0x1d809a8;
const VTABLE_COUNTRY_ADJECTIVE: usize = 0x1d809c8;
const VTABLE_COUNTRY_NAME: usize = 0x1d809d8;
const VTABLE_CAPITAL: usize = 0x1d809e8;
const VTABLE_COLOR: usize = 0x1c48ca0;

// --- Hook and resume sites ---

const UPDATE_CHECK: usize = 0x8153fd;
const UPDATE_APPLIED_AT: usize = 0x815405;
const UPDATE_REFRESH_NAMES_AT: usize = 0x815478;
const UPDATE_DONE_AT: usize = 0x815497;
crate::resume!(
    UPDATE_RESUME:
        UPDATE_APPLIED = UPDATE_APPLIED_AT,
        UPDATE_REFRESH_NAMES = UPDATE_REFRESH_NAMES_AT,
        UPDATE_DONE = UPDATE_DONE_AT
);

const BORDER_OWNERS: usize = 0x12dc261;
const BORDER_OWNERS_RESUME_AT: usize = 0x12dc283;
crate::resume!(BORDER_RESUME: BORDER_OWNERS_RESUME = BORDER_OWNERS_RESUME_AT);

const STATE_BORDER_OWNERS: usize = 0x12dc4a4;
const STATE_BORDER_VALID_AT: usize = 0x12dc4c2;
const STATE_BORDER_INVALID_AT: usize = 0x12dc4d0;
crate::resume!(
    STATE_BORDER_RESUME:
        STATE_BORDER_VALID = STATE_BORDER_VALID_AT,
        STATE_BORDER_INVALID = STATE_BORDER_INVALID_AT
);

// --- Struct field offsets ---

const TIMELINE_SPEED: usize = 0x10;
const TIMELINE_STATE: usize = 0x18;
const TIMELINE_DEFAULT_STATE: usize = 0x20;

const DATA_EVENTS_END: usize = 0x8;
const DATA_EVENTS_CAP: usize = 0x10;
const DATA_SNAPSHOTS: usize = 0x18;

const STATE_OWNERS_BEGIN: usize = 0x8;
const STATE_OWNERS_END: usize = 0x10;
const STATE_SIZE: usize = 0x80;

const WORLD_PROVINCE_LIST: usize = 0x1c90;
const WORLD_PROVINCE_LIST_END: usize = 0x1c98;
const WORLD_COUNTRIES_BEGIN: usize = 0x1ce8;
const WORLD_COUNTRIES_END: usize = 0x1cf0;
const WORLD_START_DATE: usize = 0x1df4;

const PROVINCE_TRIBAL_OWNER: usize = 0x488;
const PROVINCE_HISTORY_HEAD: usize = 0x50;
const COUNTRY_HISTORY_HEAD: usize = 0x11c8;

const NODE_NEXT: usize = 0x10;
const ITEM_DATE: usize = 0x10;
const ITEM_CHILDREN_HEAD: usize = 0x20;
const ITEM_CHANGED_TAG: usize = 0x40;
const ITEM_CAPITAL_PROVINCE: usize = 0x40;
const ITEM_OWNER_TAG: usize = 0x58;
const ITEM_STRING: usize = 0x60;
const ITEM_COLOR_META: usize = 0x70;
const ITEM_COLOR_RGBA: usize = 0x78;
const ITEM_FAKE_OWNER: usize = 0x79;
const ITEM_COLOR_IS_NULL: usize = 0x88;
const ITEM_VTABLE_GET_TOKEN: usize = 0x80;

const TOKEN_OWNER: u32 = 0x2841;
const TOKEN_CAPITAL: u32 = 0x284e;
const TOKEN_CHANGED_TAG_FROM: u32 = 0x2d1f;
const TOKEN_COUNTRY_NAME: u32 = 0x3423;
const TOKEN_COUNTRY_MAPCOLOR: u32 = 0x3424;
const TOKEN_COUNTRY_ADJECTIVE: u32 = 0x3425;

const TIMELINE_MAP_MODE: u32 = 0x25;
const IDLER_MAP_MODE: usize = map_manager::MODE;
const IDLER_COLORS_DUE: usize = 0x34;
const IDLER_COLORS_AT: usize = 0x38;
const IDLER_APPLICATION: usize = map_manager::APPLICATION;
const APPLICATION_MAP: usize = application::MAP;
const MAP_NAMES_DUE: usize = text::map::NAMES_DUE;
const NAME_REFRESH_INTERVAL_SECS: f32 = 0.25;

/// EU4 encoded date `1.1.1` used by undated top-level history entries.
const UNDATED_DATE: i32 = 0x29c9a30;
const HOURS_PER_DAY: i32 = 24;
/// Unowned `"---\0"` tag used when clearing tribal ownership in timeline mode.
const NULL_TAG: u64 = 0x002d_2d2d;

type BuildFn = unsafe extern "C" fn(usize, usize, usize) -> usize;
static ORIGINAL_BUILD: Function<BuildFn> = Function::new();

/// Set when timeline playback advances past events without refreshing map names;
/// cleared once `map+0x25a0` is marked due or timeline stops / resets.
static NAMES_STALE: AtomicBool = AtomicBool::new(false);
static LAST_TIMELINE_DATE: AtomicI32 = AtomicI32::new(i32::MIN);
static NAME_EVENT_DATES: Mutex<Vec<i32>> = Mutex::new(Vec::new());

pub(super) const REPAIR: super::Repair = super::Repair { points, world_reset: Some(world_reset), ..super::Repair::NONE };

fn reset_playback_state() {
    NAMES_STALE.store(false, Ordering::Relaxed);
    LAST_TIMELINE_DATE.store(i32::MIN, Ordering::Relaxed);
    if let Ok(mut dates) = NAME_EVENT_DATES.lock() {
        dates.clear();
    }
}

fn world_reset(_: &Memory, _: usize) {
    reset_playback_state();
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::replace(
            BUILD,
            "4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53 56 57 41 54",
            "CTimelineData::Build reconstructs history order and swaps",
            build_timeline_data as BuildFn,
            &ORIGINAL_BUILD,
        ),
        Point::observe(
            STOP,
            "48 89 5c 24 08 57 48 83 ec 20 48 8b d9 48 8b 09 48 85 c9 74 05",
            "timeline stopped",
            1,
            Some(timeline_stopped),
            None,
        ),
        Point::keep(
            APPLY_EVENTS_UNTIL,
            "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 4c 89 74 24 20 41 57",
            "CTimelineData::ApplyEventsUntil",
        ),
        Point::keep(
            STATE_CTOR,
            "48 89 5c 24 10 48 89 74 24 18 48 89 4c 24 08 57 48 83 ec 20",
            "CTimelineState::CTimelineState",
        ),
        Point::keep(
            VECTOR_ALLOC,
            "48 83 ec 28 48 81 f9 00 10 00 00 72 2a 48 8d 41 27 48 3b c1 76 36",
            "MSVC std::vector buffer allocator",
        ),
        // Bug 1: pause and end-of-timeline name refresh checks.
        Point::jump(
            UPDATE_CHECK,
            "84 c0 0f 84 92 00 00 00",
            "timeline update branches on whether events applied",
            update_check_stub as *const () as usize,
            &UPDATE_RESUME,
        ),
        Point::keep(UPDATE_APPLIED_AT, "ba 25 00 00 00 49 8b cd e8 9e 4c ed ff", "timeline applied event path"),
        Point::keep(
            UPDATE_REFRESH_NAMES_AT,
            "c7 05 0a 60 c4 01 00 00 00 00 49 8b 85 30 03 00 00 48 8b 88 60 03 00 00 c6 81 a0 25 00 00 01",
            "timeline marks map names due",
        )
        .uses(&[NAME_TIMER, IDLER_APPLICATION, APPLICATION_MAP, MAP_NAMES_DUE]),
        Point::keep(0x6ea0b9usize, "39 51 2c 75 6d 80 79 34 00 75 67", "map mode color update guard checks mode and pending flag")
            .uses(&[IDLER_MAP_MODE, IDLER_COLORS_DUE]),
        Point::keep(0x6ea127usize, "48 89 53 38", "map mode color update deadline store").uses(&[IDLER_COLORS_AT]),
        Point::keep(UPDATE_DONE_AT, "49 8b 8d b8 10 00 00", "after timeline update"),
        Point::keep(0x815435usize, "0f 2f 78 10 73 3d", "timeline speed pause check").uses(&[TIMELINE_SPEED]),
        Point::keep(
            0x815440usize,
            "f3 0f 10 0d 00 c4 5a 01 f3 0f 5e 48 10 f3 0f 10 05 37 60 c4 01 0f 2f c1 77 1e",
            "timeline periodic name timer threshold",
        )
        .uses(&[NAME_REFRESH_MONTHS, TIMELINE_SPEED, NAME_TIMER]),
        Point::keep(
            0x815467usize,
            "48 8b 0d 0a aa b2 01 8b 89 d0 1d 00 00 39 08 7c 1f",
            "timeline end-date name refresh comparison",
        )
        .uses(&[globals::WORLD, world::DATE]),
        // Bug 5: border classification in timeline mode.
        Point::jump(
            BORDER_OWNERS,
            "4c 8b 8b 68 04 00 00 4d 8b 95 68 04 00 00 49 8b c9 48 8b 93 88 04 00 00 49 8b c2 4d 8b 85 88 04 00 00",
            "province border reads live owner and tribal owner",
            border_owners_stub as *const () as usize,
            &BORDER_RESUME,
        )
        .uses(&[province::OWNER, PROVINCE_TRIBAL_OWNER]),
        Point::keep(BORDER_OWNERS_RESUME_AT, "41 38 bb 84 74 00 00", "after province border owners load"),
        Point::jump(
            STATE_BORDER_OWNERS,
            "48 8b 87 68 04 00 00 48 c1 e8 38 84 c0 74 1d 49 8b 85 68 04 00 00 48 c1 e8 38 84 c0 74 0e",
            "state border check reads live province owners",
            state_border_owners_stub as *const () as usize,
            &STATE_BORDER_RESUME,
        )
        .uses(&[province::OWNER]),
        Point::keep(STATE_BORDER_VALID_AT, "45 84 f6 75 09 40 84 ed 75 04 b0 01 eb 02", "state border check when both owned"),
        Point::keep(STATE_BORDER_INVALID_AT, "32 c0 84 c0 b9 06 00 00 00 41 be 01 00 00 00", "state border check fallback"),
        Point::keep(
            0x12dd1d9usize,
            "83 7a 2c 25 0f b6 81 4b 01 00 00 49 8b 8e 88 00 00 00 41 0f 95 c0",
            "map mode 0x25 check feeding border non-timeline parameter",
        ),
        // Timeline singleton and CTimelineData layout keep-checks.
        Point::keep(0x13c3366usize, "48 8b 05 ab cd f7 00", "timeline singleton access").uses(&[TIMELINE_SINGLETON]),
        Point::keep(0x13c36a0usize, "48 8b 41 18 48 85 c0 75 04 48 8d 41 20 c3", "active timeline state accessor")
            .uses(&[TIMELINE_STATE, TIMELINE_DEFAULT_STATE]),
        Point::keep(
            0x13c29b9usize,
            "4c 89 29 4c 89 69 08 4c 89 69 10 4c 8d 61 18 4d 89 2c 24 4d 89 6c 24 08 4d 89 6c 24 10",
            "CTimelineData vector layout",
        )
        .uses(&[DATA_EVENTS_END, DATA_EVENTS_CAP, DATA_SNAPSHOTS]),
        Point::keep(
            0x13c29dfusize,
            "48 8b 82 98 1c 00 00 48 2b 82 90 1c 00 00 48 c1 f8 03",
            "world province pointer vector",
        )
        .uses(&[WORLD_PROVINCE_LIST, WORLD_PROVINCE_LIST_END]),
        Point::keep(0x13c2a11usize, "49 8b 80 a8 1c 00 00 48 8b 74 01 50", "world province history list")
            .uses(&[world::PROVINCES, PROVINCE_HISTORY_HEAD]),
        Point::keep(
            0x13c2b5busize,
            "49 8b 80 f0 1c 00 00 49 2b 80 e8 1c 00 00 48 c1 f8 03",
            "world country pointer vector",
        )
        .uses(&[WORLD_COUNTRIES_BEGIN, WORLD_COUNTRIES_END]),
        Point::keep(
            0x13c2b80usize,
            "49 8b 80 e8 1c 00 00 48 8b 14 d8 4c 8d 82 c8 11 00 00 48 8b 52 20",
            "country history list and final tag",
        )
        .uses(&[WORLD_COUNTRIES_BEGIN, COUNTRY_HISTORY_HEAD, country::TAG]),
        Point::keep(0x13c2fd7usize, "8b 98 f4 1d 00 00", "world start date field").uses(&[WORLD_START_DATE]),
        Point::keep(0x13c21d5usize, "48 8b 42 10 48 2b 42 08 48 c1 f8 03", "CTimelineState owners vector")
            .uses(&[STATE_OWNERS_BEGIN, STATE_OWNERS_END]),
        // Timeline event constructors & history entry readers.
        Point::keep(0x13c2a0ausize, "4c 8d 25 97 df 9b 00", "province owner event vtable").uses(&[VTABLE_PROVINCE_OWNER]),
        Point::keep(
            0x13c2a55usize,
            "48 8b 4b 58 c7 40 08 30 9a 9c 02 4c 89 20 44 89 68 10 48 89 48 14",
            "undated province owner item fields",
        )
        .uses(&[ITEM_OWNER_TAG, UNDATED_DATE as usize]),
        Point::keep(0x13c2ab6usize, "41 8b 7f 10 41 80 7f 79 00 75 47", "dated province owner item date and fake flag")
            .uses(&[ITEM_DATE, ITEM_FAKE_OWNER]),
        Point::keep(
            0x13c261fusize,
            "89 58 08 48 8d 0d 5f e3 9b 00 48 89 08 4c 89 70 10 48 89 78 18",
            "change tag timeline event vtable",
        )
        .uses(&[VTABLE_CHANGE_TAG]),
        Point::keep(
            0x13c243busize,
            "8b 4f 40 89 58 08 48 8d 1d a0 e5 9b 00 48 89 18 4c 89 70 10 89 48 18",
            "capital timeline event vtable and province id",
        )
        .uses(&[ITEM_CAPITAL_PROVINCE, VTABLE_CAPITAL]),
        Point::keep(
            0x13c26e5usize,
            "89 5f 08 48 8d 05 e9 e2 9b 00 48 89 07 4c 89 77 10",
            "country name timeline event vtable",
        )
        .uses(&[VTABLE_COUNTRY_NAME]),
        Point::keep(
            0x13c27e6usize,
            "89 5f 08 48 8d 05 d8 e1 9b 00 48 89 07 4c 89 77 10",
            "country adjective timeline event vtable",
        )
        .uses(&[VTABLE_COUNTRY_ADJECTIVE]),
        Point::keep(
            0x13c28d5usize,
            "89 58 08 48 8d 05 b9 e0 9b 00 48 89 02 4c 89 72 10 8b 4f 70 89 4a 20 48 8d 05 ad 63 88 00 48 89 42 18 0f 10 47 78 0f 11 42 28 0f b6 87 88 00 00 00 88 42 38",
            "country map color timeline event vtable and fields",
        )
        .uses(&[VTABLE_COUNTRY_MAPCOLOR, VTABLE_COLOR, ITEM_COLOR_META, ITEM_COLOR_RGBA, ITEM_COLOR_IS_NULL]),
        Point::keep(
            0x7e800cusize,
            "48 8b 59 20 48 85 db 74 6d 66 66 66 0f 1f 84 00 00 00 00 00 48 8b 0b 48 8b 5b 10",
            "history entry forward children list walk",
        )
        .uses(&[ITEM_CHILDREN_HEAD, NODE_NEXT]),
        Point::keep(0x13c2260usize, "49 8b 37 48 8b 06 48 8b ce ff 90 80 00 00 00", "history item token getter")
            .uses(&[ITEM_VTABLE_GET_TOKEN]),
        Point::keep(0x13c22ecusize, "48 8d 56 60", "country name or adjective string offset").uses(&[ITEM_STRING]),
        Point::keep(0x13c260dusize, "48 8b 7f 40", "changed tag value offset").uses(&[ITEM_CHANGED_TAG]),
    ]
}

// --- Bugs 1, 5 & playback responsiveness: timeline colors and map names ---

fn timeline_stopped(_: &mut Call) {
    reset_playback_state();
}

#[unsafe(naked)]
extern "C" fn update_check_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(
            "movzx ecx, al",
            "mov rdx, r13",
            call on_timeline_update,
            // Compare before leaving the stub frame; `mov rsp, rbp; pop rbp` preserves flags.
            "cmp dword ptr [rsp + 0x48], 1",
        ),
        "je 8f",
        "jb 9f",
        "jmp qword ptr [rip + {done}]",
        "8:",
        "jmp qword ptr [rip + {refresh}]",
        "9:",
        "jmp qword ptr [rip + {applied}]",
        on_timeline_update = sym on_timeline_update,
        applied = sym UPDATE_APPLIED,
        refresh = sym UPDATE_REFRESH_NAMES,
        done = sym UPDATE_DONE,
    );
}

fn collect_name_event_dates(events: &[RawEvent]) -> Vec<i32> {
    let mut dates: Vec<i32> = events
        .iter()
        .filter(|ev| matches!(ev.kind, RawEventKind::ChangeTag { .. } | RawEventKind::CountryName { .. } | RawEventKind::CountryAdjective { .. }))
        .map(|ev| ev.date)
        .collect();
    dates.sort_unstable();
    dates.dedup();
    dates
}

fn has_name_event_in_range(dates: &[i32], prev_date: i32, new_date: i32) -> bool {
    if new_date <= prev_date {
        return false;
    }
    let idx = dates.partition_point(|&d| d <= prev_date);
    dates.get(idx).is_some_and(|&d| d <= new_date)
}

fn should_refresh_names_policy(speed: f32, timer: f32, native_months: f32, timeline_date: i32, world_date: i32, has_name_event: bool) -> bool {
    if has_name_event || speed <= 0.0 || timeline_date >= world_date {
        return true;
    }
    let threshold = NAME_REFRESH_INTERVAL_SECS.min(native_months / speed);
    timer >= threshold
}

fn schedule_immediate_timeline_colors(m: &Memory, idler: usize) {
    if idler != 0 && m.u32(idler + IDLER_MAP_MODE).ok() == Some(TIMELINE_MAP_MODE) {
        let _ = m.write(idler + IDLER_COLORS_DUE, &[1]);
        let _ = m.write(idler + IDLER_COLORS_AT, &0i64.to_le_bytes());
    }
}

fn mark_map_names_due(m: &Memory, idler: usize) -> Result<()> {
    if idler == 0 {
        return Err("null idler".into());
    }
    let app = m.ptr(idler + IDLER_APPLICATION)?;
    if app == 0 {
        return Err("null application".into());
    }
    let map = m.ptr(app + APPLICATION_MAP)?;
    if map == 0 {
        return Err("null map".into());
    }
    m.write(m.at(NAME_TIMER), &0u32.to_le_bytes())?;
    m.write(map + MAP_NAMES_DUE, &[1])
}

/// Returns 0 to run `UPDATE_APPLIED`, 1 to jump to `UPDATE_REFRESH_NAMES`, or
/// 2 to skip directly to `UPDATE_DONE`.
#[called_by_game]
extern "C" fn on_timeline_update(updated: u8, idler: usize) -> u32 {
    let m = crate::hooked_memory();
    let applied = updated != 0;
    if applied {
        schedule_immediate_timeline_colors(&m, idler);
    }
    let Ok(should_refresh) = timeline_should_refresh_names(&m, applied) else {
        if applied {
            NAMES_STALE.store(true, Ordering::Relaxed);
            return 0;
        }
        return 2;
    };
    if applied {
        let refreshed_now = should_refresh && mark_map_names_due(&m, idler).is_ok();
        NAMES_STALE.store(!refreshed_now, Ordering::Relaxed);
        return 0;
    }
    if should_refresh && NAMES_STALE.swap(false, Ordering::Relaxed) {
        return 1;
    }
    2
}

fn timeline_should_refresh_names(m: &Memory, applied: bool) -> Result<bool> {
    let timeline = m.ptr(m.at(TIMELINE_SINGLETON))?;
    if timeline == 0 || m.ptr(timeline)? == 0 {
        reset_playback_state();
        return Ok(false);
    }
    let speed = f32::from_bits(m.u32(timeline + TIMELINE_SPEED)?);
    let timer = f32::from_bits(m.u32(m.at(NAME_TIMER))?);
    let native_months = f32::from_bits(m.u32(m.at(NAME_REFRESH_MONTHS))?);
    let state = active_timeline_state(m, timeline)?;
    let timeline_date = m.i32(state)?;
    let world = m.ptr(m.at(globals::WORLD))?;
    if world == 0 {
        return Ok(false);
    }
    let world_date = m.i32(world + world::DATE)?;
    let start_date = m.i32(world + WORLD_START_DATE)?;
    let has_name_event = if applied {
        let prev = LAST_TIMELINE_DATE.swap(timeline_date, Ordering::Relaxed);
        let prev_date = if prev == i32::MIN || timeline_date < prev { start_date } else { prev };
        NAME_EVENT_DATES.lock().map(|dates| has_name_event_in_range(&dates, prev_date, timeline_date)).unwrap_or(false)
    } else {
        LAST_TIMELINE_DATE.store(timeline_date, Ordering::Relaxed);
        false
    };
    Ok(should_refresh_names_policy(speed, timer, native_months, timeline_date, world_date, has_name_event))
}

fn active_timeline_state(m: &Memory, timeline: usize) -> Result<usize> {
    let state = m.ptr(timeline + TIMELINE_STATE)?;
    Ok(if state != 0 { state } else { timeline + TIMELINE_DEFAULT_STATE })
}

// --- Bug 5: Timeline borders ---

#[unsafe(naked)]
extern "C" fn border_owners_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rsp + 0x118], 0",
        "jne 9f",
        crate::call_rust!(
            "mov rcx, rbx",
            "mov rdx, r13",
            "lea r8, [rsp + 0x38]",
            "lea r9, [rsp + 0x40]",
            call timeline_border_owners,
            "cmp dword ptr [rsp + 0x48], 0",
            "je 8f",
            "mov r9, [rsp + 0x38]",
            "mov rcx, r9",
            "mov r10, [rsp + 0x40]",
            "mov rax, r10",
            "mov edx, {null_tag}",
            "mov r8, rdx",
            "8:",
        ),
        "je 9f",
        "jmp qword ptr [rip + {resume}]",
        "9:",
        "mov r9, [rbx + {owner}]",
        "mov r10, [r13 + {owner}]",
        "mov rcx, r9",
        "mov rdx, [rbx + {tribal}]",
        "mov rax, r10",
        "mov r8, [r13 + {tribal}]",
        "jmp qword ptr [rip + {resume}]",
        timeline_border_owners = sym timeline_border_owners,
        resume = sym BORDER_OWNERS_RESUME,
        owner = const province::OWNER,
        tribal = const PROVINCE_TRIBAL_OWNER,
        null_tag = const NULL_TAG as u32,
    );
}

#[unsafe(naked)]
extern "C" fn state_border_owners_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rsp + 0x118], 0",
        "jne 9f",
        crate::call_rust!(
            "mov rcx, rdi",
            "mov rdx, r13",
            call timeline_border_both_owned,
            "cmp dword ptr [rsp + 0x48], 0",
        ),
        "jl 9f",
        "je 7f",
        "jmp qword ptr [rip + {valid}]",
        "9:",
        "mov rax, [rdi + {owner}]",
        "shr rax, {valid_shift}",
        "test al, al",
        "je 7f",
        "mov rax, [r13 + {owner}]",
        "shr rax, {valid_shift}",
        "test al, al",
        "je 7f",
        "jmp qword ptr [rip + {valid}]",
        "7:",
        "jmp qword ptr [rip + {invalid}]",
        timeline_border_both_owned = sym timeline_border_both_owned,
        valid = sym STATE_BORDER_VALID,
        invalid = sym STATE_BORDER_INVALID,
        owner = const province::OWNER,
        valid_shift = const tag::VALID,
    );
}

#[called_by_game]
extern "C" fn timeline_border_owners(prov1: usize, prov2: usize, out1: *mut u64, out2: *mut u64) -> u32 {
    let m = crate::hooked_memory();
    let Ok(Some((t1, t2))) = read_timeline_pair(&m, prov1, prov2) else {
        return 0;
    };
    unsafe {
        out1.write_unaligned(normalize_border_tag(t1));
        out2.write_unaligned(normalize_border_tag(t2));
    }
    1
}

#[called_by_game]
extern "C" fn timeline_border_both_owned(prov1: usize, prov2: usize) -> i32 {
    let m = crate::hooked_memory();
    let Ok(Some((t1, t2))) = read_timeline_pair(&m, prov1, prov2) else {
        return -1;
    };
    i32::from(is_tag_valid(t1) && is_tag_valid(t2))
}

fn normalize_border_tag(raw: u64) -> u64 {
    if is_tag_valid(raw) {
        raw
    } else {
        NULL_TAG
    }
}

fn read_timeline_pair(m: &Memory, prov1: usize, prov2: usize) -> Result<Option<(u64, u64)>> {
    let timeline = m.ptr(m.at(TIMELINE_SINGLETON))?;
    if timeline == 0 {
        return Ok(None);
    }
    let state = active_timeline_state(m, timeline)?;
    let owners_begin = m.ptr(state + STATE_OWNERS_BEGIN)?;
    let owners_end = m.ptr(state + STATE_OWNERS_END)?;
    let count = owners_end.saturating_sub(owners_begin) / 8;
    if count == 0 {
        return Ok(None);
    }
    let world = m.ptr(m.at(globals::WORLD))?;
    if world == 0 {
        return Ok(None);
    }
    let provinces = m.ptr(world + world::PROVINCES)?;
    let Some(idx1) = province_index(provinces, count, prov1) else {
        return Ok(None);
    };
    let Some(idx2) = province_index(provinces, count, prov2) else {
        return Ok(None);
    };
    let t1 = m.u64(owners_begin + idx1 * 8)?;
    let t2 = m.u64(owners_begin + idx2 * 8)?;
    Ok(Some((t1, t2)))
}

fn province_index(provinces_base: usize, count: usize, prov: usize) -> Option<usize> {
    let offset = prov.checked_sub(provinces_base)?;
    if !offset.is_multiple_of(province::SIZE) {
        return None;
    }
    let index = offset / province::SIZE;
    (index < count).then_some(index)
}

// --- Bugs 2, 3, 4, 6: Pure-Rust history reconstruction and CTimelineData build ---

fn is_tag_valid(raw: u64) -> bool {
    ((raw >> tag::VALID) as u8) != 0 && ((raw >> tag::INDEX) as i16) >= 0
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawColor {
    meta: u32,
    rgba: [u8; 16],
    is_null: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RawCountryKind {
    Capital(i32),
    Name(Vec<u8>),
    Adjective(Vec<u8>),
    MapColor(RawColor),
    ChangedTagFrom(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawCountryItem {
    date: i32,
    is_dated_child: bool,
    kind: RawCountryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawCountry {
    final_tag: u64,
    items: Vec<RawCountryItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawProvinceOwner {
    date: i32,
    prov_index: u32,
    owner: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RawEventKind {
    ProvinceOwner { prov_index: u32, owner: u64 },
    ChangeTag { to_tag: u64, from_tag: u64 },
    Capital { tag: u64, prov_id: i32 },
    CountryName { tag: u64, bytes: Vec<u8> },
    CountryAdjective { tag: u64, bytes: Vec<u8> },
    CountryMapColor { tag: u64, color: RawColor },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawEvent {
    date: i32,
    order_group: u8,
    seq: usize,
    kind: RawEventKind,
}

#[derive(Clone, Copy, Debug)]
struct ActiveSwap {
    date: i32,
    active_obj: usize,
    active_item_idx: usize,
    from_tag: u64,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedSwap {
    date: i32,
    active_obj: usize,
    active_item_idx: usize,
    victim_obj: usize,
    from_tag: u64,
    to_tag: u64,
}

#[derive(Clone, Copy, Debug)]
struct TagTransition {
    date: i32,
    item_idx: usize,
    tag_after: u64,
}

#[called_by_game]
unsafe extern "C" fn build_timeline_data(this: usize, world: usize, base: usize) -> usize {
    // `base_timeline` (`world == 0`) builds once from static disk history.
    if world == 0 {
        return unsafe { ORIGINAL_BUILD.get()(this, world, base) };
    }
    reset_playback_state();
    let m = crate::hooked_memory();
    match try_build_timeline_data(&m, this, world, base) {
        Ok(()) => this,
        Err(err) => {
            log!("repair: timeline build fell back to native builder: {err}");
            unsafe { ORIGINAL_BUILD.get()(this, world, base) }
        }
    }
}

fn write_u32(m: &Memory, addr: usize, value: u32) -> Result<()> {
    m.write(addr, &value.to_le_bytes())
}

fn write_i32(m: &Memory, addr: usize, value: i32) -> Result<()> {
    m.write(addr, &value.to_le_bytes())
}

fn write_u64(m: &Memory, addr: usize, value: u64) -> Result<()> {
    m.write(addr, &value.to_le_bytes())
}

fn write_usize(m: &Memory, addr: usize, value: usize) -> Result<()> {
    m.write(addr, &value.to_le_bytes())
}

fn write_usizes(m: &Memory, addr: usize, values: &[usize]) -> Result<()> {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    m.write(addr, &bytes)
}

fn try_build_timeline_data(m: &Memory, data: usize, world: usize, base: usize) -> Result<()> {
    write_usizes(m, data, &[0usize; 6])?;

    let prov_events = collect_province_history(m, world)?;
    let countries = collect_country_history(m, world)?;
    let start_date = m.i32(world + WORLD_START_DATE)?;
    let reconstructed = reconstruct_events(&prov_events, &countries, start_date);
    let name_event_dates = collect_name_event_dates(&reconstructed);

    let mut native_events = Vec::with_capacity(reconstructed.len());
    let mut snapshots = [0usize; 3];
    let build_res = (|| -> Result<()> {
        for ev in &reconstructed {
            native_events.push(alloc_native_event(m, ev)?);
        }

        if !native_events.is_empty() {
            let bytes = native_events.len() * std::mem::size_of::<usize>();
            let vec_buf = VECTOR_ALLOC.call(m, [bytes])?;
            if vec_buf == 0 {
                return Err("null event vector allocation".into());
            }
            for (i, &ptr) in native_events.iter().enumerate() {
                write_usize(m, vec_buf + i * std::mem::size_of::<usize>(), ptr)?;
            }
            write_usizes(m, data, &[vec_buf, vec_buf + bytes, vec_buf + bytes])?;
        }

        let snap = VECTOR_ALLOC.call(m, [STATE_SIZE])?;
        if snap == 0 {
            return Err("null timeline state snapshot allocation".into());
        }
        snapshots = [snap, snap, snap + STATE_SIZE];
        STATE_CTOR.call(m, [snap])?;
        snapshots = [snap, snap + STATE_SIZE, snap + STATE_SIZE];
        write_usizes(m, data + DATA_SNAPSHOTS, &snapshots)?;

        let mut target_date = start_date;
        if base != 0 {
            APPLY_EVENTS_UNTIL.call(m, [base, snap, &mut target_date as *mut i32 as usize])?;
            // Rewind `snap->date` by one day so `ApplyEventsUntil` processes day-0
            // live events at `start_date` without re-applying earlier disk history.
            write_i32(m, snap, start_date.wrapping_sub(HOURS_PER_DAY))?;
        }
        target_date = start_date;
        APPLY_EVENTS_UNTIL.call(m, [data, snap, &mut target_date as *mut i32 as usize])?;
        // `CTimeline::Start` (`0x13c3529`) and `CTimelineView::Restart` (`0x1430d6d`)
        // subtract 24 hours from `active_state->date` after `CreateStateAt(&start_date)`.
        // Stamp `snap->date = start_date + 24` without applying day-2 events so playback
        // opens on `start_date` and day-2 events apply once on the first step.
        write_i32(m, snap, start_date.wrapping_add(HOURS_PER_DAY))?;
        Ok(())
    })();

    if let Err(err) = build_res {
        cleanup_partial_build(m, data, &native_events, snapshots);
        return Err(err);
    }
    if let Ok(mut dates) = NAME_EVENT_DATES.lock() {
        *dates = name_event_dates;
    }
    LAST_TIMELINE_DATE.store(start_date, Ordering::Relaxed);
    Ok(())
}

fn cleanup_partial_build(m: &Memory, data: usize, events: &[usize], snapshots: [usize; 3]) {
    let vec_buf = m.ptr(data).unwrap_or(0);
    let vec_cap = m.ptr(data + DATA_EVENTS_CAP).unwrap_or(0);
    if vec_buf != 0 && vec_cap >= vec_buf {
        let _ = game_heap::free_vector(m, [vec_buf, vec_cap, vec_cap]);
    }
    for &ev in events {
        if ev != 0 {
            if let Ok(dtor) = m.vslot(ev, 0) {
                let _ = m.call(dtor, &[ev, 1]);
            }
        }
    }
    if snapshots[0] != 0 {
        let _ = game_heap::free_vector(m, snapshots);
    }
    let _ = write_usizes(m, data, &[0usize; 6]);
}

// --- History collection from game memory ---

fn collect_province_history(m: &Memory, world: usize) -> Result<Vec<RawProvinceOwner>> {
    let list_begin = m.ptr(world + WORLD_PROVINCE_LIST)?;
    let list_end = m.ptr(world + WORLD_PROVINCE_LIST_END)?;
    let count = list_end.saturating_sub(list_begin) / 8;
    let provinces = m.ptr(world + world::PROVINCES)?;
    let mut out = Vec::new();
    for prov_idx in 0..count {
        let prov = provinces + prov_idx * province::SIZE;
        let mut node = m.ptr(prov + PROVINCE_HISTORY_HEAD)?;
        while node != 0 {
            let item = m.ptr(node)?;
            let get_token = m.vslot(item, ITEM_VTABLE_GET_TOKEN)?;
            let token = m.call(get_token, &[item])? as u32;
            if token == TOKEN_OWNER {
                let owner = m.u64(item + ITEM_OWNER_TAG)?;
                out.push(RawProvinceOwner { date: UNDATED_DATE, prov_index: prov_idx as u32, owner });
            }
            let mut child = m.ptr(item + ITEM_CHILDREN_HEAD)?;
            while child != 0 {
                let child_item = m.ptr(child)?;
                let child_token_fn = m.vslot(child_item, ITEM_VTABLE_GET_TOKEN)?;
                let child_token = m.call(child_token_fn, &[child_item])? as u32;
                if child_token == TOKEN_OWNER && m.u8(child_item + ITEM_FAKE_OWNER)? == 0 {
                    let date = m.i32(child_item + ITEM_DATE)?;
                    let owner = m.u64(child_item + ITEM_OWNER_TAG)?;
                    out.push(RawProvinceOwner { date, prov_index: prov_idx as u32, owner });
                }
                child = m.ptr(child + NODE_NEXT)?;
            }
            node = m.ptr(node + NODE_NEXT)?;
        }
    }
    Ok(out)
}

fn collect_country_history(m: &Memory, world: usize) -> Result<Vec<RawCountry>> {
    let begin = m.ptr(world + WORLD_COUNTRIES_BEGIN)?;
    let end = m.ptr(world + WORLD_COUNTRIES_END)?;
    let count = end.saturating_sub(begin) / 8;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let country = m.ptr(begin + i * 8)?;
        let final_tag = m.u64(country + country::TAG)?;
        let mut items = Vec::new();
        let mut node = m.ptr(country + COUNTRY_HISTORY_HEAD)?;
        while node != 0 {
            let top = m.ptr(node)?;
            read_country_item(m, top, UNDATED_DATE, false, &mut items)?;
            let mut child = m.ptr(top + ITEM_CHILDREN_HEAD)?;
            while child != 0 {
                let child_item = m.ptr(child)?;
                let date = m.i32(child_item + ITEM_DATE)?;
                read_country_item(m, child_item, date, true, &mut items)?;
                child = m.ptr(child + NODE_NEXT)?;
            }
            node = m.ptr(node + NODE_NEXT)?;
        }
        out.push(RawCountry { final_tag, items });
    }
    Ok(out)
}

fn read_country_item(m: &Memory, item: usize, date: i32, is_dated_child: bool, out: &mut Vec<RawCountryItem>) -> Result<()> {
    let get_token = m.vslot(item, ITEM_VTABLE_GET_TOKEN)?;
    let token = m.call(get_token, &[item])? as u32;
    let kind = match token {
        TOKEN_CAPITAL => RawCountryKind::Capital(m.i32(item + ITEM_CAPITAL_PROVINCE)?),
        TOKEN_COUNTRY_NAME => RawCountryKind::Name(read_c_game_string(m, item + ITEM_STRING)?),
        TOKEN_COUNTRY_ADJECTIVE => RawCountryKind::Adjective(read_c_game_string(m, item + ITEM_STRING)?),
        TOKEN_COUNTRY_MAPCOLOR => {
            let meta = m.u32(item + ITEM_COLOR_META)?;
            let mut rgba = [0u8; 16];
            m.read_into(item + ITEM_COLOR_RGBA, &mut rgba)?;
            let is_null = m.u8(item + ITEM_COLOR_IS_NULL)? != 0;
            RawCountryKind::MapColor(RawColor { meta, rgba, is_null })
        }
        TOKEN_CHANGED_TAG_FROM if is_dated_child => RawCountryKind::ChangedTagFrom(m.u64(item + ITEM_CHANGED_TAG)?),
        _ => return Ok(()),
    };
    out.push(RawCountryItem { date, is_dated_child, kind });
    Ok(())
}

fn read_c_game_string(m: &Memory, ptr: usize) -> Result<Vec<u8>> {
    let bytes = game_string::read(m, ptr)?;
    Ok(bytes)
}

// --- Pure-Rust swap and event order reconstruction ---

fn is_active_swap_item(items: &[RawCountryItem], idx: usize, start_date: i32) -> Option<u64> {
    let item = &items[idx];
    let RawCountryKind::ChangedTagFrom(from_tag) = item.kind else {
        return None;
    };
    if !item.is_dated_child || !is_tag_valid(from_tag) {
        return None;
    }
    if item.date > start_date {
        return Some(from_tag);
    }
    if item.date == start_date && idx > 0 {
        let prev = &items[idx - 1];
        if prev.is_dated_child && prev.date == start_date && matches!(prev.kind, RawCountryKind::MapColor(_)) {
            return Some(from_tag);
        }
    }
    None
}

fn pick_reverse_swap_index(pending: &[ActiveSwap], current_tags: &[u64]) -> usize {
    let mut best = 0usize;
    let date = pending[0].date;
    let mut same_day_end = 1usize;
    while same_day_end < pending.len() && pending[same_day_end].date == date {
        same_day_end += 1;
    }
    if same_day_end == 1 {
        return 0;
    }
    for i in 0..same_day_end {
        let cand = pending[i];
        // On a single object, later item index always happened later.
        let blocked_by_same_obj = pending[..same_day_end]
            .iter()
            .enumerate()
            .any(|(j, other)| j != i && other.active_obj == cand.active_obj && other.active_item_idx > cand.active_item_idx);
        if blocked_by_same_obj {
            continue;
        }
        let cand_to = current_tags[cand.active_obj];
        // Prefer a swap whose victim holds `from_tag` without another same-day swap
        // chained after it whose `to_tag` equals `cand.from_tag`.
        let blocks_chained =
            pending[..same_day_end].iter().enumerate().any(|(j, other)| j != i && current_tags[other.active_obj] == cand.from_tag && cand_to != other.from_tag);
        if !blocks_chained {
            return i;
        }
        best = i;
    }
    best
}

fn reconstruct_events(prov_events: &[RawProvinceOwner], countries: &[RawCountry], start_date: i32) -> Vec<RawEvent> {
    let n = countries.len();
    let mut current_tags: Vec<u64> = countries.iter().map(|c| c.final_tag).collect();

    let mut active_swaps = Vec::new();
    for (obj_id, country) in countries.iter().enumerate() {
        for (item_idx, _) in country.items.iter().enumerate() {
            if let Some(from_tag) = is_active_swap_item(&country.items, item_idx, start_date) {
                active_swaps.push(ActiveSwap { date: country.items[item_idx].date, active_obj: obj_id, active_item_idx: item_idx, from_tag });
            }
        }
    }

    // Sort descending by `(date, active_item_idx)` so `pick_reverse_swap_index` pops the
    // chronologically latest remaining swap cleanly.
    active_swaps.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.active_item_idx.cmp(&a.active_item_idx)));

    let mut resolved_desc = Vec::with_capacity(active_swaps.len());
    let mut pending = active_swaps;
    while !pending.is_empty() {
        let pick = pick_reverse_swap_index(&pending, &current_tags);
        let sw = pending.remove(pick);
        let to_tag = current_tags[sw.active_obj];
        let victim_obj = current_tags.iter().enumerate().find_map(|(idx, &tag)| (idx != sw.active_obj && tag == sw.from_tag).then_some(idx));

        if let Some(victim) = victim_obj {
            current_tags[sw.active_obj] = sw.from_tag;
            current_tags[victim] = to_tag;
            resolved_desc.push(ResolvedSwap {
                date: sw.date,
                active_obj: sw.active_obj,
                active_item_idx: sw.active_item_idx,
                victim_obj: victim,
                from_tag: sw.from_tag,
                to_tag,
            });
        }
    }

    let initial_tags = current_tags;
    let resolved_asc: Vec<ResolvedSwap> = resolved_desc.into_iter().rev().collect();

    let mut active_to_tag: HashMap<(usize, usize), u64> = HashMap::with_capacity(resolved_asc.len());
    for sw in &resolved_asc {
        active_to_tag.insert((sw.active_obj, sw.active_item_idx), sw.to_tag);
    }

    // Build per-object chronological tag transitions for passive swap victims.
    let mut passive_transitions: Vec<Vec<TagTransition>> = vec![Vec::new(); n];
    let mut used_passive_items: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for sw in &resolved_asc {
        let victim_items = &countries[sw.victim_obj].items;
        let passive_anchor = victim_items
            .iter()
            .enumerate()
            .find_map(|(idx, item)| {
                (item.is_dated_child
                    && item.date == sw.date
                    && matches!(item.kind, RawCountryKind::MapColor(_))
                    && !used_passive_items[sw.victim_obj].contains(&idx)
                    && !active_to_tag.contains_key(&(sw.victim_obj, idx + 1)))
                .then_some(idx)
            })
            .inspect(|&idx| {
                used_passive_items[sw.victim_obj].insert(idx);
            })
            .unwrap_or_else(|| {
                victim_items
                    .iter()
                    .enumerate()
                    .position(|(idx, it)| it.date >= sw.date && !active_to_tag.contains_key(&(sw.victim_obj, idx)))
                    .unwrap_or(victim_items.len())
            });

        passive_transitions[sw.victim_obj].push(TagTransition { date: sw.date, item_idx: passive_anchor, tag_after: sw.from_tag });
    }

    for list in &mut passive_transitions {
        list.sort_by_key(|t| (t.item_idx, t.date));
    }

    let mut events = Vec::with_capacity(prov_events.len() + countries.iter().map(|c| c.items.len()).sum::<usize>());
    let mut seq = 0usize;

    for pev in prov_events {
        events.push(RawEvent { date: pev.date, order_group: 0, seq, kind: RawEventKind::ProvinceOwner { prov_index: pev.prov_index, owner: pev.owner } });
        seq += 1;
    }

    for (obj_id, country) in countries.iter().enumerate() {
        let mut active_tag = initial_tags[obj_id];
        let passive_list = &passive_transitions[obj_id];
        let mut passive_cursor = 0usize;

        let len = country.items.len();
        let mut i = 0usize;
        while i < len {
            while passive_cursor < passive_list.len() && passive_list[passive_cursor].item_idx <= i {
                active_tag = passive_list[passive_cursor].tag_after;
                passive_cursor += 1;
            }

            let item = &country.items[i];
            // Pair `SetCountryColors` (`0x76491b`) immediately preceding `changed_tag_from` on the
            // same date block so the new tag receives its own formation color cleanly.
            if i + 1 < len && country.items[i + 1].date == item.date {
                if let Some(&to_tag) = active_to_tag.get(&(obj_id, i + 1)) {
                    if let RawCountryKind::MapColor(color) = &item.kind {
                        let from_tag = match country.items[i + 1].kind {
                            RawCountryKind::ChangedTagFrom(f) => f,
                            _ => active_tag,
                        };
                        events.push(RawEvent { date: item.date, order_group: 1, seq, kind: RawEventKind::ChangeTag { to_tag, from_tag } });
                        seq += 1;
                        events.push(RawEvent {
                            date: item.date,
                            order_group: 1,
                            seq,
                            kind: RawEventKind::CountryMapColor { tag: to_tag, color: color.clone() },
                        });
                        seq += 1;
                        active_tag = to_tag;
                        i += 2;
                        continue;
                    }
                }
            }

            match &item.kind {
                RawCountryKind::Capital(prov_id) => {
                    events.push(RawEvent { date: item.date, order_group: 1, seq, kind: RawEventKind::Capital { tag: active_tag, prov_id: *prov_id } });
                    seq += 1;
                }
                RawCountryKind::Name(bytes) => {
                    events.push(RawEvent { date: item.date, order_group: 1, seq, kind: RawEventKind::CountryName { tag: active_tag, bytes: bytes.clone() } });
                    seq += 1;
                }
                RawCountryKind::Adjective(bytes) => {
                    events.push(RawEvent {
                        date: item.date,
                        order_group: 1,
                        seq,
                        kind: RawEventKind::CountryAdjective { tag: active_tag, bytes: bytes.clone() },
                    });
                    seq += 1;
                }
                RawCountryKind::MapColor(color) => {
                    events.push(RawEvent {
                        date: item.date,
                        order_group: 1,
                        seq,
                        kind: RawEventKind::CountryMapColor { tag: active_tag, color: color.clone() },
                    });
                    seq += 1;
                }
                RawCountryKind::ChangedTagFrom(from_tag) => {
                    if let Some(&to_tag) = active_to_tag.get(&(obj_id, i)) {
                        events.push(RawEvent { date: item.date, order_group: 1, seq, kind: RawEventKind::ChangeTag { to_tag, from_tag: *from_tag } });
                        seq += 1;
                        active_tag = to_tag;
                    } else if is_tag_valid(*from_tag) {
                        events.push(RawEvent {
                            date: item.date,
                            order_group: 1,
                            seq,
                            kind: RawEventKind::ChangeTag { to_tag: active_tag, from_tag: *from_tag },
                        });
                        seq += 1;
                    }
                }
            }
            i += 1;
        }
    }

    events.sort_by_key(|ev| (ev.date, ev.order_group, ev.seq));
    events
}

// --- Native event allocation ---

fn alloc_event(m: &Memory, size: usize, name: &str, init: impl FnOnce(usize) -> Result<()>) -> Result<usize> {
    let ptr = game_heap::new(m, size)?;
    if ptr == 0 {
        return Err(format!("null {name} allocation"));
    }
    if let Err(err) = init(ptr) {
        let _ = delete(m, ptr, size);
        return Err(err);
    }
    Ok(ptr)
}

fn alloc_native_event(m: &Memory, ev: &RawEvent) -> Result<usize> {
    match &ev.kind {
        RawEventKind::ProvinceOwner { prov_index, owner } => alloc_event(m, 0x20, "CProvinceOwnerTimelineEvent", |ptr| {
            write_usize(m, ptr, m.at(VTABLE_PROVINCE_OWNER))?;
            write_i32(m, ptr + 0x08, ev.date)?;
            write_u32(m, ptr + 0x0c, 0)?;
            write_u32(m, ptr + 0x10, *prov_index)?;
            write_u64(m, ptr + 0x14, *owner)?;
            write_u32(m, ptr + 0x1c, 0)
        }),
        RawEventKind::ChangeTag { to_tag, from_tag } => alloc_event(m, 0x20, "CChangeTagTimelineEvent", |ptr| {
            write_usize(m, ptr, m.at(VTABLE_CHANGE_TAG))?;
            write_i32(m, ptr + 0x08, ev.date)?;
            write_u32(m, ptr + 0x0c, 0)?;
            write_u64(m, ptr + 0x10, *to_tag)?;
            write_u64(m, ptr + 0x18, *from_tag)
        }),
        RawEventKind::Capital { tag, prov_id } => alloc_event(m, 0x20, "CCapitalTimelineEvent", |ptr| {
            write_usize(m, ptr, m.at(VTABLE_CAPITAL))?;
            write_i32(m, ptr + 0x08, ev.date)?;
            write_u32(m, ptr + 0x0c, 0)?;
            write_u64(m, ptr + 0x10, *tag)?;
            write_i32(m, ptr + 0x18, *prov_id)?;
            write_u32(m, ptr + 0x1c, 0)
        }),
        RawEventKind::CountryName { tag, bytes } => alloc_tag_string_event(m, VTABLE_COUNTRY_NAME, ev.date, *tag, bytes),
        RawEventKind::CountryAdjective { tag, bytes } => alloc_tag_string_event(m, VTABLE_COUNTRY_ADJECTIVE, ev.date, *tag, bytes),
        RawEventKind::CountryMapColor { tag, color } => alloc_event(m, 0x40, "CCountryMapColorTimelineEvent", |ptr| {
            write_usize(m, ptr, m.at(VTABLE_COUNTRY_MAPCOLOR))?;
            write_i32(m, ptr + 0x08, ev.date)?;
            write_u32(m, ptr + 0x0c, 0)?;
            write_u64(m, ptr + 0x10, *tag)?;
            write_usize(m, ptr + 0x18, m.at(VTABLE_COLOR))?;
            write_u32(m, ptr + 0x20, color.meta)?;
            write_u32(m, ptr + 0x24, 0)?;
            m.write(ptr + 0x28, &color.rgba)?;
            m.write(ptr + 0x38, &[u8::from(color.is_null), 0, 0, 0, 0, 0, 0, 0])
        }),
    }
}

fn alloc_tag_string_event(m: &Memory, vtable_rva: usize, date: i32, tag: u64, bytes: &[u8]) -> Result<usize> {
    let ptr = game_heap::new(m, 0x38)?;
    if ptr == 0 {
        return Err("null string timeline event allocation".into());
    }
    let write_res = (|| -> Result<()> {
        write_usize(m, ptr, m.at(vtable_rva))?;
        write_i32(m, ptr + 0x08, date)?;
        write_u32(m, ptr + 0x0c, 0)?;
        write_u64(m, ptr + 0x10, tag)?;
        write_usizes(m, ptr + 0x18, &game_string::empty())?;
        if !bytes.is_empty() {
            game_string::append_checked(m, ptr + 0x18, bytes)?;
        }
        Ok(())
    })();
    if let Err(err) = write_res {
        let _ = game_string::destroy_checked(m, ptr + 0x18);
        let _ = delete(m, ptr, 0x38);
        return Err(err);
    }
    Ok(ptr)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn make_tag(chars: [u8; 3], index: i16) -> u64 {
        (chars[0] as u64) | ((chars[1] as u64) << 8) | ((chars[2] as u64) << 16) | (((index as u16) as u64) << tag::INDEX) | (6u64 << tag::VALID)
    }

    const BUR: u64 = make_tag(*b"BUR", 10);
    const FRA: u64 = make_tag(*b"FRA", 20);
    const ROM: u64 = make_tag(*b"ROM", 30);
    const LOT: u64 = make_tag(*b"LOT", 40);
    const NAV: u64 = make_tag(*b"NAV", 50);

    const D_BASE: i32 = UNDATED_DATE + 1_000_000;

    fn color(tag_byte: u8) -> RawColor {
        RawColor { meta: 1, rgba: [tag_byte; 16], is_null: false }
    }

    fn item(date: i32, kind: RawCountryKind) -> RawCountryItem {
        RawCountryItem { date, is_dated_child: date != UNDATED_DATE, kind }
    }

    #[test]
    fn same_day_conquest_then_formation_orders_province_before_change_tag() {
        let start = D_BASE;
        let d = D_BASE + 1000;
        let provs = vec![RawProvinceOwner { date: d, prov_index: 183, owner: BUR }];
        let countries = vec![
            RawCountry {
                final_tag: FRA,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(192)),
                    item(d, RawCountryKind::MapColor(color(0xF1))),
                    item(d, RawCountryKind::ChangedTagFrom(BUR)),
                ],
            },
            RawCountry { final_tag: BUR, items: vec![item(UNDATED_DATE, RawCountryKind::Capital(183)), item(d, RawCountryKind::MapColor(color(0xB1)))] },
        ];

        let events = reconstruct_events(&provs, &countries, start);
        assert_eq!(
            events.iter().map(|e| (&e.date, &e.kind)).collect::<Vec<_>>(),
            vec![
                (&UNDATED_DATE, &RawEventKind::Capital { tag: BUR, prov_id: 192 }),
                (&UNDATED_DATE, &RawEventKind::Capital { tag: FRA, prov_id: 183 }),
                (&d, &RawEventKind::ProvinceOwner { prov_index: 183, owner: BUR }),
                (&d, &RawEventKind::ChangeTag { to_tag: FRA, from_tag: BUR }),
                (&d, &RawEventKind::CountryMapColor { tag: FRA, color: color(0xF1) }),
                (&d, &RawEventKind::CountryMapColor { tag: BUR, color: color(0xB1) }),
            ]
        );
    }

    #[test]
    fn same_province_double_transfer_keeps_forward_order() {
        let provs =
            vec![RawProvinceOwner { date: D_BASE + 1000, prov_index: 5, owner: BUR }, RawProvinceOwner { date: D_BASE + 1000, prov_index: 5, owner: FRA }];
        let events = reconstruct_events(&provs, &[], D_BASE);
        assert_eq!(
            events.into_iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![RawEventKind::ProvinceOwner { prov_index: 5, owner: BUR }, RawEventKind::ProvinceOwner { prov_index: 5, owner: FRA },]
        );
    }

    #[test]
    fn chained_formations_and_passive_reuse_reconstruct_all_histories() {
        // 1444: O0 = BUR, O1 = FRA, O2 = ROM
        // d1: BUR (O0) forms FRA (O1) -> O0=FRA, O1=BUR
        // d2: FRA (O0) forms ROM (O2) -> O0=ROM, O2=FRA
        // d3: BUR (O1) is released & forms LOT (O3) -> O1=LOT, O3=BUR
        let start = D_BASE;
        let (d1, d2, d3) = (D_BASE + 1000, D_BASE + 2000, D_BASE + 3000);
        let countries = vec![
            RawCountry {
                final_tag: ROM,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(192)),
                    item(D_BASE + 500, RawCountryKind::Capital(193)),
                    item(d1, RawCountryKind::MapColor(color(0xF1))),
                    item(d1, RawCountryKind::ChangedTagFrom(BUR)),
                    item(D_BASE + 1500, RawCountryKind::Name(b"France".to_vec())),
                    item(d2, RawCountryKind::MapColor(color(0xA1))),
                    item(d2, RawCountryKind::ChangedTagFrom(FRA)),
                    item(D_BASE + 2500, RawCountryKind::Capital(118)),
                ],
            },
            RawCountry {
                final_tag: LOT,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(183)),
                    item(d1, RawCountryKind::MapColor(color(0xB1))),
                    item(D_BASE + 2200, RawCountryKind::Capital(192)),
                    item(d3, RawCountryKind::MapColor(color(0xC1))),
                    item(d3, RawCountryKind::ChangedTagFrom(BUR)),
                ],
            },
            RawCountry { final_tag: FRA, items: vec![item(UNDATED_DATE, RawCountryKind::Capital(118)), item(d2, RawCountryKind::MapColor(color(0xF2)))] },
            RawCountry { final_tag: BUR, items: vec![item(UNDATED_DATE, RawCountryKind::Capital(195)), item(d3, RawCountryKind::MapColor(color(0xB2)))] },
        ];

        let events = reconstruct_events(&[], &countries, start);
        let summary: Vec<(i32, &RawEventKind)> = events.iter().map(|e| (e.date, &e.kind)).collect();
        assert!(summary.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: BUR, prov_id: 192 })));
        assert!(summary.contains(&(D_BASE + 500, &RawEventKind::Capital { tag: BUR, prov_id: 193 })));
        assert!(summary.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: FRA, prov_id: 183 })));
        assert!(summary.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: ROM, prov_id: 118 })));
        assert!(summary.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: LOT, prov_id: 195 })));
        assert!(summary.contains(&(D_BASE + 1500, &RawEventKind::CountryName { tag: FRA, bytes: b"France".to_vec() })));
        assert!(summary.contains(&(D_BASE + 2200, &RawEventKind::Capital { tag: BUR, prov_id: 192 })));
        assert!(summary.contains(&(D_BASE + 2500, &RawEventKind::Capital { tag: ROM, prov_id: 118 })));
        assert!(summary.contains(&(d1, &RawEventKind::CountryMapColor { tag: FRA, color: color(0xF1) })));
        assert!(summary.contains(&(d1, &RawEventKind::CountryMapColor { tag: BUR, color: color(0xB1) })));
        assert!(summary.contains(&(d2, &RawEventKind::CountryMapColor { tag: ROM, color: color(0xA1) })));
        assert!(summary.contains(&(d2, &RawEventKind::CountryMapColor { tag: FRA, color: color(0xF2) })));
        assert!(summary.contains(&(d3, &RawEventKind::CountryMapColor { tag: LOT, color: color(0xC1) })));
        assert!(summary.contains(&(d3, &RawEventKind::CountryMapColor { tag: BUR, color: color(0xB2) })));
    }

    #[test]
    fn later_passive_swap_on_former_tag_preserves_prior_swap_to_tag() {
        // d1: BUR (O0) -> FRA (O1), so O0 = FRA, O1 = BUR
        // d2: NAV (O2) -> FRA (O0), so O2 = FRA, O0 = NAV
        let start = D_BASE;
        let (d1, d2) = (D_BASE + 1000, D_BASE + 2000);
        let countries = vec![
            RawCountry {
                final_tag: NAV,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(192)),
                    item(d1, RawCountryKind::MapColor(color(0xF1))),
                    item(d1, RawCountryKind::ChangedTagFrom(BUR)),
                    item(D_BASE + 1500, RawCountryKind::Capital(183)),
                    item(d2, RawCountryKind::MapColor(color(0xD1))),
                ],
            },
            RawCountry { final_tag: BUR, items: vec![item(d1, RawCountryKind::MapColor(color(0xB1)))] },
            RawCountry {
                final_tag: FRA,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(210)),
                    item(d2, RawCountryKind::MapColor(color(0xF2))),
                    item(d2, RawCountryKind::ChangedTagFrom(NAV)),
                ],
            },
        ];

        let events = reconstruct_events(&[], &countries, start);
        let kinds: Vec<(i32, &RawEventKind)> = events.iter().map(|e| (e.date, &e.kind)).collect();
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: BUR, prov_id: 192 })));
        assert!(kinds.contains(&(d1, &RawEventKind::ChangeTag { to_tag: FRA, from_tag: BUR })));
        assert!(kinds.contains(&(D_BASE + 1500, &RawEventKind::Capital { tag: FRA, prov_id: 183 })));
        assert!(kinds.contains(&(d2, &RawEventKind::ChangeTag { to_tag: FRA, from_tag: NAV })));
        assert!(kinds.contains(&(d2, &RawEventKind::CountryMapColor { tag: NAV, color: color(0xD1) })));
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: NAV, prov_id: 210 })));
    }

    #[test]
    fn round_trip_tag_formation_preserves_both_eras() {
        let start = D_BASE;
        let countries = vec![
            RawCountry {
                final_tag: BUR,
                items: vec![
                    item(D_BASE + 500, RawCountryKind::Capital(192)),
                    item(D_BASE + 1000, RawCountryKind::MapColor(color(0xF1))),
                    item(D_BASE + 1000, RawCountryKind::ChangedTagFrom(BUR)),
                    item(D_BASE + 1500, RawCountryKind::Capital(183)),
                    item(D_BASE + 2000, RawCountryKind::MapColor(color(0xB1))),
                    item(D_BASE + 2000, RawCountryKind::ChangedTagFrom(FRA)),
                    item(D_BASE + 2500, RawCountryKind::Capital(195)),
                ],
            },
            RawCountry {
                final_tag: FRA,
                items: vec![item(D_BASE + 1000, RawCountryKind::MapColor(color(0xB0))), item(D_BASE + 2000, RawCountryKind::MapColor(color(0xF0)))],
            },
        ];

        let events = reconstruct_events(&[], &countries, start);
        let kinds: Vec<(i32, &RawEventKind)> = events.iter().map(|e| (e.date, &e.kind)).collect();
        assert!(kinds.contains(&(D_BASE + 500, &RawEventKind::Capital { tag: BUR, prov_id: 192 })));
        assert!(kinds.contains(&(D_BASE + 1000, &RawEventKind::ChangeTag { to_tag: FRA, from_tag: BUR })));
        assert!(kinds.contains(&(D_BASE + 1500, &RawEventKind::Capital { tag: FRA, prov_id: 183 })));
        assert!(kinds.contains(&(D_BASE + 2000, &RawEventKind::ChangeTag { to_tag: BUR, from_tag: FRA })));
        assert!(kinds.contains(&(D_BASE + 2500, &RawEventKind::Capital { tag: BUR, prov_id: 195 })));
    }

    #[test]
    fn same_day_overlapping_swaps_across_countries_unswap_cleanly() {
        // Same day `d`: BUR (O0) -> FRA (O1), then NAV (O2) -> BUR (O1).
        let start = D_BASE;
        let d = D_BASE + 1000;
        let countries = vec![
            RawCountry {
                final_tag: FRA,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(192)),
                    item(d, RawCountryKind::MapColor(color(0xF1))),
                    item(d, RawCountryKind::ChangedTagFrom(BUR)),
                ],
            },
            RawCountry {
                final_tag: NAV,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(183)),
                    item(d, RawCountryKind::MapColor(color(0xB1))),
                    item(d, RawCountryKind::MapColor(color(0xD1))),
                ],
            },
            RawCountry {
                final_tag: BUR,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(210)),
                    item(d, RawCountryKind::MapColor(color(0xB2))),
                    item(d, RawCountryKind::ChangedTagFrom(NAV)),
                ],
            },
        ];

        let events = reconstruct_events(&[], &countries, start);
        let kinds: Vec<(i32, &RawEventKind)> = events.iter().map(|e| (e.date, &e.kind)).collect();
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: BUR, prov_id: 192 })));
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: FRA, prov_id: 183 })));
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: NAV, prov_id: 210 })));
        assert!(kinds.contains(&(d, &RawEventKind::CountryMapColor { tag: BUR, color: color(0xB1) })));
        assert!(kinds.contains(&(d, &RawEventKind::CountryMapColor { tag: NAV, color: color(0xD1) })));
    }

    #[test]
    fn pre_game_disk_changed_tag_from_is_not_treated_as_runtime_swap() {
        let start = D_BASE + 1000;
        let countries = vec![
            RawCountry {
                final_tag: ROM,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(118)),
                    item(D_BASE + 500, RawCountryKind::ChangedTagFrom(FRA)),
                    item(start, RawCountryKind::ChangedTagFrom(BUR)),
                ],
            },
            RawCountry { final_tag: FRA, items: vec![item(UNDATED_DATE, RawCountryKind::Capital(183))] },
            RawCountry { final_tag: BUR, items: vec![item(UNDATED_DATE, RawCountryKind::Capital(192))] },
        ];

        let events = reconstruct_events(&[], &countries, start);
        let kinds: Vec<(i32, &RawEventKind)> = events.iter().map(|e| (e.date, &e.kind)).collect();
        // Initial tags were NOT un-swapped because neither entry is a runtime swap.
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: ROM, prov_id: 118 })));
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: FRA, prov_id: 183 })));
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: BUR, prov_id: 192 })));
    }

    #[test]
    fn day_zero_formation_with_preceding_mapcolor_is_recognized_as_runtime_swap() {
        let start = D_BASE;
        let countries = vec![
            RawCountry {
                final_tag: FRA,
                items: vec![
                    item(UNDATED_DATE, RawCountryKind::Capital(192)),
                    item(start, RawCountryKind::MapColor(color(0xF1))),
                    item(start, RawCountryKind::ChangedTagFrom(BUR)),
                ],
            },
            RawCountry { final_tag: BUR, items: vec![item(UNDATED_DATE, RawCountryKind::Capital(183)), item(start, RawCountryKind::MapColor(color(0xB1)))] },
        ];

        let events = reconstruct_events(&[], &countries, start);
        let kinds: Vec<(i32, &RawEventKind)> = events.iter().map(|e| (e.date, &e.kind)).collect();
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: BUR, prov_id: 192 })));
        assert!(kinds.contains(&(UNDATED_DATE, &RawEventKind::Capital { tag: FRA, prov_id: 183 })));
        assert!(kinds.contains(&(start, &RawEventKind::ChangeTag { to_tag: FRA, from_tag: BUR })));
        assert!(kinds.contains(&(start, &RawEventKind::CountryMapColor { tag: FRA, color: color(0xF1) })));
        assert!(kinds.contains(&(start, &RawEventKind::CountryMapColor { tag: BUR, color: color(0xB1) })));
    }

    #[test]
    fn invalid_from_tag_is_ignored_without_panicking() {
        let invalid_tag = 0x2d2d2du64; // valid bit = 0
        let countries = vec![RawCountry {
            final_tag: FRA,
            items: vec![item(D_BASE + 1000, RawCountryKind::ChangedTagFrom(invalid_tag)), item(D_BASE + 1100, RawCountryKind::Capital(183))],
        }];
        let events = reconstruct_events(&[], &countries, D_BASE);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, RawEventKind::Capital { tag: FRA, prov_id: 183 });
    }

    #[test]
    fn same_day_capital_before_and_after_change_tag_preserves_order() {
        let start = D_BASE;
        let d = D_BASE + 1000;
        let countries = vec![
            RawCountry {
                final_tag: FRA,
                items: vec![
                    item(d, RawCountryKind::Capital(192)),
                    item(d, RawCountryKind::MapColor(color(0xF1))),
                    item(d, RawCountryKind::ChangedTagFrom(BUR)),
                    item(d, RawCountryKind::Capital(183)),
                    item(d, RawCountryKind::Name(b"Kingdom of France".to_vec())),
                ],
            },
            RawCountry { final_tag: BUR, items: vec![item(d, RawCountryKind::MapColor(color(0xB1)))] },
        ];

        let events = reconstruct_events(&[], &countries, start);
        assert_eq!(
            events.into_iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec![
                RawEventKind::Capital { tag: BUR, prov_id: 192 },
                RawEventKind::ChangeTag { to_tag: FRA, from_tag: BUR },
                RawEventKind::CountryMapColor { tag: FRA, color: color(0xF1) },
                RawEventKind::Capital { tag: FRA, prov_id: 183 },
                RawEventKind::CountryName { tag: FRA, bytes: b"Kingdom of France".to_vec() },
                RawEventKind::CountryMapColor { tag: BUR, color: color(0xB1) },
            ]
        );
    }

    #[test]
    fn collect_name_event_dates_filters_sorts_and_dedups() {
        let events = vec![
            RawEvent { date: D_BASE + 50, order_group: 0, seq: 0, kind: RawEventKind::ProvinceOwner { prov_index: 183, owner: BUR } },
            RawEvent { date: D_BASE + 300, order_group: 1, seq: 1, kind: RawEventKind::CountryAdjective { tag: FRA, bytes: b"French".to_vec() } },
            RawEvent { date: D_BASE + 100, order_group: 1, seq: 2, kind: RawEventKind::Capital { tag: FRA, prov_id: 183 } },
            RawEvent { date: D_BASE + 200, order_group: 1, seq: 3, kind: RawEventKind::ChangeTag { to_tag: FRA, from_tag: BUR } },
            RawEvent { date: D_BASE + 200, order_group: 1, seq: 4, kind: RawEventKind::CountryName { tag: FRA, bytes: b"France".to_vec() } },
            RawEvent { date: D_BASE + 250, order_group: 1, seq: 5, kind: RawEventKind::CountryMapColor { tag: FRA, color: color(0xF1) } },
            RawEvent { date: D_BASE + 120, order_group: 1, seq: 6, kind: RawEventKind::CountryName { tag: BUR, bytes: b"Burgundy".to_vec() } },
        ];
        assert_eq!(collect_name_event_dates(&events), vec![D_BASE + 120, D_BASE + 200, D_BASE + 300]);
    }

    #[test]
    fn has_name_event_in_range_uses_half_open_interval() {
        let dates = [100, 200, 300];
        assert!(!has_name_event_in_range(&dates, 100, 100));
        assert!(!has_name_event_in_range(&dates, 200, 150));
        assert!(!has_name_event_in_range(&dates, 100, 199));
        assert!(has_name_event_in_range(&dates, 100, 200));
        assert!(has_name_event_in_range(&dates, 100, 250));
        assert!(!has_name_event_in_range(&dates, 200, 299));
        assert!(has_name_event_in_range(&dates, 200, 300));
        assert!(!has_name_event_in_range(&dates, 300, 400));
    }

    #[test]
    fn should_refresh_names_policy_triggers_on_name_event_pause_end_and_interval() {
        assert!(should_refresh_names_policy(12.0, 0.0, 24.0, 200, 500, true));
        assert!(should_refresh_names_policy(0.0, 0.0, 24.0, 200, 500, false));
        assert!(should_refresh_names_policy(80.0, 0.0, 24.0, 500, 500, false));

        for speed in [6.0f32, 12.0, 30.0, 80.0] {
            assert!(!should_refresh_names_policy(speed, 0.24, 24.0, 200, 500, false));
            assert!(should_refresh_names_policy(speed, 0.25, 24.0, 200, 500, false));
        }

        // Fast extreme where native `24.0 / speed` (`0.10 s`) is shorter than `0.25 s`.
        assert!(!should_refresh_names_policy(240.0, 0.09, 24.0, 200, 500, false));
        assert!(should_refresh_names_policy(240.0, 0.10, 24.0, 200, 500, false));
    }
}
