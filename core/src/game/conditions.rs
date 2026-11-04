//! Script conditions: whether one holds for a country, and its lines that do
//! not, as the game's tooltips write them (docs/research/triggers.md). What
//! the player can take or choose in a panel, such as a decision, a reform, a
//! policy or a privilege, is allowed by conditions in script; the panel's
//! tooltip lists them, and a capability refusing one gives those lines as
//! the game's reasons (docs/player-actions.md).
use super::{GResult, Tag};
use crate::{game_string, memory::Memory, points::Point, unicode::game_text};

/// Makes a scope: `(scope, country tag by value, province, 0)`.
const SCOPE: usize = 0x94590;
/// Frees what a scope owns, without freeing the scope.
const SCOPE_DESTROY: usize = 0x947f0;
/// A content trigger's verdict: `(trigger, scope) -> bool`, true when every
/// condition holds.
const EVALUATE: usize = 0xb98200;
/// A content trigger's lines: `(trigger, out, scope, negated, indent, mode)`.
const LINES: usize = 0xb97cd0;
/// The scope's size.
const SCOPE_SIZE: usize = 0xd0;
/// [`LINES`]'s mode for the conditions that do not hold only.
const FAILING: usize = 2;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(SCOPE, "48 89 51 2c 48 8d 05 bd 3b ba 01 48 89 01 33 d2 c7 41 08 65 01 00 00", "scope maker"),
        Point::keep(SCOPE_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 48 8b 99 c0 00 00 00 48 8b f9 48 85 db", "scope destructor"),
        Point::keep(EVALUATE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b 59 08 48 8b f2 48 63 41 14", "condition verdict"),
        Point::keep(LINES, "48 89 5c 24 18 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57", "condition lines"),
    ]
}

/// A scope of the game's for `country`, and `province` when not 0, freed when
/// it is dropped. A new one for each question: evaluating writes into it.
struct Scope<'m> {
    m: &'m Memory,
    bytes: [u64; SCOPE_SIZE / 8],
}

impl<'m> Scope<'m> {
    fn new(m: &'m Memory, country: Tag, province: u32) -> GResult<Box<Self>> {
        let mut scope = Box::new(Self { m, bytes: [0; SCOPE_SIZE / 8] });
        m.call(m.at(SCOPE), &[scope.address(), country.0 as usize, province as usize, 0])?;
        Ok(scope)
    }

    fn address(&mut self) -> usize {
        self.bytes.as_mut_ptr() as usize
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let at = self.address();
        if let Err(e) = self.m.call(self.m.at(SCOPE_DESTROY), &[at]) {
            crate::log!("a condition's scope was not freed: {e}");
        }
    }
}

/// Whether the content trigger at `trigger` holds for `country` (and
/// `province`, when not 0).
pub fn holds(m: &Memory, trigger: usize, country: Tag, province: u32) -> GResult<bool> {
    let mut scope = Scope::new(m, country, province)?;
    Ok(m.call(m.at(EVALUATE), &[trigger, scope.address()])? & 0xff != 0)
}

/// The lines of the content trigger at `trigger` that do not hold for
/// `country` (and `province`, when not 0), as plain Unicode, as the game's
/// tooltip writes them; empty when every one holds.
pub fn failing(m: &Memory, trigger: usize, country: Tag, province: u32) -> GResult<Vec<String>> {
    let mut scope = Scope::new(m, country, province)?;
    let at = scope.address();
    let (_, text) = game_string::written(m, |out| m.call(m.at(LINES), &[trigger, out, at, 0, 0, FAILING]))?;
    Ok(lines(&text))
}

/// Game text's condition lines as plain Unicode: the mark of whether each
/// holds (an icon, `£yes ` or `£no `), the indent and color codes left out.
pub fn lines(text: &[u8]) -> Vec<String> {
    let unmarked = strip(&strip(text, b"\xa3yes "), b"\xa3no ");
    game_text::plain(&unmarked).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

fn strip(text: &[u8], mark: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if rest.starts_with(mark) {
            rest = &rest[mark.len()..];
        } else {
            out.push(rest[0]);
            rest = &rest[1..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::lines;
    use crate::unicode::game_text;

    #[test]
    fn a_condition_s_lines_lose_their_marks_and_indent() {
        let text = game_text::from_str("£no Administrative technology is at least 5\n   £no §YFrance§! owns Paris\nAll of the following:\n   Is at peace\n");
        assert_eq!(lines(&text), ["Administrative technology is at least 5", "France owns Paris", "All of the following:", "Is at peace"]);
        assert!(lines(b"").is_empty());
    }
}
