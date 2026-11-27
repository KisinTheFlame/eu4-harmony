# Fold `eu4-harmony-checks` into `eu4-harmony-remote`

This supersedes in part [ADR refusals-live-with-their-capabilities](2026-11-07-refusals-live-with-their-capabilities.md): refusals no fake makes still live with their capabilities in the core, while the shared fake-game refusals, `Date`, and `form` now live in `eu4-harmony-remote` rather than a separate `eu4-harmony-checks` crate.

After ADR refusals-live-with-their-capabilities moved 16 of 22 refusal modules into the core, `eu4-harmony-checks` shrank to ~600 lines with zero independent consumers: every crate depending on `eu4-harmony-checks` (`core`, `cli`, `lua`, and `remote` itself) already depended on `eu4-harmony-remote`. Keeping it separate required a redundant `Status`/`Refusal` pair converted into both `eu4_harmony_remote::Error` and `core::base::status::Fail`.

## Considered Options

- **Keep `eu4-harmony-checks` separate**: preserves a smaller compilation unit if a future crate ever needed `Date` or shared refusals without the capability catalogue, at the cost of a redundant error type and extra workspace dependency wiring today.
- **Move shared refusals into `core`**: breaks `cli`'s fake game and `lua`, which must share refusals without depending on the Windows/native `core`.

## Consequences

- Shared refusals live in `eu4_harmony_remote::checks`, `Date` in `eu4_harmony_remote::Date`, and Lua serialization mode in `eu4_harmony_remote::form`.
- Shared checks return `eu4_harmony_remote::Error` directly; `checks::Status` and `checks::Refusal` are removed, and `core::base::status::Fail` converts directly from `eu4_harmony_remote::Error`.
- The `checks/` workspace member is removed.
