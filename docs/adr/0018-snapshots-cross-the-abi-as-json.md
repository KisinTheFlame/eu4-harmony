# A snapshot with text and lists crosses the ABI as JSON

A snapshot of a game object that holds text or lists of varying length, such as a province's (its name, its buildings, what can be recruited there and why not), reaches an extension as one capability that writes UTF-8 JSON into the caller's buffer, by the two-call pattern every list uses. The JSON's shape is written on the `GameApi` field; the SDK reads it into its own Rust types, so an extension never sees the JSON. A country is its `CountryId` in this JSON, as everywhere else in the ABI, not the tag remote clients use.

Snapshots made of fixed fields only, such as an army's, stay `#[repr(C)]` structs.

## Considered Options

- **A `#[repr(C)]` struct with a host function for each text or list in it** (the name, the terrain, the buildings, the recruit options with their reasons): one snapshot would take five calls and as many fields in `GameApi`, an extension could mix pieces read at different moments, and a later field would mean another function or a struct that grows by appending inside a struct.
- **A struct holding pointers to text the core owns**: the text would have to outlive the call and be freed by someone; the ABI passes nothing it does not copy into the caller's buffer.
- **A binary encoding of our own**: as compact as a struct and as flexible as JSON, but both sides would need a reader and a writer that nothing else uses, while the SDK already depends on `serde_json` for extension save data.

## Consequences

- A field added to such a snapshot is a new JSON member: an SDK built before it ignores it, and an SDK built after it gives it a default (`#[serde(default)]`) so that it still reads a core built before it. Changing a member's meaning still breaks the ABI (ADR 0014).
- Reading a snapshot costs an extension a JSON parse, a few microseconds, at most once a call.
- The core and the SDK each have a test against the same JSON text, so neither can drift from the shape the field describes.
