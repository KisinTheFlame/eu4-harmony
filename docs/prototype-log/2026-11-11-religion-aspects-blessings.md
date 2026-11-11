# Religion: church aspects and blessings (M18)

Tried 2026-11-11 in a test game of this checkout's (a developer build with `dev-test-games`), driven with `eu4-harmony game` as the player, from the 1700.1.1 save of the AI's game (`observer-1700.1.1`), the player's country changed with the game console's `tag`. Part M18 of [mod-capabilities.md](../plans/mod-capabilities.md), after [personal deities and cults](2026-11-11-religion-deities-cults.md); the research is [religion.md](../research/religion.md#church-aspects-and-blessings). The church's and the blessings' fields were first read from the running game's memory from outside, then through the framework.

## What was tried

- **Sweden** (Protestant), 200 church power, its church's three slots holding `icons_images_aspect`, `organised_through_bishops_aspect` and `allow_usury_aspect`:
  - `religion.options` listed the 16 aspects Protestantism's list holds: the three chosen as `remove_aspect`, allowed, and the other 13 as `add_aspect`, each costing 100 `church_power`, refused with the game's line "至多能同时有 3 宗教信条。移除一个旧的信条，可以选择新的信条。"
  - `religion.remove_aspect allow_usury_aspect` answered null: the slot held the game's `noaspect`, the church power stayed 200, and every `add_aspect` became allowed.
  - `religion.add_aspect translated_bibles_aspect` answered null: the free slot held it and the church power was 100.
  - `add_aspect parish_registers_aspect` with the slots full was refused `rejected` with the slots' line; `remove_aspect allow_usury_aspect`, an aspect the church no longer had, and `add_aspect nosuch_aspect` were refused `not_found`.
  - With the church power spent (removing one, adding one at 100, removing another), `add_aspect parish_registers_aspect` was refused with "你需要至少100.00教会力量来选用另一个信条（目前有0.00）".
- **Ethiopia** (Coptic), holding `send_monks_to_establish_monasteries` and `will_of_the_martyrs`:
  - `religion.options` listed the Copts' five blessings, those two `current`, each refused with "并非所有圣地都处于你的宗教的控制下。2个你最近选取的神赐不会生效。": the Copts did not hold their holy sites in that save.
  - `religion.choose_blessing legitimize_government` was refused `rejected` with that line, and `blessing_yasna`, a Zoroastrian blessing, `not_found`.
- `cargo xtask scenario sweep --sweep.only religion` passed: 6 capabilities in 5 places, 55 requests, each answered with its shape or a refusal the catalogue has.
- The test game was released and did not crash.

## Not verified

- **A blessing taken**: no Coptic or Zoroastrian country in the save keeps its holy sites, so `choose_blessing` was only refused; whether taking one again that the country holds replaces it was not seen.
- **Stability too low to remove an aspect** (`CHANGE_ASPECT_TOO_LOW_STAB`): Sweden's stability allowed it.
- Whether the church's view leaves out an aspect whose `potential` fails: Protestantism's list holds `anglican_aspect`, which `options` listed for Sweden, refused only for the slots.
- The Hussites' and the Jewish aspects, a church with fewer than three slots, and an extension's own calls (everything was sent from the command line).
- The religion view was never opened on screen.
- The view's other buttons (fervor, religious reforms, harmonization, piety, icons, the patriarch, scholars, the curia, holy orders, teachings, converting) are not offered; [the research](../research/religion.md#what-is-known-of-the-views-other-buttons) says how far each was read.
