# Routes: finding one, moving, stopping (for `game::orders`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` while regional-combat was developed (2026-10-26), and checked again with `core/examples/disasm.rs` on 2026-11-03: every address below that the framework uses, and those marked "checked", hold the bytes or instructions described. What was seen in the running game on 2026-10-26, before the framework existed, is marked "seen 2026-10-26"; it is not in [prototype-log/](../prototype-log/). What was checked in the running game through the framework is marked "tested", from that log. Everything else was read from the code only. How a right click becomes a move or a stop, and how `game::events` hears the player's orders, is in [army-orders.md](army-orders.md).

## A route

- **An army's route** is a list of the game's usual kind (`objects::list`) at `army+0x100`: the first node, `+0x108` the last, `+0x110` the count (i32) and `+0x114` a byte set while the list is being changed. A node is 0x20 bytes: `+0x00` the province ID (i32), `+0x08` the previous node, `+0x10` the next, `+0x18` a byte set when the node is to be removed later. The unit constructor clears the list at `0xd3ae28` (dependency point "unit route").
- **The route holds the provinces still to go**: the next one first, the destination last. The province the army stands in, `army+0xe8`, is not in it.
- **Nodes** are allocated with `0x1a332d4(0x20)` (the route install calls it at `0xd5073f`, checked) and freed with `0x17394f0(node, 0x20)` (a jump to the game's sized delete; dependency point "route node free").

## The route query `0x96c520`

```
bool query(Policy* policy, Army* army, Province* origin, Province* goal,
           List* out, Options* options, scratch, u8* visited)
```

- **Eight arguments** (checked: the function reads 8). The answer is the byte in `al`; the rest of `rax` means nothing. `out` and `visited` may be null. The seventh slot is never read before the function writes it: it zeroes its low 32 bits at `0x96c555` and uses it as its own scratch, so callers leave it uninitialized. `visited`, when given, gets a copy of the search's closed set, one byte per province ID; since the search stops at the goal, it is not the set of provinces the army can reach, and the one-step shortcut below skips the copy.
- **It searches with weighted edges and a heuristic**, not by counting steps. The edge cost `0x96bd00`(policy, &cost, army, current province, neighbor index, options) is called directly at `0x96ccb9` (checked), not through the policy; a negative cost skips the edge. It starts from the i32 at `+0x0c` of the neighbor's record (`objects::campaign::adjacency`, read at `0x96bd65`, checked) and adds penalties for country, control, zone and the options, in steps such as 2000, 8000, 24000, 32000, 160000, 200000 and 400000; it also calls the army's method at `+0x138`. The heuristic `0x96c400`(&h, goal, candidate) is called directly at `0x96ccd6` (checked): a distance between the provinces' map positions, wrapping around the map's sides, in fixed point. At `0x96ccdf` (checked) the options' byte `+7` picks `10 × h` when set and `900 × h / 1000` when clear; a node's priority is its cost so far plus the edge cost plus the scaled heuristic (`0x96cd2f`..`0x96cd39`).
- **What it writes**: on success the list builder `0xe9720` walks the predecessors back from the goal and pushes each province ID to the front, the origin included; at `0x96ce34` (checked) `0x10c9b0` takes the first node off again. So `out` holds next province to goal, the origin left out, and its count is the number of steps of the route the search chose. When the origin is the goal the answer is true with an empty list, so an empty list alone does not say the search failed. The search takes the first node off, so `out` must start empty.
- **Before searching** it asks the policy's method `+0x10` whether the army may set out from the origin for the goal (`0x96c56d`, checked) and answers false at once if not.

### The policy

The policy is an object whose first word is a vtable of four methods.

| Vtable | `+0x00` | `+0x08` edge check | `+0x10` start check | `+0x18` goal test |
| --- | --- | --- | --- | --- |
| `0x1c3af98`, normal movement | `0xd0210` | `0x96f860` | `0x96f510` | `0x96fd50` |
| `0x1c3af78`, the rebel hunt | `0xd0240` | `0x970f60` | `0x96f510` | `0x96fd50` |

Both checked. The framework's dependency points name the normal policy through the stop, which loads its address at `0xd4fcef` (dependency point "army followers") and again at `0xd4fd96` (checked).

- **Edge check** (`0x96f860`, `0x970f60`; checked: 7 arguments): (policy, army, goal, current, neighbor, neighbor's record, options) → `al`. The normal one checks access, impassable provinces (dependency point "province impassable" at `0x96f88d`), the kind of connection, straits and the units in their sea zone (dependency point "strait units" at `0x96f691`), forts and the army's route so far.
- **Start check** `0x96f510` (checked: 5 arguments): (policy, army, origin, goal, options) → `al`. When the options' byte `+2` is clear it asks the army's method `+0x120` about the goal, passing the tag at options `+0x14` and the pointer at `+0x38`.
- **Goal test** `0x96fd50`: `cmp rdx, r8; sete al; ret` (checked), whether the candidate is the goal.
- **The rebel hunt's edge check** `0x970f60` calls the normal one first, then adds checks of strength: it compares the goal's `+0x590` / 10 with the army's regiments × 1000, and on some paths calls `0x96fd60`, which goes over the armies in the province and its neighbors and computes a ratio, and `0x9713d0`. So the rebel hunt's policy is the normal one with the odds of winning added, not a policy without them. Its object has a byte at `+8`, set to 0 where the rebel hunt makes it (`0x1c8420`).

### The options

0x48 bytes the caller fills (`objects::campaign::route_options`). What each field does, as read from the code; the game's names for them are not known.

| Offset | Size | What it does |
| --- | --- | --- |
| `+0x00` | u8 | Clear: the edge check asks the strait blockade query `0x96f650` ([provinces.md](provinces.md)). Set: skips it, and changes the branches between land and sea. |
| `+0x01` | u8 | Set: refuses every connection of kind 1 (a strait), land straits too. |
| `+0x02` | u8 | Set: skips the start check's question to the army (`+0x120`) and the refusals that go with it. It does not make a route succeed. |
| `+0x03` | u8 | Set: skips the blocking check `0xd45250`, which reads the army's route so far. |
| `+0x04` | u8 | Passed to the army's `+0x120`; changes several zone branches and costs; refuses a neighbor whose map data has bit `0x10` of `+0x228` (checked at `0x96cc85`). |
| `+0x05` | u8 | Changes the cost of edges between zones: some return 0, others add 400000. |
| `+0x06` | u8 | Set: skips an edge cost increase for some province bits and fog. |
| `+0x07` | u8 | The heuristic's scale (above). No value turns it off. |
| `+0x08` | u8 | Allows a one-step shortcut, tried only when `out` is given and empty (checked at `0x96c58d`): it looks among the origin's neighbors for the goal, pushes the goal's ID and answers true. |
| `+0x09` | u8 | Passed to `0xd45250` as its fourth argument: set, it copies the army's own route and reads it backwards, adding the army's province when needed. |
| `+0x0c` | i32 | Passed to `0x9dd6a0`(candidate, army, value) on the way from land to sea. |
| `+0x10` | i32 | Above 0, and the army's method `+0x100` positive: each edge costs `8 × value` more. |
| `+0x14` | tag | A country tag, passed to the army's `+0x120`; `---` (`0x2d2d2d`) names no country. Not aligned. |
| `+0x20` | i64 | Used by the rebel hunt's strength check (`0x96fd60`). |
| `+0x28` | f32 | Passed to `0x9713d0` when the rebel hunt's byte `+8` is set. |
| `+0x2c` | u8 | Turns on a cost that uses the object at `+0x30`. |
| `+0x30` | ptr | The rebel hunt's context for `0x9713d0`; null means a global made on first use. |
| `+0x38` | ptr | Passed to the army's `+0x120`. |
| `+0x40` | u8 | Set: `out` holds one node whose item is the number of provinces on the route (steps + 1), not a province ID (`0x96ce01`, checked). The search is the same. The one-step shortcut ignores this byte and still writes a province ID. Such a list must never be installed. |

`+0x0a`, `+0x0b`, `+0x1c`..`+0x1f` and `+0x2d`..`+0x2f` were not seen used; the game leaves garbage in some of them (seen 2026-10-26: `+0x1c` read `df 01 00 00`), so they cannot be required to be zero.

### Who asks for a route

- **The map's preview of a move**, as the mouse moves with an army selected: `0xd45c60` calls the query at `0xd45d1b` (checked). Seen 2026-10-26: 32 queries from there, all with the normal policy, the eighth argument null, and the named options as below.
- **The shared movement** `0xd5d680` (reached from the move command, [army-orders.md](army-orders.md)) builds the options in two ways. When the order is to be added to the army's route (`replace` clear) and the army has a route whose last province resolves, it queries from that last province with `+0x08` and `+0x09` both set (`0xd5d779`, checked). Otherwise it queries from the army's province, `army+0xe8`, with `+0x08` set and `+0x09` clear (`0xd5d860`, checked). Both use the normal policy, `+0x0c` = −1, the tag `---`, and zero elsewhere.
- **The rebel hunt** (the policy is set up in `0x1c8420`, its task update is `0x1c9d30`) fills the options with `+0x08` and `+0x09` set, `+0x0c` = −1, the tag `---` and zero elsewhere (dependency point "route options" at `0x1c878e`), stores 0 into the eighth argument's slot at `0x1c87d5` (checked) and leaves the seventh alone.
- **A retreat** `0xd6b910` queries itself (`0xd6b9d7`, checked) before installing the route.

### Route history

`0xd45250`(army, candidate, current, `options+0x09`) (checked: 4 arguments) reads the army's province `+0xe8`, two province pointers at `+0xf0` and `+0xf8` that remember where the army came from, its route `+0x100`, and the forts and relations of the provinces involved. So a query depends on where the army really is and has been, not only on the origin passed in: a query from another origin is a mix of that origin and the army's own history, not the route of an army standing there. What writes `+0xf0` and `+0xf8` is not known.

### Sea zones and straits

- **The normal policy can lead a land army onto the sea**: in the edge check at `0x96fc67` (checked), a neighbor that is not land (bit `0x08` of its map data's `+0x228` clear), with options `+0x00` and `+0x04` clear, goes to `0x9dd6a0`(neighbor, army, options `+0x0c`); if that finds something, the edge stays open. `0x9dd6a0`(sea zone, army, regiments) walks the sea zone's units (`+0x350`) and answers yes for the first fleet (the unit's method `+0x88`) that has no other context (`+0x98`) than the army's and that `0x90acf0`(fleet, army, regiments) lets take the army aboard: room for `regiments` beside those aboard, and for all the army's when it is negative, as the −1 every caller gives (docs/research/units.md, "Transport as the player does it"). So a route the game chooses may hold sea zones. The framework passes it on as the game chose it.
- **A strait does not put its sea zone in the route**: the search takes the neighbor's province ID from the record's `+0x00` and outputs only provinces it went through; the sea zone at the record's `+0x08` is checked by the edge check but never output. So a route across a strait is all land. Tested (2026-10-30, "Regional combat's radius box, straits, defeat and ironman saves"): the game's route from Thessaly to Kocaeli was `[148,4705,4779,2296,317,316]`, across the Dardanelles. Options `+0x01` refuses straits altogether, so it is no way to keep a route off the sea.

### Not the fewest steps

The count of a route is the steps of the route the weighted search chose, not the fewest steps to the goal, and there is no way to ask for that. The edge cost and the heuristic are direct calls, not policy methods, so a policy of one's own cannot change them; no option makes every edge cost the same (`+0x05` and `+0x10` change some edges only), and none turns the heuristic off. Counting steps with the game's own rules would need a search of one's own that carries the route history (`+0xf0`, `+0xf8`, the route) along each path, and how the game changes that history step by step is not known. Asking the edge check once per pair of provinces with the real army is not enough, since the check does not take the history of the search's node.

## Moving: the route install `0xd50340`

```
void install(Army* army, const List* route, u8 replace, u8 reason)
```

- **Four arguments in registers** (checked), no return value: nothing the function returns says whether the route was taken. The prologue (dependency point, observed by `game::events`) is `48 89 5c 24 20 44 88 44 24 18 48 89 54 24 10 55`.
- **When the army is in a battle** (`army+0xc0` set), it goes on only if the battle's method `+0x70` says so (`0xd50425`, checked).
- It may take the army out of its siege (`+0xc8`), clear battle state, and change the armies attached to it: it calls itself for each follower with `reason` 0 (`0xd50ba6`, checked).
- **`replace` set**: it calls `0xd3b8b0`(army, 1, reason, 0, 0) (`0xd5057a`, checked; 5 arguments), which handles the army's task at `+0x1f8` and tells the player; `reason` is passed on as a byte, and `0xd3b8b0` tells apart 0, 1, `0x35`, `0x36` and the rest, whose names are not known. Then `0xd50ec0` (`0xd5058b`, checked; below), clears the old route, and sets the movement lock `army+0x202` to 0 (`0xd50605`, checked). It resets the dword at `+0x130` when the old and new first provinces differ or either route is empty, and keeps it when they are the same.
- **Then it copies**: from the army's province (`+0xe8`, dependency point "army location" at `0xd5060d`) when replacing, or from the route's last province when adding, it looks for each next ID among the current province's neighbors, and for each one found allocates a node and appends it (dependency point "route node" at `0xd50733`). At the first ID that is no neighbor it stops, and the provinces copied so far stay as the route. The copy asks nothing else: no access, no blocked strait, no forts, no history. A list of neighbors made up by hand would bypass every rule the query applies, so only a route the query just made from the army's real position is safe to install.
- **It copies, never takes**: the caller still owns the list it passed and frees its nodes afterwards, as the rebel hunt does. Seen 2026-10-26: a right click's route of 35 provinces was installed into a node chain with the same IDs and other nodes.

The callers:

| Caller | Where | Arguments |
| --- | --- | --- |
| The rebel hunt's task update `0x1c9d30` | `0xd50340` called at `0x1ca4d9` (checked) | (army, list, 1, 0); before it, the stop (army, 0, 0) at `0x1ca483` (checked) when the army has a route |
| The shared movement `0xd5d680` | `0xd5d973` (checked) | (army, list, replace, `0x18`) |
| The install itself, for each follower | `0xd50ba6` (checked) | (follower, list, replace, 0) |
| A retreat `0xd6b910` | `0xd6b9eb` (checked) | (army, list, 0, 0) |

Seen 2026-10-26: one right click with the game paused called the install from `0xd5d973` with `replace` 1 and `reason` `0x18`, and once more from `0xd50ba6` for the army attached to it with `reason` 0; both armies had no route before and 35 provinces after.

## Stopping: `0xd4fcb0`

```
void stop(Army* army, u8 reason, u8 leave_context)
```

- **Three arguments in registers** (checked), no return value. Prologue `48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57` (dependency point, observed by `game::events`).
- **It stops the army whatever its state**: there is no check of the movement lock, a retreat, a battle, exile or who owns the army. It writes `+0x8c` and `+0x90`, stops each follower (`+0x58`) by calling itself with the same `reason` and 0 (`0xd4fd23`, checked), but only a follower whose method `+0x118` says it is not locked (`0xd4fd0f`, checked; for a land army `0xd4f2a0`: `movzx eax, byte [rcx+0x202]; ret`, checked); the army itself gets no such check. It resets `+0x130`, sets the movement lock `+0x202` to 0 (dependency point "army movement lock" at `0xd4fda4`), calls `0xd50ec0` (`0xd4fdae`, checked), frees the route's nodes and clears the list (dependency point "army route clear" at `0xd4fdf2`).
- **`reason`** set: when the task's method `+0x70` allows, it is passed to `0xd3b8b0`(army, 1, reason, 0, 0) (`0xd50156`, checked).
- **`leave_context`** set, with `army+0x98` set: `0x1d3c90`(`army+0x98`, army, 1) (`0xd5017a`, checked), then `+0x98` is cleared and another list at `+0x118` updated.
- At `0xd4ff16`, an army standing in a province controlled by an enemy, not in a battle, siege or retreat, may start a siege; this reaches followers and fleets too. So an empty route afterwards does not show the stop did nothing else.
- **`0xd50ec0`**, called by the stop and by a replacing install: when `army+0xa0` is set, `0xd76860`(`army+0xa0`, army) and `+0xa0` cleared; the vector at `+0xa8`..`+0xb8` freed and zeroed (the whole function checked). What these hold is not known (`objects::campaign::unit::CONTEXTS`).

The callers:

- **The rebel hunt**: (army, 0, 0) before a new route, when the army has one (`0x1ca483`, above).
- **The stop command**: the army's action `0xd429e0` makes it for a click on the army's own province ([army-orders.md](army-orders.md)) and writes 1 to both of its bytes `+0x60` and `+0x61` (`0xd42d61`, checked). Its execution `0xd5e1a0` finds the army by the ID at the command's `+0x58`, keeps the pointer at `+0x50`, loads `reason` from `+0x61` and `leave_context` from `+0x60`, and jumps to the stop (`0xd5e1c1`..`0xd5e1cf`, checked): so the player's stop is (army, 1, 1).
- **The stop command's check** `0xd5e450` (checked) answers false when the army is not found or is retreating (`+0x203`); for an army whose method `+0x88` says yes and whose route's first province is land, it also asks the army's `+0x120` about that province with the tag `---`. It writes the command's pointer at `+0x50`, so it cannot be asked with a made-up command, and it is no general check of whether an army takes orders.
- **Battles' retreats and defeats** in `0x21eb30`: (army, 1, 0) at `0x21ed6f` and `0x21ee3e` (checked).

`0xd42850`(army, province) → `al` (checked: 2 arguments) is the army's action's refusal check: nonzero sends the action to the "cannot go there" path. It looks at the regiments, the retreat bytes, the army's `+0xb0` and `+0x205`, a battle, `army+0x50`, the army's kind, the country, whether the province is seen and known, and access. Read from the code only, never called on its own.

## What the framework does (`game::orders`)

- **Dependency points**: the query `0x96c520` and the node free `0x17394f0` are declared by `game::orders`; the install `0xd50340` (`orders::MOVE`) and the stop `0xd4fcb0` (`orders::STOP`) by `game::events`, which observes them ([army-orders.md](army-orders.md)). The fields are in `objects::campaign` (`unit`, `route_options`, `list`), with the points named above.
- **Which armies it orders**: only the player's own land armies (`army` vtable `0x1c42540`), and not one in a battle, besieging, retreating, exiled, movement-locked, attached to another army, with others attached to it, with a task, or without regiments (`checks::armies::orderable`). Neither the install nor the stop checks any of this, and both clear the movement lock (`+0x202`), so calling them on such an army would let it out of what the game holds it to; with followers, attachments or a task, the calls change more than the route. An extension's orders also wait while the game is paused; remote control's are the player's and go while paused, as a right click does (tested, Z2).
- **The query**: from the army's province with the normal policy and the options as the shared movement's replacing branch fills them (`+0x08` set, `+0x09` clear, `+0x0c` = −1, the tag `---`, the rest zero), `+0x09` clear because the query is always from where the army stands; the seventh and eighth arguments are 0. Into a list of its own, on the main thread, at a safe point. It checks that the army's state (province, route, identity, regiments, date) is the same after the query, that the list reads whole (count, links, no node to be removed, the last node), and that the ends are right: empty when the goal is the army's province, else ending at the goal and not starting at the army's province.
- **Freeing the query's list**: only after checking that none of its nodes is in the army's route, before or after; each with `0x17394f0(node, 0x20)`.
- **Moving and stopping** (`army.move`, `army.stop`, an extension's `move`): since 2026-11-03 the framework has the game execute the move and cancel commands a right click makes, through the command layer, at once, which end in the install and the stop ([army-orders.md](army-orders.md), "The framework's orders as commands"; ADR commands-run-at-once). Until then it called the install as the rebel hunt does, (army, list, 1, 0), inside its own query, and the stop as (army, 0, 0); the tests below were of that way. Either way the army's route must afterwards have exactly the queried provinces, or be empty after a stop, or the order fails.
- **After a native fault in the query**, routes stay off for the rest of the process: what the fault left half done may lie outside the world, and nothing shows a load rebuilds it. **After a list it could not safely free**, which it leaves as it was among the world's armies, routes stay off until the world is replaced, by a load or a world reset (an in-process return); a change of country keeps the world, and them off.
- **Whose route it is**: a route an extension installed is that extension's while the army's route is what is left of it: the provinces not yet reached, in the same nodes, and the army standing where that part begins. A remote client's move or stop is the player's order: it takes the army back from any extension and raises `army_ordered` once.

Tested (F1, 2026-10-29): an extension's moves to 146, 151 and 316 and a stop of its own route were accepted, and the army marched. Tested (F2, F4, 2026-10-29): `army.route_to` (then `army.find_route`), `army.move` and `army.stop` through remote control, the route read back as ordered, an `army_ordered` event for each, and refusals for an army with others attached and one in a battle. Tested (Z2, 2026-10-30): the player's route queries, moves and stops while the game is paused.

## Seen in the running game on 2026-10-26

- **A query called from the main thread**, with the normal policy and all eight arguments, for a Castilian land army standing in 149: 149 itself answered true with an empty list; 151 and 2750 one step each; 222 thirty-three steps, ending in 222. The army's route, province and identity were the same before and after, and the list's nodes were all freed with `0x17394f0(node, 0x20)`. That query had options `+0x09` set; the framework has used the shared movement's replacing options (`+0x09` clear) since.
- **A right click's install**, while paused, as in the table of callers above: it shows the install takes a right-click route while paused, not that it does in a battle, a retreat or under a movement lock.

## Not known

- The game's names for the options, for `reason` (only 0, 1, `0x18`, `0x35` and `0x36` were seen), for `leave_context`, for `+0x130`, `+0xa0`..`+0xb8` and the army methods `+0x88`, `+0x100` and `+0x118` beyond what is said here, and what writes `+0xf0` and `+0xf8`.
- The install called the framework's way, (army, list, 1, 0), was not compared with the rebel hunt's own calls in the running game, nor kept under watch over the following frames; the framework checks the route right after the call only. The stop (army, 0, 0) was tested only through the framework's orders above.
