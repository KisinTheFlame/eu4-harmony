# Stream initialization and guard acquisition

Initializer `0x1a14f0` spans `[0x1a14f0, 0x1a1648)` and reads RCX, RDX and R8b. It calls the [shared-state getter](stream-locks.md) at `0x1a1553`, returning to `0x1a1558`, stores the implementation in an allocated wrapper and retains it before constructing a stack-local locale argument.

At `0x1a1580` it calls `0x91a70`, returning to `0x1a1585`. That helper takes numbered guard zero on the same critical section `0x233cdd0`; the observed inner return site is `0x91a8b`. Its complete normal-return behavior and the remaining cold/exceptional limits are described in [formatting and facet lookup](stream-formatting.md#the-second-helpers-actual-work).

The two native acquisitions have separate guard lifetimes. Experiments that released Harmony's presentation ownership at the first return exposed the second acquisition; that observation does not change native ownership. The A/B results and limitations are retained in [performance evidence](../../research/performance-evidence.md#frame-pacing-and-lock-ownership).
