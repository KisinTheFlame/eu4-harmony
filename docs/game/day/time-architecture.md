# Time


- A day is a command. The in-game frame `0x8137c0` does two things in this order:
  1. updates the session, which executes the commands queued since the last frame (`0x813fb5`);
  2. calls the time step `0x757ad0` (`0x815312`), which posts a `daily_tick` once the frame times add up to a day ([time.md](time.md)).
- In multiplayer only the host posts days. It also waits until every player has reported the day before.
- `daily_tick`'s execute `0xb79520` calls the daily update `0x758430`, which:
  1. moves the date on;
  2. posts `client_ping` (the date and lag level a player reports);
  3. runs the day's body `0x75bbe0`;
  4. runs the monthly update `0x75d690` when the month changed, then the interface's monthly refresh (the in-game idler's `+0x2b0`);
  5. runs the yearly update `0x75e260` when the year changed;
  6. deletes the units waiting to be deleted.

**The day's body, in order:**

1. Objects waiting to be destroyed are destroyed.
2. Wars that ended (`0x759640`); pirates (`0x760250`); the active countries are rebuilt.
3. The papacy, great projects, the celestial empire.
4. The game's events due today (inferred: the event manager finds today in two lists sorted by date).
5. Two parallel passes over the countries.
6. Rebels; units (a parallel gather, then deletions).
7. **Each country's daily update `0x2ee280`, one after another**, which runs each unit's daily update (`0xd3e110`).
8. The HRE.
9. Provinces: two parallel passes, then a serial one (`0x9f52e0`: constructions, spies).
10. Battles.
11. The AI (`0x75b4b0`).
12. Military power; each war's daily update (`0xd91a10`).
13. The interface's daily refresh (the in-game idler's `+0x2a8`, `0x8339f0`).
14. Army missions; unit templates.

**Parallel work** is about ten stages inside the day and the month, over countries, provinces, units and AI countries.

- Each stage goes through `0x1798a0`, which divides the range by the number of cores and hands the parts to the worker pool.
- A stage sets the random-forbidden flag `0x1fd10c4`: drawing a random number in it logs "Calling random inside forbidden area!" and gives 0. The randomness a stage needs comes as 1000 numbers drawn beforehand on the main thread (`0x14c74c0`).
- Results are merged under a lock and applied one after another.
- Everything else is serial on the main thread: commands, the date, wars, battles, every country's and unit's daily update, the game's events and the interface.
