# Factions, naval doctrines and releasing a colony

2026-11-11, one claim's test game, a developer build with the test games' pipes and the command log, every request from `eu4-harmony game` as the player. It tried `government.factions`, `government.boost_faction`, `naval_doctrine.options`, `naval_doctrine.choose` and `subject.options`'s and `subject.interact`'s `release_colony` of [mod-capabilities.md](../plans/mod-capabilities.md) (M20's second half); the game's side is in [research/power-spending.md](../research/power-spending.md#the-countrys-other-spendings).

## What was tried

The save `observer-1700.1.1`, loaded, then other countries through the game console's `tag`.

- **Factions.** Venice (a monarchy by 1700) read none. The Hansa and Ragusa read their three merchant republic factions, `mr_aristocrats` (贵族, military points), `mr_traders` (商人, diplomatic) and `mr_guilds` (行会, administrative), each costing 9 points, their influences adding up to 100 (Ragusa: 7.249, 0.352, 92.399). Ragusa's `boost_faction mr_traders` answered null: its traders went to 10.352, the aristocrats to 2.249 and the guilds to 87.399, and its diplomatic points from 664 to 655. The command log showed `spend_power_command={ … country="RAG" power={ data=5 category=23 skill=1 } }`. `mr_nobody` was `not_found`.
- **Naval doctrines.** Ragusa read every doctrine of the database (34) refused with `你必须具有至少20的海军规模上限才能更改你的海军学说。`, costing 0. Great Britain read them costing 5648.5 ducats each: `wooden_wall` current (`这是你现在的学说。`), the doctrines of other cultures refused with `你不能选择这个海军学说：` and `can_select`'s failing lines (`文化组是伊比利亚`; `以下条件至少满足一个:` with `主流文化是葡萄牙`, `主流文化是巴西`; `拥有国家标志'…'`), and the rest with `你负担不起改变你的海军学说的花费。`. After the console's `cash 6000`, `choose merchant_navy` answered null: the treasury went from 9110.686 to 3462.186, 5648.5 less; `merchant_navy` read current and `wooden_wall` unaffordable. The command log showed `naval_doctrine={ … members={ dummy="GBR" dummy="merchant_navy" } }`. `flying_dutchman` was `not_found`.
- **Releasing a colony.** Great Britain's crown colony `C02` listed `release_colony` (允许殖民地独立) refused with `我们处于战争时期` and `我们正在战争中`; its trade protectorate `MGD` listed none. The first build wrote the command with the keys `first` and `second`, which the game does not read: `subject.options` failed for Portugal's colonies with the command layer's "did not come back". Written with the two keys' names from the game's token table, Portugal's crown colonies `C03`, `C05`, `C08` and its private enterprise `C00` read `can: true`, and `subject.interact C08 release_colony` answered null: `C08` left Portugal's relations and had none, and a second `subject.options C08` was refused, `C08` no longer the player's subject. The command log showed `release_colony={ … from="C08" to="POR" }`.
- **The sweep**: `cargo xtask scenario sweep --sweep.only naval_doctrine,government,subject` passed, 13 capabilities in 5 places, 132 requests.
- The claim was released; the game did not crash.

## What was not verified

- **A faction's refusals**: no faction was met at 100%, nor a player with too few points, so the framework's two lines were not seen in the game. Boosting a revolutionary or a pirate republic's faction, and a faction whose name the panel takes from its other key (`+0x3b0`, where its trigger at `+0x338` holds), were not met: the framework names a faction by its key alone.
- **A naval doctrine's cost** against the military view's tooltip on screen; only the treasury's change was compared.
- **Releasing a colony** while playing on as it (the dialog's `play_as_released_vassal`), which the framework does not offer; and the multiplayer option's refusal.
- Extensions' calls; everything was sent from the command line.
