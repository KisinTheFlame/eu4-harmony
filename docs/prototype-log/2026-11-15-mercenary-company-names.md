# 2026-11-15 Mercenary company names (tested by me)

Status: reproduced and diagnosed; no implementation change or successful repaired run.

## Build and setup

- Baseline: EU4 1.37.5, from the installed Steam game folder.
- Source: `9cedf657`, branch `claude/vibrant-spence-f7794b`; the host's sync confirmed the branch already contained the current `origin/master`.
- Build: `cargo build --release --features eu4-harmony-core/dev-test-games`, completed successfully.
- Own hidden, silent test game: claim `merc-name-85cc`, with its own copied user folder, core and pipe. The player's existing game was left alone.
- Playset copied by the claim: Workshop items `253263609`, `2830362508`, `2976470733`, `1999055990`, `3253723536`, `3837775887`, `3837793510`, `3838535796`. The active main Chinese translation was item `2976470733`.
- No save was loaded. A new game began as France on 1444.11.11. Brief console switches to Navarra and Ming checked their company lists, then returned to France before hiring. The framework files in the installed game folder were not changed.

## Reproduction

The remote `mercenary.list` result was correct before hiring, including:

```json
{"company":"merc__local_grand_company","name":"大佣兵团"}
{"company":"merc__local_free_company","name":"自由佣兵团"}
```

The translation's original entry for the grand company decoded to "大佣兵团", with legacy bytes `10 27 59 10 63 4f 10 75 51 10 e2 56`. No `£` was present in the decoded mercenary names scanned in the active translation.

In the test game only, `console.run` added 1000 ducats, then `mercenary.hire` hired `merc__local_grand_company` in province 183. The French opening event `flavor_fra.206` was answered with option 0, and `time.advance` advanced 30 days to 1444.12.11 so recruitment completed.

A temporary Python reader opened only the PID registered to this claim, with query and memory-read permissions. It followed the world's province array to province 183, walked that province's unit list, checked each unit's ID and read its string at `+0x148`. It made no writes to the process.

The compact assertion command, run twice while the game was paused, was:

```bash
python -I target/merc_name_probe.py --assert
```

Both executions exited 1 with the same result:

```text
province_id 183
unit 545460846646
0x148 e7 8e 8b e5 ae b6 e9 99 86 e5 86 9b 王家陆军
unit 5282809774134
0x148 e5 a4 bd a3 e5 85 b5 e5 9b a2 夽£兵团
AssertionError: mercenary army name is corrupted; expected 大佣兵团
```

The assertion compared the raw army name with the expected UTF-8 bytes, not merely its displayed reconstruction. Reading the existing unit list takes seconds and requires no input or further time advancement.

A second company, `merc__local_free_company`, was then hired in the same province. After a further 31 days its army, ID `5295694676022`, had the correct name:

```text
0x148 e8 87 aa e7 94 b1 e4 bd a3 e5 85 b5 e5 9b a2 自由佣兵团
```

At that point `mercenary.list` still returned the correct names for both companies, marking both as already hired. The company text and the army's own text had diverged only for the grand company.

## Diagnosis and limits

Disassembly identified the intervening native color remover. It drops every `a7` byte and its following byte, including `a7 e4` spanning "大" and "佣". The resulting bytes exactly match the damaged army name. The copying function receives already damaged input; the company getter itself returns correct text. [The research note](../research/mercenary-company-names.md) records the complete relevant function behavior and the call chain.

The supplied screenshot's glyphs were reproduced in the actual army-name field. No separate screenshot of that army's panel was captured. No implementation was changed, so the original assertion still fails. Existing army restoration, other callers, names with real color selectors and save/reload behavior were not tested. Attempts to use the console's `savegame` command produced no save listed by `menu.saves`; no saved-name result is claimed.

## Cleanup

The claim was released after the reads; its own game ended normally and its user folder and copied core were removed. The claim manager retained the run's ordinary logs under its seven-day retention policy. Temporary memory readers, disassembly, captures and remote-response files were removed from this worktree's build folder. No runtime instrumentation or product-code edits were made.
