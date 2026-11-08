# Plans

A plan is written for each larger piece of work: what it is for, what it changes as a player, a mod author or a developer sees it, and the parts it is built in. The list of plans, by whether they are in progress, built or replaced, is in [docs/README.md](../README.md).

A plan is not where the framework or the game is described as it is now: that is `eu4-harmony wiki`, the code's own comments, [dev-tools.md](../dev-tools.md) and the pages of [game/](../game/). A decision that is hard to reverse, taken in a plan, gets an ADR of its own, which the plan links.

## What a plan holds

- **A status line** under the title: decided on what day, which parts are built and tried and on what day, with the runs of [prototype-log/](../prototype-log/) that tried them, and which were set aside. The terms it adds to [GLOSSARY.md](../../GLOSSARY.md) and the ADRs it rests on.
- **Goal and scope**: what the work is for, and what it leaves out.
- What the game does that the work rests on, in a few lines, linking [game/](../game/) and [research/](../research/).
- **Behavior**: what changes, part by part, as those who use it see it.
- **How it fits together**, where the parts meet in the code.
- **Parts**, each named by a letter and a number (`O1`, `S10`), with what it delivers; "Every part is done when" says what a part needs before it counts as built.

A section is left out where the work has nothing to say in it.

## When a plan is done

A plan is done when each of its parts is built or set aside, and its status line says so. From then it is a record of how the work went, as an ADR is of a decision: its text is no longer changed, but for its status line. What changes afterwards is written where things are described as they are now; later work on the same thing gets a plan of its own, which links the old one.

A plan that takes new parts as it goes, as [game-speed.md](game-speed.md) does, stays in progress, and may be changed, until the owner closes it.
