# Mercenary discipline and the battle panel

This note separates the land-battle panel's displayed discipline from the discipline used by an individual mercenary regiment, for investigating a player's report that mercenary discipline was absent from that panel. Harmony uses the native panel and combat calculations; no framework repair or new interface is implemented here. Baseline 1.37.5, read on 2026-11-12 with the repository disassembler after consulting the address index. Addresses are RVAs into the installed `eu4.exe`.

## Terminology and the reported symptom

In the installed Chinese language mod, `MODIFIER_MERCENARY_DISCIPLINE` is “雇佣兵训练度”, and `DISCIPLINE`/`COMBAT_DISCIPLINE` are “训练度”. `ARMY_DRILL` is “陆军操练度”. A prior [drill investigation](mercenary-drill.md) answered a different question; successful drill growth does not resolve a report about mercenary discipline.

A [test of the supplied Portugal save](../prototype-log/2026-11-12-mercenary-discipline-panel.md) reproduces the display mismatch in a reduced six-mod playset: the country has `discipline=0.05` and `mercenary_discipline=0.298`, while its battle-side discipline field shows 105%. That run verifies the display and its tooltip, not a measured difference in battle losses.

## The panel's number is a side overview

The base game's `interface/combat.gui` names both sides' discipline field **`dicipline_value`**, with the same spelling in the binary. The battle-panel update **`0x1051fd0`** finds that field at `0x1053a9f` and obtains its value by calling **`0x21d880(side, out_value, optional_text)`** at `0x1053abc`, with the text argument null. The tooltip path in **`0x104d2c0`** calls the same function at `0x104e2f3`, then adds the localisation `DISC_ADD_DESC`.

`0x21d880` does the following:

1. **`0x21fd90(side, out_country)`** chooses the country for the overview: a valid side leader's associated army's country, otherwise the first participating country; it has a special-country fallback through the side's units.
2. **`0x341820(country, out_value, optional_text)`** starts at 1000 (100%) and adds the country's ordinary discipline, modifier **`0x77`**. It has a minimum of 10 in the game's thousandths.
3. It adds the side leader's ordinary discipline modifier `0x77` through `0x8685a0`, if applicable.
4. When text is requested it formats the country's and leader's ordinary-discipline explanation. It does not iterate the side's regiments, calculate a mercenary share, add company discipline, or read mercenary discipline **`0x130`**.

The default explanation `DISC_ADD_DESC`, observed in the Chinese test, explicitly says that this is the leading army's discipline as a guide and that each unit uses its own value in combat. The base English text says: “This is the Discipline of the combat leader and should be seen as representative. Every unit still uses its own values during combat.” This is not a weighted discipline average of the side. An all-mercenary force need not make this overview show its mercenary discipline.

## The regiment's combat value includes the mercenary addition

The actual regiment discipline reader is **`0xae6580(regiment, out_value)`**, not `0xadf180` (the latter is unit-kind combat ability). Its complete body was read:

- It resolves the regiment's military country with `0xadf040` and calls `0x341820` for ordinary national discipline.
- If `regiment+0x64` is category 1 **or** its army (`regiment+0x40`) has a company at `army+0x48`, it reads the regiment owner's country modifier **`0x130`**, `mercenary_discipline`, and **adds** it to the result. The eligibility branch is at `0xae65ce`–`0xae65ec`; the addition is at `0xae667d`. The modifier registration at `0x8dfef4`–`0x8dff24` identifies index `0x130` with `MODIFIER_MERCENARY_DISCIPLINE`.
- It builds the regiment-local set with **`0xae7190`**, reads its ordinary discipline **`0x77`**, and adds that at `0xae66f4`. That set includes company ordinary discipline and the regiment's leader's modifiers. It also includes unit-category and applicable unit-conditional modifiers through **`0xae7600`**; this is an additional possible source, not a value shown by the side overview.

These are additions in percentage points to the discipline factor, not multiplication of national and mercenary discipline. A company with `discipline=0.05` adds another five percentage points through the local `0x77` term; the country `mercenary_discipline=0.298` is separately added only for qualifying regiments.

For the tested save's Free Swiss Guard, the known inputs are 100% base, +5% national discipline, +29.8% national mercenary discipline, and +5% company discipline: **139.8% before any other applicable regiment-local or leader discipline modifier**. The saved company leader has no serialized personality. The sampled battlefield's side tooltip adds no leader discipline. The existing capabilities do not expose the return of `0xae6580` or the complete regiment-local modifier set, so 139.8% is a prediction from the checked inputs and formula, not a directly measured per-regiment runtime return. It must not be reported as a measured battle result or as every mercenary company's value.

## It reaches the damage calculation

In the land-regiment attack routine **`0x225860`**, the acting regiment is taken from the attack context's `+8`. Its discipline is read through `0xae6580` at **`0x2262ee`** for strength damage and at **`0x226b0b`** for morale damage. The resulting factor is multiplied into those calculations. No later mercenary branch replaces it with the side overview's 105%.

The final values reach **`0xadd8d0`** at `0x226e4a`: strength damage is accumulated in the target regiment's `+0x34`, morale damage in `+0x38`. **`0x220c00`**, called by the battle's daily update, subtracts the pending values from strength `+0x28` and morale `+0x2c` and clears them. The full attack routine and these relevant callees were read through their returns in the [drill-path investigation](mercenary-drill.md#from-the-factor-to-losses); those shared loss-writing facts apply here too. This establishes the native route for the mercenary discipline addition. It does not measure how much a particular battle's losses change when that addition is removed.

## What the supplied save has

The source save's Portugal state and the installed base rules account for the six-mod runtime result:

| Source present in the save | Installed rule | Mercenary discipline contribution |
| --- | --- | --- |
| `mercenary_ideas=7` | Completion bonus in `common/ideas/00_basic_ideas.txt` | +5% |
| Active `ab_mercenary_discipline`, current age `age_of_reformation` | `common/ages/00_default.txt` | +5% |
| `economic_hegemon`, country POR, progress 100 | `common/hegemons/0_economic_hegemon.txt`, scaled maximum | +10% |
| Active `doppelsoldner_reform`, `hessian_militarization` power 98.016 | The reform grants the mechanic; `common/government_mechanics/10_hessian_militarization.txt` gives +10% at power 100 | About +9.8% at this power |
| **Total read from the running country** | `country.modifiers POR mercenary_discipline` | **+29.8%** |

The normal discipline query returned +5%. The hired military advisor is a `commandant`, whose installed rule gives `discipline=0.05`; the battle tooltip names that advisor as the +5% source. Neither active policy `mercenary_registration_act` nor `hired_expeditions_act` gives discipline in the installed definitions: they give maintenance/supply-depot and forced-march/morale-recovery effects respectively.

The saved company `merc_free_swiss_guard`, company ID 681, fields an army with save ID 27254. Its installed company definition in `common/mercenary_companies/00_mercenaries.txt` adds ordinary `discipline=0.05`. This is the distinct company contribution above, not another national mercenary-discipline entry.

## Limits

Two of the source save's eight mods were unavailable locally. The six installed test mods were checked for overrides of the relevant rules; the runtime national totals provide direct evidence for that reduced playset. The original eight-mod files and historical versions remain unknown, so these results do not exclude a separate interaction there.

The panel's omission was observed and matches its native construction and explicit explanatory text. The native damage path includes mercenary discipline. No product code was changed; this work does not propose changing the panel, turning its single overview into a company-specific value, or altering combat. The exact regiment discipline return and a controlled battle-loss comparison remain unmeasured.
