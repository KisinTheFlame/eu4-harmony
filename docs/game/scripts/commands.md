# Commands as data: the factory, their written form, where they run

## Every command is written out and read back, in single player too

- **The local handler's post** `0x1580370(handler, command)` (slot `+0x30` of the local handler, vtable `0x1d96178`; the host's is `0x15774f0`, the client's `0x157c6e0`), which both posts (`0x14ca2d0`, `0x14ca210`) reach:
  1. stamps the command (`+0x48`) and, unless `+0x4a` is set, gives it the next ID (`+0x4c`);
  2. makes a memory stream over a byte vector (vtable `0x1dc0358`) and a writer over it (vtable `0x1dbf7d8`) in binary (the writer's `+0x18` set);
  3. writes the command's type token as a key (`0x1709280`) and the command as a block (its `+0x08`, `0xd2f410` for all of them, which calls its `+0x10`);
  4. for each entry of the vector at `0x23587e8` (one in single player; inferred), makes a binary tokenizer (vtable `0x1dbf988`) and a reader (`0x17096f0`) over the bytes, reads the first token (`0x170ada0`), has the factory make a command of that type (`0x14c6500`), reads it (its `+0x18`, `0x17068b0` for all of them, which calls `+0x20` for each key), and queues it (`0x1580ff0`);
  5. destroys the posted command (`[rsp+0xbf8]`, its `+0` with flag 1).
- So **what the game runs is only what the written form carries.** Whatever a command's caller set and did not write is lost on the way, in single player as in multiplayer. Every command that works in the game round-trips through its writer and reader.
- The post `0x14ca2d0` does not free the command itself: the handler takes it. `0x14ca210` takes the pointer out of its `std::unique_ptr` before handing it over, so it destroys nothing either.

## The factory and the registry

- **The registry** is a vector of prototypes at `0x23587b0` (begin), `0x23587b8` (end), indexed by type token. `0x14c6930(prototype)` stores one at the index its `+0x50` answers, growing the vector.
- **The factory** `0x14c6500(_, reader, token)` takes the prototype at the token's index and clones it (its `+0x60`, given the string at `reader + 0x648`). Out of bounds it logs "Command type out of bounds", for an empty slot "Couldn't find command, token: " with the token's name (`0x14c6f00`), and returns null.
- **The command classes.** `.rdata` holds 340 vtables with the command layout: `+0x08` = `0xd2f410`, `+0x18` = `0x17068b0`, `+0x38` = `0x1707670`, `+0x90` = `0x14c64d0`, `+0x98` = `0x14c64e0`. One is the base `0x1c3a710`; the other 339 each answer a type token from `+0x50` (`mov eax, imm32; ret`), and each token is a name in the token table (`0x1fd8660`): `buildunit`, `move_order`, `diplomaticactioncommand`, `select_event_option`, `daily_tick` and so on. `0x249120` calls `0x14c6930` at 336 places; where the other classes are registered was not followed.
- So **a command type is a name**, the same name in its written form, and the factory makes an empty one from it.

## The written form

- **The writer writes text or binary with the same code.** `0x1709280(writer, token)` writes the token's id when the writer's `+0x18` is set and its name otherwise; the saves' writer (`0x14e3f00`) is the same class. A command's `+0x10` writes through that interface only.
- **The base fields** (`0x14c6290`, called first by both commands' `+0x10` read here): `tickstamp` (`0xc7`), `istargetasynchronous` (`0xcc`), `identity` (`0xdb`), `sender` (`0x240`).
- **The recruit command** (`buildunit`, vtable `0x1c4dff8`; write `0x26d880`, read one key `0x26dba0`) writes `country` (the tag as a string), `province` (`%d`), `unit` (the unit type's key, when there is one), a mercenary company (`0x2660e0`) when there is one, `category`, `trade_mission`, `node` (each `%d`), `update_macro`, `update_map`, `flagship` (a block, through the object's own `+0x08`) and `refresh_modifier`, the yes-or-no ones only when they are not the default. Numbers are written as text (`0x15c230` with `"%d"`) and read back from the token's text (`reader + 0x444`, `"%i"`), so a value reads the same from a text tokenizer as from a binary one (inferred).
- **A command that carries an object of a kind of its own** writes that object's type token as the key: the diplomatic action command (vtable `0x1c814b0`, write `0x4e8440`) writes the action under its type (`+0x98`) as a block (`+0x08`); reading (`0x4e8490`) makes the action by that token (`0x35d210`) when the slot is empty and has it read itself (`+0x18`). So nested objects of any kind come back as the right class.
- **Which country.** 124 of the 339 writers write `country` (`0x289d`) themselves (counted by the instructions that carry the token; others may write it through a helper or under another key, as the diplomatic actions do inside the action). Nothing in the session's post or its execution compares the country a command names with the sender (inferred from `0x14ca2d0`, `0x1580370` and `0x14ca870`); whether each command's check does is per command.

## Where commands run

- **`CSession::Update`'s execution** `0x14ca870` takes the queued commands from the handler (`+0x48`, then `+0x88`) into a vector of the session's (`+0x140`), drops those held back during a hot join, asks each command's check (`+0x40`, at `0x14cab30`) and logs and drops a command that fails it ("Discarding invalid command! ID: %u, Type: %s"). For one that passes (`0x14caed8`) it:
  1. calls before (`+0x68`) only when the session's mode (`+0x50`) is 4, 6 or 7 (`lea eax, [rcx-4]; test eax, ~3` skips it for a mode outside 4 to 7, and `cmp ecx, 5` for 5; read wrongly as "unless" until 2026-11-03);
  2. sets the session's `+0x7d4` (executing) and the random-forbidden flag `0x1fd10c4` to the command's `+0x4a`;
  3. runs a lambda (vtable `0x1d8c8b0`, holding the command at `+8`) through `0x14d4e70`;
  4. puts both flags back, calls after (`+0x70`) under the same condition as before, and, unless `+0x4a` is set, hands the command to `0x157b1a0(handler, command)`, which writes it out once more in binary (whether to send it or to record it is still open);
  5. keeps the command's ID in a list of the session's (`+0x7b8`, its count at `+0x7c8`, at most `+0x7d0`), keeps a clone (`+0x60`) in another (`+0x158`) when the command's `+0x58` says so, then takes the command out of the vector, destroys it, and calls the session's callback at `+0x838` with `+0x830` when one is set.
- **The lambda's call** `0x14cea10` (its vtable's `+0x10`; the vtable is referenced only at `0x14caf25`) logs "Executing command! ID: %u, Type: %s" when a log category's bit is set and the command's `+0x78` does not say otherwise, then jumps to the command's execute (`+0x48`). `0x14d4e70` reaches it through `0x14d4e30` and `0x10a0f20`, which jumps to the function object's `+0x10` too, so an execute returns to `0x14d4e62`: the call trace of recruiting saw the recruit command's execute called from there ([recruiting.md](../military/recruiting.md)). It is called only for commands that passed their check, on the main thread, with the command object whole: **one observe point there hears every command the game executes**, the player's, the AI's (`0xbfe50`) and the game's own (`daily_tick`), before and after it runs.
- **What it does not hear**: what the game does without a command (the game console's handlers, the pause key in single player; [architecture.md](../program/architecture.md#commands-and-multiplayer)), and commands the game executes by calling their execute directly (the diplomatic action command's `0x4e7c50` is called so for actions the game sends itself; [diplomacy.md](../diplomacy/diplomacy.md)).

## When the update runs the queue

Read from the code on 2026-11-03, for the layer to execute a command at once (ADR commands-run-at-once).

- **The local handler's queue.** Its post (`+0x30`, `0x1580370`) puts the command it read back into a concurrent queue at the handler's `+0x118` (`0x1580ff0`). Its `+0x38`, `0x15801c0(handler, 0)`, takes each command out of that queue, clones it (`+0x60`, with the ID copied), puts the clone at the end of the list at `+0xc8` and destroys the one taken; its `+0x48` (`0x8b0e0`) answers that list, which the execution `0x14ca870` takes, and its `+0x88` (`0x1574f30`) the one at `+0x98`.
- **`CSession::Update`** `0x14ca360` reads the session's state, a u32 at `+0x48` (`0x14ca572`):
  - in the states whose bits `0xc014` holds (2, 4, 14, 15) it sets `+0x814`, and, unless a command is executing (`+0x7d4`) or commands are held back (`+0x841`), calls the handler's `+0x38` with 0 and runs the queue with `0x14ca870`; then clears `+0x814` (`0x14ca5de`–`0x14ca621`);
  - in state 13 it calls the handler's `+0x38` only; in 3, 7 and 16 it runs nothing;
  - in any other state it calls the handler's `+0x38`, and, in state 6 alone and under the same two conditions, first handles `+0x840` when it is set (clears it, tells the session's listeners with 3, calls `0x14cd8b0`), then runs the queue (`0x14ca651`–`0x14ca6b7`).
- The update refuses to run on another thread than the main one ("CSession::Update called from other than main thread").
- The day is posted in the map manager's frame ([time.md](../day/time.md)), so at the framework's safe point after that frame a day posted then is still queued: running the queue there runs it. Tested: a build that ran the queue at its safe point hung the game at speed 5, at the first command it posted while time ran. The main thread's stack showed why: the day the queue held ran there, past its budget it ran a whole application frame ([architecture.md](../program/architecture.md), frames inside a day), whose Present came back into the framework's safe point, which waited on a lock its outer call held.
- **Not known**: which state a single player game is in while it is played (it is one the update executes commands in: every command the layer executed was taken); what `+0x840`'s step does.

## Kinds of command

From the vtables' slots (a slot that is not the base's answer, `0x8f450`, is the class's own):

- `+0x58`, a command that may be posted outside the frame (lobby and setup): 61 classes, the custom nations, teams, the lobby (`startgame`, `selectbookmark`, `setdifficulty`, `quit`, `load_file` and so on).
- `+0x80`, the other answer the post asks: `daily_tick`, `client_ping`, `increase_game_speed`, `decrease_game_speed`, `doautosave`, `savegamecommand`. The two speed commands carry nothing of their own: `increase_game_speed`'s execute `0xb79970` calls the speed setter `0x7579f0` with the speed's index plus one, `decrease_game_speed`'s `0xb79ab0` with it minus one, unless an answer of the interface's (`[world+0x1e00]`'s `+0x3f8`) says so and the speed is already the slowest ([time.md](../day/time.md)).
- `+0x78`, not logged by the lambda: the chat commands, `set_DLCs` and `increase_game_speed`.
- The rest are the game's actions: building, recruiting, armies, diplomacy, estates, reforms, events' options, decisions, trade, religion. `console_command` is one of them: its execute was not read.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x8ded9` | code | the text tokenizer |
| `0x14ca274` | code | Referenced while examining tried in the game: the command log; complete role not established |
| `0x14ca318` | code | a posted command stamped |
| `0x15803d6` | code | Referenced while examining tried in the game: the command log; complete role not established |
| `0x158053d` | code | a posted command written |
| `0x15806fc` | code | Referenced while examining tried in the game: reading text back; complete role not established |
| `0x15807fe` | code | Referenced while examining tried in the game: reading text back; complete role not established |
| `0x15809af` | code | Referenced while examining tried in the game: reading text back; complete role not established |
| `0x1709950` | function | end a reader |
| `0x171ed90` | leaf | memory stream mode |
| `0x1c448b8` | data | Referenced while examining tried in the game: reading text back; complete role not established |
