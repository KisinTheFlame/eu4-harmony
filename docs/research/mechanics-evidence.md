# Capability and mechanics evidence

Historical observations on Windows EU4 1.37.5, 2026-10-29 through 2026-11-15. Later tests used isolated hidden games; early tests backed up and restored player files. These results are not a claim that every path has been retested on the current build. Routine build lists, repeated scenario transcripts and successful UI screenshots have been omitted.

## Commands and extensions

On 2026-11-03, 26,429 executed commands of 70 types were written as text, read into new objects and written again identically. Posting text commands recruited in an owned province and proposed an alliance; invalid provinces, misspelled keys and unknown commands were refused. Executing the session queue at a safe point initially hung when a daily update drew a frame and reentered remote control. Executing the requested command alone removed the demonstrated reentrancy. The native construction/execution path is in [commands](../game/scripts/commands.md).

On 2026-11-05, Lua obtained 156 answers across all 38 extension groups, each matching its shape. Player-view reads hid another country's treasury and fog-covered armies while the full view read them. Tasks resumed after diplomatic/network answers; a failed handler did not stop later handlers. Declared file and HTTPS requests worked; traversal and unlisted hosts were refused, HTTP 404 remained a response and unreachable hosts returned failure. Real-game redirects, large bodies and timeouts were not exercised.

Reload on 2026-11-04 carried one task through `save_data` into changed code/localisation. Syntax errors kept the old extension running for another 120 days. Valid code restarted an extension stopped for exceeding its time limit; stopped code supplied no save data. These observations supplement the current `wiki lua` and shipped skills.

## Saves, sessions and achievements

Extension data survived plain text, zipped text and binary ironman saves. Plain-text placement after AI was not read; placement before AI fixed that path. Placement after the checksum also failed. A new-game save with extension data passed the game's checksum, and the next game-written save loaded with the data intact. Cloud extension data, multiplayer and vanilla loading were untested.

The 2026-11-13 non-ironman regression exposed a gap earlier eligibility checks missed: Castile saved `achievement_ok=yes` but no tracked IDs despite 373 mapped definitions and a ready platform. After repair it tracked 178 IDs; in-game loading preserved exactly 178; disabling the setting yielded zero eligible/tracked achievements. Separately, ID 7 completed after one day, the saved pending count fell to 177 and exactly one platform call was intercepted. No Steam unlock was submitted. Existing saves without a list were not reconstructed; achievement-window tooltips were not visually checked. [Achievement tracking](../game/countries/achievements.md) explains the starting-condition distinction.

The 2026-11-13 whole scenario initially refused nine save-based scenarios for descriptor-path mismatches. All nine later passed with identical core binaries and private registration aliases; this was not one uninterrupted passing run.

## Native mechanics

France's loan amount rose from 128 to 129 ducats immediately after developing Paris. A 300-ducat peace term deducted 300 from England but gave France 258.777 and Orléans 41.223, consistent with participation-based sharing; separate-peace recipient selection remained unresolved.

Castile's 28-development Galicia area cost 28.000 as a full-core state, 7.000 after abandonment and 14.000 after becoming a state without completing cores. Live national capacity changed without date advancement; cached-field refresh timing was not measured. A mixed trade-company area was skipped. Costs crossing the discount floor, negative scripted costs, total-conversion maps and arbitrary side effects were not tested. See [area and national governing cost](../game/provinces/area-governing-cost.md).

Religion tests exercised deity/cult selection, church-aspect replacement, fervor, harmonization, icons/metropolitans, piety, papal actions, golden bulls, holy orders, secondary religions and Sikh teachings with resource/readback checks. Deity modifiers appeared after recount, not immediately. Blessings and native religious reforms were refused rather than successfully exercised. Many windows were never opened; holy-order/teaching candidates were not independently compared with native buttons.

## Mercenary names, drill and discipline

A Portugal save dated 1582.7.1 was tested with six of eight recorded mods; two were unavailable. Its source bytes were preserved. Thirteen Free Swiss Guard regiments drilled to saved value 0.833 after 31 days; the tooltip showed 0.83 and runtime `allow_mercenary_drill` was 1.0. The battle overview showed 105% despite national mercenary discipline 29.8% and company discipline 5%. Native logic distinguishes the overview from the regiment; 139.8% was a prediction from known inputs, not a sampled return or casualty comparison. The eight-mod environment remains untested. See [drill](../game/military/drill.md) and [discipline](../game/military/discipline.md).

On 2026-11-15, raw army memory reproduced a damaged grand-company name while company-list text stayed intact. Character-boundary color removal preserved that 12-byte name and removed only real color codes from a 20-byte control, exercising inline and game-allocated storage. Existing damaged names, save/reload and all other callers were not tested. [The byte evidence](mercenary-company-names.md) supports [native text formatting](../game/interface/text-formatting.md).

## Reformation-center references

Native events created Protestant A in province 50, let it target 51, created Reformed B there and removed A. Before repair, province 51 retained A's pointer and progress 287; the captured-state checker failed. After repair the independently selected target was 1759: creation cleared the old reciprocal pointers/progress while preserving B. Seven days later B targeted 59 with consistent reciprocal pointers and progress 1981. The manager creation path ran; the trading-policy path and visual conversion colors did not. This establishes a reference omission and bounded repair, not the reported crash's allocator history or exact cause. See [center lifecycle](../game/provinces/reformation-centers.md) and [captured-crash limits](reformation-center-crash.md).
