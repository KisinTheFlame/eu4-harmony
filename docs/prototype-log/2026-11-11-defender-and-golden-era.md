# The defender of the faith and the golden era

2026-11-11, one claim's test game, a developer build with the test games' pipes, every request from `eu4-harmony game` as the player unless it says `--as-game`. It tried `religion.defender_of_the_faith`, `religion.claim_defender_of_the_faith`, `age.get`'s `golden_era` and `age.enact_golden_era` of [mod-capabilities.md](../plans/mod-capabilities.md) (M20's second half and the rest of its first); the game's side is in [research/religion.md](../research/religion.md#the-defender-of-the-faith) and [research/institutions-ages-monuments.md](../research/institutions-ages-monuments.md#ages).

## What was tried

The save `observer-1700.1.1`, loaded as the Ottomans, then other countries through the game console's `tag` (`--as-game console run "tag VEN"`).

- **Reading the defender.** The Ottomans (Sunni) read the Teutonic Order's `TRS` as the defender and were refused with the game's prestige line; Spain, France, Brandenburg and Portugal read Austria's `AUH`; Great Britain read itself, refused with the game's "is the defender" line; Sweden and Saxony read none. (The console's `tag` took no effect for some tags, such as `DAN` and `NED`; `session.get` showed the country played, and only those reads are counted here.) Every answer cost 500 ducats.
- **The refusals' lines** were the game's own: money (`你需要500.00金钱。`), the prestige against the defender, the defender already, a female ruler where the religion does not allow her (Sweden's Kristina I, Saxony's Bianca I, Brandenburg's Amalia: `country.get`'s rulers are women), and after a claim the date before which it may not be claimed again.
- **Claiming it.** Venice, Catholic with more prestige than Austria, read `can: true`. `religion.claim_defender_of_the_faith` answered null; the defender read `VEN`, the treasury went from 6461.384 to 5961.384, and claiming again was refused with `是信仰守护者`, the prestige line and `在1702年1月01日之后才能宣称此头衔。`. Austria then read `VEN` as the defender, refused with the prestige and date lines.
- **The golden era's refusals.** The Ottomans, Spain, France, Brazil, Great Britain, Portugal and Venice read `你已经开启了黄金时代。` (had one) with `until` null; Sweden and Saxony read `开启黄金时代需要完成至少3个时代目标。` followed by the tooltip's lines of what the golden era brings (army and navy morale, costs, goods produced, absolutism, revolutionary zeal), which stay among the reasons as the tooltip writes them. Of 23 more countries (Korea to Lan Xang) none could enact one.
- **The day it ends.** After the console's `golden_age SWE` started Sweden's golden era, `age.get` read `until: "1750.1.1"`, and the reasons `我们正处于黄金时代。黄金时代将持续到1750年1月01日。`, the same day.
- **The sweep**: `cargo xtask scenario sweep --sweep.only religion,age` passed, 12 capabilities in 5 places, 93 requests.
- The claim was released; the game did not crash.

## What was not verified

- **Enacting a golden era**: no country in the save could, so `enact_golden_age={ country="…" }` was never carried out; its written form is the writer's (`0x2a4d40`), which `go_bankrupt` shares and which was tried with it.
- A religion whose group allows no defender, the technology and subject refusals, a regency and a revolutionary were not met.
- That the country's `+0x28` counts the age's objectives completed is inferred from the tooltip's line; it was not compared with the ages view.
- Extensions' calls; everything was sent from the command line.
