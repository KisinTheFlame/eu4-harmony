# Mercenary discipline and the battle panel

This note separates the land-battle panel's displayed discipline from the discipline used by an individual mercenary regiment, for investigating a player's report that mercenary discipline was absent from that panel. Harmony uses the native panel and combat calculations; no framework repair or new interface is implemented here. Baseline 1.37.5, read on 2026-11-12 with the repository disassembler after consulting the address index. Addresses are RVAs into the installed `eu4.exe`.

## Terminology and the reported symptom

In the installed Chinese language mod, `MODIFIER_MERCENARY_DISCIPLINE` is “雇佣兵训练度”, and `DISCIPLINE`/`COMBAT_DISCIPLINE` are “训练度”. `ARMY_DRILL` is “陆军操练度”. A prior [drill investigation](../game/military/drill.md) answered a different question; successful drill growth does not resolve a report about mercenary discipline.

A [test of the supplied Portugal save](interface-evidence.md) reproduces the display mismatch in a reduced six-mod playset: the country has `discipline=0.05` and `mercenary_discipline=0.298`, while its battle-side discipline field shows 105%. That run verifies the display and its tooltip, not a measured difference in battle losses.

For the tested save's Free Swiss Guard, the known inputs are 100% base, +5% national discipline, +29.8% national mercenary discipline, and +5% company discipline: **139.8% before any other applicable regiment-local or leader discipline modifier**. The saved company leader has no serialized personality. The sampled battlefield's side tooltip adds no leader discipline. The existing capabilities do not expose the return of `0xae6580` or the complete regiment-local modifier set, so 139.8% is a prediction from the checked inputs and formula, not a directly measured per-regiment runtime return. It must not be reported as a measured battle result or as every mercenary company's value.

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

The systematic game account is [discipline](../game/military/discipline.md).
