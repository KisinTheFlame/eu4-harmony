# Armies and fleets

Armies and fleets are units: an army of regiments, a fleet of ships. Each has an owner, a province it stands in, a route still to go, perhaps a leader and a mission. The player gives them orders on the map and in the unit panel, and the game makes each order into a command. Units find their way over the map's connections, fight battles and sieges, are recruited in provinces, and are led by leaders; a country may also hire mercenary companies. All of it is as of the 1.37.5 baseline.

## A unit

- **Armies and fleets** are two classes of one kind of unit. Most of what follows holds for both: a fleet takes orders through the army's own action ([research/armies.md](../research/armies.md#the-armys-classes); [research/units.md](../research/units.md#c-fleets-moving)).
- **Its ID** is a type and a sequence number, and it is all a save keeps of the unit. A unit kept by its ID across a load got the same ID back in the one save tried. Whether a number is ever given out again, and what splitting, merging or a change of owner do to an ID, is not known ([research/armies.md](../research/armies.md#identity)).
- **Its regiments or ships** each have a type and a strength: a full regiment is 1000 men. A country has one current unit type for each of seven kinds: infantry, cavalry, artillery, heavy ship, light ship, galley and transport ([research/provinces.md](../research/provinces.md#what-can-be-recruited-there); [research/units.md](../research/units.md#e-unit-fields)).
- **A regiment or ship** has its own ID, name, unit type, strength (1, 1000 men, at full, the unit type's full strength) and morale. A unit's morale as its panel shows it is its regiments' weighted by their strength, against a most the game works out; its monthly maintenance is its class's own sum over its regiments or ships ([research/units.md](../research/units.md#f-regiments-strength-morale-leader-and-maintenance-for-m7-of-mod-capabilities)).
- **The country it acts for** is its owner, except that a hired army acts for whoever hired it. That holds only when one of the game's switches is on, which was not identified ([research/armies.md](../research/armies.md#the-military-tag)).
- **What it is doing**: in a battle, besieging, movement-locked a few days into a step, retreating (two flags, set by different retreats), and its mission. A unit's mission is what it does on its own: exploring, hunting rebels, carpet sieging or drilling for an army; for a fleet, protecting trade, privateering, hunting pirates, patrolling, exploring, evading, hunting fleets, blockading, intercepting or repairing ([research/armies.md](../research/armies.md#what-an-army-is-doing); [research/units.md](../research/units.md#b-fleet-missions)).
- **An army aboard a fleet** points at its fleet, and the fleet keeps the list of armies it carries ([research/units.md](../research/units.md#d-transport-as-the-player-does-it)).
- **Every day** each unit has its update, inside its country's daily update, one after another ([day.md](day.md); [research/time.md](../research/time.md#the-daily-update)).

## Orders

- **A right click** on the map goes to each selected unit's action (`0xd429e0`), which makes one command: a retreat for a unit in a battle, a cancel for a click on the unit's own province, and otherwise a move, after the route to the province clicked has been searched for. The command is posted to the session whatever its check answered; the session checks it again and executes it in a later frame ([research/army-orders.md](../research/army-orders.md#the-armys-action-0xd429e0), [Taken, then carried out](../research/army-orders.md#taken-then-carried-out); [scripts.md](scripts.md)).
- **A movement-locked army** keeps the step it is on, and the new route follows it ([research/army-orders.md](../research/army-orders.md#a-movement-locked-army-tested)).
- **Some orders change nothing.** A move onto the sea with no fleet there is posted, and the route stays as it was ([research/army-orders.md](../research/army-orders.md#orders-that-change-nothing-tested)).
- **The movement is everyone's.** The AI, the unit's daily update, the missions and the retreat after a battle all make the same move commands, and go through the same movement, as the player's right click does ([research/army-orders.md](../research/army-orders.md#not-only-the-players)).
- **The unit panel's buttons** each post a command, without asking its check first: merge, split in half, disband, consolidate, drill, mothball a fleet, attach to an army or to a transport. Each button has a function of the game's that says whether it may, with the tooltip's reasons ([research/units.md](../research/units.md#the-unit-panels-actions), [Merging, splitting, disbanding](../research/units.md#a-merging-splitting-disbanding-consolidating-drilling)).
- **Organizing a unit.** The unit card's icon of a kind of regiment or ship detaches all of that kind into a new unit, unless that would leave the unit empty. Attach makes a unit follow another of its kind in its province, and Detach ends it; the game offers as targets the units it allows there, of another country only in some cases. An army in a port attached to the transports there embarks when they leave; a fleet's automatic transportation, which needs a ship that carries troops and a fleet not mothballed, is the same toggle in both its states. Detach Special Units splits off the regiments of a special kind, such as banners, and is refused for an army of nothing else, as a mercenary company is ([research/units.md](../research/units.md#g-organizing-an-army-detaching-attaching-transports-special-units-for-m15-of-mod-capabilities)).
- **Fleet missions.** Hunting fleets, blockading and intercepting are given for a region of the sea. Protecting trade, privateering and hunting pirates are given for a trade node ([provinces.md](provinces.md#trade)). Patrolling and evading need a route first: they are given after a move order ([research/units.md](../research/units.md#b-fleet-missions)).
- **Armies by sea.** An army ordered onto a sea zone boards an own fleet there with room for all its regiments. It lands when it is ordered to a coastal province, or the fleet carries it there. Where no route by land is found, the right click offers the game's automatic transport, which plans the splitting and boarding itself ([research/units.md](../research/units.md#d-transport-as-the-player-does-it)).

## Routes

- **A route** is the provinces still to go: the next one first, the destination last. The province the army stands in is not in it ([research/routes.md](../research/routes.md#a-route)).
- **The route search** (`0x96c520`) is a weighted search with a distance heuristic. Its answer is the route it chose, not the one with the fewest steps, and there is no way to ask for that ([research/routes.md](../research/routes.md#the-route-query-0x96c520), [Not the fewest steps](../research/routes.md#not-the-fewest-steps)).
- **What a step may be** is the search's policy's question. It asks about access, impassable provinces, the kind of connection, blockaded straits, forts, and the army's route so far. The rebel hunt's policy is the same with the odds of winning added ([research/routes.md](../research/routes.md#the-policy)).
- **Where the army has been matters**: the search reads where the army came from and its route, so a search from another province is no route of an army standing there ([research/routes.md](../research/routes.md#route-history)).
- **A land route may cross the sea** where a fleet there can take the army aboard. A strait does not put its sea zone in the route: a route across a strait is all land ([research/routes.md](../research/routes.md#sea-zones-and-straits)).
- **Moving and stopping.** Moving copies a route into the army, checking only that each next province is a neighbor; stopping clears the route whatever the army's state. The rules are the search's alone ([research/routes.md](../research/routes.md#moving-the-route-install-0xd50340), [Stopping](../research/routes.md#stopping-0xd4fcb0)).
- **Access.** Whether an army may be in a province asks first the army's own cases (ships to board in a sea zone, exile, being hired, exploring) and then its country's access through the owner's and the controller's relations: war, subjects, access granted. It reads no fort, zone of control or route, so it does not say whether a move would succeed ([research/armies.md](../research/armies.md#access-to-a-province); [diplomacy.md](diplomacy.md)).

## Hostility

- **Between countries.** A country keeps records of the countries it is hostile to. The rebels and two other special countries are hostile to everyone ([research/armies.md](../research/armies.md#hostility-between-countries); [countries.md](countries.md)).
- **Hostile occupation** of a province is a controller hostile to the owner, not merely another controller ([research/armies.md](../research/armies.md#hostility-between-countries)).
- **Between armies** it is the countries' hostility, but a hired army stands for its hirer, two rebel armies of one faction are not hostile, and a unit's own owner never is ([research/armies.md](../research/armies.md#hostility-between-armies)).

## Battles

- **Battles and sieges** are one kind of thing, a combat, in three kinds: land, naval and siege. A combat has an attacker side and a defender side, each with its units, countries, leader, composition and losses ([research/sieges.md](../research/sieges.md#objects)).
- **Deciding.** A battle's daily update (`0x22c080`) decides it when a side is overrun, has no units, or has under ten men of strength left; a land battle may also end at once in an overrun. Land and naval battles go the same way ([research/defeat.md](../research/defeat.md#deciding-a-battle); [research/battles.md](../research/battles.md#hearing-a-battle-once)).
- **Settling.** The regular settlement either overruns the loser or makes the results and sends the loser off. Every losing unit retreats to a province of its own, stopped and ordered there through the army's action; a unit with nowhere to go is removed. An overrun destroys the losing side ([research/defeat.md](../research/defeat.md#settling-it)).
- **Not every end is a defeat.** A siege ending, a unit taken out of a battle, and the player's own retreat use the same retreat and results ([research/defeat.md](../research/defeat.md#what-is-not-a-defeat)).

## Sieges and forts

- **A siege** belongs to its province. Its besiegers are its attacker side, and it keeps a breach, whether an assault is under way, its morale and its status ([research/sieges.md](../research/sieges.md#a-siege)).
- **Barrages and the assault** are spendings of military power, priced as all monarch power is ([countries.md](countries.md)). An artillery barrage needs artillery at least the fort's level. A naval barrage needs a coastal province and own fleets beside it with cannons, a hundred for each fort level. An assault needs a breach, infantry and morale. All three need a fort, a siege the player leads, and no assault under way ([research/sieges.md](../research/sieges.md#a-artillery-barrage-naval-barrage-assault), [Can the player press it](../research/sieges.md#can-the-player-press-it-and-why-not-one-function-each)).
- **A fort can be mothballed and reopened** by its controller when there is no siege, unless it is locked. A whole country's forts can be mothballed at once ([research/sieges.md](../research/sieges.md#b-mothballing-and-reopening-a-fort); [provinces.md](provinces.md#a-province)).

## Recruiting

- **Recruiting a regiment or a ship** is a command for the province's unit type. The check (`0x346570`) asks that the province is a core, owned and controlled, not under siege or in combat, and not of too high autonomy, and that there are manpower, sailors and money; a ship needs a port. The reasons are the tooltip's ([research/provinces.md](../research/provinces.md#what-can-be-recruited-there); [research/recruiting.md](../research/recruiting.md#the-recruit-button)).
- **The cost is taken at once**, and the unit waits among the province's constructions until it is built ([research/recruiting.md](../research/recruiting.md#when-the-game-carries-it-out); [provinces.md](provinces.md#a-province)).
- **A command the session cannot carry out is dropped quietly.** In a game of one player the session takes every command at once, and checks it again when it executes it, at the next frame. One that fails then is discarded with a line in the game's log, and whoever sent it does not hear ([research/recruiting.md](../research/recruiting.md#the-session), [When the game carries it out](../research/recruiting.md#when-the-game-carries-it-out); [scripts.md](scripts.md)).

## Leaders and mercenaries

- **Leaders** are generals, admirals, conquistadors and explorers, each with siege, maneuver, fire and shock. A country keeps them in a list, and each unit has at most one. Once a country has as many leaders as it may keep without upkeep, its leaders cost monarch power a month, military for land and diplomatic for naval ones. Being over that number costs only upkeep: it does not stop recruiting ([research/leaders.md](../research/leaders.md#the-leader-object)).
- **Recruiting a leader** costs military power for a general or a conquistador and diplomatic power for an admiral or an explorer. Conquistadors and explorers need the country able to explore, and admirals a naval force limit. The leader's skills are rolled from the country's army or navy tradition; given no name, the game picks one ([research/leaders.md](../research/leaders.md#a-recruiting-a-leader-create_leader)).
- **Assigning, removing and dismissing.** A leader is given to a unit, or taken off it with an empty leader. A leader leading a unit, or who is the ruler or heir, cannot be dismissed. The commands do not check that a general goes to an army and an admiral to a fleet ([research/leaders.md](../research/leaders.md#b-assigning-removing-and-dismissing-a-leader)).
- **Mercenary companies** are offered to each country as copies of their own. Hiring one is the recruit command with the company, in a province the player picks within the company's range. It costs 48 months of the company's upkeep, and the army is raised over some days ([research/leaders.md](../research/leaders.md#d-hiring-a-mercenary-company)).

## The AI's armies and fleets

The army agents think every two to four days: they merge, drill, retreat and attach, search the map for where to go, split off to besiege, and move. The Admiral keeps the fleets, and the army agents take fleets from it to carry them. All of it goes through the same commands and the same route search as the player's orders ([ai.md](ai.md); [research/ai.md](../research/ai.md#the-army-and-fleet-agents), [The Admiral](../research/ai.md#the-admiral)).

## Where it meets the other systems

- **Provinces**: where units stand, their owners and controllers, forts, straits and fog of war, which hides every army but the player's own ([provinces.md](provinces.md)).
- **Countries**: hostility, manpower, sailors, money, monarch power and tradition ([countries.md](countries.md)).
- **Diplomacy**: wars and access granted decide where an army may go ([diplomacy.md](diplomacy.md)).
- **The day**: units, battles and sieges have their updates inside the day ([day.md](day.md)).
- **Scripts**: every order and button is a command in written form ([scripts.md](scripts.md)).
- **The interface**: the right click on the map and the unit and siege panels ([interface.md](interface.md)).

## What the framework relies on

- Reading units, access, hostility and the armies the player sees (`game::world`, `game::armies`), and their regiments, morale and maintenance (`game::army`): [plans/remote-control.md](../plans/remote-control.md), [plans/mod-capabilities.md](../plans/mod-capabilities.md) (M7), [ADR remote-control-reads-the-players-view](../adr/2026-11-05-remote-control-reads-the-players-view.md).
- Moving and stopping armies as the commands a right click makes, executed at once (`game::orders`): [plans/commands.md](../plans/commands.md), [ADR commands-run-at-once](../adr/2026-11-03-commands-run-at-once.md).
- Hearing the player's orders where a route changes, and defeats and battles (`game::signals`): [plans/remote-control.md](../plans/remote-control.md), [plans/player-actions.md](../plans/player-actions.md).
- Recruiting, the unit panel, fleets, sieges, forts, leaders and mercenaries as commands (`game::recruiting`, `game::units`, `game::sieges`, `game::forts`, `game::leaders`, `game::mercenaries`): [plans/player-actions.md](../plans/player-actions.md).
- The army agents' searches worked out ahead on threads of the framework's (`army_search`): [plans/game-speed.md](../plans/game-speed.md), [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md); [research/late-game.md](../research/late-game.md#the-armies-searches-on-idle-threads).

## What is not known yet

- Whether a unit's ID number is ever given again, and what splitting, merging or a new owner do to it ([research/armies.md](../research/armies.md#identity)).
- Which retreat each of the two retreat flags stands for ([research/armies.md](../research/armies.md#what-an-army-is-doing)), and what writes the route history the search reads ([research/routes.md](../research/routes.md#not-known)).
- Which switch lets hired armies act for their hirer, and who the two special countries besides the rebels are ([research/armies.md](../research/armies.md#hostility-between-countries)).
- When a side's composition is filled, and what losses mean for ships ([research/battles.md](../research/battles.md#hearing-a-battle-once)).
- Whether an overrun reached through the regular settlement is heard as two defeats, as the code reads ([research/defeat.md](../research/defeat.md#what-the-framework-hears)).
