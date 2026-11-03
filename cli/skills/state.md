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
