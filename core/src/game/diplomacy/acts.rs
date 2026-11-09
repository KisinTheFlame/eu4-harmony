//! The actions the diplomacy panel offers the player, each by the game's type
//! of diplomatic action and whether it starts or ends something
//! (docs/research/diplomacy.md, "The diplomacy panel").
//!
//! A type is a token of the game's, the number its key has in saves; the
//! game's table of tokens holds each with its key, and each act's entry there
//! is a dependency point. An act's name is the capability's that does it,
//! and its details, in parentheses, are the arguments its capability chooses in
//! the window the act opens ([`super::Details`]), by their names.
use crate::points::Point;
use eu4_harmony_remote::diplomacy::{DiplomacyAnswer, Outcome};

macro_rules! acts {
    ($( $Act:ident $token:literal $offer:literal $name:literal $at:literal $key:literal $(($($detail:ident),+))?; )*) => {
        /// One of the actions the diplomacy panel offers.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Act {
            $( $Act, )*
        }

        /// Each act: its type's token, whether it starts something (the
        /// action's offer byte), its name, and where the game's table of
        /// tokens holds the token with its key.
        const ACTS: &[(Act, u32, bool, &str, usize, &str)] = &[
            $( (Act::$Act, $token, $offer != 0, $name, $at, $key), )*
        ];

        impl Act {
            /// The arguments its capability chooses in the window it opens,
            /// besides the country it is sent to.
            pub fn details(self) -> &'static [&'static str] {
                match self {
                    $( Act::$Act => &[$($(stringify!($detail)),+)?], )*
                }
            }

            /// Its capability's answer, around what came of it.
            pub fn answer(self, outcome: Outcome) -> DiplomacyAnswer {
                match self {
                    $( Act::$Act => DiplomacyAnswer::$Act(outcome), )*
                }
            }
        }
    };
}

acts! {
    OfferPeace 0x293c 1 "offer_peace" 0x2094090 "requestpeace";
    SurrenderUnconditionally 0x33c4 1 "surrender_unconditionally" 0x21d4158 "unconditional_surrender";
    DeclareWar 0x293b 1 "declare_war" 0x2093e88 "declarewar";
    ProposeAlliance 0x293d 1 "propose_alliance" 0x2094298 "allianceaction";
    CancelAlliance 0x293d 0 "cancel_alliance" 0x2094298 "allianceaction";
    SupportIndependence 0x2f2e 1 "support_independence" 0x21a47a0 "support_independence_action";
    StopSupportingIndependence 0x2f2e 0 "stop_supporting_independence" 0x21a47a0 "support_independence_action";
    CallToArms 0x294d 1 "call_to_arms" 0x2096520 "callaction";
    FormCoalition 0x2de0 1 "form_coalition" 0x2183370 "form_coalition";
    LeaveCoalition 0x2de0 0 "leave_coalition" 0x2183370 "form_coalition";
    EnforcePeace 0x2dda 1 "enforce_peace" 0x2182740 "enforce_peace" (enemy);
    DemandReligiousUnity 0x2d48 1 "demand_religious_unity" 0x21700b8 "religious_unity_action";
    RemoveElector 0x3177 1 "remove_elector" 0x21c8e98 "remove_electorate";
    GrantElector 0x2b1c 1 "grant_elector" 0x20d0568 "grant_electorate";
    RemoveFreeCity 0x31f7 1 "remove_free_city" 0x21e69e8 "remove_freecity";
    GrantFreeCity 0x31f6 1 "grant_free_city" 0x21e67e0 "grant_freecity";
    DemandUnlawfulTerritory 0x2e0b 1 "demand_unlawful_territory" 0x21888c0 "demand_unlawful_territory_action" (province);
    CallCrusade 0x2cd5 1 "call_crusade" 0x2161720 "call_crusade_action";
    Excommunicate 0x2cd6 1 "excommunicate" 0x2161928 "excommunicate_action";
    LiftExcommunication 0x2cd6 0 "lift_excommunication" 0x2161928 "excommunicate_action";
    InviteToFederation 0x2f0f 1 "invite_to_federation" 0x21a2f40 "invite_to_federation";
    RemoveFromFederation 0x2f0f 0 "remove_from_federation" 0x21a2f40 "invite_to_federation";
    AskToJoinFederation 0x2f0e 1 "ask_to_join_federation" 0x21a2d38 "request_to_join_federation";
    LeaveFederation 0x2f0e 0 "leave_federation" 0x21a2d38 "request_to_join_federation";
    InviteToTradeLeague 0x331d 1 "invite_to_trade_league" 0x21d3320 "invite_to_trade_league";
    RemoveFromTradeLeague 0x331d 0 "remove_from_trade_league" 0x21d3320 "invite_to_trade_league";
    InviteToEmpire 0x3b85 1 "invite_to_empire" 0x22bc480 "invite_to_hre";
    AskToJoinTradeLeague 0x3320 1 "ask_to_join_trade_league" 0x21d3938 "request_to_join_trade_league";
    ImproveRelations 0x2ddb 1 "improve_relations" 0x2182948 "improve_relation";
    StopImprovingRelations 0x2ddb 0 "stop_improving_relations" 0x2182948 "improve_relation";
    Annex 0x293f 1 "annex" 0x20946a8 "annexationaction";
    CancelAnnexation 0x293f 0 "cancel_annexation" 0x20946a8 "annexationaction";
    Integrate 0x2a9c 1 "integrate" 0x20c0780 "integrationaction";
    CancelIntegration 0x2a9c 0 "cancel_integration" 0x20c0780 "integrationaction";
    DesignateMarch 0x301d 1 "designate_march" 0x21b0488 "designate_march";
    RevokeMarch 0x301d 0 "revoke_march" 0x21b0488 "designate_march";
    AbandonUnion 0x34af 1 "abandon_union" 0x220f5f0 "abandon_union";
    OfferVassalization 0x2940 1 "offer_vassalization" 0x20948b0 "vassalaction";
    ReleaseVassal 0x2940 0 "release_vassal" 0x20948b0 "vassalaction";
    AskForMarch 0x3021 1 "ask_for_march" 0x21b0898 "ask_for_march";
    DemandTributary 0x35bd 1 "demand_tributary" 0x2220418 "tributary_state_action";
    ReleaseTributary 0x35bd 0 "release_tributary" 0x2220418 "tributary_state_action";
    ArrangeRoyalMarriage 0x28df 1 "arrange_royal_marriage" 0x20885b0 "royal_marriage";
    BreakRoyalMarriage 0x28df 0 "break_royal_marriage" 0x20885b0 "royal_marriage";
    ConvertTributaryToVassal 0x3c89 1 "convert_tributary_to_vassal" 0x22d99b8 "convert_tributary_to_vassal";
    TradeFavorsForGold 0x3bce 1 "trade_favors_for_gold" 0x22c56c0 "trade_favors_for_gold";
    TradeFavorsForMen 0x3bcf 1 "trade_favors_for_men" 0x22c58c8 "trade_favors_for_men";
    TradeFavorsForSailors 0x3bd0 1 "trade_favors_for_sailors" 0x22c5ad0 "trade_favors_for_sailors";
    TradeFavorsForHeir 0x3bd2 1 "trade_favors_for_heir" 0x22c5ee0 "trade_favors_for_heir";
    TradeFavorsForTrust 0x3bd3 1 "trade_favors_for_trust" 0x22c60e8 "trade_favors_for_trust" (favors, trust);
    TradeFavorsForWarPreparations 0x3bd4 1 "trade_favors_for_war_preparations" 0x22c62f0 "trade_favors_for_war_prep";
    SupportHeir 0x2ff6 1 "support_heir" 0x20ddec0 "support_heir";
    StopSupportingHeir 0x2ff6 0 "stop_supporting_heir" 0x20ddec0 "support_heir";
    CurryFavors 0x3acc 1 "curry_favors" 0x22a6518 "curry_favors";
    StopCurryingFavors 0x3acc 0 "stop_currying_favors" 0x22a6518 "curry_favors";
    BuildSpyNetwork 0x3378 1 "build_spy_network" 0x21fcd60 "spy_network";
    StopBuildingSpyNetwork 0x3378 0 "stop_building_spy_network" 0x21fcd60 "spy_network";
    CounterEspionage 0x33f9 1 "counter_espionage" 0x2203f20 "counterespionage";
    StopCounterEspionage 0x33f9 0 "stop_counter_espionage" 0x2203f20 "counterespionage";
    FabricateClaim 0x2af1 1 "fabricate_claim" 0x20cb018 "fabricate_claim" (province);
    ClaimStates 0x36b6 1 "claim_states" 0x20f9990 "claim_states" (province);
    JustifyTradeConflict 0x2f5e 1 "justify_trade_conflict" 0x21a6e38 "justify_trade_conflict";
    SupportRebels 0x2dde 1 "support_rebels" 0x2182f60 "support_rebels" (rebels);
    StealMaps 0x33c8 1 "steal_maps" 0x21d4978 "stealmap";
    SlanderMerchants 0x337b 1 "slander_merchants" 0x21fd378 "slander_merchants";
    SabotageRecruitment 0x33bf 1 "sabotage_recruitment" 0x22022b0 "sabotage_recruitment";
    SowDiscontent 0x2ddf 1 "sow_discontent" 0x2183168 "sow_discontent";
    SabotageReputation 0x2ddd 1 "sabotage_reputation" 0x2182d58 "sabotage_reputation";
    CorruptOfficials 0x337c 1 "corrupt_officials" 0x21fd580 "corrupt_officials";
    InfiltrateAdministration 0x2ddc 1 "infiltrate_administration" 0x2182b50 "infiltrate_administration";
    AgitateForLiberty 0x3242 1 "agitate_for_liberty" 0x21ce5f0 "agitate_for_liberty";
    TransferTradePower 0x2e09 1 "transfer_trade_power" 0x21886b8 "transfer_trade_power" (percent);
    CancelTradePowerTransfer 0x2e09 0 "cancel_trade_power_transfer" 0x21886b8 "transfer_trade_power";
    SteerTrade 0x3778 1 "steer_trade" 0x2243ad0 "steer_trade";
    StopSteeringTrade 0x3778 0 "stop_steering_trade" 0x2243ad0 "steer_trade";
    Embargo 0x293e 1 "embargo" 0x20944a0 "embargoaction";
    LiftEmbargo 0x293e 0 "lift_embargo" 0x20944a0 "embargoaction";
    AskToBeTributary 0x35c4 1 "ask_to_be_tributary" 0x2221250 "ask_for_tributary_state_action";
    Guarantee 0x2941 1 "guarantee" 0x2094ab8 "guaranteeaction";
    CancelGuarantee 0x2941 0 "cancel_guarantee" 0x2094ab8 "guaranteeaction";
    Warn 0x2942 1 "warn" 0x2094cc0 "warningaction";
    ThreatenWar 0x3307 1 "threaten_war" 0x21d0670 "threaten_war" (province);
    AskMilitaryAccess 0x2943 1 "ask_military_access" 0x2094ec8 "milaccess";
    CancelMilitaryAccess 0x2943 0 "cancel_military_access" 0x2094ec8 "milaccess";
    OfferMilitaryAccess 0x2944 1 "offer_military_access" 0x20950d0 "offermilaccess";
    RevokeMilitaryAccess 0x2944 0 "revoke_military_access" 0x20950d0 "offermilaccess";
    AskFleetBasing 0x2dd6 1 "ask_fleet_basing" 0x2181f20 "fleet_access";
    CancelFleetBasing 0x2dd6 0 "cancel_fleet_basing" 0x2181f20 "fleet_access";
    OfferFleetBasing 0x2dd7 1 "offer_fleet_basing" 0x2182128 "offer_fleet_access";
    RevokeFleetBasing 0x2dd7 0 "revoke_fleet_basing" 0x2182128 "offer_fleet_access";
    ShareKnowledge 0x3776 1 "share_knowledge" 0x22436c0 "knowledge_sharing";
    StopSharingKnowledge 0x3776 0 "stop_sharing_knowledge" 0x22436c0 "knowledge_sharing";
    AskForKnowledgeSharing 0x3edc 1 "ask_for_knowledge_sharing" 0x2322398 "ask_knowledge_sharing";
    ImperialRelations 0x2d46 1 "imperial_relations" 0x216fca8 "imperial_relations_action";
    ImperialRealmWar 0x3a6b 1 "imperial_realm_war" 0x229d2d8 "imperial_realm_war_action";
    SellShips 0x3028 1 "sell_ships" 0x21b12c0 "sell_ships_action";
    ShareMaps 0x3323 1 "share_maps" 0x21d3f50 "sharemap";
    SendGift 0x2948 1 "send_gift" 0x20958f0 "giftaction" (amount);
    TakeOnDebt 0x3469 1 "take_on_debt" 0x21d7830 "takeondebt";
    Influence 0x346e 1 "influence" 0x21d8258 "influence_nation";
    InterveneInWar 0x343e 1 "intervene_in_war" 0x2209880 "intervene_in_war" (enemy);
    BreakAlliance 0x3375 1 "break_alliance" 0x21fc748 "break_alliance" (ally);
    TradeFavorsToBreakAlliance 0x3bcb 1 "trade_favors_to_break_alliance" 0x22c50a8 "break_alliance_favor" (ally);
    TradeFavorsToReduceRelations 0x3bcd 1 "trade_favors_to_reduce_relations" 0x22c54b8 "reduce_relations_favor" (target);
    Insult 0x2947 1 "insult" 0x20956e8 "insultaction";
    ClaimThrone 0x2949 1 "claim_throne" 0x2095af8 "claimaction";
    OfferLoan 0x2a24 1 "offer_loan" 0x20b15c8 "offerloan" (amount);
    SubsidizeWar 0x2a25 1 "subsidize_war" 0x20b17d0 "warsubsidy" (amount, duration);
    StopWarSubsidies 0x2a25 0 "stop_war_subsidies" 0x20b17d0 "warsubsidy";
    SellProvince 0x2a29 1 "sell_province" 0x20b1ff0 "sellprov" (province, price);
    TradeFavorsForCore 0x3bcc 1 "trade_favors_for_core" 0x22c52b0 "return_core_favor" (province);
    OfferCondottieri 0x33b9 1 "offer_condottieri" 0x2201680 "condottieri_action" (army, price);
    RecallCondottieri 0x33b9 0 "recall_condottieri" 0x2201680 "condottieri_action";
    CharterCompany 0x3866 1 "charter_company" 0x225f5a0 "charter_company" (province, price);
    AppointCardinal 0x39c1 1 "appoint_cardinal" 0x22885b8 "appoint_cardinal" (pay);
}

/// The token of a scripted diplomatic action, one of
/// `common/new_diplomatic_actions`, whose content key the action holds.
pub const SCRIPTED: u32 = 0x3662;

/// The types of diplomatic action whose `ai_accept` the diplomacy panel never
/// shows, though they need an answer (the table at `0x1c35460`).
pub const NO_ACCEPTANCE: [u32; 12] = [0x3028, 0x2a29, 0x3bcc, 0x2de0, 0x2dda, 0x293c, 0x3307, 0x33b9, 0x3375, 0x3bcb, 0x3bcd, 0x3866];
const NO_ACCEPTANCE_AT: usize = 0x1c35460;

impl Act {
    /// The act of a type, starting or ending.
    pub fn of(token: u32, offer: bool) -> Option<Self> {
        ACTS.iter().find(|a| a.1 == token && a.2 == offer).map(|a| a.0)
    }

    /// The act named so, as its capability is.
    pub fn named(name: &str) -> Option<Self> {
        ACTS.iter().find(|a| a.3 == name).map(|a| a.0)
    }

    fn entry(self) -> &'static (Act, u32, bool, &'static str, usize, &'static str) {
        ACTS.iter().find(|a| a.0 == self).expect("every act is in the table")
    }

    pub fn token(self) -> u32 {
        self.entry().1
    }

    pub fn offer(self) -> bool {
        self.entry().2
    }

    pub fn name(self) -> &'static str {
        self.entry().3
    }

    /// The key of its type in saves, such as `allianceaction`.
    pub fn key(self) -> &'static str {
        self.entry().5
    }
}

/// Where the game's table of tokens holds each type's token, its key and a
/// NUL, as their bytes.
const TYPES: &[(usize, &str)] = &[
    (0x2094090, "3c 29 00 00 72 65 71 75 65 73 74 70 65 61 63 65 00"),
    (0x21d4158, "c4 33 00 00 75 6e 63 6f 6e 64 69 74 69 6f 6e 61 6c 5f 73 75 72 72 65 6e 64 65 72 00"),
    (0x2093e88, "3b 29 00 00 64 65 63 6c 61 72 65 77 61 72 00"),
    (0x2094298, "3d 29 00 00 61 6c 6c 69 61 6e 63 65 61 63 74 69 6f 6e 00"),
    (0x21a47a0, "2e 2f 00 00 73 75 70 70 6f 72 74 5f 69 6e 64 65 70 65 6e 64 65 6e 63 65 5f 61 63 74 69 6f 6e 00"),
    (0x2096520, "4d 29 00 00 63 61 6c 6c 61 63 74 69 6f 6e 00"),
    (0x2183370, "e0 2d 00 00 66 6f 72 6d 5f 63 6f 61 6c 69 74 69 6f 6e 00"),
    (0x2182740, "da 2d 00 00 65 6e 66 6f 72 63 65 5f 70 65 61 63 65 00"),
    (0x21700b8, "48 2d 00 00 72 65 6c 69 67 69 6f 75 73 5f 75 6e 69 74 79 5f 61 63 74 69 6f 6e 00"),
    (0x21c8e98, "77 31 00 00 72 65 6d 6f 76 65 5f 65 6c 65 63 74 6f 72 61 74 65 00"),
    (0x20d0568, "1c 2b 00 00 67 72 61 6e 74 5f 65 6c 65 63 74 6f 72 61 74 65 00"),
    (0x21e69e8, "f7 31 00 00 72 65 6d 6f 76 65 5f 66 72 65 65 63 69 74 79 00"),
    (0x21e67e0, "f6 31 00 00 67 72 61 6e 74 5f 66 72 65 65 63 69 74 79 00"),
    (0x21888c0, "0b 2e 00 00 64 65 6d 61 6e 64 5f 75 6e 6c 61 77 66 75 6c 5f 74 65 72 72 69 74 6f 72 79 5f 61 63 74 69 6f 6e 00"),
    (0x2161720, "d5 2c 00 00 63 61 6c 6c 5f 63 72 75 73 61 64 65 5f 61 63 74 69 6f 6e 00"),
    (0x2161928, "d6 2c 00 00 65 78 63 6f 6d 6d 75 6e 69 63 61 74 65 5f 61 63 74 69 6f 6e 00"),
    (0x21a2f40, "0f 2f 00 00 69 6e 76 69 74 65 5f 74 6f 5f 66 65 64 65 72 61 74 69 6f 6e 00"),
    (0x21a2d38, "0e 2f 00 00 72 65 71 75 65 73 74 5f 74 6f 5f 6a 6f 69 6e 5f 66 65 64 65 72 61 74 69 6f 6e 00"),
    (0x21d3320, "1d 33 00 00 69 6e 76 69 74 65 5f 74 6f 5f 74 72 61 64 65 5f 6c 65 61 67 75 65 00"),
    (0x22bc480, "85 3b 00 00 69 6e 76 69 74 65 5f 74 6f 5f 68 72 65 00"),
    (0x21d3938, "20 33 00 00 72 65 71 75 65 73 74 5f 74 6f 5f 6a 6f 69 6e 5f 74 72 61 64 65 5f 6c 65 61 67 75 65 00"),
    (0x2182948, "db 2d 00 00 69 6d 70 72 6f 76 65 5f 72 65 6c 61 74 69 6f 6e 00"),
    (0x20946a8, "3f 29 00 00 61 6e 6e 65 78 61 74 69 6f 6e 61 63 74 69 6f 6e 00"),
    (0x20c0780, "9c 2a 00 00 69 6e 74 65 67 72 61 74 69 6f 6e 61 63 74 69 6f 6e 00"),
    (0x21b0488, "1d 30 00 00 64 65 73 69 67 6e 61 74 65 5f 6d 61 72 63 68 00"),
    (0x220f5f0, "af 34 00 00 61 62 61 6e 64 6f 6e 5f 75 6e 69 6f 6e 00"),
    (0x20948b0, "40 29 00 00 76 61 73 73 61 6c 61 63 74 69 6f 6e 00"),
    (0x21b0898, "21 30 00 00 61 73 6b 5f 66 6f 72 5f 6d 61 72 63 68 00"),
    (0x2220418, "bd 35 00 00 74 72 69 62 75 74 61 72 79 5f 73 74 61 74 65 5f 61 63 74 69 6f 6e 00"),
    (0x20885b0, "df 28 00 00 72 6f 79 61 6c 5f 6d 61 72 72 69 61 67 65 00"),
    (0x22d99b8, "89 3c 00 00 63 6f 6e 76 65 72 74 5f 74 72 69 62 75 74 61 72 79 5f 74 6f 5f 76 61 73 73 61 6c 00"),
    (0x22c56c0, "ce 3b 00 00 74 72 61 64 65 5f 66 61 76 6f 72 73 5f 66 6f 72 5f 67 6f 6c 64 00"),
    (0x22c58c8, "cf 3b 00 00 74 72 61 64 65 5f 66 61 76 6f 72 73 5f 66 6f 72 5f 6d 65 6e 00"),
    (0x22c5ad0, "d0 3b 00 00 74 72 61 64 65 5f 66 61 76 6f 72 73 5f 66 6f 72 5f 73 61 69 6c 6f 72 73 00"),
    (0x22c5ee0, "d2 3b 00 00 74 72 61 64 65 5f 66 61 76 6f 72 73 5f 66 6f 72 5f 68 65 69 72 00"),
    (0x22c60e8, "d3 3b 00 00 74 72 61 64 65 5f 66 61 76 6f 72 73 5f 66 6f 72 5f 74 72 75 73 74 00"),
    (0x22c62f0, "d4 3b 00 00 74 72 61 64 65 5f 66 61 76 6f 72 73 5f 66 6f 72 5f 77 61 72 5f 70 72 65 70 00"),
    (0x20ddec0, "f6 2f 00 00 73 75 70 70 6f 72 74 5f 68 65 69 72 00"),
    (0x22a6518, "cc 3a 00 00 63 75 72 72 79 5f 66 61 76 6f 72 73 00"),
    (0x21fcd60, "78 33 00 00 73 70 79 5f 6e 65 74 77 6f 72 6b 00"),
    (0x2203f20, "f9 33 00 00 63 6f 75 6e 74 65 72 65 73 70 69 6f 6e 61 67 65 00"),
    (0x20cb018, "f1 2a 00 00 66 61 62 72 69 63 61 74 65 5f 63 6c 61 69 6d 00"),
    (0x20f9990, "b6 36 00 00 63 6c 61 69 6d 5f 73 74 61 74 65 73 00"),
    (0x21a6e38, "5e 2f 00 00 6a 75 73 74 69 66 79 5f 74 72 61 64 65 5f 63 6f 6e 66 6c 69 63 74 00"),
    (0x2182f60, "de 2d 00 00 73 75 70 70 6f 72 74 5f 72 65 62 65 6c 73 00"),
    (0x21d4978, "c8 33 00 00 73 74 65 61 6c 6d 61 70 00"),
    (0x21fd378, "7b 33 00 00 73 6c 61 6e 64 65 72 5f 6d 65 72 63 68 61 6e 74 73 00"),
    (0x22022b0, "bf 33 00 00 73 61 62 6f 74 61 67 65 5f 72 65 63 72 75 69 74 6d 65 6e 74 00"),
    (0x2183168, "df 2d 00 00 73 6f 77 5f 64 69 73 63 6f 6e 74 65 6e 74 00"),
    (0x2182d58, "dd 2d 00 00 73 61 62 6f 74 61 67 65 5f 72 65 70 75 74 61 74 69 6f 6e 00"),
    (0x21fd580, "7c 33 00 00 63 6f 72 72 75 70 74 5f 6f 66 66 69 63 69 61 6c 73 00"),
    (0x2182b50, "dc 2d 00 00 69 6e 66 69 6c 74 72 61 74 65 5f 61 64 6d 69 6e 69 73 74 72 61 74 69 6f 6e 00"),
    (0x21ce5f0, "42 32 00 00 61 67 69 74 61 74 65 5f 66 6f 72 5f 6c 69 62 65 72 74 79 00"),
    (0x21886b8, "09 2e 00 00 74 72 61 6e 73 66 65 72 5f 74 72 61 64 65 5f 70 6f 77 65 72 00"),
    (0x2243ad0, "78 37 00 00 73 74 65 65 72 5f 74 72 61 64 65 00"),
    (0x20944a0, "3e 29 00 00 65 6d 62 61 72 67 6f 61 63 74 69 6f 6e 00"),
    (0x2221250, "c4 35 00 00 61 73 6b 5f 66 6f 72 5f 74 72 69 62 75 74 61 72 79 5f 73 74 61 74 65 5f 61 63 74 69 6f 6e 00"),
    (0x2094ab8, "41 29 00 00 67 75 61 72 61 6e 74 65 65 61 63 74 69 6f 6e 00"),
    (0x2094cc0, "42 29 00 00 77 61 72 6e 69 6e 67 61 63 74 69 6f 6e 00"),
    (0x21d0670, "07 33 00 00 74 68 72 65 61 74 65 6e 5f 77 61 72 00"),
    (0x2094ec8, "43 29 00 00 6d 69 6c 61 63 63 65 73 73 00"),
    (0x20950d0, "44 29 00 00 6f 66 66 65 72 6d 69 6c 61 63 63 65 73 73 00"),
    (0x2181f20, "d6 2d 00 00 66 6c 65 65 74 5f 61 63 63 65 73 73 00"),
    (0x2182128, "d7 2d 00 00 6f 66 66 65 72 5f 66 6c 65 65 74 5f 61 63 63 65 73 73 00"),
    (0x22436c0, "76 37 00 00 6b 6e 6f 77 6c 65 64 67 65 5f 73 68 61 72 69 6e 67 00"),
    (0x2322398, "dc 3e 00 00 61 73 6b 5f 6b 6e 6f 77 6c 65 64 67 65 5f 73 68 61 72 69 6e 67 00"),
    (0x216fca8, "46 2d 00 00 69 6d 70 65 72 69 61 6c 5f 72 65 6c 61 74 69 6f 6e 73 5f 61 63 74 69 6f 6e 00"),
    (0x229d2d8, "6b 3a 00 00 69 6d 70 65 72 69 61 6c 5f 72 65 61 6c 6d 5f 77 61 72 5f 61 63 74 69 6f 6e 00"),
    (0x21b12c0, "28 30 00 00 73 65 6c 6c 5f 73 68 69 70 73 5f 61 63 74 69 6f 6e 00"),
    (0x21d3f50, "23 33 00 00 73 68 61 72 65 6d 61 70 00"),
    (0x20958f0, "48 29 00 00 67 69 66 74 61 63 74 69 6f 6e 00"),
    (0x21d7830, "69 34 00 00 74 61 6b 65 6f 6e 64 65 62 74 00"),
    (0x21d8258, "6e 34 00 00 69 6e 66 6c 75 65 6e 63 65 5f 6e 61 74 69 6f 6e 00"),
    (0x2209880, "3e 34 00 00 69 6e 74 65 72 76 65 6e 65 5f 69 6e 5f 77 61 72 00"),
    (0x21fc748, "75 33 00 00 62 72 65 61 6b 5f 61 6c 6c 69 61 6e 63 65 00"),
    (0x22c50a8, "cb 3b 00 00 62 72 65 61 6b 5f 61 6c 6c 69 61 6e 63 65 5f 66 61 76 6f 72 00"),
    (0x22c54b8, "cd 3b 00 00 72 65 64 75 63 65 5f 72 65 6c 61 74 69 6f 6e 73 5f 66 61 76 6f 72 00"),
    (0x20956e8, "47 29 00 00 69 6e 73 75 6c 74 61 63 74 69 6f 6e 00"),
    (0x2095af8, "49 29 00 00 63 6c 61 69 6d 61 63 74 69 6f 6e 00"),
    (0x20b15c8, "24 2a 00 00 6f 66 66 65 72 6c 6f 61 6e 00"),
    (0x20b17d0, "25 2a 00 00 77 61 72 73 75 62 73 69 64 79 00"),
    (0x20b1ff0, "29 2a 00 00 73 65 6c 6c 70 72 6f 76 00"),
    (0x22c52b0, "cc 3b 00 00 72 65 74 75 72 6e 5f 63 6f 72 65 5f 66 61 76 6f 72 00"),
    (0x2201680, "b9 33 00 00 63 6f 6e 64 6f 74 74 69 65 72 69 5f 61 63 74 69 6f 6e 00"),
    (0x225f5a0, "66 38 00 00 63 68 61 72 74 65 72 5f 63 6f 6d 70 61 6e 79 00"),
    (0x22885b8, "c1 39 00 00 61 70 70 6f 69 6e 74 5f 63 61 72 64 69 6e 61 6c 00"),
    (0x222b6d8, "62 36 00 00 73 63 72 69 70 74 61 62 6c 65 5f 64 69 70 6c 6f 6d 61 74 69 63 5f 61 63 74 69 6f 6e 00"),
];

/// The token table's entry of each type, and the types without acceptance.
pub fn points() -> Vec<Point> {
    let mut points: Vec<Point> = TYPES.iter().map(|&(at, bytes)| Point::keep(at, bytes, "diplomatic action type")).collect();
    points.push(Point::keep(
        NO_ACCEPTANCE_AT,
        "28 30 00 00 29 2a 00 00 cc 3b 00 00 e0 2d 00 00 da 2d 00 00 3c 29 00 00 07 33 00 00 b9 33 00 00 75 33 00 00 cb 3b 00 00 cd 3b 00 00 66 38 00 00",
        "diplomatic actions without acceptance",
    ));
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_act_has_one_type_and_name() {
        for (i, a) in ACTS.iter().enumerate() {
            assert_eq!(Act::of(a.1, a.2), Some(a.0), "{}", a.3);
            assert_eq!(Act::named(a.3), Some(a.0));
            assert!(ACTS[..i].iter().all(|b| b.3 != a.3 && b.0 != a.0), "{} twice", a.3);
            // One type, one key, one place in the table.
            assert!(ACTS.iter().filter(|b| b.1 == a.1).all(|b| b.4 == a.4 && b.5 == a.5), "{}", a.3);
            // Its entry in the token table is kept: the token, then the key.
            let bytes: Vec<String> = a.1.to_le_bytes().iter().chain(a.5.as_bytes()).chain(&[0]).map(|b| format!("{b:02x}")).collect();
            assert!(TYPES.contains(&(a.4, bytes.join(" ").as_str())), "{}", a.3);
        }
        assert_eq!(Act::of(0x293d, false), Some(Act::CancelAlliance));
        assert_eq!(Act::of(SCRIPTED, true), None);
    }
}
