# Religion: piety, the papacy and the curia (M18)

Tried 2026-11-11 in a test game of this checkout's (a developer build with `dev-test-games`), driven with `eu4-harmony game` as the player: new 1444.11.11 games as the Ottomans, France and the Papal States, the player's country changed with the game console's `tag`, and amounts added with effect files the console's `run` ran from the claim's user folder. Part M18 of [mod-capabilities.md](../plans/mod-capabilities.md), after [reforms, fervor, harmonization and icons](2026-11-11-religion-reforms-fervor-harmony-icons.md); the game's side is [research/religion.md](../research/religion.md#piety) and its next section.

## What was tried

- **Piety**, the Ottomans (Sunni):
  - At piety 0, `religion.options` listed `enforce_faithful_adherence` (named 教条主义, Legalism) and `call_on_religious_followers` (神秘主义, Mysticism), each costing 50 `piety`, refused with the view's own lines: "选择「忠实执行教法」需要至少向教条主义倾向75点。这将使腐败度改变-2.00。", and for Mysticism its line with "获得6,816人力。".
  - After `console run piety 90` and `piety`, Legalism's was allowed. With corruption raised to 10 by an effect file (`add_corruption = 10`), `religion.enforce_faithful_adherence` answered null; two days later corruption was 8.0, and the action was refused with piety no longer far enough and the cooldown's lines, "每5年你只能使用一次「忠实执行教法」。" and "1449年11月11日后你将可以再次使用它。".
  - After `piety -150`, Mysticism's was allowed; `religion.call_on_religious_followers` answered null and manpower went from 7592 to 14818, the 7,226 its line gave by then. A second call was refused `rejected` with its lines and the cooldown until 1449.11.13.
- **The papacy**, France (Catholic):
  - The first build failed `religion.options` `internal`, "reversed vector": the papal actions are the game's own array (its data at `+0x18`, its count at `+0x24`), not a `std::vector` as the execute's lone read of `+0x18` suggested. Read as the array, the rest of the run was made with it.
  - `religion.options` listed the eight papal actions France's `potential` lets it see (`levy_church_tax` to `sanction_commercial_monopoly`; not `investigate_heresy`), each with its cost in `papal_influence` (50, `local_saint` 100) and "你需要至少50教廷影响力来采取这一行动。", `proclaim_holy_war` with its `allow`'s "处于战争状态" too; `call_ecumenical_council` for 48.852 ducats, refused with no line; `buy_indulgence` for 128.448 ducats, "你需要128.44¤才能购买赎罪券，而你只有66.48¤"; `invest_papal_influence`, "你没有足够的教廷影响力。"; and the six golden bulls, each "你不是教廷监护。只有教廷监护有权花费教廷金库来颁布金玺诏书。", 400 `curia_treasury`.
  - With 120 papal influence and 500 ducats added (`add_papal_influence`, `add_treasury`), `religion.take_papal_action bless_monarch` answered null and France had the named modifier `papal_blessing` until 1464.11.11; a second one was refused with its `allow`'s "没有修正教宗的祝福"; `proclaim_holy_war` was refused "处于战争状态"; `no_such_action` `not_found`.
  - `religion.buy_indulgence` answered null: the treasury went from 566.487 to 438.039, the 128.448 listed, France had `bought_indulgence`, and buying again was refused "你已为洗清罪孽购买了赎罪券，现在任何数额的钱财都无法改变教宗的想法。".
  - `religion.invest_papal_influence` answered null; it was then refused "你没有足够的教廷影响力。" while `levy_church_tax`, costing 50, was still allowed: the investment costs more each time it is made (inferred from the cost function; the amount spent was not read).
- **The curia controller**, the Papal States:
  - `investigate_heresy` listed with only its 500 `curia_treasury` and its `allow`'s "改革呼声至少为5.0" and "你需要在教廷金库拥有至少500¤才能采取此行动。"; the golden bulls with their conditions (`illius_qui_se_pro_divini`'s ages, `libertas_ecclesiae`'s emperor) and "你需要400.00¤教廷金库来颁布金玺诏书，而你只有214.14¤。". The game heads the conditions with "需求：" even when none fails; the framework now leaves that header out, and a papal influence cost of 0 out of the cost.
  - With 1000 added to the curia's treasury (`add_curia_treasury`), `religion.pick_golden_bull apostolicae_servitutis` answered null and read back current, its line "这份金玺诏书已被教廷监护颁布，将在其去世前持续生效。"; `dei_gratia_rex` was then refused "同一时间只能有一份金玺诏书。", `no_such_bull` `not_found`.
  - `call_ecumenical_council`, allowed for the Papal States, answered null: the treasury went from 52.761 to 3.909, the 48.852 listed, and calling again was refused "你需要48.85¤来召开全罗马天主教普世会议，而你只有3.90¤".
- `cargo xtask scenario sweep --sweep.only religion` passed: 23 capabilities in 5 places, 188 requests. None of the four games crashed; the claim is released.

## Not verified

- Why France's call of the ecumenical council is refused with no line: the game's check (`0xdbc020`) compares the country with the tag the papacy keeps at `+0x58` and writes nothing when they differ; whether that is the Papal States or the curia controller was not told apart, since the Papal States were both.
- Whether `apostolicae_servitutis` lowered the cost of the curia's powers: `country.modifiers PAP curia_powers_cost` read 0 a day after it was enacted; when the game counts it was not followed.
- How much papal influence one investment spends, and the invested influence it gives.
- Which golden bulls the papacy view lists, and the papal action's own check (`+0x40`) the execute asks after `allow`.
- The views on screen; calls from an extension; ironman; a country under a regency or a Shia country.
