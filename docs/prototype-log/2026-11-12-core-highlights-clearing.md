# 2026-11-12 Clearing temporary highlights before asynchronous drawing (tested by me)

A single-variable causal test after [the 0.1.4 baseline](2026-11-12-core-highlights-baseline.md).

- **Build and setup:** same master base, independent tooltip guard, developer features, playset, save, resolution, trigger and pixel criterion as that run. The only added change replaced the call to `0xe1ac80` at `0x815928` with a five-byte no-op. The core build completed before `game start`; no concurrent build/start ambiguity. Its startup log identified core 0.1.4 and the drawing thread on, unlimited.
- **Test game:** hidden and silent, owner `core-highlight`, claim `core-highlight-20261112-185059`, own core/user folder/pipe. No installation into the game folder.
- **Result:** eight red-pixel counts `[6, 6, 50, 704, 846, 389, 10, 6]`. Screenshots showed the Ottoman core outlines again, pulsing normally. The baseline's eight counts were all 6.
- **Interpretation:** the main frame's clearing, after `frame_draws` returns but before the asynchronous draw, removes the transient border inputs. The producer and reader were traced separately in [research](../research/map-drawing-reads.md#the-frames-temporary-highlights-read-and-tested-2026-11-12). Repeated drawing consuming the input is not needed to explain the loss.
- **Not a correction:** leaving the clear disabled accumulates old targets. This experiment was removed before building the prepared-input correction; the original clear call remains in the final code.
- **Cleanup:** released immediately after capture, no crash recorded, framework/game logs kept. No test game of this owner remained held.
