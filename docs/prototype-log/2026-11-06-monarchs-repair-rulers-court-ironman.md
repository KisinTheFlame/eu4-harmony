# 2026-11-06 The monarchs' repair through rulers changing, the court and ironman (tested by me)

What the run before left untried of the monarchs' repair (`core/src/repairs/monarchs.rs`), on a player build of master (aec0044), within a claim, the game started hidden, driven by `eu4-harmony game` from a script.

- **A long session**: a new game as Castile, 1444.11.11 to 1456.9.8 at speed 5 in 310 s, each question answered with its first option (35). The return after it deleted 1,094 persons the histories left, where the sessions of a year or less before it deleted 202 and 214: the records made in play went with the session, and their persons after it. No crash, nothing logged about a person not deleted, none kept for the game still having it.
- **The court**: F1 at 1456.9.8 showed the country view's court, its king, heir and consort drawn, in a screenshot.
- **Ironman**: the newest of the player's saves that have a backup beside them, `Castilla-5` (Castile, 1470.3.13), loaded as an ironman session; 60 days, then an in-game load of `England-1`, which wrote the ironman exit save first (`ExitSave, Ended(Load), Load`), 30 days and a return. Then a load of `England-1`, an in-game load of `英格兰1444_11_11` and a return. The resets after them deleted 3,633, 2,838 and 2,838 persons; no crash.
- **Not verified**: that Castile's own king died in those twelve years (the game's logs say nothing of rulers, and the screenshot is too small to read his name); a history shown anywhere but the court.
- **Cleanup**: the release put every file back ("every file is as the backup has it"), the ironman save rewritten by the run among them. The script, its screenshot and the claim's folder were deleted.
