# Native UI polish 0.2.3

Validated on 2026-10-26 against the unrestricted EU4 executable SHA256
`97dd63a811487fbe2c917f2c15b1e68f7222bef74a833889cf943c8010049d5e`.

## Changes and native evidence

The army panel tooltip dispatcher at RVA `0x1453b70` builds a caller-owned
aggregate with primary and delayed MSVC strings at offsets 0 and 32. Its
post-call observer fills only the empty result for the bound regional-combat
button. Both strings must initially have length 0 and SSO capacity 15.
The native append routine at RVA `0x932f0` copies plugin-encoded Chinese bytes
using the game's allocator. The callback checks copied length, capacity and
contents. The primary text includes a gold title and line breaks; the delayed
section stays empty. No Rust allocation is transferred to a game destructor.

Focused Ghidra evidence is in the original analysis workspace's
`work/native-ui/pseudocode1-annotated.txt`, `pseudocode2.txt`,
`pseudocode-tooltip-string.txt`, and `pseudocode-tooltip-aggregate.txt`.
The latter confirms the aggregate destructor at RVA `0x8dad0` destroys the two
strings. A full REA provider analysis timed out; these focused headless results
and live observations are the evidence, not a claimed successful full analysis.

The radius editor now has a root-owned background GUI button using a new
`GFX_rc_settings_background` cornered tile and the stock `tiles_dialog.dds`.
It has no command subscription, consumes clicks, and is drawn before controls.
Its native visibility follows the draft editor and selected army. Vtable,
show/hide targets and visibility readback are checked. Both GUI and GFX files
must accompany the DLL update.

## Validation

- In the migrated source at `D:\Workspace\eu4-regional-combat`, Cargo formatting,
  offline locked release build and all-target tests passed: 65 library and 11
  launcher tests; one explicit resource-generation test remains ignored.
- Isolated PID 45964 loaded an Ethiopia save while paused. Chinese tooltip text
  appeared; the framed background displayed without hiding controls. Radius
  changed from 6 to 7. A click on blank background did not select the map.
  Cancel dismissed the background and restored radius 6. The game exited
  without saving. This first run exposed that +32 is delayed text, not a title;
  the final version moved the gold title into the primary body.
- Installed final PID 77584 loaded Ethiopia 1445-01-01, paused. Hovering the
  regional button visibly displayed the gold Chinese title and complete help.
  Log `ui-tooltip-verified` reports 420 primary bytes and 0 delayed bytes.
  Right-click opened a dark framed panel at radius 5; applying that value
  closed the complete panel. Visibility logs confirmed false -> true -> false.
- The installed process loaded the intended native DLL; its file hash matches
  the release build. No error events occurred in the inspected session log.
- Final installation was launched through the root `eu4.exe` wrapper directly.
  Steam/Paradox launch compatibility was validated separately in 0.2.2; it was
  not re-tested through Steam for this UI-only update. Combat behavior and
  held-modifier mouse input were not re-tested in this paused UI session.

The installed update is backed up under
`outputs/eu4-program-backups/20261026-before-ui-0.2.3` in the original workspace.
The project migration verified 4,720 copied files by SHA256 and preserved Git
state. Development now uses the D: repository; the old source copy is retained
until the user runs the supplied cleanup command.
