# Fixed window backgrounds

Status: built and checked 2026-11-15; B1–B3 complete. Runtime evidence: [fixed window backgrounds](../prototype-log/2026-11-15-fixed-window-backgrounds.md). No new glossary terms. Follows ADR extension-windows-are-game-windows.

## Goal and scope

Keep the normal blue panel and gold edge of button areas and fixed extension windows without button hover or press presentation. Keep the existing texture, border thickness, spacing and size adaptation. Vanilla windows, movable extension windows and real controls retain their behavior.

## What the game does

A named window background stretches a corner-tiled sprite to the window's size. Disabling it suppresses pressing but the default button effect also removes its colors. Ordinary images separate input from state presentation, but the earlier experiments did not stretch them and replacing a named background with an image crashed. The game ships a static button effect whose four states all draw the original colors. See [the interface](../game/interface.md) and [extension window research](../research/extension-windows.md).

## Behavior

A fixed background keeps its colors on hover, press and release. Left and right clicks on empty background do nothing and do not reach the map; dragging does not move the window. The wheel over empty background does not zoom the map. Lists handle their own scrolling first. Real buttons retain hover, press, disabled presentation and actions.

## How it fits together

Use a separate fixed-background sprite, retaining the existing nine-slice texture and border. Prefer the game's existing static effect; validate its corner-tile support in the game before relying on it. Button areas and non-movable extension windows select it; movable windows keep their original sprite. Retain the existing fixed-background input state initially. Research native wheel delivery and use its own input handling rather than a global desktop input interception. If the preferred effect cannot satisfy the behavior, document the concrete limitation before choosing another route.

## Parts

- **B1 — Fixed presentation.** Separate fixed and movable background sprites and select the fixed sprite for button areas and fixed extension windows.
- **B2 — Background input.** Verify left and right clicks and dragging, and complete native wheel consumption with child scrolling preserved.
- **B3 — Runtime evidence.** Check one and multiple button columns, different fixed-window dimensions, real controls, list scrolling and movable-window behavior; record the run and update the research and player-facing documentation.

Every part is done when its behavior is implemented, relevant repository checks pass, the runtime checks have evidence, and test games and temporary probes have been released or removed.
