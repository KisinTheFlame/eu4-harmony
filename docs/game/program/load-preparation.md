# Preparing the world and its inputs during an in-game load

Native EU4 1.37.5, read from the installed executable on 2026-11-16 after `xtask what` lookups. This page records established ordering, ownership and concrete shared writes. [Save reading](saves.md) and [the main menu](../interface/main-menu.md) retain the complete load path and reset behavior.

## Input preparation before the reset

In `0x5d0580`, `0x5d09a3` opens the local source stream. The reset call is at `0x5d0a2f`; `0x5d0a29` sets its second argument. The source remains alive until `0x5d0f58` on successful completion. The save-description constructor at `0x5d0b9f` and header validation at `0x5d0bb2` follow the reset, before the world's load at `0x5d0c78`.

Description/header processing and the world's load use the same source stream and cursor in sequence. The existing memory-stream substitution uses the original stream's state after header validation. Reset may write an ironman exit save, including a save with the chosen input's name, so source identity alone does not establish that its contents remain unchanged throughout reset. Cloud and ironman source lifetimes and version behavior have not been fully established here.

`0x5cd120(source, output, entry name)` takes the mutex at `0x23587a8`, uses a shared scratch buffer from `[0x242bf50]+0x338`, and seeks the common source stream. The scratch capacity is 90 MiB. `0x153bce0` performs entry reading, decompression, inline CRC32 and output copying. ZIP-entry extraction therefore shares both scratch storage and the source cursor.

The checksum wrapper `0x97500` also triggers lazy gamestate extraction and changes achievements state. `0x14e36b0` scans for checksum text; `0x14e3390` computes the digest using the caller-selected salt.

## Country setup contains map and graphics work

`0x207310(application)` includes more than country files, in this order:

| Call site | Callee | Work |
| --- | --- | --- |
| `0x2074ce` | `0x281410` | Country-tag source files and shared tag assignment |
| `0x207752` | `0x2830a0` | Allocate, construct, publish and size countries |
| `0x20798e` | `0x283d20` | Apply country definitions and validate cross-country inheritance |
| `0x207a2e` | `0x2855b0` | Country colors |
| `0x207c7f` | virtual `+0x40`, `0x87d140` | Map/runtime-province reset and geometry |
| `0x207c90` | `0x1b7bf0` | Allocate and append per-country records |
| `0x207f1e` | `0xb65d30` | Trade-node definitions |
| `0x208201` | `0x284bf0` | Revolt triggers |
| `0x208434` | `0x10b2b70` | Flags and atlas resources, with second argument 1 |
| `0x208447` | `0x453610` | Register additional named textures |

The complete tag handler `0x281e60` assigns the next numeric index by changing database `+0x100` at `0x28217e`, publishes into shared vectors/buckets, and changes the special rebel/pirate/native tag globals. Existing-tag lookup and duplicate handling precede assignment. File ordering therefore determines indices and duplicate precedence.

`0x2830a0` publishes one contiguous country block at database `+0x108`, appends each pointer to its shared vector, then calls the country's clear. `0x283d20` consumes stored source text, applies the native reader to each country, and afterwards resolves inherited names/colors across countries. The complete shared-state effects of constructors, clears and table-sizing callees have not been established.

## Map pixels and retained province shapes

The complete reset `0x87d140` destroys runtime provinces through `0x88b4b0`, constructs their replacements with `0x9d5a20`, and publishes their pointers in static map entries. It then rebuilds adjacency and geographic groups, calls `0x888e60` at `0x87d298`, sums shape spans through `0xa40450`, stores area at entry `+0x1b4`, and tail-calls `0x757260`. The latter reaches `0x77cfb0`, which consumes that area and presence bits to build island data. These consumers depend on the preceding geometry writes. The runtime constructor contains first-use writes to global managers and virtual calls.

Static entries are at `map+0x6c0`, stride `0x230`; runtime provinces are separate objects, stride `0x2e10`. Do not confuse their identically numbered fields. `0x888e60` reads the i16 province-ID grid at `map+0x10` and dimensions at `+0x28/+0x2c`. It updates the second grid at `+0x6d8`, each static entry's shape at `+0x130`, bounding box at `+0x154/+0x158/+0x15c/+0x160`, and presence bit `+0x229` bit `0x80`. Invalid/negative source pixels follow diagnostics and do not indiscriminately overwrite the destination cell.

A shape holds begin/end/capacity and a last-span pointer at relative `+0/+8/+0x10/+0x18`. Its spans are six-byte `(x, y, length)` records. `0xa402a0`, called only at `0x889088` and `0x88960a` (with vector growth at `0xa445c0`), appends/extends spans or finds an existing span containing a point. Its existing-point branch at `0xa40368` does not restore the last-span pointer, so on every world reset after startup all 11.5 million existing pixels lose the cache and search each province's span vector linearly. Harmony's `province-shapes` speed-up replaces `0xa402a0` so existing-point hits restore `last_span`, covered prefixes skip in O(1), and newly created or already-cached right-extending spans extend across contiguous horizontal runs in one step while producing the exact same span vector.

The complete immediate reset, constructor/destructor and geometry helpers were read; not every virtual/transitive path was established. Universal retention of shapes throughout reset has not been established.

## Flag cache verification is ordered input work

Reset passes 1 to `0x10b2b70`, forwarded as the fifth argument of `0x10c7280`. That argument skips the two loading-screen draws at `0x10c7a72` and `0x10c7b21`. The atlas still creates/uses graphics resources and registers textures; skipping the loading screen does not make it CPU-only.

`0x10c5ff0` validates metadata, flag names, expected atlas pages and the source digest. Its result, called at `0x10c7a23`, chooses rebuilding versus cached-page loading. In its checksum work:

- `0x10c6b48` constructs a local `FlagFiles` checksum. Each ordered path goes through `0x171d600`, and existing sources through `0x171d650`.
- `0x171d650` opens its own input file, calls `0x171d750`, then closes/deletes the input. It is a **read**, not an atlas-cache write.
- `0x171d750` calls `0x171e330`, then feeds the file path to the same MD5 state at checksum `+0x80` through `0x1723520`.
- `0x171e330` reads 128 KiB chunks, normalizes CRLF to LF, preserves isolated CR and a final pending CR, and feeds normalized bytes to MD5. Pending CR crosses chunks but resets between files. This rule also applies to image-file bytes.
- Finalization at `0x10c6f73` and comparison at `0x10c6f86` occur before the atlas branch.

The digest is one ordered stream across file contents and paths. Independent per-file MD5 digests are not equivalent to it, and unchanged file count alone does not establish unchanged digest input.

On rebuilding, the image path around `0x10c8f50` decodes from shared scratch `[ [0x242bf50]+0x330 ]` and uploads at `0x10c8f9c`. Cached pages around `0x10c9ff2` reuse that scratch and decode/upload at `0x10ca038`/`0x10ca084`, after `0x10c9f8f` checks whether the page texture slot is null or `0x16c10a0` requests a reload. Inside `0x10c7280`, even on a cache hit (`0x10c5ff0` returning 0), the loop still reads `flagfilescolor.txt` and initializes each newly constructed country object's three flag colors at `country+0x24d8` via `0x453850`, and `0x10b2b70` re-registers `flagmap0..N` in the texture manager via `0x16c3890`. Harmony's `flag-atlas` speed-up caches `0x10c5ff0` when the directory, flag-dir mode (`[0x233fe28]+0x40`/`+0xa8`), page capacity (`flagmap+0x58`), and ordered flag-name vector match the last valid run, stashes the live D3D9 page texture handles right before `0x10c5ea0` on world resets (`is_reset != 0`), lets `0x10c7280` populate country colors and UV grids normally, re-attaches the stashed handles at `0x10c9f8f` to avoid decoding and re-uploading `flagfiles_N.tga`, releases any unused stashed handles through `0x1fd1330` after `0x10c7280`, and invalidates the stash on any D3D device reset (`0x14ddc70`). Renderer state and manager registration are also shared. `0x453610` calls `0x16c35b0`, whose global critical section at `0x242b818` covers cache lookup, loading and publication. The complete atlas's indirect graph is not claimed audited; selected decode/checksum branches and the complete named wrappers establish these constraints.

## Shared registration during save-object reads

`0x861d20` reads a leader, checks its ID and registers it through a virtual method. In `0x14cf190`, the lock around `0x14cf411` through `0x14cf445` protects numeric-ID allocation, not the subsequent global container insertion at `0x14cf4ab`, table registration at `0x14cf4e1` or count change at `0x14cf50f`. Object readers therefore share registration state beyond the ID-allocation lock. Separately, dynamic token creation `0x170d410` changes shared token storage without the complete lookup/grow lock used elsewhere. This does not mean every text-token read creates a token.
