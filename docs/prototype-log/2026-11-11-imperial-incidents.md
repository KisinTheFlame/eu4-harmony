# The Empire's imperial incidents

2026-11-11. The rest of M19 of [mod-capabilities.md](../plans/mod-capabilities.md): `empire.incident` and `empire.pick_incident_option`, tried in a hidden test game on the developer build with test games' pipes, in a new game of 1444.11.11 as Austria, then Brandenburg, France and Austria again through the game console's `tag`. What the game's code says is in [research/empires.md](../research/empires.md#imperial-incidents).

## What was done and what came out

- **No incident.** `empire.incident` answered `incident` null, no options, and the Empire's 78 princes; `empire.pick_incident_option 0` was `not_found`, no imperial incident in progress.
- **An incident begun.** The game console's `imperial_incident incident_burgundian_succ_emp` started the Burgundian inheritance. `empire.incident` answered it by its key, its name 勃艮第的遗产, its event `incidents_bur_inheritance.2`, ending 1445.11.11, a year on, and its three options with the texts the event gives them (保持我们与勃艮第的联合统治。, 对法国作出让步。, 将勃艮第整合进帝国。), none with votes yet, option 0, the incident's default, favored and so picked by Austria, the emperor.
- **The emperor's pick.** `pick_incident_option 1` answered null; option 1 read favored and picked, 0 not. Option 7 was `not_found`.
- **The members' votes.** After 20 days the members' picks read 34, 27 and 16 of 78, the AI's.
- **A member's pick.** As Brandenburg, option 1 read as Brandenburg's own pick, the AI's, and every option could be picked. `pick_incident_option 2` answered null: the votes read 34, 26 and 17, option 2 Brandenburg's pick, option 1 still the emperor's favored. The command went through the command layer with both members under `dummy`, the tag first.
- **Not a member.** As France every option was refused with the framework's words, only the emperor and the Empire's members can pick, and so was `pick_incident_option 0`.
- **An option's own trigger.** As Austria, with France made Austria's vassal by the console's `run` of a file holding `create_subject = { subject_type = vassal subject = FRA }`, option 1 (its trigger: France neither the emperor nor a subject) was refused with the game's lines, 法兰西: 不是属国 不是神圣罗马帝国皇帝, and so was picking it.
- **The sweep**, `cargo xtask scenario sweep --sweep.only empire`: passed, 5 capabilities in 5 places, 43 requests; `empire.incident` answered in each place with a game, `pick_incident_option` was `not_found` with no incident.
- The test game was released; it did not crash.

## Not verified

- The incident's end: the emperor's choice coming into effect on its day was not waited for.
- Comparing with the Empire's view on screen: the window was not opened; the texts and counts are compared with the game's own code that writes them, not with the view.
- The refusal's lines name both of France's conditions though only one fails, as the game's failing lines of a block whose scope fails write them; not compared with the view's tooltip.
- An incident whose scope is not the emperor's, and incidents of a mod.
- Calls from an extension.
