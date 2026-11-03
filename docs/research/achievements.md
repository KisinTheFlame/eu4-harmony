# Achievements (for `achievements`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-11-04, for the achievement rules of ADR 0034; nothing here was checked in the running game yet.

## The campaign's achievements

The global `[0x233d890]` (0xa8 bytes, made on first use by `0x96080`) holds, besides the lists main-menu.md describes:

- `+0` (u8): set by the multiplayer setup (`0x110ff46`), cleared by the single player setup (`0x1110573`) only when the check below passes. The daily update and the list builders want it clear.
- `+1` (u8): the save checksum of the save loaded matched. Set to 1 by a new game (`0x74a9ad`) and by a load (`0x74b8ec`), then by the save checksum check `0x97500` (saves.md).
- `+2` (u8): the game checksum is vanilla's. Written only by `0x97770(achievements, vanilla)` (`movzx edi, dl` at `0x97786`, stored at `0x977ae` or `0x977cc`); the two platform calls between do not take the value. Its callers pass `strcmp(application+0x258, "f2de20fd7cc5d735418b384c759d491d") == 0`, once a process, at startup: `0x205990` when the checksum is taken in place, or, when it is taken on a worker, from `0x211170` once `0x211100` finds it done. The global starts with `+0..3 = 00 01 01 00` (`0x96080`), so until then the game checksum counts as vanilla's.
- `+3` (u8): the developer command `set_achievements_ok` (`0xe86f1d`) sets it; the check then passes at once.

## The check

`0x96900(achievements, params, reasons, ignore ironman r9b, ignore player [rsp+0x28]) -> al` is the one place the game decides. `0x96810(achievements, reasons, ignore ironman r8b, ignore player r9b) -> al` fills `params` from the world (`[0x233fe78]`) and calls it, handing its `r8b` on as `r9b` (`0x96837`, `0x96898`). `params`: `+0` `world+0x7484`, `+4` `world+0x747c` (u32), `+8` `0x6eaa10(world+0x1e00)`, `+9` the world's ironman flag `world+0x24d0` (`0x9687c`), `+0xa` `world+0x7480` (set by a load, `0x74b8f0`), `+0xc` the player's tag, `+0x14` `world+0x23ac` (the major DLCs), `+0x18` the game settings `world+0x2140`. The meanings of `+0`, `+4` and `+8` are inferred (multiplayer, and another human in the session).

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

## Not settled

- What `world+0x7484`, `world+0x747c` and `0x6eaa10` are, beyond the inference above; which element `0x1438790` draws.
- Whether the game-state checksum of multiplayer (`0x743aa0`) reads anything the check reads: it reads none of `[0x233d890]`, `world+0x24d0` or `world+0x7480` directly; its callees were not followed.
