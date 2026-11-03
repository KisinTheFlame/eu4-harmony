//! Recruiting: a regiment or a ship in a province, ordered as the player
//! orders it, through the game's own recruit command.
use crate::ProvinceId;
use serde::{Deserialize, Serialize};

group! {
    /// Recruiting regiments and ships.
    "recruit": RecruitRequest, RecruitAnswer;

    /// What the player can recruit in a province now: for each kind of
    /// regiment and ship, the player's unit type of that kind, whether the
    /// game would let the player recruit it there, and why not.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Recruit>, "one for each kind, in the order infantry, cavalry, artillery, heavy_ship, light_ship, galley, transport: kind, unit_type, allowed, reasons";

    /// Recruits one regiment or ship of a kind in a province, as the player
    /// does with the recruit button in the province view: the player's unit
    /// type of that kind, at the game's cost. Refused with the game's
    /// reasons, one a line, in the language of the game's settings, when the
    /// game would not let the player recruit it there now. `recruit.options`
    /// says so beforehand, but for one rule of the recruit command's on
    /// ships, a restriction it asks of the player where recruit.options asks it
    /// of the province's owner: a ship recruit.options allows can still be
    /// refused, with no reason. The game carries the order out at its next
    /// frame, paused or not, and the regiment or ship then waits in the
    /// province's recruit queue. The game checks the order again then, and
    /// drops one that no longer passes with only a line in its own log:
    /// orders run at one safe point are each checked against the same
    /// treasury, manpower and sailors, and those the first ones leave too
    /// little for are dropped at that frame.
    game Order "order" {
        /// The province.
        province: ProvinceId,
        /// The kind: infantry, cavalry, artillery, heavy_ship, light_ship,
        /// galley or transport.
        kind: UnitKind,
    } -> (), "null once the game has the order";
}

/// A kind of regiment or ship, as the game sorts unit types: what a recruit
/// order names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Infantry,
    Cavalry,
    Artillery,
    HeavyShip,
    LightShip,
    Galley,
    Transport,
}

/// Whether the player can recruit one regiment or ship of a kind in a
/// province now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Recruit {
    pub kind: UnitKind,
    /// The key of the unit type the player recruits of this kind, such as
    /// `western_medieval_infantry`; null when the player has none.
    pub unit_type: Option<String>,
    pub allowed: bool,
    /// Why not, in the game's own words, from its localisation for the
    /// language in the game's settings, which can differ from the player
    /// language; empty when it is allowed or the game gives no reason.
    pub reasons: Vec<String>,
}
