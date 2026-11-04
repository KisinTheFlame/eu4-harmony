//! Diplomacy: the diplomacy panel, what it offers the player toward another
//! country and the player's diplomatic actions, sent as the player's click in
//! the panel sends them, so that the game's checks, costs, AI answers and
//! multiplayer rules apply.
//!
//! `diplomacy.options` lists what the panel offers toward a country, each
//! entry named by the capability that does it. Each of the panel's buttons
//! is a capability of its own, which sends the action as the panel's button
//! and the default diplomacy window send it, with what the game chooses for
//! it there; an action whose window has the player choose more (a province,
//! an amount) is sent with the game's own choice, as in
//! docs/player-actions.md, part A6.
//!
//! A refusal is `rejected`, with the game's reasons, one a line. An action
//! the game lets the player send is answered once the game has done with
//! it: in a single player game the other country answers a proposal as the
//! game carries it out, a frame or two after it is sent, so the answer is
//! `accepted` or `declined`; a declaration of war is `declared`, with the
//! war's name, once the war began; an action that needs no answer is
//! `done` once carried out. When nothing comes within a few seconds,
//! such as when a human player is to answer, the answer is `pending`: what
//! comes of it later shows in `country.get`, as a relation or a war. A
//! proposal the game drops as it carries it out, its own check failing
//! then, is `pending` too: the game says nothing of it.
use crate::choices::{Acceptance, Choice};
use crate::{ArmyId, CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's diplomatic actions.
    "diplomacy": DiplomacyRequest, DiplomacyAnswer;

    /// The war goals the game offers the player for declaring war on a
    /// country, as the declare war window lists them: under each casus belli
    /// the player has against it, each war goal it allows, and last, when
    /// the game offers it, declaring war without a casus belli, whose
    /// `casus_belli` is `none`.
    game WarGoals "war_goals" {
        /// The country to declare war on.
        country: CountryTag,
    } -> Vec<WarGoal>, "the war goals: casus_belli, war_goal, province, target, name";
    /// Declares war on a country for one of the war goals the game offers
    /// (`diplomacy.war_goals`): the first with the casus belli, and the war
    /// goal, province and target when given. It calls to arms the allies
    /// `call_allies` names, as ticking their call boxes in the declare war
    /// window does, with no land promised, and the answer lists them in
    /// `called`. When `call_allies` is `all`, `passed_over` lists each ally
    /// not called, with the reasons: those the window gives for not letting
    /// the player call it, or that the window no longer lists it once
    /// others are called. When `call_allies` names allies, a named one the
    /// window would not let the player call, or no longer lists, refuses
    /// the declaration instead, and nothing is sent. Subjects and
    /// others who join anyway are in neither list: they are in the war, as
    /// `country.get` shows. A costly declaration (no casus belli, a truce
    /// broken) is sent without the window's confirmation, and the game takes
    /// its costs. `rejected` when the game offers no such war goal, does not
    /// let the player declare war, or would not let the player call an ally
    /// named, with its reasons; `invalid_argument` for an empty
    /// `casus_belli` or a country to call that is not among the allies the
    /// window lists.
    game DeclareWar "declare_war" {
        /// The country to declare war on.
        country: CountryTag,
        /// The casus belli's key, such as `cb_claim`, or `none` to declare
        /// war without one.
        casus_belli: String,
        /// The war goal's key, such as `take_claim`; the first under the
        /// casus belli when left out.
        war_goal: Option<String>,
        /// The province the war goal is about.
        province: Option<ProvinceId>,
        /// The country the war goal is about.
        target: Option<CountryTag>,
        /// The allies to call to arms: `all`, the default, for each ally the
        /// declare war window would let the player call; `none`; or a list
        /// of the tags of those to call, such as `["DUL", "CND"]` (on the
        /// command line `DUL,CND`), each one the window must let the player
        /// call.
        call_allies: Option<CallAllies>,
    } -> Outcome, "answer: declared (with war, its name) or pending; called: the allies called to arms; passed_over: under call_allies all, every ally not called, each a country and the reasons, one a line";
    /// Offers a country the player is at war with a white peace, a peace
    /// treaty with no terms, in the war between them, as the peace view
    /// sends one. The game allows it to the war's leader, with the other
    /// side's leader or, as a separate peace, another country in it.
    game OfferPeace "offer_peace" {
        /// The country to offer peace to.
        country: CountryTag,
    } -> Outcome, "answer: accepted, declined or pending";
    /// Proposes an alliance to a country.
    game ProposeAlliance "propose_alliance" {
        /// The country to ally with.
        country: CountryTag,
    } -> Outcome, "answer: accepted, declined or pending";
    /// What the diplomacy panel offers the player toward a country, in its
    /// order: each action it shows a button for, named by the capability
    /// that does it, with whether the player can send it now and, when not,
    /// the reasons its tooltip gives, its description, and, where the panel
    /// shows whether the country would accept, the `acceptance`. The same
    /// in the player's view and the full view: the panel shows all of it.
    game Options "options" {
        /// The other country.
        country: CountryTag,
    } -> Options, "country, actions: each action (its capability, or scripted_action), key, name, can, reasons, cost, description and, where the panel shows it, acceptance";
    /// Makes a country one of the player's rivals, as the rivals list of the
    /// diplomacy view does. `rejected` when the game's command refuses it
    /// (not a valid rival, or the player has as many as the game allows); the
    /// game gives no reason.
    game AddRival "add_rival" {
        /// The country to rival.
        country: CountryTag,
    } -> (), "nothing: the country is a rival";
    /// Drops a rival, as the rivals list of the diplomacy view does.
    /// `rejected` when the game's command refuses it, such as for a country
    /// that is not a rival; the game gives no reason.
    game RemoveRival "remove_rival" {
        /// The rival to drop.
        country: CountryTag,
    } -> (), "nothing: the country is no rival any more";
    /// Sends one of the scripted diplomatic actions the panel offers
    /// (`common/new_diplomatic_actions`), by its content key, as
    /// `diplomacy.options` lists it under `scripted_action`.
    game ScriptedAction "scripted_action" {
        /// The other country.
        country: CountryTag,
        /// Its content key, such as `sell_indulgence`.
        action: String,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Offers a country the player is at war with the player's unconditional surrender. As the diplomacy panel's button `unconditional_surrender` sends it
    /// (`diplomacy.options` says whether the player can).
    game SurrenderUnconditionally "surrender_unconditionally" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Dissolves the alliance with a country. As the diplomacy panel's button `allianceaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game CancelAlliance "cancel_alliance" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Supports a subject's independence from its overlord. As the diplomacy panel's button `support_independence_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game SupportIndependence "support_independence" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops supporting a subject's independence. As the diplomacy panel's button `support_independence_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopSupportingIndependence "stop_supporting_independence" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Forms a coalition against a country, or joins one. As the diplomacy panel's button `form_coalition` sends it
    /// (`diplomacy.options` says whether the player can).
    game FormCoalition "form_coalition" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Leaves the coalition against a country. As the diplomacy panel's button `form_coalition` sends it
    /// (`diplomacy.options` says whether the player can).
    game LeaveCoalition "leave_coalition" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Enforces peace on a country at war with one the player guarantees or protects. As the diplomacy panel's button `enforce_peace` sends it
    /// (`diplomacy.options` says whether the player can).
    game EnforcePeace "enforce_peace" {
        /// The other country.
        country: CountryTag,
        /// The country at war with it on whom peace is enforced.
        enemy: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Demands religious unity of a member of the empire, as its emperor. As the diplomacy panel's button `religious_unity_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game DemandReligiousUnity "demand_religious_unity" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Removes a country's elector status, as the emperor. As the diplomacy panel's button `remove_electorate` sends it
    /// (`diplomacy.options` says whether the player can).
    game RemoveElector "remove_elector" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Grants a country elector status, as the emperor. As the diplomacy panel's button `grant_electorate` sends it
    /// (`diplomacy.options` says whether the player can).
    game GrantElector "grant_elector" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Removes a country's free city status, as the emperor. As the diplomacy panel's button `remove_freecity` sends it
    /// (`diplomacy.options` says whether the player can).
    game RemoveFreeCity "remove_free_city" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Grants a country free city status, as the emperor. As the diplomacy panel's button `grant_freecity` sends it
    /// (`diplomacy.options` says whether the player can).
    game GrantFreeCity "grant_free_city" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Demands that a country return unlawful territory in the empire. As the diplomacy panel's button `demand_unlawful_territory_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game DemandUnlawfulTerritory "demand_unlawful_territory" {
        /// The other country.
        country: CountryTag,
        /// The province of the empire to demand.
        province: ProvinceId,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Calls a crusade against a country, as the Papal controller. As the diplomacy panel's button `call_crusade_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game CallCrusade "call_crusade" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Excommunicates a country's ruler, as the Papal controller. As the diplomacy panel's button `excommunicate_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game Excommunicate "excommunicate" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Lifts the excommunication of a country's ruler. As the diplomacy panel's button `excommunicate_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game LiftExcommunication "lift_excommunication" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Invites a country into the player's federation. As the diplomacy panel's button `invite_to_federation` sends it
    /// (`diplomacy.options` says whether the player can).
    game InviteToFederation "invite_to_federation" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Removes a country from the player's federation. As the diplomacy panel's button `invite_to_federation` sends it
    /// (`diplomacy.options` says whether the player can).
    game RemoveFromFederation "remove_from_federation" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks to join a country's federation. As the diplomacy panel's button `request_to_join_federation` sends it
    /// (`diplomacy.options` says whether the player can).
    game AskToJoinFederation "ask_to_join_federation" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Leaves a country's federation. As the diplomacy panel's button `request_to_join_federation` sends it
    /// (`diplomacy.options` says whether the player can).
    game LeaveFederation "leave_federation" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Invites a country into the player's trade league. As the diplomacy panel's button `invite_to_trade_league` sends it
    /// (`diplomacy.options` says whether the player can).
    game InviteToTradeLeague "invite_to_trade_league" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Removes a country from the player's trade league. As the diplomacy panel's button `invite_to_trade_league` sends it
    /// (`diplomacy.options` says whether the player can).
    game RemoveFromTradeLeague "remove_from_trade_league" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Invites a country into the empire, as its emperor. As the diplomacy panel's button `invite_to_hre` sends it
    /// (`diplomacy.options` says whether the player can).
    game InviteToEmpire "invite_to_empire" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks to join a country's trade league. As the diplomacy panel's button `request_to_join_trade_league` sends it
    /// (`diplomacy.options` says whether the player can).
    game AskToJoinTradeLeague "ask_to_join_trade_league" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sends a diplomat to improve relations with a country. As the diplomacy panel's button `improve_relation` sends it
    /// (`diplomacy.options` says whether the player can).
    game ImproveRelations "improve_relations" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Recalls the diplomat improving relations with a country. As the diplomacy panel's button `improve_relation` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopImprovingRelations "stop_improving_relations" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Starts annexing a subject diplomatically. As the diplomacy panel's button `annexationaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game Annex "annex" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops annexing a subject. As the diplomacy panel's button `annexationaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game CancelAnnexation "cancel_annexation" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Starts integrating a junior partner in a personal union. As the diplomacy panel's button `integrationaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game Integrate "integrate" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops integrating a junior partner. As the diplomacy panel's button `integrationaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game CancelIntegration "cancel_integration" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Makes a vassal a march. As the diplomacy panel's button `designate_march` sends it
    /// (`diplomacy.options` says whether the player can).
    game DesignateMarch "designate_march" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Makes a march a vassal again. As the diplomacy panel's button `designate_march` sends it
    /// (`diplomacy.options` says whether the player can).
    game RevokeMarch "revoke_march" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Abandons the personal union with a junior partner. As the diplomacy panel's button `abandon_union` sends it
    /// (`diplomacy.options` says whether the player can).
    game AbandonUnion "abandon_union" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Offers a country to become the player's vassal. As the diplomacy panel's button `vassalaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game OfferVassalization "offer_vassalization" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Releases a vassal from the player's rule. As the diplomacy panel's button `vassalaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game ReleaseVassal "release_vassal" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks the overlord to make the player's country a march. As the diplomacy panel's button `ask_for_march` sends it
    /// (`diplomacy.options` says whether the player can).
    game AskForMarch "ask_for_march" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Demands that a country become the player's tributary. As the diplomacy panel's button `tributary_state_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game DemandTributary "demand_tributary" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Releases a tributary. As the diplomacy panel's button `tributary_state_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game ReleaseTributary "release_tributary" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Proposes a royal marriage to a country. As the diplomacy panel's button `royal_marriage` sends it
    /// (`diplomacy.options` says whether the player can).
    game ArrangeRoyalMarriage "arrange_royal_marriage" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Breaks the royal marriage with a country. As the diplomacy panel's button `royal_marriage` sends it
    /// (`diplomacy.options` says whether the player can).
    game BreakRoyalMarriage "break_royal_marriage" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Makes a tributary a vassal. As the diplomacy panel's button `convert_tributary_to_vassal` sends it
    /// (`diplomacy.options` says whether the player can).
    game ConvertTributaryToVassal "convert_tributary_to_vassal" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes for ducats. As the diplomacy panel's button `trade_favors_for_gold` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsForGold "trade_favors_for_gold" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes for manpower. As the diplomacy panel's button `trade_favors_for_men` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsForMen "trade_favors_for_men" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes for sailors. As the diplomacy panel's button `trade_favors_for_sailors` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsForSailors "trade_favors_for_sailors" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes to have it name the player's heir as its heir. As the diplomacy panel's button `trade_favors_for_heir` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsForHeir "trade_favors_for_heir" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes for its trust. As the diplomacy panel's button `trade_favors_for_trust` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsForTrust "trade_favors_for_trust" {
        /// The other country.
        country: CountryTag,
        /// The favors to spend.
        favors: u32,
        /// The trust to gain.
        trust: u32,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes to have it prepare for war beside the player. As the diplomacy panel's button `trade_favors_for_war_prep` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsForWarPreparations "trade_favors_for_war_preparations" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Supports a heir of the player's dynasty to a country's throne. As the diplomacy panel's button `support_heir` sends it
    /// (`diplomacy.options` says whether the player can).
    game SupportHeir "support_heir" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops supporting a heir to a country's throne. As the diplomacy panel's button `support_heir` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopSupportingHeir "stop_supporting_heir" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sends a diplomat to curry favors with a country. As the diplomacy panel's button `curry_favors` sends it
    /// (`diplomacy.options` says whether the player can).
    game CurryFavors "curry_favors" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Recalls the diplomat currying favors with a country. As the diplomacy panel's button `curry_favors` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopCurryingFavors "stop_currying_favors" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sends a diplomat to build a spy network in a country. As the diplomacy panel's button `spy_network` sends it
    /// (`diplomacy.options` says whether the player can).
    game BuildSpyNetwork "build_spy_network" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Recalls the diplomat building a spy network in a country. As the diplomacy panel's button `spy_network` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopBuildingSpyNetwork "stop_building_spy_network" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sends a diplomat to counter a country's espionage. As the diplomacy panel's button `counterespionage` sends it
    /// (`diplomacy.options` says whether the player can).
    game CounterEspionage "counter_espionage" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Recalls the diplomat countering a country's espionage. As the diplomacy panel's button `counterespionage` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopCounterEspionage "stop_counter_espionage" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Fabricates a claim on a province of a country, with the spy network. As the diplomacy panel's button `fabricate_claim` sends it
    /// (`diplomacy.options` says whether the player can).
    game FabricateClaim "fabricate_claim" {
        /// The other country.
        country: CountryTag,
        /// The province to fabricate a claim on, one the country owns.
        province: ProvinceId,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Claims a country's states, with the spy network. As the diplomacy panel's button `claim_states` sends it
    /// (`diplomacy.options` says whether the player can).
    game ClaimStates "claim_states" {
        /// The other country.
        country: CountryTag,
        /// A province of the state to claim, one the country owns.
        province: ProvinceId,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Justifies a trade conflict with a country, with the spy network. As the diplomacy panel's button `justify_trade_conflict` sends it
    /// (`diplomacy.options` says whether the player can).
    game JustifyTradeConflict "justify_trade_conflict" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Supports rebels in a country, with the spy network. As the diplomacy panel's button `support_rebels` sends it
    /// (`diplomacy.options` says whether the player can).
    game SupportRebels "support_rebels" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Steals a country's maps, with the spy network. As the diplomacy panel's button `stealmap` sends it
    /// (`diplomacy.options` says whether the player can).
    game StealMaps "steal_maps" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Slanders a country's merchants, with the spy network. As the diplomacy panel's button `slander_merchants` sends it
    /// (`diplomacy.options` says whether the player can).
    game SlanderMerchants "slander_merchants" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sabotages a country's recruitment, with the spy network. As the diplomacy panel's button `sabotage_recruitment` sends it
    /// (`diplomacy.options` says whether the player can).
    game SabotageRecruitment "sabotage_recruitment" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sows discontent in a country, with the spy network. As the diplomacy panel's button `sow_discontent` sends it
    /// (`diplomacy.options` says whether the player can).
    game SowDiscontent "sow_discontent" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sabotages a country's reputation, with the spy network. As the diplomacy panel's button `sabotage_reputation` sends it
    /// (`diplomacy.options` says whether the player can).
    game SabotageReputation "sabotage_reputation" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Corrupts a country's officials, with the spy network. As the diplomacy panel's button `corrupt_officials` sends it
    /// (`diplomacy.options` says whether the player can).
    game CorruptOfficials "corrupt_officials" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Infiltrates a country's administration, with the spy network. As the diplomacy panel's button `infiltrate_administration` sends it
    /// (`diplomacy.options` says whether the player can).
    game InfiltrateAdministration "infiltrate_administration" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Agitates for liberty in a subject, with the spy network. As the diplomacy panel's button `agitate_for_liberty` sends it
    /// (`diplomacy.options` says whether the player can).
    game AgitateForLiberty "agitate_for_liberty" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Has a subject or a country transfer trade power to the player. As the diplomacy panel's button `transfer_trade_power` sends it
    /// (`diplomacy.options` says whether the player can).
    game TransferTradePower "transfer_trade_power" {
        /// The other country.
        country: CountryTag,
        /// The share of the country's trade power to transfer, in percent.
        percent: u32,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops the transfer of trade power from a country. As the diplomacy panel's button `transfer_trade_power` sends it
    /// (`diplomacy.options` says whether the player can).
    game CancelTradePowerTransfer "cancel_trade_power_transfer" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks a country to steer trade to the player. As the diplomacy panel's button `steer_trade` sends it
    /// (`diplomacy.options` says whether the player can).
    game SteerTrade "steer_trade" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Ends a country steering trade to the player. As the diplomacy panel's button `steer_trade` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopSteeringTrade "stop_steering_trade" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Embargoes a country's trade. As the diplomacy panel's button `embargoaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game Embargo "embargo" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Lifts the embargo on a country. As the diplomacy panel's button `embargoaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game LiftEmbargo "lift_embargo" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks a country to take the player's country as its tributary. As the diplomacy panel's button `ask_for_tributary_state_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game AskToBeTributary "ask_to_be_tributary" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Guarantees a country's independence. As the diplomacy panel's button `guaranteeaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game Guarantee "guarantee" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops guaranteeing a country's independence. As the diplomacy panel's button `guaranteeaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game CancelGuarantee "cancel_guarantee" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Warns a country against expanding near the player. As the diplomacy panel's button `warningaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game Warn "warn" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Threatens a country with war to have it give up a province or a claim. As the diplomacy panel's button `threaten_war` sends it
    /// (`diplomacy.options` says whether the player can).
    game ThreatenWar "threaten_war" {
        /// The other country.
        country: CountryTag,
        /// The province to demand.
        province: ProvinceId,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks a country for military access. As the diplomacy panel's button `milaccess` sends it
    /// (`diplomacy.options` says whether the player can).
    game AskMilitaryAccess "ask_military_access" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Gives up the military access a country gives the player. As the diplomacy panel's button `milaccess` sends it
    /// (`diplomacy.options` says whether the player can).
    game CancelMilitaryAccess "cancel_military_access" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Offers a country military access. As the diplomacy panel's button `offermilaccess` sends it
    /// (`diplomacy.options` says whether the player can).
    game OfferMilitaryAccess "offer_military_access" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Revokes the military access the player gives a country. As the diplomacy panel's button `offermilaccess` sends it
    /// (`diplomacy.options` says whether the player can).
    game RevokeMilitaryAccess "revoke_military_access" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks a country for fleet basing rights. As the diplomacy panel's button `fleet_access` sends it
    /// (`diplomacy.options` says whether the player can).
    game AskFleetBasing "ask_fleet_basing" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Gives up the fleet basing rights a country gives the player. As the diplomacy panel's button `fleet_access` sends it
    /// (`diplomacy.options` says whether the player can).
    game CancelFleetBasing "cancel_fleet_basing" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Offers a country fleet basing rights. As the diplomacy panel's button `offer_fleet_access` sends it
    /// (`diplomacy.options` says whether the player can).
    game OfferFleetBasing "offer_fleet_basing" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Revokes the fleet basing rights the player gives a country. As the diplomacy panel's button `offer_fleet_access` sends it
    /// (`diplomacy.options` says whether the player can).
    game RevokeFleetBasing "revoke_fleet_basing" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Shares the player's knowledge of technology with a country. As the diplomacy panel's button `knowledge_sharing` sends it
    /// (`diplomacy.options` says whether the player can).
    game ShareKnowledge "share_knowledge" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops sharing knowledge with a country. As the diplomacy panel's button `knowledge_sharing` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopSharingKnowledge "stop_sharing_knowledge" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks a country to share its knowledge of technology. As the diplomacy panel's button `ask_knowledge_sharing` sends it
    /// (`diplomacy.options` says whether the player can).
    game AskForKnowledgeSharing "ask_for_knowledge_sharing" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks a country about the empire, as the panel's imperial relations action does. As the diplomacy panel's button `imperial_relations_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game ImperialRelations "imperial_relations" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Declares an imperial realm war on a country, as the emperor. As the diplomacy panel's button `imperial_realm_war_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game ImperialRealmWar "imperial_realm_war" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sells ships to a country. As the diplomacy panel's button `sell_ships_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game SellShips "sell_ships" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Shares the player's maps with a country. As the diplomacy panel's button `sharemap` sends it
    /// (`diplomacy.options` says whether the player can).
    game ShareMaps "share_maps" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sends a country a gift of ducats. As the diplomacy panel's button `giftaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game SendGift "send_gift" {
        /// The other country.
        country: CountryTag,
        /// How many ducats to give, at least 1 and no more than the treasury, as
        /// the gift window allows.
        amount: u32,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Takes on a country's debt, as a great power. As the diplomacy panel's button `takeondebt` sends it
    /// (`diplomacy.options` says whether the player can).
    game TakeOnDebt "take_on_debt" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Influences a country, as a great power. As the diplomacy panel's button `influence_nation` sends it
    /// (`diplomacy.options` says whether the player can).
    game Influence "influence" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Intervenes in a country's war, as a great power. As the diplomacy panel's button `intervene_in_war` sends it
    /// (`diplomacy.options` says whether the player can).
    game InterveneInWar "intervene_in_war" {
        /// The other country.
        country: CountryTag,
        /// The enemy in whose war with the country the player intervenes.
        enemy: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Asks a country to break an alliance. As the diplomacy panel's button `break_alliance` sends it
    /// (`diplomacy.options` says whether the player can).
    game BreakAlliance "break_alliance" {
        /// The other country.
        country: CountryTag,
        /// The ally of the country whose alliance it is to break.
        ally: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes to have it break an alliance. As the diplomacy panel's button `break_alliance_favor` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsToBreakAlliance "trade_favors_to_break_alliance" {
        /// The other country.
        country: CountryTag,
        /// The ally of the country whose alliance it is to break.
        ally: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes to have it worsen its relations with another. As the diplomacy panel's button `reduce_relations_favor` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsToReduceRelations "trade_favors_to_reduce_relations" {
        /// The other country.
        country: CountryTag,
        /// The country it is to worsen its relations with.
        target: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Insults a country. As the diplomacy panel's button `insultaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game Insult "insult" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Claims a country's throne, where a royal marriage and the same dynasty
    /// allow it. As the diplomacy panel's button `claimaction` sends it
    /// (`diplomacy.options` says whether the player can).
    game ClaimThrone "claim_throne" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Offers a country a loan. As the diplomacy panel's button `offerloan` sends it
    /// (`diplomacy.options` says whether the player can).
    game OfferLoan "offer_loan" {
        /// The other country.
        country: CountryTag,
        /// How many ducats to lend.
        amount: u32,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Gives a country war subsidies. As the diplomacy panel's button `warsubsidy` sends it
    /// (`diplomacy.options` says whether the player can).
    game SubsidizeWar "subsidize_war" {
        /// The other country.
        country: CountryTag,
        /// Ducats a month, at least 1.
        amount: u32,
        /// How long, as the subsidies window counts it: 10 to 400, by 10; 10,
        /// as the window starts, when left out.
        duration: Option<u32>,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Stops giving a country war subsidies. As the diplomacy panel's button `warsubsidy` sends it
    /// (`diplomacy.options` says whether the player can).
    game StopWarSubsidies "stop_war_subsidies" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Sells a province to a country. As the diplomacy panel's button `sellprov` sends it
    /// (`diplomacy.options` says whether the player can).
    game SellProvince "sell_province" {
        /// The other country.
        country: CountryTag,
        /// The province to sell, one the player controls, not the capital.
        province: ProvinceId,
        /// The price, in ducats.
        price: u32,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Spends favors a country owes to have it return a core province. As the diplomacy panel's button `return_core_favor` sends it
    /// (`diplomacy.options` says whether the player can).
    game TradeFavorsForCore "trade_favors_for_core" {
        /// The other country.
        country: CountryTag,
        /// The province, a core of the player's that the country owns.
        province: ProvinceId,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Offers a country an army as condottieri. As the diplomacy panel's button `condottieri_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game OfferCondottieri "offer_condottieri" {
        /// The other country.
        country: CountryTag,
        /// The player's army to hire out.
        army: ArmyId,
        /// The price, in ducats.
        price: u32,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Recalls the condottieri a country hires from the player. As the diplomacy panel's button `condottieri_action` sends it
    /// (`diplomacy.options` says whether the player can).
    game RecallCondottieri "recall_condottieri" {
        /// The other country.
        country: CountryTag,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Charters a trade company in a province of a country. As the diplomacy panel's button `charter_company` sends it
    /// (`diplomacy.options` says whether the player can).
    game CharterCompany "charter_company" {
        /// The other country.
        country: CountryTag,
        /// The province to charter a company in.
        province: ProvinceId,
        /// The price, in ducats.
        price: u32,
    } -> Outcome, "answer: done, accepted, declined or pending";
    /// Appoints a cardinal in a country. As the diplomacy panel's button `appoint_cardinal` sends it
    /// (`diplomacy.options` says whether the player can).
    game AppointCardinal "appoint_cardinal" {
        /// The other country.
        country: CountryTag,
        /// Whether to pay for it from the treasury, as the window's box; no when
        /// left out.
        pay: Option<bool>,
    } -> Outcome, "answer: done, accepted, declined or pending";
}

/// What the diplomacy panel offers toward a country.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Options {
    pub country: CountryTag,
    /// Each action the panel shows a button for, in its order.
    pub actions: Vec<Offered>,
}

/// An action the diplomacy panel offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Offered {
    /// The capability that sends it, in this group, such as
    /// `propose_alliance`; `scripted_action` for a scripted one, whose
    /// content key is `key`; `unknown` for one the framework cannot send.
    pub action: String,
    /// The key of its type, as in saves (`allianceaction`); a scripted
    /// action's content key (`sell_indulgence`).
    pub key: String,
    /// Its name, as the panel's button shows it, in the player language.
    pub name: String,
    #[serde(flatten)]
    pub choice: Choice,
    /// What its tooltip says after the reasons, one a line.
    pub description: Vec<String>,
    /// Whether the country would accept it, as the panel shows it; only for
    /// an action that needs an answer, and not for every such one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub acceptance: Option<Acceptance>,
}

/// A war goal the game offers for a declaration of war.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct WarGoal {
    /// Its casus belli's key, such as `cb_claim`; `none` for declaring war
    /// without one.
    pub casus_belli: String,
    /// Its type's key, such as `take_claim`; null when it has none.
    pub war_goal: Option<String>,
    /// The province it is about, if any.
    pub province: Option<ProvinceId>,
    /// The country it is about, if any.
    pub target: Option<CountryTag>,
    /// Its name, as the declare war window shows it.
    pub name: String,
}

/// What came of a diplomatic action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Outcome {
    pub answer: AnswerKind,
    /// The war that began, by its name as the game shows it, for `declared`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub war: Option<String>,
    /// For a declaration of war, the allies it called to arms; empty when it
    /// called none.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub called: Option<Vec<CountryTag>>,
    /// For a declaration of war, under `call_allies` `all`, every ally the
    /// declare war window listed that it did not call, and why; empty
    /// under `none` or allies named.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub passed_over: Option<Vec<PassedOver>>,
}

/// An ally a declaration of war did not call to arms: the declare war
/// window's call box would not let the player tick it, or the window no
/// longer listed it once others were called.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct PassedOver {
    pub country: CountryTag,
    /// Why, one a line: the reasons the call box's check gives, as its
    /// tooltip shows them (for an AI ally, whether it would accept, and
    /// each factor for and against); or that the window no longer lists it
    /// once others are called.
    pub reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum AnswerKind {
    /// The game carried out an action that needs no answer.
    Done,
    /// The other country accepted the proposal.
    Accepted,
    /// The other country declined it.
    Declined,
    /// The war began.
    Declared,
    /// No answer yet: the game has not carried the action out, or the
    /// other country has not answered.
    Pending,
}

/// Which of the player's allies a declaration of war calls to arms
/// (`call_allies`). As text: `all`, `none`, or the tags of those to call,
/// separated by commas.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CallAllies {
    /// Each ally the declare war window would let the player call.
    #[default]
    All,
    None,
    /// These, each one the window must let the player call.
    Only(Vec<CountryTag>),
}

impl CallAllies {
    /// Read from its text, in either case and with spaces around its words.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim() {
            "all" => Ok(Self::All),
            "none" => Ok(Self::None),
            list => list
                .split(',')
                .map(str::trim)
                .map(|word| CountryTag::parse(word).ok_or_else(|| format!("call_allies is all, none, or country tags separated by commas; {word:?} is not a tag")))
                .collect::<Result<_, _>>()
                .map(Self::Only),
        }
    }
}

impl std::fmt::Display for CallAllies {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("all"),
            Self::None => f.write_str("none"),
            Self::Only(tags) => f.write_str(&tags.iter().map(CountryTag::to_string).collect::<Vec<_>>().join(",")),
        }
    }
}

/// `"all"`, `"none"`, or a list of tags.
impl Serialize for CallAllies {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::All => serializer.serialize_str("all"),
            Self::None => serializer.serialize_str("none"),
            Self::Only(tags) => tags.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for CallAllies {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Word(String),
            Tags(Vec<CountryTag>),
        }
        match Form::deserialize(deserializer)? {
            Form::Word(word) if word.trim() == "all" => Ok(Self::All),
            Form::Word(word) if word.trim() == "none" => Ok(Self::None),
            Form::Word(word) => Err(serde::de::Error::custom(format!("call_allies is all, none, or a list of country tags; {word:?} is none of them"))),
            Form::Tags(tags) => Ok(Self::Only(tags)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{request_from_json, Answer, Request, Response};
    use serde_json::json;

    #[test]
    fn a_declaration_names_its_war_and_a_proposal_does_not() {
        let passed_over = vec![PassedOver { country: CountryTag(*b"CND"), reasons: vec!["They would not accept.".into()] }];
        let outcome = Outcome { answer: AnswerKind::Declared, war: Some("Ottoman-Byzantine War".into()), called: Some(vec![CountryTag(*b"DUL")]), passed_over: Some(passed_over) };
        let declared: Response = Ok(Answer::Diplomacy(DiplomacyAnswer::DeclareWar(outcome)));
        assert_eq!(
            crate::response_json(&declared),
            json!({"ok": true, "result": {"answer": "declared", "war": "Ottoman-Byzantine War", "called": ["DUL"], "passed_over": [{"country": "CND", "reasons": ["They would not accept."]}]}})
        );
        let declined: Response = Ok(Answer::Diplomacy(DiplomacyAnswer::ProposeAlliance(Outcome { answer: AnswerKind::Declined, war: None, called: None, passed_over: None })));
        assert_eq!(crate::response_json(&declined), json!({"ok": true, "result": {"answer": "declined"}}));
    }

    #[test]
    fn a_war_goal_in_json() {
        let goal = WarGoal { casus_belli: "none".into(), war_goal: None, province: Some(ProvinceId(151)), target: Some(CountryTag(*b"BYZ")), name: "Conquer Constantinople".into() };
        assert_eq!(
            serde_json::to_value(&goal).unwrap(),
            json!({"casus_belli": "none", "war_goal": null, "province": 151, "target": "BYZ", "name": "Conquer Constantinople"})
        );
    }

    #[test]
    fn a_declaration_needs_only_its_country_and_casus_belli() {
        let request = request_from_json(r#"{"capability": "diplomacy.declare_war", "country": "BYZ", "casus_belli": "none"}"#).unwrap();
        assert_eq!(
            request,
            Request::Diplomacy(DiplomacyRequest::DeclareWar { country: CountryTag(*b"BYZ"), casus_belli: "none".into(), war_goal: None, province: None, target: None, call_allies: None })
        );
        let request = request_from_json(r#"{"capability": "diplomacy.declare_war", "country": "BYZ", "casus_belli": "none", "call_allies": ["DUL", "CND"]}"#).unwrap();
        assert!(matches!(request, Request::Diplomacy(DiplomacyRequest::DeclareWar { call_allies: Some(ref c), .. }) if *c == CallAllies::Only(vec![CountryTag(*b"DUL"), CountryTag(*b"CND")])));
    }
}
