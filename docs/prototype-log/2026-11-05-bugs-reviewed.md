# 2026-11-05 The bugs reviewed: reproduced, then fixed (tested by me)

A review of the code found bugs across the framework; those simple to show in the game were tried on master (8735166) first, then on the branch with every fix. Both runs within a claim, started hidden, new games as France at 1444.11.11 driven through `eu4-harmony game …`, the player's playset; the event of the Hundred Years' War answered first each time.

- **Before, on master**:
  - `time advance 365 --speed 1`, then `menu return` 4 s later: the return answered `main_menu`, and the advance never answered until its client gave up at 90 s.
  - After that return, `session get` sent 2 s into a `menu start FRA`: `session_changed`, "it was sent in session 3, which ended before it ran; this is session 4".
  - `menu start FRA` from within a game: `session_ended` with `reason` `return`.
  - `peace terms REB`: `internal`, "invalid read 0x0+0".
  - After the console's `favors PRO 50`, `trade_favors_for_trust PRO --favors 10 --trust 5`, the window's own step ("花费10人情来交换5信任度。"), was accepted, and `options PRO` then refused it with "你已经有拥有普罗旺斯的完全信任。": full trust from one step.
  - `sell_province BUR --province 183`, Paris, the capital: refused only with "巴黎距离勃艮第太远，无法将其核心化。". `--province 87`, England's Calais: "我们并不是加莱的所有者", so the game's check does look at the owner.
  - `sell_ships PRO`, sent with no ships and a price of -1 as the panel makes it: not refused by the game's check; Provence `declined`. The game went on.
- **After, on the branch**:
  - The advance answered as the return's did, `stopped` `main_menu`, "the game returned to the main menu".
  - The `session get` sent during the start waited and answered session 2.
  - `menu start FRA` from a game: `reason` `new_game`.
  - `peace terms REB`: `rejected`, "the player is not at war with REB".
  - Favors for trust: 15 favors refused (the window spends 10 at a time), `--trust 5000` refused (10 favors buy 5), 60 favors refused (Provence owed 50, its trust 50); 10 favors with no trust given accepted, and `options` then refused it only for the diplomat, not for full trust.
  - The panel's limits: Paris refused as the capital, Calais as not the player's, `sell_ships` `unavailable`, a gift of 0, a loan of 100000 ducats with 71 in the treasury and war subsidies for 5 refused `invalid_argument`; a gift of 10 `done`.
  - The console's `tag CAS`: "Now Playing - 卡斯蒂利亚", then `session_ended` `reason` `country` and session 4.
  - The framework settings with a byte order mark before `language="english"` (Windows displays Simplified Chinese): the report said "language: english (chosen in the framework settings)", and the file was not written (its MD5 the same before and after).
- **Found in the run**: the claim's first release put every file back and said the game was free, but its record stayed, held, with the watch keeping it: two checkouts waiting in the queue on master's xtask had written the record again without the mark that says who puts it back. Fixed on the branch and released again with it; the queue went on about 5 minutes late.
- **Not tried in the game**: an ironman exit save that fails; `console.run` with a command that loads a save (it needs the console's developer lock off); an input dropped for another frame size; the Lua time limit and memory limit (unit tests only); `fabricate_claim`, `claim_states`, `trade_favors_for_core`, `transfer_trade_power`, `offer_condottieri` with the new checks (no spy network, no favors or no core to try them on); the framework UI's glyph page filling and a touchpad's wheel; the claim's new marks, beyond one claim, its watch and its release; a scenario run stopped by Ctrl-C or an error.
- **Cleanup**: each release put the player's files back, every file as its backup had it, and the game folder's framework files.
