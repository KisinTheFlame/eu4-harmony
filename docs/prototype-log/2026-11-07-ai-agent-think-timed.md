# 2026-11-07 Each AI agent's think, timed (tested by me)

How often each of an AI country's agents thinks and what its thinks cost ([research/ai.md](../research/ai.md), "The agents"). Master at `1d47fca`, the call trace built in (`dev-call-trace`), `times` on the King's day `0x14b370`, its think `0x14acc0`, the agents' day `0xe4890` (with its vtable as the field) and eleven thinks (with the country at `+0x28`); the AI-only game's autosave of 1780.7.1, observed, a year at speed 5. One claim of a few minutes, the game started hidden.

- **First start**: the thinks counted as reading one argument (`0xe9f30`, `0x1b82b0`, `0x1b4180`, `0x11dc00`, `0x173180`, `0x168070`) were traced forwarding one; the game crashed on the first day, in `eu4+0x415cce` under the army think `0xe9f30` and the trace's observer, with a register holding a thread id as a pointer: the think hands its second argument on to its calls unread. Given as `RVA:2`, every function then forwarded two.
- **Second start**: 366 days, 1780.7.1 to 1781.7.1, 431,143 agent days and 200,000 thinks; 115 AI countries, about 375 army and fleet agents; no exploration plan thought. The thinks' times are in [research/ai.md](../research/ai.md).
- **Not verified**: other years, a player's game; the trace's own cost in the times.
- **Cleanup**: `dev.toml` deleted; the crash folder went into the claim's kept files and out of the user folder; the timing file (48 MB) deleted from the scratchpad and the claim's kept logs.
