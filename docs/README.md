# Documentation

Start with the account that answers the question; other pages link to it rather than repeat it.

| Need | Read |
| --- | --- |
| Install Harmony or make a first mod | [README](../README.md) |
| Capability arguments, results, handlers and manifest | `eu4-harmony wiki` (generated from the catalogue) |
| A procedure for making or driving a mod | [Shipped skills](../cli/skills/index.md), also `eu4-harmony skills` |
| Native game logic, data ownership or a game address | [Game systems](game/README.md), [address index](game/addresses.toml), `cargo xtask what <rva>` |
| Build, isolated test games and development commands | [Development tools](dev-tools.md), [adding a capability](adding-a-capability.md) |
| Measured behavior and measurement limits | [Performance](performance.md) |
| Logs or publication | [Logs](logs.md), [releases](releases.md) |
| Why a costly decision was made | [ADRs](adr/) |
| Open work | [Backlog](plans/README.md) |
| Durable experiments and incident evidence | [Research evidence](research/README.md) |
| Project language and contribution rules | [Glossary](../GLOSSARY.md), [AGENTS](../AGENTS.md) |

Ordinary validation belongs in PR descriptions. Evidence pages preserve significant findings, counterexamples and limits; they are not a log of every run. Implementation is explained beside its code.
