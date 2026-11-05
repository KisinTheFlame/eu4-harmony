//! What the groups over the game's script content share: reading a content
//! key and its name, the game's arrays its databases keep, and posting a
//! panel's command with the game's reasons (docs/player-actions.md).
use super::{
    commands::{self, Refused},
    fail, Fail, GResult, Status,
};
use crate::{game_string, memory::Memory, unicode::game_text};

/// The game's array type: its data at `+8`, its count (i32) at `+0x14`.
pub const ARRAY_DATA: usize = 0x8;
pub const ARRAY_COUNT: usize = 0x14;
/// The most entries of a database or a list read.
const MOST: usize = 100_000;

/// The text of the `std::string` at `string`, a content key.
pub fn key(m: &Memory, string: usize) -> GResult<String> {
    Ok(game_text::to_utf8(&game_string::read(m, string)?))
}

/// The game's text for the localisation key `key`, in the player language,
/// without color codes.
pub fn name(m: &Memory, key: &str) -> GResult<String> {
    let key = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
    Ok(game_text::plain(&game_string::localized(m, key.as_ptr() as usize)?))
}

/// The pointers the game's array at `array` holds.
pub fn pointers(m: &Memory, array: usize) -> GResult<Vec<usize>> {
    let (data, count) = (m.ptr(array + ARRAY_DATA)?, m.i32(array + ARRAY_COUNT)?);
    (0..bounded(count)?).map(|i| Ok(m.ptr(data + i * 8)?)).collect()
}

/// The pointers of the `std::vector` at `vector`.
pub fn vector(m: &Memory, vector: usize) -> GResult<Vec<usize>> {
    let (begin, end) = (m.ptr(vector)?, m.ptr(vector + 8)?);
    (0..super::span(begin, end, 8, MOST)?).map(|i| Ok(m.ptr(begin + i * 8)?)).collect()
}

/// A count the game keeps, checked.
pub fn bounded(count: i32) -> GResult<usize> {
    match usize::try_from(count) {
        Ok(n) if n <= MOST => Ok(n),
        _ => Err(format!("a count of {count}").into()),
    }
}

/// Has the game write its reasons for not allowing something into a text of
/// the framework's: `check` is given the address of an empty `std::string`
/// and answers whether it is allowed. The reasons are the text's lines, the
/// conditions that hold left out ([`super::conditions::refusal`]), without
/// the bullet some checks start a line with.
pub fn checked(m: &Memory, check: impl FnOnce(usize) -> GResult<usize>) -> GResult<(bool, Vec<String>)> {
    let (allowed, text) = game_string::written(m, check)?;
    let reasons = super::conditions::refusal(&text).into_iter().map(|l| l.trim_start_matches(['-', '－', '•']).trim_start().to_string());
    Ok((allowed & 0xff != 0, reasons.filter(|l| !l.is_empty()).collect()))
}

/// Posts a panel's command, refusing as the game's own check does: with
/// `reasons` when the check found some, otherwise saying that the game
/// refuses `what` and gives no reason.
pub fn post(text: &str, what: &str) -> GResult<()> {
    match commands::post(text) {
        Ok(()) => Ok(()),
        Err(Refused::Check) => fail(Status::Rejected, format!("the game's command refuses {what}, and gives no reason")),
        Err(refused) => Err(Fail::from(refused)),
    }
}

/// The refusal of an action the game does not allow, with its `reasons`.
pub fn refused<T>(reasons: &[String], what: &str) -> GResult<T> {
    Err(eu4_harmony_checks::government::not_allowed(reasons, what).into())
}

/// A country tag's letters, as commands write it.
pub fn letters(tag: super::Tag) -> String {
    String::from_utf8_lossy(&tag.letters()).into_owned()
}
