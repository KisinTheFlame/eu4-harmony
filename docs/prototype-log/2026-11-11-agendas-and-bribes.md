# Estate agendas and parliament's bribes

Date: 2026-11-11. Build: this checkout's release build with `dev-test-games`, on the branch of [mod-capabilities.md](../plans/mod-capabilities.md), M20. What was found in the game's code is in [research/government.md](../research/government.md#estates) ("Estate agendas", "The seats", "The bribes' database", "Paying a bribe").

## What was tried

A test game of its own, hidden, the 1700.1.1 save of the AI-only game (`observer-1700.1.1`) loaded as the Ottomans, requests sent through `eu4-harmony game`.

- **No agenda.** `estate.agenda` answered `agenda: null` for the Ottomans.
- **An agenda taken up.** `script.fire_event` (as the game) fired `estate_privileges_and_agendas_events.3` for the Ottomans; `question.list` gave it with five options, one an estate's proposal each, and `question.answer` took the ulema's. `estate.agenda` then answered `estate_church_develop_x` of `estate_church`, expiring 1720.1.1, twenty years on, not met, with the lines `布尔加斯:` and `基础税收至少为4¤`: Burgas, which the agenda's script picked, is in the agenda's own scope, and its base tax was 2 (`province.get 1764`).
- **Its requirement met.** After the console's `set_base_tax 1764 4`, `estate.agenda` answered `met: true`, `unmet` empty. 35 days later the game had completed it: `agenda: null`, with no question waiting.
- **A parliament.** After the console's `tag GBR`, `parliament.seats` gave Great Britain's seats, each with the bribe it asks and refused while no debate was going on.
- **A bribe's conditions in the seat's province.** The first build asked a bribe's `trigger` in the country's scope alone, and Cornwall's (233) `grant_local_autonomy` read refused with `大不列颠:` `省份不是首都省份`: the bribe's conditions are on `FROM`, the seat's province. Asked as the bribe's execute asks them, the player's scope with the province's as `FROM`, the same seat read no such line.
- **A debate.** `parliament.debate reduce_trade_regulations` started it; the seats' bribes were drawn again and there were 18 seats, 10 backing it, as many as the seats marked backing, 12 needed, those not backing payable.
- **A bribe paid.** Seat 234 asked `lose_navy_tradition`: `parliament.bribe 234` brought the navy tradition from 82.579 to 81.579, the seat to backing and the count to 11; paying it again was refused, `the seat backs the issue already`; `parliament.bribe 999` was `not_found`.
- **The debate passed.** Paying seat 4372's `diplomatic_support` made 12 of 18: `parliament.options` then gave no debate and `reduce_trade_regulations` enacted, and the seats were refused as no debate was going on.
- **The sweep**, `cargo xtask scenario sweep --sweep.only estate,parliament`: passed, 12 capabilities in 5 places, 135 requests.
- The game did not crash; the claim was released.

## Not verified

- A bribe refused for its `trigger` in the game: no seat in the debate asked one whose conditions failed. The refusal is covered by a unit test only.
- Whether the parliament view shows the bribe button disabled for a failing `trigger`, or for a seat asking none; the framework's own words stand for those.
- The agenda's name keeps the game's text commands (`提高[agenda_province.GetName]税收`); the framework does not fill them in.
- An agenda failed by its `fail_if` or by expiring, and one of another estate than the clergy.
- Calls from an extension; everything was sent from the command line.
