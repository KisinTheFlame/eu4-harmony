# 2026-11-11 No frame drawn while the player is at war (tested by me)

A player's game stopped drawing as they accepted an ally's call to arms, and closed when asked. What held the frames back, and the change that lets them through ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The war overview after the drawing").

## The player's files

The troubleshooting files the About page saved, of 0.1.2 (api 0.3) on 1.37.5 with seven Workshop mods, a new game from 1453 as Lotharingia. The framework log of the session that stopped (22:02 to 22:36), its drawing line at 22:36:17: "0.0 frames a second, … 0 drawn by the main thread; the main thread waited 33 ms for the lock (33 at frames, 0 at crossings, 0 at days' ends) … the frames held back longest by 0x16d7f30 888x 9552 ms". The ten seconds before drew 39 frames a second. The game quit at 22:36:25 through its own quit ("the game quits; the drawing thread draws no more"). The second session in the files loaded a save, returned to the main menu after a minute, and drew normally.

## The run

Test game `ally-war-freeze`, from this branch's core built with `dev-test-games`; the player's playset and settings of this computer, hidden. `observer-1700.1.1.eu4` copied in as `allywar-1700` and loaded as the Ottomans (`menu.load`), paused. Tunis, the Ottomans' ally, made the target of a war by a file run from the console: `SON = { declare_war_with_cb = { who = TUN casus_belli = cb_insult } }` (`cb_conquest` declared no war, and Morocco no longer existed). Tunis's call to arms came at once (`offer.list`: "召集盟友！"); its accept button clicked by `input.click` at 2102, 1267 (the first click after a load only hovered it).

- **Before the change** (master as of `10b16708`): the popup shown and hovered, paused, 95 to 136 frames a second. Accepted: 0 frames a second from that frame on, "held back longest by 0x16d7f30 1640x 9744 ms, 0x16d80f0 5x 15 ms, 0x16d4e00 1645x 0 ms"; a screenshot timed out. Again in a second game: 1,272 holds by `0x16d7f30`, 7,121 ms, in the ten seconds after the click.
- **The stack**: a probe built for the run and not kept logged, for a crossing into `0x16d7f30` made after the frame was prepared in the same application frame, the main thread's stack scanned for addresses in the game's code: every one through `0x81635d` in the in-game frame, after its drawing, under the application's frame (`0x14c2818`). A first version of the probe, which scanned past the stack's top, crashed the game as it started (`0xc0000005`, 22:52:23); bounded by the thread's stack base it ran. The tail after the drawing is the war overview's update (research note).
- **After the change**: the same steps, accepted at 23:12:14: 138.6 and 133.1 frames a second in the next two ten-second stretches, nothing held back by the renderer. Then speed 5 from 1700.1.2, the player at war, to 1709.4.21 (about six minutes): 0 drawn by the main thread, 2 to 22 frames held back in ten seconds, by message popups (`0x8c1430`), the monthly refresh (`0x81e280`) and the minimap (`0x10b6c50`); no crash, no error in the log; the game ended by the release with 1. The war ended in a peace on the way.
- **Frames a second fell** from 107 to 40–65 over those years, the late frames counted by the drawing (208 to 271 in ten seconds), a frame's drawing 11 to 12.5 ms against 3 to 4 at the start, paused too. The last screenshot had a pile of windows no one answered in the middle of the screen (an event, combat and peace windows); that the pile is what made the drawing dearer is inferred, not measured.

## Not verified

- A game shown to the player, and the player's own playset and save: the run used this computer's playset and an Ottoman game of 1700.
- A war the player declared, or was declared on, rather than joined: the same tail draws the war overview for any war of the player's, which was read, not run.
- Whether any call into the renderer's table with the lock held leaves something the next update has to finish: argued from what the button's text drawing does (research note); one stretch of nine game years at speed 5 is all that was run.
- The cleanup: the test game released, its logs kept in the claim's `kept\` for seven days; the probe taken out of the code.
