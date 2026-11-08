# 2026-11-05 Game addresses named once, the catalogue bound once, events turned once (tested by me)

This branch's player build, within a claim, started hidden; a new game as France at 1444.11.11, driven through remote control only. The run checked that the capabilities whose code moved still answer as before.

- **Startup** passed, every dependency point with its new home and its longer bytes; `cargo xtask offline-check` had passed against `eu4.exe` before.
- **Reads** answered as in the run of the parts together above: `country.get FRA` (governing capacity 198.54 of 400, through `game::modifiers`), `technology.options` (598 adm, through `power::Spending`), `stability.options` (100 adm), `economy.options`, `idea.list`, `province.options 183` (developing at 65 a power, through the spending about a province), `estate.list` (the church's influence 16.25, through `game::modifiers`), `leader.options` (the conquistador refused "你需要「探索新世界」国家理念……", the yes-or-no modifier), `trade.nodes`, `trade.node champagne`, `merchant.list` (Bordeaux steering to Champagne, Valencia collecting, through `trade::collects`), `merchant.options champagne`, `loan.list`.
- **Actions**: `province.develop_tax 183` took 65 adm (84 to 19); `economy.take_loan` gave a loan that `loan.list` listed at 129 ducats, 4 % and due 1449.11.11, read through `objects::campaign::loan`.
- **Events**: after the opening question was answered, `time.advance 60` gave `session`, a `day` for each of the 60 days, `month` at 1444.12.1 and 1445.1.1 and `year` at 1445.1.1, through the one translation into events.
- **Not tried in this run**: buying an idea (France has no group taken at the start), a siege's options and a white peace (no war), an extension's handlers hearing the events (unit tests compare their arguments with the old ones), a battle event.
- **Cleanup**: the release put the player's files and the game folder's framework files back.
