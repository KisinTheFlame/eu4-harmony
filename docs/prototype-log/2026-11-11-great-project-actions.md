# Building, moving, cancelling and boosting great projects

Part of M20 of [mod-capabilities.md](../plans/mod-capabilities.md): the `monument` group's `buildable`, `build`, `evaluate_move`, `move`, `constructions`, `cancel` and `boost`, on a branch of `claude/mod-capabilities-gaps-899e75` at `4bc8d5e7`. The build had the test games' pipes. One test game of this worktree's, started hidden, with the player's playset (the Chinese one); the save `observer-1700.1.1` of the late game test saves, loaded as the Ottomans (`player="TUR"`), and other countries put in play with the game console's `tag`. Every request went through `eu4-harmony game` as the player; the console's `cash` gave the money the actions needed.

## What was tried

- **Building**: `monument.buildable 4316` (Suez) listed the Suez canal, 苏伊士运河, a canal, refused at first with "无法负担所须的20060.00¤。" against a treasury of 752.50; `monument.buildable 1775` (Holstein) the Kiel canal, refused with "奥斯曼与它的任意属国都未拥有省份荷尔斯泰因。"; Constantinople none. After `cash 50000` it offered the Suez canal for 20060 with no reason; `monument.build suez_canal 4316` answered null, the treasury went from 50752.50 to 30692.50, 20060 less, and `province.get 4316` listed a `great_project_construction` from 1700.1.1 to 1709.11.12. Building it again was refused with "东部省现在正忙于建造苏伊士运河。 于1709.11.12完成。", building it in Constantinople with "不能在此建造。", an unknown key `not_found`.
- **The constructions**: `monument.constructions` listed the Ottomans' five, the four upgrades the save had (Sultan Ahmed mosque, the city of Sana'a, Nizwa fort, Petra) and the canal, each with its province, start and end, cancelling as a `Choice` that can, and boosting for 10000 manpower and for 250 ducats.
- **Boosting**: `monument.boost suez_canal ducats` answered null, the treasury went down 250 and the canal's end from 1709.11.12 to 1707.11.12; `monument.boost suez_canal manpower` took 10000 of the 301514 men and brought it to 1705.11.12. `with` `gold` was `invalid_argument`, a monument not under construction `not_found`.
- **Cancelling**: `monument.cancel suez_canal` answered null, the treasury went from 30442.50 to 40472.50, 10030 back, half of the cost; the province's constructions were empty after, cancelling again was `not_found`, and the Suez canal was offered for building again.
- **Moving, to the capital**: as the Ottomans, Hagia Sophia in Constantinople was refused "hagia_sophia is in the capital already" (the framework's words; the game's check refuses the province the project stands in without one), and so was Stonehenge as Great Britain, in London. As Transoxiana the Bamiyan Buddhas were refused with "你从巴米扬到撒马尔罕没有可用路线。". As Vijayanagar the Gomateshwara statue (巴胡巴利雕像) in Seringapatam was offered for 1056.84 ducats; `monument.move gomateshwara_statue` answered null, the treasury went down exactly that, a construction ran until 1700.12.19, the move was refused after with "巴胡巴利雕像正被搬迁至毗奢耶那伽罗", and `monument.cancel` gave back all 1056.84.
- **The move check and other countries' provinces**: before the moves went to the capital, a build whose move took a target province offered Stonehenge's move from London to Constantinople, the Ottomans', and to Stockholm, Sweden's, and moved it to Stockholm for 1092 ducats ("巨石阵正被搬迁至斯德哥尔摩"): the game's check does not ask whose the target is. The window offers only the capital, so the capabilities do too.
- **Two calls first wrong**: the build cost first faulted, called with three arguments where it takes five (the province and an explanation), and the move cost first faulted, called with the province where the move check passes the country and then the province; both were read again from the game's code and fixed before the rest of the run.
- **Sweep**: `cargo xtask scenario sweep --sweep.only monument` passed, 9 capabilities in 5 places, 118 requests.
- The test game did not crash.

## What was not verified

- A boost refused: no construction had ended and no country lacked the manpower or the money, so `CANNOT_BOOST_PROGRESS_FINISHED` and the two not-enough lines were not seen.
- A canal built to its end, and a canal of a country without the expansion (`0x9751b0`, inferred to be the expansion's check).
- The window's boosts of `type` 2 and 3, over and over, are not offered; what sets them was not read.
- A cancel refused: the game's check asks only that the construction is the player's, which `monument.constructions` lists alone.
- The monuments window was not opened on screen; what it offers was compared with nothing but the game's checks, its tooltip texts and what each action paid.
- Another country in view (outside the sweep's shape checks) and an ironman game.
