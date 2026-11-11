# 2026-11-11 A core's hover outline lost with the drawing thread (tested by me, not fixed)

The owner noticed that hovering a core's flag in the province view no longer outlines that country's cores in red on the map, as the game does.

- **The setup**: a hidden test game with `dev-test-games` and `dev-test-switches`, `observer-1700.1.1` as the Ottomans; the camera on Constantinople (151), a click at the screen's middle to open its view, the cursor moved onto the Ottoman core's flag by remote control, screenshots after 1.5 to 2 s. The map's borders pulse, so a single screenshot can miss an outline; eight in a row were taken where it mattered.
- **With the drawing thread**: the core's tooltip shown, no red outline, in all eight screenshots.
- **Without it** (`game-paths = ["drawing-thread"]`): every Ottoman core outlined in red.
- **The tooltips' update where the game has it**, after the drawing (the framework's write at `0x815949` taken out, its call before the frame's preparation too): the outline back, thinner-looking in the one screenshot. With the update right after the frame's preparation, before it is handed on: no outline.
- **Traced** (`dev-call-trace`, two seconds of hover, with the drawing thread and without): `0x12f0b40` and `0x6ec450` not called; the pdxmap's borders `+0x1270` built once, at the load. What the code read is in [research](../research/map-drawing-reads.md), "The borders, and a core's highlight".
- **Not verified**: where the outline is set and what takes it away; whether the drawing thread's drawing a prepared frame more than once is the cause.
- **Cleanup**: the test game released; its logs kept in its claim's `kept`.
