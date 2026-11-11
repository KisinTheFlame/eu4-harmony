# Religion: personal deities and cults (M18)

Tried 2026-11-11 in a test game of this checkout's (a developer build with `dev-test-games`), driven with `eu4-harmony game` as the player, new games of 1444.11.11. Part M18 of [mod-capabilities.md](../plans/mod-capabilities.md); the research is [religion.md](../research/religion.md).

## What was tried

- **Vijayanagar** (Hindu), with no deity at the start:
  - The first build read every deity's `potential` and `trigger` at the offsets the deity's reader stores them at, and every one held: Buddha and the Norse deities were offered to Vijayanagar, and the game's `Null` deity was listed as the one followed. The blocks start `0x38` before those offsets, where the deity's constructor makes them; read there, `religion.options` offered the six Hindu deities whose `potential` holds (Shiva, Ganesha, Surya, Ganga, Shakti, Vishnu; Buddha left out, his missions not completed), each allowed, none current.
  - `religion.choose_deity odin` and `nobody` were refused `not_found`; `religion.choose_cult yemoja_cult` was refused `rejected`, "the player's religion has no cults".
  - `religion.choose_deity shiva` answered null and `options` then said Shiva `current`; `vishnu` moved `current` to Vishnu, and back to Shiva, with no refusal between: the game lets a deity change at any time but a regency.
  - The game applies a deity's modifiers when it recounts the country's, not at once: `country.modifiers` gave `ae_impact` −0.004 and `core_creation` 0 two days after choosing Shiva, and −0.054 and −0.1 (Shiva's −0.05 and −0.10) on 1444.12.8, after the month turned.
- **Benin** (Fetishist), with no cult:
  - The first build listed the game's `nocult` as the cult followed; it is left out now. `options` lists every cult with its `allow`'s failing lines as the game writes them ("贝宁的宗教是梦创神话" for the Dreamtime's stories; the technology groups and `has_unlocked_cult` for the other African cults), and Yemoja, Roog and Nyame allowed.
  - `religion.choose_cult yemoja_cult` answered null; `options` then said Yemoja `current`, and every cult not allowed, "A cult is followed until the monarch changes."; `choose_cult roog_cult` was refused `rejected` with that line.
  - On 1444.12.6 `country.modifiers` gave `naval_maintenance_modifier` −0.15, Yemoja's.
- `religion.options` as the game answered as it did as the player.
- The test game was released and did not crash.

## Not verified

- A regency: neither the deity's nor the cult's regency line (`CANT_CHANGE_DIETY_REGENCY`, `CANT_CHANGE_CULT_REGENCY`) was seen.
- The Norse deities, offered to a Norse country; the Dreamtime's stories, to an Aboriginal country; a cult unlocked by an event.
- What the religion view itself lists and shows disabled: its lists were not opened, and the framework's (those whose `potential` holds, every cult) is inferred.
- A deity or a cult chosen by a request sent from an extension.
