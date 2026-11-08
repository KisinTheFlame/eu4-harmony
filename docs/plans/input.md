# Input plan

Status: decided (2026-11-04); I1 to I4 built the same day, I1 to I3 tried in the game ([Input](../prototype-log/2026-11-04-input.md)). Terms are in [GLOSSARY.md](../../GLOSSARY.md) (Input, Screen position). The decision and its reasons are in [ADR remote-clients-give-the-players-input](../adr/2026-11-04-remote-clients-give-the-players-input.md). How the game reads its input is in [research/input.md](../research/input.md); how the framework UI takes its own is in `core/src/framework_ui/input.rs`.

## Goal and scope

- **Goal**: a remote client gives the game mouse and keyboard input at screen positions, as the player does, for what has no capability of its own, such as an AI that looks at `screenshot` and clicks.
- **In scope**: the `input` group of remote control: clicks, moving the mouse, dragging, the wheel, keys and key combinations, typing text, holding a button or a key; reading where the game takes the cursor to be; the order between input and the requests after it; removing `tools/game-input.ps1` and `tools/screenshot.ps1`.
- **Out of scope**: input for extensions (ADR remote-clients-give-the-players-input); the input method (composing text from keys); waiting in the framework for what the input does; multiplayer.

## Behavior

### Screen positions

- A position is `x`, `y` in pixels of the frame the game draws, from its top left corner: the frame `screenshot` answers `frame_width` and `frame_height` of.
- Every capability with a position takes an optional `scale`, as `screenshot` does (1 to 100, percent of the frame): the position is then one in a screenshot taken at that scale, and the framework scales it up. So a client passes what it sees in the image it got.
- A position outside the frame presented last is `invalid_argument`. Before the first frame, `not_ready`. An input whose frame has changed size by the time it is handed to the game (the resolution changed) is dropped and written to the framework log; what it was handed over already and would have let go of later, a modifier or a button held down, is let go of in its place, one frame each.

### The capabilities

| Capability | Arguments | What the game gets |
| --- | --- | --- |
| `input.click` | `x`, `y`; optional `button` (`left`, `right` or `middle`; `left` when absent), `count` (1 to 3; 2 is a double click), `modifiers`, `scale` | the cursor moved there, then the button pressed and let go `count` times |
| `input.move` | `x`, `y`; optional `modifiers`, `scale` | the cursor moved there, for a tooltip or a hover |
| `input.drag` | `from_x`, `from_y`, `to_x`, `to_y`; optional `button`, `modifiers`, `scale` | the button pressed at the first position, the cursor moved to the second over a few frames, the button let go there |
| `input.scroll` | `x`, `y`, `direction` (`up`, `down`, `left` or `right`); optional `amount` (notches, 1 to 100; 1 when absent), `modifiers`, `scale` | the cursor moved there, then the wheel turned that many notches |
| `input.key` | `keys` (a key or a combination, such as `f1`, `escape`, `ctrl+s`); optional `repeat` (1 to 100) | the keys pressed in order and let go in reverse, `repeat` times |
| `input.type` | `text` (any Unicode) | the text, as the input method gives the game text it composed |
| `input.mouse_down`, `input.mouse_up` | `x`, `y`; optional `button`, `scale` | the cursor moved there, then the button pressed, or let go |
| `input.key_down`, `input.key_up` | `key` | the key pressed, or let go |
| `input.get` | | `x`, `y`, the screen position the game takes the cursor to be at, the input's or Windows' own (null off the frame); `buttons` and `keys` held by remote control |

- **Key names** are one string, keys joined with `+`, in any case. Letters, digits and the keys' own names: `escape`, `enter`, `space`, `tab`, `backspace`, `delete`, `insert`, `home`, `end`, `page_up`, `page_down`, `left`, `right`, `up`, `down`, `f1` to `f12`, `minus`, `equals`, the key left of 1 as `grave`, `comma`, `period`, `slash`, `semicolon`, `apostrophe`, `left_bracket`, `right_bracket`, `backslash`, `numpad_plus`, `numpad_minus`; modifiers `ctrl`, `shift`, `alt`, `win`. `plus` (or `+`, as in `ctrl++`) is shift and `equals`, as a keyboard types `+`, the game's key for a faster speed (found in the game, research/input.md). The names other tools give the same keys are taken too, so a client used to one of them writes what it knows: `control`, `super` and `meta`; `return`, `esc`, `pageup`, `Page_Down`; `arrowleft`; `` ` ``. An unknown name refuses the request as it is read (`bad_request`, as for any argument that is not of its kind), and the command line refuses it before it sends anything; `key_down` and `key_up` given more than one key are `invalid_argument`. The keys are the keyboard's places, as a US layout names them, whatever layout the player uses.
- **Modifiers** on a mouse capability are written as keys (`shift`, `ctrl+shift`) and held for that action only: pressed before it, let go after it.
- **Held** buttons and keys stay held across requests, until their `_up`, or until a session begins or the game restarts, which let go of everything remote control held. A key held is pressed once; the game gets no repeats as from a real keyboard held down, so one `backspace` deletes one character. `input.key` with `repeat` repeats.
- **The cursor** stays where the last input left it, as the player's mouse does, until the player's own mouse moves over the game, from when the game follows that. Left at an edge of the frame, it scrolls the map, as the player's would; the descriptions say so.
- **`input.type`** gives the game text past any input method, so Chinese and every other script can be typed without one; it reaches an edit box through the Unicode module as composed text does. Keys from `input.key` reach no input method. Composing text with one is tried with a real keyboard.
- **Where it goes**: through the framework UI first, as the player's input does. The framework UI takes what falls on its windows and, while a modal window is open, everything; the game gets the rest. So what a screenshot shows can be clicked, the framework's own windows included.

### When it answers, and in what order

- A request is checked (positions, keys, ranges) and then answered at once, null: the input is taken, not yet handed to the game. How long to wait for what it does, such as a panel opening, is the client's.
- Inputs are handed to the game in the order they were taken, one after another: one that spans frames (a drag, a double click) is done before the next begins.
- A request of any capability that comes after an input, from any client, runs only once every input taken before it has been handed to the game and the frame that handled it has run. So a `screenshot` after a click shows at least the frame that took the click.
- Input needs no game: it works in the main menu and in a game alike. While no frame runs (a save loading, the game minimized), it waits for the next frame.
- An extension asking is refused `unavailable`, as for `screenshot`.

### The development tools

`tools/game-input.ps1` and `tools/screenshot.ps1` went: input and screenshots are `eu4-harmony game input …` and `eu4-harmony game screenshot`. `tools/perf-run.ps1`, which measured vanilla too and so sent its own clicks and keys and compared its own screen regions, went later with the rest of `tools/` ([dev-commands.md](dev-commands.md)); what one run costs is now the scenario `perf` ([scenarios.md](scenarios.md)), on the player build only. Earlier runs in [prototype-log/](../prototype-log/) keep naming the tools they used.

## How it fits together

One module of the core's capability layer, `game::input`, takes every input and hands it to the game:

```rust
/// Checks an action, its positions in a screenshot at `scale` percent, against the frame presented last, and queues it.
pub fn take(caller: Caller, action: Action, scale: u32) -> GResult<Taken>;
/// Whether every input up to `taken` has been handed to the game and a frame has run since.
pub fn handled(taken: Taken) -> bool;
/// Where the game takes the cursor to be, and what remote control holds.
pub fn cursor(caller: Caller) -> GResult<Cursor>;
/// Just before the main thread presents a frame: hands the game the next frame's steps.
pub fn presenting(size: (u32, u32));
```

Inside, an `Action` becomes the steps of each frame (move, press, let go, a wheel notch, text) by a pure function with unit tests of its own (`game::input::plan`), which also keeps the buttons and keys held; `game::input::window` turns a step into the messages Windows sends and hands them to the framework's window procedure. The engine keeps the order of the requests: a request queued after an input runs once `handled` says its input has run; its tests put a fake layer behind it. The key names, their aliases and the arguments' checks are in the shared crates (`remote/`, `checks/`), so the command line refuses a wrong key before it sends anything.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`, and so do the offline checks against `eu4.exe`;
- what it delivers was tried in the game, with the run recorded in a file of its own in [prototype-log/](../prototype-log/), saying as plainly what was not verified;
- docs/research holds what was found in the game's code, every new place in it is a dependency point, and README.md and README.zh-CN.md say what changed for players.

### I1 Where input reaches the game

Find where the framework hands the game input, so that it goes through the framework UI first and then as the player's does: the window procedure's messages, SDL's queue or the current idler's input slot; whether the game or SDL drops input, or the game behaves otherwise, while another program is in front or the game is minimized, and what the framework does about it without changing what the player sees; where the game and SDL read the cursor's position and the buttons and keys held (hover, tooltips, scrolling at an edge, a shift held during a click); how text reaches an edit box past the input method; why an injected Backspace deleted more than one character. Findings in `docs/research/input.md`; `Receiver` behind the module, with a click from a test switch of the developer build tried in the game with another program in front.

Built (2026-11-04). The game reads Windows' own cursor while its window has the focus and SDL's otherwise, once a frame, through `0x14e5e50`, and sets the focus flag again at every button pressed; no game code reads key state from Windows or SDL (research/input.md). So the steps go to the framework's window procedure as Windows' messages (`game::input::window`, `framework_ui::input::inject`), at each frame the main thread presents, after the safe point there, and `0x14e5e50` is replaced to answer the virtual cursor while input holds it (one dependency point). `game::input` keeps the queue; `game::input::plan` turns an action into its frames, a step a frame (unit tests); the engine keeps the requests after an input waiting until `handled` (unit tests with the fake). The framework UI's modal window now also holds back `WM_UNICHAR`, `WM_MOUSEHWHEEL` and the middle button's double click. No seam of its own stands where the steps reach the game, and no test switch gave the first click: there is one way to the game, the window procedure, and the click was tried through remote control itself, with another program in front. Why `tools/game-input.ps1`'s Backspace deleted more than one character was not looked into; given a frame each, one deletes one.

### I2 Mouse

`input.click`, `input.move`, `input.drag`, `input.scroll`, `input.mouse_down`, `input.mouse_up` and `input.get`, with `scale` and the order between input and the requests after it. Tried in the game, with the game behind another program: a button of the game's interface and of the framework UI clicked, an army selected with a click and ordered with a right click, a box drawn round armies, a tooltip shown, the map zoomed with the wheel, a slider dragged; a screenshot right after a click shows its frame.

Built (2026-11-04), with I3: the capabilities in `remote/src/input.rs` (three argument kinds of their own: `button`, `direction`, `keys`), their checks in `checks/src/input.rs`, the catalogue's group in `core/src/catalogue/input.rs`. Tried as said, with another program in front but for the first two clicks; a drag selected the army in its box. Not tried: the middle button, `mouse_down` and `mouse_up`, the wheel's left and right, the game minimized, a modal window holding input back, an input dropped for a frame of another size.

### I3 Keyboard and text

`input.key`, `input.type`, `input.key_down`, `input.key_up`, the key names and their aliases, and `modifiers` on the mouse capabilities. Tried in the game: the pause key and the speed keys, a panel's hotkey, a shift-click, the game console opened with `grave` and a command typed, Chinese typed into a name, one `backspace` deleting one character, a key held across two requests and let go, held keys let go when a session begins.

Built (2026-11-04). Tried as said, but for the panel's hotkey, which was not pressed. The first run found `plus` naming the `=` key, which the game does not speed up on; `plus` is now shift and `equals`, and the keypad's plus and minus have names of their own, tried in the second run. A key a keyboard types a character with sends the character too, in the same frame, as Windows' own handling of a key press does (the US layout's, unless ctrl, alt or win is held). Not tried: alt and win.

### I4 The tools

`tools/game-input.ps1` and `tools/screenshot.ps1` removed, `tools/perf-run.ps1` sending its own input and taking its own regions of the screen, `tools/README.md` and README.md's tools row changed. Tried: a vanilla run and a framework run of `perf-run.ps1`.

Built (2026-11-04). `perf-run.ps1` clicks, presses Space and saves its crops itself (`Hand`, `ScreenRegion.Save`), and takes the introduction's reference with `-IntroReference`. Not tried: no run of `perf-run.ps1` was made after the change, vanilla or with the framework.
