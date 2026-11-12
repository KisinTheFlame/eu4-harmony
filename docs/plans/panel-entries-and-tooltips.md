# Panel entries and tooltips

**Status:** Decided on 2026-11-12; implementation in progress. Native behavior still to be checked is called out below. Builds on [extension windows](extension-windows.md) and [interface bindings](../research/interface-bindings.md).

## Goal and scope

Give every visual element created or managed by the framework one readable and writable tooltip, and let a mod declare a panel entry with its own image and an optional number. The framework owns presentation, native resources and lifetime. Layout-only rows and columns have no tooltip target. Arbitrary vanilla elements are outside this interface.

## Behavior

A tooltip is initialized by its declaration. Assignment replaces that content; clearing leaves it empty. Declaration and assignment accept the same literal-text or localisation-key description, including named values. Localisation retains the game's colors and line breaks. Only the element under the pointer supplies its tooltip: there is no inheritance. Disabled elements still show it; hidden, destroyed and unloaded elements do not. Changing the content while the pointer stays still updates the native tooltip. Rebinding, list rebuilding, session changes and localisation changes are handled together by the framework.

A panel entry opens or closes its window by default. Entries retain a stable order; hiding one closes the gap and showing it restores that order. Visibility and enabled state may change. A disabled entry cannot invoke its action but retains its tooltip. The icon alone owns the action and tooltip. An optional number is a separate text element below it, following the game's floating-number structure. No value draws nothing; zero draws zero. Integer, decimal and percent formatting and compact large numbers belong to the framework. Text length does not move neighboring entries or change the tooltip.

Images may be declared by a mod-relative file path or by an existing sprite and frame. The framework checks the path, existence, format and dimensions, registers the native resource and supplies native hover and pressed states. Errors are reported instead of displaying a missing-resource tile.

## How it fits together

- A shared text description in the Lua host preserves literal text or a localisation key and values. Window and hosted-element state own that description, including the initial value, independently of native objects.
- Bound native elements and framework-drawn elements adapt the same tooltip content and lifetime rules to the game's presentation. Native timing, positioning and screen constraints remain the game's.
- The existing window-entry module owns entry resources, arrangement, number formatting and input. The native number's positioning and hit behavior must be established from the game before changing it.
- Test-game registration must load native mod resources as well as Lua, without writing through a junction into the player's mod directory.

## Parts

- **T1:** Shared text descriptions and one readable/writable tooltip state, with declaration, replacement, clearing, localisation and rebinding checks.
- **T2:** Native-window, supported vanilla-host and framework-drawn adapters; disabled and hidden behavior, stationary-pointer updates and cleanup.
- **E1:** Direct image declarations, checked native registration and resource lifetime.
- **E2:** Stable entry arrangement, enabled state and a separate formatted number with the native hit behavior established.
- **M1:** Migrate Convenience Tools, Regional Combat source and examples to the current interface; repair private test-game registration.
- **V1:** Formatting, release build, tests, both clippy builds, offline checks and a bounded hidden-game run, with evidence recorded in its own prototype log.

Every part is done when its implementation, developer documentation and meaningful regression checks agree, and native claims have actual native evidence. The user's current hands-on game is not changed or controlled. Steam creation and uploads remain paused.

## Internal migration record

The published Regional Combat 0.4.0 item (manifest interface version 0.3) must remain runnable during this change. Its source of evidence is the read-only Workshop item 3837793510. It sets a finished tooltip string on `game.ui.toggle` before the army panel binds, uses a GUI sprite reference and retains separate left- and right-click behavior. Keep these conversions inside the framework and mark their removal points internally. Migrate the local source without committing or pushing that repository. Before the public beta, remove these temporary conversions after the source and published item have migrated. Player-facing documentation and release notes describe only the current behavior.
