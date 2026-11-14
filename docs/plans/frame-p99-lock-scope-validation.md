# Validate shorter lock-order ownership for submission P99

Status: completed 2026-11-14. V1–V3 finished: state tests, full checks and four A/B/B/A games completed. No repeatable submission-P99 improvement was demonstrated; the prototype was not accepted and was removed, with its patch and measurements retained. See the [comparison](../research/frame-drawing-p99.md#bounded-ownership-experiment). Terms are those of the existing glossary; the plan follows [ADR simulation-and-interface-take-turns](../adr/2026-11-10-simulation-and-interface-take-turns.md).

## Goal and scope

Determine whether shortening ownership acquired only for game-lock ordering reduces the interval between successful frame submissions without unacceptable loss of completed-day throughput. This is an optimization experiment, not authorization to publish a release or remove synchronization generally.

The [P99 investigation](../research/frame-drawing-p99.md) observed a short stream-related critical-section scope followed by tens of milliseconds of continued presentation-lock ownership. Its normal return releases that internal critical section. The frame's real crossings and pending preparation remain independent reasons not to draw.

## Prototype behavior

Only the measured native stream shared-state getter opts into a bounded scope. A main-thread lock-order acquisition inside it can end at its outermost normal return. Ownership already held for an application frame or crossing is left alone. A real crossing inside the scope promotes the ownership to the ordinary frame-check lifetime, including graphics-table entries that would otherwise skip detection merely because the lock is held.

Nested scopes remain protected until the outer return. Abnormal returns cancel early release; unexpected frame/lifecycle transitions relinquish eligibility rather than releasing a different owner's lock. A frame check inside an active scope cannot release crossing ownership while its internal game lock may remain held. Other game-lock acquisitions retain the previous behavior. The prototype is explicitly labeled and its complete diff is retained as research evidence.

## Parts

- **V1 — Bound ownership and test transitions.** Add the minimal state distinction and one verified observer. Check scope-only ownership, preexisting ownership, nesting, promotion, abnormal returns and frame/day transitions; run the repository's normal validation and offline dependency checks.
- **V2 — Compare real games.** Preserve immutable baseline and prototype cores with hashes. Run A/B/B/A sequentially in owned hidden games with the same seed, playset, rendering settings and camera. Each run records a paused control and a 120-second speed-5 window. No compilation or binary analysis during measurements. Record completed-day throughput, submission P99 and the rest of its distribution, interface age, failures, missing events, and external processes. A change in window progress or background load remains a limitation, not a measured gain.
- **V3 — Keep the evidence and decide.** Write one run record per game and a comparison in the research note. Release every claim and remove or archive disposable runtime code and binaries. Report the observed benefit and cost without extrapolating to all saves or promising a release.

Every part is done when its evidence is retained, its limitations are stated, and it leaves no owned test game running. A large P99 improvement alone is not acceptance if it comes from substantially slowing the simulation or losing submissions or events.
