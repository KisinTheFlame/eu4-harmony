# Religion: scholars, holy orders, the secondary religion and the gurus' teachings (M18)

Tried 2026-11-11 in a test game of this checkout's (a developer build with `dev-test-games`), driven with `eu4-harmony game` as the player: a new 1444.11.11 game as the Ottomans, the player's country then changed with the game console's `tag` to Castile, the Mongols (KHA) and the Oirat; monarch points added and a country made Sikh with the console's `run` of an effect file in the claim's user folder. Part M18 of [mod-capabilities.md](../plans/mod-capabilities.md), after [reforms, fervor, harmonization and icons](2026-11-11-religion-reforms-fervor-harmony-icons.md); the research is [religion.md](../research/religion.md).

## What was tried

- **Scholars**, the Ottomans (Sunni, following the Hanafi school):
  - `religion.options` listed the seven schools of the Muslim group as `invite_scholar`: `hanafi_school` current, refused with the framework's line for the player's own school; the six others refused with the school's `can_invite_scholar` lines in the game's words, each with a country that follows it as FROM (罕百里派: "代瓦西尔对奥斯曼的好感度至少为150。", and an alliance, a subject or an overlord with 代瓦西尔; the Shia schools also "虔诚低于-50。").
  - With the school's own `can_always_invite_hanbali_school` flag set (`script.set_flag`, as the game), `hanbali_school` read allowed with no reasons; `religion.invite_scholar hanbali_school` answered null, administrative points went from 112 to 62, and `country.get` read the named modifier `hanbali_scholar_modifier` until 1464.11.11.
  - A second invite was refused with the school's line "没有修正罕百里派学者"; `invite_scholar nonsense_school` was refused `not_found`.
- **Holy orders**, Castile:
  - `religion.options` listed all 28 holy orders as `establish_holy_order`, each with its cost in its kind of points (Santiago 50 military, the Benedictines 50 administrative, the Safavid order 100 military) and its trigger's failing lines; three were allowed for Castile: Santiago, Alcántara and Calatrava.
  - `establish_holy_order santiago_order 217` (Madrid) answered null and military points went from 98 to 48. Alcántara in the same state was then refused with the points line and "A holy order is established in this state already."; province 220 (Valencia, Aragon's) "province 220 is not the player's"; the Benedictines in Seville for points (42 of 50); an unknown order `not_found`.
  - With 500 points of each kind added, `establish_holy_order alcantara_order 224` (Seville) answered null and military points went from 548 to 498.
- **The secondary religion**:
  - The Mongols (Tengri, Vajrayana as their secondary): `religion.options` offered `noreligion` and Confucianism as `choose_secondary_religion`, not Vajrayana. `choose_secondary_religion confucianism` answered null; the list was then `noreligion` and Vajrayana. `choose_secondary_religion sunni` was refused `not_found`.
  - The Oirat (Tengri, none): Vajrayana, Sunni and Confucianism, no `noreligion`.
  - Back as the Mongols, `choose_secondary_religion noreligion` answered null, and the list was then Vajrayana and Confucianism, as for a country with none: the game's `noreligion` drops the secondary religion.
- **Teachings**, the Mongols made Sikh: `religion.options` listed the 33 teachings of Sikhism's 11 gurus as `select_teaching`, three a guru, each 50 points of its kind, all allowed with 570 points. `select_teaching share_and_consume` answered null, administrative points went from 570 to 520, and it read back current; a second one was refused "the player has taken this teaching already", and an unknown key `not_found`.
- `cargo xtask scenario sweep --sweep.only religion` passed: 20 capabilities in 5 places, 190 requests, each answered with its shape or a refusal the catalogue has.
- The test game was released; neither of its two games crashed.

## Not verified

- **A scholar invited through the school's own conditions** (an ally of the follower with 150 opinion), rather than with its always-allow flag; and the Shia schools' piety line holding.
- **The state's own lines for a holy order** (`ESTABLISH_HOLY_ORDER_WRONG_OWNER`, `ESTABLISH_HOLY_ORDER_NOT_OWNING_ALL`): no state the player owned only in part was tried. That the province view's button also refuses too few points, which the command's check does not ask, is inferred.
- **The holy orders' and the teachings' buttons in the views** were not found, so whether the province view offers an order only where its trigger holds, and whether the gurus' view offers only the current guru's teachings, are not known: the framework offers every one the command's check allows.
- The secondary religion chosen through the view, against which the list was not compared on screen; what the game makes of a secondary religion beyond the list, such as its `country_as_secondary` modifiers, was not looked at.
- Calls from an extension; ironman.
