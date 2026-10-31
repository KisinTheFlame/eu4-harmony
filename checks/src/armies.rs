//! Armies: which an extension or remote control may route and order, and
//! what is not found.
use crate::{Checked, Refusal, Status};

/// What of an own land army's state stops an order of the framework's, as
/// its snapshot has it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Condition {
    pub in_battle: bool,
    pub sieging: bool,
    pub retreating: bool,
    pub exiled: bool,
    pub movement_locked: bool,
    /// Attached to another army.
    pub attached: bool,
    /// Other armies are attached to this one.
    pub has_followers: bool,
    /// Running a native task (for example hunting rebels) or otherwise not
    /// idle.
    pub busy: bool,
    pub regiments: u32,
}

/// Refuses an army the game would not let the framework order reliably: in
/// battle, sieging, retreating, exiled, its movement locked, attached or
/// with armies attached, busy with a task of the game's, or with no
/// regiments; asked in that order.
pub fn orderable(army: &Condition) -> Checked {
    for (on, reason) in [
        (army.in_battle, "army is in battle"),
        (army.sieging, "army is sieging"),
        (army.retreating, "army is retreating"),
        (army.exiled, "army is exiled"),
        (army.movement_locked, "army movement is locked"),
        (army.attached, "army is attached to another army"),
        (army.has_followers, "other armies are attached to this army"),
        (army.busy, "army has a native task"),
        (army.regiments == 0, "army has no regiments"),
    ] {
        if on {
            return Err(Refusal::new(Status::Rejected, reason));
        }
    }
    Ok(())
}

/// The refusal of an extension's order while the game is paused: its orders
/// wait for the game to run.
pub fn paused() -> Refusal {
    Refusal::new(Status::NotReady, "game is paused")
}

/// The refusal of a fleet where a capability takes a land army.
pub fn not_land() -> Refusal {
    Refusal::new(Status::InvalidArgument, "not a land army")
}

/// The refusal of an ID that is none of the player's armies and fleets.
pub fn not_own(id: u64) -> Refusal {
    Refusal::new(Status::NotFound, format!("no own army {id}"))
}

/// The refusal of what reads the armies the player sees while visibility
/// does not reflect the date yet.
pub fn visibility_not_current() -> Refusal {
    Refusal::new(Status::NotReady, "visibility is not current yet")
}

/// The refusal of a target that is not among the armies the player sees in
/// `province`.
pub fn not_visible(target: u64, province: u32) -> Refusal {
    Refusal::new(Status::NotFound, format!("no visible army {target} in {province}"))
}

/// The refusal of a goal the game finds no route to.
pub fn no_route(goal: u32) -> Refusal {
    Refusal::new(Status::NotFound, format!("no route to {goal}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_army_is_orderable_with_regiments_and_nothing_that_stops_an_order() {
        let ready = Condition { regiments: 1, ..Default::default() };
        assert_eq!(orderable(&ready), Ok(()));
        let refused = |army: Condition| orderable(&army).map_err(|r| (r.status, r.message));
        assert_eq!(refused(Condition { regiments: 0, ..ready }), Err((Status::Rejected, "army has no regiments".into())));
        // The first that applies is the one given.
        assert_eq!(refused(Condition { sieging: true, busy: true, regiments: 0, ..ready }), Err((Status::Rejected, "army is sieging".into())));
        assert_eq!(refused(Condition { has_followers: true, ..ready }), Err((Status::Rejected, "other armies are attached to this army".into())));
    }
}
