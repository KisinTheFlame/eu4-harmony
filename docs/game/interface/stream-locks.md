# AI stream helpers and lock lifetime

The native stream initializer `0x1a14f0` initializes state flags, formatting fields, the associated buffer and locale state. Its error paths name `ios_base::badbit`, `failbit` and `eofbit`. Calling the shared state a locale implementation is inferred from its use, not a recovered original symbol.

## Getter and guard

At `0x1a1553` the initializer calls `0x1a0c408` with a true flag and returns to `0x1a1558`. The getter constructs numbered guard zero through `0x1a0c750`, reads cached state `0x233b2e0`, initializes it if absent, conditionally retains it through a virtual method, destroys its guard through `0x1a0c7c8` and returns the shared state.

Guard zero enters the critical section at `0x233cdd0` through `0x1a5f504` and IAT slot `0x1b66750`. Its destructor uses `0x1a5f520` and IAT slot `0x1b66758`; the imports identify `EnterCriticalSection` and `LeaveCriticalSection`. `0x1a5f504` is the sole reference to `0x233cdd0` in `eu4.exe`, and `0x1a0c750` is its sole caller. Across all 175 callers of `0x1a0c750` (including the 15 `std::use_facet` templates outside the CRT), every path touches only MSVC STL locale/facet internals and heap allocation; none reaches EU4 graphics, the renderer table, or any crossing. `0x233cdd0` is therefore a leaf lock with respect to Harmony's presentation lock. Once-only initialization and all exceptional paths were not exhaustively reconstructed.

## AI caller context

Observed return addresses follow `0x1a0c764` → `0x1a0c42a` → `0x1a1558` → `0xa3829` → `0xb21f4` → `0x75ba18` → `0x75c313` → `0x758af4`. They connect naval-doctrine work `0xa3620` to country task `0xb0fa0` in AI phase `0x75b4b0`. These are recorded caller sites, not function starts inferred from a stack alone.

The initializer then acquires the same lock in a [second lookup](stream-initialization.md). [Formatting helpers](stream-formatting.md) describe the full known chain. Harmony's presentation-lock lifetime, invalidation and map-name eligibility are separate framework behavior; their measured effects are in [performance evidence](../../research/performance-evidence.md#frame-pacing-and-lock-ownership).
