# Mod capabilities plan

Status: in progress. Decided with the owner (2026-11-11): what is open to mods now, the roadmap of what is not, and the parts taken up first, M1 to M20. M1 to M15 built and tried in the game (2026-11-11: [M1's run](../prototype-log/2026-11-11-signals.md), [M2's to M4's](../prototype-log/2026-11-11-country-rulers-legitimacy-modifiers.md), [M5's](../prototype-log/2026-11-11-opinions.md), [M6's and M7's](../prototype-log/2026-11-11-province-and-army-reads.md), [M8's](../prototype-log/2026-11-11-mechanics-and-advisors.md), [M9's](../prototype-log/2026-11-11-requests-as-the-game.md), [M10's to M12's](../prototype-log/2026-11-11-script-flags-variables-events.md), [M13's and M14's](../prototype-log/2026-11-11-tasks-and-game-days.md), [M15's](../prototype-log/2026-11-11-army-organization.md)); the others not built yet. Terms it added to [GLOSSARY.md](../../GLOSSARY.md): Modifier, Named modifier, Legitimacy, Government mechanic, Mechanic power, Opinion, As the player, As the game, Event (now the scripts'), Signal (the framework's, formerly its event). The decisions are [ADR requests-act-as-the-player-or-as-the-game](../adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md), [ADR framework-notifications-are-signals](../adr/2026-11-11-framework-notifications-are-signals.md), [ADR changes-as-the-game-are-typed-capabilities](../adr/2026-11-11-changes-as-the-game-are-typed-capabilities.md) and [ADR tasks-are-awaited-like-promises](../adr/2026-11-11-tasks-are-awaited-like-promises.md); it builds on [player-actions.md](player-actions.md) and [ADR mods-hold-the-logic](../adr/2026-11-03-mods-hold-the-logic.md).

## Goal and scope

- **Goal**: one place that says what the framework opens to mods, in five areas (interface, reads, actions, signals, the Lua host), what each still lacks, and in what order it is taken up. The plan stays in progress and takes new parts as the roadmap is worked through, as [game-speed.md](game-speed.md) does.
- **In scope now**: the signals renamed (M1); the reads grown (M2 to M8); the script group and requests as the game (M9 to M12); tasks awaited as promises are, and waiting for game days (M13, M14); every action of the game's panels the catalogue still lacks (M15 to M20).
- **Out of scope**: multiplayer; changing how the game computes (combat, the AI's choices, its rules), which stays outside the catalogue ([ADR mods-hold-the-logic](../adr/2026-11-03-mods-hold-the-logic.md)); running effect or trigger text ([ADR changes-as-the-game-are-typed-capabilities](../adr/2026-11-11-changes-as-the-game-are-typed-capabilities.md)); events put together in Lua (a mod defines its events in its scripts, and fires them); extensions calling each other or sharing code ([lua-extensions.md](lua-extensions.md)).

## What is open now

Each capability group in one line; its arguments, results and refusals are `eu4-harmony wiki`'s. Groups marked *remote* are remote control's only. A remote client's request acts as the player, reading the player's view, unless it asks to act as the game (`--as-game`), reading the full view as extensions do (M9).

### Interface

- Interface fragments (`eu4-harmony/ui/<window>/`) spliced into a host window, with buttons laid out in a button area; the only host window is the single army panel (`single_unitpanel`). `game.ui.<element>` sets an element's visibility, whether it is enabled, its text and tooltip.
- Extension windows (`game.window`): titles, texts, icons, buttons, progress bars, flags and lists, opened from an entry button on the panel button strip ([extension-windows.md](extension-windows.md)).
- Pop-ups (`game.popup`; `popup.list`, `popup.answer`).
- The map painted by province (`game.map.paint`, `game.map.release`).
- `camera`: where the camera looks, looking at a map position, zooming. `screenshot`, `input` and `desktop` (*remote*): a picture of the frame, the player's mouse and keyboard, showing and hiding the game.

### Reads

- `session`: the session, the country acted as, putting a country in view in observe mode. `time.get`: the date, whether paused, the speed. `text.language`: the player language.
- `country.get`: treasury, manpower, stability, prestige, monarch points, wars, relations, technology, ideas, governing capacity, inflation, mercantilism, loans, rebels; the ruler, heir and consort; legitimacy of the government's kind and what it adds, corruption, absolutism, army and navy tradition, army professionalism; named modifiers. `country.budget`: the month's income and expenses. `country.hostile`: the countries at war with one. `country.modifiers`: what a country has of each modifier, by its key. `country.opinion`: what one country thinks of another, the opinion's parts, the aggressive expansion, trust and favors; `country.opinions`: every country's of one.
- `province.list`, `province.get` (owner, controller, development, terrain, fort, buildings, autonomy, culture, religion, trade good, unrest, trade node, state, edict, cores, claims, constructions, prosperity, devastation, manpower, supply limit, separatism, institutions, named modifiers), `province.neighbors`, `province.modifiers` (what a province has of each modifier, of its own).
- `army.get`, `army.list`, `army.at` (each with its units by kind, strength, morale, leader and maintenance), `army.regiments`, `army.selected`; the game's rules for moving: `army.route_to`, `army.can_step`, `army.can_attack`.
- `trade.nodes`, `trade.node`; and each panel's own list: `leader.list`, `loan.list`, `fort.list`, `estate.list`, `idea.list`, `decision.list`, `mission.list`, `policy.list`, `mercenary.list`, `merchant.list`, `question.list`, `offer.list`.

### Actions

Each group reads what its panel offers now (`options` or `list`, each entry a `Choice` with the game's reasons and its cost) and does what its buttons do ([player-actions.md](player-actions.md)).

- Armies and war: `army` (move, stop, split, merge, disband, consolidate, drill, a fleet's missions; detaching a kind or special units, attaching to another unit, to transport, a fleet's automatic transportation), `recruit`, `leader`, `mercenary`, `siege`, `fort`, `occupation`, `peace` (terms, evaluating them, demanding, offering).
- Provinces: `province` (develop, exploit, autonomy, state or territory, edicts, cores, culture), `building`, `center_of_trade`, `colonist`, `missionary`.
- The country: `technology`, `idea`, `stability`, `economy` (loans, inflation, mercantilism), `loan`, `focus`, `government` (reforms, rank, government mechanics and their interactions), `advisor` (hiring, firing, promoting), `estate`, `parliament`, `policy`, `decision`, `mission`.
- Trade: `merchant`, `trade_company`.
- Other countries: `diplomacy` (every action of the diplomacy panel, the proposals answering with the other country's answer), `subject`.
- Answering: `question`, `offer`.
- Beyond the panels: `console.run`, the game console with its own checks; `menu` (*remote*: saves, loading, a new game, back to the main menu); `time` (pausing, resuming, the speed; waiting for days to pass; advancing days, *remote*).
- As the game, what the scripts do and no player could: `script` (reading, setting and clearing a country's, a province's, a monarch's or the game's flags; reading and setting a country's or a province's variables; firing an event).

### Signals

`on_session`, `on_session_ended`, `on_country_in_view`, `on_popup`, `on_day`, `on_month`, `on_year`, `on_army_ordered`, `on_army_defeated`, `on_war_declared`, `on_peace_made`, `on_province_owner_changed`, `on_building_finished`, `on_battle`. Remote clients read them numbered (`signals`).

### The Lua host

Handlers for each signal and `on_button`, `on_window`, `on_map_lost`, `on_frame`, `save_data`; tasks that wait for a capability's answer or for days in the game (`game.time.wait`), started with `spawn` or an async function (`async`), awaited one at a time or several at once (`await_all`, `await_any`) and cancelled; `require`, `json`, `log`, `print`, `text`, `mod_files`; the permissions `clock`, `files` and `network` (`http.fetch`); extension save data in the save; extension reload; the fake game for `eu4-harmony test` ([lua-extensions.md](lua-extensions.md), [extension-reload.md](extension-reload.md)).

## Roadmap

What each area still lacks, as found on 2026-11-11. The parts of this plan take some of it up; the rest waits for later parts.

- **Interface** (later): host windows beyond the army panel (the province view, the country and diplomacy panels, the top bar); check boxes, text input, tabs and grids across in extension windows, and windows in the main menu; marks, arrows, icons and text on the map, and lines in the game's tooltips; reading what is selected or under the mouse besides an army (a province, a country); keys bound by a mod.
- **Reads**: rulers, heirs, consorts and advisors; legitimacy and the values every country has (corruption, absolutism, traditions, professionalism); modifiers, summed and named; opinion, aggressive expansion, trust and favors; government mechanics; a province's prosperity, devastation, local manpower, supply limit, separatism and institutions; an army's units and their strength and morale; flags and variables (M2 to M10).
- **Actions**: the panels' actions the catalogue lacks (M15 to M20); changes as the game, typed (later: adding to a country's treasury and the like, as mods need them, [ADR changes-as-the-game-are-typed-capabilities](../adr/2026-11-11-changes-as-the-game-are-typed-capabilities.md)); setting flags and variables and firing events (M10 to M12).
- **Signals** (later; the rename was M1): a save finished writing; a question or an offer coming to the player; a ruler's or an heir's change; a technology or an idea taken; rebels rising; a siege ended; a claim, a core or a colony finished. Today a mod polls `question.list` and `offer.list` for them.
- **The Lua host**: awaiting and cancelling a task, and awaiting several (M13); waiting for game days (M14); extensions calling each other or sharing code stays out of scope.

## What the game does

What is known of each part's game side is in [research/](../research/), as found on 2026-11-11:

- Rulers, heirs and consorts are one monarch layout, at country `+0x1280`, `+0x1298` and through `0x30f660`; their names are read, their skills and dynasty are not ([research/names.md](../research/names.md)). Advisors, legitimacy and the values every country has are not researched; the country's save writer (`0x2dc6b0`), which writes each field under its token, is the way to find them.
- A country's modifier sums and how they are rebuilt are known ([research/countries.md](../research/countries.md), `core/src/modifier_sums.rs`); the name of each sum, and the list of named modifiers, are not.
- Each country's relations with every other are a table at country `+0x1510`, with favors and trust in it ([research/diplomacy.md](../research/diplomacy.md)); opinion's parts and aggressive expansion are not read.
- Prosperity and devastation are found ([research/provinces.md](../research/provinces.md)); the province's save writer (`0x9d6e60`) is the way to the rest.
- A regiment's strength, maximum and type are found, and detaching by type, attaching, a transport and its auto-transport are researched commands ([research/units.md](../research/units.md)); morale and maintenance are not.
- Government mechanics' interactions are a command found and not offered (`government_interaction_command`, [research/government.md](../research/government.md)); so are decrees, estate agendas, parliament bribes, native and trading policies.
- The province view's and the country's power spendings (moving the capital or the trade port, harsh treatment, scorched earth, seizing or burning a colony, attacking natives, sorties, the Siberian frontier, tribal land, war exhaustion, war taxes, tariffs, bankruptcy, a primary or accepted culture, a religious reform) are kinds of one spending command whose check is known ([research/power-spending.md](../research/power-spending.md)); each button's reasons are not.
- Firing an event is a function found ([research/questions-and-offers.md](../research/questions-and-offers.md)), looking one up by its id is not. Where flags and variables are kept is not known ([game/scripts.md](../game/scripts.md)).
- Religions' own mechanics, institutions, the ages' abilities, monuments, the Empire's reforms and the Celestial Empire's decrees are not researched.

## Behavior

### Signals (M1)

- What the framework tells of the game is a signal ([ADR framework-notifications-are-signals](../adr/2026-11-11-framework-notifications-are-signals.md)); remote control reads them with `signals`, as it read `events`, with the same arguments; its answer lists them under `signals`, each naming its kind in `signal` (`events` and `event` before), and so does `time.advance`'s. Handlers keep their names: no extension changes.
- "Event" is the scripts' from then on, in the catalogue, the wiki, the guides and the code.

### Reads

- `country.get` grows with `ruler`, `heir` and `consort` (each a name, a dynasty, administrative, diplomatic and military skill, an age; none when there is none); `legitimacy` (`kind`, one of `legitimacy`, `republican_tradition`, `devotion`, `horde_unity`, `meritocracy`, with `value`, `max` and `effects`); `corruption`, `absolutism`, `army_tradition`, `navy_tradition`, `army_professionalism`; and `named_modifiers` (each a key and the date it ends, none for one that does not).
- `effects` is what a value adds to the modifiers now, a list of `{modifier, value}`, as the game's tooltip of it totals it; how the value is turned into them is the game's scripts', and is not given.
- `country.modifiers(country, keys)` and `province.modifiers(province, keys)` answer what the country or the province has of each modifier asked for, by its key.
- `country.opinion(of, toward)` answers what `of` thinks of `toward`: `opinion`, its `parts` (each a `text` as the diplomacy view's tooltip writes it, a `value`, and `yearly`, how much it changes a year, for a part that does), `aggressive_expansion`, `of`'s `trust` in `toward` and the `favors` `of` owes `toward`. `country.opinions(toward)` answers the same of every country that owns a province toward one, without the parts. As the player, the trust and the favors are null unless one of the two is the country acted as; the opinion and its parts are shown of any two, as the opinion map mode shows them.
- `province.get` grows with `prosperity`, `devastation`, `local_manpower`, `supply_limit`, `separatism` and `institutions` (each institution's key and its progress there).
- `army.get` grows with its regiments by kind (infantry, cavalry and artillery; a fleet's heavy, light, galley and transport ships), `strength` and its maximum, `morale` and its maximum, its `leader` and its monthly `maintenance`. `army.regiments(army)` lists each regiment, in the game's order: its ID, name, kind, unit type (a content key), strength and morale.
- `government.mechanics()` lists the player's government mechanics, each by its content key, with its mechanic powers (each a key, a `value` between `min` and `max`, and its `effects`) and its interactions as `options` entries: a `Choice` and, where it has a cooldown, `ready_on`, the day it can be used again. `government.interact(mechanic, interaction)` uses one, as its button does. What an interaction does is not given.
- `advisor`: `advisor.list` (the player's advisors and those to hire), `advisor.options`, `advisor.hire`, `advisor.fire`, `advisor.promote`, shaped as `leader`.
- What a read leaves out as the player is compared with the game's panels, part by part, as [player-actions.md](player-actions.md) did.

### Scripts, and acting as the game

- The `script` group is what scripts and extensions share. A scope is a table of one key: `{ country = "FRA" }`, `{ province = 183 }`, `{ ruler = "FRA" }`, `{ heir = "FRA" }`, `{ consort = "FRA" }` or `{ global = true }`; each operation takes the scopes the game has for it.
- `script.flag(scope, name)` answers whether the flag is set and the day it was set (`{set, day}`); `script.set_flag(scope, name)` and `script.clear_flag(scope, name)` set and clear it, as `set_country_flag` and `clr_country_flag` (and their kin for the other scopes) do.
- `script.variable(scope, name)` answers whether the scope has the variable and its value, 0 when it has none as the scripts read it (`{set, value}`); `script.set_variable(scope, name, value)` sets it, as `set_variable` does.
- A read answers a table rather than nil for a flag or a variable not set: in Lua a capability's result is nil only with `err`, and an answer of nothing is `true`.
- `script.fire_event(scope, id)` fires an event for a country or a province at once, as `country_event = { id = … }` or `province_event` does: the game checks it and has the AI choose its option as for any event, and for the player it waits as a question. `not_found` for an id no event has. There is no delay: a task waits first (`game.time.wait`).
- A remote client's request acts as the player unless it asks to act as the game: `--as-player` (the default) or `--as-game` on the command line, `"as"` on the pipe. As the player it reads the player's view; as the game the full view, and it may use the `script` group, which as the player is refused with `as_the_game`. The setting "Remote control sees everything" is removed. Extensions read the full view and use the `script` group always ([ADR requests-act-as-the-player-or-as-the-game](../adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md)).

### Tasks

- `spawn(fn, ...)` starts a task and returns it; `async(fn)` returns a function that starts one at each call and returns it.
- `t:await()` waits for the task's end and returns what `fn` returned; an error in it is raised again there, and a cancelled task raises one with the code `cancelled`. `t:cancel()` ends it at the wait it is in, taking back nothing it already asked of the game; cancelling a task that ended does nothing. `t:done()` says whether it ended.
- `await_all(tasks)` returns each task's results, raising the first error; `await_any(tasks)` returns the number of the first to end and its results.
- `game.time.wait(days)` waits until that many days have passed in the game; days paused do not count, and the session's end drops the task as any.
- The editor's types make a task generic, `eu4.Task<T>`, so that `t:await()` has `fn`'s type. `eu4-harmony skills` gets a chapter on tasks, built on these examples:

```lua
-- 1. A capability that waits needs no await.
on_button = function(game, name)
  local reply, err = game.diplomacy.propose_alliance("CAS")
  if not reply then
    log("not proposed: " .. err.message)   -- the game said no: nil, err
    return
  end
  log("Castile " .. reply.answer)          -- accepted or declined
end

-- 2. Several things at once.
on_button = function(game, name)
  local tasks = {}
  for _, tag in ipairs({ "CAS", "POR", "ARA" }) do
    tasks[#tasks + 1] = spawn(game.diplomacy.improve_relations, tag)
  end
  local results = await_all(tasks)
end

-- 3. An async function, and game days.
local check_later = async(function(game, tag, days)
  game.time.wait(days)
  return game.country.get(tag).stability
end)

on_button = function(game, name)
  local t = check_later(game, "FRA", 30)
  log("France's stability in 30 days: " .. t:await())
end

-- 4. Cancelling: an order by hand takes the army back.
local pending = {}

on_army_ordered = function(game, army)
  if pending[army] then pending[army]:cancel() end
  pending[army] = spawn(function()
    game.time.wait(10)
    game.army.stop(army)
    pending[army] = nil
  end)
end

-- 5. A time-out.
local reply = spawn(http.fetch, "https://api.example.com/plan")
local timer = spawn(game.time.wait, 5)
local first = await_any({ reply, timer })
if first == 2 then reply:cancel() end

-- 6. Errors and cancelling, caught.
local ok, err = pcall(t.await, t)
if not ok and not (type(err) == "table" and err.code == "cancelled") then
  error(err, 0)
end
```

### Actions of the panels

Each follows [player-actions.md](player-actions.md): a group per panel, `options` or `list` with a `Choice` for each entry, the action named after its button, the game's reasons for a refusal. In the owner's order:

- **Army organization** (M15): detaching regiments of a kind, attaching an army to another and detaching it, boarding a transport and leaving it, auto-transport, detaching mercenaries.
- **The province view's spendings** (M16): moving the capital and the trade port, harsh treatment, scorched earth, seizing or burning a colony, attacking natives, sorties, the Siberian frontier, tribal land, razing.
- **Culture** (M17): the primary culture, accepting and removing a culture, promoting one where the government has a culture capacity.
- **Religion** (M18): the religious reforms and every religion's own mechanic (personal deities, fervor, Hindu deities, Shinto, harmonizing, karma, patriarch authority, church aspects, Tengri's syncretic faith), the curia, holy orders.
- **The empires** (M19): the Holy Roman emperor's reforms and incidents, the Celestial Empire's decrees and reforms.
- **Institutions, ages and monuments, and the rest** (M20): embracing an institution, the ages' abilities and splendor, upgrading a monument; reducing war exhaustion, war taxes, tariffs, bankruptcy, releasing a colonial nation, the defender of the faith, strengthening the government, a faction's boost, estate agendas, parliament's bribes, native and trading policies, naval doctrines.

## How it fits together

- The capability catalogue (`remote/src/`) gains the new reads, the `advisor` and `script` groups and the panels' groups; `signals` replaces `events` there. Each capability says whether it needs a request as the game, and `describe` and the wiki say so.
- The core's capabilities (`core/src/game/`) read the game and post commands through the one command layer ([ADR one-command-layer-in-written-form](../adr/2026-11-03-one-command-layer-in-written-form.md)); the `script` group's writes call the game's own functions for flags, variables and events, outside the command layer, as the scripts' effects do.
- The pipe's request carries `as`; the remote control engine takes the view from it instead of the setting, and refuses a capability as the game to a request as the player.
- Tasks are the Lua host's (`lua/src/`): the prelude gives `spawn`, `async`, `await_all` and `await_any`, a task's handle, its cancelling, and waiting for game days beside waiting for an answer; the fake game has them too.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and both clippy runs pass, and so do the offline checks against `eu4.exe`;
- what it delivers was tried in the game, its refusals among them, with the run recorded in a file of its own in [prototype-log/](../prototype-log/), saying as plainly what was not verified;
- docs/research holds what was found in the game's code and the page of docs/game it touches says what changed of what is known; every new place in the game is a dependency point;
- the fake game has it, `eu4-harmony wiki`, the editor's types and the guides say it, its read is described for both views, the sweep asks it, and README.md and README.zh-CN.md say what changed for players and mod authors;
- its section below says what it built, and "What is open now" above gets its line.

### M1 Signals

The framework's events renamed signals throughout: `signals` for `events`, the wiki, the guides, the editor's types, the code, the scenarios; GLOSSARY.md and the documents now in use.

Done (2026-11-11): the catalogue's group is `signals` (`remote/src/signals.rs`, its `Signal`, `Signals` and `SIGNALS`), its answer's list is `signals` and each item's kind is in `signal`, as in `time.advance`'s answer; a read naming no kind is refused with "no signal is called …". In the core, the remote control engine keeps them in `catalogue::signals`, the frames make them in `game::signals` and `game::frame`, and their cost is logged as `signal costs:` (`game::signal_costs`); the scenario `perf` takes `--perf.signals`. `eu4-harmony wiki signals` lists them, and the guides, README.md, README.zh-CN.md (处理函数, 信号), adding-a-capability.md, dev-tools.md and performance.md's description of `perf` use the new words. Handlers and the kinds' names are as they were. What stays "event" is the game's: questions (`question.list` gives each one's `event`, its id), event windows, and the records. Tried in a test game: [the run](../prototype-log/2026-11-11-signals.md).

### M2 Rulers, heirs and consorts

`ruler`, `heir` and `consort` in `country.get`.

Done (2026-11-11): each a given name with the regnal number in it, the dynasty's name (null for none), the three skills and the age, null where the country holds the game's null person; the age is the game's own (`0xa4e2d0`), whole years from the birth date, so a person the game made with no birth date is centuries old, as Qing's ruler of a 1760 save is. Both views show them, as the diplomacy view shows another country's ruler (not compared). The person's fields are in `objects::campaign::person`, which the names' wording reads too (docs/research/names.md). Tried: [the run](../prototype-log/2026-11-11-country-rulers-legitimacy-modifiers.md).

### M3 Legitimacy and the values every country has

`legitimacy` with its `effects`; `corruption`, `absolutism`, `army_tradition`, `navy_tradition`, `army_professionalism`.

Done (2026-11-11): the kind is asked as strengthening the government asks it (a horde, meritocracy, a monarchy, a republic, devotion), the value read as the game shows it (a junior partner's is its senior partner's), whole at 100; `effects` is the set the country's modifiers made for it when they were last made again, and a republic's second set, each modifier by its key, none for a legitimacy, horde unity or devotion at exactly 50, which adds none (docs/research/countries.md, docs/research/modifiers.md). Army professionalism is a fixed point of 15 bits, in percent. In the player's view another country's corruption, absolutism, professionalism and effects are null; its legitimacy and traditions show, which was not compared with its diplomacy view or the ledger. Tried: [the run](../prototype-log/2026-11-11-country-rulers-legitimacy-modifiers.md); legitimacy's effects compared only for monarchies.

### M4 Modifiers

`country.modifiers`, `province.modifiers`; `named_modifiers` in `country.get` and `province.get`.

Done (2026-11-11): both read through the game's own getter of one modifier (`0x8e6bd0`), keys found among the kinds the game builds and refused `not_found` when they name none; `country.modifiers` answers `{country, values}`, values null in the player's view for another country (a capability's whole answer cannot be null, as `nil` means a refusal in Lua), and `province.modifiers` the province's own set, not what its owner adds across the country, refusing a province not discovered in the player's view. A named modifier is its key and the day it ends, null for one that does not (the game keeps −1.1.1 for those given for good); a hidden one is left out in the player's view, and another country's are null there. Entries the game leaves out of saves (their set's byte `+0x151`) are read as the others; what that byte means was not followed (docs/research/modifiers.md). Tried: [the run](../prototype-log/2026-11-11-country-rulers-legitimacy-modifiers.md): totals against the game console's `country_modifier` and `province_modifier`.

### M5 Opinion

`country.opinion`, `country.opinions`.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-opinions.md)). The opinion is the game's own getter's, the number the panels show; the parts are the tooltip's lines as the game's count writes them, read back into text, value and change a year, a line with no value kept with the part above it (the provinces a country covets); aggressive expansion is the sum of the `aggressive_expansion` opinion modifiers of the one's opinion of the other; trust and favors are the one's record of the other ([research/diplomacy.md](../research/diplomacy.md#opinion)). A part's `yearly` was added to the plan's shape, since the tooltip shows it. `country.opinion` refuses a country's opinion of itself (`invalid_argument`), for which the game shows none. The fake game's world takes `opinions`, each `of`, `toward` and the fields to answer.

### M6 Provinces

`prosperity`, `devastation`, `local_manpower`, `supply_limit`, `separatism`, `institutions` in `province.get`.

Built (2026-11-11). `province.get` has `prosperity` (its state's, in percent; null in a territory), `devastation` (percent), `local_manpower` (thousands of men, as the province view's manpower), `supply_limit` (regiments, for the country acted as, the owner's while observing with none in view), `separatism` (years left) and `institutions` (each institution that has appeared, its key and the province's progress in percent). The province view shows another country's province with its manpower, supply limit and devastation, and its state tab its prosperity, so the player's view leaves none of them out. The game's side is in [research/provinces.md](../research/provinces.md#prosperity-devastation-manpower-supply-separatism-and-institutions-for-m6-of-mod-capabilities).

### M7 Armies' units

The regiments by kind, strength, morale, leader and maintenance in `army.get`; `army.regiments`.

Built (2026-11-11). `army.get`, `army.list` and `army.at` have `units` (regiments or ships by kind), `strength` and `max_strength` (summed, 1 a full regiment or ship), `morale` and `max_morale` (as the unit panel's morale tooltip gives them), `leader` (the leader's id) and `maintenance` (ducats a month at full maintenance, before the maintenance sliders; null in the player's view for another country's). `army.regiments(army)` lists one of the player's armies' regiments or a fleet's ships in the game's order, each with its id, name, kind, unit type, strength and morale. The game's side is in [research/units.md](../research/units.md#f-regiments-strength-morale-leader-and-maintenance-for-m7-of-mod-capabilities).

### M8 Government mechanics and advisors

`government.mechanics`, `government.interact`, `ready_on`; the `advisor` group.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-mechanics-and-advisors.md)). `government.mechanics` lists the government's mechanic states (`core/src/game/mechanics.rs`), each named by the text of `ability_<key>`, with every power's value, bounds and `effects`, worked out from its scaled, reverse scaled and range modifiers as the game adds them up (`0x7b9b50`), each modifier named by the game's table of modifier types, and the interactions whose `potential` holds, each with the game's own check and its reasons (`0x7b6ab0`: cost, cooldown, trigger), its cost as `mechanic_power` with `cost_power`, and `ready_on` from the country's cooldowns; `government.interact` posts `government_interaction_command` by the mechanic's and the interaction's keys. The `advisor` group (`core/src/game/advisors.rs`) reads the three seats and the country's pool, the salary, hire and promotion costs from the game's own functions; `hire` posts `change_advisor` into the seat of the advisor's power, `fire` the same command with no advisor, `promote` `promote_advisor`, each refused with the game's reasons (`0x1aebb0`, `0x1ad440`) or, for hiring, the treasury in the framework's words. Dismissing an advisor of the pool (`dismiss_advisor`) is not offered. The modifier types' table (`[0x2358080]`) is read here for `effects` only; M4 reads modifiers by key, and the two are one table.

### M9 Requests as the game

`as` on the pipe, `--as-player` and `--as-game` on the command line, the view by the request, the setting removed, `as_the_game` refusals, `describe` and the wiki saying which capabilities need it.

Done (2026-11-11): the catalogue has `As` (`player`, `game`), read beside the capability by `sent_from_json` and written by `Sent`, capability first and `"as": "game"` after it; a request as the player says nothing. A capability that needs a request as the game is declared with the place `as_game`, a `world` capability that extensions have; `Capability::as_game` says it, `describe` answers it, and the wiki marks it "(as the game)" and gives its command line `--as-game`. A request as the player asking for one is refused `as_the_game` (a new code) before it runs (`catalogue::may_ask`). In the core, a remote caller is `Caller::Remote(As)`, whose view is the player's or the full view (`game::view`); remote control's engine queues each request with how it acts, says so to the catalogue before it runs (`Catalogue::act_as`, which sets `Live`'s caller) and again before a request that waits is polled, and writes " as the game" after one in the log. Remote control now keeps every signal extensions hear, each marked whether the player's view shows it, and a read or an advance answers those of its request's view: the numbers of what the player's view leaves out are skipped in its reads. The setting `remote_full_view`, the settings window's remote control page and their texts are gone; a settings file that still has it reads without it. `eu4-harmony game` takes `--as-player` and `--as-game`, the sweep asks a capability that needs it both ways, and `eu4-harmony wiki remote`, the game guide, README.md, README.zh-CN.md and adding-a-capability.md say how a request acts. No capability needs a request as the game yet: the script group is the first. Tried in a test game: [the run](../prototype-log/2026-11-11-requests-as-the-game.md).

### M10 Flags

`script.flag`, `script.set_flag`, `script.clear_flag`, in every scope the game has.

Done (2026-11-11): the `script` group, every capability in it as the game (`as_game`), with a new kind of argument, the scope (`{country = …}`, `{province = …}`, `{ruler = …}`, `{heir = …}`, `{consort = …}`, `{global = true}`; `country:FRA`, `province:183` or `global` on the command line), and the checks a fake makes too in `eu4_harmony_checks::script`: each use's scopes, a name as the scripts write one, a variable's value. A flag is read from its scope's flag set, found as the effects find it (a country's, a province's, the world's, a monarch's through its vtable, a rulerless country's own for its ruler), by the id the game gave its name; the framework only looks the name up, so asking for one no script named adds nothing. Setting asks the game's own `0x737ea0` for the id and sets it with `0x7371f0`, on today's date; clearing calls `0x7372f0`. A flag on an heir or a consort a country has not reads as not set, clears as nothing, and is refused when set ([research/scripts.md](../research/scripts.md)). The fake game's world keeps the flags a mod sets, on its date. Tried in a test game: [the run](../prototype-log/2026-11-11-script-flags-variables-events.md).

### M11 Variables

`script.variable`, `script.set_variable`.

Done (2026-11-11): a variable is read by walking its scope's map for the name, so that 0 is told from none, and set with the game's `0xd7c450`, in thousandths; a country's and a province's, the game having no variables of its own. A new kind of argument, the number, carries the value. The fake game's world keeps them, rounded as the game rounds.

### M12 Firing events

`script.fire_event`.

Done (2026-11-11): the event is found by its id among the game's events, its trigger asked in a scope `game::conditions` makes (the country, or the province and `---` so that it is its owner's), and fired with the game's `TriggerEvent` through the event manager, as `country_event` and `province_event` fire one with no days to wait. A trigger that does not hold is refused `rejected`, an id no event has and a province no one owns `not_found`. The fake game leaves it to a test's answer, having no events.

### M13 Tasks awaited

`spawn` returning the task, `async`, `await`, `cancel`, `done`, `await_all`, `await_any`, `eu4.Task<T>`, the chapter in `eu4-harmony skills`.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-tasks-and-game-days.md)). The prelude builds `spawn`, `async`, `await_all`, `await_any` and a task's methods on three functions of the Lua host's: starting a task with its handle, cancelling one, and waiting for any of some tasks to end. A task's handle keeps how it ended, what its function returned or the error it raised; the function runs inside `pcall`, so that the error is kept, then raised again for the log. The host keeps a waiting task's handle with it: cancelling drops a task that waits at once, and one that runs or is being taken up at its next wait. A poll takes up the waiting tasks again, up to 16 passes in one frame, while any was taken up, so that a task awaiting another that just ended goes on in the same frame. A session's end marks the handles of the tasks it drops cancelled. `await_all` returns each task's first result; a task awaiting itself is an error. The editor's types write `spawn`, `async` and the task's methods generic in `T` (`eu4.Task<T>`); `eu4-harmony wiki lua` lists the task's methods under the globals.

### M14 Waiting for game days

`game.time.wait`.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-tasks-and-game-days.md)). `time.wait`, for extensions and remote clients both: from 1 to 36500 days, answered with the date once the game's date has reached the date that many days on, at a game frame; it neither runs nor pauses the game. A remote client's wait is answered `session_changed` when its session ends first; an extension's task is dropped with its session. The fake game answers it as the days a test raises (`mod:day`) reach the date, taking up the waiting tasks before it raises the day, as the game's frame does. The sweep asks it only at its edges, since a paused game passes no days.

### M15 Army organization

`army.detach_kind`, `army.attach`, `army.detach`, `army.attach_to_transport`, `army.detach_from_transport`, `army.detach_special_units`, `army.auto_transport`, `army.stop_auto_transport`, and their entries in `army.options`.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-army-organization.md)); [research/units.md](../research/units.md), "Organizing an army". Each posts the command the panel posts: `detach_subunit` with the kind as the game numbers it, `attach_unit` and `detach_unit`, `connect_to_ship_command` (detaching leaves `do_connect_to_ship` out, as the game writes it), `detach_merc` and `toggle_auto_transfer`. The game's own checks give the reasons: detaching a kind and special units, and attaching to transport, by the functions the buttons call; attaching, by asking the game of each unit in the province, as the Attach button and its window do, the button's `NO_TARGETS_TO_ATTACH_TO` when none answers yes; automatic transportation, as its button does, refused for a mothballed fleet and one with no ship that carries troops in both of its states. `army.options` gives `detach_kind` once for each kind the unit has, with the kind, and `attach` with the units it may attach to (`targets`), or `detach` while it is attached; `ArmyAction` gained `kind` and `targets`, null for the other buttons. A detach answers the new unit. Mercenaries are detached as the game's button does it, its special units: "Detach Special Units". Leaving a transport is landing, which `army.move` does; the game's `load_unit` and `unload_unit` are the AI's alone and not offered. Detaching a siege or a blockade is not taken up.

### M16 to M20 Actions of the panels

As listed under Behavior, one part each.
