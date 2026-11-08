//! What the player's missionaries, colonists and merchants have in common:
//! the country's pools of envoys, each kind's count and how many are free
//! (docs/research/envoys.md).
use super::GResult;
use crate::containers;
use crate::{memory::Memory, points::Point, unicode::game_text};

/// A country's envoys: a pointer to the pools, one per kind at its offset.
pub(super) const POOLS: usize = 0x1480;
/// A pool's envoys, a vector of pointers.
pub(super) const ENVOYS: usize = 0x20;
/// An envoy is away while this byte is set.
pub(super) const AWAY: usize = 0x18;
/// The most envoys of a kind read.
const MOST: usize = 1_000;

/// A kind of envoy, by its pool's offset in the country's pools (the
/// colonists' is at 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Merchant = 0x08,
    Missionary = 0x10,
}

pub fn points() -> Vec<Point> {
    vec![
        // The missionary command's check counts the free missionaries of the
        // sender's pool: the pools, the missionaries', their vector and the
        // byte that says one is away.
        Point::keep(0xaab03eusize, "48 8b 81 80 14 00 00 4c 8b 48 10", "missionary pool").uses(&[POOLS]),
        Point::keep(0xaab06busize, "4d 8b 41 20 49 8b 41 28", "missionary pool's envoys").uses(&[ENVOYS]),
        Point::keep(0xaab087usize, "80 78 18 00", "envoy away").uses(&[AWAY]),
    ]
}

/// The envoys of `kind` a country has, each its address.
pub fn all(m: &Memory, country: usize, kind: Kind) -> GResult<Vec<usize>> {
    let pool = m.ptr(m.ptr(country + POOLS)? + kind as usize)?;
    if pool == 0 {
        return Ok(Vec::new());
    }
    let (begin, end) = (m.ptr(pool + ENVOYS)?, m.ptr(pool + ENVOYS + 8)?);
    (0..containers::span(begin, end, 8, MOST)?).map(|i| Ok(m.ptr(begin + i * 8)?)).collect()
}

/// Whether an envoy is free to send: neither on its way nor at work.
pub fn free(m: &Memory, envoy: usize) -> GResult<bool> {
    Ok(m.u8(envoy + AWAY)? == 0)
}

/// The envoys of `kind` a country has, and how many of them are free.
pub fn count(m: &Memory, country: usize, kind: Kind) -> GResult<(u32, u32)> {
    let envoys = all(m, country, kind)?;
    let mut free_ones = 0;
    for &envoy in &envoys {
        if free(m, envoy)? {
            free_ones += 1;
        }
    }
    Ok((envoys.len() as u32, free_ones))
}

/// The reasons in game text that lists the rules that hold as well: the
/// lines not marked as holding (`£yes`), as plain Unicode, without a heading
/// (a line ending in a colon) under which nothing fails (`£no`).
pub fn failing(text: &[u8]) -> Vec<String> {
    const NO: &[u8] = b"\xa3no";
    const YES: &[u8] = b"\xa3yes";
    let has = |line: &[u8], mark: &[u8]| line.windows(mark.len()).any(|w| w == mark);
    let all: Vec<&[u8]> = text.split(|&b| b == b'\n').collect();
    let mut kept = Vec::new();
    for (i, line) in all.iter().enumerate() {
        let heading = game_text::plain(line).trim_end().ends_with([':', '\u{ff1a}']);
        if has(line, YES) || heading && !all[i + 1..].iter().any(|l| has(l, NO)) {
            continue;
        }
        let mut line = line.to_vec();
        while let Some(at) = line.windows(NO.len()).position(|w| w == NO) {
            line.drain(at..at + NO.len());
        }
        kept.extend_from_slice(&line);
        kept.push(b'\n');
    }
    lines(&kept)
}

/// Game text's reasons as plain Unicode lines, a bullet left out.
pub fn lines(text: &[u8]) -> Vec<String> {
    game_text::plain(text).lines().map(|l| l.trim()).map(|l| l.strip_prefix("- ").unwrap_or(l).trim()).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// A tag as a command's text writes it.
pub fn quoted(tag: super::Tag) -> String {
    format!("\"{}\"", String::from_utf8_lossy(&tag.letters()))
}

/// The game's text for the localisation key `key`, as plain Unicode.
pub fn localized(m: &Memory, key: &str) -> GResult<String> {
    let key = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
    Ok(game_text::plain(&crate::game_string::localized(m, key.as_ptr() as usize)?))
}

/// The refusals the framework words itself in the actions of missionaries,
/// colonists, merchants, trade companies and centers of trade (part A8 of
/// docs/player-actions.md); those the game gives reasons for are
/// [`super::action`]'s.
pub mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of recalling an envoy of a `kind` ("missionary") from where
    /// the player has none.
    pub fn none_there(kind: &str) -> Fail {
        Fail::new(Status::Rejected, format!("the player has no {kind} there"))
    }

    /// The refusal of recalling an envoy of a `kind` ("merchant") that is
    /// still on its way.
    pub fn on_its_way(kind: &str) -> Fail {
        Fail::new(Status::Rejected, format!("the player's {kind} is still on its way there"))
    }

    /// The refusal of changing a center of trade in a province that has none.
    pub fn no_center_of_trade() -> Fail {
        Fail::new(Status::Rejected, "the province has no center of trade")
    }

    /// The refusal of raising the automatic missionaries when the game already
    /// sends all of the player's `missionaries`.
    pub fn all_automatic(missionaries: u32) -> Fail {
        Fail::new(Status::Rejected, format!("the game already sends all of the player's {missionaries} missionaries by itself"))
    }

    /// The refusal of lowering the automatic missionaries when the game sends
    /// none.
    pub fn none_automatic() -> Fail {
        Fail::new(Status::Rejected, "the game sends none of the player's missionaries by itself")
    }

    /// The refusal of choosing the trading policy the player already follows in
    /// the node.
    pub fn policy_followed(policy: &str) -> Fail {
        Fail::new(Status::Rejected, format!("the player already follows {policy} there"))
    }

    /// The refusal of expelling a minority from a province the colonize button
    /// does not list for it: not the player's, or with no minority to expel.
    pub fn no_minority(from: u32) -> Fail {
        Fail::new(Status::Rejected, format!("the colonize button lists no minority to expel from province {from}"))
    }

    /// The refusal of a content key `key` that names no `kind` ("trade company
    /// investment", "trade node").
    pub fn unknown(kind: &str, key: &str) -> Fail {
        Fail::new(Status::NotFound, format!("no {kind} is named {key}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_framework_s_own_refusals_name_what_is_missing() {
            assert_eq!(none_there("colonist").message, "the player has no colonist there");
            assert_eq!(all_automatic(3).message, "the game already sends all of the player's 3 missionaries by itself");
            assert_eq!(unknown("trade node", "nowhere").status, Status::NotFound);
            assert_eq!(policy_followed("maximize_profit").message, "the player already follows maximize_profit there");
            assert_eq!(no_minority(151).message, "the colonize button lists no minority to expel from province 151");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_rules_that_do_not_hold_are_reasons() {
        let text = b"\xa3yes Has a free merchant\n\xa3no In range\n";
        assert_eq!(super::failing(text), ["In range"]);
        assert_eq!(super::failing(b"Not discovered\n"), ["Not discovered"]);
        let held = b"Requirements:\n\xa3yes Fewer than one mess\nCannot afford 1000\n";
        assert_eq!(super::failing(held), ["Cannot afford 1000"]);
    }
}
