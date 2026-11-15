# Interface and drawing evidence

Historical investigations on EU4 1.37.5, normally with a Chinese translation playset, 3840×2160 output and GUI scale 1.5. Hidden games used private cores, user folders and pipes. Results apply to their identified setup, not every interleaving or the current build. Routine success captures and build transcripts have been removed; no retained test PNG showed an irreplaceable original failure.

## Text, fonts and input

On 2026-10-27–28, map names were missing without Harmony too: the translation mod's 49 MB DXT5 page exceeded the 16 MiB texture limit and loaded as 0×0. Raising the limit restored country, province and sea names. Separate driver failures occurred when framework UI overlapped main-thread work on the loading thread's shared device. Restricting framework UI to main-thread presentations preceded many starts without those failures. Device multithreading protects calls, not arbitrary rendering sequences.

Input tests found Backspace leaving continuation bytes, Home/End unexpectedly selecting, and Shift+arrows reaching the map. Corrections were checked by copying Chinese selections. Real Microsoft Pinyin then placed composition/candidates beside the edit box; Enter committed without closing, Backspace edited composition only, and Escape canceled composition without closing. Injection-only selection failures were superseded by these real-input checks.

Mixed Windows-1252 punctuation stopped ICU at non-UTF-8 bytes, shortening lines and splitting Latin names. Character-aware processing restored paragraphs and truncation. Counting characters alone left single-character Ming sideways; fixing its direction made it upright. Band-by-band texture writes were associated with about 425 MB map-page growth; staging reduced the measured main-menu process from 4,353 to 3,866 MB and shared glyph storage from 66 to 3.2 MB on that machine/configuration.

October 30–November 2 captures at scales 1.5/1.6 checked denser system glyphs and pixel alignment. Tracing a fifteen-argument font function with twelve forwarded arguments corrupted tooltip widths. November 8 map-page growth at the main menu left stale country-name UVs in the next game; rebuilding at the next in-game frame restored them. Transient missing-glyph caching remained unestablished. Native accounts: [text/input](../game/interface/text-input.md), [font textures](../game/interface/font-textures.md), [map shader](../game/interface/map-name-shader.md).

## Rendering races and negative results

November 8 write watches stalled in the driver when its threads were held; watchdog-limited samples cover only accesses before those waits. November 9–11 exposed distinct drawing hazards: idler changes during frames, freed mesh objects, main-thread scenes changing renderer state, and deferred floating text using the wrong allocator. A later complete scenario passed in 3 h 17 min, but overlapping work and changing builds exclude an isolated timing comparison.

November 10 camera drift came from the drawing thread lacking the main thread's active-window origin; the sampled main-thread cursor stopped it. Among 5,000 unchanged map points, 4,979 icon projections were within one pixel and the rest within ten. Post-update screenshots could not distinguish old/new behavior.

Natural events on November 11 drew the whole frame into an option texture from the twenty-second event; console-fired events did not reproduce it. Render-target protection was followed by 44 natural events without that corruption. November 12 traces separately showed drawing initialization nested inside main-thread text initialization in eight of forty events, then none of sixty after event-display protection. A developer build from published 0.1.3 showed seven overlaps without identifying each object or proving texture-write order. The reporter's exact recurrent corruption was not reproduced there.

Accepting a call to arms on November 11 stopped rendered frames while processing continued: frame-tail renderer work invalidated prepared frames. Avoiding invalidation for already-locked work yielded approximately 133–139 FPS for the same trigger and nine subsequent years without the freeze. Country switching reproduced the mechanism separately. See [frame lifecycle](../game/interface/frame-lifecycle.md) and [map reads](../game/interface/map-drawing-reads.md).

## Temporary map highlights

November 11–12 burst captures handled pulsing borders. Baseline red-pixel counts were six in all eight samples. Removing only the temporary-highlight clear restored varying counts up to 846; permanently skipping it accumulated stale targets and was rejected. Preserving prepared inputs while retaining native clearing restored outlines, target replacement/removal, in-game loading and changing the country in view. Greece's unowned cores were purple, disproving an initial red-only assertion. A 180-day observe run did not verify natural player questions or every producer.

## Tooltips, backgrounds and ordering

November 13 tooltip failures came from needless generation changes, a vector read as a linked list, and requiring a hit target that native initialization clears. Corrected captures checked stationary changes, row replacement, disabled controls and native tooltips above framework windows on both presenting threads. Device loss, arbitrary resource overrides and every destruction interleaving were not covered.

November 14 disproved an older claim that disabled backgrounds kept their color: a sampled rectangle changed from equal RGB means 52.23 to 43.58/53.95/61.02 when enabled. November 15 fixed-background checks used two window sizes and one-/two-column areas. Hover/held captures were identical; backgrounds stopped wheel input while lists scrolled and enabled controls retained pressed presentation. Dragging outside closed the panel; capture beyond its rectangle was not established.

Independent traversal matched 53 threaded and 44 native-drawing samples; live refresh matched 39 pairs. These counts did not prove input recipients. An ineffective first Escape was later traced to the original `startup_screen`, despite no waiting questions. Dismissing it allowed the two-Escape/Return course to pass.

A November 14 button-only picker fixture passed while the actual playset still overlapped. November 15 derived-icon classification passed both button-plus-glow and unchanged eight-mod playset checks: picker coverage, blocked underlying callbacks, native target-row action and restored controls. The native Detach tooltip established attachment; an unavailable `army.get` field did not. Shared hide code also received a derived window and terminated on ordinary-class validation after army selection/time advance. Classifying it first survived seventeen days, another selected army and picker reopening. Exact derived identity, all classes, same-native-batch input and live scrollbar movement remain unverified. Native mechanisms are under [window visibility](../game/interface/window-visibility.md), [placement](../game/interface/window-placement.md) and [activation](../game/interface/window-activation.md).

## Unreproduced message buttons

November 15 annexation/alliance results did not reproduce missing action groups. Twelve later captures contained twenty-six open-message samples, thirteen of an alliance rejection; these were not independent messages. Selected buttons were visible, active, textured, meshed, positively colored and in the first draw list. Two captures had other notifications in front. Reads/captures were nonatomic, and the reporter's failing state was never captured. No cause or repair was established. [Message drawing](../game/interface/message-button-drawing.md) records the native chain.
