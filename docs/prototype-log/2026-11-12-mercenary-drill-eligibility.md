# 2026-11-12 Portugal's mercenary drill permission (tested by me)

This follow-up to [the drill-growth run](2026-11-12-mercenary-drill.md) reads the country's actual `allow_mercenary_drill` value. It concerns **drill**, not **discipline**: the player's later clarification established that the report concerns mercenary discipline missing from the battle panel. This run does not answer that report; the relevant [discipline-panel run](2026-11-12-mercenary-discipline-panel.md) is separate.

## Setup and result

- Build: `origin/master` at `7f694b85`, Harmony 0.1.3, built with `eu4-harmony-core/dev-test-games`.
- Claim owner `merc-drill-eligibility`, folder suffix `20261112-041045`, process 13492, hidden and silent. No development-options file. Startup reported 2,426 matching dependency points, 281 hooks and 90 writes, with the Regional Combat Workshop extension loaded.
- Input: a fresh copy of the same 63,074,897-byte Portugal save, SHA-256 `5ce7b9cc6e5b211d8e9e8fb35bfedae934cbe68a3f5cf9a9339138399030640a`. The two missing mod entries were removed only from a derivative copy. The six remaining mods and their order were as in the first run. The original was not edited.
- The initial `game start` was refused because a concurrent disassembly build had replaced the output DLL with one without test-game pipes. Nothing launched on that attempt. A fresh `cargo build --release --features eu4-harmony-core/dev-test-games` passed; the next start succeeded. This was a build-artifact mismatch, not a game crash.
- Loading returned session 2, Portugal, 1582.7.1, not observe mode. No days were advanced, no army was ordered and no battle was created.

```bash
eu4-harmony game --test-game merc-drill-eligibility country modifiers POR allow_mercenary_drill,drill_gain_modifier,fire_damage,shock_damage,fire_damage_received,shock_damage_received
```

The returned values were:

| Modifier | Value |
| --- | --- |
| `allow_mercenary_drill` | 1.0 |
| `drill_gain_modifier` | 0.0 |
| `fire_damage` | 0.006 |
| `shock_damage` | 0.006 |
| `fire_damage_received` | 0.0 |
| `shock_damage_received` | 0.0 |

The first value directly establishes the country's permission in this reduced environment; it is not inferred solely from the completed idea group. The native shared eligibility predicate and drill-damage path are described in [the research](../research/mercenary-drill.md). Regiment-local modifiers and actual battle losses were not measured in this run. The complete eight-mod environment remains untested.

## Cleanup

`cargo xtask game release --owner merc-drill-eligibility` reported `released`, process 13492 stopped with exit code `0x1`, and no crash. `game status` no longer listed this owner. The claim tool retained only its seven-day `kept` logs. Temporary save copies were removed after the subsequent discipline investigation had finished using them; the original hash was unchanged. No product source, installation, commit or publication was made.
