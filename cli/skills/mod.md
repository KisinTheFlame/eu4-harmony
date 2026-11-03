mod: making a mod

An EU4 Harmony mod is a Paradox mod with an eu4-harmony/ folder: its
manifest, its Lua extension, interface fragments, texts and files. Where a
file is says what it is; eu4-harmony wiki manifest has the manifest's fields.

  descriptor.mod                 the Paradox descriptor: name, version
  eu4-harmony/manifest.toml      id, api_version, description, permissions
  eu4-harmony/lua/main.lua       the extension, run first; require loads lua/<name>.lua
  eu4-harmony/ui/<window>/       interface fragments (eu4-harmony skills ui)
  eu4-harmony/localisation/      texts (eu4-harmony skills localisation)
  eu4-harmony/data/              files the extension reads with mod_files.read
  eu4-harmony/tests/             tests, run by eu4-harmony test, never uploaded
  .eu4-harmony/types/            the editor's type definitions, never uploaded

steps:

1. Create it: eu4-harmony new <id> --name "<display name>". Done when the
   folder has descriptor.mod and eu4-harmony/manifest.toml.
2. Write main.lua. It returns a table of handlers, one per event the mod
   hears (eu4-harmony wiki lua lists them); keep ---@type eu4.Handlers above
   it so the editor checks every capability call. Each handler gets game and
   reaches the game only through it: game.<group>.<name>(...), as
   eu4-harmony wiki <group> <name> shows. Test every result: a capability
   returns nil, err when the game refuses, and the handler goes on from
   there. Done when every event the mod needs has its handler and every
   result is tested.
3. Declare what the mod uses beyond the game, if anything: the real time,
   files of its own, HTTP. Add permissions to the manifest
   (eu4-harmony wiki manifest); the player sees them in the mods window.
   Done when every os.time, os.date, files.* and http.fetch in the code is
   declared, and nothing else is.
4. Keep every handler short. A stretch of a handler over 5 ms is logged; one
   over 1 s is stopped, and three in a row stop the mod. Spread long work
   over frames or days, keeping where it got to in a local of main.lua.
5. Check and test it: eu4-harmony check reads the manifest, the Lua files,
   the localisation and where the files are as the framework does, and
   eu4-harmony skills testing has the tests. Done when eu4-harmony check
   says ok and eu4-harmony test passes with a test for each handler.
6. Try it in the game: eu4-harmony skills game. Done when each handler has
   run there and the mod's log shows no error.
7. Package it: eu4-harmony package. Done when it reports the package
   folder; it refuses a mod eu4-harmony check finds problems in, and lists
   them.

A newer eu4-harmony: run eu4-harmony types for its type definitions, and
check api_version in eu4-harmony wiki.
