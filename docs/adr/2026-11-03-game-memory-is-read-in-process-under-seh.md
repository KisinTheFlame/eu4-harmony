# Game memory is read inside the process, under SEH

The framework reads the game's memory by copying it inside the process, in the C++ SEH boundary that already guards its calls into game code (`eu4h_read` in `core/native.cpp`). An access violation, a guard page or a failed page-in during the copy becomes an error the capability answers with, as before, instead of a crash. Writes stay with `WriteProcessMemory`, which also deals with a page's protection; they are few, at startup.

Where the game has a function that answers a question, the framework calls it instead of working the answer out from the game's fields: whether an army may enter a province or attack another, whether a strait is open, the game's route, and finding an object by its ID. What the game itself reads straight from a field, such as a province's owner, the framework reads too, each field kept by a dependency point.

## Considered Options

- **`ReadProcessMemory` on our own process** (what the first version did): the same fault tolerance, but each read is a system call, about 325 ns for 8 bytes against well under 1 ns for a plain read (measured 2026-11-03, Ryzen 7 9700X). A capability that reads a few thousand fields, such as listing every province, took 13 ms in the game, and the cost of a capability came down to how many fields it read.
- **Plain reads with no guard**: as fast, but a stale or wrong pointer, which a different game build or a game state not foreseen can give, ends the game instead of failing one answer.
- **Checking each address first** (`VirtualQuery`, cached by region): a system call again for each region, and a page can still go between the check and the read.

## Consequences

- A capability's cost is its own work and the game functions it calls, not the number of fields it reads; there is no reason left to shape a capability around saving reads (ADR mods-hold-the-logic).
- A read that meets a guard page turns it into an ordinary page, as any first touch would; the framework reads game objects, which are not kept on guard pages.
- Reads on another thread than the main one are guarded the same way; they are no more consistent than before, and the capability layer still reads only at safe points.
