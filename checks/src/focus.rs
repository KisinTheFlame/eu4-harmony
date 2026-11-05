//! The national focus.
use crate::{Refusal, Status};

/// The refusal of a focus on the kind of power (`adm`, `dip`, `mil`) it is
/// on already.
pub fn already(focus: &str) -> Refusal {
    Refusal::new(Status::Rejected, format!("the national focus is on {focus} already"))
}

/// The refusal of cancelling the focus when none is set.
pub fn none_set() -> Refusal {
    Refusal::new(Status::Rejected, "no national focus is set")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_focus_set_already_or_none_set_is_refused() {
        assert_eq!(super::already("adm").message, "the national focus is on adm already");
        assert_eq!(super::none_set().message, "no national focus is set");
    }
}
