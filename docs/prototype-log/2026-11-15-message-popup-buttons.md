# 2026-11-15 Native message popup action buttons (tested by me)

A user supplied a screenshot of a diplomatic-annexation result with its body, shields and message-settings icon visible, but no action buttons. This run attempts the same native message and checks button visibility separately from the captured pixels. No cause or repair has been established.

## Build and setup

- Checkout `claude/peaceful-curie-70ea5b`, commit `5d26a838`, matching the locally recorded `origin/master`. The app's attempt to fetch and merge master failed with `git ls-remote timed out`; this is not a claim about the latest remote master.
- Built with `cargo build --release --features eu4-harmony-core/dev-test-games`.
- Hidden test game `loving-wiles-message`, with its own user folder, core and pipe. No installation into the game folder, and no control of another game.
- EU4 1.37.5, copied playset: Graphical Map Improvements (`253263609`), Emoji Universalis (`2830362508`), Chinese Language Mod for 1.37 (`2976470733`), Chinese Language Supplementary Mod for 1.37 (`1999055990`), the modifier-sorting mod (`3253723536`), Harmony (`3837775887`), Regional Combat (`3837793510`) and Convenience Tools (`3838535796`). The copied settings select 3840x2160 and UI scale 1.500. None of those mod folders contains `interface/eu3dialog.gui`; this narrow file check does not rule out other interface changes.
- Started a new Saxony campaign. The reporter's exact framework version, playset, settings and save were not supplied, so this setup is not an exact copy of the failing environment.

## Identifying the message and measuring its buttons

The screenshot's lines match `ANNEXACCEPT` in the game's `localisation/messages_l_english.yml`. The native `DefaultPopup` definition in `interface/eu3dialog.gui` has `AgreeButton`, `DeclineButton`, `CenterOK` and a separate `Settings` button. The [existing static research](../research/message-pop-up-actions.md) describes two valid modes: a single centered confirmation, or the left Go to button and right confirmation. All three action buttons need not be visible at once.

A temporary Python probe used only `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)` and `ReadProcessMemory` against this claim's PID. It resolved the executable's image base through Toolhelp, traversed the map manager's native dialog list, selected the existing message-popup class, and read each popup's key, open/done state and named button tables. Layouts came from the existing [questions and offers](../research/questions-and-offers.md), [notice window](../research/notice-window.md) and [interface bindings](../research/interface-bindings.md) notes; the button-table lookup was also checked in the game's disassembly before use.

The assertion requires at least one action button to have `visible = 1` and `dead = 0` in an open, unfinished message of the expected key. If that message is absent, the probe reports `NOT_REPRODUCED` and exits 2. The probe cannot establish that visible objects were drawn correctly: screenshots are a separate check. A passing control is not a reproduction of the reported missing buttons.

## Attempts and control

- `console.run` with `integrate FIN` was rejected because Finland did not exist as a country in the new campaign.
- Instant `integrate THU` annexed Thuringia but did not create `ANNEXACCEPT`. This console command is not a demonstrated route to that popup.
- An alliance proposal to Brandenburg was refused by the diplomacy panel's checks. A proposal to Bohemia reached the AI and was declined, creating `MILALLREJECT`.
- For that open `MILALLREJECT`, the probe found `DeclineButton` and `AgreeButton` visible, not dead and not suppressed; `CenterOK` hidden; and `Settings` visible. The captured screenshot also showed both bottom action buttons, with Chinese Go to and confirmation labels. This validates the probe against one normal native diplomatic result; it does not validate `ANNEXACCEPT`.
- Returned to the main menu and started Saxony again. Thuringia is its junior partner, so `diplomacy.annex` was unavailable. Set the date to `1500.11.11` to meet the union's duration requirement, used `add_opinion THU SAX` twice to meet the opinion requirement, and started the normal `diplomacy.integrate THU` action, which returned `answer = done`.
- The first `time.advance 3650` stopped at `1501.1.1` because `institution_events.200` awaited an answer. It did not complete ten years. Subsequent advances use speed 5, answer pending events through `question.answer`, and inspect native messages after each bounded advance.

## Integration result

The seventeenth bounded advance reached `1504.7.1` and produced an open, unfinished `ANNEXACCEPT` after the normal integration completed. The probe returned:

```text
key = ANNEXACCEPT, open = 1, done = 0
DeclineButton: visible = 0, dead = 0, suppressed = 0
AgreeButton:   visible = 0, dead = 0, suppressed = 0
CenterOK:      visible = 1, dead = 0, suppressed = 0
Settings:      visible = 1, dead = 0, suppressed = 0
PASS: expected native message has a visible action button (pixels require a screenshot)
```

This is the valid single-button mode. The first frame was covered by the campaign introduction and other messages, so it did not verify this message's pixels. The introduction was dismissed with Escape, and the integration's event `610` was answered through `question.answer`. Normal Return-key confirmations dismissed the remaining native messages one at a time; a screenshot was taken before each confirmation and the target's continued presence checked afterward. The forty-third confirmation dismissed `ANNEXACCEPT`, leaving its unobstructed preceding frame as evidence. In that frame, the popup's body, shields, settings icon and centered confirmation button all display normally. The confirmation label is the Chinese translation of `ANNEXACCEPT_BTN1`. The target accepted Return and closed normally.

The local evidence is in the ignored `target/diagnostics/message-popup-buttons-2026-11-15/`: the full frame, a crop of its dialog, the alliance control and a compact `annexaccept-state.json`. The dialog crop was also delivered in the conversation. These are inspection records, not product assets or automated regression fixtures.

## Limits and cleanup

The reported missing action buttons were not reproduced. One actual `ANNEXACCEPT` was checked in its single-button mode, together with the two-button `MILALLREJECT` control. The target's two-button mode, the reporter's original save and settings, older framework builds, and brief failures between captures were not tested. This run therefore establishes neither a root cause nor that the report is resolved. The passing object assertion is not a demonstrated failing regression test. Further diagnosis needs the failing environment's framework version and troubleshooting collection, ideally with a save and steps that reach the affected message.

No product code was changed, and no dependencies were installed. The release build completed; the later changes are this run note and its documentation-index entry, so the Rust test suite was not rerun for them.

`xtask game release --owner loving-wiles-message` reported `released`, a controlled stop with exit code `0x1`, and no crash. `game-ends.tsv` confirms that code, and the retained emergency log contains only its owned-log header. `xtask game status` no longer lists this owner. The claim's framework and game logs remain in its normal seven-day `kept/` folder; the temporary user folder and copied core were removed by release. The one-use probe, advance/capture drivers, generated Python bytecode and obscured initial screenshot were removed from the checkout. Only the useful local evidence and the run's documentation were retained. No game-folder installation, player-file change or mod registration was made.
