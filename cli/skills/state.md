state: keeping state

Locals in main.lua live as long as the game runs, across sessions: reset
what belongs to one session in on_session. Two places outlive the game:

save data, in the save

  save_data = function(game)
    return { routes = routes, version = 1 }    -- any value JSON can hold
  end,
  on_session = function(game, saved)
    routes = saved and saved.routes or {}      -- nil for a new game
  end,

- save_data is called whenever the game writes its world, autosaves and
  the ironman exit save included, and what it returns goes into that save;
  and before eu4-harmony reload replaces the mod's code, whose new
  on_session gets it. It cannot wait.
- Tables whose keys are all strings, sequences, strings, numbers (integers
  stay 64-bit, so army IDs survive), booleans. A function, a table with
  mixed keys or a cycle fails: that save holds nothing for the mod, and the
  error is logged. At most 16 MiB of JSON.
- Put a version in it, and read older shapes in on_session.

Done when a save written by the mod and loaded again (eu4-harmony skills
game) gives on_session what save_data returned.

files, in the data folder

Settings or caches that belong to the player, not to one save, go in the
mod's data folder with files.read, files.write, files.list and
files.remove (eu4-harmony wiki lua). Declare permissions = { files = true }
in the manifest. files.write writes a file whole or not at all; paths are
relative, with / between names.

The mod's own files, shipped in eu4-harmony/data/, are read with
mod_files.read and need no permission.

json.encode and json.decode read and write JSON as save data does.

flags and variables, shared with the mod's scripts

The mod's Paradox scripts (events/, decisions/, common/) and its Lua meet
in the scripts' own state, which the game keeps in the save:

  local me = { country = game.session.get().player }
  game.script.set_flag(me, "my_mod_started")           -- set_country_flag
  if game.script.flag(me, "my_mod_reformed").set then  -- has_country_flag
    game.script.set_variable({ province = 183 }, "my_mod_level", 2.5)
    game.script.fire_event(me, "my_mod.1")             -- country_event
  end

- A scope is a table of one key: country, province, ruler, heir, consort
  (each a tag but province, a number), or { global = true }. Variables are
  a country's or a province's; events fire for a country or a province.
- flag answers { set, day }, variable { set, value }: value is 0 for a
  variable the scope has none of, as the scripts read it. Values keep three
  decimal places.
- fire_event fires an event the mod's events/ define, as country_event
  does: only when its trigger holds (refused rejected otherwise), the AI
  choosing an option at once, the player getting it as a question. To fire
  it later, wait first: game.time.wait(days) (eu4-harmony skills tasks).
- Prefix every name with the mod's, so that two mods' flags do not meet.
- A remote client uses these only as the game (eu4-harmony game --as-game).

Done when a flag the Lua sets is one an event's trigger of the mod's sees,
in the running game (eu4-harmony skills game).
