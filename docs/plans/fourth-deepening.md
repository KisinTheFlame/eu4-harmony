# Fourth deepening plan

Status: decided on 2026-11-13 after an architecture review of the recent window, tooltip and drawing changes; both parts authorized together and built. The [native window courses](../prototype-log/2026-11-13-fourth-deepening-native-windows.md) and [tooltip handoff](../prototype-log/2026-11-13-fourth-deepening-tooltip-handoff.md) passed on 2026-11-13; all nine scenarios in the [private-registration replay](../prototype-log/2026-11-13-fourth-deepening-private-registrations.md) passed, while the [whole scenario set](../prototype-log/2026-11-13-fourth-deepening-whole.md) is finishing its first long-game stretch and performance check. No glossary terms or new ADRs are needed.

## Goal and scope

Bring the native lifetime and frame-handoff knowledge now spread through callers into modules whose interfaces exercise the whole process. This follows the [third deepening](third-deepening.md), retaining its window desired state, bound elements and presentation-lock turns.

The work concerns the game's side of extension windows, pop-ups and list rows, and the handoff that draws the game's tooltip over the framework UI. It does not change capabilities, Lua layouts, dependency points, the game's drawing order or the thread division of ADR simulation-and-interface-take-turns. Entry-button placement, the development tools' declarations and the catalogue remain outside this work.

## What the game does

The game builds windows hidden, marks them dead before its sweep frees them, and owns a list's row windows. Subscriptions must be detached while their elements still live ([research/extension-windows.md](../research/extension-windows.md), [research/window-elements.md](../research/window-elements.md), [game/interface.md](../game/interface.md)). The framework owns its windows' lifetime under ADR extension-windows-are-game-windows and supplies definitions as ADR framework-files-in-the-game-file-system specifies.

A framework tooltip uses the game's prepared text node. A ready node is omitted from ordinary drawing and drawn after the framework UI, before screenshots and Present, under the presentation lock. Native object identity, current content and the current frame determine whether that handoff remains usable ([research/panel-entry-text.md](../research/panel-entry-text.md)).

## Behavior

Healthy rendering and the extension interface stay as they are. A partial native construction or update must retain a clear owner until recovery; it must not be recorded as successfully applied. Tooltip cancellation must prevent an old node reaching another frame, and failed native drawing must leave the scene and reentry state closed.

## Parts

**F1. Native window lifetime through one process.** Concentrate definition generations, creation, binding, application, list replacement and teardown, including native destruction and session end. The internal native-operation seam has both the real game adapter and a recording test adapter that can fail operations. Test complete courses through the production interface: unchanged frames, replacing rows, failure at each construction step, generation changes, repeated destruction and closing. Keep the desired-state and bound-element behavior tests; remove only assertions replaced by stronger process tests.

**F2. A tooltip's frame handoff through one process.** Concentrate eligibility, content identity, pending drawing, completion and cancellation. Keep native layout and readiness checks, drawing-thread ownership and the presentation lock. Exercise ordinary drawing through completion or cancellation; retain the real-memory tests of node readiness and vector filtering. Where native drawing is substituted, both the game and recording test adapters must exist. Test content changes, stable refresh, holder and owner destruction, reset, held frames, new frames, and begin/draw/end failures.

## Every part is done when

The `windows` scenario drives a private test mod through initial, replaced and cleared rows, tooltip clearing, close/reopen, another big panel and pop-up answers, repeated through extension reload, in-game load and another session. It rejects native errors and retains selected captures for visual inspection; it is also included in `whole`.

The new module replaces its callers' protocols rather than duplicating them. Tests drive the same process that the native adapters use. Formatting, the locked release build, workspace tests, both clippy configurations and offline checks pass. A hidden claimed test game exercises the changed native paths; each run has its own [prototype log](../prototype-log/README.md), stating what was not verified. Both parts are reviewed and delivered together in one pull request.
