# 2026-11-11 The script group: flags, variables and events (tested by me)

What was tried: the `script` group of [mod-capabilities.md](../plans/mod-capabilities.md), M10 to M12, from remote control as the game and as the player, and from an extension; what the framework sets seen by the game's own triggers, and what the game's own effects set read by the framework. The research is [scripts.md](../research/scripts.md).

## Setup

- Branch `worktree-agent-a765df354d947c964`, on `claude/mod-capabilities-gaps-899e75` at `1557dbbf`, the core built with `dev-test-games`; a test game of its own, hidden.
- The player's playset as the claim copied it, and a probe mod of an extension and an `events/` file. A new game as France, 1444.11.11, paused throughout.
- The probe mod was first given with `game start --mod`, which names it in the test game's `dlc_load.json` by its full path. The framework loaded its extension, but the game did not load its events: the game console's `event harmony_script_probe.1` answered "No event found". Naming it relative to the user folder, `mod/../../probe-mods/script-probe.mod`, through the claim's own folder rather than the player's `mod` folder, did not either. So the game's own events stood in for the probe's: events fired only by script whose trigger is one flag check, and whose options set flags or variables.

## What came out

- **As the player**, `script.flag` was refused `as_the_game`; every other request below was sent `--as-game`.
- **Flags the framework sets, seen by the game.** Each of these was refused "the trigger of event … does not hold" before the flag, and fired as the player's question after:
  - `privateers.13` (`has_country_flag = promised_merchants_war`), set on France;
  - `colonial.10` (`has_province_flag = rival_settlements`), set on Paris (183), fired as a province's event for its owner;
  - `flavor_rus.128` (`has_ruler_flag = peter_the_great`), set on France's ruler;
  - `consort_events.60` (`has_consort_flag = consort_was_thrown_party`), set on France's consort.
  Each flag read back `{"set":true,"day":"1444.11.11"}`. After `clear_flag`, France's read `{"set":false,"day":null}` and `privateers.13` was refused again. Clearing a name no script uses answered null.
- **Flags the game's effects set, read by the framework.** After answering `coptic_flavor.41`, France's `pleased_by_events` read set on 1444.11.11; after `shinto_events.39`, the heir's `christian_tutors`; after `flavor_swe.134`, the game's `evangelical_union_happened`, which read not set before.
- **The AI's event.** `flavor_gbr.12`, of one option, fired for Castile: its flag `eng_globe_theatre` read set at the next request, the game still paused. `coptic_flavor.41` fired for Castile left its flag unset: its first option's `ai_chance` is 0 unless the country thinks well of the event's FROM, which the framework's scope does not have, so Castile took the other option.
- **Variables.** A variable an effect set to 2.5 (`set_variable` in a file run by the game console's `run`) read `{"set":true,"value":2.5}`; France's `lua_var`, set to 12.25 by the framework, let `check_variable = { which = lua_var value = 12 }` through in that file and not `value = 12.3`. A province's variable set to 3.5 read 3.5; a name never set read `{"set":false,"value":0.0}`. The first build read the variables' list head as the list itself and failed `internal` ("the list … breaks, loops or holds more than 1048576 nodes"); fixed before the rest of the run.
- **The heir's and the game's flags the framework set**, `probe_h` and `probe_g`, were seen by `has_heir_flag` and `has_global_flag` in the same file, and read back set.
- **England, without an heir in 1444**: `set_flag heir:ENG` was refused "ENG has no heir", reading answered not set, clearing null.
- **Refusals**: a variable of a ruler and an event of the game `invalid_argument`, naming the scopes there are; `{"global":false}`, a name of two words and a value of 3000000 `invalid_argument`; `no_such.1` `not_found`; province 0 `not_found`; a tag no country has (`QQX`) `not_found`, both for a flag and for an event; firing an event in province 481, owned by no one, `not_found`. (`ZZZ` is a country in the player's playset, so it was not one to try.)
- **From the extension**, on the session's first day: `set_flag` answered true, the flag read `true 1444.11.11`, a name never set `false`, `set_variable` then `variable` `true 12.25`, a variable never set `false 0.0`, an unknown event `nil not_found`.
- The game did not crash; both of the claim's games ended by `game stop` and the release.

## Not verified

- A mod's own events, decisions and scripts reading what the framework set: the test game did not load the probe mod's content, so the game's own events stood in for it. Whether the game ever loads a mod named by a full path, or relative to the user folder through `..`, was not looked into; `--mod` and the scenarios' test mods carry extensions only.
- A flag set on a ruler while a country has none (kept by the country, `+0x2b0`), and a consort ruling as regent: no such country was tried.
- Events fired for the player in ironman; the sweep, which asks the group both ways (only unit tests ran it).
- Firing a province's event whose trigger needs its owner's country scope; `colonial.10`'s trigger asks the province only.
