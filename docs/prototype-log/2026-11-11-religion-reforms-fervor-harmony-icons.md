# Religion: reforms, fervor, harmonization, icons and metropolitans (M18)

Tried 2026-11-11 in a test game of this checkout's (a developer build with `dev-test-games`), driven with `eu4-harmony game` as the player: new 1444.11.11 games as Muscovy and as Ming, and the 1700.1.1 save of the AI's game (`observer-1700.1.1`), the player's country changed with the game console's `tag`. Part M18 of [mod-capabilities.md](../plans/mod-capabilities.md), after [church aspects and blessings](2026-11-11-religion-aspects-blessings.md); the research is [religion.md](../research/religion.md).

## What was tried

- **Fervor**, Switzerland (Reformed, 1700), with `fervor_trade` on:
  - The first build listed the game's placeholder `nullfervor`, cost 0, among the effects; the framework now lists only effects of the effect's own class, and `enable_fervor nullfervor` is refused `not_found`.
  - The first build wrote the effect by its number (`effect=2`), and posting it faulted in the command's reader (`0xaadebd`, reading 0x0), refused as `internal` with nothing done: the writer writes the effect by its key, and the reader looks the key up. Written by its key, it was carried out.
  - `religion.options` listed `fervor_trade` as `disable_fervor`, current, and `fervor_war`, `fervor_stability` and `fervor_diplomacy` as `enable_fervor`, each costing 8 `fervor`.
  - `enable_fervor fervor_war` answered null and read back current; a second one was refused "fervor_war is on already". `fervor_stability` and `fervor_diplomacy` turned on in turn; `disable_fervor fervor_trade` answered null, a second one "fervor_trade is not on".
  - With the fervor spent, `fervor_trade` was then offered refused with the game's line "无法做到这点，需要8热情点数。".
- **Harmonization**:
  - Shun (CSH, Confucian, 1700): `religion.options` listed `harmonize` for Shinto and Theravada, both of its own group. `harmonize shinto` answered null; it then read `stop_harmonizing shinto`, current, and Theravada still offered. `harmonize sunni` was refused `not_found`. `stop_harmonizing` answered null and both were offered again; a second one was refused "the player harmonizes with no religion now".
  - Ming (1444): the first build failed `options` with `internal` ("unreadable memory 0xd0+4"): it read another group's candidate as a `std::vector` of all its religions, and met a null. The framework now names such a group by its first religion, as the view's button does: Ming was offered Theravada, Vajrayana and animism, the pagan group's first. `harmonize animism` answered null and read back as `stop_harmonizing animism`, current.
- **Icons and metropolitans**, Muscovy (1444):
  - `religion.options` listed the five icons, `icon_michael` to `icon_climacus`, by their keys (the first build gave their shown names, the string at `+0x18`, as keys; the key is at `+0x158`), each costing 10 `patriarch_authority`, refused with "你需要10点牧首权威才能让全部教堂敬拜新的圣像，但是当前只有0.00点。".
  - It listed `consecrate_metropolitan` for each of Muscovy's 11 states with a province of it: Moscow's, Kolomna's state (306) and Perm's (1754) allowed, the others refused with the game's lines (the state's development below 30, Kasimov's Sunni faith and Mishary culture).
  - `consecrate_metropolitan 295` answered null; patriarch authority read 5.00 in the icons' lines, and Moscow's state was then refused "莫斯科直属州已祝圣都主教！". 296 was refused `rejected` with the game's lines, and province 1 "province 1 is not the player's".
  - After 306 and 1754 were consecrated (15 authority), `commission_icon` answered null: the icon read current, the others' lines read 5.00 authority, and a second commission was refused "「圣米迦勒像」是我们当前正在敬拜的圣像！".
- **Religious reforms**, the Aztecs (1444): `religion.options` listed `nahuatl_reform_1` to `_5` as `pass_reform`, none passed, each refused with the group's failing `can_buy_idea` lines ("稳定度至少为1", "属国数量至少为5"), no cost; `reform_religion` refused with the framework's two lines (every reform first, no one to learn from).
- `cargo xtask scenario sweep --sweep.only religion` passed: 14 capabilities in 5 places, 130 requests, each answered with its shape or a refusal the catalogue has.
- The test game was released; none of its four games crashed.

## Not verified

- **A religious reform passed**, and **the religion reformed**: no country met a reform group's conditions; only the refusals were seen. That the spending posts the reform by its index and the group's kind of power rests on the reforms window's button (`0x139d930`).
- **Harmonization's own refusals**: the framework offers only the game's candidates, each allowed; why a religion is not offered (development, the one at `+0xf38`) is not given.
- **An icon's own conditions** and the icon's twenty years running out; a metropolitan where the state is not fully cored or of an accepted culture (only the development and the faith lines were seen).
- Fervor's monthly gain and the effects' own `trigger` (the block at `+0x288`) were not looked at; fervor read only through the check's lines.
- An extension's own calls (everything was sent from the command line), ironman, and the religion view on screen.
- Not built: scholars, piety, the curia and papal influence, holy orders, teachings and the secondary religion ([the research](../research/religion.md#what-is-known-of-the-views-other-buttons)).
