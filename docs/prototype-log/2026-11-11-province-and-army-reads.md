# 2026-11-11 A province's prosperity, manpower, supply and institutions, and an army's units, morale and maintenance (tested by me)

Parts M6 and M7 of [mod-capabilities.md](../plans/mod-capabilities.md), on a branch of `claude/mod-capabilities-gaps-899e75` after M1 (`d7cc5d25`), with the change not yet committed. The build had the test games' pipes. One test game of this worktree's, started hidden twice (the second time with the fix to the player's view below), with the player's playset (the Chinese one); the save `observer-1700.1.1` of the late game test saves, loaded as the Ottomans (`player="TUR"`), the AI's game until then. Every read went through `eu4-harmony game` in the player's view (the setting "Remote control sees everything" off), the game's panels compared on screenshots.

- **Checks**: `cargo fmt`, the release build, `cargo test --release` for the workspace, both clippy runs and `cargo xtask offline-check` passed. One run of the tests had the Lua host's `files_keep_the_extension_s_data_in_its_own_folder` fail once, and pass on the next two runs with nothing changed.

## Provinces (M6)

- **Constantinople (151)**, the Ottomans' capital, in the state of Thrace: `prosperity` 70.0, `devastation` 0.0, `local_manpower` 4.04, `supply_limit` 79.36, `separatism` 0, and seven institutions, the first six at 100 and `enlightenment` at 0. The province view showed manpower 4,040, supply limit 79 and devastation 0.00%; the state tab's prosperity bar's tooltip "当前繁荣进度为70.00%".
- **Venice (112)**, another country's: `prosperity` and `local_manpower` were first left out in the player's view; the province view showed manpower 4,025, supply limit 52 (we read 52.56) and devastation 0.00% with the unrest hidden, and its state tab "当前繁荣进度为100.00%". So the player's view now leaves out neither: after the fix, in the same save loaded again, Venice read `prosperity` 100.0 and `local_manpower` 4.025.
- **Mangyshlak (439)**, Transoxiana's, after the fix: `prosperity` 68.0, `devastation` 5.37, `local_manpower` 0.256, `supply_limit` 29.34, `separatism` 8. Its province view showed devastation 5.37%, manpower 256 and supply limit 29; its separatism is not on it, the unrest being hidden for another country's province. Dalarna (8) and the Andamans (574), territories, read `prosperity` null and devastation 55.884 and 59.916.
- **The map's tooltip** of another country's provinces shows the supply limit too (Ancona's 44, Augsburg's 45, with the army needing it).
- **Every owned land province** (2,930), read one by one with no error: separatism in 47 (Mangyshlak, Transoxiana's, 8 years; Quang Nam 10), devastation in 324 (Dalarna, Muscovy's, 55.884; Stockholm 6.717), an institution part way in 231 (the Andamans, France's, printing press 60.261 and manufactories 22.756), six territories of the Ottomans' with `prosperity` null, and enlightenment, which appeared at the start date, at 0 in each of the Ottomans'.

## Armies (M7)

- **The Ottomans' 28 armies and fleets** read with `army.list`. The 7th army in Constantinople: `units` 6 infantry, 3 cavalry, 9 artillery, `strength` 18.0 of `max_strength` 18.0, `morale` 3.447 of `max_morale` 0.51, no `leader`, `maintenance` 5.088. The army panel showed 6,000, 3,000, 9,000 and 18,000 men, "无将领", and its morale tooltip "士气上限为0.51，当前为3.44": the game itself gives a most of 0.51, below the morale the save had. A month later (`time advance 31`) every regiment's morale was 0.51. Why this game's most is so low was not looked into. The chest's 2.9 on the panel is the loot a month, not the upkeep.
- **Maintenance**, summed over the armies, 111.884, against the ledger's army maintenance of the month, 112.252; over the fleets 53.632, against the ledger's 27.441, about half: the maintenance is at full, before the sliders. Where the army's 0.368 comes from was not found.
- **`army.regiments`**: the 7th army's 18 regiments, each with its id, name ("博卢第 4 团"), kind, unit type (`ottoman_toprakli_hit_and_run`, `horse_artillery_unit`), strength 1.0 and morale; a fleet of 72 ships, transports among them (`cog`, named "全知者"); `army regiments 1` refused `not_found`, "no own army 1". Ours are in the game's order; the panel sorts them by kind.

## Not verified

- Another country's army's panel was not opened (a click on its flag opened the diplomacy view), so what the player's view shows of it, its units, strength, morale and leader, rests on the map's counter, not on its panel.
- A leader's id was read for four armies (43408, 42850, 42670, 42751) but not compared with `leader.list`.
- Separatism was not compared with anything the game shows: another country's province hides it, and none of the Ottomans' had any. The full scan was not run again after the fix.
