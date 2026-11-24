# Timeline playback, history replay and rendering

In-game timeline playback uses map mode `0x25`, driven each idle frame (`0x8153b2`–`0x815497`) by the `CTimeline` singleton at `0x2340118` and built from live `CWorld` (`0x233fe78`) province/country history lists.

## Objects and layout

- **`CTimeline` (`0xa0` bytes at `[0x2340118]`)**:
  - `+0x00`: `CTimelineData* data` (`0` when stopped or unloaded).
  - `+0x08`: `CTimelineData* base_timeline` (static history database replay).
  - `+0x10`: `float speed` (`0.0f` when paused via `CTimelineView::Pause` at `0x1430df0`).
  - `+0x14`: `float accumulator` (advances one month per `1.0f`, `[0x1dc1604]`).
  - `+0x18`: `CTimelineState* active_state`.
  - `+0x20..+0xa0`: embedded default `CTimelineState` returned by `0x13c36a0` when `+0x18` is null.
- **`CTimelineData` (`0x30` bytes, destroyed by `0x13c33f0`)**:
  - `+0x00..+0x18`: `std::vector<CTimelineEvent*> events` (allocated with `0x950b0` / freed with `0x13c40c0`).
  - `+0x18..+0x30`: `std::vector<CTimelineState> snapshots` (element size `0x80`, freed with `0x13c4050`).
- **`CTimelineState` (`0x80` bytes, constructed by `0x13c36b0`)**:
  - `+0x00`: `i32 date` (`0x29c77f8` before seeding).
  - `+0x08..+0x20`: `std::vector<CTag> owners` indexed by `0`-based province array index (`[world + 0x1c90..+0x1c98]` / `[world + 0x1ca8]`).
  - `+0x20..+0x38`: `std::vector<i32> capitals` indexed by tag index.
  - `+0x38..+0x50`: `std::vector<std::string> names` indexed by tag index.
  - `+0x50..+0x68`: `std::vector<std::string> adjectives` indexed by tag index.
  - `+0x68..+0x80`: `std::vector<CTimelineColor> mapcolors` (`0x28` bytes each, `CColor` vtable `0x1c48ca0`) indexed by tag index.
- **`CTimelineEvent` subclasses**:

| Event | VTable | Size | Payload offsets |
| --- | --- | --- | --- |
| `CProvinceOwnerTimelineEvent` | `0x1d809a8` | `0x20` | `+0x08 i32 date`, `+0x10 u32 prov_index`, `+0x14 u64 owner` (unaligned) |
| `CChangeTagTimelineEvent` | `0x1d80988` | `0x20` | `+0x08 i32 date`, `+0x10 u64 to_tag`, `+0x18 u64 from_tag` (`Apply` at `0x13c21b0`) |
| `CCapitalTimelineEvent` | `0x1d809e8` | `0x20` | `+0x08 i32 date`, `+0x10 u64 tag`, `+0x18 i32 prov_id` |
| `CCountryNameTimelineEvent` | `0x1d809d8` | `0x38` | `+0x08 i32 date`, `+0x10 u64 tag`, `+0x18 std::string` |
| `CCountryAdjectiveTimelineEvent` | `0x1d809c8` | `0x38` | `+0x08 i32 date`, `+0x10 u64 tag`, `+0x18 std::string` |
| `CCountryMapColorTimelineEvent` | `0x1d80998` | `0x40` | `+0x08 i32 date`, `+0x10 u64 tag`, `+0x18 vtable 0x1c48ca0`, `+0x20 u32 meta`, `+0x28 [u8; 16] rgba`, `+0x38 u8 is_null` (`Apply` at `0x13c2160`) |

## Lifetime and update loop

- **Starting / restarting**: `CTimeline::Start` (`0x13c3480`) and `CTimelineView::Restart` (`0x1430ac0`) build `CTimelineData` via `0x13c2980`, seed the active state with `CTimelineData::CreateStateAt(&world->start_date)` (`0x13c3160`), and subtract `24` hours from `active_state->date`.
- **Speed buttons (`CTimelineView::Init` at `0x1430120`–`0x143070f`)**:
  - `timeline_pause` (`0x1430df0`): `0.0f` months/sec (`CTimelineView::Pause`).
  - `timeline_slow` (`0x1430e60`): `6.0f` months/sec (`0x1dc17f8`).
  - `timeline_medium` (`0x1430ed0`): `12.0f` months/sec (`0x1dc1820`).
  - `timeline_fast` (`0x1430f40`): `30.0f` months/sec (`0x1dc1860`).
  - `timeline_veryfast` (`0x1430fb0`): `80.0f` months/sec (`0x1dc18b4`).
- **Stopping**: `CTimeline::Stop` (`0x13c3540`) destroys `CTimelineData` (`0x13c33f0`) and the active `CTimelineState` copy.
- **Stepping (`0x13c35c0`)**: `CTimeline::Update(timeline, dt)` multiplies `dt * speed`, adds to `accumulator` (clamped to `100.0f` at `[0x1dc18c8]`), and in a `while (accumulator >= 1.0f)` loop steps `active_state->date` forward by one month (`0x14c8030`, clamped to `world->date`, `world + 0x1dd0`), calling `CTimelineData::ApplyEventsUntil` (`0x13c3030`). `Update` returns `true` (`al = 1`) only if at least one event had `old_date < event->date <= new_date`.
- **Idler frame & map rendering (`0x8153ce`–`0x815497`)**:
  - Accumulates `dt` into static `s_fNameTimer` (`0x245b48c`).
  - When `CTimeline::Update` returns `true` (`0x8153fd`):
    - Calls `0x6ea0b0(idler, 0x25)`, which checks `[idler + 0x2c] == mode` (`0x6ea0b9`) and `[idler + 0x34] == 0`, sets `IDLER_COLORS_DUE` (`+0x34`) = `1`, and writes the deadline `[idler + 0x38]` (`0x6ea127`) to `now + MAX_MAPMODE_COLOR_UPDATE_DELAY_MS * 1_000_000` (`[0x233e84c]`, initialized to `200` ms at `0x4c3b0a`). On a later draw (`0x818619`, and `core/src/drawing/prepare.rs` `first_steps`), `0x10a8c30` (`MAP_RENDERER`, calling `0x12fd9b0` and `0x132c050`, which returns early in mode `0x25`) uploads the 40 KB province color texture once `clock >= [idler + 0x38]`.
    - Calls `0x12dd0c0` to rebuild borders.
    - Sets `map+0x25a0 = 1` (triggering `0x10a9be0` `MAKE_NAMES` on the next draw) and clears `s_fNameTimer` if `speed <= 0`, `s_fNameTimer > 24.0f / speed` (`[0x1dc1848]`), or `timeline_date >= world->date`.

## History storage in saves, `history/`, and `ChangeTag` side effects

- Save files (`gamestate`) and static history files retain exact daily timestamps (`YYYY.M.D`, encoded as `days * 24` hours with no global cross-list sub-day sequence counter). Saves do not store daily full-world snapshots; they keep an event-driven change log containing only dates where something changed, from which `CTimelineData::Build` extracts 6 event types (`owner`, `changed_tag_from`, `capital`, `name`, `adjective`, `changed_country_mapcolor_from`).
- Province history (`province + 0x50`) and country history (`country + 0x11c8`) are intrusive doubly-linked lists `{ head: +0x00, tail: +0x08 }` of nodes `{ item: +0x00, prev: +0x08, next: +0x10 }`. Each `CHistoryEntry` has date (`+0x10`, `0x29c9a30` = `1.1.1` when undated), child list `{ head: +0x20, tail: +0x28 }`, and `vtable+0x80` token (`0x2841` owner, `0x284e` capital, `0x2d1f` `changed_tag_from`, `0x3423` name, `0x3424` mapcolor, `0x3425` adjective).
- When `ChangeTag` executes (`0x76416c`):
  1. Swaps the two `CCountry*` entries in `world->countries` (`+0x1ce8..+0x1cf0`) and the tag table, so the forming country keeps its old object pointer (with pre-formation history) while receiving the target tag, and the formed tag's old `CCountry*` inherits the old tag.
  2. Writes `SetCountryColors` (`0x375650`): `0x76491b` appends `changed_country_mapcolor_from` with the new tag's colors onto the forming object (`r12`), and `0x76492a` appends `changed_country_mapcolor_from` with the old tag's colors onto the swapped-out object (`rdi`).
  3. At `0x76625c`, appends `changed_tag_from = old_tag` onto `r12` after `0x76491b`'s `changed_country_mapcolor_from`.

## Confirmed native bugs repaired in Harmony

`core/src/repairs/timeline.rs` repairs six vanilla defects:

1. **Same-day event inversion (`0x13c2980`)**: Vanilla builds events by pushing province owner events forward, pushing country events backward (`0x13c2220` via `tail -> prev`), calling `std::stable_sort` with descending `a->date > b->date` (`0x13c1f60`), and calling `std::reverse`. Same-day `ChangeTag` therefore executes before same-day `ProvinceOwner(owner = old_tag)`, reverting provinces conquered on formation day back to the old tag.
2. **Formation-day map color mis-attribution (`0x13c2220`)**: Because `0x13c2220` walks children backward (`0x13c23f7`: `+0x28` tail -> `+0x08` prev), it sees `changed_tag_from = old_tag` first, switches its active tag variable `r14` to `old_tag`, and then gives `0x76491b`'s preceding `changed_country_mapcolor_from` (`new_tag`'s color) to `old_tag`.
3. **Swapped-out dead-tag history & undated top-level items (`0x76416c`, `0x13c2220`)**: Neither the pre-swap history left on the swapped-out `CCountry*` nor undated top-level entries on either object are resolved away from `country->tag` (its final tag), polluting `old_tag` in the `1444` snapshot when forming a previously existing tag.
4. **Day-0 live-game events skipped (`0x13c2ff5`–`0x13c3011`)**: `Build` applies `base_timeline` up to `start_date` (`snap->date == start_date`) and then calls `ApplyEventsUntil(this, snap, start_date + 24)`, whose binary search (`0x13c30f0`) skips every event with `date <= snap->date` — dropping day-0 campaign events at `start_date`. The replacement rewinds `snap->date` to `start_date - 24`, applies live events up to `start_date`, and then stamps `snap->date = start_date + 24` to cancel `Start`/`Restart`'s `-24` without pre-applying day-2 (`start_date + 24`) events.
5. **Pause and end-of-timeline name refresh missed (`0x8153fd`–`0x815497`)**: The `speed <= 0`, `s_fNameTimer > 24.0f / speed`, and `timeline_date >= world->date` checks sit inside `if (CTimeline::Update(dt))`, so clicking Pause or crossing `24.0f / speed` / `world->date` during an event-free month leaves stale country names on the map.
6. **Live province owners leak into timeline borders (`0x12dc261`, `0x12dc4a4`)**: `0x12dd1d9` passes `non_timeline = (mode != 0x25)` to `0x12dbfc0` at `[rsp + 0x118]`, but `0x12dc261` and `0x12dc4a4` still read live `province+0x468` (`owner`) and `+0x488` (`tribal_owner`) instead of `CTimelineState->owners[]`.

## Playback responsiveness improvements and limits

- **Immediate timeline color updates (`0x6ea0b0`, `0x815405`)**: When `CTimeline::Update` applies events while in map mode `0x25`, Harmony marks `IDLER_COLORS_DUE` (`+0x34`) = `1` and sets `IDLER_COLORS_AT` (`+0x38`) = `0`. The subsequent call to `0x6ea0b0` returns early (`0x6ea0be`) without imposing the `200 ms` `MAX_MAPMODE_COLOR_UPDATE_DELAY_MS` delay, so the 40 KB province color texture (`0x10a8c30`) updates in the same frame as borders (`0x12dd0c0`).
- **Immediate tag/name rebuild & `0.25 s` name cap (`0x8153fd`)**: Map names (`map+0x25a0 = 1`, `0x10a9be0`) are flagged on the exact frame any `ChangeTag`, `CountryName`, or `CountryAdjective` event applies, and periodic map-name rebuilds during territory changes are capped at `min(0.25, 24.0 / speed)` seconds instead of waiting `24.0 / speed` (`4.0 s` / `2.0 s` / `0.8 s` / `0.3 s` across `slow`..`veryfast`).
- **Untouched cadence**: The 1-month stepping rate inside `CTimeline::Update` (`0x13c35c0`) remains unchanged; all daily events inside `(old_date, new_date]` still apply together at each monthly step.
- **Explicit limit (cross-list sub-day tie-breaking)**: `CProvince` and `CCountry` history lists store integer hour timestamps (`days * 24`) with no cross-list global sequence counter. On the same calendar day, province ownership events run before country events (`ChangeTag`), which handles conquest/integration before tag formation and conquest after formation; releasing the old tag on the exact same day after forming the new tag cannot distinguish sub-day province order without engine-side instrumentation. Also, when `world == 0` (`base_timeline` static disk construction from `[0x233fea0]`), rebuilding delegates to the native function at `0x13c2980`.
