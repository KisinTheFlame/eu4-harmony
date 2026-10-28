# Countries (for `game::countries`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-10-29; what was checked in the running game is marked "tested".

## The country array

- **The country manager** is at the global `0x233d8d0`. Its countries are an array object at `+0x110`: the data, one pointer per country indexed by the tag's index (bits 32–47 of the tag), at `+0x118`; the length, an i32, at `+0x124`. In arrays of this type `+0x10` is the capacity (below), so `+0x120` presumably is; no code reads it through the manager, and the framework does not use it.
  - Indexing, at almost every place that turns a tag into its country, for example `0xb239c` (`4c 8b 05 … 48 8b 41 20 48 c1 e8 20 48 0f bf d0 49 8b 80 18 01 00 00 48 8b 1c d0`): load the manager, shift the tag right by 32, sign-extend the 16-bit index, read `+0x118`, read the pointer at index × 8.
  - The length: when the manager is made (`0xb510c6`, after `new` and the constructor `0x2812a0`), `lea rdx, [rax+0x110]` hands the array to `0xb510f0`, which reads its length at `+0x14` and its data at `+8` (`0xb5113f` `41 8b 46 14 4d 8b 76 08 48 98 49 8d 2c c6`) and walks every pointer, reading each country's tag at `+0x20` (`0xb5117c` `48 8b 5b 20`). The array it fills, in its first argument at `+0x10`, has the same layout: data `+8`, capacity `+0x10` (doubled when the length would pass it, `0xb51176`–`0xb51188`), length `+0x14`.
- **A country's tag** is at `+0x20` of the country (`country::TAG`); its index is the country's place in the array. The framework finds a country by its letters by walking the array to its length and comparing the letters of each valid tag, checking that each tag's index is its place (`game::world::countries`).

What the framework relies on is in `objects::campaign::countries` and the dependency points "country array" (`0xb510c6`) and "country array walk" (`0xb5113f`).
