# Player actions plan

Status: built (2026-11-05), every part; what each did not verify is in its section below and in [prototype-log.md](prototype-log.md). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Content key, Player's view, Full view, Capability, Command). The decisions and their reasons are in [ADR remote-control-reads-the-players-view](adr/remote-control-reads-the-players-view.md), and build on [ADR one-capability-catalogue](adr/one-capability-catalogue.md), [ADR one-command-layer-in-written-form](adr/one-command-layer-in-written-form.md) and [ADR remote-control-uses-capabilities](adr/remote-control-uses-capabilities.md). How a capability is added, layer by layer, is [adding-a-capability.md](adding-a-capability.md).

## Goal and scope

- **Goal**: an extension or a remote client can play a whole game as the player does: everything the player does in the game's panels, with what the player reads there, and the events that tell what happened.
- **In scope**: the capabilities below, part by part; the player's view and the full view; the events named in the parts.
- **Out of scope**: multiplayer; a player without every DLC (the framework takes the player to have them all: without one, what the game's interface does not offer the player is refused by the game's own check, with the reason the game gives, or none); offering the game's commands themselves (ADR one-command-layer-in-written-form).

## Behavior

### Capabilities follow the game's panels

- Each capability group is one of the game's panels or views, such as the province view, the estates view, the diplomacy panel, the peace view; each of the panel's buttons is a capability of its own, named after what the button does (`estate.grant_privilege`, `decision.take`, `diplomacy.warn`).
- What the player chooses among is named by its content key: a building `marketplace`, a privilege, a decision, a reform, an idea group, an edict. What a mod adds is then named as the game's own is, and is listed and taken the same way.
- Each panel has a read of what its buttons offer now, named `options` (`province.options`, `diplomacy.options`), or `list` where the panel is a list of things of a kind (`decision.list`, `estate.list`). Each entry the player could choose says whether the player can, why not in the game's own words, and what it costs: a `Choice` (below).
- A panel's state that is large is read in its own group (`estate.list`, `idea.list`, `loan.list`, `country.budget`, `trade.node`); `country.get` and `province.get` keep the summary a player sees at a glance, grown with the fields this plan names.

### Choices, reasons, costs

- A `Choice` is `can` (true or false), `reasons` (the game's lines for why not, as its tooltip shows them, in the player language, without color codes or bullets; empty when it can, or when the game gives none) and `cost` (a list of amounts, each a `resource` and an `amount`: ducats, adm, dip or mil points, manpower, sailors, prestige, an envoy of a kind, and the rest as the parts need them).
- Every action refuses with the game's reasons, the same `reasons` its `Choice` gives (`rejected`, the reasons one a line). A command the game's own check refuses with no reason the framework found is refused saying so, as recruiting does.
- An action whose button first asks the player to confirm a price (revoking a privilege, breaking a truce, disbanding) is carried out as a player who confirms: the price is in its `Choice` and its description.
- An action is done when it returns: the command layer has the game execute it at once (ADR commands-run-at-once). An action that waits for another country's answer (a proposal) answers with that answer, as `diplomacy.propose_alliance` does.

### Would they accept

- Where the game's interface tells the player whether another country would accept (a diplomatic proposal, peace terms, a call to arms, a subject interaction), the answer carries an `acceptance`: `accepts`, `total`, and `factors`, each a `text` as the interface shows it and a `value`.
- An action that takes no arguments beyond the country has its `acceptance` in its `options` entry; an action that needs no answer has none.
- An action whose arguments are composed, such as peace terms, has an `evaluate` capability that takes the same arguments as the action and answers its `Choice` and `acceptance` for that composition.
- `acceptance` is what the player's interface says, so it is the same in both views.

Two cases set the shape; the others follow them:

```
eu4-harmony game diplomacy options CAS
→ {"country":"CAS","actions":[
    {"action":"propose_alliance","can":true,"reasons":[],"cost":[],
     "acceptance":{"accepts":false,"total":-12,"factors":[{"text":"…","value":-10}, …]}},
    {"action":"warn","can":true,"reasons":[],"cost":[]}, …]}

eu4-harmony game peace terms ENG
eu4-harmony game peace evaluate ENG --terms '[{"term":"demand_provinces","province":236},{"term":"humiliate"}]'
→ {"can":true,"reasons":[],"cost":[{"resource":"war_score","amount":27}],"acceptance":{"accepts":true,"total":8,"factors":[…]}}
eu4-harmony game peace demand ENG --terms '…'
```

### The player's view and the full view

- Extensions read the full view. Remote control reads the player's view unless the framework setting "Remote control sees everything" is on (off as installed); the setting is in the settings window.
- Under the player's view a read leaves out what the player's interface does not show: fields of another country its panels do not show are null, and what fog of war hides is not listed, as `army.at` does now. Each part says, for each read it adds, what the player's view leaves out.
- `army.at` gives extensions the armies under fog of war too.

What each read leaves out in the player's view, read by read, as compared in the game on 2026-11-05 (a new game as Ming, each read under both views and beside the game's panels: [prototype-log.md](prototype-log.md)). "Another's province" is one another country owns; a province no country owns counts as the player's own. "Undiscovered" is terra incognita to the player, as the fog of war refresh reads it ([research/fog-of-war.md](research/fog-of-war.md)).

| Read | Left out in the player's view |
| --- | --- |
| `country.get` | of another country: `treasury`, `power`, `governing_capacity`, `inflation`, `mercantilism`, `loans`, `rebels` (null) |
| `country.budget` | of another country: `income`, `expenses`, `loans` (null) |
| `province.get` | of another's province: `unrest`, `constructions`, `missionary` (null); an undiscovered province is refused (`not_found`) |
| `province.list` | of an undiscovered province: `owner`, `controller` (null); `visible` is false under fog of war |
| `army.at` | the armies in a province fog of war hides (none listed); of another country's army, what stops it taking orders beyond fighting, besieging and retreating (`blocked` gives no exiled, movement_locked, attached, has_followers or busy) |
| `building.options` | in another's province: what is being built, and the offer to cancel it |
| `missionary.options` | in another's province: the progress of a missionary not the player's |
| `siege.options` | in a province fog of war hides: the siege (refused as no siege) |
| `trade.nodes`, `trade.node` | the nodes none of whose provinces the player has discovered (left out; `trade.node` refuses one, `not_found`) |
| event `battle` | a battle the player takes no part in, in a province fog of war hides |
| `country.hostile`, `province.neighbors`, `province.options`, `recruit.options`, `colonist.options`, `trade_company.options`, `center_of_trade.options`, `occupation.options`, `diplomacy.options`, `diplomacy.war_goals`, `peace.terms`, `peace.evaluate`, `subject.options`, `merchant.options` | nothing: what they give of another country or province its panel shows, and the rest are the player's own buttons, with the game's own reasons |
| `army.get`, `army.list`, `army.options`, `army.route_to`, `army.can_step`, `army.can_attack`, `fort.list`, `merchant.list`, `mercenary.list`, `estate.list`, `estate.interactions`, `technology.options`, `idea.list`, `policy.list`, `government.options`, `government.reforms`, `parliament.options`, `mission.list`, `decision.list`, `focus.options`, `stability.options`, `economy.options`, `loan.list`, `leader.options`, `leader.list`, `offer.list`, `question.list`, `missionary.automatic` | nothing: they read only the player's own country, armies and panels |
| events `building_finished`, `war_declared`, `peace_made`, `province_owner_changed`, `month`, `year` | nothing: they are about every country, as the plan has them |

Not hidden yet: a country the player has not met (its `country.get`, `diplomacy.options` and the rest answer as for any other), and a flow in `trade.node` toward a node the player has not discovered, which names that node.

### Events

- New events are added with the part whose topic they are: `month` and `year` (foundations), a building finished (province administration), a battle fought (armies), a war declared and a peace made, a province changing owner (war and peace). They are about every country, each naming the countries it concerns; a battle only where the player sees it (the full view: every battle).
- An event kind no extension has a handler for, and no remote client has read since the game started, is not worked out at all.
- Remote control's `events` takes `kinds` and `countries`, and answers only the events of those kinds that name one of those countries; an event that names no country, such as `day`, passes any `countries`.
- What the events cost the game is measured in each part that adds one, into [performance.md](performance.md). Measured for the seven together (2026-11-05, [performance.md](performance.md#the-events-2026-11-05)): about 74 µs of the main thread a game day while all of them are listened to, most of it the provinces' owners compared day by day, about 0.1% at speed 5; nothing when nobody listens.

## How it fits together

- **The catalogue** (`remote/`): each new group is a file of its own, with its capabilities and results; `Choice`, `Amount` and `Acceptance` are shared types of the catalogue's (`remote/src/choices.rs`).
- **The capability layer** (`core/src/game/`): a file per group. A read takes the caller's view (`game::view(caller)`); an action checks as the panel does, giving the game's reasons, writes its command and posts it through the command layer (`game::commands::post`).
- **Reasons from script conditions**: decisions, missions, privileges, reforms, policies, ideas, parliament issues and subject interactions are allowed by conditions in script; one module (`game::conditions`) writes a condition's failing lines as the game's tooltip does, for every group to use ([research/triggers.md](research/triggers.md)).
- **Objects**: what the framework reads of the game's countries and provinces is in [research/countries.md](research/countries.md) and [research/provinces.md](research/provinces.md); each field read is a dependency point.

## Parts

Each part after the foundations is built in a session of its own, in its own worktree and branch, merged into one integration branch that reaches master in one pull request; the parts may run side by side, the game shared through the claim (AGENTS.md, "Testing in the game").

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`, and so do the offline checks against `eu4.exe`;
- every capability it adds was tried in the game, its refusals among them, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- docs/research holds what was found in the game's code, every new place in it is a dependency point, its section below lists the capabilities it added, and README.md and README.zh-CN.md say what changed for players.

### A0 Foundations

This session, before the rest: this plan, [adding-a-capability.md](adding-a-capability.md), `Choice`, `Amount` and `Acceptance`; the player's view and the full view, with the setting; `game::conditions`, with `decision.list` as the first group on it and the example the other parts follow; events not worked out when nobody listens, `events` filtered by `kinds` and `countries`, and `month` and `year`; the countries' and provinces' fields found ([research/countries.md](research/countries.md), [research/provinces.md](research/provinces.md)).

### A1 Reading countries, provinces and trade

`country.get` grown with technology levels, ideas, governing capacity, inflation, loans, rebels; `country.budget` (income and expenses as the ledger shows them); `province.get` grown with autonomy, culture, religion, unrest, trade good, trade node, state or territory, cores and claims, the construction queue, the missionary's and colonist's progress; `trade.nodes`, `trade.node` (value, flows, merchants). What the player's view leaves out of each.

Done: `country.get` has `technology`, `ideas` (each group with the ideas taken, the national ideas first), `governing_capacity` (used and its maximum, as the game's own share works them out), `inflation`, `mercantilism`, `loans` (count and ducats) and `rebels` (each faction's type, name and progress); `country.budget` gives this month's income and expenses, every category in the economy view's order with its key and its name in the player language, and each loan with its lender, interest and due date; `province.get` has `autonomy`, `culture`, `religion`, `trade_good`, `unrest`, `trade_node`, `center_of_trade`, `trade_company`, `state` (`state` or `territory`) with its `edict`, `cores`, `claims`, `constructions` (each kind, country, progress, start and end, and the building or unit it makes), `missionary` and `colony_size`; `trade.nodes` lists every node with its value, and `trade.node` gives a node's value (as the trade node view's total: incoming and local less outgoing), its local value, the flows in and out on each link, and every country with power there, its share, whether it collects, its home node and its merchant ([research/trade.md](research/trade.md)).

What the player's view leaves out follows the game's panels: the diplomacy view of another country shows its technology, ideas, stability, prestige and manpower but not its treasury or monarch points, so for another country `treasury`, `power`, `governing_capacity`, `inflation`, `mercantilism`, `loans` and `rebels` are null, and `country.budget` answers nulls; the province view of another country's province shows its unrest as 0.0 whatever it is (Hanseong's was -11.5, 2026-11-05), so `unrest`, `constructions` and `missionary` are null there; the trade node view shows every country, so the trade reads leave nothing out. Since 2026-11-05 an undiscovered province (terra incognita) is refused by `province.get` and listed by `province.list` without owner or controller, and the trade reads leave out the nodes the trade map does not show (the table under [The player's view and the full view](#the-players-view-and-the-full-view)).

### A2 Monarch points and the treasury

Researching technology; choosing and cancelling the national focus; raising stability; reducing inflation; raising mercantilism; taking and repaying loans. Each with its `Choice` in an `options` or `list` read.

Built 2026-11-05 ([research/power-spending.md](research/power-spending.md); tried in the game, [prototype-log.md](prototype-log.md), "Player actions A2"):

- `technology.options` (the three technologies, each level and its next level's `Choice`), `technology.research` (`adm`, `dip`, `mil`): the technology view's research button, the game's `spend_power_command`.
- `focus.options` (the focus set, choosing each kind, cancelling), `focus.choose`, `focus.cancel`: the court view's focus buttons, `set_national_focus_command`. Choosing the focus set already is refused by the framework (in the game that button removes it, which is `focus.cancel`).
- `stability.options`, `stability.raise`: the stability and expansion view's button.
- `economy.options` (`take_loan` with the loan's ducats in `gives`, `reduce_inflation`, `raise_mercantilism`), `economy.take_loan`, `economy.reduce_inflation`, `economy.raise_mercantilism`: the economy view's buttons and the trade view's mercantilism button, in one group because they are the treasury's spending.
- `loan.list` (each loan with its repay `Choice`, the price the amount and the interest left), `loan.repay`: the loan window, repaying as a player who confirms.
- The argument kind `power_kind`; `game::power` builds the game's power spending for its cost and command, and writes tooltip lines with the game's localisation and values, for later parts' spending (developing, coring, culture) to use; the conditions of `common/scripted_functions` (`can_increase_stability`, `can_reduce_inflation`, and those A3 needs) give their failing lines.
- Verified in the follow-ups run (2026-11-05, [prototype-log.md](prototype-log.md)): the bank loans' limit (Muscovy refused at its 56th, the second reason then losing its `- `), mercantilism at its most, every technology at its last level (its `options` entry now says so, as research does).
- Not verified: a refusal by the rule against ruining a nation, by `can_increase_stability` or `can_reduce_inflation` (empty in the unmodded game), by `may_not_reduce_inflation`, a loan from another country or an estate, and remote control's player's view (every read here is the player's own country). The focus's text is the game's; a mod's own `tooltip` on a scripted function's condition is not shown.

### A3 Province administration

Developing and exploiting development; raising and lowering autonomy; making a state or a territory; setting and cancelling state edicts; coring and cancelling a core; changing culture; building, cancelling and destroying buildings. The event of a building finished.

Built and tried in the game on 2026-11-05 ([prototype-log.md](prototype-log.md); what was found in the game's code is [research/province-view.md](research/province-view.md)):

- **The province view**, in the `province` group, one capability a button: `develop_tax`, `develop_production`, `develop_manpower`, `exploit_tax`, `exploit_production`, `exploit_manpower`, `raise_autonomy`, `lower_autonomy`, `make_state`, `make_territory`, `set_edict` (an edict by its content key), `cancel_edict`, `core`, `cancel_core`, `change_culture` (to the player's primary culture, as the button does) and `cancel_culture`; and `province.options`, each of them with its `Choice`, setting an edict one entry an edict. Each refuses with the reasons its button's tooltip gives, from the game's own check; the prices are the game's, in monarch points.
- **The building panel**, a group of its own, `building`: `building.options` (each building the game has, to build where it does not stand and to destroy where it does, with its price in ducats and the game's reasons; what is being built, its progress, end and builder; cancelling it), `building.build`, `building.cancel` and `building.destroy`, a building by its content key.
- **The event** `building_finished` (province, building, the province's owner), about every country, worked out only while someone listens; its Lua handler `on_building_finished`.
- **Decided here**: exploiting, which asks the player to confirm, is carried out as a player who confirms; the culture button converts to the primary culture, so `change_culture` takes no culture; where the game's own check leaves out what its button asks (the culture button's points, a culture that is the primary one already), the framework asks it as the button does; the state and culture commands carry no country and act for the province's owner, so the framework refuses them for a province the player does not own, as it does destroying a building, which no check of the game's refuses. These reads are the player's own buttons, which the player sees for any province, and differ between the views only in what `building.options` says is being built in another country's province, which the player's view leaves out (since 2026-11-05).
- **Verified in the follow-ups run** (2026-11-05): cancelling a building (the price given back whole, the outliner's construction gone), abandoning a state (a territory at 50 autonomy), and abandoning the capital's state, refused with the button's own reason ("不能取消首都所在的直属州。").
- **Not verified**: an extension's `on_building_finished`; cancelling another country's building and destroying under siege, whose refusals come from the command's check without a reason; the edict's year after it is set is the game's own rule.

### A4 Armies and fleets

Fleets moving, `army.move` taking `transports` for a goal across the sea, armies embarking and landing; merging, splitting and disbanding; recruiting and assigning leaders; hiring mercenaries; bombarding, naval bombarding and assaulting; mothballing and reopening forts; drilling; consolidating regiments; fleet missions. The event of a battle fought.

Added (tried in the game, [prototype-log.md](prototype-log.md), 2026-11-05; found in the game's code in [research/leaders.md](research/leaders.md), [research/units.md](research/units.md), [research/sieges.md](research/sieges.md) and [research/battles.md](research/battles.md)):

- `army`: `army.move` and `army.route_to` for fleets too, and `army.get` of a fleet with its sea zone, ships and route; `army.move` with `transports`, as the right click's automatic transport (`start_auto_fleet_transport`, with the fleets the game finds for it); `army.options` (split, merge with the player's others of its kind in its province, disband, consolidate, drill or stop_drill, a fleet's patrol and evade), each with the game's own answer and reasons where the game has a function for them, and the command's check otherwise (split, patrol, evade); `army.split` (in half, as the button), `army.merge`, `army.disband` (as a player who confirms), `army.consolidate`, `army.drill`, `army.stop_drill`, `army.mission` (protect_trade, privateer and hunt_pirates by a trade node's number; hunt_fleets, blockade and intercept by a sea region's number; patrol and evade toward where the fleet was moved) and `army.stop_mission`.
- `leader`: `leader.options` (general, admiral, conquistador, explorer, with the cost in mil or dip points and the tooltip's reasons), `leader.recruit` (with a name the game picks), `leader.list`, `leader.assign`, `leader.remove`, `leader.dismiss`.
- `mercenary`: `mercenary.list` (content key, name, regiments, hiring cost, upkeep, the game's reasons) and `mercenary.hire` in a province the caller picks, as the player picks one on the map.
- `siege`: `siege.options`, `siege.bombard`, `siege.naval_bombard`, `siege.assault`: the game's spending of military points (`spend_power_command`), with the game's answers and reasons and the cost its payment counts.
- `fort`: `fort.list`, `fort.mothball`, `fort.reopen` (the province view's maintenance box, `mothball_fort`).
- The event `battle` (province, naval, attackers, defenders, winner, the losses in men or ships), heard at a combat's results once per battle and worked out only when someone listens; remote control in the player's view hears the battles the player takes part in or whose province fog of war does not hide, in the full view every one, as extensions do; Lua's `on_battle`.
- For every part: `commands::passes`, whether a command would pass its check, for a button the game has no answer for besides its command's check; `game::reasons`, a tooltip's lines without their bullets; `game_string::unformatted`, a localisation text with its values unfilled; and the `armies` kind of argument, a list of army IDs.

Decided here: the siege view's buttons are a group of their own (`siege`), as is the fort box (`fort`), each named after what the player does; a mission's target is the game's number for its trade node or sea region, and no read lists them yet; hiring takes the province, which the player picks; a leader is named by its number (`id`), which the game writes `{ id=N type=49 }`.

Followed up (tried in the game, [prototype-log.md](prototype-log.md), "Player actions follow-up: armies aboard fleets"): an army boards a fleet of the player's by `army.move` onto its sea zone, which the game allows only when one fleet there has room for the whole army; `army.get` says which fleet an army is aboard (`aboard`) and a fleet's regiments aboard (`carrying`); armies aboard move with their fleet and land by `army.move` to a coastal province; a move onto the sea that finds no route says when the player has no fleet there, or none with room, and the most room one has.

Not verified in the game: boarding a fleet that is in a battle (no naval battle came about in the run); an artillery barrage carried out; what a fleet mission does once taken; remote control hearing a battle the player only sees; the full view's every battle; an extension's `on_battle`. The event's cost was measured on 2026-11-05: 1.1 µs a combat's results ([performance.md](performance.md#the-events-2026-11-05)). Since then `siege.options` in the player's view finds no siege in a province fog of war hides; not tried in the game.

### A5 War and peace

Handing over occupied provinces; calling allies into a war; peace terms (`peace.terms`, `peace.evaluate`, `peace.demand`, `peace.offer`), every term the peace view offers. The events of a war declared, a peace made, a province changing owner.

Added ([research/peace.md](research/peace.md), tried in the game: [prototype-log.md](prototype-log.md), 2026-11-05, A5):

- **`peace`**, the peace view: `peace.terms` (every term the view offers, on the demand tab or with `offer` the offer tab, each a `Choice` with its war score cost; `gold` with the most ducats the view lets the player set), `peace.evaluate` (a composition's `Choice`, its war score cost and the AI's `acceptance` with its factors, as the send button and the accept or decline icon tell it), `peace.demand` and `peace.offer` (sent as the send button sends it, answered as `diplomacy.offer_peace` is). A term is named by the game's key without `po_` (`demand_provinces`, `humiliate`, `gold`) or a scripted peace treaty's key, with the `province`, the other `country`, the `region` or the `amount` that tell it apart; the argument is the new kind `terms`, a JSON list. The plan's example, which named a province term `province`, names it so now.
- **`occupation`**, the transfer occupation window: `occupation.options` (the button's `Choice` and the allies the window lists) and `occupation.transfer`, the game's own transfer command.
- **`diplomacy.call_to_arms`**: the diplomacy panel's call to arms in the player's wars, refused with the panel's reasons, answered with the ally's answer. Since A6 it is one of the diplomacy panel's acts (`game::diplomacy::act`), and `diplomacy.options` gives its `Choice` and `acceptance`, asked as the panel asks them.
- **Events** `war_declared`, `peace_made` and `province_owner_changed`, about every country and naming the countries they concern, with `on_war_declared`, `on_peace_made` and `on_province_owner_changed`: the world's wars and the provinces' owners compared with the day before, at each new day, only while someone listens, so a peace, an annexation or the console are heard alike, at the new day after it. Their cost, measured on 2026-11-05 ([performance.md](performance.md#the-events-2026-11-05)): 19 µs a day for the wars and 55 µs for the owners.
- **The player's view** leaves nothing out of these: the peace view and the transfer window show the player all they list, and the events are about every country, as the plan has them.
- **Verified in the follow-ups run** (2026-11-05): an ally accepting a call to arms (Circassia, `accepted`, then in the war) and a separate peace with an enemy's ally (East Chagatai and its subjects left the war).
- **Not verified**: scripted terms, colonial regions and the terms France was not offered in the run; extensions' calls and handlers. Demanded gold arriving short (300 asked, 228.7 paid) is the game sharing it among the player's side by participation ([research/peace.md](research/peace.md)).

### A6 Diplomacy and subjects

Every action the diplomacy panel offers the player, with `diplomacy.options`; rivals; annexing and integrating subjects and cancelling it; subject interactions.

Added (research in [research/diplomacy.md](research/diplomacy.md#the-diplomacy-panel) and [research/subjects.md](research/subjects.md); the run in [prototype-log.md](prototype-log.md), 2026-11-05):

- `diplomacy.options <country>`: what the panel offers toward a country, made by the game's own list maker as the panel makes it, in its order, each entry `action` (the capability that sends it), `key` (the type's key in saves, or a scripted action's content key), `name` (the button's text), the `Choice` (reasons from the panel's and the window's checks), `description` (what the tooltip says after the reasons) and, where the panel shows it, `acceptance` (the factors the other country's AI weighs, their sum, accepting at 1 or more). `cost` stays empty: the panel shows no price beside its buttons; what an action costs is in its description. The same in both views: the panel shows all of it.
- A capability for every button of the panel, 113 of them, named after what the player does (`cancel_alliance`, `arrange_royal_marriage`, `break_royal_marriage`, `improve_relations`, `fabricate_claim`, `guarantee`, `ask_military_access`, `offer_fleet_basing`, `revoke_military_access`, `embargo`, `warn`, `influence`, `send_gift`, `subsidize_war`, the `trade_favors_*`, `sell_province`, `offer_condottieri`, `support_independence`, `support_rebels`, `insult`, `threaten_war`, `claim_throne`, `transfer_trade_power`, `offer_vassalization`, `share_knowledge`, `annex`, `integrate`, `cancel_annexation`, `cancel_integration`, …), and `scripted_action` for `common/new_diplomatic_actions` by content key. Each sends the panel's own action, with what the window it opens has the player choose written into it as that window does (a province, a country, an army, an amount, a price); a proposal is answered `accepted` or `declined`, any other action `done`. `call_to_arms` is A5's capability, sent as one of the panel's acts.
- `diplomacy.add_rival` and `diplomacy.remove_rival`, the game's own commands, refused with the reasons the game's rivals check gives.
- `subject.options <subject>` and `subject.interact`: the game's 41 subject interactions (by their keys, such as `scutage`), those the player's country may use and the subject's type allows, with whether each goes on (`active`; carrying it out again stops it), the reasons its conditions give, and the game's `subject` command.
- Decided: the panel's actions are named by what the player does, not by the game's keys, which `options` gives too; an action whose window has the player choose something takes it as an argument and refuses `invalid_argument` without it; a window whose choice the framework does not set yet (the maps' region, the ships sold, a federation's name) sends the action as the panel made it, and the game's check refuses it when it needs the choice. `support_rebels` takes the faction by its kind (`rebels`, as `country.get` lists a country's rebels), since without one the game's check let it through and nothing was supported (follow-ups run, 2026-11-05); in the player's view `country.get` leaves another country's rebels out, though the window lists them, so a remote client names a kind without seeing which the country has.
- Verified in the follow-ups run (2026-11-05): annexing a vassal and integrating a personal union, and cancelling each (Muscovy and Pskov, Hungary and Croatia, the outliner's progress the same); `subject.interact` with a province (`grant_province`, `grant_core_claim`, `seize_territory`, the command's `index`); `subsidize_war`, `support_rebels` with its faction and `share_maps`.
- Not verified in the game: most actions beyond those the runs sent (each goes the same way); `threaten_war`, `break_alliance`, `transfer_trade_power` and the favors carried out (each refused for want of a diplomat in the run); what `share_maps` shares with no region chosen (Castile accepted it); `return_land` and `fortify_march`, which take a province as the others do; `upgrade_subject_type` (the upgrade's key, not passed) and `start_colonial_war` (an `index` not worked out); a third country's interactions; Lua extensions calling any of it.

### A7 Government

Idea groups and ideas; policies; government reforms; raising the government's rank; estates (privileges, interactions); parliament; decisions; missions.

Added (2026-11-05; where the game keeps each and the commands' written forms in [research/government.md](research/government.md)):

- `idea.list`, `idea.take_group`, `idea.buy`, `idea.abandon_group`: each group's button is taking it, or buying its next idea at its cost in monarch points (the game's own cost for the spend command); the national ideas and the database's placeholder group are not listed.
- `policy.list`, `policy.enable`, `policy.disable`: the reasons are what the game's policy check writes, the conditions that hold left out (`conditions::refusal`).
- `government.options` (type, rank, reform progress, the reforms adopted, whether the rank can be raised with the game's reasons), `government.reforms` (tier by tier, each reform's cost in reform progress, `Resource::ReformProgress`), `government.adopt_reform` (enacting in the next tier, or changing a reached tier's reform, as the view's two commands do), `government.raise_rank` (with the prestige the button gives).
- `estate.list` (loyalty, influence, land share, the privileges the view lists, each with its grant or revoke Choice and the game's reasons), `estate.interactions` and `estate.interact` (the crown land's `sale_of_titles`, `summon_the_diet`, `seize_land`, with their cooldown in the game's words), `estate.grant_privilege`, `estate.revoke_privilege` (revoking takes the price the game takes).
- `parliament.options`, `parliament.debate`, `parliament.add_seat`, `parliament.remove_seat`.
- `decision.take`, beside A0's `decision.list`.
- `mission.list` (column by column), `mission.complete`.

Decided: what the game's interface shows as a picture or not at all is said in the framework's words: no free idea group slot, too few monarch points for an idea, too little reform progress, a debate going on, the missions before one not completed. A disabled policy's minimum time is refused by the game's command with no reason. Every read is of the player's own panels, so the player's view leaves nothing out. Not offered: estate agendas, parliament's bribes, government mechanics' interactions (their commands are in the research).

Verified in the follow-ups run (2026-11-05): England's parliament (a debate begun, the others then refused, the parliament view showing the same debate and six seats with the one added), a mission completed (`eng_recruit_the_mercenaries`), the rank raised to an empire, the estates' influence and land against the estates view (the view rounds down: 22.6 shows 22%). Fixed after it, not seen again in the game: an issue's name that refers to another key (`grant_consessions_to_the_church`'s `$ESTATE_CHURCH$`, which read "向让步") is filled in; the rank's reasons lose the check's `- ` before their own full-width dash.

Not verified in the game: taking a seat away, which the game's check refused in every try (during a debate and without one), for a reason not established; changing a reform for free (the framework always posts the change paying; when the view posts it free was not found); an extension calling the capabilities.

### A8 Religion, colonies and trade

Missionaries and the number of automatic missionaries; colonists; trade companies and their investments; centers of trade; merchants.

Added (what was found is in [research/envoys.md](research/envoys.md), the run in [prototype-log.md](prototype-log.md)):

- `missionary.options`, `missionary.send`, `missionary.recall` (the province view's missionary button, its cost a missionary and its upkeep a month), `missionary.automatic`, `missionary.raise_automatic`, `missionary.lower_automatic` (the religion view's automatic missionaries).
- `colonist.options`, `colonist.send`, `colonist.recall` (the province view's colonize buttons; the cost a colonist and the colonial upkeep a month its work adds, as the game counts it for the province now), `colonist.expel_minorities` (the colonize button's list of the player's provinces whose minority it can expel into a new colony in a colonial region), `colonist.start_settlement_growth`, `colonist.stop_settlement_growth` (the province view's settlement growth button).
- `trade_company.options`, `trade_company.add`, `trade_company.remove`, `trade_company.invest` (the province view's trade company button, and the investments of the area of a province in a trade company, by content key, with their cost in ducats).
- `center_of_trade.options`, `center_of_trade.upgrade`, `center_of_trade.downgrade` (the province view's center of trade button: ducats up, prestige down).
- `merchant.list`, `merchant.options`, `merchant.collect`, `merchant.steer`, `merchant.recall`, `merchant.steer_to` (the trade view, nodes named by their content key; sending the merchant there again changes its mission), `merchant.policies`, `merchant.set_policy` (the trade view's trading policies, by content key, the cooldown after a choice in the game's words).
- Decisions: the reasons are the game's tooltip functions' lines, the lines that hold left out where a function writes them; where a panel decides on its own (a merchant already collecting, steering in the home node, no free merchant), the reason is the game's text for it. The actions with a confirm dialog (recalling a missionary, the trade company, investments, centers of trade) are carried out as confirmed. These reads are of the player's own things and are the same in both views, but for `missionary.options` in another country's province, where the player's view leaves out the progress of a missionary that is not the player's (since 2026-11-05).
- Done in a follow-up: the trading policies, the colonial upkeep, expelling minorities and settlement growth. `merchant.policies` lists every policy of the game's, where the trade view shows those whose conditions hold for the country; the upkeep is the game's own count for the province as it is, without the modifiers a settled colony gets later (tested: 2.00 a month given, 10.00 once settled).
- Not verified in the game: the panels on screen; a conversion or a colony finishing; a subject's center of trade; a unique trading policy another country follows; extensions calling these capabilities.
