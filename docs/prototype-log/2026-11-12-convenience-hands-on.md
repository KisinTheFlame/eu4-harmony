# 2026-11-12 Convenience Tools hands-on handoff

Status: handed to the user for the requested trial; awaiting their feedback and release. This is not a report of the user's results.

After [the reviewed-capacity smoke](2026-11-12-convenience-reviewed-capacity.md), a separate claim `convenience-hands-on` was prepared in `convenience-hands-on-20261112-200922`, with its own user directory, core, mod copy and pipe. The user explicitly asked to see and operate it before publication. Preparation was hidden; `desktop.show` then returned `hidden: false` without taking focus. No other EU4 process was present before showing it.

- Window: `Europa Universalis IV`, process 71172 at handoff.
- Fresh Castile, 1444.11.11, paused. The camera looks at Castile and the Convenience Tools window is open with an empty result line.
- No country-wide button was pressed in this claim. Initial live capacity was 231500 of 400000 thousandths; Galicia was still a full-core state costing 28000.
- The actual isolated core has SHA-256 `5715ca3226a16d7507aa90ad1f0fa29bc4de6619fe80a7a24ae1c309e23fedc6`. The isolated `main.lua` and `batch.lua` were compared byte for byte with the tested source. Both the new local-area response and live country-capacity response were checked before handoff.
- The native local-save dialog wrote an untouched initial save, named `Convenience Tools - Castile 1444.eu4` in this claim only. `menu.saves` confirmed player CAS, date 1444.11.11 and no missing mods. It can be loaded again through the native menu or this claim's `menu.load`.
- Castile is not eligible to raze; no second country fixture was prepared to delay the initial hands-on handoff.

After handoff, no automated input, tests or core updates are sent to this claim. No player mod directory or installed framework is changed. The claim is intentionally retained only while the user tries it; on their finish, release it with `cargo xtask game release --owner convenience-hands-on` and record the outcome. Workshop creation and upload remain paused pending that trial and a supporting public Harmony release.
