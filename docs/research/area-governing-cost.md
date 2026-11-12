# An area's governing cost

This note follows an owned province's area and the governing capacity used before and after making it a state, without completing cores. `game::areas` uses these reads for `province.area`; the country-wide choice of areas lives in a separate mod. Baseline 1.37.5, read on 2026-11-12 with `cargo xtask disasm`. The runtime comparisons are in [the run](../prototype-log/2026-11-12-convenience-tools.md). Static reading and those comparisons are distinct evidence.

## Area and country state

The province's map data (`+0x28`) holds its area definition at `+0x40`. The state confirmation reads it at `0x133ece8`. The definition has its content key at `+0x48` (the area writer `0xac2c10`, `0xac2c36`) and province numbers as a vector of i32 at `+0x88`..`+0x90` (the area's cost reader `0x3f12e0`). This is the definition, not the runtime area whose `+0x10` points to it. The original ambiguity in [provinces.md](provinces.md#state-or-territory-area-and-state-edict) is resolved here.

A country may own only part of an area. Both the native cost reader and `province.area` include only members owned by the country. Trade company membership remains per province; the capability's `trade_company` says whether any owned member belongs to one.

Making a state (`0x3f0810`) asks the runtime area for the country's state through `0xac2800`, adds it to the country's state list, updates the area's owned provinces and invalidates their modifiers (`0x9f9500`). The state constructor initializes prosperity to zero, no edict, no patriarch, no pasha and no holy order. It does not complete cores.

## The native cost path

- `0xa2e0f0(province, out, tag*, explanation)` returns a fixed-point multiplier. It adds 1000, country `governing_cost` (`0x19b`), one country modifier selected by trade-company/state/territory status (`0x19c`, `0x19e`, `0x19d`), province `local_governing_cost` (`0x19f`), and every same-owner area member's `statewide_governing_cost` (`0x1a2`). The final multiplier is at least 10, or 1%. The modifier getter `0x8e6bd0` includes each type's callback.
- `0xa2e6f0(province, out, unused, multiplier*)` reads tax, production and manpower through the same native development getters used elsewhere, multiplies their fixed-point sum by the nonnegative multiplier, divides by 1000 with integer truncation, adds `local_governing_cost_increase` (`0x1a0`), then clamps the cost to zero. This function was read to its return.
- `0x3f31d0(country, out)` recomputes used capacity. Its area list is at country `+0x98a0`..`+0x98a8`, valid when `+0x98b8` is set. It sums each owned province's cost and `state_governing_cost_increase` (`0x1a1`), then keeps the larger of each area's sum and that sum divided by 100. It was read to its return. The per-area comparison matters for negative scripted flat costs, even though ordinary positive costs keep the original sum.
- `0x2f1120` stores that getter's answer in country `+0xe2c`. A caller cannot assume this cached field has already been refreshed after a command. `province.area` calls the live getter and reads the maximum through the existing maximum getter `0x3f2e10`.

No development-based estimate or binary floating-point capacity comparison is needed: the capability returns all costs and capacity in integer thousandths.

## Statehood without completing cores

The province modifier builder's core branch `0x95d2a0` selects a loaded static modifier. Without a country state, a core gets `territory_core` and a non-core gets `territory_non_core`. With a state, a non-core gets `non_core`; a core whose country remains in the territorial-core tag vector (`+0x4a8`..`+0x4b0`) gets `colonial_core`, otherwise `core`. `0x95db70` adds `in_state` for a state; a newly constructed state has none of the prosperity, edict, pasha or patriarch additions.

The static database getter is `0x8ecec0`. The reader `0x8ed850` assigns `in_state` at `+0x310`, `core` at `+0x460`, `non_core` at `+0x470`, `colonial_core` at `+0x480`, `territory_core` at `+0x490` and `territory_non_core` at `+0x4a0`. Their values come from the loaded game's scripts, not constants of vanilla's files.

The projection changes the country type modifier from territory to state, replaces that core static, and adds `in_state`, for every owned member. It carries through local percentage, local flat, area flat and statewide modifiers. It computes the raw new multiplier before the 1% floor, so a currently floored territory does not lose the information below that floor. The current multiplier and province cost are cross-checked with both native getters before returning a projection. Arithmetic overflow or a mismatch leaves the projection absent with a reason.

The state's confirmation window (`0x133eba0`) subtracts the loaded `territory_core` local modifier from the current multiplier and prices that with the native cost getter. It does not add the `colonial_core` modifier. That is a full-core projection, and was not reused for a command that only makes a state.

The native command remains responsible for eligibility and execution. The extension reads an area again before each command and compares a fresh live total afterward. It stops further state creation if a read fails or the result differs; it does not attempt an oversized state and abandon it afterward.

## Runtime evidence and limits

Castile's nine owned areas summed to the live total of 231.500. Galicia, 28 development, changed from a full-core state's cost 28.000 to territory cost 7.000 when abandoned. The capability predicted an additional 7.000 for statehood; the real command produced cost 14.000, with the live country total changing immediately from 210.500 to 217.500 without advancing the date. Thus this tested path preserves territorial cores and does not require the caller to request a country-cache refresh; when the game itself updates that cache was not measured.

The separate run also tested a mixed trade-company area and all three buttons. The checks do not constitute exhaustive proof for arbitrary mod scripts or native modifications: scripted effects that change other country's/province's modifiers as a consequence, or later game updates, have not been exhaustively tested. There was no real-game test of heavily reduced costs crossing the 1% floor, negative scripted area costs, a large total-conversion map, or overflowing modifier values; fixed-point, overflow and negative-area arithmetic have offline tests. A current/native mismatch is refused, and the extension checks the result before proceeding to another area.
