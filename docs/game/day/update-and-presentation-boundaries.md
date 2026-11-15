# Completed days and submitted frames

The daily update (`0x758430`) has finished only when it returns; reading a changed date inside the call does not establish a completed day. A frame inside that update does not mean that its commands may execute at every point of the day. See [time](time.md) and [safe points](safe-points.md).

The native Present wrapper (`0x16d4db0`) returns false for a negative device result or S_PRESENT_OCCLUDED. Successful return must be distinguished from a completed interface update or a visible frame. The framework’s measurement contract, held-frame attribution, event capacity and incomplete-window limits are described in [performance](../../performance.md), rather than treated as rules of the unmodified game.

The original boundary study did not measure input-to-visible response or pure interface work independently.
