# Leak finding plan

Status: decided (2026-11-04); M1 to M5 built and tried in the game the same day, M6 on 2026-11-05 ([prototype-log/](../prototype-log/), from [The heap census through remote control](../prototype-log/2026-11-04-heap-census-through-remote-control.md) to [What grows outside the census](../prototype-log/2026-11-05-growth-outside-census-window-cloaked.md)). Terms are in [GLOSSARY.md](../../GLOSSARY.md) (Developer build). What the game leaks of a session, and what the repairs let go, is in [sessions.md](sessions.md#known-leaks) and [research/main-menu.md](../research/main-menu.md).

## Goal and scope

- **Goal**: a developer build that finds where the game's memory goes as sessions are left and begun, and why it stays, with as little looking by hand as can be: which code allocated what stays, what still points at it, and where blocks of its kind are let go when they are. And that tells whether a repair is safe: whether anything touches what it let go.
- **In scope**: the memory account, which is the `dev-memory` feature, and splits what the process holds by where it lies; the heap census, which is the `dev-heap-census` feature, and what it answers through remote control; a leak run, which plays sessions through remote control and writes a report (a tool script first, now the scenarios `leak` and `growth`); the quarantine for repairs.
- **Out of scope**:
  - repairing anything automatically: the report says where to look, and a person works the repair out from the game's code;
  - tracking regressions: a leak run gives numbers to compare, and nothing fails on them;
  - blocks outside the game's C runtime heap: Direct3D's, the driver's, other modules' heaps. These are counted as a whole, not by call site;
  - handles, which are counted and not followed.

## Behavior

### The memory account

The memory account reads what Windows keeps of the process and its heaps when remote control asks, and nothing between two asks, so a run can measure without the census, which slows the game and holds hundreds of megabytes of its own. It splits the private bytes into:

- the game's heap, what the game holds there: the game's C runtime allocates from the process's default heap, which the framework shares, so this is what that heap has allocated less the framework's heap;
- the other heaps, what they hold;
- the heaps' free pages: what they committed and do not use;
- what lies outside the heaps: Direct3D's, the driver's, other whole pages;
- the framework's heap.

### The census

The census counts every live block of the game's C runtime heap: the code that allocated it (its site), its size, and its generation.

- **Generations.** A generation ends at every world reset, and when remote control asks for one to end (`memory.generation`), say inside one long game. A **leftover** is a block still live two generations after its own: it outlived two resets of the world it was made in.
- **The site.** A site is the return address of the allocation. A block of a large buffer from the game's generic allocation, or of a string growing, is counted instead at the two calls above it, which say whose buffer it is.
- **Where blocks of each site are let go.** For every site, the census keeps the places that let its blocks go, and how many each one let go. A site whose leftovers never reached the place that lets its other blocks go points at the destructor or the cleanup that did not run.

### What remote control answers

Only in a developer build with the memory account do these capabilities exist; a player's build has neither the capabilities nor their descriptions. Without the census, `memory.leftovers` and `memory.generation` answer `unavailable`.

- `memory` gives what the process holds now:
  - private bytes, and how much of them the heaps have allocated and committed;
  - the memory account's parts, and each heap, with which is the game's and which the default;
  - the handles;
  - whether the census is built in, and its live blocks and bytes by generation.
- `memory.leftovers` gives the leftovers by site, largest first, and with `holders` what still points at them (M2, below). For each site:
  - the function the site is in;
  - the blocks and bytes each generation left there;
  - the bytes a generation leaves there, on average over the last three;
  - its **trend**:
    - `steady`: each of the last generations left some, which is a leak of every session;
    - `first`: only the first generations left any, which is a cache filled once;
    - `sometimes`: anything else;
  - the places that let its blocks go.
- `memory.watch` watches sites, for who let their leftovers' owners go (M3, below).
- `memory.generation` ends the generation now and answers the new one's number.

### A leak run

The scenarios `leak` and `growth` ([scenarios.md](scenarios.md)) play rounds, `leak` on the build with the heap census and `growth` on the build with the memory account only. A round is:

1. load a save from the main menu, and let time run;
2. load another save from the game, and let time run;
3. return to the main menu;
4. start a new game, and let time run;
5. return to the main menu.

The saves and the country of the new game are the scenarios' parameters, named in their code: England-1 and 英格兰1444_11_11, and FRA; `--leak.first_save` and the like change them for a run.

After every round the run asks `memory`, and with the census `memory.leftovers`, and prints one line: private bytes, the memory account's parts, the handles. At the end it writes `report.json`, with every answer, and `report.md`, with:

- a table of the rounds;
- where each round's growth went, part by part, as the least-squares slope over the rounds from the second on (the first round loads what stays for good, and one round's figures jump about), and each heap's growth;
- with the census, the leftover sites ordered by what each leaves per generation, with their trend, function and the places that let their blocks go, and what holds them.

### Who still points at a leftover (M2)

`memory.leftovers` with `holders` looks for what points at the leftovers of the sites it gives. With every shard of the census locked, so that none of the blocks it counts is let go meanwhile, it reads the game's writable data and every live block it counts. It takes any eight bytes that hold an address in a leftover's first 64 bytes as a pointer to it; an object's bases and a large buffer's alignment lie there. A block pointing at itself does not count. Each leftover is counted once, by the best that points at it:

- **by data**: the game's writable data points at it, a registry or a cache the game keeps;
- **by blocks**: a live block that is not a leftover given points at it, its owner;
- **by leftovers**: only leftovers given point at it, such as the next one in a list nothing else reaches;
- **by nothing**: nothing points at it, memory something forgot to let go.

Each site also gets the five that point at its leftovers most: an RVA of the data, or a block's site with its function. An owner's site is in the same answer when it is a leftover site too. The threads' stacks, other modules' memory and Direct3D's are not read: at the main menu no stack holds a session's leftover for long.

The game's threads wait at their next allocation while the reading lasts, seconds at the main menu; a leak run asks for holders after its last round only.

### Who let a leftover's owner go (M3)

A leftover nothing points at was lost somewhere: most often its owner was let go, and the owner's destructor did not let it go too. `memory.watch` names sites to watch, at most 16, from then on in the same process. While some are watched, every block the game lets go is read first, and for each live block of a watched site the census keeps the last block let go while it pointed into it, with the code that let it go. `memory.leftovers` with `holders` then gives, for each watched site, its leftovers nothing points at by their last owner: the owner's site and function, and the place that let the owner go, which is in the destructor or the code that deletes it. Those with no owner let go are counted apart: they were lost before watching began, or the pointer was written over.

An owner let go after its pointer was copied elsewhere, as a vector's buffer is when it grows, is kept too, and is the last owner until a later one; for a leftover nothing points at, the last owner let go is the one that lost it.

A leak run with `--leak.watch N` watches, after round `--leak.watch_after`, the N steady sites that leave the most and whose leftovers nothing (or only other leftovers) mostly points at, and adds a table of who let them go; `--leak.watch_sites` names the sites instead, as `memory.leftovers` writes them and whatever their trend, separated by commas. Reading every block let go slows the game while sites are watched.

Both this and M2 take a word for a pointer by one rule, in one module (`core/src/census/pointers.rs`): eight bytes that hold an address in a block's first 64 bytes.

### The quarantine (M4)

A repair lets go memory the game forgot; if the game, or anything else, still uses it, the heap gives the block to another allocation and the two write over each other, and the game fails much later somewhere else, or not at all while its data goes wrong. A run that does not crash says little: the sets of modifiers' first repair corrupted the heap at the sixteenth of twenty rounds.

With the census built in, `memory.quarantine` turns the quarantine on and off at run time, off by default, as `memory.watch` does watching: a separate feature would be on in every developer build (`dev` builds them all in), and the quarantine keeps memory a leak run measures. While it is on:

- every block freed on a thread where a repair is letting memory go is kept instead of freed: the census's `free` hook gives `free` null instead (an observer's "before" may change the arguments). A repair marks where it lets memory go (`LettingGo` in `core/src/repairs/mod.rs`, around every deletion of what waited for a return or a world reset in `core/src/repairs/waiting.rs` too, or from a destructor's start to its end where the game deletes what the repair handed over, as for the text boxes' animations), so the blocks that freeing frees in turn are kept too. What the game frees later, a window a repair killed and the GUI's sweep frees, is not;
- each block kept is filled with `0xdd`. A read of it then gives those bytes, and a call through them faults at once, in the code that read it, with a crash dump that names it;
- every two seconds, on a thread of its own, every block kept is compared with the pattern, and the first write to each is kept: the repair, the block's site and size, the first byte changed and the bytes from it, and the generations it was kept in and found in;
- past 512 MB the oldest blocks are freed again, and turning it off frees all of them.

A leak run with `--leak.quarantine true` turns it on from the start and reports, by repair, what it kept and every block written to; a run the game does not finish still reports what was found until then, and in which round it stopped. What the quarantine keeps counts in the game's heap, so such a run measures no growth: it is for trying a repair, not for measuring it.

### What keeps the free pages committed (M5)

About half of what a round adds to the private bytes is free pages the game's heap keeps committed: the framework has the heaps give back the pages no block is on after every world reset, and a page any block is on stays, however little of it the block uses. Which blocks keep the most is not which leave the most bytes: a leak of a few small blocks spread over many pages keeps more than a large one.

`memory.pages`, with the census built in, walks the game's heap page by page (`HeapWalk` under the heap's lock, which lists the low-fragmentation heap's small blocks one by one and not its free ones): each committed page, the bytes the busy blocks on it use, and so the pages no block is on and the free bytes of the others. A page's free bytes fall to the blocks on it alike, and through the census to their sites; the blocks the census does not count, the framework's own and the census's among them, are "not counted". It answers the totals, the pages in use by how full they are, the free bytes the leftovers keep, and the sites that keep the most.

The walk holds the heap, and so the game, for seconds, and must not allocate from it: what it fills is allocated before. A leak run asks after its last round, with the census. What the census holds of its own is in the same heap and keeps pages too, so the figures are those of a game with the census; which sites keep the most is what they are for.

### What the census does not count (M6)

The memory account's game heap is every block of the process's default heap but the framework's; the census counts only those the game's C runtime allocated after the framework started. What grows in the heap and not in the census is other modules': Direct3D's, the graphics driver's, the libraries the game is linked with, and what Windows allocates for them.

`memory.uncounted`, with the census built in, walks the heap as `memory.pages` does and keeps the busy blocks the census has no record of, leaving out the walk's own buffers. Each ask marks them by start and size; the next gives, size by size, how many more there are than at the mark and how many were made since, the sizes that grew the most first, with samples of the blocks made since: their first 64 bytes, and the module and offset of each word that points into a loaded module. A block's first words are often a vtable or its owner, which say whose it is.

`leak` marks after its second round, or its first when it plays one, and compares after its last; its sections give what the census does not count at both, against the game's heap, and the sizes that grew.
## How it fits together

- `remote` declares the `memory` group behind its `dev-memory` feature, which the core's `dev-memory` turns on, and `dev-heap-census` with it. A workspace build of the developer build gives the command line tool the same group.
- `core/src/memory_account.rs` answers `memory` from Windows' figures, and hands the group to the census when it is built in.
- `core/src/census.rs` keeps the blocks and answers the group. Building the answer from the census's figures (the leftovers by generation, the trend, the average) is a function of its own, tested without a game. Its modules, each tested without a game: `pointers` (the rule for a pointer), `holders` (M2's scan) and `watch` (M3's owners).
- `leak` and `growth` (`xtask/src/scenario/leak.rs`) use only remote control, as every scenario does.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and the developer build;
- what it delivers was tried in the game, with the run recorded in a file of its own in [prototype-log/](../prototype-log/).

### M1 The census answers through remote control

The `memory` group; generations that remote control ends; sites counted above the generic allocation; where each site's blocks are let go; the leftovers with their function, trend and average; `tools/leak-run.ps1` and its report (since replaced by the scenarios `leak` and `growth`).

### M1b The memory account

The `dev-memory` feature, `memory` without the census, and the leak run's split of each round's growth.

### M2 Who still points at a leftover

`holders` in `memory.leftovers`, the scan, and the report's table of what holds each leftover site.

### M3 Who let a leftover's owner go

`memory.watch`; the owners let go in `memory.leftovers` with `holders`; the leak run's `-Watch` (now `--leak.watch`); the rule for a pointer shared with M2.

### M4 The quarantine

`memory.quarantine`, the repairs saying where they let memory go, and the leak run's `--quarantine` (now `--leak.quarantine`).

### M5 What keeps the free pages committed

`memory.pages`, the walk of the game's heap and what its pages' free bytes fall to, and the leak run's table of it.

### M6 What the census does not count

`memory.uncounted`, the blocks of the game's heap no site has, against the last ask, and the `leak` scenario's section on them.
