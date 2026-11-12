# Area governing costs

Status: decided 2026-11-12 by the owner as a prerequisite of a separate convenience mod; A1 to A3 built and tried the same day, with limits recorded in [the run](../prototype-log/2026-11-12-convenience-tools.md). Review refinement separates live national capacity into `country.governing_capacity`; the current interface and prediction failure boundary are in [the research](../research/area-governing-cost.md).

## Goal and scope

Let any extension read a country's share of an area and determine the additional governing capacity of making it a state without completing cores. Selection and batching belong to the mod, outside this repository. No country-wide command, new setting, automatic coring, or changed game rule.

## Behavior

`province.area` takes an owned province and returns the area's content key, owned members, owned total development, statehood, whether any owned member belongs to a trade company, current area cost, and the additional cost of statehood without coring. Cost and capacity are integer thousandths. Used capacity is recomputed by the game's own getter rather than trusting its cached country field. An unverifiable projection is absent with a reason.

The native state command remains unchanged. The mod checks a fresh area immediately before it commands a state, and checks capacity again afterward. It never tries a state and abandons it to undo an oversized operation.

## How it fits together

The capability stays in the province group and uses a country session. Its implementation is `game::areas`; the game's central dependency-point collection includes its points. State projection follows the game's fixed-point cost path and the loaded static modifiers, preserving territorial cores. Evidence is in [area-governing-cost.md](../research/area-governing-cost.md).

## Parts

- **A1**: owned-area grouping, statehood and trade-company reads.
- **A2**: live governing capacity, native province costs and a checked state projection.
- **A3**: offline checks and real-game comparisons, including state creation followed immediately by a fresh capacity read, and territorial-core preservation.

Each part is complete when its capability shape, tests, research and game page agree, and A3 records what the actual game verified and what remains unknown.
