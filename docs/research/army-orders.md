# Army orders: from a right click to the route (for `game::events`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` with `core/examples/disasm.rs`; what was checked in the running game is marked "tested", from [prototype-log.md](../prototype-log.md) (2026-10-31, army orders). The path from the map input to the army's action is in [regional-combat/native-manual-findings.txt](regional-combat/native-manual-findings.txt).

## Taken, then carried out

- **The army's action** `0xd429e0`(army, province, queue, from the UI) makes the order. For a province other than the army's own it makes a move command (`0xd5cd20`, class `0x1d1d3f8`), calls the command's check (its method `+0x40`, `0xd5dee0`) and posts the command to the session (`0x14ca2d0`, at `0xd43a50`) whatever the check answered; the answer is only kept as the action's result. For the army's own province, when the army's method `+0x118` says no, it makes a cancel command (class `0x1d1da88`) and posts it the same way (`0xd42da2`).
- **The session takes every command** in a single player game ([recruiting.md](recruiting.md)), so taking one says nothing about what it will do.
- **The session carries the command out** in a later map manager frame. The move command's method `+0x48`, `0xd5cf10`, finds the army by its ID and calls the shared movement `0xd5cf90`, which sets the route through `0xd5d680` and the route install `0xd50340` (called at `0xd5d973`); the cancel command's `0xd5e1a0` ends in the stop `0xd4fcb0`. Tested: each right click that made a route was posted, the framework's safe point came 7 to 19 ms later, and the route was installed 2 to 12 ms after that, in the next frame; the install was called once or twice for one order.

## Orders that change nothing (tested)

- **A move onto the sea**: a right click on the sea zone next to a coastal army, with no fleet there, posted a move command, and the route stayed as it was; the install was not called. This was so for an army with no route and for a movement-locked one with two provinces still to go, whose route was not cut either.
- **Clicks that make no order**: a right click on a sea zone away from the army (the Gulf of Valencia, from Toledo) or on an island it cannot reach (Mallorca) posted nothing.

## A movement-locked army (tested)

- `army+0x202` was set four or five days into a step to a neighbor. An order then keeps that step and puts the new route after it: from Asturias on the way to `4554` with `206` next, a right click on León made the route `[4554, 208]`; from Toledo on the way to `2754`, one on Albacete `[2754, 4547]` and one on Toledo itself `[2754, 219]`.
- **The same order again** left the same provinces, set again through the install: the framework found the route's nodes changed.

## What the framework hears

`game::events` observes the install `0xd50340` and the stop `0xd4fcb0` (`game::orders::MOVE` and `STOP`). For an own army, unless the framework itself is moving it, it reads the route before the call and after it, and hears an order when they differ, in their provinces or in their nodes. A posted command is not heard: until 2026-10-31 every posted movement command for an own land army was, and so was a move onto the sea that changed nothing (seen on 2026-10-30, for the Ottomans' army and the Sea of Marmara). An order is so heard once it is carried out, at the safe point after the frame that carried it out, not at the one between taking and carrying out.
