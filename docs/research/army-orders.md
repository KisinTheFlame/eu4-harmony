# Army orders: from a right click to the route (for `game::events`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` with `core/examples/disasm.rs`, checked again on 2026-11-03. What was checked in the running game is marked "tested", from [prototype-log.md](../prototype-log.md) (2026-10-31, army orders). What was seen in the running game on 2026-10-26, while regional-combat was developed and before the framework existed, is marked "seen 2026-10-26"; it is not in that log. Everything else was read from the code only. The route query, the route install and the stop themselves are in [routes.md](routes.md).

## From the map input to the army's action

- **The map manager's frame** `0x8137c0` handles input through `0x830fd0` (called at `0x815028`) and `0x816ad0` (at `0x831132`). That code goes on in an entry of its own in the exception directory, at `0x8171e0`, which begins inside a loop and so is no function to observe on its own; it calls the map input `0x116d6c0` at `0x81724f`.
- **The map input** `0x116d6c0` goes through the frame's input list. For a right click (an input whose `+0x18` is 2) it turns the screen position into a province (`0x10a3dd0`), checks that the selection can act there (`0x1170b00`), and calls the selection's action `0xad0ba0`(selection, province, modifiers) at `0x116e3ea`.
- **The selection's action** `0xad0ba0` goes through the selected objects (a list whose node holds the object at `+0` and the next node at `+0x10`). For each it asks the object's method `+0x38`(object, confirmations, province, modifiers); when that says yes it calls the object's action, its method `+0x30`(object, province, modifiers), at `0xad0c29`. Otherwise it may ask the player to confirm cancelling a mission (`CANCEL_DRILLING_MOVE_CONFIRM`, `CANCEL_REBEL_HUNT_MOVE_CONFIRM`, `CANCEL_CARPET_SIEGE_MOVE_CONFIRM` and others for several armies); the answer is acted on later, from the window that asks (its callback class `0x1ce2a28`), not inside this call. Several selected armies get one action each. `0xad0ba0` has a second caller, at `0x116cd81`, in another part of the input handling, not worked out.
- **The land army's action**, method `+0x30` of class `0x1c42540`, is `0xd423a0`: `mov r9b, 1; jmp 0xd429e0`. The army's action proper is shared: it is called with a last argument of 0 too, by the retreat after a lost battle (`0x21eb30` at `0x21edba`, [defeat.md](defeat.md)). That argument says only that the caller is the interface: with it, the action reads the keyboard modifiers (`0xcfc70`) and may ask the player about access or transport (`MOVE_AUTO_TRANSPORT_DIALOG`). Neither the action nor that argument marks the player's order.
- Seen 2026-10-26: in a paused game, a right click on a map province with an own army selected (in province 149, the click on 159) called the army's action from `0xad0c2c` with queue 0 and the last argument 1, through `0x116e3ef`, `0x817254`, `0x831137` and `0x81502d`. It posted a move command for that army's ID at `0xd43a50`; the post and the action both returned 1. Not seen: a Shift right click (queue), a right click on the army's own province, a retreat from a battle, several armies selected, a mission confirmation, the stop button or a key.

## The army's action `0xd429e0`

`0xd429e0`(army, province, queue, from the interface), with u8 flags, returns a u8. It does nothing for an army with no manager at `+0x140`. A rejected order plays `illegal_moveorder`. Otherwise it makes one command, in this order:

- **Retreat**: for an army in a battle (`+0xc0` not null) whose method `+0xb0` says yes, a retreat command (class `0x1d1c438`, type `0x2b34`; check `0xd6bb30`, carried out by `0xd6b910`). This is the player retreating, not a defeat.
- **Cancel**: for the army's own province (`+0xe8`), when the order is not queued or the route is empty (its count at `+0x110`) and the army's method `+0x118` says no, a cancel command (class `0x1d1da88`, type `0x28b2`; check `0xd5e450`, carried out by `0xd5e1a0`) with `+0x60` set to `0x0101`. `0xd5e1a0` finds the army by its ID (`+0x58`) and goes on to the stop `0xd4fcb0`(army, byte `+0x61`, byte `+0x60`).
- **Move**: for another province, after querying the route (`0x96c520`), a move command made by `0xd5cd20`(command, army, province ID, replace, …), where replace is the queue flag inverted. The move command, class `0x1d1d3f8`, type `0x28a5` at `+0x44`: the army at `+0x50`, its ID at `+0x58`, the destinations (a vector of province IDs) at `+0x60`, a route list at `+0x78`, replace at `+0x90`, flags at `+0x91`, `+0x98` and `+0x99`, and the province the army is in at `+0x94`. Its method `+0x40` is the check `0xd5dee0`, `+0x48` carries it out, `0xd5cf10`. `0xd5ce40` makes one from a route list instead.

Each command is checked (its method `+0x40`) and then posted through `0x14ca2d0`(session, command), which returns 1 when it posted the command.

## Taken, then carried out

- **The army's action** `0xd429e0` makes the order. For a province other than the army's own it makes a move command (`0xd5cd20`, class `0x1d1d3f8`), calls the command's check (its method `+0x40`, `0xd5dee0`) and posts the command to the session (`0x14ca2d0`, at `0xd43a50`) whatever the check answered; the answer is only kept as the action's result. For the army's own province, when the army's method `+0x118` says no, it makes a cancel command (class `0x1d1da88`) and posts it the same way (`0xd42da2`).
- **The session takes every command** in a single player game ([recruiting.md](recruiting.md)), so taking one says nothing about what it will do.
- **The session carries the command out** in a later map manager frame. The move command's method `+0x48`, `0xd5cf10`, finds the army by its ID and calls the shared movement `0xd5cf90`, which sets the route through `0xd5d680` and the route install `0xd50340` (called at `0xd5d973`); the cancel command's `0xd5e1a0` ends in the stop `0xd4fcb0`. Tested: each right click that made a route was posted, the framework's safe point came 7 to 19 ms later, and the route was installed 2 to 12 ms after that, in the next frame; the install was called once or twice for one order.

## Not only the player's

Nothing after the map input belongs to the player's orders alone:

- `0xd5cf10` is called by the army's daily update `0xd3e110` too, which makes a move command (`0xd5cd20` at `0xd3e5f0`) and carries it out at once (`0xd3e5fa`); the missions in `0x1d0e20` make move commands of their own. The movement `0xd5cf90`, `0xd5d680` and the install `0xd50340` are shared by all of them.
- Neither way to make a move command is the player's: `0xd5ce40` is called by the AI's army routines `0xec760`, `0xf2e90` and `0x109ee0`, which post through `0xbfe50` (as does `0xeab50`, which names `ai_army.cpp`), and `0xe4f80` makes a move command inline and posts it the same way. `0xd5cd20` is called by the daily update, the AI, the retreat and others.
- The army's action itself is called by the retreat after a battle.

The input path above, with the action called at `0xad0c29` from `0xad0ba0` called at `0x116e3ea`, is the narrow way to tell a right click on the map from the rest, but it covers only that one way of giving an order. The framework does not observe it; it hears the order where it is carried out.

## Orders that change nothing (tested)

- **A move onto the sea**: a right click on the sea zone next to a coastal army, with no fleet there, posted a move command, and the route stayed as it was; the install was not called. This was so for an army with no route and for a movement-locked one with two provinces still to go, whose route was not cut either.
- **Clicks that make no order**: a right click on a sea zone away from the army (the Gulf of Valencia, from Toledo) or on an island it cannot reach (Mallorca) posted nothing.

## A movement-locked army (tested)

- `army+0x202` was set four or five days into a step to a neighbor. An order then keeps that step and puts the new route after it: from Asturias on the way to `4554` with `206` next, a right click on León made the route `[4554, 208]`; from Toledo on the way to `2754`, one on Albacete `[2754, 4547]` and one on Toledo itself `[2754, 219]`.
- **The same order again** left the same provinces, set again through the install: the framework found the route's nodes changed.

## What the framework hears

`game::events` observes the install `0xd50340` and the stop `0xd4fcb0` (`game::orders::MOVE` and `STOP`). For an own army, unless the framework itself is moving it, it reads the route before the call and after it, and hears an order when they differ, in their provinces or in their nodes. A posted command is not heard: until 2026-10-31 every posted movement command for an own land army was, and so was a move onto the sea that changed nothing (seen on 2026-10-30, for the Ottomans' army and the Sea of Marmara). An order is so heard once it is carried out, at the safe point after the frame that carried it out, not at the one between taking and carrying out. The install and the stop are shared, so a call of theirs that changes an own army's route for another reason is heard as an order all the same: the retreat after a lost battle, for one, stops the army (`0xd4fcb0` at `0x21ed6f`) and orders it away through the army's action. Read from the code; not checked in the running game.

## The framework's orders as commands

Read from the code on 2026-11-03; not yet checked in the running game. Since then `army.move` and `army.stop` post the commands the army's action makes for a right click, through the command layer, which has the session run them at once (ADR 0033); the route changes they make are the framework's own, as its direct calls' were, so the observer above does not hear them as orders.

- **The move command's written form** (its write `0xd5d9e0`; type `move_order`, `0x28a5`): `unit` (the army's ID, `{ id=… type=… }`, from `+0x58`: the u64 at `unit+0x20` holds the type in its low half and the number in its high half, as `0xce610` writes them), `province` (the destinations at `+0x60`, `{ 183 }`, written only when there are some), `path` (the route list at `+0x78`, only when it is not empty), `location` (`+0x94`), then `clear` (`+0x90`), `safe` (`+0x91`), `cancel_autofleet` (`+0x98`) and `clear_all_but_next` (`+0x99`), each only when set, and the base fields last.
- **What the right click sets** (`0xd43a23`, the constructor `0xd5cd20`): `clear` is the queue flag inverted, `cancel_autofleet` is 1, `safe` comes from the action's own flag (`bl`), `clear_all_but_next` from `r12b`, the destination is the province clicked, and `location` the province the army is in. The framework sets `clear` and `cancel_autofleet`, and leaves `safe` and `clear_all_but_next` out.
- **Carrying it out** (`0xd5cf10` → `0xd5cf90`): with a route list, the install `0xd50340`(army, list, clear, `0x18`); without one, for each destination `0xd5d680`(army, province, clear for the first only, safe), which queries the route (`0x96c520`, with the policy `0x1c3af98` unless `safe`, the one the framework's own route query takes) from the army's province, or from the end of its route when not clearing, and installs what it found with `0x18`. A destination with no route is passed over, and nothing says so: the framework asks the route query first and refuses such a goal itself.
- **The cancel command** (class `0x1d1da88`, type `cancel_movement`, `0x28b2`), which the right click on the army's own province makes: its write `0xd5e1f0` writes the base fields, `unit`, `cancel_autofleet` (`+0x60`, `yes` or `no`) and `reason` (`+0x61`, a number); the right click sets both to 1 (`0x0101` at `+0x60`), and its execute `0xd5e1a0` ends in the stop `0xd4fcb0`(army, `reason`, `cancel_autofleet`). Its check `0xd5e450` finds the army by its ID and refuses a retreating one (`+0x203`), among others not read.
