---
status: superseded in part by ADR-0019
---

# A snapshot with text and lists crosses the ABI as JSON

The reason given below that the ABI passes nothing it does not copy into the caller's buffer has been superseded by ADR 0019: a host function that cannot be called twice for the same answer lends its text to a callback for the call. The decision on snapshots still holds.

A snapshot of a game object that holds text or lists of varying length, such as a province's (its name, its buildings, what can be recruited there and why not), reaches an extension as one capability that writes UTF-8 JSON into the caller's buffer, by the two-call pattern every list uses. The JSON's shape is written on the `GameApi` field; the SDK reads it into its own Rust types, so an extension never sees the JSON.

Every such snapshot writes its JSON by the same rules:

- **Identifiers are the ABI's own, as JSON numbers**: a country is its `CountryId`, a province its `ProvinceId`, an army its `ArmyId`, as everywhere else in the ABI; not the tags and decimal strings remote clients use. A `CountryId` or an `ArmyId` may be above 2^53; `serde_json` reads it exactly as a `u64`.
- **Text is Unicode**: a name, a key or a reason the game keeps as game text (GLOSSARY) is converted to Unicode (`unicode::game_text`) before it is written, so the JSON holds its characters, never the game's bytes. A name or a key is converted with `game_text::to_utf8`. Text the game builds from its localisation, which can carry formatting codes, such as a reason or an event's text, goes through `game_text::plain`: colors left out, an icon written as its name.
- **A fixed-point value is written as the number it stands for**: a development the game keeps in thousandths, 9500, is `9.5`.
- **The SDK never refuses a member it does not know**: no type it reads from such JSON has `#[serde(deny_unknown_fields)]`.

Snapshots made of fixed fields only, such as an army's, stay `#[repr(C)]` structs.

## Considered Options

- **A `#[repr(C)]` struct with a host function for each text or list in it** (the name, the terrain, the buildings, the recruit options with their reasons): one snapshot would take five calls and as many fields in `GameApi`, an extension could mix pieces read at different moments, and a later field would mean another function or a struct that grows by appending inside a struct.
- **A struct holding pointers to text the core owns**: the text would have to outlive the call and be freed by someone; the ABI passes nothing it does not copy into the caller's buffer.
- **A binary encoding of our own**: as compact as a struct and as flexible as JSON, but both sides would need a reader and a writer that nothing else uses, while the SDK already depends on `serde_json` for extension save data.

## Consequences

- A field added to such a snapshot is a new JSON member: an SDK built before it ignores it, and an SDK built after it gives it a default (`#[serde(default)]`) so that it still reads a core built before it. Changing a member's meaning still breaks the ABI (ADR 0014).
- A member that names one of a fixed set of values, such as a recruit option's `kind`, may get a new value. Every enum the SDK reads from such JSON therefore keeps a catch-all variant (`#[serde(other)] Unknown`, or a hand-written reader that does the same), so that a value added after an SDK reaches an extension built with it as unknown, instead of failing the whole snapshot. Giving an existing value another meaning still breaks the ABI (ADR 0014).
- Reading a snapshot costs an extension a JSON parse, a few microseconds, at most once a call.
- The two-call pattern would make the core build every snapshot twice, the first time only for its length. The SDK's first call therefore has room for 16 KiB, several times a province's snapshot, so a snapshot that fits is built once; a larger one is built again for the second call, with the room the first asked for. Both calls come within one call of the extension's, on the main thread, so they give the same snapshot.
- Each such snapshot has one JSON file in `abi/json/` (`province.json`), holding every value each enumerated member can take. The core's test checks that its writer turns a snapshot into the same JSON value as the file, and the SDK's test reads the file, both by `include_str!`; so neither side can change the shape without the other's test seeing it.
