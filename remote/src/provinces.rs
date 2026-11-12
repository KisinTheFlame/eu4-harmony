//! Provinces: every province, a province's neighbors, one province's
//! snapshot, and what the player does in the province view: developing it,
//! its autonomy, its state, its edict, coring it and changing its culture.
use crate::{
    choices::Choice,
    countries::{ModifierValue, NamedModifier},
    CountryTag, Date, ProvinceId, ProvinceList, Words,
};
use serde::{Deserialize, Serialize};

group! {
    /// Provinces and their neighbors.
    "province": ProvinceRequest, ProvinceAnswer;

    /// Every province, or those named. In the player's view a province the
    /// player has not discovered has no owner or controller, as the map
    /// shows terra incognita.
    world List "list" {
        /// Only these provinces, in their order; a number that names none is
        /// left out. Every province when it is left out.
        provinces: Option<ProvinceList>,
    } -> Vec<Province>, "the provinces, by number: id, kind (land, sea, lake or wasteland), visible (to the player, not under fog of war), owner and controller (null when none)";
    /// The provinces an army can go to from a province over land, possibly
    /// across a strait; none for a province an army cannot stand on.
    world Neighbors "neighbors" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Neighbor>, "its neighbors: province, and strait, whether it is across a strait";
    /// One province's snapshot: its name, owner and controller, development,
    /// terrain, fort and buildings, local autonomy, culture, religion,
    /// unrest, trade good, trade node, center of trade, trade company, state
    /// or territory and the state's edict, cores and claims, what is being
    /// built or done there, the colony's size, and the named modifiers its
    /// scripts put on it. In the player's view a province another country
    /// owns leaves out its unrest and what is being done there, as the
    /// province view does, a named modifier its scripts hide is left out,
    /// and one the player has not discovered is refused (not_found). What
    /// the player can recruit there is recruit.options'.
    world Get "get" {
        /// The province.
        province: ProvinceId,
    } -> Box<ProvinceSnapshot>, "its snapshot: id, name, owner, controller, development, terrain, fort, buildings, autonomy (percent), culture, religion and trade_good (their keys), unrest, trade_node (its key), center_of_trade (its level, 0 for none), trade_company, state (state or territory, null when it has no owner), edict (the state edict's key), cores, claims, constructions (each its kind, the game's key such as building_construction, the country, progress from 0 to 1, start and end dates, and the building or unit type it makes), missionary (a missionary's progress in percent, null when none works there), colony_size (null for no colony), prosperity (its state's, in percent; null in a territory), devastation (percent), local_manpower (thousands of men), supply_limit (regiments, for the country acted as), separatism (years left), institutions (each appeared institution's key and the progress toward it in percent) and named_modifiers (each its key and the day it ends, null for one that does not); in the player's view unrest, constructions and missionary are null for a province another country owns";
    /// The province's area as held by the country acted as: its own provinces,
    /// development, trade company membership, state status and governing cost.
    /// Costs are integer thousandths. For live country capacity, call
    /// country.governing_capacity separately before and after a command.
    /// Refused for another country's province. A state projection is absent
    /// when its costs cannot be determined without changing the world.
    game Area "area" {
        /// An owned province of the area.
        province: ProvinceId,
    } -> AreaSnapshot, "area (content key), owner, provinces (owned province numbers), development (their total), trade_company (any owned member belongs to one), state, governing_cost, make_state_cost (additional cost of statehood without coring, null when unavailable), unavailable_reason; all costs in integer thousandths";
    /// What a province has of each modifier asked for, by its key, as the
    /// game totals the province's own: its buildings, terrain, trade good,
    /// named modifiers and the rest, not what its owner's modifiers add
    /// across the country. Refused (not_found) for a key that names no
    /// modifier, and in the player's view for a province the player has not
    /// discovered.
    world Modifiers "modifiers" {
        /// The province.
        province: ProvinceId,
        /// The modifiers' keys, such as `local_defensiveness` or
        /// `local_tax_modifier`.
        keys: Words,
    } -> Vec<ModifierValue>, "one for each key asked, in their order: modifier (its key) and value, as the game's scripts write it (0.05 for 5%)";

    /// What the province view's buttons offer the player in a province now:
    /// each action this group has, with whether the player can, the game's
    /// reasons why not, as the button's tooltip gives them, and its cost.
    /// Setting a state edict is one entry for each edict. Its buildings are
    /// building.options'.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> Vec<ProvinceOption>, "one for each action: action (the capability's own name, such as develop_tax), edict (the edict's content key, for set_edict), can, reasons, cost";
    /// Raises the province's base tax by one, for administrative points, as
    /// the province view's develop button does. Refused with the game's
    /// reasons, one a line, as province.options gives them.
    game DevelopTax "develop_tax" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Raises the province's base production by one, for diplomatic points.
    game DevelopProduction "develop_production" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Raises the province's base manpower by one, for military points.
    game DevelopManpower "develop_manpower" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Exploits the province's base tax for ducats, as the province view's
    /// exploit button does once the player confirms: the province loses
    /// development and the button waits for years.
    game ExploitTax "exploit_tax" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Exploits the province's base production for sailors.
    game ExploitProduction "exploit_production" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Exploits the province's base manpower for manpower.
    game ExploitManpower "exploit_manpower" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Raises the province's local autonomy for a while, as the province
    /// view's button does.
    game RaiseAutonomy "raise_autonomy" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Lowers the province's local autonomy for a while, as the province
    /// view's button does.
    game LowerAutonomy "lower_autonomy" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Makes the province's area a state of the player's, as the province
    /// view's button does.
    game MakeState "make_state" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Makes the player's state in the province's area a territory again, as
    /// the province view's button does.
    game MakeTerritory "make_territory" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Sets an edict in the player's state the province is in, replacing the
    /// one set there, as the state's edict picker does.
    game SetEdict "set_edict" {
        /// A province of the state.
        province: ProvinceId,
        /// The edict's content key, such as `edict_advancement_effort`.
        edict: String,
    } -> (), "null once done";
    /// Cancels the edict set in the player's state the province is in.
    game CancelEdict "cancel_edict" {
        /// A province of the state.
        province: ProvinceId,
    } -> (), "null once done";
    /// Starts making the province a core of the player's, for administrative
    /// points, as the province view's core button does.
    game Core "core" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Stops the player making the province a core, as the province view's
    /// cancel button does; the points spent are not given back.
    game CancelCore "cancel_core" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Starts changing the province's culture to the player's primary
    /// culture, for diplomatic points, as the province view's button does
    /// once the player confirms.
    game ChangeCulture "change_culture" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Stops changing the province's culture, as the province view's cancel
    /// button does; the points spent are not given back.
    game CancelCulture "cancel_culture" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Moves the player's capital to the province, for administrative
    /// points, as the province view's button does once the player confirms.
    game MoveCapital "move_capital" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Moves the player's trade port to the coastal province, and with it
    /// the home trade node, for diplomatic points, as the province view's
    /// button does once the player confirms.
    game MoveTradePort "move_trade_port" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Starts a Siberian frontier in an uncolonized province, for
    /// diplomatic points, as the province view's button does once the
    /// player confirms: it grows into the player's province by itself, with
    /// no colonist.
    game EstablishSiberianFrontier "establish_siberian_frontier" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Makes an uncolonized province tribal land of the player's native
    /// country, for administrative points, as the province view's button
    /// does.
    game AddTribalLand "add_tribal_land" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Razes a province of the player's horde, as the province view's
    /// button does once the player confirms: the province loses
    /// development, and the horde gains reform progress.
    game Raze "raze" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
}

/// One thing the province view offers the player in a province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ProvinceOption {
    /// The capability of this group that does it, such as `develop_tax`.
    pub action: String,
    /// For `set_edict`, the edict's content key; null for the rest.
    pub edict: Option<String>,
    #[serde(flatten)]
    pub choice: Choice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Province {
    pub id: ProvinceId,
    pub kind: ProvinceKind,
    /// Not under fog of war; false while visibility is not current.
    pub visible: bool,
    pub owner: Option<CountryTag>,
    pub controller: Option<CountryTag>,
}

/// What a province is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum ProvinceKind {
    /// Land an army can stand on.
    Land,
    Sea,
    Lake,
    /// Land no army can enter.
    Wasteland,
}

/// A province next to another over land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Neighbor {
    pub province: ProvinceId,
    /// Across a strait.
    pub strait: bool,
}

/// All of one province at one moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ProvinceSnapshot {
    pub id: ProvinceId,
    /// Its name as the game shows it, in its own characters (the game keeps
    /// it as game text).
    pub name: String,
    pub owner: Option<CountryTag>,
    pub controller: Option<CountryTag>,
    pub development: Development,
    /// The key of its terrain, such as `farmlands`; null when it has none.
    pub terrain: Option<String>,
    pub fort: Fort,
    /// The keys of its buildings, such as `fort_15th` and `marketplace`.
    pub buildings: Vec<String>,
    /// Local autonomy as the province view shows it, in percent.
    pub autonomy: f64,
    /// The keys of its culture, religion and trade good, such as
    /// `castillian`, `catholic` and `wine`; null when it has none.
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub trade_good: Option<String>,
    /// Unrest as the province view shows it; null in the player's view for
    /// another country's province.
    pub unrest: Option<f64>,
    /// The key of its trade node, such as `sevilla`.
    pub trade_node: Option<String>,
    /// The level of its center of trade, 0 for none.
    pub center_of_trade: i32,
    /// It is in a trade company.
    pub trade_company: bool,
    /// Whether it is in a state of its owner's or a territory; null without
    /// an owner.
    pub state: Option<StateKind>,
    /// The key of the edict its state has, such as `edict_advancement_effort`.
    pub edict: Option<String>,
    /// The countries with a core on it.
    pub cores: Vec<CountryTag>,
    /// The countries with a claim on it, permanent or not.
    pub claims: Vec<CountryTag>,
    /// What is being built or done there; null in the player's view for
    /// another country's province, as the missionary.
    pub constructions: Option<Vec<Construction>>,
    /// The missionary's progress converting it, in percent; null when none
    /// works there.
    pub missionary: Option<f64>,
    /// The colony's size in settlers; null when it is no colony.
    pub colony_size: Option<f64>,
    /// The named modifiers its scripts put on it, such as an event's
    /// `province_modifier`; in the player's view one its scripts hide is
    /// left out.
    pub named_modifiers: Vec<NamedModifier>,
    /// The prosperity of the state it is in, in percent; null in a
    /// territory and without an owner.
    pub prosperity: Option<f64>,
    /// Its devastation, in percent.
    pub devastation: f64,
    /// The manpower it gives its owner, as the province view shows it, in
    /// thousands of men.
    pub local_manpower: f64,
    /// How many regiments it supplies, as the map's tooltip shows it to the
    /// country acted as (to its owner while the player observes with no
    /// country in view): supply depends on whose the army is.
    pub supply_limit: f64,
    /// The years of separatism left, 0 for none.
    pub separatism: i32,
    /// Its progress toward each institution that has appeared in the world,
    /// in the game's order.
    pub institutions: Vec<InstitutionProgress>,
}

/// The country's share of an area, with exact governing costs in thousandths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct AreaSnapshot {
    pub area: String,
    pub owner: CountryTag,
    pub provinces: Vec<ProvinceId>,
    pub development: f64,
    pub trade_company: bool,
    pub state: StateKind,
    pub governing_cost: i32,
    pub make_state_cost: Option<i32>,
    pub unavailable_reason: Option<String>,
}

/// A province's progress toward an institution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct InstitutionProgress {
    /// The institution's content key, such as `renaissance`.
    pub institution: String,
    /// In percent: 100 once the province has it.
    pub progress: f64,
}

/// Whether a province is in a state or a territory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    State,
    Territory,
}

/// Something being built or done in a province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Construction {
    /// The game's key for what it is: `building_construction`,
    /// `military_construction` (regiments or ships), `missionary_construction`,
    /// `colony_construction`, `build_core_construction`,
    /// `change_culture_construction` and the others the game has.
    pub kind: String,
    /// Whose it is.
    pub country: Option<CountryTag>,
    /// From 0 to 1.
    pub progress: f64,
    pub start: Date,
    pub end: Date,
    /// For a building, its key; for regiments or ships, their unit type's.
    pub makes: Option<String>,
}

/// Base development, as the game shows it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Development {
    pub tax: f64,
    pub production: f64,
    pub manpower: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Fort {
    /// The fort level that defends the province now; 0 for none.
    pub level: i32,
    /// Its fort is mothballed: then only a capital's own level is left.
    pub mothballed: bool,
}

