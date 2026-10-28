# Reverse-engineering findings from regional-combat

The framework's game capabilities (reading, pathfinding and orders, interface bindings, map coloring, save data, events) were ported from the native implementation in eu4-regional-combat. Its reverse-engineering notes from that time are kept here, as the source for those addresses and how they are used.

- Written on 2026-10-26, against the unrestricted baseline (`97dd63…`). It has the same section layout as vanilla 1.37.5 (`9ad3efe1…`), so the RVAs in these files apply to both.
- The implementation at the time was Frida scripts (`tools/*.js`) and later a Rust launcher/DLL (`native/`). The scripts, probes, pseudocode under `work/` and so on that the text mentions exist only in the git history of eu4-regional-combat before 2026-10-27, and are no longer maintained.
- The `Dependency` tables and comments in the framework code refer to these files. Before you change a game capability in the framework, read the file that covers it.

| File | Contents | In the framework |
| --- | --- | --- |
| `native-ui-findings.txt`, `native-settings-findings.txt` | Army panel button bindings, click signals, right click, show and hide, text boxes | `core/src/ui.rs` |
| `ui-polish-validation.md` | Output structure of hover tooltips (`0x1453b70`, `0x932f0`) | `core/src/ui.rs` |
| `native-range-preview-findings.txt` | Map mode refresh and coloring | `core/src/map.rs` |
| `native-save-findings.txt` | Save and load flow | `core/src/saves.rs` |
| `native-scheduler-findings.txt`, `native-fow-findings.txt` | Daily update, pause, fog refresh | `core/src/game/events.rs` |
| `native-manual-findings.txt`, `native-defeat-findings.txt` | Manual orders, defeat | `core/src/game/events.rs` |
| `native-path-findings.txt`, `native-order-adapter-contract.md` | Pathfinding, move, stop | `core/src/game/orders.rs` |
| `native-geography-findings.md`, `native-hostility-findings.txt`, `native-targets-findings.md`, `native-army-identity-findings.txt` | Province adjacency, straits, hostility, visible armies, army identity | `core/src/game/world.rs` |
| `native-userdir-findings.txt` | Format of `userdir.txt` | `core/src/paths.rs` |
