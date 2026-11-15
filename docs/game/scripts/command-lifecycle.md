# Commands and multiplayer


**Commands.**

- **The player's interface and the AI change the game through commands**, with the few exceptions below.
- The base vtable is `0x1c3a710`, 21 slots:
  - `+0x08`–`+0x20` write and read, as a persistent object;
  - `+0x40` the check;
  - `+0x48` execute;
  - `+0x50` the type token;
  - `+0x60` clone;
  - `+0x68` and `+0x70` before and after executing;
  - three flags: a lobby or system command; allowed during a hot join; may be posted outside the frame.
- Fields: `+0x38` the sender, `+0x44` the type, `+0x48` a stamp, `+0x4c` the ID, the command's own data from `+0x50`.
- There are **339 command classes**. `0x249120` registers 337 prototypes and `0x7422d0` one more, through `0x14c6930` into `0x23587b0`. The factory `0x14c6500(type)` clones a prototype.
- Examples:
  - `daily_tick` (`0x1cea280`);
  - `buildunit` (`0x1c4dff8`, [provinces.md](../provinces/provinces.md));
  - `diplomaticactioncommand` (`0x1c814b0`; its execute `0x4e7c50` is the framework's "diplomatic action carried out");
  - `move_order` (`0x1d1d3f8`);
  - `select_event_option`.

**The life of a command:**

1. The interface, the AI (`0xbfe50`, 184 call sites) or the simulation itself makes the command.
2. It is posted with `0x14ca2d0` or `0x14ca210`, stamped, and handed to the session's handler.
3. Even in single player, the local handler (`0x1580370`) writes the command out and makes the command that runs from that copy, as a network command would be. So the check runs on "a command object of its own" ([recruiting.md](../military/recruiting.md)).
4. In the next in-game frame, `CSession::Update` `0x14ca360` executes the queued commands (`0x14ca870`, the main thread only): it asks the check again and drops a command that fails it ("Discarding invalid command!"), then calls before, execute and after.

Commands' written form, the factory and the one place every executed command passes: [commands.md](commands.md).

**What is not a command.** The game console's handlers change the state directly; the console is refused in multiplayer and ironman ([game-console.md](game-console.md)), and the checksum records whether it was used. In single player the pause key toggles the pause directly ([time.md](../day/time.md)).

**Session kinds** (`server.cpp`), by the session's configuration (`0x14c83e0`):

| Kind | Handler | Vtable | Notes |
| --- | --- | --- | --- |
| 0 client | `0x157b5b0` | `0x1d96268` | |
| 1 host | `0x15764d0` | `0x1d96088` | Sends commands in batches per tick |
| 2 local (single player) | `0x157ffe0` | `0x1d96178` | No network |

The transport is `pdx_net`, over Steam or Nakama.

**Lockstep and out-of-sync checks.**

- Every machine executes the same commands in the same order.
- The host posts the days and waits for the slowest player; `client_ping` can pause the game ("LAG_PAUSE").
- After a `daily_tick` executes, the host fills a checksum into it (`+0x70` → `0x743aa0`, `checksums.h`). It is a record of values, not a hash of memory: the random seed and draw count, whether the console was used, and many values per province, country and unit.
- Each client compares its own state with it (`0x742420`, `oos_handler.cpp`).
- `setrandom` reseeds the generator and advances it by a count, so the seed and the count reproduce its state; both are saved.
- A hot join sends a save and holds the commands back until the new player has it.
