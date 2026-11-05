//! Forts: the player's forts, and mothballing and reopening them, as the
//! province view's fort button does.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's forts.
    "fort": FortRequest, FortAnswer;

    /// The player's provinces with a fort, each with its level, whether it
    /// is mothballed, and whether the player can mothball or reopen it now,
    /// why not as the button's tooltip says.
    game List "list" {} -> Vec<PlayerFort>, "the forts: province, level, mothballed, can, reasons, cost";
    /// Mothballs a fort of the player's, as the province view's button does:
    /// it no longer defends, and costs no upkeep.
    game Mothball "mothball" {
        /// The province.
        province: ProvinceId,
    } -> (), "nothing";
    /// Reopens a mothballed fort of the player's, as the province view's
    /// button does.
    game Reopen "reopen" {
        /// The province.
        province: ProvinceId,
    } -> (), "nothing";
}

/// A fort of the player's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct PlayerFort {
    pub province: ProvinceId,
    /// Its level, as the fort's tooltip gives it; what it keeps while
    /// mothballed.
    pub level: i32,
    pub mothballed: bool,
    /// Whether the player can mothball it (reopen it, when mothballed) now.
    #[serde(flatten)]
    pub choice: Choice,
}
