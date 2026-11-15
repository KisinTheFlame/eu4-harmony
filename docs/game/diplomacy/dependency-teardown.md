# A subject relation's end, and the crash in it

## The list it walks

- **The `support_independence` list** (`+0x19a0`, token `0x2f2f`; read at `0x2def9f`, written at `0x2d6ce3`, cleared with the country's diplomacy at `0x2e2c9b`) is a cache. `0x93fe30`(country, source label), the country's refresh of its diplomatic caches, clears it with the country's other lists (`0x9400a3`; allies `+0x19b8`, `+0x19d0`, `+0x19e8` and more) and builds them again; a country takes into `+0x19a0` (`0x9419d3`) each other country found in its own array of support relations (`+0x14f8 + 0x170` = `+0x1668`, count `+0x1674`) while it has a valid overlord (`+0x18f0`, the valid byte `+0x18f7`).
- So the list holds every country the subject shares a support relation with, both those supporting its independence and those whose independence it supports: in the 1478.1.1 save, Transoxiana (the Timurids' vassal) lists `MUL` for the relation `first="TRS" second="MUL"`, and the Kazakhs list both `SHY` (`first="SHY" second="KHA"`) and `CHG` (`first="KHA" second="CHG"`).
- **A list of the game's kind** (`objects::list`): head `+0`, tail `+8`, count `+0x10`; each node `0x20` bytes from `operator new` (`0x1a332d4`): the item at `+0`, the previous node at `+8`, the next at `+0x10`. Appending is `0xdf120`; clearing (`0xd0020`) deletes every node (`0x17394f0`, size `0x20`) and zeroes head, tail and count.

## The defect

- Inside the walk, after each relation it ends, `0xa61900` refreshes both countries' caches: `0x93fe30`(the other country, `relation.cpp:1247`) at `0xa61a05` and `0x93fe30`(the subject, `relation.cpp:1248`) at `0xa61a2f`. The subject's refresh clears the very list being walked, deleting the node the walk stands on, and the walk then takes that freed node's next (`0xa61ace`: `mov rax, [rbx+0x10]`).
- What it finds there depends on the heap. While the freed node lies untouched, its old next leads on through the old (freed) nodes and nothing shows. Once the refresh has given the node again to a list it builds, the walk goes on into that other list. The refresh builds a dozen lists of `0x20`-byte nodes for each of the two countries, among them, through `0x40e770` (its only caller is the refresh, at `0x940fa5` and `0x941049`), `home_neighbours` (`+0x1aa8`, token `0x2e93`) and `core_neighbours` (`+0x1ac0`, `0x2e94`), appended with `0xdf120` at `0x40ea5f` and `0x40ea05`; `neighbours` (`+0x1a90`, `0x2e92`) is a vector. Transoxiana's two lists were `KHO SHY NOG TIM CHG` in the last save, the Nogai among them. A country the walk reaches so that shares no support relation with the subject gives the null above, and the game crashes; one with such a relation would see it ended though it was never in the list.
- It needs only a subject with one entry in its list whose subject relation ends while the refresh runs: an overlord annexed or destroyed, independence, release. Transoxiana had a single entry (`MUL`) in the last save: its node, freed after the first relation ended, could only lead on if it was taken again, since its own next was null.
- **Tried** (2026-11-06): in that save played as the Nogai, `annex TIM` in the game console ends the Timurids' rule over Transoxiana through this walk, over Transoxiana's list (`MUL`). With the heap census's quarantine, which keeps what the walk frees and fills it with `0xdd`, the game's own walk faults every time, before it ends.
- That the list mixes both directions is a second oddity: ending the Timurids' rule over Transoxiana also ends Transoxiana's support of Multan's independence. Not looked into further.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x367390` | function | Referenced while examining what crashed; complete role not established |
| `0x3d7f40` | code | Referenced while examining not established; complete role not established |
| `0x3d7f62` | code | Referenced while examining not established; complete role not established |
| `0x442fe0` | function | Referenced while examining not established; complete role not established |
| `0x4e1570` | function | Referenced while examining what crashed; complete role not established |
| `0x513b80` | code | Referenced while examining not established; complete role not established |
| `0x513d82` | code | Referenced while examining not established; complete role not established |
| `0x93e630` | function | Referenced while examining not established; complete role not established |
| `0x976730` | function | Referenced while examining what crashed; complete role not established |
| `0x9786c7` | code | Referenced while examining what crashed; complete role not established |
| `0x9910e0` | function | Referenced while examining what crashed; complete role not established |
| `0xa020f0` | function | Referenced while examining what crashed; complete role not established |
| `0xa5e130` | function | Referenced while examining what crashed; complete role not established |
| `0xa5e14f` | code | Referenced while examining a subject relation's end, and the crash in it; complete role not established |
| `0xa6196d` | code | subject relation's end walks the supporters |
| `0xa619e5` | code | Referenced while examining what crashed; complete role not established |
| `0xa619e7` | code | Referenced while examining not established; complete role not established |
| `0xa61ad8` | code | Referenced while examining the repair; complete role not established |
| `0xa61b15` | code | subject relation's end past the supporters |
