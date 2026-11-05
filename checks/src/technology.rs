//! Technology.
use crate::{Refusal, Status};

/// The refusal of research in a technology at its last level, named by its
/// kind (`adm`, `dip`, `mil`).
pub fn last_level(technology: &str) -> Refusal {
    Refusal::new(Status::Rejected, format!("the {technology} technology is at its last level"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_last_level_names_the_technology() {
        assert_eq!(super::last_level("dip").message, "the dip technology is at its last level");
    }
}
