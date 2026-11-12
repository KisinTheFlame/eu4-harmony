# 2026-11-12 Mercenary drilling from a player's save (tested by me)

This run checks whether mercenary regiments in a supplied save can begin drilling, gain drill over a month, and show that drill in the army panel. Those paths worked in the reduced playset below. The player later clarified that the report concerns **mercenary discipline in the battle panel**, called “雇佣兵训练度” in the installed Chinese language mod, while drill is “操练度”. This run tested a different mechanic and does not resolve that report. The relevant [discipline-panel run](2026-11-12-mercenary-discipline-panel.md) records the corrected investigation.

## Build and setup

- Clean checkout fast-forwarded from `10b16708` to `origin/master` at `7f694b85` before building.
- `cargo build --release --features eu4-harmony-core/dev-test-games` passed. The CLI was built with `cargo run --release -p eu4-harmony-cli -- game --test-game merc-drill menu saves`.
- Harmony 0.1.3, capability version 0.3, developer build with test-game pipes. Startup reported baseline 1.37.5, 2,426 matching dependency points, 281 hooks and 90 writes. No development-options file was present in the claim's core folder.
- Claim owner `merc-drill`, process 39048, started hidden. The player's installed framework, playset and saves were not changed. The other holder's `core-highlight` test game was not used or stopped.
- Original input: the player's desktop save named `雇佣兵训练度未生效.eu4`, 63,074,897 bytes, plain `EU4txt`, SHA-256 `5ce7b9cc6e5b211d8e9e8fb35bfedae934cbe68a3f5cf9a9339138399030640a`. Its header gives Portugal (`POR`), 1582.7.1, game 1.37.5.0. It does not give the Harmony or Regional Combat version used when saving.
- The original was copied into a separate temporary data folder before inspection. A second copy, `merc-drill-reduced.eu4`, removed only the two missing mods' entries from `mods_enabled_names`; its save checksum was consequently not preserved. That copy was passed to `game start --save` and copied into the claim's own save folder. The original was not overwritten.

The source save lists eight mods, in this order:

| Workshop item | Name from the source save | This run |
| --- | --- | --- |
| 2743798482 | Automatic Operation自动化操作Mod | Missing locally; omitted from the test copy and playset |
| 2830362508 | Emoji Universalis 抽象风云 | Enabled |
| 3253723536 | 修正分类排序染色1.37-优化版 | Enabled |
| 3804356450 | 国家成立条件优化-Formable Nations Conditions Optimization | Missing locally; omitted from the test copy and playset |
| 3837775887 | Harmony | Enabled; claim loaded the freshly built core |
| 3837793510 | Regional Combat | Enabled, Workshop descriptor version 0.4.0 |
| 2976470733 | Chinese Language Mod for 1.37 | Enabled |
| 1999055990 | Chinese Language Supplementary Mod for 1.37 | Enabled |

Only the claim's copy of `dlc_load.json` was changed. Its mod registrations remained the claim tool's read-only junction to existing registrations. `menu.saves` returned `missing_mods: []` for the reduced copy. Startup confirmed the Regional Combat extension loaded from Workshop item 3837793510.

The enabled mods' files were checked for idea and define overrides and for `allow_mercenary_drill`, `mercenary_ideas` and `DRILL`. None overrides the mercenary idea group or drilling defines. The supplementary Chinese mod does contain `common/ideas/00_country_ideas.txt`; that is not the basic idea-group file. The missing mods were not inspected and their behavior or interactions cannot be excluded.

## Save evidence and expected behavior

The source save's Portugal block contains 35 land armies, including 23 with a `mercenary_company` reference. It has `mercenary_ideas=7`, `army_professionalism=0.06998`, and no regiment `drill` field anywhere in that country block. Absence of the field is serialization evidence; by itself it does not establish a malfunction or the runtime reader's default.

The installed game's `common/ideas/00_basic_ideas.txt` defines the seventh mercenary idea, `organised_mercenary_recruitment`, with `mercenary_manpower=0.3` and `allow_mercenary_drill=yes`. Its English localisation is “Allow Mercenary units to Drill”: it permits drilling rather than directly granting drill. `common/defines.lua` sets `ARMY_DRILL_YEARLY_GAIN=10.0` and `ARMY_DRILL_MAX=100.0`.

Regional Combat's Workshop Lua implements daily movement decisions and the adapter to the framework. It does not replace the game's combat-damage calculation. No combat replacement was assumed in this test.

## What was tried

The save loaded successfully as Portugal on 1582.7.1. Army IDs passed to capabilities include both the save's ID and type; a bare `27233` was initially refused as `not_found` and was corrected to its full ID. This was a client argument error, not a failure to load the army.

`army.options` was asked for existing armies with leaders. The Gascon company, save ID 27233, full ID `116964844372022`, in province 220 could not drill. The game's reason was “你不能在一个不属于你的省份操练军队。” Other companies on eligible land could drill. These are conditions observed in this save, not a diagnosis of what the player meant.

The tested army was the Free Swiss Guard (`自由瑞士卫队`), save ID 27254, full ID `117055038685238`, 13 infantry regiments in province 192, with leader 29889. Its `army.options` answer gave `drill`, `can: true`, with no reasons. It had no drill mission in the supplied save.

The effective sequence, after loading the reduced save, was:

```bash
eu4-harmony game --test-game merc-drill army drill 117055038685238
eu4-harmony game --test-game merc-drill console run autosave
eu4-harmony game --test-game merc-drill time advance 31 --speed 5 --timeout 120
eu4-harmony game --test-game merc-drill console run autosave
```

`army.drill` returned `null`. The first automatic save was copied aside before advancing. A prior attempt to use console command `save merc-drill-before` was rejected as unknown; `savegame` returned an empty string but produced no save file in the claim's save folder. Only the confirmed `autosave` files supplied the before/after data.

| Observation | 1582.7.1, after starting drilling | 1582.8.1, after 31 days |
| --- | --- | --- |
| Free Swiss Guard regiments | 13 | 13 |
| Regiment `drill` fields | Absent in all 13 | `0.833` in all 13 |
| `drill_army_mission` in its army block | Present | Present |
| Portugal regiments with serialized drill | 0 | 13 |

`time.advance` returned reason “the date was reached,” date `1582.8.1`, with no missed signals. The increment agrees with one month's share of the base annual gain, at the save's precision. The before save establishes that the instruction took effect while the regiment fields were still absent; this run did not separately inspect the native deserializer or capture a pre-advance drill tooltip, so it does not claim to prove the reader's default from code.

The army was then selected through the map, and `army.selected` confirmed `117055038685238`. Hovering its drill bar showed average drill **0.83**, with received fire damage **-0.2%**, received shock damage **-0.2%**, and movement speed **+0.1%**. These are the game's displayed effects. The nearby **0.0%** in the card is attrition, as its own tooltip confirms; it is not the drill value. No report from the player establishes that they were looking at that number.

The tooltip was read at the same date as the after save. Its screenshot crop is retained as local evidence outside the repository.

## Result and limits

- Confirmed in this reduced playset: an existing mercenary army can start drilling, its regiment drill grows, and the army-panel tooltip shows the resulting drill. No growth or display failure was reproduced.
- Confirmed separately: the game refuses drilling for an army on land it says the player does not own, despite the country having the relevant idea.
- Not verified: the complete eight-mod environment, the versions active when the source save was made, the player's precise symptom, drill bonuses in actual battle damage, or any difference between Harmony drawing modes or a game without Harmony. The successful growth and UI check gave no reason to widen those comparisons in this run. A subsequent [static investigation](../research/mercenary-drill.md) follows the shared eligibility check through the native land-battle loss path; that is code evidence, not a measured battle comparison.
- No framework or mod source code was changed. No repair, installation, commit, pull request or publication was made.

## Cleanup

`cargo xtask game release --owner merc-drill` reported `released`, process 39048 stopped with exit code `0x1`, and “the game did not crash.” Its retained logs are in the claim tool's seven-day `kept` folder under `merc-drill-20261112-013009`. A subsequent `game status` confirmed that this owner no longer holds a test game. Temporary full-save copies and screenshots were removed after retaining the tooltip crop above. The original save's hash was checked again and remained unchanged.
