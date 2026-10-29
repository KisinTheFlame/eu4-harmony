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
use std::sync::{Mutex, OnceLock};

/// The kinds of name, by the game's key and the framework's template.
const KINDS: [(&str, &str); 3] = [("BATTLE_OF", "EU4HARMONY_BATTLE"), ("SIEGE_OF", "EU4HARMONY_SIEGE"), ("OCCUPATION_OF", "EU4HARMONY_OCCUPATION")];

/// The game's words for the kinds, as localised: looked up the first time a
/// battle is named, and again while a lookup fails.
static WORDS: Mutex<Option<Vec<Vec<u8>>>> = Mutex::new(None);
/// Whether the player language words battle names its own way.
static REWORDS: OnceLock<bool> = OnceLock::new();

fn localised(key: &str) -> crate::Result<Vec<u8>> {
    let m = crate::memory().ok_or("no memory")?;
    let key = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
    game_string::localized(m, key.as_ptr() as usize)
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
        if !*REWORDS.get_or_init(|| KINDS.iter().any(|&(_, template)| loc::framework_own(language, template, &[]).is_some())) {
            return;
        }
        let words = {
            let mut words = WORDS.lock().unwrap_or_else(|e| e.into_inner());
            if words.is_none() {
                *words = KINDS.iter().map(|&(key, _)| localised(key)).collect::<crate::Result<Vec<_>>>().ok();
            }
            let Some(words) = words.clone() else { return };
            words
        };
        if let Some(name) = reordered(language, unsafe { game_string::text(out) }, &words) {
            unsafe { game_string::assign(out, &name) };
        }
    });
    out
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(0x23a900usize, "48 89 5c 24 08 48 89 74 24 18 48 89 54 24 10 55", "battle name", battle_name as BattleName, &ORIGINAL_BATTLE_NAME),
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
