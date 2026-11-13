# 2026-11-14 Shorter startup with large mods verified and measured (tested by me)

M1 to M4 of [large-mod-startup.md](../plans/large-mod-startup.md), built on `claude/gracious-kepler-9a0592` from master at `0319e115` and following [the sampled run](2026-11-13-guixu-startup-sampled.md):

- **M1 (`localisation`)**: `main`'s redundant second localisation load at `0x877a79` skipped (`0x14b4a20` kept for `reloadloc`, `-reload` and `-update_loc`), `0x16faae0`'s provenance search answered through a per-collection `(hash, file_index)` open-addressing index, and `0x16fd7a0`'s duplicate/collision check answered in one pass over sorted `collection + 0x20` when category 5 (`text.log`) is not registered.
- **M2 (`trigger-profile`)**: `0xb9b4e0`'s per-trigger lookup in `trigger_profile.txt` (`[0x242be60] + 0x48`) answered through bottom-up canonical structural IDs with a type bloom filter, a scratch stack and per-pass trigger pointer memoization.
- **M3 (`listed-folders`, `lookups`)**: non-ASCII files in a directory skipped during listing instead of dropping the directory's ASCII paths back to Windows `stat`.
- **M4 (`drawing-thread`)**: the drawing thread waits on a condition variable while `PREPARED == 0` instead of spinning on the presentation lock when `drawing_unlimited` is enabled.

Tested on a claimed test game (`guixu-verify`), hidden starts, every DLC enabled, music cached in `music.txt`, no other `eu4.exe` running.

## Checked against the game's own answers

- **Guixu (`ugc_3047072888` + `ugc_2976470733` + `ugc_2830362508`)**, `checked = ["localisation", "trigger-profile", "listed-folders", "lookups"]`, startup to the main menu, `menu start` (observe mode), and `time advance 10` (1444.11.11 to 1444.11.21):
  - `localisation`: **76,964** answers checked (every `localisation/replace` provenance lookup, the collision check, and a full store snapshot of all 286,150 entries and provenance records before and after running `0x14b4a20` at `0x877a79`), **0 differing**.
  - `trigger-profile`: **2,826,796** trigger lookups checked against `0xb9b4e0`, **0 differing**.
  - `listed-folders`: **1,067,167** directory lookups checked (including Guixu's root folder with Chinese filenames), **0 differing**.
  - `lookups`: **268,395** missing-path lookups checked, **0 differing**.
- **The player's playset** (including Regional Combat and Convenience Tools), `checked = ["localisation", "trigger-profile"]`, startup and `menu start FRA`:
  - `localisation`: **579** answers checked, **0 differing**.
  - `trigger-profile`: **2,345,254** answers checked, **0 differing**.
- **`scenario checked`** (20 years in observe mode from 1444.11.11 to 1464.11.11 with `checked = ["all"]`): passed, 0 differences across all nine checkable speed-ups.
- **Console commands**: in-game `console run verify_loc` answered `"Unlocalized keys found, written to error log now"`, and `console run reloadloc` answered `"Localisation reloaded!"`.
- **Mod localisation splicing**: with `localisation` on, `core.log` spliced `"localisation/text_l_english.yml"` once per startup instead of twice.
- **Drawing thread at startup**: `core.log` recorded `0 ms` of lock wait on both the main thread and the drawing thread throughout startup before a session began.

## Startup times (back-to-back, alternating on and `game-paths = ["localisation", "trigger-profile"]`)

Times in seconds from `core.log`'s `loading: the main menu at ...` summary line:

| Playset | Mode | `main` to `Init` | `the rest of InitGame` | Total to main menu |
| --- | --- | ---: | ---: | ---: |
| **Guixu 1.254** (3 pairs) | `game-paths` | 11.78, 11.76, 11.70 s | 3.57, 3.69, 3.72 s | **30.52, 30.55, 32.38 s** |
| | **on** | **2.48, 2.38, 2.44 s** | **2.39, 2.43, 2.42 s** | **20.55, 20.16, 20.02 s** |
| **Player's playset** (2 pairs) | `game-paths` | 2.23, 2.18 s | 3.08, 2.95 s | **18.24, 17.91 s** |
| | **on** | **1.91, 1.79 s** | **2.01, 1.97 s** | **16.88, 16.45 s** |
| **2000 AD v18** (warm, 1 pair) | `game-paths` | 5.54 s | 10.16 s | **78.08 s** |
| | **on** | **2.24 s** | **8.46 s** | **75.61 s** |

- On Guixu, `main` up to the engine's Init dropped by **9.3 s** (from 11.75 s to 2.43 s) and the end of `InitGame` by **1.25 s** (from 3.66 s to 2.41 s), bringing the main menu **10.5 to 12.3 s sooner** (~34% faster).
- On 2000 AD, `main` up to the engine's Init dropped by **3.30 s** (5.54 s to 2.24 s) and the end of `InitGame` by **1.70 s** (10.16 s to 8.46 s), 5.0 s off those two stretches (while single-threaded database and event parsing swung by ~1.2 s between the single runs).

## Not verified

- First-ever launch with no `settings.txt` on disk (verified by code trace in [localisation-loading.md](../research/localisation-loading.md), not driven interactively through the language screen).
- Passing `-debug` on the command line to confirm `text.log` receives category-5 lines.
- The command-line switches `-export_loc`, `-import_loc`, `-purge_loc` and `-verify_loc`.

## Cleanup

`guixu-verify` released cleanly (all 17 `eu4.exe` runs exited with `0x1`, 0 crashes), `dev.toml` removed, and `cargo xtask game status` reported 0 claims held after the scenarios finished.
