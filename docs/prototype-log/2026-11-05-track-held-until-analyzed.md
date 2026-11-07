# 2026-11-05 A track held until its file is analyzed (tested by me)

The owner asked for a track to wait for its analysis rather than play as it is until then. The player build of the change, hidden without sound, the player's volumes and playset, the main menu and a new game as France; four starts within one claim.

- **Held**: the main theme "plays", was "-10.9 LUFS, analyzed in 0.58 s", and then "held 441 ms for its analysis" before its first frame was mixed; the next starts held it 557 ms for an analysis of 0.72 s. The hold is counted in mixer calls, which SDL's `dummy` driver makes somewhat slower than real time. In the games, `music/037_theageofdiscovery.ogg` and `music/031_kingscourt.ogg` (files of the playset's mods) and `music/march_on_granada.ogg` (a DLC's) were each held 371 to 557 ms and analyzed in 0.49 to 0.74 s.
- **The stream goes on as it was**: a temporary log of the seconds each voice mixed gave the main theme 270.1 s, its whole 271.2 s once, with the hold at its start.
- **A gap the game makes**: with this playset, the next track came about 6 min after the main theme began, some 90 s after it ended, where earlier runs gave about 4 min 34 s. A start with loudness normalization off, which holds nothing, gave the same 6 min 2 s, so the gap is the game's, not the hold's; why the game waits longer with this playset was not looked into.
- **Not verified**: anything heard; a hold that runs out at 2 s, or an analysis that fails, which play as they are (by reading the code only).
- **Cleanup**: releasing the game put back the player's files, among them the framework's settings with the line the last start added and the autosaves the games wrote, and the game folder's framework files; the temporary log was taken out of the code.
