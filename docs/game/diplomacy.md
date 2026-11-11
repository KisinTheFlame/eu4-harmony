# Diplomacy

Countries deal with each other through diplomatic actions: an alliance offered or broken, a royal marriage, a gift, a claim fabricated, a war declared, a peace offered, about ninety kinds in all. An action is made by one country to another, checked, sent as a command, and answered: by the AI at once, or by a human player through an offer that waits for them. Beside them are the game's envoys (colonists, merchants, missionaries), which a country sends to provinces and trade nodes, and the events that wait for a player's answer. What a country is and keeps is [countries.md](countries.md); the commands that carry actions are [scripts.md](scripts.md); the AI that answers them is [ai.md](ai.md).

## Two countries' relation

- **Each country keeps a record of its relation to every other country**, by the other's index: what the diplomacy view and the actions read of the two, among them the casus belli one holds against the other, the wars between them, the favors owed and the trust ([research/countries.md](../research/countries.md#a-countrys-snapshot-countriessnapshot), [research/diplomacy.md](../research/diplomacy.md#the-war-goals-offered), [research/diplomacy.md](../research/diplomacy.md#table)).
- **Opinion is what one country thinks of another**, from -200 to 200: the sum of its parts, some from what the two are (religion, culture, a rivalry, an alliance, claims, a border), others opinion modifiers the game, events and actions give it for a time, among them the aggressive expansion the other's conquests cost it, each worn away a little a year. The panels show the number the game last counted, which it counts again from time to time; the tooltip counts it afresh with its parts. Each country's record of another also holds its trust in the other and the favors it owes it ([research/diplomacy.md](../research/diplomacy.md#opinion)).
- **Lasting relations are objects of their own**: a subject relation between overlord and subject, a support of independence and the other diplomatic relations, which share one base, the two countries and their dates; the lists of allies and the like that a country shows are worked out from them ([research/countries.md](../research/countries.md), [research/dependency-teardown.md](../research/dependency-teardown.md#the-list-it-walks), [countries.md](countries.md)).

## Diplomatic actions

- **Every action is one kind of object**: the country making it and the one it is made to, the day it was made and sent, its state (a proposal, declined, accepted), and whether it starts something (offers an alliance) or ends it (dissolves one) ([research/diplomacy.md](../research/diplomacy.md#diplomatic-actions)).
- **Three checks, each a level above the last**: the action's own conditions (opinion, a truce, already at war); what the window that sends it asks on top (both countries exist, a free diplomat, the wait after a diplomat was sent to the same country, stability, the first month of the game or of the last war); and what the diplomacy panel asks to enable the button (one alliance offer a day, one peace offer a day). Each writes its reasons as the red lines of the tooltip ([research/diplomacy.md](../research/diplomacy.md#the-checks-and-the-games-reasons)).
- **The diplomacy panel's list** is made again whenever it is shown: for each type in a fixed order, whether the panel offers it toward that country now, each type that both starts and ends something asked both ways. The scripted actions of `common/new_diplomatic_actions` come first, named by their keys ([research/diplomacy.md](../research/diplomacy.md#the-diplomacy-panel)).
- **Many actions open a window of their own** for what the player chooses: a province (fabricating a claim, threatening war, selling one), an amount (a gift, a war subsidy, a loan, a price), an ally to break off, a rebel faction to support, an army to hire out. Money is in thousandths of a ducat but for a loan offered and a province sold, in whole ducats ([research/diplomacy.md](../research/diplomacy.md#the-windows-the-panel-opens)).
- **Whether the other would accept** is the AI's weighing: a sum of factors, each with its value and text, accepted at a sum of at least 1, as the panel's icon and tooltip show it; a few kinds answer by a chance from the game's random generator instead, and twelve kinds show no answer at all ([research/diplomacy.md](../research/diplomacy.md#the-diplomacy-panel), [research/peace.md](../research/peace.md#whether-the-ai-accepts-and-why)).
- **Sending and the answer**: the action makes a command, posted to the session; when it is carried out a proposal to an AI is answered at once, a copy marked declined or accepted coming back, while a proposal to a human player waits in that country's offers (below). A declaration of war needs no answer and is carried out at once. The answer is there a frame or two after the send, paused or running ([research/diplomacy.md](../research/diplomacy.md#carrying-one-out-and-the-answer)).

## War

- **The war goals a declaration may have** come from the casus belli the declarer holds against the other and those its triggers give it; a casus belli marked exclusive leaves only its own; with none, the war has no war goal ([research/diplomacy.md](../research/diplomacy.md#the-war-goals-offered)).
- **Only the allies the declaration names are called to arms**, when a human player declares: the declare war window's call boxes put them in, each allowed when the AI ally would accept the call and not when the other side lists it too. An ally of both sides that is not called may join the defenders, and the alliance with the declarer ends ([research/diplomacy.md](../research/diplomacy.md#calling-allies-to-arms)).
- **A call to arms in a war going on** is a diplomatic action of its own, with its conditions (not already called, a truce that forbids an offensive war) and the AI's answer; the panel weighs it with land promised, though a call sent promises none ([research/peace.md](../research/peace.md#calls-to-arms-in-a-war-going-on)).
- **Occupied provinces may be handed to an ally** that can take them, not while under siege ([research/peace.md](../research/peace.md#handing-over-occupied-provinces)).

## Peace

- **A peace treaty** holds terms of 40 kinds (provinces, gold, cores revoked or returned, subjugation, humiliation, independence, trade power, religion, a scripted treaty and the rest), every term the peace view offers, each chosen or not; the treaty's war score cost is the sum of the chosen terms' costs, each kind capped ([research/peace.md](../research/peace.md#the-peace-treaty-and-its-terms), [research/peace.md](../research/peace.md#what-the-peace-view-does)).
- **Gold is set by its buttons**, up to what all the war score buys, and carried out by shares: each country on the receiving side weighed by its part in the war, the rest of the rounding to the treaty's maker; the receivers' inflation rises by what they get against their income. The peace view shows the whole ([research/peace.md](../research/peace.md#what-the-peace-view-does)).
- **A peace offer is a diplomatic action** carrying its treaty, one a day; the AI weighs the treaty and gives its factors ([research/peace.md](../research/peace.md#whether-the-ai-accepts-and-why)).

## Offers: what other countries propose to a player

- **A proposal to a human player is kept in that country's list of offers**, with an id and the day it arrived. The map manager shows each as a diplomatic message icon, or opens its dialog at once, presumably when the message settings have that kind pop up ([research/questions-and-offers.md](../research/questions-and-offers.md#where-they-are-kept), [interface.md](interface.md)).
- **An offer lasts two months** from the day it was made: past them its dialog declines it as soon as anything asks, and an icon left alone declines it on its own some sixty days on, after flashing for ten. During those ten days nothing asks, so a lapsed offer can still be seen ([research/questions-and-offers.md](../research/questions-and-offers.md#where-they-are-kept), [research/questions-and-offers.md](../research/questions-and-offers.md#the-dialogs)).
- **Accepting or declining** sends the action's answer back; a few dialogs are not about an answer at all, among them the two that fabricate a claim, whose accept sends an action of the player's own ([research/questions-and-offers.md](../research/questions-and-offers.md#the-dialogs)).

## Questions: events waiting for a player

- **The world keeps the events shown to players and not yet answered**, each with its scope and a last day to answer, four months after it fired; past it the game chooses the option the AI would. For an AI country, or a hidden event, the game chooses at once ([research/questions-and-offers.md](../research/questions-and-offers.md#the-pending-events)).
- **The event window shows only the options the player may choose**, numbered among all of them; choosing sends a command that runs the option's effects and removes the pending event, and checks neither that the event still waits nor that the option may be chosen ([research/questions-and-offers.md](../research/questions-and-offers.md#how-the-game-shows-and-answers-them), [scripts.md](scripts.md)).

## The Holy Roman Empire and the Celestial Empire

- **Each empire is one object in the world**, with its emperor, its imperial authority or mandate, and the reforms it has passed in order ([research/empires.md](../research/empires.md#the-empires)).
- **Both empires' reforms are one kind of thing**, each naming its empire; the Holy Roman Empire's view shows those of the set its DLC chooses, in three parts, a main line and two branches that exclude each other ([research/empires.md](../research/empires.md#imperial-reforms)).
- **Only the emperor passes a reform.** The Holy Roman Empire asks peace in the Empire and for the emperor, the reform before it, enough electors and princes for it against it, and authority; the Celestial Empire asks the reform's own conditions, a least mandate, the mandate it costs and stability it can spare. The command names no country: the game enacts it as the emperor ([research/empires.md](../research/empires.md#the-commands)).
- **An emperor of the Holy Roman Empire may revoke the last reform passed**, once in his life, for authority ([research/empires.md](../research/empires.md#revoking-0x7061e0empire-reasons---bool)).
- **The Celestial Emperor enacts one decree at a time**, for meritocracy, in force for its time ([research/empires.md](../research/empires.md#decrees)).
- **In an imperial incident**, members support one of its options and the emperor favors one, by one command ([research/empires.md](../research/empires.md#the-commands)).

## Envoys

- **A country has three pools of envoys**: colonists, merchants and missionaries, each free, travelling or placed ([research/envoys.md](../research/envoys.md#envoys)).
- **Missionaries** are sent to a province of the country's own or, presumably, of its subjects', one at a time; a missionary at work is the province's construction, and its upkeep is paid monthly. A number of them can be set to work on their own ([research/envoys.md](../research/envoys.md#missionaries)).
- **Colonists** settle a province once the country has chosen a native policy; they also expel minorities and promote settlement growth, and each colony adds to the colonial upkeep, which grows faster with each colony beyond the number of colonists (inferred from the code's shape) ([research/envoys.md](../research/envoys.md#colonists), [provinces.md](provinces.md)).
- **Merchants** collect in a trade node or steer its trade downstream; a country chooses a trading policy in each node where its merchant is, under a cooldown. Trade companies and centers of trade are the province view's ([research/envoys.md](../research/envoys.md#merchants), [research/envoys.md](../research/envoys.md#trade-companies), [provinces.md](provinces.md)).
- **Each send checks twice**: the interface's rules, with the game's texts, and the command's own check, which is often looser (a trade company's command checks only a DLC) ([research/envoys.md](../research/envoys.md)).

## What the framework relies on

- Every action of the diplomacy panel as a capability, taken from the panel's own list, asked the panel's and the window's checks, with what its window has the player choose written in, sent through the command layer, and answered by what came of it ([plans/player-actions.md](../plans/player-actions.md), [research/diplomacy.md](../research/diplomacy.md#the-diplomacy-panel)).
- Declarations of war with their war goals and allies called, alliances, peace offers with their terms and the AI's factors, calls to arms, occupation handed over ([plans/remote-control.md](../plans/remote-control.md), [research/peace.md](../research/peace.md)).
- Questions and offers answered through the game's own windows and dialogs, only while the game still takes an answer ([research/questions-and-offers.md](../research/questions-and-offers.md)).
- Missionaries, colonists, merchants, trading policies, trade companies and centers of trade ([research/envoys.md](../research/envoys.md)).
- What every country thinks of every other: the opinion as the panels show it, its parts as the tooltip writes them, the aggressive expansion among them, the trust and the favors ([plans/mod-capabilities.md](../plans/mod-capabilities.md), [research/diplomacy.md](../research/diplomacy.md#opinion)).
- The empires' reforms and the Celestial Emperor's decrees, listed as their views list them, each asked the game's own check, passed, revoked and enacted through the game's own commands ([research/empires.md](../research/empires.md)).
- Wars declared, peace made and provinces changing owner, heard by comparing the world's wars and owners from one day to the next, with no function of the game's observed for them, so that every cause is heard alike: a peace, an annexation, a rebellion, the console ([research/peace.md](../research/peace.md#the-events)).

## What is not known yet

- How the AI decides a peace, beyond the factors it gives; how it calls its own allies to arms and promises land ([research/peace.md](../research/peace.md#not-established), [research/diplomacy.md](../research/diplomacy.md#not-established-1)).
- Whether an ally allowed to be called can still decline; whether a call accepted brings the ally into every war it may join; why a call sent as the default window sends it got no answer ([research/diplomacy.md](../research/diplomacy.md#not-established-1), [research/peace.md](../research/peace.md#not-established)).
- The costs the declare war window confirms before it sends (no casus belli, a truce broken) ([research/diplomacy.md](../research/diplomacy.md#not-established-1)).
- Who exactly counts as a peace's receiving side for gold; why a treaty made outside the peace view finds no chosen gold term ([research/peace.md](../research/peace.md#not-established)).
- What the diplomacy view's opinion of a third country shows the player, beside the opinion map mode ([research/diplomacy.md](../research/diplomacy.md#opinion)).
- Some windows' lists and whether an action left at its defaults passes (a loan of nothing, a federation with no name) ([research/diplomacy.md](../research/diplomacy.md#not-established)).
- Whether a player's event ever waits without a window, and a dialog refusing for another reason than its age ([research/questions-and-offers.md](../research/questions-and-offers.md#not-established)).
- What locks an envoy pool, and several of the envoys' rules ([research/envoys.md](../research/envoys.md#not-worked-out)).
- What the Holy Roman Empire's view lists of an imperial incident, and how a pick of an option is written ([research/empires.md](../research/empires.md#not-known-yet)).
