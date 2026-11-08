# 2026-11-05 Player actions follow-up: what the events cost, and the player's view read by read (tested by me)

Player builds of `claude/player-actions-fu-cost-view-4c4534`, after the build, the tests, clippy and the offline checks passed; the player's playset with the Chinese language mod, 3840x2160, the game started hidden within a claim (game-test skill), which backed up the player's files and the game folder's framework files and put them back at its release. Everything was read through `eu4-harmony game …` and `cargo xtask scenario perf`.

**The events' cost.** Four runs of `scenario perf` (a new game as Ming, 120 s at speed 5), two listening to nothing and two with a remote client listening to the seven events; the figures and the framework's timing are in [performance.md](../performance.md#the-events-2026-11-05). In all about 74 µs of the main thread a game day, 0.1% at speed 5; the pace spread 16% between the two runs that listened to nothing. Reading only the provinces' owners took that watch from 84 to 55 µs a day. A run that also read `province.get` of every land province stopped at the first undiscovered one, which the player's view now refuses; the scenario since reads only the provinces a country owns.

**The player's view.** A new game as Ming, the start-of-game question answered and three days run; 82 reads, each with another country (Korea, KOR, and France, FRA, which Ming has not met), another's province (Hanseong 735, Korean; Kyoto 1020 and 1028, Japanese, under fog of war; Paris 183, undiscovered) or none, written to files; then `army.at` of each of the 518 provinces a country owns that fog of war hid. The game was then closed, `remote_full_view=yes` written into the framework's `settings.txt`, and the same game started again for the same reads. Between the two starts the game itself differed in what it rolled (manpower, opinions, trade values, which rival came first): those differences are left out below.

| Read | Player's view | Full view | The game's panel |
| --- | --- | --- | --- |
| `country.get KOR` | treasury, power, governing capacity, inflation, mercantilism, loans and rebels null; manpower 9789 of 17902, stability 0, prestige -4.958, overlord MNG (tributary), rival MJZ, technology 3/3/3 | those seven given (power 126/112/112, capacity 102.43 of 400, mercantilism 10, two rebel factions at 0) | the diplomacy view: stability 0, prestige -4, overlord Ming, rival Jianzhou; no treasury or monarch points |
| `country.budget KOR` | income, expenses and loans null | Korea's budget (tax 11.036 …) | not shown for another country |
| `province.get 735` | unrest and constructions null | unrest -11.494, no constructions | the province view: development 9/10/6, fort, autonomy 0.0%, unrest 0.0: the panel shows 0.0, not Korea's -11.5 |
| `province.get 1020` | unrest and constructions null | unrest -8.9, a diplomat's construction by OGS | not opened |
| `province.get 183` | refused: not discovered | Paris, owned by France | terra incognita on the map |
| `province.list` | 4027 land provinces, 263 visible, 3250 with no owner (undiscovered or unowned) | owners for all | the map |
| `army.at` of the 518 fogged provinces | none in any | armies in 171 of them, Kyoto's (ASK, 14 regiments) and Paris's (FRA, 14) among them | none drawn under fog |
| `army.at 735` | KOR's army, 14 regiments | the same | the army's tooltip on the map: 10/4/0 regiments |
| `trade.nodes` | 26 nodes, after the fix below | 80 | the trade map mode showed the same 26, Siberia, Samarkand and Alexandria among them, Persia and Basra not |
| `trade.node venice` | refused: not discovered (after the fix) | Venice's node | not on the trade map |
| `building.options`, `missionary.options` 735 | no construction, no progress | the same here: none was going on | not compared |
| `siege.options` | no siege anywhere asked | the same | no siege going on |

The other reads of the list in [player-actions.md](../player-actions.md#the-players-view-and-the-full-view) answered the same in both views, but for what the game rolled.

- **Fixed here**: `building.options` gave what is being built in another's province and `missionary.options` another's missionary's progress, which `province.get` leaves out; `siege.options` told of a siege fog of war hides; `province.get` and `province.list` gave terra incognita as the game keeps it; the trade reads gave every node. The first build of the trade fix took a node as seen when its location province was discovered, and left out Siberia, Samarkand and Alexandria, which the trade map shows: a node is seen once one of its provinces is discovered.
- **After the run**, from reading the code: `army.at` gave another country's army the game's own state (busy, attached, movement locked) among what blocks it, which no panel shows; the player's view now gives only fighting, besieging and retreating. Not tried in the game.
- **Found by the dependency check**: one byte of the first trade dependency point was wrong, and the hidden start ended at once, naming the point.
- **Not verified**: a construction or a missionary in another's province and a siege under fog of war in the player's view (none was going on); the ledger's pages on another country (manpower is not on its diplomacy view); a country the player has not met, which every read answers as any other; extensions, which read the full view.
- **Cleanup**: the release put back the player's files and the framework settings; the screenshots and the read files were in the scratchpad.
