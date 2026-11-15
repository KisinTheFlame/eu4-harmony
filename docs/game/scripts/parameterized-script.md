# Parameterized scripted triggers and effects, compiled for each use

## The definition

A scripted trigger's or effect's definition with parameters is read by `0xdb04d0(definition, reader)` (from the databases' reader at `0xacc344`; `scripted_triggers` and `scripted_effects` are `0xace660` and `0xacb950`, [loading.md](../program/loading.md)). It keeps the definition's text, the tokens joined, at `+0x38` (`std::string`), and two lists of the game's kind (data at `+8`, count at `+0x14`, i32):

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

`0xdd4330` was 10% of the samples of "What follows the flags" at the end of InitGame in a start sampled with the player's playset ([loading.md](../program/loading.md)), about 0.24 s of 2.4. Whether a use is made again when the trigger or effect runs in a game was not found.


## Limits of the reading

The tokenizer keyword lookup, runtime re-expansion and the roles of several virtual methods remain unresolved. The large excess of uses over file definitions may come from nested expansion, but was not followed completely. Trigger-profile cost construction was subsequently read and is described under [localisation loading](../program/localisation-loading.md#trigger-profile-cost-construction).
