//! What the groups over the game's script content share: reading a content
//! key and its name, the game's arrays its databases keep, and a check's
//! reasons (docs/plans/player-actions.md).
use super::GResult;
use crate::{game_string, memory::Memory, unicode::game_text};

/// The text of the `std::string` at `string`, a content key.
pub fn key(m: &Memory, string: usize) -> GResult<String> {
    Ok(game_string::key(m, string)?)
}

/// The game's text for the localisation key `key`, in the player language,
/// without color codes. A `$NAME$` in it that names another key, as
/// `grant_consessions_to_the_church` names `ESTATE_CHURCH`, is that key's
/// text, as the game's views show it; the lookup alone leaves it out.
pub fn name(m: &Memory, key: &str) -> GResult<String> {
    name_given(m, key, &[])
}

/// As [`name`], with the `$NAME$`s the view fills in from what the text is
/// about, as the estates view fills a privilege's `$ESTATE_NAME$` with its
/// estate's name: each pair a name and the localisation key whose text it
/// takes.
pub fn name_given(m: &Memory, key: &str, given: &[(&str, &str)]) -> GResult<String> {
    let names = references(&game_string::unformatted(m, key)?);
    let key = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
    if names.is_empty() {
        return Ok(game_text::plain(&game_string::localized(m, key.as_ptr() as usize)?));
    }
    let sources = names.iter().map(|name| source(name, given)).collect::<Result<Vec<_>, _>>()?;
    let texts = sources.iter().map(|source| game_string::localized(m, source.as_ptr() as usize)).collect::<Result<Vec<_>, _>>()?;
    let values: Vec<(&std::ffi::CStr, &[u8])> = names.iter().map(|n| n.as_c_str()).zip(texts.iter().map(Vec::as_slice)).collect();
    Ok(game_text::plain(&game_string::localized_with(m, key.as_ptr() as usize, &values)?))
}

/// The localisation key whose text a `$NAME$` takes: the one `given` pairs
/// with it, or the name itself.
fn source(name: &std::ffi::CStr, given: &[(&str, &str)]) -> GResult<std::ffi::CString> {
    let key = given.iter().find(|(n, _)| n.as_bytes() == name.to_bytes()).map_or(name.to_bytes(), |(_, key)| key.as_bytes());
    Ok(std::ffi::CString::new(key).map_err(|e| e.to_string())?)
}

/// The names a localisation text refers to as `$NAME$`, each once.
fn references(text: &[u8]) -> Vec<std::ffi::CString> {
    let mut names: Vec<std::ffi::CString> = Vec::new();
    for (i, part) in text.split(|&b| b == b'$').enumerate() {
        let is_name = !part.is_empty() && part.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_');
        if i % 2 == 1 && is_name && names.iter().all(|n| n.as_bytes() != part) {
            names.push(std::ffi::CString::new(part).expect("no NUL in a name"));
        }
    }
    names
}

/// Has the game write its reasons for not allowing something into a text of
/// the framework's: `check` is given the address of an empty `std::string`
/// and answers whether it is allowed. The reasons are the text's lines, the
/// conditions that hold left out ([`super::conditions::refusal`]), without
/// the bullet some checks start a line with.
pub fn checked(m: &Memory, check: impl FnOnce(usize) -> GResult<usize>) -> GResult<(bool, Vec<String>)> {
    let (allowed, text) = game_string::written(m, check)?;
    let reasons = super::conditions::refusal(&text).into_iter().map(|l| super::unbulleted(&l).to_string());
    Ok((allowed & 0xff != 0, reasons.filter(|l| !l.is_empty()).collect()))
}

/// A country tag's letters, as commands write it.
pub fn letters(tag: super::Tag) -> String {
    String::from_utf8_lossy(&tag.letters()).into_owned()
}

/// The refusals the framework words itself in the groups over script
/// content: ideas, policies, reforms, the rank, estates, parliament,
/// decisions and missions; those the game gives reasons for are
/// [`super::action`]'s.
pub mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of a content key that names nothing of `kind` (such as "idea
    /// group") the player has or is offered.
    pub fn not_found(kind: &str, key: &str) -> Fail {
        Fail::new(Status::NotFound, format!("no {kind} {key:?} for the player"))
    }

    /// The refusal of an action on something of the player's in a state it is
    /// not in: `why`, such as "the player has not taken the idea group
    /// defensive_ideas".
    pub fn not_now(why: impl Into<String>) -> Fail {
        Fail::new(Status::Rejected, why)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn an_unknown_key_is_not_found() {
            let refusal = not_found("idea group", "flying_ideas");
            assert_eq!((refusal.status, refusal.message.as_str()), (Status::NotFound, "no idea group \"flying_ideas\" for the player"));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_text_refers_to_each_name_between_dollars_once() {
        let names = super::references(b"Grant Concessions to the $ESTATE_CHURCH$ and $ESTATE_CHURCH$, 5$ $");
        assert_eq!(names.iter().map(|n| n.to_str().unwrap()).collect::<Vec<_>>(), ["ESTATE_CHURCH"]);
        assert!(super::references(b"no names").is_empty());
    }

    #[test]
    fn a_name_the_view_fills_in_takes_the_text_of_the_key_given() {
        let given = [("ESTATE_NAME", "estate_church")];
        assert_eq!(super::source(c"ESTATE_NAME", &given).unwrap().as_c_str(), c"estate_church");
        assert_eq!(super::source(c"ESTATE_CHURCH", &given).unwrap().as_c_str(), c"ESTATE_CHURCH");
    }
}
