# Player actions plan

Status: decided (2026-11-05). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Content key, Player's view, Full view, Capability, Command). The decisions and their reasons are in [ADR remote-control-reads-the-players-view](adr/remote-control-reads-the-players-view.md), and build on [ADR one-capability-catalogue](adr/one-capability-catalogue.md), [ADR one-command-layer-in-written-form](adr/one-command-layer-in-written-form.md) and [ADR remote-control-uses-capabilities](adr/remote-control-uses-capabilities.md). How a capability is added, layer by layer, is [adding-a-capability.md](adding-a-capability.md).

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
eu4-harmony game peace evaluate ENG --terms '[{"term":"province","province":236},{"term":"humiliate"}]'
→ {"can":true,"reasons":[],"cost":[{"resource":"war_score","amount":27}],"acceptance":{"accepts":true,"total":8,"factors":[…]}}
eu4-harmony game peace demand ENG --terms '…'
```

### The player's view and the full view

- Extensions read the full view. Remote control reads the player's view unless the framework setting "Remote control sees everything" is on (off as installed); the setting is in the settings window.
- Under the player's view a read leaves out what the player's interface does not show: fields of another country its panels do not show are null, and what fog of war hides is not listed, as `army.at` does now. Each part says, for each read it adds, what the player's view leaves out.
- `army.at` gives extensions the armies under fog of war too.

### Events

- New events are added with the part whose topic they are: `month` and `year` (foundations), a building finished (province administration), a battle fought (armies), a war declared and a peace made, a province changing owner (war and peace). They are about every country, each naming the countries it concerns; a battle only where the player sees it (the full view: every battle).
- An event kind no extension has a handler for, and no remote client has read since the game started, is not worked out at all.
- Remote control's `events` takes `kinds` and `countries`, and answers only the events of those kinds that name one of those countries; an event that names no country, such as `day`, passes any `countries`.
- What the events cost the game is measured in each part that adds one, into [performance.md](performance.md).

## How it fits together

- **The catalogue** (`remote/`): each new group is a file of its own, with its capabilities and results; `Choice`, `Amount` and `Acceptance` are shared types of the catalogue's (`remote/src/choices.rs`).
- **The capability layer** (`core/src/game/`): a file per group. A read takes the caller's view (`game::view(caller)`); an action checks as the panel does, giving the game's reasons, writes its command and posts it through the command layer (`game::commands::post`).
- **Reasons from script conditions**: decisions, missions, privileges, reforms, policies, ideas, parliament issues and subject interactions are allowed by conditions in script; one module (`game::conditions`) writes a condition's failing lines as the game's tooltip does, for every group to use ([research/triggers.md](research/triggers.md)).
- **Objects**: what the framework reads of the game's countries and provinces is in [research/countries.md](research/countries.md) and [research/provinces.md](research/provinces.md); each field read is a dependency point.

## Parts

Each part after the foundations is built in a session of its own, in its own worktree and pull request, which merges master before it is merged; the parts may run side by side, the game shared through the claim (AGENTS.md, "Testing in the game").

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`, and so do the offline checks against `eu4.exe`;
- every capability it adds was tried in the game, its refusals among them, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- docs/research holds what was found in the game's code, every new place in it is a dependency point, its section below lists the capabilities it added, and README.md and README.zh-CN.md say what changed for players.

### A0 Foundations

This session, before the rest: this plan, [adding-a-capability.md](adding-a-capability.md), `Choice`, `Amount` and `Acceptance`; the player's view and the full view, with the setting; `game::conditions`, with `decision.list` as the first group on it and the example the other parts follow; events not worked out when nobody listens, `events` filtered by `kinds` and `countries`, and `month` and `year`; the countries' and provinces' fields found ([research/countries.md](research/countries.md), [research/provinces.md](research/provinces.md)).

### A1 Reading countries, provinces and trade

`country.get` grown with technology levels, ideas, governing capacity, inflation, loans, rebels; `country.budget` (income and expenses as the ledger shows them); `province.get` grown with autonomy, culture, religion, unrest, trade good, trade node, state or territory, cores and claims, the construction queue, the missionary's and colonist's progress; `trade.nodes`, `trade.node` (value, flows, merchants). What the player's view leaves out of each.

### A2 Monarch points and the treasury

Researching technology; choosing and cancelling the national focus; raising stability; reducing inflation; raising mercantilism; taking and repaying loans. Each with its `Choice` in an `options` or `list` read.

### A3 Province administration

Developing and exploiting development; raising and lowering autonomy; making a state or a territory; setting and cancelling state edicts; coring and cancelling a core; changing culture; building, cancelling and destroying buildings. The event of a building finished.

Built and tried in the game on 2026-11-05 ([prototype-log.md](prototype-log.md); what was found in the game's code is [research/province-view.md](research/province-view.md)):

- **The province view**, in the `province` group, one capability a button: `develop_tax`, `develop_production`, `develop_manpower`, `exploit_tax`, `exploit_production`, `exploit_manpower`, `raise_autonomy`, `lower_autonomy`, `make_state`, `make_territory`, `set_edict` (an edict by its content key), `cancel_edict`, `core`, `cancel_core`, `change_culture` (to the player's primary culture, as the button does) and `cancel_culture`; and `province.options`, each of them with its `Choice`, setting an edict one entry an edict. Each refuses with the reasons its button's tooltip gives, from the game's own check; the prices are the game's, in monarch points.
- **The building panel**, a group of its own, `building`: `building.options` (each building the game has, to build where it does not stand and to destroy where it does, with its price in ducats and the game's reasons; what is being built, its progress, end and builder; cancelling it), `building.build`, `building.cancel` and `building.destroy`, a building by its content key.
- **The event** `building_finished` (province, building, the province's owner), about every country, worked out only while someone listens; its Lua handler `on_building_finished`.
- **Decided here**: exploiting, which asks the player to confirm, is carried out as a player who confirms; the culture button converts to the primary culture, so `change_culture` takes no culture; where the game's own check leaves out what its button asks (the culture button's points, a culture that is the primary one already), the framework asks it as the button does; the state and culture commands carry no country and act for the province's owner, so the framework refuses them for a province the player does not own, as it does destroying a building, which no check of the game's refuses. None of these reads differs between the player's view and the full view: they are the player's own buttons, which the player sees for any province.
- **Not verified**: an extension's `on_building_finished`; cancelling another country's building, destroying under siege, and abandoning the capital's state, whose refusals come from the command's check without a reason; the edict's year after it is set is the game's own rule.

### A4 Armies and fleets

Fleets moving, `army.move` taking `transports` for a goal across the sea, armies embarking and landing; merging, splitting and disbanding; recruiting and assigning leaders; hiring mercenaries; bombarding, naval bombarding and assaulting; mothballing and reopening forts; drilling; consolidating regiments; fleet missions. The event of a battle fought.

### A5 War and peace

Handing over occupied provinces; calling allies into a war; peace terms (`peace.terms`, `peace.evaluate`, `peace.demand`, `peace.offer`), every term the peace view offers. The events of a war declared, a peace made, a province changing owner.

### A6 Diplomacy and subjects

Every action the diplomacy panel offers the player, with `diplomacy.options`; rivals; annexing and integrating subjects and cancelling it; subject interactions.

### A7 Government

Idea groups and ideas; policies; government reforms; raising the government's rank; estates (privileges, interactions); parliament; decisions; missions.

### A8 Religion, colonies and trade

Missionaries and the number of automatic missionaries; colonists; trade companies and their investments; centers of trade; merchants.
