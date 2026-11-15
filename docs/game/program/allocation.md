# The world in a second process (W2, W3)

## Where the game's memory comes from

- **One heap for everything.** The game's `operator new` (`0x1a332d4`) calls the C runtime's `malloc` (`0x1a4963c` → `0x1a6837c`), which is `HeapAlloc([0x233d850], 0, size)` (`0x1a683ba`); `0x233d850` is set from `GetProcessHeap()` at `0x1a72e98`. The C runtime is linked into `eu4.exe`: the executable imports no `ucrtbase` or `vcruntime`. Its `free` (`0x1a6833c`), `calloc` (`0x1a6989c`), `realloc` (`0x1a6dd98`) and `_msize` (`0x1a706f4`) use the same handle; nothing else in `eu4.exe` calls `HeapAlloc`, `HeapReAlloc` or `HeapCreate`. So the world, the databases loaded at startup, the interface's windows and the graphics' wrappers are all blocks of the process's default heap, made by the same function on the same thread.
- **The others in the same heap.** `tbb.dll` allocates through `ucrtbase` (it imports `api-ms-win-crt-heap-l1-1-0`), which is the same process heap by another path: the game calls TBB's allocator at 31 places (`?allocate_via_handler_v3`, from `0x7dbd5` on) and its cache-aligned one at 7 (`?NFS_Allocate`). Direct3D's runtime and the driver keep most of theirs outside the heaps. The memory account listed six heaps at the main menu and after the 1780.7.1 save was loaded, the process's holding all but some 100 KB; the framework's own allocations are blocks of the process heap too.
- **The large blocks** are the heap's own. The game's only `VirtualAlloc` (`0x171f571`, in `0x171f530`) serves 24 objects of 0x48 bytes (`0x212cd0` and its neighbors, each in a page of its own that `VirtualProtect` makes read-only after it is filled, `0x212c18`), not the world's arrays. The two blocks of about 55 MB that M1 found outside the heap's regions, the countries' and the provinces' arrays, are therefore `HeapAlloc`'s large blocks, which the heap takes from `NtAllocateVirtualMemory` itself (_inferred_ from the absence of another allocator; the heap's list of its large blocks was not walked).
- **How much.** At the main menu, the game's heap held 1.80 GB (2.04 GB committed). With the 1780.7.1 save loaded: 2.08 GB (2.36 GB committed) of a process's 4.35 GB of private bytes, and 1.99 GB outside the heaps. So a late world adds some 0.3 GB to a heap already holding 1.8 GB of what the startup loaded, which the world's objects point into (their types, cultures, religions and the rest).


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x158097a` | code | a posted command read back |


## Limits of the reading

The heap was not walked to prove that the observed 55 MB arrays were large heap blocks. World pointers were not proved to remain inside that heap; TBB, aligned allocation and Windows allocation coverage were incomplete. No second process was started against a live world, and image-base observations covered two games, one boot and one machine.
