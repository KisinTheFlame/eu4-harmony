# Provinces and the map

A province is a piece of the map: land a country can own, a lake, or a sea zone. The map is the provinces and the connections between them. A land province has a name, development, terrain, a fort and buildings, a culture, a religion and a trade good, an owner and a controller, cores and claims, and the constructions under way in it. The map is drawn in map modes, hidden from the player by fog of war, and joined into trade nodes that value flows through. All of it is as of the 1.37.5 baseline.

## A province

- **Land, lake or sea zone.** The map says which provinces are lakes and which are sea zones (`lakes` and `sea_starts` in the map's configuration); every other province is land. So "not a sea zone" is not enough to be land: a lake is neither. A province the climate file lists under `impassable` is land no army may enter ([research/provinces.md](../research/provinces.md#land-lakes-and-impassable-provinces)).
- **Its name** is the localisation of `PROV<id>`, or what the province was renamed to, in game text ([research/provinces.md](../research/provinces.md#name)).
- **Development** is base tax, production and manpower. What the game shows adds, in a native country's capital, that country's tribal development, so the stored values alone are not what the player sees ([research/provinces.md](../research/provinces.md#development)).
- **Owner and controller** are two countries. A controller other than the owner is not by itself an enemy occupation: an ally may hold the province. Whether the controller is hostile is a question of the countries' relations ([military.md](military.md#hostility); [research/provinces.md](../research/provinces.md#owner-and-controller)).
- **Culture, religion and trade good** are game objects named by their keys, which are the names of the game's tokens ([research/provinces.md](../research/provinces.md#keys-of-game-objects), [Culture, religion, trade good](../research/provinces.md#culture-religion-trade-good)).
- **Local autonomy** is stored, and shown clamped between a lowest and a highest value the province keeps. What keeps those bounds up to date was not found ([research/provinces.md](../research/provinces.md#local-autonomy)).
- **Unrest** is worked out when asked: the stored unrest, the named unrest, the owner's global unrest, a term for religion, and the province's own modifiers. It is 0 for a province with no owner and for a colony ([research/provinces.md](../research/provinces.md#unrest)).
- **A state or a territory.** A province is in a state when it belongs to its owner's country state in its area. The country state holds the prosperity and the state edict; an edict cannot be changed for a year after it is set ([research/provinces.md](../research/provinces.md#state-or-territory-area-and-state-edict); [research/province-view.md](../research/province-view.md#the-checks-with-the-reasons-the-tooltips-give)).
- **Prosperity** is the country state's, in percent, so every province of a state has the same and a territory none. **Devastation** is the province's own, in percent ([research/provinces.md](../research/provinces.md#prosperity-devastation-manpower-supply-separatism-and-institutions-for-m6-of-mod-capabilities)).
- **The manpower a province gives** its owner and **its supply limit** are worked out when asked, from base manpower and from the province, through the modifiers. The supply limit depends on whose army it is: the map's tooltip asks it for the player's country ([research/provinces.md](../research/provinces.md#prosperity-devastation-manpower-supply-separatism-and-institutions-for-m6-of-mod-capabilities)).
- **Separatism** is a number of years left, which a new owner's separatism starts from (saved as `nationalism`).
- **Institutions.** The world keeps which institutions have appeared, and each province its progress toward each, up to 100 when the province has it ([research/provinces.md](../research/provinces.md#prosperity-devastation-manpower-supply-separatism-and-institutions-for-m6-of-mod-capabilities)).
- **Cores and claims.** The province keeps the tags it is a core of, the territorial cores, and the claims. A claim is permanent when the province's history adds it on or before 1.1.1. When a temporary claim runs out was not found ([research/provinces.md](../research/provinces.md#cores-and-claims)).
- **Buildings** fill one slot per kind of building. Forts are buildings too (`fort_15th` to `fort_18th`). Building one replaces what it supersedes in its chain when it is finished ([research/provinces.md](../research/provinces.md#buildings); [research/province-view.md](../research/province-view.md#a-building-finished)).
- **The fort** has a level. While the fort is mothballed, a capital keeps only its free capital fort. A fort is active and locked (never mothballed), active, or mothballed; mothballing cuts the garrison ([research/provinces.md](../research/provinces.md#fort); [research/sieges.md](../research/sieges.md#b-mothballing-and-reopening-a-fort)).
- **Modifiers.** A province keeps a set of its own modifiers, what its buildings, terrain, trade good and named modifiers give it; what its owner's modifiers add across the country is the country's. The named modifiers scripts put on it are kept with the day each ends, and whether the scripts hide it ([research/modifiers.md](../research/modifiers.md#a-provinces-modifiers), [Named modifiers](../research/modifiers.md#named-modifiers)).
- **Constructions.** Everything under way in a province is in one list: buildings, units being recruited, missionaries, colonists, cores, culture conversions, great projects, settlement growth and the expulsion of minorities. Each construction has a start date, an end date and its progress. A missionary advances every day by the country's missionary strength, and a colony grows in size until it is a city ([research/provinces.md](../research/provinces.md#constructions-buildings-units-cores-culture-missionaries-colonists)).

## The province view's actions

Each button of the province view sends one command, which the session executes ([scripts.md](scripts.md)). A function of the game's says whether the player may, with the reasons the tooltip gives ([research/province-view.md](../research/province-view.md#the-commands-and-their-written-forms), [The checks](../research/province-view.md#the-checks-with-the-reasons-the-tooltips-give)).

- **Developing and coring** are spendings of monarch power. One price function (`0x9526d0`) prices every use of monarch power, from developing to an artillery barrage ([countries.md](countries.md); [research/provinces.md](../research/provinces.md#what-the-province-views-actions-cost)).
- **Converting culture** costs diplomatic power, **building** costs ducats. **Raising or lowering autonomy**, and **making a state or a territory**, spend nothing as far as the code was read ([research/province-view.md](../research/province-view.md#prices); [research/provinces.md](../research/provinces.md#what-the-province-views-actions-cost)).
- **The state and culture commands name no country**: they act for the province's owner, whoever sends them. The destroy command only asks that the building stands ([research/province-view.md](../research/province-view.md#the-commands-and-their-written-forms)).
- **Monuments and canals**, the great projects, are the world's, one each, kept by a manager with the province each stands in and its tier from 0 to 3; whoever owns the province may upgrade it to the next tier for ducats, when the province is not building something else and the project's requirements for the next tier hold ([research/institutions-ages-monuments.md](../research/institutions-ages-monuments.md#monuments)). One the world does not have yet, such as a canal, is built at its site, or anywhere for one that may stand anywhere; a monument that may move is moved to its owner's capital, for ducats by the distance along a route there, and comes back whole if the move is cancelled; the game's check does not ask whose the province moved to is, the window offering only the capital. A great project's construction, whether a building, an upgrade or a move, may be cancelled, giving back part of its cost, or brought forward by two years for manpower or for ducats ([research/institutions-ages-monuments.md](../research/institutions-ages-monuments.md#great-projects-other-commands)).

## The connections

- **A province's neighbors** come from the map and from `map/adjacencies.csv`. A connection is ordinary, a strait (`sea`, through a sea zone), a river, impassable, or a canal with a condition of its own. A map mod or a random new world gives other connections, so only those loaded count ([research/provinces.md](../research/provinces.md#neighbors)).
- **A strait is blockaded** for a country when a hostile fleet with ships, not retreating, is in the sea zone it crosses. A strait whose two shores the country controls is not checked at all. Forts and zones of control play no part ([research/provinces.md](../research/provinces.md#straits)).
- How an army finds its way over these connections is the route search's ([military.md](military.md#routes)).

## Map modes

- **The map manager** keeps the current map mode, the one before it, and a secondary option that picks what the second layer of colors shows. Switching asks it to refresh (`0x821ff0`). A refresh to the mode already shown draws nothing unless it is forced ([research/map-modes.md](../research/map-modes.md#the-map-mode), [Switching](../research/map-modes.md#switching-the-refresh)).
- **Two layers of province colors.** The first is the mode's colors. The second is drawn over the whole province as the stripes of the occupation mask ([research/map-modes.md](../research/map-modes.md#the-color-texture)).
- **The unrest mode's second layer** shows the areas the player's armies suppress rebels in, the selected army's in one color and the others' in another. It does so only when one of the game's switches is on (perhaps a DLC's); otherwise it gets a plain option ([research/map-modes.md](../research/map-modes.md#the-secondary-colors), [Not known](../research/map-modes.md#not-known)).
- **Hovering a core or claim flag** marks that country's owned provinces and its unowned cores for the map's borders. The game clears those temporary marks after each drawing. Harmony keeps the prepared frame's marks for its drawing thread, while the game's real arrays are cleared as usual; changing the hovered target replaces the marks ([research/map-drawing-reads.md](../research/map-drawing-reads.md#the-frames-temporary-highlights-read-and-tested-2026-11-12)).
- The unmodified game draws the map on the thread the interface runs on; Harmony's drawing thread draws prepared frames between updates ([interface.md](interface.md); [research/map-drawing-reads.md](../research/map-drawing-reads.md)).

## Fog of war

- **It is the player's view only.** A province is hidden when fog of war is on and the latest refresh hid it from the player. There is no such answer for another country, and it is neither the camera's view nor what the player has discovered ([research/fog-of-war.md](../research/fog-of-war.md#the-two-bits)).
- **The refresh** (`0x8275e0`) works the whole map out again. The player sees the provinces that their country and the countries sharing its vision own or control, the provinces where those countries' armies stand, and the neighbors of both. A province the player's country has not discovered (terra incognita) stays hidden whatever else holds. With fog off, or without a country, nothing is hidden ([research/fog-of-war.md](../research/fog-of-war.md#the-refresh)).
- **When it runs** was not read from the code: it was seen at a load and as days passed ([research/fog-of-war.md](../research/fog-of-war.md#the-refresh)).
- **What fog hides**: the province tooltip says only that the province cannot be seen, and the army tooltip leaves out every army under fog but the player's own ([research/fog-of-war.md](../research/fog-of-war.md#readers-in-the-game)). The game console's `fow` turns it on and off ([research/fog-of-war.md](../research/fog-of-war.md#the-switch)).

## Trade

- **Trade nodes** are defined in `common/tradenodes` and kept in one array in the world, indexed by their definition's place in load order. Index 0 is a `Null` definition. Each province belongs to one node ([research/trade.md](../research/trade.md#the-list-of-all-nodes); [research/provinces.md](../research/provinces.md#trade-node-center-of-trade-trade-company)).
- **Value flows downstream.** A node has a value, the local value of its own provinces, and an outgoing value, which it splits over its outgoing links by their steering. The receiving node keeps each incoming flow, with the node it comes from; the sending node keeps none ([research/trade.md](../research/trade.md#the-node-0x138-bytes), [Incoming flows](../research/trade.md#incoming-flows-0x20-bytes-vtable-0x1ce9258)).
- **Countries in a node.** Every country has an entry in every node: its trade power, its share of the node's power, and its merchant, who either collects or steers down one link. A country collects in a node where its merchant collects, or in its home node (the node of its trade port) when no merchant of its steers there; the pirates always collect ([research/trade.md](../research/trade.md#the-countries-in-a-node-entries-of-0xc0-bytes), [The country's home node](../research/trade.md#the-countrys-home-node)).
- **The trade range** of a country is its technology's range raised by a modifier of the country's; a province is within it when its distance, over land and sea or straight, from one of the country's cored cities, or from those of countries whose relation lends them, is at most the range. The node of the country's capital is always in range (inferred). Every merchant at work in a node has the merchants' rules checked again each day, the range among them, and is recalled when they no longer hold ([research/trade-range-and-naval-supply.md](../research/trade-range-and-naval-supply.md#the-trade-range-check)).
- **A province adds trade power**, and may be a center of trade or part of a trade company ([research/trade.md](../research/trade.md#the-provinces-trade-power); [research/provinces.md](../research/provinces.md#trade-node-center-of-trade-trade-company)).
- Fleets on a trade mission protect trade or privateer in a node ([military.md](military.md#orders)).

## Where it meets the other systems

- **Armies** stand in provinces, need access to enter them, besiege their forts and are hidden by their fog ([military.md](military.md)).
- **Countries** own and control provinces, hold their cores, and pay for developing and coring in monarch power ([countries.md](countries.md)).
- **The day** advances missionaries, colonies and constructions, and makes what the player sees stale until the next fog refresh ([day.md](day.md)).
- **Scripts** read these fields through triggers such as `has_terrain`, `is_state`, `trade_node_value` and `construction_progress` ([scripts.md](scripts.md); [research/provinces.md](../research/provinces.md#province-fields-for-the-player-actions-reads)).
- **The program** reads the map, its adjacencies and climate when it loads ([program.md](program.md)).

## What the framework relies on

- A province's snapshot and the player-actions reads (`game::provinces`, `game::trade`): [plans/remote-control.md](../plans/remote-control.md), [plans/player-actions.md](../plans/player-actions.md), [plans/mod-capabilities.md](../plans/mod-capabilities.md) (M6).
- Its own modifiers and its named modifiers (`game::modifiers`): [plans/mod-capabilities.md](../plans/mod-capabilities.md).
- The neighbors and the strait check (`game::world`): [plans/remote-control.md](../plans/remote-control.md).
- What the player sees, read only after a whole fog refresh: [ADR remote-control-reads-the-players-view](../adr/2026-11-05-remote-control-reads-the-players-view.md).
- Coloring provinces in the unrest mode (`map`): [plans/remote-control.md](../plans/remote-control.md).
- The province view's actions as commands (`game::administration`, `game::buildings`): [plans/player-actions.md](../plans/player-actions.md), [ADR commands-run-at-once](../adr/2026-11-03-commands-run-at-once.md).

## What is not known yet

- What keeps the autonomy bounds and the unrest base up to date, and when a temporary claim ends ([research/provinces.md](../research/provinces.md#not-found)).
- When the fog refresh runs and how often, and the branches for subjects and special armies ([research/fog-of-war.md](../research/fog-of-war.md#the-refresh)).
- Which switch gives the unrest mode its suppression areas ([research/map-modes.md](../research/map-modes.md#not-known)).
- Where the half cost of a mothballed fort is applied ([research/sieges.md](../research/sieges.md#b-mothballing-and-reopening-a-fort)).
- The meaning of several trade node fields (retention, the collectors' and pull powers), where merchants on their way are kept, and whether the `Null` node really sits at index 0. Nothing of trade was tried in the running game ([research/trade.md](../research/trade.md#not-found-or-not-looked-at)).
