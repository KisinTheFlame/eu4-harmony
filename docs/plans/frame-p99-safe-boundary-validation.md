# Validate a safe release boundary across successive stream-lock calls

Status: completed 2026-11-14. Four checked normal-return boundaries verified by complete mechanism replay and A/B/B/A timing; P99 fell about 70% in both pairs. Daily-rate cost and broader safety remain unresolved. Disposable source/builds removed, clean core rebuilt and resources audited; no installation or publication.

## Question

The [release-path diagnosis](../research/frame-drawing-p99-release-path.md) proved that the getter-only prototype releases ownership and then immediately reacquires it in another helper. Read the later helper and its enclosing initialization through their normal and abnormal paths, establish which internal locks and shared data remain live at each return, then test a justified boundary. User authorization covers this research and optimization validation, not installation or publication.

## Safety argument

Use the game's own instructions, exception metadata and callees. Identify function arguments and checked entry bytes before adding observers. A normal return may release only presentation ownership acquired for lock ordering within the verified scope. Preexisting frame/crossing ownership remains intact. A genuine crossing promotes the ownership to its ordinary lifetime. Nested scopes cannot release an outer scope's protection; abnormal returns retain ownership conservatively. If the evidence does not establish a boundary, record the missing evidence rather than broadening the scope speculatively.

## Validation

First use a matched diagnostic baseline/prototype pair with explicit ownership and stage events. Verify release counts, hold durations, subsequent acquisitions, complete traces and agreement with formal successful-Present-return interval counts. Use a fixed, initially unobstructed Beijing view in hidden fresh Ming games, seed 123456, frozen settings/playset, ten seconds paused and 120 seconds at speed 5 after warmup.

The first diagnostic pair successfully shortened both scoped holds but exposed a third long acquisition through native return address `0x9168b`. Before timing comparisons, read that helper with the same normal/cold/abnormal-path standard, and add a mechanism-only diagnostic recheck if its boundary is justified. The three-scope recheck then exposed another lookup on the same formatting chain, so read the remaining formatting path together before the final candidate. Keep every diagnostic and unmeasured timing candidate as evidence; do not treat different scope counts as a matched instrumentation performance comparison.

The first four-scope diagnostic completed its formal metrics but overflowed its temporary probe buffer. Retain it as incomplete attribution evidence and repeat the mechanism check with a larger buffer and a 60-second running window. An unrelated game's presence and an interrupted startup are retained separately, with no samples claimed. Serialize builds and game startup.

Then remove temporary stage recording and stack capture from the final timing build and run A/B/B/A with the same view and settings. Record P95/P99/max, completed-day rate, submission rate, interface age, paused controls and machine load. Do not compile or analyze binaries while collecting. Record other-game process presence before, during and after each window; do not stop games belonging to another session or the player. The final comparison proceeds even when other games are present, as requested by the user; their presence no longer gates startup. Disturbed windows remain evidence and do not establish an isolated gain.

Run the repository's formatting, tests, release build, both clippy configurations and offline checks for the experimental implementation. Retain a complete patch and build identities. Each game receives its own prototype-log record. Acceptance requires both a defensible boundary and repeatable benefit without apparent correctness failures; short tests cannot establish all-view or late-game safety.

## Completion

Record the boundary argument, all results and limits, update the game page and performance summary, and index newly named addresses. Restore disposable runtime source, clean debug/release experiment artifacts, rebuild and scan the clean core, release every owned claim, archive the experimental driver and preserve useful evidence. No install, commit, push or publication is part of this validation.
