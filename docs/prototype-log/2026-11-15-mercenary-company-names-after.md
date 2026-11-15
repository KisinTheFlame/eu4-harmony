# 2026-11-15 Mercenary company names after color removal repair (tested by me)

Status: passed. Newly hired armies retain complete Chinese names, actual color selectors are removed, and both inline and heap-backed names work. Existing damaged armies and save/reload behavior were not tested.

## Build and setup

- Baseline: EU4 1.37.5 from the installed Steam game folder.
- Source: `9cedf657`, branch `claude/vibrant-spence-f7794b`, with the uncommitted `unicode/colors.rs` replacement and its registration. The host's sync confirmed this branch already contained current `origin/master`.
- Build: `cargo build --release --features eu4-harmony-core/dev-test-games`, successful.
- Own hidden, silent test game: claim `merc-name-fixed-85cc`, process 94980, with its own user folder, core and pipe. No framework was installed into the player's game folder.
- Startup report: 2548 matching dependency points, 312 hooks and 90 writes; no startup failure. Framework language `simp_chinese`, game language `english`.
- The copied playset was the same eight Workshop items as [the diagnostic run](2026-11-15-mercenary-company-names.md). An additional temporary test mod was supplied only through `game start --mod target/merc-name-mod`.
- The test mod's UTF-8 BOM localisation file, `localisation/replace/harmony_mercenary_color_check_l_english.yml`, contained `merc__local_free_company:0 "§Y自由佣兵团 test§!"`. This keeps the control free of the grand company's problematic continuation byte, exercises real color selectors and makes the cleaned name 20 bytes, longer than the game's 15-byte inline limit. The grand company was left unchanged.

## Regression and checks

The targeted test first failed under a byte-walking implementation of the native rule, producing the exact corrupted bytes observed in the diagnostic game. With character boundaries, both tests passed:

```text
a_mercenary_name_keeps_its_color_marker_continuation_byte ... ok
removes_only_colors_and_preserves_other_game_text_bytes ... ok
2 passed; 0 failed
```

The following checks completed successfully:

```text
cargo fmt --all
cargo build --release
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features eu4-harmony-core/dev -- -D warnings
cargo xtask offline-check
cargo test -p xtask the_index_lists_every_address_the_notes_name -- --nocapture
```

The core's ordinary test run reported 849 passed and five ignored; the offline check then passed its five installed-game checks in both player and developer configurations. The workspace's installation-dependent test remained ignored; this run did not install anything. The dependency-point byte check includes the new common color-removal replacement.

## Actual hiring and raw names

A new French game began on 1444.11.11. `mercenary.list` returned the unchanged grand company name and the control name with its test suffix, confirming the isolated localisation override was active. In this test game only:

1. `console.run` added 1000 ducats.
2. `mercenary.hire` hired `merc__local_grand_company` and `merc__local_free_company` in province 183.
3. `question.answer` answered the opening `flavor_fra.206` event, question 2, option 0.
4. `time.advance` ran 31 days at speed 5, reached 1444.12.12 and paused.

The temporary read-only probe checked the claim's checkout, PID and process creation time before reading. As in the diagnostic run, it followed the province array and unit list to each army's own string at `+0x148`. It did not write process memory.

This command ran twice, both times exiting 0 with identical army names:

```bash
python -I target/merc_name_probe.py --owner merc-name-fixed-85cc --free-name "自由佣兵团 test" --assert
```

```text
owner merc-name-fixed-85cc pid 94980 province_id 183
unit 545460846646 size 12 capacity 15 0x148 e7 8e 8b e5 ae b6 e9 99 86 e5 86 9b 王家陆军
unit 5274219839542 size 12 capacity 15 0x148 e5 a4 a7 e4 bd a3 e5 85 b5 e5 9b a2 大佣兵团
unit 5278514806838 size 20 capacity 31 0x148 e8 87 aa e7 94 b1 e4 bd a3 e5 85 b5 e5 9b a2 20 74 65 73 74 自由佣兵团 test
PASS: both mercenary armies retain their complete names
```

The grand company's name now retains the `a7 e4` bytes the native function deleted. The control's yellow and reset selectors are absent while all 20 surviving bytes remain. Its capacity 31 confirms game-owned heap storage; the grand company's capacity 15 confirms inline storage. Returning through `menu.return` reached `main_menu` successfully after these reads.

## Limits and cleanup

This verifies creation through the real mercenary hiring path and the actual army fields, rather than only the company list or a reconstructed display string. No army-panel screenshot was captured. Existing damaged names, save/reload, other color-removal callers' complete behavior and every possible formatting combination were not tested. Regression tests separately cover icons, country flags, Windows-1252 text and additional scripts.

The claim was released; the manager reported no crash, exit code `0x1` from release, and status listed no claim of this run. The other session's claim was left alone. Release removed the claim's copied user folder and core. Its copied test mod remained under `test-mods`; this run checked and removed that owned copy and its empty parent explicitly. Only ordinary logs remain under the claim's `kept` folder for the manager's seven-day retention period. The temporary probe, test mod and disassembly in this worktree's build folder were removed after their evidence was recorded here. No player files or registrations were changed, and no runtime instrumentation was added.
