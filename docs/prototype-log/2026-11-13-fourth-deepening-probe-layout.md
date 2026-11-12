# 2026-11-13 Fourth deepening probe layout (tested by me)

The first native run of [fourth-deepening.md](../plans/fourth-deepening.md) used commit `450b5c34`, with master `e902f0a0` included, on EU4 1.37.5. `cargo xtask scenario windows --owner native-lifetimes-4510f0 --out target/scenarios/fourth-deepening-native-20261113` built the player build with test-game pipes, claimed `native-lifetimes-4510f0-windows-xo26x3` and started process 54604 hidden.

The main menu was reached after 27 seconds, but the scenario failed waiting for its first pop-up marker. The core log explains the setup error:

```text
extension "qa-windows" not loaded: main.lua's windows.other: status at (24, 66), 260 by 35, does not fit inside the window, above its Close button, 320 by 100
```

The new probe's second window was too short by one GUI unit. The larger probe was also given more height so that all its controls fit above its Close button. This was a test-mod layout error, rejected before the extension ran, not a native construction or tooltip failure. The scenario now checks the startup rejection first, so the layout reason is reported instead of waiting for a step timeout.

The run establishes neither native lifetime behavior nor tooltip presentation. It captured no frames. Release reported no crash and exit code `0x1`; the claim was released and its logs retained under the claim's `kept` folder. The rerun is recorded separately.
