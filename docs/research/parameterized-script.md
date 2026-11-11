# Parameterized scripted triggers and effects, compiled for each use

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-11 for a candidate speed-up of [loading.md](../plans/loading.md), not built: a scripted trigger or effect written with parameters (`$name$`, and blocks `[[name] …]` kept or left out by whether an argument is given) is made into text again for each place the script uses it, and that text is made by cutting and joining the whole text once for every parameter and every block. Read from the code with `cargo xtask disasm` and a script that maps direct calls to their functions through `.pdata`; timed in a start the same day ([the run](../prototype-log/2026-11-11-five-candidates-timed.md)). It adds to [architecture.md](architecture.md), "Scripted triggers and effects", which inferred the compiling from its log lines.

In short: **`0xdb0860(definition, arguments, out)`** (`metascript.cpp`) makes one use's text: it checks the arguments (`0xdb0990`), copies the definition's text, resolves its blocks (`0xdb1410`), finds its parameters again when it had blocks (`0xdb0d60`), and puts each argument's value in place of its parameter (`0xdb1800`). **Each block and each parameter rebuilds the whole text**, from the last to the first, with about five copies of it each time, so one use costs about five times (blocks + parameters) × the text's length in bytes copied, besides the allocations: it grows with the square of a definition's size when its parameters are spread through it. The base game has 355 definitions with parameters (235 effects, 120 triggers) and 8,676 uses of them in its own files, a sum of about 79 million over the uses of (parameters × the definition's length); each use is made at least twice, when it is read and when it is validated at the end of loading. **Timed in a start with the player's playset, 320,905 uses were made, 1.42 s of the LoadingThread, 1.21 s of it the parameters' substitution**, far more uses than the files' 8,676 ("Timed in the game"). Mods with large parameterized effects make far more. A single pass over the text gives the same text, since the blocks and the parameters are found from left to right and do not overlap.

## The definition

A scripted trigger's or effect's definition with parameters is read by `0xdb04d0(definition, reader)` (from the databases' reader at `0xacc344`; `scripted_triggers` and `scripted_effects` are `0xace660` and `0xacb950`, [loading.md](loading.md)). It keeps the definition's text, the tokens joined, at `+0x38` (`std::string`), and two lists of the game's kind (data at `+8`, count at `+0x14`, i32):

- **The parameters**, at `+0x58` (data `+0x60`, count `+0x6c`), entries of 0x28 bytes: `+0` the name (`std::string`), `+0x20` where its first `$` is, `+0x24` one past its second. `0xdb0d60(definition, text, list, from, shifted)` finds a `$` from `from` (`memchr`, `0x1a35fa0`), the next `$` after it, adds the name between them (`0xdb1240`; with `shifted`, both places moved by the definition's `+0x48`), and calls itself from one past the second `$`: so the list is in order, the entries apart, and the recursion as deep as the parameters are many. A `$` without a second one was not followed: the next call would start from 0 again.
- **The blocks**, at `+0x70` (data `+0x78`, count `+0x84`), entries of 0x50 bytes: `+0` the argument's name, `+0x20` the block's content, `+0x40` where its `[` is, `+0x44` one past its end, `+0x48` (u8) negated. `0xdb0e90(definition, text, from)` finds a `[` from `from`, the next `]` after it, takes the name from two past the `[` to that `]` (a leading `!` sets negated and is dropped), the content from there to the next `]`, adds the block (`0xdb1c70`) and calls itself from one past the content's `]`. So a block's content holds no `]`, blocks do not nest, and they come in order without overlapping. Any `[` in a definition with parameters starts a block, one inside a quoted string too (none was looked for in the game's files).

## One use's text

`0xdb0860(definition, arguments, out)`, three arguments. The arguments are a list of the same kind, entries of 0x40 bytes: `+0` the name, `+0x20` the value.

1. **`0xdb0990(definition, arguments)`** checks each argument against the parameters' names and the blocks' names, string by string, and logs "Compiling source for … failed for missing args: …" with those that match neither; on failure the use gets no text. It costs arguments × (parameters + blocks) comparisons of names.
2. The definition's text is copied into `out` (`0x95110`, from its C string).
3. **`0xdb1410(definition, out, arguments)`** (called only at `0xdb08c0`) takes the blocks from the last to the first. For each it looks for an argument of the block's name (a walk of the arguments, the first that matches), then builds a new string: `out` up to the block's `[` (the place clamped to `out`'s length), the block's content when negated differs from found (`0xdb15f7`), and `out` from one past the block's end (clamped at 0). Each part is copied into a string of its own and then again from its C string, and `out` is set from the new string's C string: about five copies of the text for each block.
4. **When the definition has blocks** (`+0x84` not 0), the parameters are found again in the resolved text (`0xdb0d60(definition, out, list, 0, 0)`, into a list on the stack); otherwise the definition's own list is used.
5. **`0xdb1800(_, out, arguments, parameters)`** takes the parameters from the last to the first; for each it looks up the argument of that name (the first that matches; none gives an empty value), and builds `out` up to the parameter's first `$`, the value, and `out` from one past its second `$`, with the same copies as the blocks'.

The places are those of the text as it was before the step; taking the entries from the last keeps the earlier ones right. Every copy goes through a C string, so the text is cut at its first NUL byte, if it has one.

**Who calls it**: four places, two for triggers (`trigger.cpp`) and two for effects (`effects\basiceffects.cpp`):

| Function | Calls `0xdb0860` at | What it does with the text | Reached from |
| --- | --- | --- | --- |
| `0xb9a490`(use) | `0xb9a4e9`, with the definition at `use+0xa8` and the arguments at `use+0x88` | reads the text as script (a memory stream `0x1718f80`, the text tokenizer `0x170d250`, the reader `0x17068b0`) into the trigger | the end of `0xb99780` (a method of a class whose vtable holds it at `0x1d07788`), _inferred_ to be the use's read |
| `0xdd47b0`(use) | `0xdd4803`, `use+0x78` and `use+0x58` | the same, into the effect | the end of `0xdd3ca0` (vtable entry `0x1d23ab8`) |
| `0xb99bc0`(use, out) | `0xb99c12` | logs "Error … when compiling instance of …" on failure | `0xb99b80` and `0xb99e70` ("failed trigger validation for"), _inferred_ to be the validation |
| `0xdd4090`(use, out) | `0xdd40dc` | the same for an effect | `0xdd4050` and `0xdd4330` |

`0xdd4330` was 10% of the samples of "What follows the flags" at the end of InitGame in a start sampled with the player's playset ([loading.md](loading.md)), about 0.24 s of 2.4. Whether a use is made again when the trigger or effect runs in a game was not found.

## How much text the base game makes

Counted from the game folder's script, not from the game: the definitions in `common/scripted_effects` and `common/scripted_triggers` whose body holds a `$`, and their uses (`name = {`) in `common`, `events`, `decisions`, `missions` and `history`. The uses inside other definitions are counted once each, though each is made again for every use of the definition it is in, so the counts are a floor.

- 355 definitions with parameters; 8,676 uses; 4.0 MB of definitions' text made over the uses; 41,762 parameters put in.
- The sum over the uses of (parameters × the definition's length) is 79 million bytes, so about 400 MB copied for the parameters alone, at each of the two times a use is made; the blocks add to it (`complex_dynamic_effect`, used 102 times, is 3.6 KB with 50 parameters and about 18 blocks).
- The largest by that sum: `complex_dynamic_effect` (102 uses), `internal_advisor_generation_effect` (5 uses, 12 KB, 177 parameters), `spawn_likely_rebels` (33, 6.9 KB, 45), `create_new_culture_group` (2, 10.5 KB, 216 parameters, about 100 blocks).

So with the base game the text's making was expected to be tens to a few hundreds of milliseconds of the LoadingThread in all (it was 1.42 s, with many more uses than expected: "Timed in the game"), against the reading of the text it makes as script, which follows each use's making and was not measured apart. A mod whose effects are tens of kilobytes with hundreds of parameters each, used thousands of times, makes it seconds.

## What a faster way would rest on

- **One pass**: build `out` once, from the left, copying the text between entries and each block's content or argument's value, instead of rebuilding it for each entry. It gives the same text because the blocks, and then the parameters, are each in order and apart (from their finders above), and because each entry's place is taken against the text before its step, which the pass keeps as the original places. To give the same answers in every case it keeps the game's: the first argument of a name, an empty value for none, the places clamped to the text's length and at 0, and the text cut at a NUL.
- **Where**: `0xdb1410` and `0xdb1800` each replaced as a whole (their only callers are `0xdb0860`), or `0xdb0860` itself; the argument check and the parameters found again could stay the game's. It changes no game state, and its answers can be checked against the game's text for each use, as the speed-ups' checks do.
- **What it gives**: with the player's playset, up to most of the 1.21 s the substitution took and the 0.05 s of the blocks ("Timed in the game"), on the LoadingThread while it reads the databases, which is the start's path to the main menu ([loading.md](loading.md)); the part of each use's making that grows with the square of its size. The script reading of the made text, which is likely larger with the base game, stays as it is.
- **Risks**: the recursion of the finders (one call per parameter or block) is the game's and stays; a `[` in a quoted string makes a block in the game, and must in the pass.

## Not found, and left open

- Why a start makes 37 times as many uses as the files hold: uses inside other parameterized definitions, made again for each of their own uses, are the likely cause, not followed.
- When the validation's `0xb99bc0` and `0xdd4090` run: not in this start.
- Which methods `0xb99780`, `0xb99e70`, `0xdd3ca0` and `0xdd4330` are of their classes (the image has no type names for those vtables), and whether a use is made again at run time.
- **The script tokenizer's keyword lookup** was not read.
- **The triggers' optimization profile** at the end of loading, `0xb9b4e0`, was 1.1 s of a start with the player's playset and called 2.3 million times, recursively ([loading.md](loading.md)); what it looks up for each trigger, and how, was not read.

## Timed in the game

With `dev-call-trace`'s `times` on, through a start to the main menu with the player's playset, every DLC, on a test game with another beside it ([the run](../prototype-log/2026-11-11-five-candidates-timed.md)); all on the LoadingThread, within 7.5 s of the start:

| Function | What | Calls | Time |
| --- | --- | --- | --- |
| `0xdb0860` (3 arguments) | each use's text made | 320,905 | 1.423 s |
| `0xdb1800` (4) | the parameters' substitution | 320,905 | 1.211 s |
| `0xdb1410` (3) | the blocks | 320,905 | 0.049 s |
| `0xb9a490` (1) | a trigger's use made and read | 231,203 | 2.442 s |
| `0xdd47b0` (1) | an effect's use made and read, nested in each other | 89,702 | 14.466 s counted with the nesting |
| `0xb99bc0`, `0xdd4090` (2 each) | the validation's | none | |

Each timed call takes the tool's lock, about a microsecond on one thread, so the times are a little long; the counts stand. Too many to time: `0xdb0d60` and `0xdb1240` (once per parameter, recursively), and `0xb9b4e0` (2.3 million calls).
