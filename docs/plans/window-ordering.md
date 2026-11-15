# Native interface ordering

Status: implementation ready for review, 2026-11-14; remaining in-game acceptance is listed below. Q1 through Q14 are approved. The private ordering process, native placement, input-depth protection, lifecycle reconciliation and host binding are implemented. Traversal and repeated live refresh have matched in both drawing modes. The keyboard course passed with threaded drawing and, after dismissing the separate original startup screen, with native drawing. Recipient tracing identified that screen's close button as the earlier extra Escape consumer. Build, workspace tests, both clippy configurations and offline checks passed. The [critical pointer run](../prototype-log/2026-11-14-window-ordering-pointer-acceptance.md) passed activation with exactly one action, keyboard precedence after activation, army-picker coverage and input without click-through, and button-area restoration. Same-batch input and live scrollbar movement remain unverified and are reported as such in the pull request. Window scope is recorded in [GLOSSARY.md](../../GLOSSARY.md); no ADR has been adopted.

## Goal and scope

Provide reusable ordering rules for the native interface: elements added to host windows, extension windows, pop-ups, and their relationships with vanilla windows. The army attach picker is the first acceptance case, not the complete design scope. Preserve unrelated vanilla-to-vanilla ordering and keep the existing extension interface unchanged. The concrete native integration and host adaptation coverage must be established from evidence.

The framework UI uses a separate drawing system and is outside this design's current scope.

## Native evidence

The native interface uses ordered scene trees, restores definition order during construction, and draws each subtree before its next sibling. Showing an existing window does not itself bring it forward. A window's own front/back methods move its scene container within the same parent without changing internal child membership or manager registration. The separate named-child operation can duplicate ordinary children across internal lists and is not used for general placement ([placement study](../research/window-placement.md)). Input selection uses draw order with additional clipping, eligibility and modal rules.

See [the game's interface](../game/interface.md#what-draws-above-what) and [the static study](../research/window-ordering.md). The [threaded keyboard course](../prototype-log/2026-11-14-window-ordering-keyboard.md) verified nontrivial native placement and three single actions; [native-drawing tracing](../prototype-log/2026-11-14-window-ordering-startup-screen.md) identifies the original startup screen's earlier key handling and verifies the prepared three-key course. The critical pointer and army-picker acceptance passed; multiple events within one native batch remain unverified.

## Accepted behavior

### Q1: Native interface scope

Accepted in round 1: establish framework-maintained rules across the native interface areas above. Do not limit the design to a special case for the army attach picker. Keep the separately drawn framework UI outside this round of design.

### Q2: Cover the button area for the first acceptance case

Accepted in round 1: when the army attach picker opens, it may cover the button area. Where they overlap, the picker receives the applicable pointer input and the covered added controls or background must not intercept it. Closing the picker restores access to the button area in its original position.

The user identified automatic movement of the button area to the right as another valid product solution, and chose coverage first. Automatic displacement, panel growth and space negotiation are deferred, not ruled out permanently. The game already provides position setters; an automatic avoidance policy has not been established by the research and would require separate behavior decisions.

The independent-window and modal rules are specified separately below.

### Q3: Framework authority and a simple ordering policy

Accepted in round 2: extensions express their needs and the framework determines order. Arbitrary numeric z-index values and repeated native reorder calls are not part of the proposed extension contract.

The user proposed a simpler policy: windows that appear later come forward. Q5 through Q7 subsequently define participation, appearance and activation for that policy. There is no accepted fixed hierarchy of window purposes. Q10 retains existing ownership relationships and Q13 keeps the entire existing extension interface unchanged.

### Q4: Preserve existing modal behavior

Accepted in round 2: make drawing and pointer handling agree where windows overlap, while preserving existing modal behavior. Do not introduce a new rule that opening an ordinary pop-up blocks the rest of the interface or map. A new modal capability requires a separate explicit decision.

### Q5: Order windows within their scope

Accepted in round 3: independently opened windows participate in ordering within their window scope. Decorative and permanent attached content does not compete as a separate window. A temporary child picker stays within its host's scope; independent top-level windows share a broader scope. Bringing a child forward does not put it above other independent windows.

The term window scope is recorded in the glossary. This is a logical relationship, not a commitment to native reparenting or to a new public ownership interface.

### Q6: Appearance means becoming visible

Accepted in round 3: a hidden/closed-to-visible transition counts as a new appearance, including reopening. Construction while hidden, repeated requests to show an already-visible window, and content or list refreshes do not change order. When A opens, then B opens, B is in front; closing and reopening A puts A in front within that scope.

### Q7: Pointer activation also brings a window forward

Accepted in round 3: clicking an exposed, interactive part of an already-visible participating window brings it forward within its scope. Hover alone does not. Existing modal restrictions remain in force. Q8 preserves the clicked control's action.

### Q8: Preserve the activating click's action

Accepted in round 4: an ordinary click on an exposed control both brings its participating window forward and retains the control's native action. Do not consume the first click solely to activate the window, replay the event, or send it to newly exposed underlying controls. This specifies the outcome, not the native event stage at which order changes.

### Q9: Reconstruction preserves logical order

Accepted in round 4: technical reconstruction alone is not a new appearance. When the same logical window is restored as visible during a supported rebuild, preserve its order relative to the other surviving logical windows. This does not require reopening windows that existing reload/session rules close; a genuine close followed by a new open still follows Q6.

### Q10: Keep existing ownership relationships

Accepted in round 4: use existing logical/native containment and keep independently declared extension windows and pop-ups independent. Do not add automatic ownership by the triggering window or a new parent-window declaration in this effort. Closing a source window continues not to close an independent pop-up. Existing big-panel exclusion and lifecycle behavior remain in force.

### Q11: Shortcuts follow order after native input priority

Accepted in round 4: preserve native modal and focused-input precedence, then give the foremost eligible window the first opportunity to handle a shared shortcut. Once handled, the event must not act on a window behind it. A front window that does not handle that shortcut does not introduce a new global input block.

### Q12: Intervene only in framework-related ordering

Accepted in round 5: maintain the order of framework-added content and the native windows that directly interact with it, without globally rewriting vanilla-to-vanilla activation behavior. This includes native host children such as the army attach picker and relevant native panels beside extension windows. Automatic coverage of every unrelated vanilla window is outside this effort.

### Q13: Existing extensions require no changes

Accepted in round 5: existing extensions receive the ordering behavior without changing their Lua declarations or calls. Preserve current window and pop-up operations; do not add a required ordering field, numeric depth, ownership declaration or explicit bring-forward operation in this effort. Ordering is maintained internally by the framework.

### Q14: Preserve vanilla order when activation would conflict

Accepted in round 6: preserve native vanilla-to-vanilla order as a constraint. Activation changes framework-related ordering only to the extent compatible with that constraint and existing modal rules. If the front-to-back order is vanilla A, extension X, vanilla B, clicking B moves X behind B, yielding A, B, X. The framework does not swap A and B; it respects a change made independently by the game itself. Activating an original window does not promise to make it foremost among all original windows.

### Consequences of the accepted rules

- Q5 and Q7 confine a child's activation to its own window scope. This does not promote its ancestors in their scopes or detach it to compete with independent windows.
- Q5, Q7 and Q10 place independent nonmodal extension windows and pop-ups in the top-level scope. A pop-up has no permanent priority solely because it is a pop-up: a later opening or an allowed click can put an ordinary window ahead of it. Existing modal priority remains protected by Q4 and Q11.
- Q10 retains existing closing/hiding propagation rather than adding a new lifecycle policy. Scope membership does not automatically make the triggering window an owner.

## Design tree

- Q1, accepted: native-interface scope.
  - Q3, accepted: framework authority over ordering.
    - Q5, accepted: participating windows and window scopes.
    - Q6, accepted: visibility transitions change order; refreshes do not.
    - Q7, accepted: pointer activation brings an existing window forward.
      - Q8, accepted: preserve the activating click's action exactly once.
    - Q9, accepted: technical reconstruction preserves logical order.
    - Q10, accepted: retain existing ownership without a new parent declaration.
    - Q11, accepted: shared shortcuts follow order after native input priority.
  - Q12, accepted: preserve unrelated vanilla-to-vanilla ordering.
  - Q13, accepted: existing extension declarations and calls remain unchanged.
  - Q14, accepted: vanilla-to-vanilla order constrains framework activation effects.
- Q2, accepted: cover the button area first; automatic displacement deferred.
  - Q4, accepted: preserve existing modal behavior.
- Source investigation complete: existing synchronization and lifecycle seams are identified; missing native guarantees are explicit O1 evidence obligations below.
- Implementation parts and verification cases below were approved after the completed interview.
- The design frontier is closed; native evidence obligations are engineering work in O1.

## Existing lifecycle constraints

The source investigation for round 4 found:

- Extension-window declarations have no parent or host field (`lua/src/layout.rs`, `Window`). Pop-ups retain their extension and number, with no source-window relationship (`lua/src/windows/wanted.rs`, `OpenPopup`). Current ordinary extension windows and pop-ups are constructed as top-level native windows.
- Closing an extension window preserves its logical property, row and position state but destroys its native instance. Reopening rebuilds it. A closed pop-up is removed; a later pop-up receives a new number.
- Redeclaring an extension closes its own logical windows and pop-ups. The global definition generation also causes all ordinary extension windows' native instances to be removed and rebuilt according to their surviving logical open state (`core/src/windows/lifecycle.rs`, `reconcile`). A native rebuild therefore does not reliably identify a new logical appearance.
- Native destruction can retire an instance while leaving its logical open intent intact. Session end closes windows and resets their dynamic state. Host bindings are renewed when the host is rebuilt; there is no existing propagation from a source host's closure to independently opened extension windows.
- Existing big-panel exclusion remains separate from ordering: `panel` defaults to true, and those windows close other big panels; non-panel windows and pop-ups can coexist.
- Generated close buttons already use Esc. Pop-up buttons use Z/Esc and C/Return, or C/Return for a single button (`core/src/windows/gui.rs`). Ordering therefore has consequences for shortcut precedence as well as pointer input.

These are source observations, not newly accepted lifecycle behavior or runtime verification.

## Source-level integration findings

The focused source investigation is complete. It identifies reusable infrastructure but does not establish that a new native call is safe at every entry point.

| Existing location | What can be reused | What it does not establish |
| --- | --- | --- |
| [drawing.rs](../../core/src/drawing.rs), `app_frame_before`, `main_holds`, and [drawing/turns.rs](../../core/src/drawing/turns.rs), `frame_begins`/`day_begins`/`day_ends` | The main application frame and drawing thread share a presentation lock. The daily update releases it; returning to the frame reacquires it. | Main-thread identity or a framework state mutex alone does not prove that lock is held. It also does not prevent reordering a list currently traversed on the same thread. |
| [safe_point.rs](../../core/src/safe_point.rs), [lua/mod.rs](../../core/src/lua/mod.rs), and [windows/live.rs](../../core/src/windows/live.rs), `frame` | A central main-thread path consumes window actions and reconciles native instances after the map-manager update. | It is not the last possible interface change in the entire frame, and the window coordinator itself does not acquire or assert the presentation lock. |
| [ui.rs](../../core/src/ui.rs), host binding and retirement; [windows/live.rs](../../core/src/windows/live.rs), `killed` | Bindings can renew host identities and retire old native objects. The window lifetime process owns its explicit destruction paths. | Existing bindings do not register every original child. A failed `LIVE.try_lock()` can skip a destruction notification, so an additional ordering registry must also follow framework-owned destruction. |
| [wanted.rs](../../lua/src/windows/wanted.rs), `show`; [ui.rs](../../core/src/ui.rs), `apply` | Logical extension-window transitions distinguish repeated open requests; a known native window's visible flag can be read. | The existing show/hide dependencies are not a complete original-window transition observer. Sampling once per frame can miss hide/show pairs and cannot by itself resolve ancestor visibility. |
| [bound_element.rs](../../core/src/bound_element.rs), subscriptions; [ui.rs](../../core/src/ui.rs), `button_input` | Bound button actions retain their object and context. The existing native button-input observer receives an input pointer. | Current subscriptions do not cover all backgrounds, row surfaces or original controls. The current observer only corrects selected right-double-click input; it does not establish accepted mouse activation. A button action can also originate from a shortcut. |
| [drawing/prepare.rs](../../core/src/drawing/prepare.rs), `frame`, and [native ordering research](../research/window-ordering.md#drawing-and-input) | The native drawing walk rebuilds the first tree's draw list and GUI-owner list. | The framework's existing preparation function does not explicitly rebuild these GUI lists. Tree mutation under a lock does not make the prepared input order current. |

O1 must close three specific evidence gaps before the native ordering behavior is implemented:

1. **Original visibility transitions:** establish observation coverage and identity for the supported original windows, including visibility affected by their parent and reconstruction. Do not equate construction, a show request, and an actual appearance.
2. **Accepted pointer activation:** identify the target and originating mouse event across supported controls, backgrounds and rows, distinguish keyboard-generated actions, and avoid reentrant list mutation or stale targets after an action closes its window.
3. **Order commit and input freshness:** establish a synchronization point and a verified way for the next relevant input to see the new order. Prove the contract for both drawing modes and for more than one input in a batch. Do not assume a redraw occurs between events or call a full drawing routine as an unexamined list-refresh shortcut.

These are engineering investigations, not unresolved product choices. If they reveal that an accepted behavior cannot be met with the proposed integration, report the concrete conflict before relaxing behavior or expanding scope. No runtime verification is claimed by this source investigation.

## Proposed implementation

This section translates the accepted behavior into work to be reviewed. It is not a claim that native integration or runtime behavior is already verified.

### One private ordering process

Maintain ordering through one private framework module, used by host bindings and extension-window lifetime handling. Its inputs are window registration, actual visibility transitions, accepted pointer activation, native-instance replacement and retirement. Its output is the required relative placement of framework-owned content within existing scopes. Lua keeps its current declarations and operations.

An unchanged desired order should not cause repeated native reattachment. Keep work bounded by registered live content and pending events rather than scanning every original window or continually appending framework nodes. Becoming uncovered when another window closes is not a new visibility transition and must not create activation.

Keep logical identity separate from the current native pointer and its lifetime. Rebinding a surviving logical window restores its position in the order; it does not generate activation. A destroyed native instance cannot remain a valid target for a queued placement. A new logical window follows the ordinary appearance rule. Existing extension reload and game-session reset semantics determine which logical identities survive.

Do not flatten all windows into a global numeric sort. Within each scope, retain the game's relative order of original windows, and place framework-owned windows or attached content around those original entries. Opening or activating an extension window can move it ahead; activating an original window can move relevant framework content behind it without swapping original entries. The native order observed after the game's own handling is authoritative for original entries.

Treat the button area's background and controls as related content despite their separate native objects. Preserve their own background-before-controls order and their existing ownership and lookup behavior. They do not receive independent activation rank. The attach picker must be ahead of this content while shown. Do not implement that guarantee by moving the picker into the second scene tree or by changing unrelated original siblings' order.

### Native adaptation

Use verified native methods to maintain every representation of order that the game relies on. The named-child method demonstrates that a window can keep both scene links and a separate element list; raw scene-link writes are not an adequate default. Choose concrete native operations after following the relevant callers and dependent methods, and register every new game dependency with expected bytes.

Observe input without consuming or replaying the event. Resolve an accepted hit to the innermost participating window in its existing scope; do not promote ancestors. Hover, keyboard dispatch and repeated visibility requests must not masquerade as pointer activation. The clicked action can close or destroy its window, so the eventual placement must validate lifetime again.

Commit order changes only at a point that excludes simultaneous scene traversal and respects the current native input walk. The prepared draw and input lists must agree with the new tree before subsequent input uses them. A lock around the tree edit alone does not establish this. Native transition and event coverage, synchronization and prepared-list refresh remain evidence obligations, not assumptions about a method being named a safe point.

### Initial coverage

- All existing extension-window declarations and pop-ups, with big-panel exclusion preserved.
- The army-panel host's button area, added controls and attach picker, including binding loss and reconstruction.
- Original windows whose interaction with framework windows is needed to satisfy the constrained ordering rule. Inventory the concrete native paths during O1 and record verified coverage; do not infer global coverage from one common vtable or hook.
- Both main-thread drawing and the framework's optional drawing thread. The separately drawn framework UI remains outside this work.

## Parts

| Part | Deliverable |
| --- | --- |
| O1 | Verify native integration: show-transition coverage, accepted pointer targets including backgrounds and list rows, lifetime invalidation, a synchronized mutation point, and refresh of dependent draw/input lists. Record evidence and the concrete original-window coverage. |
| O2 | Implement the private ordering process with stable logical identity, scope-local activation, original-order constraints and native-generation replacement. Exercise behavior through an internal native-operation seam. |
| O3 | Integrate extension windows and pop-ups into that process without changing Lua declarations, operations, big-panel exclusion or reload semantics. |
| O4 | Integrate host bindings and the button area; preserve original sibling order and guarantee that the attach picker covers added content. Rebinding must restore the rule. |
| O5 | Integrate native activation observations and synchronized placement, including dependent input-list freshness. Verify that the activating action runs exactly once and that shortcuts respect the resulting order. |
| O6 | Run behavioral regressions, required repository checks, offline dependency checks and isolated game verification; update research, game documentation, player/mod-author documentation and the prototype log. |

Every part is done when its stated behavior is implemented, its native evidence or tests are recorded, and its errors and lifecycle exits preserve the same rules. O1 is a prerequisite for claiming the native parts feasible; a missing observation or refresh seam is reported and resolved before those parts are treated as complete. O2 through O5 form one coordinated change, not separately releasable partial repairs. The user gave final design confirmation and authorized implementation on 2026-11-14.

## Acceptance cases

| Scenario | Required result |
| --- | --- |
| Open the army attach picker with the button area visible | The picker and its rows draw over the overlapping added content; selecting a row does not activate an underlying button. |
| Close the picker | The button area is usable again at its original position. |
| Open extension A, then B; close and reopen A | B initially leads; reopening A puts A ahead within the same scope. Use coexisting non-panel windows rather than accidentally testing big-panel exclusion. |
| Refresh a background, text, list or an already-visible window | No activation or change of relative order. |
| Click an exposed control in a window behind another | Its participating window comes forward within its scope, and the original control action runs once. Newly exposed controls receive no replayed click. |
| Activate a nested picker | Only its scope changes; unrelated ancestor scopes keep their order. |
| Open or activate an independent nonmodal window after a pop-up | The pop-up has no unconditional always-on-top privilege. |
| Front-to-back order original A, extension X, original B; activate B | X moves behind B while A remains ahead of B unless the game itself changes the original order. |
| Native reconstruction with logical open intent preserved | The same logical window retains its relative order, including a definition-generation change caused by another extension. |
| Close or destroy a window during its activating action | No placement uses its retired native instance and no deferred event resurrects it. |
| Shared Esc or Return between eligible windows | After native modal/focus precedence, the foremost matching window handles the key once; the same event does not act on the window behind it. |
| Activation followed by another input in the same batch | Later input uses the intended updated order; an old prepared list must not silently defeat activation. |
| Host reconstruction, extension reload and game-session end | Restore only the state existing lifecycle rules preserve; clear retired identities and retain existing big-panel exclusion. |
| Existing extension without any new fields or calls | Receives the behavior automatically. |
| No relevant framework content participates | Preserve the game's original behavior; do not impose a new global activation policy. |

Unit/process tests should drive event sequences and native lifetime failures through the same private ordering interface used in the game. They do not replace native verification of tree/list consistency, click targeting or synchronization. Game verification uses a claimed isolated test game under the project procedure, never the player's installation or running game.

## Completion of the design

Every behavior branch must be resolved or explicitly deferred, with the consequences recorded. Newly accepted project-specific terms belong in the glossary; an ADR is added only for a decision with a costly reversal, a non-obvious rationale and a real tradeoff. Implementation parts and runtime acceptance checks above are approved. Remaining native evidence is investigated before implementing the corresponding integration.
