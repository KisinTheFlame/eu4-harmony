# Institutions, ages and monuments

Part M20's first half of [mod-capabilities.md](../plans/mod-capabilities.md): the `institution`, `age` and `monument` groups, on a branch of `claude/mod-capabilities-gaps-899e75` at `22737ae9`, committed as `5e05fa02`. The build had the test games' pipes. One test game of this worktree's, started hidden, with the player's playset (the Chinese one); the save `observer-1700.1.1` of the late game test saves, loaded as the Ottomans (`player="TUR"`). Every request went through `eu4-harmony game` as the player; the game console gave the money, splendor and institution spread the actions needed.

## What was tried

- **`institution.list`**: seven institutions, the six of 1444–1650 embraced (`share` 100) and the enlightenment not, at a share of 0.0 and 13297.49 ducats, refused with the game's line "无法接纳启蒙主义，因为其尚未在我国广泛地传播。至少需要在我们经过自治度修正的总体发展度中占比 10%，当前只有 0.0%。". Names in the player language (封建制度, 启蒙主义).
- **Embracing**: refused at first with that line. The console's `institution <province> enlightenment 100` on nine Ottoman provinces made the share 7.1, the line's own "当前只有 7.1%", and on twelve more 13.1, when `institution.list` offered it for 11553.03 ducats with no reason. `institution.embrace enlightenment` answered null; the treasury went from 19749.50 to 8196.47, 11553.03 less, and the list said `embraced`. Embracing it again was refused "the player has embraced enlightenment already"; an unknown key `not_found`.
- **`age.get`**: `age_of_absolutism`, 专制主义时代, splendor 320; after the console's `splendor 1000`, 1320.
- **`age.abilities`**: the age's eleven abilities in the game's order, five taken (no cost, no reason), the others at 800 splendor: at 320 splendor refused with "你需要更多的辉煌点数，当你有800才可以开启下一项能力，你当前只有320。", France's, the Netherlands' and Sweden's and the Manchus' with their `allow`'s lines ("是法兰西"; "以下条件至少满足一个:" with its two lines).
- **Taking an ability**: at 1320 splendor `age.take_ability ab_rival_change` answered null; splendor read 520 after, the ability `taken`, and `ab_autonomy` refused again with the splendor line, by `age.take_ability` too. Taking `ab_rival_change` again was refused "the player has taken ab_rival_change already", an ability of another age (`ab_justified_wars`) "is not an ability of the age the world is in", an unknown key `not_found`.
- **`monument.list`**: the Ottomans' sixteen great projects, all monuments, with their provinces and tiers (Hagia Sophia tier 2 in 151, the Parthenon tier 0 in 146), costs of 1003, 2507.50 and 5015 ducats by tier, and the game's reasons: a province busy building ("君士坦丁堡现在正忙于建造苏丹艾哈迈德清真寺。 于1703.12.28完成。"), too little money ("无法负担所须的1003.00¤。"), and a monument's requirements with their lines (Rila monastery: "该省份不满足里拉修道院的要求。" and the province's religion).
- **Upgrading**: with 20752.50 ducats the Parthenon was offered; `monument.upgrade parthenon` answered null, the treasury went down 1003, `province.get 146` listed a `great_project_construction` from 1700.1.1 to 1709.5.16, and the Parthenon was refused after with "雅典现在正忙于建造帕特农神庙。 于1709.05.16完成。". Hagia Sophia was refused with its province's line; an unknown key `not_found`.
- The test game did not crash.

## What was not verified

- The game's panels were not opened on screen: the institutions view, the ages view and the monuments window were compared with nothing but their own texts and with what the actions paid.
- No canal: the Ottomans have none in 1700, so `kind` read `monument` only.
- Embracing refused for its cost (`INSTI_TOO_EXPENSIVE`) was not met; the monuments' refusal for money was.
- An upgrade to tier 3, and a project at tier 3.
- Another country's view (observe mode with a country in view) and an ironman game.
