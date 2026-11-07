# Prototype test log

What was tested in the running game and what came out, one file per run. A run's file says what was built and how the game was set up for it (the branch or build, the save, the mods and the extensions), what was tried, what came out, with the numbers and the log lines it rests on, and what was not verified, as plainly as what was.

## Naming

A run's file is named by the day of the run and a short slug of its title, `YYYY-MM-DD-<slug>.md` (`2026-11-04-extension-reload.md`), with no number, so that branches written side by side each add a file of their own and do not meet in one. Its title, the file's `#` heading, is the day and what the run tried, as `# 2026-11-04 Extension reload (tested by me)`. Pick a slug no file of that day has.

A later run that changes what an earlier one found gets a file of its own and links to the run it follows. This folder keeps no list of the runs, which would bring back the one place every branch changes: the files' names sort by day.

Refer to a run by a link to its file, as [the extension reload](2026-11-04-extension-reload.md), and name its title only where the link's text does not say which run it is.

## The baseline

Unless a run says otherwise, it was on vanilla 1.37.5 (`9ad3efe1…`), the baseline of [docs/research/](../research/). The runs before the first version was built checked the items under "Order of work" in [v1.md](../v1.md); what the implementation came to differ from that design is at the end of v1.md.
