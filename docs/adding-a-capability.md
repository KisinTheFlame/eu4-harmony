# Adding a capability

How a capability reaches both extensions and remote clients, layer by layer, and what is done before it is merged. Terms are in [GLOSSARY.md](../GLOSSARY.md); the rules that shape a capability are in [ADR one-capability-catalogue](adr/one-capability-catalogue.md), [ADR one-command-layer-in-written-form](adr/one-command-layer-in-written-form.md) and, for the panels' actions, [player-actions.md](player-actions.md).

## Before writing code

- **Find it in the game first.** What the panel's button checks, the reasons its tooltip gives, the command it posts and that command's written form, the fields the panel reads. The command log of a developer build ([dev-tools.md](dev-tools.md)) shows a button's command in its written form when the player presses it in the game; `cargo xtask disasm` finds the code. Write what was found into `docs/research/`, addresses as RVAs into the 1.37.5 baseline, before or with the code.
- **Name it after what the player sees and does**, in the group of the panel it is on: `estate.grant_privilege`, not after the command (`grant_privilege`) or a field. What the player chooses among is named by its content key.

## The layers

1. **The catalogue** (`remote/src/<group>.rs`): the capability in its group's `group!` list, with its arguments, result and descriptions; a new group also goes into `groups!` in `remote/src/lib.rs`. `game` for one extensions and remote control both have, `remote_game` or `remote_anywhere` for one only remote control has. A result is a struct deriving `Shaped`, with each field described; the shared `Choice`, `Amount` and `Acceptance` are in `remote/src/choices.rs`. An argument of a new kind adds a `Kind` there. Lua's calls, the editor's types, `wiki` and `describe` all come from this declaration, and the command line and the fake game of `eu4-harmony test` take it with nothing more.
2. **Refusals** (`checks/src/<group>.rs`): each refusal the framework makes itself, its status and words, with a unit test; the game's own reasons are passed through as they are, one a line.
3. **The capability layer** (`core/src/game/<group>.rs`): one function per capability, taking the `Caller` first and returning the framework's own types. A read takes the caller's view (`game::view(caller)`) and leaves out what the player's view does not show. An action checks as the panel does, with the game's reasons, writes its command with a small writer of its own (a pure function, unit-tested) and posts it with `commands::post`. Its addresses are dependency points in the file's `points()`, added to `crate::points()`; a field offset is checked against an instruction that uses it (`.uses`).
4. **The catalogue's group** (`core/src/catalogue/<group>.rs`), which remote control's engine and the Lua host both run: the group's `layer!` slice and its `run`, which turns the catalogue's request into the layer's call and the layer's answer into the catalogue's result; the group is one line in the `groups!` list in `core/src/catalogue/mod.rs`, which makes the `Game` trait and `dispatch`. The engine's tests (`core/src/remote/tests.rs`) drive the group through `core/src/catalogue/fake.rs`, a fake layer of fixed answers.
5. **Events** a part adds: a `Heard` variant in `core/src/game/mod.rs`, its `Event` and line in `EVENTS` (`remote/src/events.rs`), its handler in `HANDLERS` (`remote/src/lua.rs`), the two places that turn `Heard` into a handler's call (`core/src/lua/mod.rs`) and into an event (`happened` in `core/src/remote/mod.rs`, the engine's), and whether anyone listens (`game::events::listened`), so that it is not worked out for nobody.

New groups and new arguments' kinds are lines added to lists other sessions add to as well; when merging master, keep both sides' lines.

## Trying it in the game

- Claim the game, install and start it as [.claude/skills/game-test](../.claude/skills/game-test/SKILL.md) says, and drive it with `eu4-harmony game <group> <capability> …`; release it when done.
- Try each capability as the player would use it, and each of its refusals, comparing with what the game's panel shows; read back what changed. A capability only extensions have is tried from a test mod.
- Record the run in [prototype-log.md](prototype-log.md), one dated section, saying as plainly what was not tried.

## Before it is merged

- `cargo fmt --all`, the build, the tests and both clippies (AGENTS.md), and `cargo xtask offline-check` with the game installed.
- README.md and README.zh-CN.md, as AGENTS.md says of the front page: a line where players or mod authors notice the change, such as a feature or a setting; the full list of capabilities is `eu4-harmony wiki`'s.
- The plan's part says what it added.
