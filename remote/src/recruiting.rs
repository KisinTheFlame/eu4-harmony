//! Recruiting: a regiment or a ship in a province, ordered as the player
//! orders it, through the game's own recruit command.
use crate::{provinces::UnitKind, ProvinceId};

group! {
    /// Recruiting regiments and ships.
    "recruit": RecruitRequest, RecruitAnswer;

    /// Recruits one regiment or ship of a kind in a province, as the player
    /// does with the recruit button in the province view: the player's unit
    /// type of that kind, at the game's cost. Refused with the game's
    /// reasons, one a line, in the language of the game's settings, when the
    /// game would not let the player recruit it there now. `province.get`
    /// says so beforehand, but for one rule of the recruit command's on
    /// ships, a restriction it asks of the player where the snapshot asks it
    /// of the province's owner: a ship the snapshot allows can still be
    /// refused, with no reason. The game carries the order out at its next
    /// frame, paused or not, and the regiment or ship then waits in the
    /// province's recruit queue. The game checks the order again then, and
    /// drops one that no longer passes with only a line in its own log:
    /// orders run at one safe point are each checked against the same
    /// treasury, manpower and sailors, and those the first ones leave too
    /// little for are dropped at that frame.
    game Recruit "" {
        /// The province.
        province: ProvinceId,
        /// The kind: infantry, cavalry, artillery, heavy_ship, light_ship,
        /// galley or transport.
        kind: UnitKind,
    } -> (), "null once the game has the order";
}
