# 2026-11-09 Pop-ups as the game's message pop-up, windows with a ribbon and gold (tested by me)

Whether pop-ups look and act as the game's message pop-ups, and extension windows wear a title ribbon, a gold line and, when asked, the gold crest, as the owner asked after [the first run](2026-11-09-extension-windows.md) ([plans/extension-windows.md](../plans/extension-windows.md)).

## Setup

- **Build**: this branch, `--release --features eu4-harmony-core/dev-test-games`, in a test game of its own claim, started with `--mod` and the test mod Windows Demo from the first run, changed: three pop-ups of three kinds (a header and the game's OK alone; Go to province 134 with France's and England's shields and one button of the mod's; a header and two buttons of the mod's), the reforms panel with `crest = true`, the tracker with `ribbon = "blue"`, and a window with no title. Its two tests pass under `eu4-harmony test`.
- **Game**: a new game as Austria through `menu.start`; input through `eu4-harmony game input`, looked at through screenshots.

## What came out

- **Pop-ups.** Each was the game's message pop-up: the near black background, the header line (Our Steward reports), the title on the green band, a shield at each corner, the text, and the buttons along the bottom. The first two pop-ups showed Austria's shields, the player's; the second France's on the left and England's on the right. The game's own words stood on its buttons in the game's language: 前往 for Go to, 确定 for OK.
- **Answers.** No on the third answered 2. Go to on the second turned the camera from province 112 to 134, as `camera.get` read before and after, and answered `"goto"`. `popup.list` gave the first with its header and the game's 确定 as its one button; `popup.answer --button 1` answered 1.
- **Windows.** The tracker's title stood on a blue ribbon, the reforms panel's on a red one with the gold crest above it, its scrolls hanging down the sides beside the title and its game widgets beginning below them; each window had the gold line along its bottom. The window with no title had no ribbon, its close button and its text.
- **The log.** The game's `error.log` had no line about the framework's definitions; the framework's log no panic; the release reported no crash.

## Not verified

- A pop-up whose shields name a country that does not exist (refused in code: the shield is left as it is and the log says so).
- Shields in observe mode with no country in view, where none is shown (from the code).
- A crest on a window narrower than 500 (scaled down in code) or wider (kept at its size, centered).
