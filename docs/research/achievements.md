# Achievements (for `achievements`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-11-04, for the achievement rules of ADR achievement-rules-can-be-relaxed; what was checked in the running game is in [Achievement rules, and the save checksum of an in-game load](../prototype-log/2026-11-04-achievement-rules-load-checksum.md) (2026-11-04).

## The campaign's achievements

The global `[0x233d890]` (0xa8 bytes, made on first use by `0x96080`) holds, besides the lists main-menu.md describes:

- `+0` (u8): set by the multiplayer setup (`0x110ff46`), cleared by the single player setup (`0x1110573`) only when the check below passes. The daily update and the list builders want it clear.
- `+1` (u8): the save checksum of the save loaded matched. Set to 1 by a new game (`0x74a9ad`) and by a load (`0x74b8ec`), then by the save checksum check `0x97500` (saves.md).
- `+2` (u8): the game checksum is vanilla's. Written only by `0x97770(achievements, vanilla)` (`movzx edi, dl` at `0x97786`, stored at `0x977ae` or `0x977cc`); the two platform calls between do not take the value. Its callers pass `strcmp(application+0x258, "f2de20fd7cc5d735418b384c759d491d") == 0`, once a process, at startup: `0x205990` when the checksum is taken in place, or, when it is taken on a worker, from `0x211170` once `0x211100` finds it done. The global starts with `+0..3 = 00 01 01 00` (`0x96080`), so until then the game checksum counts as vanilla's.
- `+3` (u8): the developer command `set_achievements_ok` (`0xe86f1d`) sets it; the check then passes at once.

## The check

`0x96900(achievements, params, reasons, ignore ironman r9b, ignore player [rsp+0x28]) -> al` decides the session's overall achievement eligibility. This is separate from each achievement's script conditions below. `0x96810(achievements, reasons, ignore ironman r8b, ignore player r9b) -> al` fills `params` from the world (`[0x233fe78]`) and calls it, handing its `r8b` on as `r9b` (`0x96837`, `0x96898`). `params`: `+0` `world+0x7484`, `+4` `world+0x747c` (u32), `+8` `0x6eaa10(world+0x1e00)`, `+9` the world's ironman flag `world+0x24d0` (`0x9687c`), `+0xa` `world+0x7480` (set by a load, `0x74b8f0`), `+0xc` the player's tag, `+0x14` `world+0x23ac` (the major DLCs), `+0x18` the game settings `world+0x2140`. The meanings of `+0`, `+4` and `+8` are inferred (multiplayer, and another human in the session).

In order:

1. `+3` set: passes.
2. `(+0 && +8) || (!+0 && +4)`: fails with `ACHIEVEMENTS_DISABLED_MULTIPLAYER` alone.
3. Unless the player is ignored, after a load, with a tag kept at `achievements+0x80` that is not the player's: fails, `ACHIEVEMENTS_DISABLED_WRONG_PLAYER`.
4. It needs `+1`, the major DLCs, `+2` and `ignore ironman || ironman` (`0x9696d`..`0x96994`); the reasons are `_MODIFIED_SAVE`, `MAJOR_DLCS_NOT_IN_SAVE`, `_MODIFIED_GAME` and `_NOT_IRONMAN`.
5. ANDed with `0x747970(game settings)`: the difficulty at least normal (`_WRONG_DIFFICULTY`), no custom nations beyond one (`_CUSTOM_NATION_SETUP`), no custom development (`_CUSTOM_DEVELOPMENT`), lucky nations off (`_WRONG_LUCK`), end-game tags blocked (`_ENDGAME_TAGS_BLOCKED`).

There is no rule about the game console, the start date or mods: the game console is kept from achievements only by being refused in ironman (game-console.md), and a start date or a country is each achievement's own condition, checked as the list below is built.

## Who asks

`0x96810` keeps nothing; it is asked whenever the answer is wanted: by the day's update (`0x75c6f8` → `0x96130`, which unlocks through the platform), by the saver for `achievement_ok` and for which list it writes (`0x751ada`), by the builders of the list of achievements in progress (`0x96440`, `0x962d0`), by the platform's resync (`0x6eb9f0`), the achievements window (`0x1435160`), the setup's start tooltip (`0x1123b50`), the ironman load warning (`0x111f930`) and an element of the in-game interface every frame (`0x1438790`, from `0x81575d`). Two ask `0x96900` with values from a save's header: the continue box (`0x10d8eb8`, for ironman saves only) and the load list (`0x10eeb16`, ignoring ironman already).

- **The list of achievements in progress** (`achievements+0x20`) is built only as a session begins: from the in-game interface's start (`0x81d8a9` → `0x967b0`/`0x96440`) or from a save's `achievement` entry (`0x962d0`). It stays empty when the check fails then, so a change to the check's inputs in the middle of a session takes effect only at the next.
- **The single player setup's own copy**: `0x13b6cc0` asks `0x96810` ignoring ironman (`0x13b6d95`), then wants its own ironman checkbox (`setup panel+0x280`) as well: at `0x13b6db7` before it says `ACHIEVEMENTS_CAN_BE_EARNED`, and at `0x13b6eec` before it lists the reasons after `ACHIEVEMENTS_CANNOT_BE_EARNED`.
- **The salt** of the save checksum (`0x5ce788`): `I_am_such_a_cheater` when `!+3 && (!+2 || !+1)`. The save checksum check verifies with the other salt (`+1` was just set to 1), so a save written with this one fails it when loaded, and every save after it is salted so again: a game played once with the game checksum not vanilla's, or once loaded from a modified save, keeps achievements off.
- **A save loaded in a session** failed the save checksum check in vanilla, whatever it held: the in-game load sums the file otherwise than the save was summed when written (saves.md). The framework repairs it, so that a game that can earn achievements keeps them across an in-game load.
- **"Loading ironman in modded game"** (`0x5d053b`) reads `+2` too (main-menu.md).

The game checksum's text in the main menu and the ironman checkbox's state do not read `+2`: a game checksum that is not vanilla's turns achievements off, not ironman.

## Each achievement's script conditions

Read on 2026-11-13 after an eligible non-ironman new game saved `achievement_ok=yes` with no `achievement` list. All 373 definitions in the installed baseline's `common/achievements.txt` independently require `ironman = yes` in `possible`. Relaxing the overall check alone does not admit any of them. The repaired build and its limits were checked in [Non-ironman achievement tracking](../prototype-log/2026-11-13-non-ironman-achievement-tracking.md).

- **The ironman condition** is the leaf function `0xcc6db0`, outside the exception directory. It loads the world from `[0x233fe78]`, returns true when the developer-command bypass at `world+0x74db` is set, otherwise reads the real ironman byte at `world+0x24d0` (`0xcc6dc3`) and compares it with the script's expected byte at `trigger+0x68` (`0xcc6dca`). The token record at `0x218a328` is `0x2e18`, `ironman`; the condition factory's case at `0xb8f1ff` calls constructor `0xbb4060`, which installs the virtual table `0x1cf2ae8`, whose evaluation method at `+0x30` is this leaf.
- **The new-start builder** `0x96440(achievements, objects out, IDs out)` returns for an invalid player tag, a failed overall check or `achievements+0` set. It walks definition pointers at `achievements+0x8..+0x10`. A definition's platform object is at `+0x70`; its virtual method `+0x28` says whether it was already unlocked and should be skipped. Its `possible` block at `+0x78` has child pointers at `block+0x8`, with the count at `+0x14`. Every child's virtual method `+0x30`, called at `0x965bb`, must pass. The definition pointer and its i16 ID at `+0x108` then enter the pending object and ID vectors (`achievements+0x20` and `+0x38`).
- **Platform readiness comes before building.** `0x967b0` calls `0x979d0` while `achievements+0xa0` is set and does not build if that fails. The latter reaches the service through `[0x242bf50] -> application+0x348 -> platform+0x30`, asks its virtual `+0x38` whether it is ready, maps definition names to platform objects with virtual `+0x48`, and clears `+0xa0`. The definition reader `0x96ed0` has the same platform mapping. A service not ready can therefore also leave the pending list empty. In the failing test game it was ready, with all 373 definitions mapped.
- **The day's completion check** `0x96130(achievements, scope)` asks `0xb98200` to evaluate the `happened` block at `definition+0x88` (`0x961d7`), also through the child conditions' virtual `+0x30`. A pass calls the platform object's virtual `+0x8` (`0x961f6`), records the ID at `achievements+0x50`, and removes it from both pending vectors. The observed platform object's unlocked query `0x24f3b0` just reads `object+0x58`; its unlock method `0x164b7f0` sets that byte once and tail-calls service virtual `+0x60`. The test intercepted this last boundary, so the framework's daily completion and saved result were observed without submitting an achievement to Steam.
- **Loading restores IDs, not initial conditions.** `0x962d0` walks the save's `achievement` IDs and restores their definitions without evaluating `possible` again. A save with no list cannot safely have it reconstructed from current state: start date, country and conditions such as not having a certain army or navy at the start are historical facts. The repair does not guess them or change an old save's eligibility.

The framework now brackets the new-start builder and the daily completion check with a thread-local achievement-evaluation scope. With achievements outside ironman enabled, only that scope makes the ironman condition read true. The jump at `0xcc6dc3` changes the value fed to the game's own comparison; it preserves the developer bypass, `ironman = no` and negation, and never writes the real world flag. Other script conditions and scripts evaluated outside that scope retain the game's behavior. An in-game load preserves the IDs a corrected new game saved. Achievement-window condition tooltips were not checked visually.

## Not settled

- What `world+0x7484`, `world+0x747c` and `0x6eaa10` are, beyond the inference above; which element `0x1438790` draws.
- Whether the game-state checksum of multiplayer (`0x743aa0`) reads anything the check reads: it reads none of `[0x233d890]`, `world+0x24d0` or `world+0x7480` directly; its callees were not followed.
