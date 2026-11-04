//! Provinces: every province, a province's neighbors, one province's
//! snapshot, and what the player does in the province view: developing it,
//! its autonomy, its state, its edict, coring it and changing its culture.
use crate::{choices::Choice, CountryTag, ProvinceId, ProvinceList};
use serde::{Deserialize, Serialize};

group! {
    /// Provinces and their neighbors.
    "province": ProvinceRequest, ProvinceAnswer;

    /// Every province, or those named.
    game List "list" {
        /// Only these provinces, in their order; a number that names none is
        /// left out. Every province when it is left out.
        provinces: Option<ProvinceList>,
    } -> Vec<Province>, "the provinces, by number: id, kind (land, sea, lake or wasteland), visible (to the player, not under fog of war), owner and controller (null when none)";
    /// The provinces an army can go to from a province over land, possibly
    /// across a strait; none for a province an army cannot stand on.
    game Neighbors "neighbors" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Neighbor>, "its neighbors: province, and strait, whether it is across a strait";
    /// One province's snapshot: its name, owner and controller, development,
    /// terrain, fort and buildings. What the player can recruit there is
    /// recruit.options'.
    game Get "get" {
        /// The province.
        province: ProvinceId,
    } -> ProvinceSnapshot, "its snapshot: id, name, owner, controller, development, terrain, fort, buildings";

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

