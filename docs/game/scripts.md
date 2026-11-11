# Scripts

The game's content is written in script: text files of keys and blocks, the same files a modder edits, which the game reads into conditions (triggers), outcomes (effects), modifiers and localised text. Decisions, missions, reforms, policies and estate privileges are script with a few conditions each, and what the interface shows of them is the conditions written out as text. Beside the script stand the game console, whose console commands change the game directly, and the commands, the one way the player's interface and the AI change the game.

## Reading script

- **Every script file is read whole first.** The file is read through the game's file system (PhysFS) into memory in one go and only then handed to the lexer; nothing is cached in between ([research/ui-injection.md](../research/ui-injection.md), "The GUI loading path (verified)"). The same reader serves text and the binary form, through two tokenizers; one writer writes both, and saves and commands go through them too ([research/architecture.md](../research/architecture.md), "Script"; [research/commands.md](../research/commands.md), "The written form").
- **Keys are tokens**: every key, trigger, effect and modifier name the game knows has a number in its token table; a name it does not know gets one made at run time ([research/architecture.md](../research/architecture.md), "Script").
- **One shape for what is read**: triggers, effects, scopes, commands, the world and the application are persistent objects with one layout of methods: write as a block, read (one key at a time), fix up after reading, report an error with its file and line ([research/architecture.md](../research/architecture.md), "Script").

## Triggers

- **A trigger** is a parsed condition, made by its token from a switch of 899 cases; a name the switch does not have is tried as an idea group, a religion, a trade good, an institution, an area and a scripted trigger ([research/architecture.md](../research/architecture.md), "Script"). A block, as `potential`, `allow` or `and = { }`, holds its children, and holds when every child holds; an empty block holds ([research/triggers.md](../research/triggers.md), "The trigger object").
- **A trigger is asked in a scope**, the object it is about: a country, a province, or both, and the scopes it came from (ROOT, FROM and PREV by their use, inferred). A trigger that needs a country takes a province's owner ([research/triggers.md](../research/triggers.md), "The scope"; [research/questions-and-offers.md](../research/questions-and-offers.md)).
- **The conditions as text.** Each trigger writes its own localised line, and a block its children's, indented; a top-level line is marked by the green tick or the red cross, the `£yes`/`£no` icons, not by a color. The game can list every line, those that hold, or those that fail; the views of decisions, missions, reforms, policies, privileges and idea groups list every line ([research/triggers.md](../research/triggers.md), "Writing the conditions as text").
- **Where content keeps its conditions** ([research/triggers.md](../research/triggers.md), "The content, where its triggers are and where the interface shows them"):
  - a decision's `potential` says whether the decisions view lists it, its `allow` whether it may be taken, and the view sorts them into allowed and not, major and not; taking one posts a command;
  - a mission can be completed when it is not completed, the missions before it are done, and its `trigger` holds;
  - a policy may be taken when the country has not got it, its `allow` holds and a slot is free; an estate privilege when there is crown land enough and its `can_select` holds; reforms have `potential`, `trigger` and `allow`.
- **Asking only reads** (inferred): the only write seen is the name a scope keeps for error messages. The AI asks triggers in its countries' updates side by side ([ai.md](ai.md)), so asking is safe beside other readers, not beside the game changing what it reads ([research/triggers.md](../research/triggers.md), "Calling them outside the interface").

## Effects, modifiers and the rest

- **Effects** are about 617 kinds, each with an execute and a key for its tooltip; every effect is listed, and the list is checked once the game is set up ([research/architecture.md](../research/architecture.md), "Script").
- **Scripted triggers, effects and functions** are databases of their own; parameterized script is compiled once per use (inferred from its log lines) ([research/architecture.md](../research/architecture.md), "Script").
- **Modifier types** are built into the code; `static_modifiers` and the like give them their values ([research/architecture.md](../research/architecture.md), "Script"). The modifiers countries and provinces carry are in [countries.md](countries.md) and [provinces.md](provinces.md).
- **Scripted GUI** binds `common/custom_gui` to the elements of the interface by name ([research/architecture.md](../research/architecture.md), "Script"; [interface.md](interface.md)).

## Flags, variables and firing events

- **A flag** is a name set on a scope, with the day it was set: a country, a province, the game as a whole, and a country's ruler, heir and consort each keep flags of their own. The game gives every flag name it meets a number, once, and a scope keeps its flags as those numbers beside their days; setting a flag set before sets it again today, and clearing one not set does nothing ([research/scripts.md](../research/scripts.md), "Flags").
- **Who keeps a monarch's flags**: the person, while the country has one. A country without a ruler keeps its ruler's flags itself; flags set on an heir or a consort a country has not go nowhere. A consort ruling as regent is the consort too ([research/scripts.md](../research/scripts.md), "Flags").
- **A variable** is a number kept under a name by a country or a province, in thousandths; there are no variables of the game as a whole. A name a scope has none of reads as 0 ([research/scripts.md](../research/scripts.md), "Variables").
- **Firing an event** from script (`country_event`, `province_event`) asks the event's trigger in a scope of the country, or of the province with no country so that it is its owner's, and fires the event only when the trigger holds; with days to wait, it is put among the country's delayed events instead. Firing has the AI choose an option of its countries' events at once, and makes the player's a question ([research/scripts.md](../research/scripts.md), "Events"; [research/questions-and-offers.md](../research/questions-and-offers.md)).

## Localisation

- **The text the player reads** comes from `.yml` files: `localisation/languages.yml`, then every `.yml` under `localisation`, then `localisation/replace`, into one table. A lookup fills in `$NAME$` ([research/architecture.md](../research/architecture.md), "Script").
- **The game's text is single bytes.** Each entry is turned from UTF-8 into Windows-1252 as it is imported, and a character Windows-1252 lacks becomes `?` ([research/unicode-text.md](../research/unicode-text.md), "Localisation"). How text is drawn and typed is in [interface.md](interface.md).

## The game console

- **378 console commands** are registered whenever a game is set up, so in the main menu there are none; 324 are in players' builds, the other 54 are tools for the game's developers (`debug_*` but `debug_mode`, `build_all`, `moveunit`, `deiron` and the like) ([research/game-console.md](../research/game-console.md), "The game console and its commands").
- **What refuses one** ([research/game-console.md](../research/game-console.md), "What keeps a command from running"):
  - one for developers, while every game start locks them: only `enable_all_commands` with the game's password opens them, and anything else gets "Cheaters are not welcome here.";
  - every one but `enable_all_commands` and `stats` in multiplayer or ironman; got around, the game crashes itself on purpose once the console command has run;
  - an unknown name, answered with the closest one ("Did you mean 'cash'?"). Names match in either case.
- **A console command is not a command**: its handler changes the game at once ([research/architecture.md](../research/architecture.md), "Commands and multiplayer"). That a console command ran is kept, and folded into the game checksum ([research/game-console.md](../research/game-console.md), "The game console and its commands").
- **The game console's window** trims a line, keeps it in a history of 512 lines (`console_history.txt`), cuts it into words at every space (quotes mean nothing), runs it, and prints the answer in white when it succeeded and in red when not; a line with one `|` writes the answer to the file named after it ([research/game-console.md](../research/game-console.md), "Running a command, and what it printed").
- **Gold** is `cash`, which with no amount adds 5000; `add_treasury` is an effect, which `run <file>` runs from a file ([research/game-console.md](../research/game-console.md), "The game console and its commands").

## Commands

- **The player's interface and the AI change the game through commands.** Not through commands: console commands, the pause key in single player ([day.md](day.md)), and the diplomatic actions the game carries out itself by calling them ([research/architecture.md](../research/architecture.md), "Commands and multiplayer"; [research/commands.md](../research/commands.md), "Where commands run"; [diplomacy.md](diplomacy.md)).
- **A command's type is a name.** There are 339 types, `buildunit`, `move_order`, `diplomaticactioncommand`, `select_event_option`, `daily_tick` and the rest, each a name in the token table, and the game makes an empty command of a type from its name ([research/commands.md](../research/commands.md), "The factory and the registry").
- **The life of a command** ([research/architecture.md](../research/architecture.md), "Commands and multiplayer"; [research/commands.md](../research/commands.md)):
  1. the interface, the AI or the game itself makes it and posts it;
  2. even in single player it is written out and a new command read back from what was written, as it would cross the network, so **what runs is only what its written form carries** ("Every command is written out and read back, in single player too");
  3. at the next in-game frame the session asks each queued command's check again, drops one that fails ("Discarding invalid command!"), and executes the rest in order, on the main thread only ("Where commands run").
- **When the queue runs** depends on the session's state; the day itself is posted inside the map manager's frame and executed at the next one ([research/commands.md](../research/commands.md), "When the update runs the queue"; [day.md](day.md)).
- **The written form says everything.** Text and the binary form come from the same code; a command written as text and read back came back exactly, every one of 26,429 of 70 types. The reader is lenient: a key left out takes its default, an unknown key is passed over, a mistake becomes a default and a line in the game's logs, and nothing tells the caller ([research/commands.md](../research/commands.md), "Tried in the game: reading text back").
- **A command is checked for the country it names**, not for who sent it: a recruit for England, posted from France's game, was carried out at England's cost ([research/commands.md](../research/commands.md), "Tried in the game: posting a command read from text"). Where a command names its country differs by type (`country`, `tag`, inside the object it carries) ([research/commands.md](../research/commands.md), "Tried in the game: the command log").
- **Kinds of command**: 61 that may be posted in the lobby and the setup (`startgame`, `setdifficulty`, `load_file`…); the game's own (`daily_tick`, `client_ping`, the speed's, the autosave and the save); and the game's actions, building, recruiting, armies, diplomacy, estates, reforms, events' options, decisions, trade, religion. Most of a day's commands are the AI's, about 275 a day early in a game ([research/commands.md](../research/commands.md), "Kinds of command", "Tried in the game: the command log").
- **In multiplayer** every machine executes the same commands in the same order; the day's checksum, taken after `daily_tick`, records the random generator, whether the game console was used and many values of every province, country and unit ([research/architecture.md](../research/architecture.md), "Commands and multiplayer"; [day.md](day.md)).

## What the framework relies on

- **The command layer** writes commands in their written form, has the game check them and executes them at once, and hears every command the game executes at the one place each passes ([plans/commands.md](../plans/commands.md); [ADR one-command-layer-in-written-form](../adr/2026-11-03-one-command-layer-in-written-form.md), [ADR commands-run-at-once](../adr/2026-11-03-commands-run-at-once.md)).
- **Running a console command** goes through the game console's own run, with its refusals, as the game console's window does ([research/game-console.md](../research/game-console.md), "What the framework does"; [plans/remote-control.md](../plans/remote-control.md)).
- **Content's conditions as text**, for capabilities over decisions, policies and the rest, are the views' own ([research/triggers.md](../research/triggers.md); [plans/player-actions.md](../plans/player-actions.md)).
- **Flags, variables and events** are read, set and fired as the scripts' own effects do, through the same functions of the game's, for the `script` group ([research/scripts.md](../research/scripts.md), "What the framework does"; [plans/mod-capabilities.md](../plans/mod-capabilities.md)).
- **The Unicode module** reads script through the text tokenizer and localisation through its import, so that text keeps its characters ([research/architecture.md](../research/architecture.md), "Where the framework attaches"; [ADR built-in-unicode-module](../adr/2026-10-27-built-in-unicode-module.md)).

## What is not known yet

- What a content trigger's last two fields are for, what several of a scope's fields hold (saved scopes and variables are guesses), and how the decisions view makes its scope ([research/triggers.md](../research/triggers.md), "Not established").
- Whether any trigger colors its own line; how the interface lists a country's missions; where a reform's `allow` shows; parliament issues and subject interactions, which were not read; an estate agenda's blocks are in [research/government.md](../research/government.md#estates) ([research/triggers.md](../research/triggers.md), "Not established").
- Where `[Root.GetName]` in localisation is worked out ([research/architecture.md](../research/architecture.md), "Open questions").
- The other variable effects, such as `change_variable`, and whether any thread but the main one sets flags ([research/scripts.md](../research/scripts.md), "Not established").
- Which of the game console's two refusing flags is multiplayer and which ironman; the refusal was tried in an ironman game, not in a multiplayer one ([research/game-console.md](../research/game-console.md)).
- What the reader's list of strings and its name are for (reading back did without them); which session state a single player game is in while it is played; whether the session's last hand-off of an executed command sends it to other players or records it ([research/commands.md](../research/commands.md), "What this means for the framework", "When the update runs the queue"; [research/architecture.md](../research/architecture.md), "Open questions").
- What `console_command`, a command type, does when executed ([research/commands.md](../research/commands.md), "Kinds of command").
