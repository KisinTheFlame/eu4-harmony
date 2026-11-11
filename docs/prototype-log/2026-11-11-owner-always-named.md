# 2026-11-11 The owner always named, a test game for each scenario (tested by me)

The claims' commands and the scenario runner needing `--owner`, each scenario in a test game named `<owner>-<scenario>-<six letters and digits>`, and the report written again as each scenario ends ([test-games.md](../plans/test-games.md), [scenarios.md](../plans/scenarios.md)). Before, two runs of one scenario each from this checkout took its folder's name; the second played in the first's test game, and both failed at their starts (`cannot copy … eu4_harmony_core.dll …: the file is in use`). Hidden test games of this branch's player build.

- **No owner**: `cargo xtask game claim --purpose x` and `cargo xtask scenario extensions` each refused, `--owner <name> is needed`, and claimed nothing.
- **Two runs at once**: `cargo xtask scenario extensions --owner lucid`, twice two seconds apart: test games `lucid-extensions-1klito` and `lucid-extensions-koyr95`, both passed.
- **The report as it goes**: `late extensions --owner lucid --jobs 1`: once `late` had ended, `report.md` had its row and "Still playing or to play: extensions."; at the end both rows, that line gone. The runner said the report's folder as it began.
- **The build's output**: `cargo build --release` gave no warning; each DLL had given a linker message of several lines before.
- **Not verified**: a run of more scenarios at once than two, and one interrupted by Ctrl-C, with the new names.
- **Cleanup**: every claim released by the runner; their kept logs and the reports deleted.
