//! Battle names in the player language's word order
//! (docs/research/names.md). 0x23a900 names a battle, a siege or an
//! occupation as the localised word (`BATTLE_OF`, `SIEGE_OF`,
//! `OCCUPATION_OF`), a space and the province: "Battle of Athens", but also
//! "战役 雅典" where Chinese says "雅典战役". Battle results, the combat and
//! siege views, their tooltips and a leader's death in battle use it.
//!
//! A language's templates (`EU4HARMONY_BATTLE`, `_SIEGE`, `_OCCUPATION`, with
//! `$KIND$`, the localised word, and `$PLACE$`) word the name instead. The
//! war overview's battle rows put their names together elsewhere and keep the
//! game's order.
use crate::game_string;
use crate::lang;
use crate::loc;
use crate::points::{Function, Point};
use crate::unicode::game_text;
use eu4_harmony_loc::Language;
use std::sync::OnceLock;

/// The game's localisation lookup: (out, key, values, count); out is a
/// `std::string` slot it constructs.
const LOCALISE: usize = 0x14b34a0;
/// Destroys a `std::string`.
const DESTROY_STRING: usize = 0x95660;

/// The kinds of name, by the game's key and the framework's template.
const KINDS: [(&str, &str); 3] = [("BATTLE_OF", "EU4HARMONY_BATTLE"), ("SIEGE_OF", "EU4HARMONY_SIEGE"), ("OCCUPATION_OF", "EU4HARMONY_OCCUPATION")];

/// The game's words for the kinds, as localised: looked up once, the first
/// time a battle is named.
static WORDS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();

fn localised(key: &str) -> crate::Result<Vec<u8>> {
    let m = crate::memory().ok_or("no memory")?;
    let key = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
    let mut slot = [0u8; 0x20];
    let out = slot.as_mut_ptr() as usize;
    m.call(m.at(LOCALISE), &[out, key.as_ptr() as usize, 0, 0])?;
    let text = game_string::read(m, out);
    m.call(m.at(DESTROY_STRING), &[out])?;
    text
}

/// `text` (game text), which is `word`, a space and the place for one of
/// the kinds, in `language`'s template for that kind; None when it is none
/// of them or the language keeps the game's order.
fn reordered(language: Language, text: &[u8], words: &[Vec<u8>]) -> Option<Vec<u8>> {
    KINDS.iter().zip(words).find_map(|(&(_, template), word)| {
        let place = text.strip_prefix(word.as_slice())?.strip_prefix(b" ")?;
        let (kind, place) = (game_text::to_utf8(word), game_text::to_utf8(place));
        loc::framework_own(language, template, &[("KIND", &kind), ("PLACE", &place)]).map(|name| game_text::from_str(&name))
    })
}

type BattleName = unsafe extern "C" fn(usize, *mut u8) -> *mut u8;

static ORIGINAL_BATTLE_NAME: Function<BattleName> = Function::new();

/// A battle's name (0x23a900): the game's, then reworded.
extern "C" fn battle_name(combat: usize, out: *mut u8) -> *mut u8 {
    let original = ORIGINAL_BATTLE_NAME.get();
    let out = unsafe { original(combat, out) };
    crate::guarded("battle name", || {
        let language = lang::player();
        // Most languages have no templates: leave their names, unlooked.
        if KINDS.iter().all(|&(_, template)| loc::framework_own(language, template, &[]).is_none()) {
            return;
        }
        let words = WORDS.get_or_init(|| KINDS.iter().map(|&(key, _)| localised(key).unwrap_or_default()).collect());
        if let Some(name) = reordered(language, unsafe { game_string::text(out) }, words) {
            unsafe { game_string::assign(out, &name) };
        }
    });
    out
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(0x23a900usize, "48 89 5c 24 08 48 89 74 24 18 48 89 54 24 10 55", "battle name", battle_name as BattleName, &ORIGINAL_BATTLE_NAME),
        Point::keep(LOCALISE, "44 89 4c 24 20 4c 89 44 24 18 55 56 57 41 54 41", "localisation lookup"),
        Point::keep(DESTROY_STRING, "40 53 48 83 ec 20 48 8b 51 18 48 8b d9 48 83 fa", "string destructor"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_puts_the_place_where_it_belongs() {
        let words: Vec<Vec<u8>> = ["战役", "围攻", "占领"].iter().map(|w| game_text::from_str(w)).collect();
        let name = |text: &str| reordered(Language::SimpChinese, &game_text::from_str(text), &words).map(|n| game_text::to_utf8(&n));
        assert_eq!(name("战役 雅典").as_deref(), Some("雅典战役"));
        assert_eq!(name("围攻 雅典").as_deref(), Some("围攻雅典"));
        assert_eq!(name("占领 Athens").as_deref(), Some("占领Athens"));
        assert_eq!(name("雅典"), None);
        let english: Vec<Vec<u8>> = ["Battle of", "Siege of", "Occupation of"].iter().map(|w| w.as_bytes().to_vec()).collect();
        assert_eq!(reordered(Language::English, b"Battle of Athens", &english), None);
    }
}
