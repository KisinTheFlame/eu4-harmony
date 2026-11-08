# Extension windows plan

Status: decided with the owner (2026-11-09); built, W1 to W4, and tried in the game (2026-11-09, [the run](../prototype-log/2026-11-09-extension-windows.md)); what was found not to be done is said where it belongs below. Terms are in [GLOSSARY.md](../../GLOSSARY.md) (Extension window, Pop-up, Game widget, Panel button strip, Entry button, Window). The decision is [ADR extension-windows-are-game-windows](../adr/2026-11-09-extension-windows-are-game-windows.md).

## Goal and scope

- **Goal**: a mod shows the player windows that look like the game's own: pop-ups with a title, a text and buttons, and windows of its own laid out in Lua, as rich as the papacy's or the Holy Roman Empire's panel, opened from an entry button on the panel button strip beside the game's own.
- **In scope**: pop-ups; extension windows laid out in Lua out of game widgets; their place, dragging, Esc and the game's one big panel at a time; their lifetime in a session; entry buttons; pop-ups for remote clients; both in the fake game.
- **Out of scope**: windows in the main menu; mods writing `windowType`s of their own in `.gui` files for these windows (fragments spliced into vanilla windows stay as they are, `eu4-harmony/ui/<window>/`); check boxes, text input, grids across and tabs as game widgets of their own (tabs are buttons and game widgets shown or hidden); the framework UI, which stays the framework's own.

## What the game does

From the game's files and code (RVAs into 1.37.5), read statically on 2026-11-08; none of it is verified in the running game yet.

- **The panel button strip** is the `floating_buttons` subwindow of `menubar` (`interface/menubar.gui`): eight fixed slots `f_button_1..8` with their backgrounds `menubar_floating_bg_1..8` and numbers `floating_number_1..8`, anchored to the lower right at y = -58, x = -360 and on, 45 apart (vanilla's backgrounds of slots 6 to 8 step 55). Every button is `GFX_menubar_floating_strip`, one frame per kind of panel; the background is `GFX_menubar_floating_bg`. The in-game idler keeps the subwindow at `idler+0x1198`, fetched again when the interface is read again (`0x81af10`, `0x82b470`). Verified.
- **Its layout is made every frame** (verified): the idler's frame calls `0x80c9f0`, which walks a fixed order of kinds (papacy, the Holy Roman Empire, …, the Celestial Empire) and gives each that applies to the player's country the next slot through `0x830700`, which shows it, sets its frame and number and writes the kind to `button+0x10`; the slots left over are hidden. Used slots pack from the right. There is no free slot, and none marks its panel open.
- **A click toggles its panel** (verified): one callback for all eight buttons (`0x80da30`) switches on the kind. Opening a panel first calls `0x818080`, which closes the views that cover the map, and `0x80c6e0`, which closes each big panel by its slot in the idler; no slot holds "the open panel". The close buttons of the papacy and the Holy Roman Empire carry `shortcut = "ESCAPE"`: Esc closes a window through its close button, and the game has no Esc of its own for the topmost window.
- **The look**: the papacy, the Holy Roman Empire and the Celestial Empire have backgrounds drawn for their own size; the backgrounds that stretch, the game's dialogs' (`SandboxFloaterBackground`), are near black or red; the blue of the game's panels is drawn for each panel's size. Extension windows and pop-ups take the blue of the religion view's background (`gfx/interface/country_religion_view_allreligion_bg_large.dds`, plain, with a thin gold edge), made a sprite that stretches, `GFX_eu4harmony_window_background`, by a `.gfx` among the framework files. The close button is `GFX_closebutton2`; text buttons are `button_type_1`, `_6` and `_8`; titles are `vic_29` or `vic_22`, texts `vic_18`. The Holy Roman Empire's and the Celestial Empire's windows are `moveable`; dragging is the window class's own.
- **The game's dialogs** (`interface/eu3dialog.gui`, verified): the callback dialog (`0x10a03f0`, built at about 200 places) shows a title, a text and two buttons, and its idler's list (`0x6ecae0`) drops a new one while another is open. Message pop-ups cascade in a stack of their own at `idler+0x1400`, but are made only from a message of one of the game's message types. Neither is used here.
- **Window definitions after startup** (verified): `0x14eb7d0(mgr, path)` reads an interface file again with the override flag `mgr+0x228` set, so a definition of the same name replaces the old one. `CreateWindow` (`0x14e9030`) builds a window from a definition and registers it with the GUI manager, which then draws it; Kill (`0x1547520`) and the manager's sweep destroy it.
- **Lists** (verified for the Holy Roman Empire's electors): each row is a small `windowType` built by `0x158ef10` and added to the list's box with `0xf2bed0`. **Flags**: a button whose sprite is a `maskedShieldType` such as `GFX_shield_thin`, given its country through `vtbl+0x218` (inferred). Frames, progress bars, flags and scrolling lists: [research/window-elements.md](../research/window-elements.md); the functions the windows, the big panels and the strip take: [research/extension-windows.md](../research/extension-windows.md).

## Behavior

### Pop-ups (W1)

- A pop-up is the game's message pop-up (`DefaultPopup` in `interface/eu3dialog.gui`, as a mission's success is told): the dialogs' near black background, a small header line, the title on the green band, a shield at each top corner, the text, and one or two buttons along the bottom.
- `game.popup { header = key, title = key, text = key, values = {…}, buttons = { key, key }, province = number, shields = { tag, tag } }` shows a pop-up and waits, in the handler's task, for its answer: the number of the mod's button pressed, `"goto"`, or `"closed"` when the pop-up was closed without one. Header, title, text and buttons are localisation keys of the mod's, the text with the game's formatting and icons; `values` fill their `$NAME$`s. The header may be left out. With no buttons of the mod's it has the game's own OK, answering 1; with a province it has the game's own Go to on the left, which turns the camera to that province and answers `"goto"`; two buttons at most, Go to among them. Its shields are the countries `shields` names, left and right, the player's where none is named (2026-11-09, asked by the owner; until then a pop-up had one to four buttons of the mod's own and no shields).
- Any number may be open at once. Each new one cascades over the last, as the game's message pop-ups do. A pop-up does not pause the game, does not take the map's input, and may be dragged. Esc and Return press its last button, the rightmost.
- A pop-up given an `id` may be closed by the mod from any handler, `game.popup.close(id)`; its answer is then `"closed"`.
- Pop-ups exist only in a session. One left unanswered when the session ends is closed.
- Remote clients: `popup.list` gives the open pop-ups (which mod, title, text, buttons, in the player language), `popup.answer {popup, button}` presses a button, as `question.list` and `question.answer` do; the `popup` event tells of a new one.

### Extension windows (W2, W3)

- `main.lua` returns, beside its handlers, `windows`: each window's name and layout. The layout is a tree of game widgets, laid out by columns and rows with spacing and alignment, and any game widget may instead stand at `at = {x, y}`; every game widget is given its size. The framework checks the layout when the extension loads and refuses the extension with a reason for a duplicate name, a game widget outside its window or a localisation key the mod lacks.
- The look: the blue of the game's panels with a thin gold edge, the title on a ribbon, red unless the layout says `ribbon = "blue"`, and the gold line of the game's mission pop-up along the bottom; with `crest = true`, the gold crest of that pop-up above the title, as the grander panels have, its scrolls hanging down the window's sides, so that its game widgets begin lower. A window with no title has no ribbon (2026-11-09, asked by the owner).
- Game widgets: the background, the title, the close button (with Esc), text (a localisation key, the game's formatting; text longer than its box wraps or is cut as the game's), an icon (a sprite of the game's or the mod's, with its frame), a button (the game's text button, in the widths its sprites come in, or an icon button), a progress bar (an icon of one of the game's `progressbartype` sprites), a country's flag (a button of a `maskedShieldType`), and a list (rows down, scrolling, each row a layout of its own). Buttons, icons, progress bars and flags may have a tooltip; a text may not: the game's text boxes show none ([research/window-elements.md](../research/window-elements.md) §5).
- A window is placed at the middle of the screen unless its layout anchors it to a corner or an edge with an offset; it may be dragged unless its layout says not; a place it was dragged to holds for the rest of the session.
- A window is one of the game's big panels unless its layout says not: opening it closes the game's big panels and the other extension windows, and opening one of the game's closes it. Pop-ups never are.
- From Lua: `game.window.open(name)`, `game.window.close(name)`; `game.window.<name>.<widget>` is set as `game.ui`'s elements are: `visible`, `enabled`, `text` (a text's words, as `text.get` gives them), `tooltip`, `frame`, `value` (a progress bar's), `country` (a flag's); a list's `rows` is set to every row at once, each a table from its game widgets' names to what they show. What is set while a window is closed shows when it opens. Not done: a text button's words change only with its layout, as no way the game sets a button's text was found.
- A click on a button in a window calls `on_button(game, "<window>.<widget>", click)`, with `click.row` the row's number for a button in a list's row. `on_window(game, name, open)` is called whenever a window opens or closes, whoever opened or closed it: the extension, its entry button, its close button, Esc, or another big panel.
- Windows exist only in a session: all close when it ends, and all of an extension's close at its extension reload. When the game builds its interface again (`reload`, a change of resolution), the framework builds the open windows again as they were.
- Remote clients reach extension windows only by input.

### Entry buttons (W4)

- A window's layout may have an entry button: its icon (a sprite of the mod's or the game's, with its frame), and its tooltip. It stands on the panel button strip left of the game's own buttons that show, 45 apart, in the game's round background, in the playset's order of mods and then the order of the mod's windows.
- A click opens its window, or closes it when open, as the game's buttons there do; it is not heard as `on_button`. Like the game's buttons, it is not marked while its window is open.
- `game.window.<name>.entry` sets `visible`, `tooltip` and `frame`. A hidden entry button leaves no gap. Not done: the number under it, as the game's show the imperial authority: no way to move a text box with the strip's buttons was found, so an entry button has none.
- Entry buttons are spliced into the strip as the game reads its file at startup: a window an extension reload gives an entry button shows it from the next start.
- With every vanilla button showing, the strip reaches the message log at 1280 pixels wide; entry buttons beyond it overlap the log, and nothing moves them.

### In the fake game

- The fake game holds the same windows and pop-ups: a layout is checked as in the game; a test reads whether a window is open, what each game widget shows and a list's rows, presses a window's buttons and an entry button, and answers a pop-up.

## How it fits together

- **The layouts** are the Lua host's (`lua/src/layout.rs`), so the game and the fake game read and check them alike: `main.lua`'s `windows` read, checked against the mod's texts, and laid out into each game widget's place when the extension loads. `lua/src/windows.rs` checks what `game.window` sets and puts a pop-up into words; the host (`eu4_harmony_lua::Host`) takes a `WindowChange`, a `Popup` and closing pop-ups, as it takes `game.ui`'s elements.
- **One module, `core/src/windows/`**, whose interface is what the Lua host and remote control need: declare an extension's windows, change them, show and close pop-ups, list and press pop-ups, and, out of it, what extensions hear (`take_seen`, `take_clicks`) and the pop-ups shown (`take_shown`, the `popup` event). What extensions want is kept there apart from the game's windows, so that it holds while a window is closed and across the game building it again. Inside it:
  - **the definitions** (`gui.rs`): a pure writing of the game's `windowType` out of a laid out window, in the game's look, and of a pop-up's, tested without the game;
  - **the game's side** (`live.rs`): at every safe frame of a game, after the extensions, the definitions served as two framework files (`crate::physfs::serve`) and read through the game's reread, under fresh names after an extension reload and a name of its own for each pop-up; a window built with CreateWindow, placed, shown and bound, its buttons subscribed as the army panel's are (`ui::Subscription`); what changed applied; a window no longer wanted killed, and one the game kills let go of (`ui`'s Kill observer tells it); the place a window was dragged to kept for the session.
  - **the game's big panels**: an observer before `0x80c6e0` closes the extension windows that are big panels, but when the framework itself calls it to open one.
  - **tooltips**: an observer after the map manager's tooltip, `0xb81870`, fills the tooltip of an element of an extension window or an entry button.
  - **entry buttons**: spliced into `floating_buttons` at startup (ADR inject-ui-at-runtime), placed after `0x80c9f0` has laid out the game's slots, from the count it showed, and bound again whenever the game made the strip anew.
- **The fake game** (`cli/src/fake.rs`) keeps the same state without drawing: what each window shows and its rows, the pop-ups open, and what tests press (`mod:press`, `mod:entry`, `mod:close`, `mod:answer_popup`).
- **The catalogue**: a `popup` group, `popup.list` and `popup.answer`, for extensions and remote clients; `game.window` and `game.popup` in the prelude, as `game.ui` is.
- Every RVA above is a dependency point, with its bytes; the offline checks pass on them.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and both clippy runs pass, and so do the offline checks against the game's files;
- what it delivers was tried in the game, with the run recorded in a file of its own in [prototype-log/](../prototype-log/), saying as plainly what was not verified;
- a change players or mod authors notice is in README.md and README.zh-CN.md, and `eu4-harmony skills ui` says how a mod uses it.

### W1 Pop-ups

The definitions written and read after startup, building and killing an instance, the cascade, waiting for the answer, closing at the session's end, `popup.list`, `popup.answer` and the `popup` event, the fake game. Unit tests: the pop-up's definition with the game's OK alone, and with Go to and a button of the mod's; a task waiting for its answer and for `"closed"`. Tried: three pop-ups open at once, answered in turn by a click, by Esc and by `popup.answer`, one left open across a return to the main menu.

### W2 Extension windows

The layout and its checks; the background, title, close button, text, icon and button; placing, anchoring and dragging; the game's big panels; on_button and on_window; building again after `reload`; closing at the session's end and at an extension reload; the fake game. Unit tests: layouts by columns, rows and `at`, and their refusals. Tried: a window opened over the papacy's panel and the other way round, dragged, closed by Esc, kept across `reload` of its file.

### W3 Progress bars, flags and lists

The research of icon frames, progress bars, flags and scrolling lists, written into docs/research; the three game widgets; rows and clicks in rows. Tried: a window like the Holy Roman Empire's, with a list of countries with their flags and a bar, rows changed while open.

### W4 Entry buttons

The splice into `floating_buttons`, the placing after the game's slots, the toggle, `entry` from Lua, the fake game. Tried: as a member of the Holy Roman Empire and as a country with no vanilla button there, two mods' entry buttons, one hidden.

## Order

In order, W1, W2, W3, W4, in one pull request.
