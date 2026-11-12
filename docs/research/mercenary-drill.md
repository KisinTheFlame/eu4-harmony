# Mercenary drill: permission, modifiers and land-battle losses

This note follows mercenary drill from the game's permission check into land-battle damage, to distinguish a refused `army.drill` capability call from drill that grows but has no battle effect. It concerns a different mechanic from the player's later-clarified report about **mercenary discipline** (Chinese “训练度”; drill is “操练度”), which is investigated in [mercenary-discipline.md](mercenary-discipline.md). The framework exposes the game's drill command; it does not implement these damage calculations. Read from the installed 1.37.5 `eu4.exe` on 2026-11-12 with the repository's disassembler, after looking up the addresses in the index. All addresses below are RVAs. This is static evidence of the game's paths; it is not a measured battle comparison.

## One eligibility check, two uses

The existing [army-modifier research](army-checks-in-searches.md#the-fort-zones-first-question) follows `0xadcb50` but primarily for movement. The full function also establishes the shared permission below:

- `0xadcb50(regiment)` first requires the regiment's virtual method at `+0x60` to return true, and the expansion availability test to pass. The expansion globals and thread-local fallback were not identified by DLC name in this investigation.
- It returns true directly for a category other than 1 at `regiment+0x60`. Category 1 is the category gated as mercenary drill here; this category is distinct from the unit type's infantry/cavalry/artillery kind.
- In that category, it gathers the company's modifiers through `regiment+0x40` (the army), `army+0x48` (the company), `0x900070` (the company's set), and `0x8ea420` (add the set).
- It tests modifier **`0x21f`, `allow_mercenary_drill`**, in the company's set and, if that did not allow it, in the army owner's country modifiers (`0x93d750`, country `+0x28c0`, set at the result `+8`; `0x8e7b60` tests the modifier). Either source suffices; neither means false. The company lookup is at `0xadcc31`–`0xadcc51`; the country fallback and its test at `0xadccfd`–`0xadcd43`.

The army's may-drill check **`0xd40ce0`** calls this exact function for its regiments (`0xd40d30`). If none qualifies it refuses with `NO_DRILL_MERCENARY`; otherwise it continues through the separate technology, province ownership/control, leader, movement and other conditions described in [units.md](units.md#drill_army_mission-vtable-0x1d1c780-token-0x370a-and-stop_unit_mission-vtable-0x1d1bda8-token-0x3bd7). Starting a mission and receiving drill's modifiers therefore share this eligibility predicate, while starting a mission has further requirements.

The country's idea entry alone does not prove the runtime modifier is present: files may be overridden. A runtime country modifier query is separate evidence. Similarly, an enabled drill button proves at least one regiment passed the shared predicate in that army, not that every mixed regiment qualifies.

## Two static modifiers, two scales

The static-modifier registration at `0x8ee9f7`–`0x8eea18`, in the initializer `0x8ed850`, names the entries obtained through `0x8ecec0()`:

| Entry offset | Script key | Installed base content at full drill |
| --- | --- | --- |
| `+0xf20` | `regiment_drill_modifier` | `fire_damage=0.1`, `shock_damage=0.1`, `fire_damage_received=-0.25`, `shock_damage_received=-0.25` |
| `+0xf30` | `army_drill_modifier` | `movement_speed=0.2` |

These values are from `common/static_modifiers/00_static_modifiers.txt`; they are content values, not constants in the combat code. Mods can change them.

**`0xae7230(regiment, out_set)` builds the drill set.** It initializes an empty set, calls `0xadcb50` at `0xae72da`, and returns the empty set if ineligible. For positive `regiment+0x30` (drill), it adds entry `+0xf20` at `0xae7300` and scales its values by that drill divided by the define at `0x233e530`, `ARMY_DRILL_MAX` (100 in base content). It uses the game's integer fixed-point arithmetic. The sparse-set loop scales all pairs; the alternate loop excludes modifier `0x286`, whose special meaning is not needed to identify the four damage modifiers below. With an army at `regiment+0x40`, it also adds `0xd4ca70(army, set)`, the strength-weighted army drill set. That function is the movement path in [army-checks-in-searches.md](army-checks-in-searches.md).

**`0xae7190(regiment, out_set)` builds the regiment's local set used in land combat.** It calls `0xae7230` at `0xae71ad`, then adds the regiment's other modifiers (`0xae7600`), its company's set (`0x900070`) and its leader's set (`0x866900`). It does not replace the drill set after adding it. Country modifiers are added by the caller where needed.

Consequently, the army average's movement modifier and an individual regiment's damage modifiers are different calculations. The four base damage values scale with that regiment's drill, rather than granting the full values merely because drilling is allowed or a mission has started. At 0.833 drill out of 100, their unrounded contributions are only about +0.0833% dealt and -0.20825% received. Fixed-point truncation and displayed rounding matter at that size; a tooltip showing about -0.2% received is consistent with the base file.

## The land-battle path reads those values

The battle's daily update **`0x22c080`** calls each side's virtual method at `+0x80` at `0x22cad1` and `0x22caee`. **`0x226fb0`** occupies that slot in side vtables `0x1c4a420` and `0x1c4a208`; it iterates the land formation and calls **`0x225860(side, attack_context)`**, at `0x22743b` and `0x22745c`. The attack context carries the acting regiment at `+8`, the formation position, phase and other inputs. `0x225860` selects a target through `0x228fe0`, reads unit pips, dice, strength and other combat terms, and calculates losses. Its separate mercenary diagnostic line, `Combat Is Merc, %d, %d`, is at `0x225be4`: a mercenary company is not a reason to leave this routine.

The modifier-registration table `0x8d84a0`, around `0x8ded00`, identifies:

| Index | Script modifier | Localisation key in the binary |
| --- | --- | --- |
| `0x106` | `fire_damage` | `MODIFIER_FIRE_DAMAGE` |
| `0x107` | `shock_damage` | `MODIFIER_SHOCK_DAMAGE` |
| `0x108` | `fire_damage_received` | `MODIFIER_FIRE_DAMAGE_RECIEVED` |
| `0x109` | `shock_damage_received` | `MODIFIER_SHOCK_DAMAGE_RECIEVED` |

The spelling `RECIEVED` is the game's localisation key.

In `0x225860`, phase 1 uses shock, and phase 0 uses fire:

| Phase | Acting regiment's local drill set | Target regiment's local drill set |
| --- | --- | --- |
| Shock | `0xae7190` at `0x226432`, read `0x107` | `0xae7190` at `0x226524`, read `0x109` |
| Fire | `0xae7190` at `0x22677d`, read `0x106` | `0xae7190` at `0x226689`, read `0x108` |

Each is added to that country's value of the same modifier and to the appropriate leader's term. The received-damage total is clamped at -950 in the game's thousandths, then `1 + received` and `1 + dealt` multiply the strength-damage result (`0x2265bf`–`0x226624` for shock, `0x226812`–`0x22686e` for fire). These are additive modifier totals within each factor; the base drill percentages do not guarantee the same percentage change in final battle losses when other modifiers contribute.

The damage result stays live through the rest of the function. Later calculations include morale and the battle-duration factor; the branch that reads country `+0x9878` for a mercenary army concerns the morale calculation and does not discard the already calculated strength-damage term. There is no later mercenary-only bypass of the drill-adjusted strength damage. The end of the routine was read through its return and initialization tails.

## From the factor to losses

At `0x226e2d`–`0x226e4a`, the result is passed to **`0xadd8d0(target_regiment, strength_damage*, morale_damage*)`**, whose own diagnostic text is `TakeDamage, %d, %d, %d`. That function adds strength damage to `regiment+0x34` and morale damage to `regiment+0x38`; it contains no mercenary test or alternative calculation. The subsequent call at `0x226ee4` distributes defined shares to another formation position when present, still using `0xadd8d0`.

The daily update later calls **`0x220c00(side, out_losses, ..., ...)`** for each side, at `0x22cb30` and `0x22cb56`, after the sides' `+0x80` calls. For every regiment it caps pending strength damage at current strength (`+0x28`), subtracts it there at `0x220d9a`, caps and subtracts pending morale damage from `+0x2c`, and clears the pending fields at `0x220dea`. It then updates the side's losses at `+0x134` by unit kind and total `+0x1c`. The physical-strength subtraction precedes its manpower/accounting calls and is not gated on being a regular regiment.

This establishes that eligible mercenary regiment drill can enter the native land-battle loss path through the same local modifier set as regular regiments. It does **not** establish a measured loss difference in the player's battle, that every mod leaves these content values intact, or that the player's report concerned damage in the first place.

## Evidence boundaries

The [first save run](../prototype-log/2026-11-12-mercenary-drill.md) separately established actual growth and matching UI in a six-mod reduced playset. Its source save had no serialized Portugal regiment drill values. The follow-up runtime qualification check is recorded separately in the prototype log; neither run recreates the missing two mods or identifies their historic versions.

The REA MCP tools were not advertised to this worker and no `rea` executable was on its shell PATH; the static evidence here comes from the repository's `cargo xtask disasm`, not REA Evidence. No binary or framework code was modified, and no instrumentation, new capability or development switch was added. No controlled battle replay was performed. The native path, runtime eligibility, and the complete eight-mod user environment remain separate claims.
