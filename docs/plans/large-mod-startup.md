# Shorter startup with large mods

Status: built (2026-11-14), M1 to M4 in one pull request ([the verification run](../prototype-log/2026-11-14-large-mod-startup.md), following [the sampled run](../prototype-log/2026-11-13-guixu-startup-sampled.md)). Follows [loading.md](loading.md) and meets the same bar ([ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/2026-11-04-making-the-game-faster-leaves-what-it-computes-as-it-is.md)): every entry, provenance record and trigger cost the game keeps is the game's own.

## Goal and scope

- **Goal**: the game reaches its main menu sooner with large total-conversion and translation mods, with the same localisation, files and trigger ordering loaded, and without burning a processor core on an idle drawing thread.
- **In scope**: `main`'s duplicate localisation load, `localisation/replace`'s provenance search, the duplicate-key scan when `text.log` is not registered, the triggers' optimization profile lookup at the end of `InitGame`, folders in the search path that hold non-ASCII documentation files beside ASCII game paths, and the drawing thread before a session prepares its first frame.
- **Out of scope**: the single-threaded script parsing of `common/` databases and `events/` on the LoadingThread; loading mesh types beside the databases (B1 of [startup-side-by-side.md](startup-side-by-side.md), set aside until its map-logic crash is explained).

## What the game does

After L1 to L5 ([loading.md](loading.md)) and S12 ([game-speed.md](game-speed.md)), a warm start with Guixu (`ugc_3047072888`, 归墟之门) still took ~30 s to reach the main menu ([research/localisation-loading.md](../research/localisation-loading.md)):

- `main` (`0x8757e0`) loads the whole localisation at `0x8779b0` (`0x873fc0`), sets the external editor's strings, and at `0x877a79` unconditionally calls `0x14b4a20`, which frees `[0x2357b20]` and loads `languages.yml`, `localisation` and `localisation/replace` a second time from scratch. On every replaced key in `localisation/replace` (333 files in Guixu), `0x16faae0` linearly scans the entire `collection + 0x38` provenance vector (~280,000 records), and `0x16fd7a0` linearly scans `collection + 0x38` again for every run of equal hashes to format category-5 lines that are dropped unless `-debug` registered `text.log`. Together this took 11.7 s of `main` before the engine's Init.
- At the end of `InitGame`, `0xb9b4e0` looks up every compound trigger in `[0x242be60] + 0x48` (`trigger_profile.txt`, 6,593 trees) by recursively allocating and freeing a temporary `std::unordered_set` at every node in the trigger's subtree (~2.3 to 2.8 million calls).
- Non-ASCII documentation files in a mod's root (`1.310更新内容.txt` in Guixu, `18.x更新日志.txt` in 2000 AD) made `listed-folders` and `lookups` drop the whole directory back to Windows `stat`.
- With `drawing_unlimited` enabled, the drawing thread spun on `PREPARED == 0` throughout startup and on the main menu, burning one full core.

## Behavior

- Nothing the player or a mod sees changes except how soon the main menu comes and the CPU used before a session starts.
- In `error.log`, missing-BOM and syntax warnings from `localisation` are logged once at startup instead of twice; `"Localization hash collision found..."` is logged under the exact same condition as before, and passing `-debug` still writes every duplicate line to `text.log`. The `reloadloc` and `verify_loc` console commands and the `-*_loc` command-line switches work as before.
- Always on, with no setting; a developer build's `game-paths` leaves `localisation` or `trigger-profile` to the game, and `checked` compares their answers against the game's.

## Parts

### M1 Localisation loaded once, with indexed replacement and fast collision check

Built (`loading::localisation`, the speed-up `localisation`, group `loading`):

- At `0x877a79` in `main`, the call to `0x14b4a20` is skipped (`0x14b4a20` itself stays intact for `reloadloc`, `-reload` and `-update_loc`).
- In `0x16faae0` (`0x16fabb6`), the linear walk of `collection + 0x38` for `(hash, file_index)` is answered from a per-collection open-addressing index (`RecordIndex`) that catches up with appended records and clears when `0x16fb7c0` clears `[0x2357b20]`.
- In `0x16fd7a0`, when category 5 (`text.log`) is not registered in `[0x2349598]`, each collection's sorted `+0x20` vector is checked in one pass for adjacent equal hashes; when `-debug` registered category 5, the game's `0x16fd7a0` runs.
- Shown: `checked = ["localisation"]` checked 76,964 answers on Guixu (including a full store snapshot of all 286,150 entries and provenance records before and after `0x14b4a20`) and 579 on the player's playset, with 0 differing; `reloadloc` and `verify_loc` tested in-game ([the run](../prototype-log/2026-11-14-large-mod-startup.md)).

### M2 Trigger optimization profile matched through an interned index

Built (`loading::trigger_profile`, the speed-up `trigger-profile`, group `loading`):

- The loaded `trigger_profile.txt` table (`[0x242be60] + 0x48`) is interned bottom-up into canonical structural IDs by `(type_id, sorted_deduped_child_ids)` with a type bloom filter, invalidated when `0xb9b5a0` loads or `0xb9dae0` destroys a manager.
- `0xb9b4e0` matches a live trigger against the interned IDs using a thread-local scratch stack and per-pass trigger pointer memoization (cleared when `0xb89560` frees `[0x2340010]`), with zero per-call heap allocations and immediate exit on any trigger type absent from `trigger_profile.txt`.
- Shown: `checked = ["trigger-profile"]` checked 2,826,796 triggers on Guixu and 2,345,254 on the player's playset, with 0 differing ([the run](../prototype-log/2026-11-14-large-mod-startup.md)).

### M3 Non-ASCII files in a directory no longer disable ASCII listings

Built (`loading::listed`, `loading::lookups`):

- When listing a folder, non-ASCII directory entries are skipped instead of discarding the folder's listing: only ASCII paths without `~` query the listings, and Windows's `CompareStringOrdinal` case folding maps no non-ASCII UTF-16 code unit to ASCII. Non-ASCII query paths still go to the game's `stat`.
- Shown: unit tests with Chinese file and folder names beside ASCII paths, and `checked = ["listed-folders", "lookups"]` over Guixu startup and play: 1,067,167 `listed-folders` answers and 268,395 `lookups` answers checked, 0 differing.

### M4 The drawing thread sleeps while no session frame is prepared

Built (`drawing`):

- While `PREPARED == 0` (at startup, on the main menu, and after an idler switch), `drawing::run` waits on a condition variable notified when `frame_draws` prepares the first frame of a session (or when the game quits), instead of spinning through the presentation lock when `drawing_unlimited` is on.
- Shown: in `core.log` during startup, the drawing thread's lock wait dropped to 0 ms, and `scenario checked` and `scenario perf` passed.

## How it fits together

`loading::localisation` and `loading::trigger_profile` are modules under `core/src/loading/`, registered in `core/src/speed.rs` (`ALL` now holds 18 speed-ups) and documented in `core/dev.toml.example`. M3 and M4 live inside `core/src/loading/listed.rs`, `core/src/loading/lookups.rs` and `core/src/drawing.rs`.
