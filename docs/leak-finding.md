# Leak finding plan

Status: decided (2026-11-04). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Developer build). What the game leaks of a session, and what the repairs let go, is in [sessions.md](sessions.md#known-leaks) and [research/main-menu.md](research/main-menu.md).

## Goal and scope

- **Goal**: a developer build that finds where the game's memory goes as sessions are left and begun, and why it stays, with as little looking by hand as can be: which code allocated what stays, what still points at it, and where blocks of its kind are let go when they are. And that tells whether a repair is safe: whether anything touches what it let go.
- **In scope**: the heap census, which is the `dev-heap-census` feature, and what it answers through remote control; a leak run, a tool script that plays sessions through remote control and writes a report; the quarantine for repairs.
- **Out of scope**:
  - repairing anything automatically: the report says where to look, and a person works the repair out from the game's code;
  - tracking regressions: a leak run gives numbers to compare, and nothing fails on them;
  - blocks outside the game's C runtime heap: Direct3D's, the driver's, other modules' heaps. These are counted as a whole, not by call site;
  - handles, which are counted and not followed.

## Behavior

### The census

The census counts every live block of the game's C runtime heap: the code that allocated it (its site), its size, and its generation.

- **Generations.** A generation ends at every world reset, and when remote control asks for one to end (`memory.generation`), say inside one long game. A **leftover** is a block still live two generations after its own: it outlived two resets of the world it was made in.
- **The site.** A site is the return address of the allocation. A block of a large buffer from the game's generic allocation, or of a string growing, is counted instead at the two calls above it, which say whose buffer it is.
- **Where blocks of each site are let go.** For every site, the census keeps the places that let its blocks go, and how many each one let go. A site whose leftovers never reached the place that lets its other blocks go points at the destructor or the cleanup that did not run.

### What remote control answers

Only in a developer build with the heap census do these capabilities exist; a player's build has neither the capabilities nor their descriptions.

- `memory` gives what the process holds now:
  - private bytes, and how much of them the heaps have allocated and committed;
  - what lies outside the heaps (Direct3D, the driver);
  - the framework's own heap;
  - the handles;
  - the census's live blocks and bytes by generation.
- `memory.leftovers` gives the leftovers by site, largest first. For each site:
  - the function the site is in;
  - the blocks and bytes each generation left there;
  - the bytes a generation leaves there, on average over the last three;
  - its **trend**:
    - `steady`: each of the last generations left some, which is a leak of every session;
    - `first`: only the first generations left any, which is a cache filled once;
    - `sometimes`: anything else;
  - the places that let its blocks go.
- `memory.generation` ends the generation now and answers the new one's number.

### A leak run

`tools/leak-run.ps1` plays rounds on a developer build installed with the heap census. A round is:

1. load a save from the main menu, and let time run;
2. load another save from the game, and let time run;
3. return to the main menu;
4. start a new game, and let time run;
5. return to the main menu.

The saves are chosen with `-Saves`, or else the two newest that `menu.saves` lists. The country of the new game is chosen with `-Country`, FRA by default.

After every round the run asks `memory` and `memory.leftovers`, and prints one line: private bytes, the heaps, the framework's heap, the handles. At the end it writes `report.json`, with every answer, and `report.md`, with:

- a table of the rounds;
- the leftover sites ordered by what each leaves per generation, with their trend, function and the places that let their blocks go.

### Who still points at a leftover (M2)

At the main menu after a world reset, the census looks for what points at each leftover. It reads every live block it counts, the game's writable data and the threads' stacks, and takes any eight bytes that hold the start of a leftover as a pointer to it. Each leftover site then gets:

- **held by the game's data at an RVA**: a registry or a cache the game keeps;
- **held by blocks of another site**: its owner, which is itself followed up to what holds it;
- **held by nothing**: a block nothing can reach, where something forgot to let it go.

This stops the game for as long as the reading takes, seconds at the main menu.

### The call paths of chosen sites (M3)

In dev.toml, `[heap-census]` `paths` names sites. Each block allocated at one of them is kept with the calls above it, and `memory.leftovers` adds the paths that leftovers took. A leak run with `-Paths` runs twice: the first run finds the steady sites that leave the most, the second runs with their paths.

### The quarantine (M4)

With the `dev-quarantine` feature, the blocks a repair lets go, and the blocks that frees in turn, are kept instead of freed and filled with a pattern. Every two seconds they are compared, and the first write to each is logged with the repair that let it go. `memory` adds what the quarantine holds and what was written to. A leak run with `-Quarantine` ends by saying, for each repair, whether anything was written to what it let go.

## How it fits together

- `remote` declares the `memory` group behind its `dev-memory` feature, which the core's `dev-heap-census` turns on. A workspace build of the developer build gives the command line tool the same group.
- `core/src/census.rs` keeps the blocks and answers the group. Building the answer from the census's figures (the leftovers by generation, the trend, the average) is a function of its own, tested without a game.
- `tools/leak-run.ps1` uses only remote control, as `perf-run.ps1` does.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and the developer build;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md).

### M1 The census answers through remote control

The `memory` group; generations that remote control ends; sites counted above the generic allocation; where each site's blocks are let go; the leftovers with their function, trend and average; `tools/leak-run.ps1` and its report.

### M2 Who still points at a leftover

The scan, and what holds each leftover site in `memory.leftovers` and the report.

### M3 The call paths of chosen sites

`paths` in dev.toml, the paths in `memory.leftovers`, and the leak run's second run.

### M4 The quarantine

The `dev-quarantine` feature, the repairs saying what they let go, and the leak run's verdict.
