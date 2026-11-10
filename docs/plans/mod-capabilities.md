# Mod capabilities plan

Status: in progress. Decided with the owner (2026-11-11): what is open to mods now, the roadmap of what is not, and the parts taken up first, M1 to M20. M1 built and tried in the game (2026-11-11, [the run](../prototype-log/2026-11-11-signals.md)); M8 built and tried in the game (2026-11-11, [the run](../prototype-log/2026-11-11-mechanics-and-advisors.md)); M13 and M14 built and tried in the game (2026-11-11, [the run](../prototype-log/2026-11-11-tasks-and-game-days.md)); the others not built yet. Terms it added to [GLOSSARY.md](../../GLOSSARY.md): Modifier, Named modifier, Legitimacy, Government mechanic, Mechanic power, Opinion, As the player, As the game, Event (now the scripts'), Signal (the framework's, formerly its event). The decisions are [ADR requests-act-as-the-player-or-as-the-game](../adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md), [ADR framework-notifications-are-signals](../adr/2026-11-11-framework-notifications-are-signals.md), [ADR changes-as-the-game-are-typed-capabilities](../adr/2026-11-11-changes-as-the-game-are-typed-capabilities.md) and [ADR tasks-are-awaited-like-promises](../adr/2026-11-11-tasks-are-awaited-like-promises.md); it builds on [player-actions.md](player-actions.md) and [ADR mods-hold-the-logic](../adr/2026-11-03-mods-hold-the-logic.md).

## Goal and scope

- **Goal**: one place that says what the framework opens to mods, in five areas (interface, reads, actions, signals, the Lua host), what each still lacks, and in what order it is taken up. The plan stays in progress and takes new parts as the roadmap is worked through, as [game-speed.md](game-speed.md) does.
- **In scope now**: the signals renamed (M1); the reads grown (M2 to M8); the script group and requests as the game (M9 to M12); tasks awaited as promises are, and waiting for game days (M13, M14); every action of the game's panels the catalogue still lacks (M15 to M20).
- **Out of scope**: multiplayer; changing how the game computes (combat, the AI's choices, its rules), which stays outside the catalogue ([ADR mods-hold-the-logic](../adr/2026-11-03-mods-hold-the-logic.md)); running effect or trigger text ([ADR changes-as-the-game-are-typed-capabilities](../adr/2026-11-11-changes-as-the-game-are-typed-capabilities.md)); events put together in Lua (a mod defines its events in its scripts, and fires them); extensions calling each other or sharing code ([lua-extensions.md](lua-extensions.md)).

## What is open now

Each capability group in one line; its arguments, results and refusals are `eu4-harmony wiki`'s. Groups marked *remote* are remote control's only.

### Interface

- Interface fragments (`eu4-harmony/ui/<window>/`) spliced into a host window, with buttons laid out in a button area; the only host window is the single army panel (`single_unitpanel`). `game.ui.<element>` sets an element's visibility, whether it is enabled, its text and tooltip.
- Extension windows (`game.window`): titles, texts, icons, buttons, progress bars, flags and lists, opened from an entry button on the panel button strip ([extension-windows.md](extension-windows.md)).
- Pop-ups (`game.popup`; `popup.list`, `popup.answer`).
- The map painted by province (`game.map.paint`, `game.map.release`).
- `camera`: where the camera looks, looking at a map position, zooming. `screenshot`, `input` and `desktop` (*remote*): a picture of the frame, the player's mouse and keyboard, showing and hiding the game.

### Reads

- `session`: the session, the country acted as, putting a country in view in observe mode. `time.get`: the date, whether paused, the speed. `text.language`: the player language.
- `country.get`: treasury, manpower, stability, prestige, monarch points, wars, relations, technology, ideas, governing capacity, inflation, mercantilism, loans, rebels. `country.budget`: the month's income and expenses. `country.hostile`: the countries at war with one.
- `province.list`, `province.get` (owner, controller, development, terrain, fort, buildings, autonomy, culture, religion, trade good, unrest, trade node, state, edict, cores, claims, constructions), `province.neighbors`.
- `army.get`, `army.list`, `army.at`, `army.selected`; the game's rules for moving: `army.route_to`, `army.can_step`, `army.can_attack`.
- `trade.nodes`, `trade.node`; and each panel's own list: `leader.list`, `loan.list`, `fort.list`, `estate.list`, `idea.list`, `decision.list`, `mission.list`, `policy.list`, `mercenary.list`, `merchant.list`, `question.list`, `offer.list`.

### Actions

Each group reads what its panel offers now (`options` or `list`, each entry a `Choice` with the game's reasons and its cost) and does what its buttons do ([player-actions.md](player-actions.md)).

- Armies and war: `army` (move, stop, split, merge, disband, consolidate, drill, a fleet's missions), `recruit`, `leader`, `mercenary`, `siege`, `fort`, `occupation`, `peace` (terms, evaluating them, demanding, offering).
- Provinces: `province` (develop, exploit, autonomy, state or territory, edicts, cores, culture), `building`, `center_of_trade`, `colonist`, `missionary`.
- The country: `technology`, `idea`, `stability`, `economy` (loans, inflation, mercantilism), `loan`, `focus`, `government` (reforms, rank, government mechanics and their interactions), `advisor` (hiring, firing, promoting), `estate`, `parliament`, `policy`, `decision`, `mission`.
- Trade: `merchant`, `trade_company`.
- Other countries: `diplomacy` (every action of the diplomacy panel, the proposals answering with the other country's answer), `subject`.
- Answering: `question`, `offer`.
- Beyond the panels: `console.run`, the game console with its own checks; `menu` (*remote*: saves, loading, a new game, back to the main menu); `time` (pausing, resuming, the speed; waiting for days to pass; advancing days, *remote*).

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
- `country.opinion(of, toward)` answers what `of` thinks of `toward`: `opinion`, its `parts` (each a `text` as the diplomacy view's tooltip writes it and a `value`), `aggressive_expansion`, `trust` and `favors`. `country.opinions(toward)` answers the same of every country toward one, without the parts.
- `province.get` grows with `prosperity`, `devastation`, `local_manpower`, `supply_limit`, `separatism` and `institutions` (each institution's key and its progress there).
- `army.get` grows with its regiments by kind (infantry, cavalry and artillery; a fleet's heavy, light, galley and transport ships), `strength` and its maximum, `morale` and its maximum, its `leader` and its monthly `maintenance`. `army.regiments(army)` lists each regiment: its ID, kind, unit type (a content key), strength and morale.
- `government.mechanics()` lists the player's government mechanics, each by its content key, with its mechanic powers (each a key, a `value` between `min` and `max`, and its `effects`) and its interactions as `options` entries: a `Choice` and, where it has a cooldown, `ready_on`, the day it can be used again. `government.interact(mechanic, interaction)` uses one, as its button does. What an interaction does is not given.
- `advisor`: `advisor.list` (the player's advisors and those to hire), `advisor.options`, `advisor.hire`, `advisor.fire`, `advisor.promote`, shaped as `leader`.
- What a read leaves out as the player is compared with the game's panels, part by part, as [player-actions.md](player-actions.md) did.

### Scripts, and acting as the game

- The `script` group is what scripts and extensions share. A scope is a table of one key: `{ country = "FRA" }`, `{ province = 183 }`, `{ ruler = "FRA" }`, `{ heir = "FRA" }`, `{ consort = "FRA" }` or `{ global = true }`; each operation takes the scopes the game has for it.
- `script.flag(scope, name)` answers the day the flag was set, or nil; `script.set_flag(scope, name)` and `script.clear_flag(scope, name)` set and clear it, as `set_country_flag` and `clr_country_flag` (and their kin for the other scopes) do.
- `script.variable(scope, name)` answers a variable's value, or nil; `script.set_variable(scope, name, value)` sets it, as `set_variable` does.
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

### M3 Legitimacy and the values every country has

`legitimacy` with its `effects`; `corruption`, `absolutism`, `army_tradition`, `navy_tradition`, `army_professionalism`.

### M4 Modifiers

`country.modifiers`, `province.modifiers`; `named_modifiers` in `country.get` and `province.get`.

### M5 Opinion

`country.opinion`, `country.opinions`.

### M6 Provinces

`prosperity`, `devastation`, `local_manpower`, `supply_limit`, `separatism`, `institutions` in `province.get`.

### M7 Armies' units

The regiments by kind, strength, morale, leader and maintenance in `army.get`; `army.regiments`.

### M8 Government mechanics and advisors

`government.mechanics`, `government.interact`, `ready_on`; the `advisor` group.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-mechanics-and-advisors.md)). `government.mechanics` lists the government's mechanic states (`core/src/game/mechanics.rs`), each named by the text of `ability_<key>`, with every power's value, bounds and `effects`, worked out from its scaled, reverse scaled and range modifiers as the game adds them up (`0x7b9b50`), each modifier named by the game's table of modifier types, and the interactions whose `potential` holds, each with the game's own check and its reasons (`0x7b6ab0`: cost, cooldown, trigger), its cost as `mechanic_power` with `cost_power`, and `ready_on` from the country's cooldowns; `government.interact` posts `government_interaction_command` by the mechanic's and the interaction's keys. The `advisor` group (`core/src/game/advisors.rs`) reads the three seats and the country's pool, the salary, hire and promotion costs from the game's own functions; `hire` posts `change_advisor` into the seat of the advisor's power, `fire` the same command with no advisor, `promote` `promote_advisor`, each refused with the game's reasons (`0x1aebb0`, `0x1ad440`) or, for hiring, the treasury in the framework's words. Dismissing an advisor of the pool (`dismiss_advisor`) is not offered. The modifier types' table (`[0x2358080]`) is read here for `effects` only; M4 reads modifiers by key, and the two are one table.

### M9 Requests as the game

`as` on the pipe, `--as-player` and `--as-game` on the command line, the view by the request, the setting removed, `as_the_game` refusals, `describe` and the wiki saying which capabilities need it.

### M10 Flags

`script.flag`, `script.set_flag`, `script.clear_flag`, in every scope the game has.

### M11 Variables

`script.variable`, `script.set_variable`.

### M12 Firing events

`script.fire_event`.

### M13 Tasks awaited

`spawn` returning the task, `async`, `await`, `cancel`, `done`, `await_all`, `await_any`, `eu4.Task<T>`, the chapter in `eu4-harmony skills`.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-tasks-and-game-days.md)). The prelude builds `spawn`, `async`, `await_all`, `await_any` and a task's methods on three functions of the Lua host's: starting a task with its handle, cancelling one, and waiting for any of some tasks to end. A task's handle keeps how it ended, what its function returned or the error it raised; the function runs inside `pcall`, so that the error is kept, then raised again for the log. The host keeps a waiting task's handle with it: cancelling drops a task that waits at once, and one that runs or is being taken up at its next wait. A poll takes up the waiting tasks again, up to 16 passes in one frame, while any was taken up, so that a task awaiting another that just ended goes on in the same frame. A session's end marks the handles of the tasks it drops cancelled. `await_all` returns each task's first result; a task awaiting itself is an error. The editor's types write `spawn`, `async` and the task's methods generic in `T` (`eu4.Task<T>`); `eu4-harmony wiki lua` lists the task's methods under the globals.

### M14 Waiting for game days

`game.time.wait`.

Built (2026-11-11, [the run](../prototype-log/2026-11-11-tasks-and-game-days.md)). `time.wait`, for extensions and remote clients both: from 1 to 36500 days, answered with the date once the game's date has reached the date that many days on, at a game frame; it neither runs nor pauses the game. A remote client's wait is answered `session_changed` when its session ends first; an extension's task is dropped with its session. The fake game answers it as the days a test raises (`mod:day`) reach the date, taking up the waiting tasks before it raises the day, as the game's frame does. The sweep asks it only at its edges, since a paused game passes no days.

### M15 to M20 Actions of the panels

As listed under Behavior, one part each.
