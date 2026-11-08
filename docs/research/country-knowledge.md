# A country's knowledge, refreshed (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-08 as the third step of [game-speed.md](../game-speed.md), "How a part is chosen", for the candidate the plan called the diplomatic caches' refresh (`0x93fe30`): 3.6 ms of the main thread a game day late in a game, half after the commands between days and half in each country's daily update ([a late frame's drawing and the driver](../prototype-log/2026-11-08-a-late-frames-drawing-and-the-driver.md)). From its code (a Capstone script over the image, the direct calls mapped to functions through `.pdata`), the country's save writer, and an experiment build that counted every refresh in the game ([the refreshes counted](../prototype-log/2026-11-08-country-knowledge-refreshes-counted.md)).

In short: **`0x93fe30` is `CCountry::RefreshKnowledge`** (`country_knowledge.cpp`, its log line `CCountry::RefreshKnowledge, %s, %s`): it works out again, from the country's provinces and its relations with the other living countries, about seventy facts the rest of the game reads instead of working them out itself, among them the counts and averages that script triggers such as `num_of_cities` and `average_unrest` ask for. It runs either at once, from 228 places, or once a day for a country that asked for it. Late in a game it ran 11.7 times a game day at about 290 µs each, 3.5 ms a day; it walks only the country's own provinces and the living countries, and most refreshes change something, mostly values that move a little every day. **It is no large waste**: the ways to make it cheaper are bounded at about 1 ms a day, and the one that is not hard to build changes when the knowledge is new.

## What it works out

The facts it clears and builds again, by the names the country's save writer (`0x2dc6b0`) gives them, with their place in the country (0x9b78 bytes). The places come from the writer's code, each field read just before its key's token id (`mov edx, id`); the names from the tokens' records in the image's data (an id, then the name). Four of them agree with [dependency-teardown.md](dependency-teardown.md) (`support_independence` `+0x19a0`, `neighbours` `+0x1a90`, `home_neighbours` `+0x1aa8`, `core_neighbours` `+0x1ac0`); the rest are read so and not checked one by one.

| What | The facts, and where they are |
| --- | --- |
| Its provinces counted by kind | `num_of_cities` `+0x1fc8`, `num_of_provinces_in_states` `+0x1fcc`, `num_of_provinces_in_territories` `+0x1fd0`, `num_of_ports` `+0x1c70`, `num_of_core_ports` `+0x1c74`, `num_of_total_ports` `+0x1c78`, `num_of_non_cores` `+0x1f5c`, `num_of_controlled_cities` `+0x1998`, `num_of_colonies` `+0x2004`, `num_of_overseas` `+0x2008`, `num_of_heathen_provs` `+0x2018`, `num_of_heretic_provs` `+0x201c`, `num_of_rebel_controlled_provinces` `+0x198c`, `num_of_revolts` `+0x1990`, `non_overseas_development` `+0x2014` |
| Averages over its provinces | `average_unrest` `+0x2030`, `average_effective_unrest` `+0x2034`, `average_autonomy` `+0x2038`, `average_autonomy_above_min` `+0x203c`, `average_home_autonomy` `+0x2040`, `inland_sea_ratio` `+0x2044` |
| Its relations, as lists of countries | `allies` `+0x19b8`, `extended_allies` `+0x19d0`, `support_independence` `+0x19a0`, `coalition_friends` `+0x19e8`, `trade_embargoed_by` `+0x1bd8`, `trade_embargoes` `+0x1bf0`, `call_to_arms_friends` (a vector) `+0x1ad8`; and their counts: `num_of_allies` `+0x1fe4`, `num_of_independence_supporters` `+0x1fe8`, `num_of_royal_marriages` `+0x1fec`, `num_of_subjects` `+0x1ffc`, `num_of_trade_embargos` `+0x1fd8` |
| Its geography | `neighbours` (a vector) `+0x1a90`, `home_neighbours` `+0x1aa8`, `core_neighbours` `+0x1ac0` (lists of the game's kind), `border_provinces` (a vector, `+0x2338`, filled by `0x2d0a3f` comparing provinces' owners at `+0x46c`), `border_pct` `+0x1c38`, `border_sit` `+0x1c50` |
| The rest | its leaders counted (`num_of_leaders` `+0x1f68` and two more), its buildings and goods by kind (`num_of_buildings_indexed` `+0x2060`, `produced_goods_value` `+0x20a8`), its religions by kind (`num_of_religions_indexed` `+0x20c0`), and arrays of 16-bit counts at `+0x10aa`..`+0x10d2` that are not written to saves |

**Who reads them**: the script's triggers (`num_of_cities` and its kin are names of triggers too; `0x6a7af0` loads that name, _inferred_ to be its trigger), the AI, the interface and the rest of the day, each instead of walking the country's provinces and relations again. What every reader expects of how new they are was not read.

**What it walks**: the country's own provinces (its lists at `+0x1338` and `+0x1378`, each province found in the world's array at `[world+0x1ca8]` by its index, 0x2e10 bytes apiece), and every living country (the world's array at `[world+0x1d08]`, its count at `+0x1d14`: 118 countries in 1780, against 1,380 tags), each with the record of the two countries' relation (`country+0x1510`, 0x48 bytes by the other's index). Not all 4,942 provinces, nor every tag.

**When it does nothing**: while `[0x233fe78]+0x7485` is set, when `0x7495e0` says no, and while `[0x233fea0]+0x40` is set ([dependency-teardown.md](dependency-teardown.md)).

## When it runs

**At once**: 228 direct calls, from 30 source files, each passing a label of its own (`file.cpp:line`), and one that passes none (`0x75f160`): 90 in `diplomaticaction.cpp` (a diplomatic action refreshes both countries), 18 each in `countrycommands.cpp` and `constructioncommands.cpp`, 16 in `country.cpp`, 12 each in `effectimplementation.cpp` and `ConsoleCmdImpl.cpp`, 7 in `peacetreatyimplementation.cpp`, and from one to five in each of 23 more files. Among those that refreshed most in the game (below): recruiting a regular unit (`0x26cfe0`, `constructioncommands.cpp:1187`, [recruiting.md](recruiting.md)), choosing an event's option (`0x6f2700`, `eventcommands.cpp:436`), spending monarch power (`0x2a7310`, `countrycommands.cpp:1466`, [power-spending.md](power-spending.md)).

**Asked for, once a day**: about 50 places set the country's `needs_refresh` (`+0x2882`) and their label at `+0x2858`, among them a unit's construction finished (`0x251df0`, `construction.cpp:890`) and changes to a province (`0xa00270`, `0xa015c0` and others in `province.cpp`). Each country's daily update (`0x2ee280`) then refreshes the country once when the byte is set (`0x2f0854`), with the label of the last place that asked (or its own, `country.cpp:9209`), makes the country's modifiers again (`0x327dc0`) and clears the label. The refresh clears `needs_refresh` as it begins (`0x93ff8e`), so a refresh at once also answers a request made before it. So the game already has the way to put a refresh off to the day: the places that refresh at once do not use it.

## How often, and what it changes

Counted in the game on 2026-11-08 by an experiment build, not kept ([the refreshes counted](../prototype-log/2026-11-08-country-knowledge-refreshes-counted.md)): the 1780.7.1 save in observe mode in the Ottomans' view at speed 5, 2,500 days a run, every refresh on the main thread through the framework's own observer of it (the repair of [dependency-teardown.md](dependency-teardown.md) hooks `0x93fe30`); a refresh asked for by `needs_refresh` told by its label.

- **11.7 refreshes a game day after the first 250 days, about 290 µs each, 3.5 ms a day**, which the sampler's 3.6 agrees with. The first 250 days after the load had 27 to 31 a day: right after a load the game refreshes every country (`gamestatecommands.cpp:450`, 2,760 calls).
- **42% were asked for and done in the daily updates, 58% at once.**
- The largest, in the first run, by their time:

| Label | Calls in 2,500 days | Asked for | µs each | ms a day |
| --- | --- | --- | --- | --- |
| `constructioncommands.cpp:1187` (recruiting a regular unit) | 5,103 | no | 321 | 0.66 |
| `country.cpp:29232` | 7,521 | yes | 216 | 0.65 |
| `construction.cpp:890` (a unit's construction finished) | 2,986 | yes | 491 | 0.59 |
| `eventcommands.cpp:436` (an event's option) | 1,627 | no | 513 | 0.33 |
| `countrycommands.cpp:1466` (spending monarch power) | 1,247 | no | 378 | 0.19 |
| `country.cpp:29220` | 1,482 | yes | 210 | 0.13 |
| `constructioncommands.cpp:306` | 594 | no | 466 | 0.11 |
| `gamestatecommands.cpp:450` (after the load) | 2,760 | no | 94 | 0.10 |
| `countrycommands.cpp:1480` | 1,247 | no | 206 | 0.10 |
| `unit.cpp:355` | 570 | yes | 437 | 0.10 |

- **What changed** (the second run): each refresh's country compared before and after it, word by word, with the items of its 15 lists and the bytes of its 6 vectors; the 11 words that change at every refresh (found by refreshing the first 300 twice) left out. **22% of the refreshes after the first 250 days changed nothing** (the first 250 days' counts taken from a later, shorter run of the same save). A country refreshed again on a day it had been refreshed already: 28% of the refreshes, and 37% of those changed nothing.
- **What changes most**, of 33,851 refreshes: `power_projection` (`+0x2280`) in 42%, `average_unrest` (`+0x2030`) in 30%, the 16-bit counts at `+0x10a8`..`+0x10d0` in 4% to 19%, `average_home_autonomy` and `average_autonomy` in 8% to 9%, `total_war_worth` in 3%; of the lists and vectors, `core_neighbours` (510), `neighbours` (500), the list at `+0xce0` (321), `trade_embargoed_by` and `trade_embargoes` (184, 177). So a refresh that changes something mostly finds values that move a little from day to day, and now and then a neighbor or an ally gained or lost.

## Where its time goes

Sampled on the same save ([a late frame's drawing and the driver](../prototype-log/2026-11-08-a-late-frames-drawing-and-the-driver.md)), of the refresh's time: its own code 37% (its walks of the provinces and of the living countries), `0xa1d330` 14% (once for each province of the country, `0x941f89`), `0x93fb80` → `0x2ffc60` → `0x300a70` 13%, `0x9fabe0` 8% (a province's), `0x3d7c00` 6%, `0x316430` 4%, the rest under 3% each. No one part is most of it.

## What this means for making the game faster

Its bound is its 3.5 ms of a 69 ms day, 5%. The ways, each with what it could give:

1. **The refreshes at once asked for instead**, as the game's own `needs_refresh` does, so that a country refreshed several times in a day is refreshed once in its daily update: up to the 28% of refreshes that came on a day the country had had one, about 1 ms a day, 1.4%. A difference of kind C: whatever reads the country's knowledge between the command and the daily update reads it as it was before, such as the AI counting its units after recruiting; under [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/the-game-may-be-made-faster-with-reasoned-differences.md) a difference with its reason, which needs every reader of the knowledge read for what it expects.
2. **A refresh left out when nothing it reads has changed**: up to the 22% that changed nothing, about 0.8 ms a day; it would need to know every input of the seventy facts, which this note does not.
3. **A refresh of only what its cause changes** (recruiting changes no ally): not bounded here; it would need, for each of the 228 places, what it changes.

So this candidate is small: about 1 ms a day for the way that is not hard to build, which changes when the knowledge is new. It does not come ahead of the AI stage's wait or of the rest of each country's daily update.

## Not verified

- The places of the facts other than the four checked: read from the order of the writer's code.
- Which of the refresh's callees writes `power_projection`; what the 16-bit counts at `+0x10aa`..`+0x10d2` and the list at `+0xce0` are; what `0xa1d330`, `0x2ffc60` and `0x9fabe0` work out.
- What changed, label by label: the first run's comparison found no word that changes at every refresh (it was made during the load, on its thread), so its counts of what changed are not used; the second run's counts by label were not kept.
- Whether a refresh that changed nothing could have been known to change nothing beforehand.
- One save, in observe mode, 2,500 days; a game a player plays.
