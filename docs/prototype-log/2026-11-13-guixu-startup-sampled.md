# 2026-11-13 Guixu and 2000 AD startup sampled and timed (tested by me)

Where startup spends its time with large total-conversion mods, measured on a claimed test game (`guixu-startup`) of the branch at `0319e115` (master), built in release with `dev-test-games`, `dev-call-trace` and `dev-test-switches`. Hidden starts, every DLC enabled, `music.txt` copied from the player's user folder; the player's own game (`eu4.exe` 25112, idle) was open in the background.

## Runs

1. **Guixu (`ugc_3047072888`, 归墟之门 1.254, with `ugc_2976470733` Chinese Language Mod and `ugc_2830362508` Emoji Universalis), first start** (Guixu's own 103 MB of `music/*.ogg` not yet in `music.txt`): main menu at **38.90 s** (`eu4.exe` 90148).
   - `main` up to the engine's Init: 0.00 to 11.15 s (11.15 s)
   - The engine's Init (`0x14c02b0`): 11.15 to 16.80 s (5.65 s, caching Guixu's music)
   - The gfx stage (`0x202850`): 16.92 to 21.46 s (4.54 s; meshes and particles `0x14dc5d0` 18.80 to 21.37 s, 2.57 s)
   - The databases (`0x208620`): 21.54 to 27.49 s (5.95 s)
   - The events (`0x6f3250`): 27.52 to 29.76 s (2.24 s)
   - Flags and loading screens (`0x10b2b70`): 29.91 to 31.46 s (1.55 s)
   - The rest of InitGame (`0x2043a0` after flags): 31.46 to 34.64 s (3.18 s)
   - The main menu's part (`0x206260`): 34.64 to 38.08 s (3.44 s; 4,265 glyphs drawn in 1.6 s)

2. **Guixu, warm start, sampled on all threads at 100 Hz** (`cargo xtask profile --wait --threads --hz 100 --slice 2`, no `dev.toml`, `eu4.exe` 84612): main menu at **29.69 s** (2,758 ticks in 30.3 s).
   - `main` up to the engine's Init: 0.00 to 10.91 s (**10.91 s**)
   - The engine's Init (`0x14c02b0`): 10.91 to 11.23 s (**0.32 s**)
   - The gfx stage (`0x202850`): 11.34 to 14.89 s (**3.55 s**; meshes and particles 12.78 to 14.80 s, **2.02 s**)
   - The databases (`0x208620`): 14.98 to 20.25 s (**5.27 s**)
   - The events (`0x6f3250`): 20.28 to 22.70 s (**2.42 s**)
   - Flags and loading screens (`0x10b2b70`): 22.89 to 23.56 s (**0.67 s**)
   - The rest of InitGame: 23.56 to 26.82 s (**3.26 s**)
   - The main menu's part (`0x206260`): 26.83 to 29.22 s (**2.39 s**; 4,235 glyphs drawn in 0.85 s; private memory 4,340 MB)
   - Main-thread samples: 50.8% sleeping in `ZwDelayExecution` while waiting for `LoadingThread`; **29.2% in `0x14b4320` (localisation loading)**, of which **21.8% in `0x16faae0` (replacing a localisation entry)**, 2.2% in `unicode::localisation` (`from_str` + `legacy::decode`), 1.8% in `0x16fd7a0` (duplicate/collision scan); 5.0% in `0x206260` (2.9% waiting on `DwmpDxGetWindowSharedSurface` during loading-screen presents in `0x10a2e40`, 1.8% loading map graphics `0x12db850`), 3.0% building map names `0x10af270`.
   - All-threads running samples: the drawing thread (`tid 9700`, `drawing_unlimited: true`) ran at 100% of a core throughout startup (~37% of all running samples) spinning on `PREPARED == 0`; `LoadingThread` (`tid 38196`, 21.3% of running samples) spent 7.0% in `0x208620` (decisions `0x4882c0` 1.3%, missions `0x8d5bd0` 1.2%, `customizable_localization` `0x46fab0` 1.0%, map logic `0x87b9e0` 0.8%), 4.2% in `0x6f3250` (events), 3.5% in `0xb9b4e0` (`0xb9c160`, trigger optimization profile), 0.6% in `0xb89690`, 0.4% in `0xb980a0`, 0.4% in `0xb89560`, and 1.8% in `0x14dc5d0` (meshes). Across databases and events, `0xdd47b0` (parameterized effects) was on the stack of 5.4% of running samples and `0xb9a490` (parameterized triggers) 2.4%.

3. **Guixu, warm start with `dev-call-trace` timing** (`eu4.exe` 67492): main menu at **36.82 s** (lengthened by 701,820 traced calls of `0xdb0860`, `0xdd47b0` and `0xb9a490`).
   - `0x873fc0` (first localisation load): 1.87 to 7.98 s (**6.109 s**, `0x16fd7a0` 0.671 s)
   - `0x14b4a20` (second localisation load, destroying and reloading the first): 7.98 to 13.65 s (**5.676 s**, `0x16fd7a0` 0.637 s); 572,300 localisation entries converted across the two passes (286,150 each)
   - `0x14dc390` (mesh types): 2.242 s
   - `0x208620` (databases, 7.35 s with trace overhead): `0xb9b5a0` (`trigger_profile.txt`) 0.026 s, `0x87b9e0` (map logic) 0.856 s, `0x46fab0` (`customizable_localization`) 0.770 s, `0x8d5bd0` (missions) 0.848 s, `0x4882c0` (decisions) 1.139 s, `0x7ea2c0` (history) 0.585 s, `0x8f0350` (wait for picture checks) **0.000 s**
   - Parameterized script: `0xdb0860` 350,928 calls, **0.250 s** (with S12); `0xdd47b0` 104,358 calls (12.16 s inclusive with nesting); `0xb9a490` 246,536 calls (2.68 s inclusive)
   - End of InitGame: `0xb89690` 0.342 s, `0xb89560` 0.250 s (and `0xb9b4e0` + `0xb980a0` 2.65 s between them)
   - `0x10a2e40` (map graphics): 1.454 s

4. **2000 AD (`ugc_2935149060`, 风云世纪两千年 v18, with `ugc_2830362508` and `ugc_253263609`), light call-trace** (excluding per-use `0xdb0860`/`0xdd47b0`/`0xb9a490`, `eu4.exe` 85708): main menu at **99.29 s** (first start for its music, 2.44 s in `0x14c02b0`; private memory 9,155 MB; 10,885,603 script tokens).
   - First localisation load `0x873fc0`: 2.375 s (`0x16fd7a0` 0.636 s); second load `0x14b4a20`: 2.064 s (`0x16fd7a0` 0.643 s) — 2000 AD has only 4 files in `localisation/replace` (0.29 MB) vs 1,026 in `localisation` (29.56 MB), confirming that Guixu's extra 7 s in localisation comes from `localisation/replace` (333 files, 13.51 MB).
   - Meshes and particles `0x14dc5d0`: 3.86 s (`0x14dc390` 3.12 s); gfx stage 5.94 s.
   - Databases `0x208620`: **21.96 s** (`0x87b9e0` 0.85 s, `0x8d5bd0` 1.26 s, `0x4882c0` 3.00 s, `0x7ea2c0` 2.61 s, `0x8f0350` 0.000 s).
   - Events `0x6f3250`: **41.10 s**.
   - Flags and loading screens `0x10b2b70`: **6.18 s**.
   - Rest of InitGame: **10.18 s** (`0xb89690` 1.12 s, `0xb9b4e0` + `0xb980a0` 6.32 s, `0xb89560` 0.80 s, effect validation ~1.9 s).
   - Main menu's part `0x206260`: **3.83 s** (`0x10a2e40` 2.44 s; 4,855 glyphs in 1.5 s).

## What was not verified

- Cold disk cache (all runs had files in the Windows cache after the first start).
- Entering a new game or loading a save in Guixu (this run measured startup to the main menu only).
- A machine with no background `eu4.exe` open.

## Cleanup

`cargo xtask game release --owner guixu-startup` reported all four `eu4.exe` runs exited cleanly (`0x1`, no crash), kept logs in `guixu-startup-20261113-221253\kept`, and `cargo xtask game status` confirmed 0 test games held. Temporary `dev.toml` and `/tmp/guixu-prof` removed.
