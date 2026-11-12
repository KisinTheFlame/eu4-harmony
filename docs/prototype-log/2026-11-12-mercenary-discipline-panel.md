# 2026-11-12 Mercenary discipline missing from the battle overview (tested by me)

The player clarified that their save should have substantial mercenary discipline but the battle panel did not seem to show it. The installed Chinese language mod calls discipline “训练度” and drill “操练度”; the preceding drill tests concerned a different mechanic. This run reproduces the battle overview's omission in the reduced six-mod environment and checks its explanation. The native value calculation is in [mercenary-discipline.md](../research/mercenary-discipline.md).

## Build and isolation

- Worktree: `claude/pensive-moser-8c5921`. At the player's request, the branch was rebased to freshly fetched `origin/master`, `b484480ab9a1d220e39240433d51615f03f45773`, from `7f694b8583fdccac63e568ddb2e7a79a7b8d29bc`. There were no branch-only commits. The 20 investigation files present then were restored from a uniquely named stash by its SHA; 19 were byte-identical and the address index merged with the new mainline. No files were lost, no conflicts remained, and the identified stash was dropped after verification.
- Claim `merc-discipline`, folder suffix `20261112-042058`, hidden and silent. It initially reached the main menu on the previous build (process 54800); that process was stopped before loading the save when the rebase was requested. The core was rebuilt from the new mainline with `cargo build --release --features eu4-harmony-core/dev-test-games`, which passed, and restarted as process 30216 before the test below.
- Original input and SHA-256 are recorded in [the first run](2026-11-12-mercenary-drill.md). The derivative copy was named `merc-drill-eligibility.eu4`; its six enabled mods were the same as that run. Automatic Operation 2743798482 and Formable Nations Conditions Optimization 3804356450 remained absent. The original save and the player's files were not changed.
- No six-mod file replaces `common/mercenary_companies`, the basic mercenary idea group, the relevant drilling defines or static drill modifiers. Searches of the six mods for mercenary discipline found country-idea entries in the Chinese supplementary mod, but its Portugal idea block supplies no discipline or mercenary discipline. The running country totals below are the direct evidence for this playset.

## What the save supplies

The unmodified source save records Portugal on 1582.7.1, completed mercenary ideas, active `ab_mercenary_discipline` in the Age of Reformation, economic hegemony at progress 100, active `doppelsoldner_reform`, and `hessian_militarization` power 98.016. Under the installed rules these contribute +5%, +5%, +10%, and about +9.8% mercenary discipline respectively, matching the runtime +29.8% below. The reform grants the mechanic; it does not itself give another flat discipline addition.

The active `mercenary_registration_act` and `hired_expeditions_act` do not give discipline in the installed definitions. The Free Swiss Guard's company definition gives ordinary discipline +5%. It is company ID 681, army save ID 27254, full army ID `117055038685238`, 13 infantry regiments, led by 29889 in province 192. The saved leader has no personality field.

## Runtime observations

After loading the derivative copy, before advancing time:

```bash
eu4-harmony game --test-game merc-discipline country modifiers POR discipline,mercenary_discipline
```

returned ordinary discipline **0.05** and mercenary discipline **0.298**. `advisor.list` later confirmed the hired military advisor was **commandant**, skill 1, whose base definition gives +5% ordinary discipline; the battle tooltip lists the advisor as that +5% source.

To open a battle panel without altering the player's game, only the isolated copy was changed:

```bash
eu4-harmony game --test-game merc-discipline console run "revolt 192"
eu4-harmony game --test-game merc-discipline time advance 1 --speed 1 --timeout 60
```

The revolt command confirmed rebels in Dijon. The advance reached 1582.7.2. The resulting battle was opened through its map marker after dismissing the revolt notice. This is a generated observation battle, not a replay of a battle the player identified; no casualty difference was inferred from its random rolls.

The Portuguese side's discipline field showed **105%** despite the separate +29.8% mercenary discipline and the participating Swiss company's +5%. Hovering the field showed the same 105.0 discipline, the advisor's +5% source, and this explanation:

> 这是战斗中主导军队的训练度，有一定参考性。每个单位在战斗中仍会使用自己的数值。

The base English `DISC_ADD_DESC` is:

> This is the Discipline of the combat leader and should be seen as representative. Every unit still uses its own values during combat.

The battle overview and tooltip were captured; the screenshot crop is retained as local evidence outside the repository. Hovering a formation square also showed that regiment's unit type, pips, morale, strength, flanking range and unit manpower; the sampled square was a regular artillery regiment and did not display a separate discipline value. That observation is not a claim about every formation tooltip.

The known inputs predict **139.8%** for the Swiss mercenary regiments before any additional applicable regiment-local/leader discipline: 100% + 5% ordinary national + 29.8% national mercenary + 5% company. This is an expected value from queried national totals, checked company content and the native additive formula, not a directly sampled native regiment return. The source has no Swiss leader personality; existing remote capabilities do not expose the complete local modifier set or the native function's final return.

## Interpretation and limits

The battle overview's omission was reproduced and agrees with the game's implementation: it obtains the leading country's ordinary discipline and the side leader's modifier. It neither averages every regiment nor adds mercenary discipline or a company's discipline. The native regiment-damage path separately adds those terms and multiplies that result into damage; [the research](../research/mercenary-discipline.md) records the exact functions and the loss-writing path.

This establishes a display scope difference, rather than evidence that the mercenary discipline modifier disappeared from combat. It does not establish a controlled measured damage difference. Neither the complete original eight-mod environment nor historic Workshop versions were reconstructed. No product change, additional setting, permanent diagnostic interface, installation, commit or publication was made.

## Missing-mod checks

Bounded local searches found neither missing item by ID/name in the two Steam Workshop trees, `D:/Workspace`, Downloads, Desktop, or the player's existing mod registrations. A renamed or elsewhere-stored copy is not excluded. No mod was subscribed to, downloaded or newly enabled.

Direct reads of both original Workshop pages returned HTTP 429; a description mirror returned HTTP 403. Public search results linked the automation mod to missionary/development/building automation, and the [formable-nation description](https://catalogue.smods.ru/archives/489326) to administrative-technology/culture requirements. The [automation translation's description](https://catalogue.smods.ru/archives/310706) identifies the original item. These search-index descriptions do not establish the complete rules or absence of discipline overrides and were not used to exclude either missing mod. The save was not sent to an external service.

## Cleanup

`cargo xtask game release --owner merc-discipline` reported `released`; both owned processes 54800 and 30216 ended through the claim tool with exit code `0x1`, and the game did not crash. The claim's only retained files are the tool's seven-day logs. `game status` lists none of the investigation's owners. Temporary save copies, full screenshots, rebase hash inventory and disassembly output were removed after retaining the report's crop. The original save's SHA-256 remained `5ce7b9cc6e5b211d8e9e8fb35bfedae934cbe68a3f5cf9a9339138399030640a`.
