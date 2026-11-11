//! The scripts' flags, variables and events: the names and scopes a call
//! may give them, and the values a variable may hold.
use crate::{Checked, Refusal, Status};

/// The longest name of a flag, a variable or an event, in bytes.
pub const LONGEST_NAME: usize = 100;
/// The largest value a variable is set to either side of zero: the game
/// keeps it in thousandths in 32 bits.
pub const LARGEST_VALUE: f64 = 2_000_000.0;

/// What a scope is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// A flag: every kind of scope has them.
    Flag,
    /// A variable: countries and provinces.
    Variable,
    /// An event: countries and provinces.
    Event,
}

impl Use {
    fn kinds(self) -> &'static [&'static str] {
        match self {
            Self::Flag => &["country", "province", "ruler", "heir", "consort", "global"],
            Self::Variable | Self::Event => &["country", "province"],
        }
    }

    fn what(self) -> &'static str {
        match self {
            Self::Flag => "a flag",
            Self::Variable => "a variable",
            Self::Event => "an event",
        }
    }
}

/// A scope of `kind` for `use_`; `global` is whether a global scope says
/// `true`.
pub fn scope(use_: Use, kind: &str, global: bool) -> Checked {
    if !use_.kinds().contains(&kind) {
        return Err(Refusal::new(Status::InvalidArgument, format!("{} is not kept by a {kind}: its scope is one of {}", use_.what(), use_.kinds().join(", "))));
    }
    if kind == "global" && !global {
        return Err(Refusal::new(Status::InvalidArgument, "a global scope is {global = true}"));
    }
    Ok(())
}

/// The name of a flag, a variable or an event (`what`), as the scripts can
/// write it: one word of printable ASCII, without the script's own `=`,
/// braces, quotes and `#`.
pub fn name(what: &str, name: &str) -> Checked {
    let fits = |b: u8| b.is_ascii_graphic() && !matches!(b, b'=' | b'{' | b'}' | b'"' | b'#');
    if name.is_empty() || name.len() > LONGEST_NAME || !name.bytes().all(fits) {
        return Err(Refusal::new(
            Status::InvalidArgument,
            format!("{name:?} is no {what} name: one word of 1 to {LONGEST_NAME} ASCII letters, digits and marks, without =, braces, quotes or #"),
        ));
    }
    Ok(())
}

/// A variable's new value: finite, and at most [`LARGEST_VALUE`] either side
/// of zero.
pub fn value(value: f64) -> Checked {
    if !value.is_finite() || value.abs() > LARGEST_VALUE {
        return Err(Refusal::new(Status::InvalidArgument, format!("a variable's value is from -{LARGEST_VALUE} to {LARGEST_VALUE}, not {value}")));
    }
    Ok(())
}

/// The refusal of an event id no event has.
pub fn no_event(id: &str) -> Refusal {
    Refusal::new(Status::NotFound, format!("no event {id}"))
}

/// The refusal of an event whose trigger does not hold for its scope.
pub fn trigger_fails(id: &str) -> Refusal {
    Refusal::new(Status::Rejected, format!("the trigger of event {id} does not hold"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_one_word_of_the_script() {
        assert!(name("flag", "my_mod.flag_1").is_ok());
        assert!(name("flag", "").is_err());
        assert!(name("flag", "two words").is_err());
        assert!(name("flag", "a=b").is_err());
        assert!(name("flag", "拉丁").is_err());
        assert!(name("flag", &"x".repeat(LONGEST_NAME + 1)).is_err());
    }

    #[test]
    fn each_use_takes_its_scopes() {
        assert!(scope(Use::Flag, "consort", false).is_ok());
        assert!(scope(Use::Flag, "global", true).is_ok());
        assert!(scope(Use::Flag, "global", false).is_err());
        assert!(scope(Use::Variable, "ruler", false).is_err());
        assert!(scope(Use::Event, "global", true).is_err());
        assert!(scope(Use::Event, "province", false).is_ok());
    }

    #[test]
    fn a_value_fits_in_thousandths() {
        assert!(value(2.5).is_ok());
        assert!(value(-LARGEST_VALUE).is_ok());
        assert!(value(LARGEST_VALUE + 1.0).is_err());
        assert!(value(f64::NAN).is_err());
    }
}
