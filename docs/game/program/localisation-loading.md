# Localisation loaded twice at startup, and the quadratic scans inside it

## The two localisation loads in `main`

In `0x8757e0` (`main.cpp`):

- At `0x8779b0`, `main` calls `0x873fc0(&loc_options, ..., &need_language_idler, load_all_lang)`:
  - `0x873fc0` checks `settings+0x48` (`language` from `settings.txt`, `"l_english"`). When it is non-empty, `need_language_idler` is cleared, and `0x873fc0` calls `0x1564470`.
  - `0x1564470` checks the `-export_loc`, `-import_loc`, `-tag_loc`, `-purge_loc` and `-verify_loc` flags in `loc_options` (`+0x40`..`+0x44`, filled from the command line by `0x1563f20` at `0x876b7d`). In a normal start all five are 0, so `0x1564470` calls `0x14b9370(language, ..., load_all_lang)` and returns.
  - `0x14b9370` calls `0x14b3bf0("localisation/languages.yml", 1)`:
    - `0x14b3bf0` clears the recorded folder list `[0x2358778]..[0x2358780]`, destroys `[0x2357b20]` if any (`0x16fb7c0`), allocates a fresh `[0x2357b20]` with an 8 MB text arena at `+0x20` (`0x12e4470`), records `"localisation/languages.yml"` in `[0x1fd8428]` and `[0x1fd179e]`, and reads `"localisation/languages.yml"` (`0x14b3940`).
  - `0x14b9370` then reads the known languages (`0x14b9190`), sets the active language in `[0x2357b20]` (`0x16fb890`, creating the language's `0x68`-byte collection in `[0x2357b20]+8`), and calls `0x14b4320("localisation", load_all_lang)`:
    - `0x14b4320` loads every `.yml` in `"localisation"` (`0x14b3db0` with `replace = 0`), then appends `"/replace"` and loads every `.yml` in `"localisation/replace"` (`0x14b3db0` with `replace = 1`), runs the collision/duplicate check `0x16fd7a0()`, and pushes `("localisation", load_all_lang)` onto the 0x28-byte folder list `[0x2358778]..[0x2358780]`.
- From `0x8779b5` to `0x877a74`, `main` reads two strings from `settings` (`+0x1b0` via `0x86fb20` and `+0x1d0` via `0x86fb70`) and, when non-empty, passes them to `0x1711d40()` (`[0x2456bc0]`, initialized with `"notepad"` by `0x17115f0`) through `0x1712200` and `0x1712170`. Nothing there reads or writes `[0x2357b20]`.
- At `0x877a79`, `main` unconditionally calls `0x14b4a20()`, the localisation reloader (whose only other callers are the console command `reload_loc` at `0xe5a03f`, the `-reload` handler for `"text"` / `"localize"` at `0x14e1542`, and `-update_loc` at `0x14b7331`):
  - `0x14b4a20` reads the current language's name from `[[0x2357b20]]` and calls `0x14b4740(&language)`.
  - `0x14b4740` copies the folder list `[0x2358778]..[0x2358780]` (`[("localisation", 0)]`), calls `0x14b3bf0([0x1fd8428], [0x1fd179e])` — which calls `0x16fb7c0()`, **freeing all collections and the text arena of `[0x2357b20]`** and reading `"localisation/languages.yml"` again — re-selects `language` (`0x16fb890`), and calls `0x14b4320("localisation", 0)` again.
  - `0x14b4a20` then walks `[0x2358790]..[0x2358798]`, a vector of reload callbacks. A search of the entire `eu4.exe` image (`refs:0x2358790`, `refs:0x2358798`) finds **no instruction anywhere in the binary that writes to `[0x2358790]` or `[0x2358798]`**: the vector is always empty.

Timed with `dev-call-trace` on Guixu (`0x873fc0`, `0x14b4a20`, `0x14b4320`):

- First load (`0x873fc0`): 1.87 to 7.98 s (**6.11 s**), of which `0x16fd7a0` is 0.67 s.
- Second load (`0x14b4a20`): 7.98 to 13.65 s (**5.68 s**), of which `0x16fd7a0` is 0.64 s.

The first load's entire `[0x2357b20]` is thrown away by `0x16fb7c0` at the start of `0x14b4a20` (save for any error lines written to `error.log`, which the second load writes again, as lines 1–64 and 65–128 of Guixu's `error.log` show verbatim).

Also noted in `error.log` line 60: `"Failed to find pre specified enumerate for 'localisation/replace'(.yml, (null), -1)"`. At `0x873508`, `0x872bd0` pre-registers `"localisation/replace"` with `recursive = 0`, whereas `0x14b3db0` (`0x14b3e38`) asks `0x170ffe0` for `recursive = -1`, missing the pre-registered listing and enumerating on the main thread each pass.

## Inside a localisation collection: registration, replacement and duplicate checking

Each language in `[0x2357b20]+8` is a `0x68`-byte collection (`0x16fb590`):

| Offset | Type | Contents |
| --- | --- | --- |
| `+0x00` | `std::string` | Language key (such as `"l_english"`) |
| `+0x20` | `vector<Entry16>` | Lookup index, 16 bytes each: `+0` u64 FNV-1a 32-bit hash (zero-extended from `ecx`), `+8` u32 value offset in `[0x2357b20]+0x20` (`key_offset + strlen(key) + 1`), `+0xc` padding |
| `+0x38` | `vector<Record24>` | Provenance records, 24 bytes each: `+0` u64 hash, `+8` u32 key offset, `+0xc` u32 value offset, `+0x10` u32 file index in `+0x50`, `+0x14` u32 line number |
| `+0x50` | `vector<std::string>` | File paths read into this collection (`0x16faec0` finds or appends each file's path) |

The global `[0x2357b20]` holds the active collection pointer at `+0`, the `vector<Collection*>` at `+8`, the shared NUL-terminated text arena `vector<char>` at `+0x20` (8 MB reserved initially), and the UTF-8-to-1252 flag at `+0x38`.

How files are loaded into the collection:

- **Non-replace pass (`replace = 0`, `"localisation"`):** `0x14b3db0` lists all `.yml` files recursively (`0x170ffe0`). Because the independent per-file sort flag (`[rbp+0x2c]` / `+5` in `0x16fd600`) is 0, `0x16fd600` does *not* sort `+0x20` before each file, and `0x16fa8d0` (`cmp byte ptr [rsp+0xc8], 0; je 0x16fa949`) appends every `(key, value)` to the arena `+0x20`, appends to `collection+0x20` and appends to `collection+0x38` in $O(1)$ amortized time. Only when all files of `"localisation"` have been read does `0x14b3db0` (`0x14b4150` → `0x17006e0`) merge-sort `collection+0x20` by `(hash, strcmp(value))`; `0x1700560` / `0x16ffd10` compares `[entry+0]` (the hash) first, and when hashes are equal compares the strings at `[entry+8]` in the arena.
- **Replace pass (`replace = 1`, `"localisation/replace"`):**
  1. `0x16fd600` tests the sort byte at `+5` (`0x16fd632`); `replace` is the separate byte at `+4`. The guarded sort helper is `0x16fb470`, but this path is not taken for the normal replace pass. The sort byte is cleared by `0x14b3a9f`, so replace files do not re-sort the collection. The lookup vector remains sorted from the end of the ordinary pass at `0x14b4204`.
  2. For every entry in a replace file, `0x16fa8d0` hashes the key (`0x811c9dc5`, prime `0x1000193`, signed `movsx eax, al`) and calls `0x16fb030(collection, hash)` (binary search on `collection+0x20` by hash).
  3. When the hash is already in `collection+0x20` (the normal case for `localisation/replace`), `0x16fa8d0` jumps to `0x16faae0`:
     - `0x16faae0` binary-searches `collection+0x20` again (`0x16ffd10`), updates `[rax+8]` to the new value's arena offset, and then **walks the entire `collection+0x38` vector of ~280,000 records from index 0 (`0x16fabf0`..`0x16fac06`)** looking for an entry with `[r10 + rdx*24] == hash` and `[r10 + rdx*24 + 0x10] == file_index`. Finding none (since this replace file has not defined that key yet), it appends a new 24-byte record to `collection+0x38` (`0x16fac60`) and copies `key` and `value` into the arena.
     - With ~280,000 records in `collection+0x38` and tens of thousands of replaced entries across Guixu's 13.5 MB of `localisation/replace/*.yml`, those linear scans touch billions of 24-byte records — explaining why `0x16faae0` alone is **21.8% of all main-thread samples** across the entire startup.
- **Collision and duplicate scan (`0x16fd7a0` → `0x16fb1c0`):**
  - After both passes, `collection+0x20` is sorted. `0x16fb1c0` walks `collection+0x20` for runs of equal hashes (`cmp r14, [r8+rcx*8]` at `0x16fb233`: duplicate keys from multiple files in `localisation`, or true 32-bit FNV-1a collisions).
  - For **every** run of equal hashes in `collection+0x20`, `0x16fb1c0` walks the **entire `collection+0x38` vector from 0 to N** (`0x16fb380`..`0x16fb3e8`) to collect the key, value and file pointers of matching records, then calls `0x14b36c0` to format and write `"Duplicate localization found"` (at log level 5, filtered out unless verbose logging is on, after formatting the stream!). And `0x14b4320` checks the return value of `0x16fd7a0` (whether any duplicate/collision run existed) to log `"Localization hash collision found, more info in text.log!"`.


  - `[0x242be60]` (`0x90` bytes, created by `0xb9b380`) holds two separate hash tables: `+0x08` (count at `+0x18`), a runtime cycle accumulator that `0xb9bea0` writes to `trigger_profile.txt` at shutdown only when `[+0x18] != 0` (no code in the release `eu4.exe` ever inserts into `+0x08`, so `0xb9bea0` is always a 2-instruction no-op); and `+0x48` (head sentinel at `+0x50`, count at `+0x58`), the profile table populated once at the start of `0x208620` by `0xb9b5a0` (`0xbbc0e0`) from `trigger_profile.txt` (6,593 entries in vanilla, 0.026 s).
  - While databases and events load, every compound trigger (`0xb89710`) registers itself in `[0x2340010]` (`+8` for top-level triggers when TLS depth `[0x1fd5c00] <= 0`, `+0x48` for nested compound triggers).
  - At the end of `InitGame` (`0x205baa`..`0x205ca0`): `0xb89690` runs post-load resolution (vtable `+0x68`, such as resolving country tag strings into `CCountryTag` IDs) on `[0x2340010]+8` (0.34 s on Guixu, 1.12 s on 2000 AD); then `0x205bed`..`0x205c51` calls `0xb9b4e0([0x242be60], trigger)` on every trigger in `+8` and `+0x48` to store its profile cost at `[trigger+0x58]`; then `0x205c53`..`0x205c92` calls `0xb980a0(trigger)` to sort `[trigger+8]` (its direct children) by `[child+0x58]` whenever every direct child has a non-zero cost (`0xb980f3`); finally `0xb89560` frees `[0x2340010]` (0.25 s on Guixu, 0.80 s on 2000 AD).

## Reader coverage and startup flags

- **All readers of `collection+0x38`..`+0x50`**: checked across `0x16fa000..0x1702000` and `0x14b3000..0x14ba500`. Normal runtime lookups (`0x16fd880`, `0x16fd8c0`, `0x14b34a0` → `0x16fb030`) read only `collection+0x20` and `[0x2357b20]+0x20`. `collection+0x38` and `+0x50` are read by `0x16faae0` (replacement), `0x16fd7a0` → `0x16fb1c0` (duplicate check), the developer CLI switches `-export_loc` (`0x14b6670`), `-import_loc` (`0x14b5b10`), `-purge_loc` (`0x14b7690`) and `-verify_loc` (`0x14b8b60`) (all in `0x1564470`, which `exit(0)` immediately after), and the console command `verify_loc` (`0xe5a5c0` → `0x16fd910`). Keeping `collection+0x38` and `+0x50` populated identically through an index preserves every one of them.
- **When `text.log` (category 5) is registered**: in `0x871130` (`main`'s log setup at `0x877979`), `text.log` is registered in `[0x2349598]` for category `5` (`0x872863` → `0x15630c0`) **only** inside `cmp byte ptr [rbp+0x98], 0; je 0x872889` (`0x87283b`), i.e. only when `-debug` is on the command line. Without `-debug`, `0x14b36c0` formats every duplicate into temporary strings that `0x15632d0` passes to `OutputDebugStringA` (`0x1563dac`) as `"UNKNOWN CATEGORY: 5 ..."` and frees; `text.log` stays 0 bytes, while `0x14b4320` uses only `0x16fd7a0`'s boolean return value (whether any adjacent pair in sorted `collection+0x20` has equal hashes) to log `"Localization hash collision found..."` to `error.log`.
- **Note on `0x16fd600`'s per-file sort**: at `0x16fd632`, `0x16fd600` checks `cmp byte ptr [rbx+5], 0` (`[rbp+0x2c]` in `0x14b3940`, which is cleared to `0` at `0x14b3a9f`), whereas `replace` is at `[rbx+4]` (`[rbp+0x2b]`). So `0x16fd600` never actually sorted before each `replace` file; `collection+0x20` remained sorted from the end of `localisation` (`0x14b4204`), and **all** of `0x16faae0`'s 21.8% main-thread time came from its linear scan over `collection+0x38` (`0x16fabf0`).
- **First-ever launch with no `language` in `settings.txt`**: when `settings+0x48` is empty, `0x873fc0` sets `need_language_idler = 1` and still loads `languages.yml` and `l_english` `"localisation"` (`0x14b9370` falls back to the first language in `languages.yml`). At `0x877a79`, `[0x2357b20]`, `[0x2358778]`, `[0x1fd8428]` and `[0x1fd179e]` already hold the exact state `0x14b4a20` rebuilds; later `0x2003d0` shows the language selection idler and reloads localisation for the chosen language via `0x200af0(app, 1)`. Skipping `0x14b4a20` at `0x877a79` alone (leaving `0x14b4a20` intact for `reloadloc`, `-reload` and `-update_loc`) also stops `core/src/loc.rs` from splicing `localisation/text_l_english.yml` twice at startup.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0xb8b6e0` | function | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xb9c160` | function | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xb9c9a0` | function | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xbbd0b0` | function | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xbbdfe0` | function | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xdd4939` | code | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xdd49ac` | code | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xdd49c7` | code | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |
| `0xdd49e2` | code | Referenced while examining other startup stretches measured on guixu and 2000 ad; complete role not established |


## Trigger-profile cost construction

`0xb9b4e0` is purely read-only on `[0x242be60]+0x48` (`0xbbd0b0`, `unordered_map::find`), yet for every trigger it calls `0xb9c160` recursively to build a temporary `CTriggerProfileNode` tree — allocating a `0x48`-byte `std::unordered_set` (`0xbbdf20`) on the heap for every node in the trigger's subtree, recomputing subtree sums (`0xb9c9a0`) at every level, and immediately deleting the temporary tree (`0xbbdfe0`).
