# The game

What is known of Europa Universalis IV as a program, one page for each of its systems, as it is now understood: what the system is, how it works, how it touches the others, and what is not known yet. The pages are written in the game's terms; the evidence for each claim is in a note of [research/](../research/), which the claim links. All of it is of the 1.37.5 baseline (`9ad3efe1…`).

| | |
| --- | --- |
| [day.md](day.md) | The day and its time: how the game runs a day and a month, the main thread and the threads beside it, dates, speed, and random draws |
| [countries.md](countries.md) | Countries: their state, government, knowledge of each other, monarch power, subjects, defeat, achievements and names |
| [provinces.md](provinces.md) | Provinces and the map: provinces, map modes, the province view, the fog of war, trade and sieges |
| [military.md](military.md) | Armies and fleets: units, orders, routes, battles, leaders and recruiting |
| [diplomacy.md](diplomacy.md) | Diplomacy: relations, envoys, questions and offers between countries, and peace |
| [ai.md](ai.md) | The AI: how a country the AI plays decides, and its agents |
| [scripts.md](scripts.md) | Scripts: triggers and effects, the console, and the commands a session executes |
| [interface.md](interface.md) | The interface, text and input: windows and their bindings, the main menu, text and fonts, the camera, mouse and keyboard, screenshots |
| [program.md](program.md) | The program: its architecture, startup and loading, the files and folders it uses, saves, and how it runs unseen or several at once |

## How a page is written

- **The system as the game has it.** The first paragraph says what the system is, in words a player knows; the rest how it works, in the order things happen, with the names the game's own scripts and files use. A page holds the whole system at a glance; the detail stays in research.
- **Each claim links its evidence**: the research note, and its section where it has one, as [research/time.md](../research/time.md). A claim no note holds is not written here until one does.
- **Addresses only to find the code.** A stretch of the game's code is named by what it does, with its function's RVA in brackets at most once, as the day's update (`0x758430`); offsets, bytes and measurements stay in research. A number is written when it is the game's own (a year is 365 days, with no leap day) or when its size matters to the system (late in a game the AI's stage is a quarter of the main thread), with its note linked.
- **How the systems meet.** Where a system reads or changes another's state, the page says so and links the other page.
- **What the framework relies on**, in a line each: the capabilities and dependency points that use the system, linking their plan or ADR.
- **What is not known yet**, as plainly as what is: questions no note answers, and where notes disagree.

When research changes what is known of the game, the page it changes is changed in the same change (AGENTS.md).
