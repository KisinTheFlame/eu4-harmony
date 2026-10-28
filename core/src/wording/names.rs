//! People's names family name first where their culture writes them so (East
//! Asian), when the player language asks for it (docs/research/names.md).
//! The game always joins a person's given name, a space and their dynasty:
//! "Qizhen Zhu", "祁镇 朱". It does so for rulers, heirs and consorts in
//! 0xa4b2c0, which the court view, their tooltips and most messages use, and
//! in place for the heir in the lines on what follows the ruler's death
//! (0x318870).
//!
//! The player language's template (`EU4HARMONY_NAME_FAMILY_FIRST`, with
//! `$FAMILY$` and `$GIVEN$`) words the name, and a space left between two
//! Chinese, Japanese or Korean characters is dropped: "朱祁镇", but "Zhu
//! Qizhen". Generals' and advisors' names are made once, as one string, when
//! they are created, and keep the game's order, as do names that localisation
//! puts together itself (`[Root.Monarch.GetName]` is the given name only).
use crate::game_string;
use crate::lang;
use crate::loc;
use crate::memory::Memory;
use crate::points::{Point, Resume};
use crate::unicode::game_text;
use eu4_harmony_loc::Language;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A ruler, heir or consort: their given name (a `std::string`, regnal
/// number included), their dynasty, whose name is a `std::string` at +8, and
/// their culture.
const GIVEN_NAME: usize = 0x28;
const DYNASTY: usize = 0x68;
const DYNASTY_NAME: usize = 0x8;
const CULTURE: usize = 0x60;
/// A culture: its key as in common/cultures and its group; a group's key.
const CULTURE_KEY: usize = 0x48;
const CULTURE_GROUP: usize = 0x88;
const GROUP_KEY: usize = 0x38;

/// Culture groups whose people put their family name first, the cultures in
/// them that do not, and cultures elsewhere that do (common/cultures).
const GROUPS: &[&str] = &["east_asian", "korean_g", "japanese_g"];
const EXCEPT: &[&str] = &["tibetan_new", "altaic_new", "zhuang_new", "manchu_new"];
const CULTURES: &[&str] = &["vietnamese"];

fn family_first(culture: &[u8], group: &[u8]) -> bool {
    let listed = |list: &[&str], key: &[u8]| list.iter().any(|k| k.as_bytes() == key);
    listed(CULTURES, culture) || (listed(GROUPS, group) && !listed(EXCEPT, culture))
}

/// Whether the person at `person` is of such a culture.
fn writes_family_first(m: &Memory, person: usize) -> bool {
    let read = || -> crate::Result<bool> {
        let culture = m.ptr(person + CULTURE)?;
        let group = m.ptr(culture + CULTURE_GROUP)?;
        Ok(family_first(&game_string::read(m, culture + CULTURE_KEY)?, &game_string::read(m, group + GROUP_KEY)?))
    };
    read().unwrap_or(false)
}

fn cjk(c: char) -> bool {
    matches!(c as u32, 0x2e80..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0x20000..=0x3ffff)
}

/// Drops each space that stands between two Chinese, Japanese or Korean
/// characters.
fn close_cjk_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let gap = |i: usize| chars[i] == ' ' && i > 0 && i + 1 < chars.len() && cjk(chars[i - 1]) && cjk(chars[i + 1]);
    (0..chars.len()).filter(|&i| !gap(i)).map(|i| chars[i]).collect()
}

/// `text` (game text) with `given`, a space and `family` at its start put in
/// `language`'s family-first order; None when it does not start so or the
/// language keeps the game's order.
fn reordered(language: Language, text: &[u8], given: &[u8], family: &[u8]) -> Option<Vec<u8>> {
    let prefix = [given, b" ", family].concat();
    let rest = text.strip_prefix(prefix.as_slice())?;
    let (family, given) = (game_text::to_utf8(family), game_text::to_utf8(given));
    let name = loc::framework_own(language, "EU4HARMONY_NAME_FAMILY_FIRST", &[("FAMILY", &family), ("GIVEN", &given)])?;
    Some([game_text::from_str(&close_cjk_spaces(&name)).as_slice(), rest].concat())
}

type FullName = unsafe extern "C" fn(usize, *mut u8) -> *mut u8;

static ORIGINAL_FULL_NAME: AtomicUsize = AtomicUsize::new(0);

/// A ruler's, heir's or consort's full name (0xa4b2c0): the game's, then
/// reordered.
extern "C" fn full_name(person: usize, out: *mut u8) -> *mut u8 {
    let original: FullName = unsafe { std::mem::transmute(ORIGINAL_FULL_NAME.load(Ordering::Acquire)) };
    let out = unsafe { original(person, out) };
    crate::guarded("full name", || {
        let reorder = || -> Option<()> {
            let m = crate::memory()?;
            if !writes_family_first(m, person) {
                return None;
            }
            let given = game_string::read(m, person + GIVEN_NAME).ok()?;
            let family = game_string::read(m, m.ptr(person + DYNASTY).ok()? + DYNASTY_NAME).ok()?;
            let name = reordered(lang::player(), unsafe { game_string::text(out) }, &given, &family)?;
            unsafe { game_string::assign(out, &name) };
            Some(())
        };
        reorder();
    });
    out
}

/// Where the heir's given name in the `std::string` `name` gets `suffix` (a
/// space and the dynasty): the heir's name in family-first order when their
/// culture writes it so, else the suffix appended as the game does.
extern "C" fn heir_name(name: *mut u8, suffix: *const u8, len: usize, heir: usize) {
    crate::guarded("heir name", || unsafe {
        let suffix = std::slice::from_raw_parts(suffix, len);
        let family_first_name = || -> Option<Vec<u8>> {
            let m = crate::memory()?;
            if !writes_family_first(m, heir) {
                return None;
            }
            let family = suffix.strip_prefix(b" ")?;
            let text = [game_string::text(name), suffix].concat();
            reordered(lang::player(), &text, game_string::text(name), family)
        };
        match family_first_name() {
            Some(text) => game_string::assign(name, &text),
            None => game_string::append(name as usize, suffix),
        }
    })
}

macro_rules! heir_stub {
    ($stub:ident, $done:ident, $resume:ident, $rva:expr) => {
        static $done: AtomicUsize = AtomicUsize::new(0);
        static $resume: [Resume; 1] = [Resume { rva: $rva, address: &$done }];

        /// Replaces `call append` (0x932f0) of the heir's given name in
        /// `rcx` with " " + dynasty (`rdx`, `r8` bytes); the heir is in `rbx`.
        #[unsafe(naked)]
        extern "C" fn $stub() {
            std::arch::naked_asm!(
                call_rust!("mov r9, rbx", call heir),
                "jmp qword ptr [rip + {done}]",
                heir = sym heir_name,
                done = sym $done,
            );
        }
    };
}

heir_stub!(heir_succeeds_stub, SUCCEEDS_DONE, SUCCEEDS_RESUME, 0x31925c);
heir_stub!(heir_regency_stub, REGENCY_DONE, REGENCY_RESUME, 0x319329);

pub fn points() -> Vec<Point> {
    let stub = |f: extern "C" fn()| f as *const () as usize;
    vec![
        Point::replace(0xa4b2c0usize, "48 89 5c 24 18 48 89 54 24 10 55 56 57 48 8b ec", "person's full name", full_name as *const () as usize, &ORIGINAL_FULL_NAME),
        // How 0xa4b2c0 joins them: the given name, a space, the dynasty's name.
        Point::keep(0xa4b2dfusize, "48 8d 51 28", "person given name"),
        Point::keep(0xa4b44cusize, "48 8b 57 68 48 83 c2 08", "person dynasty name"),
        Point::keep(0xa4b468usize, "66 c7 45 b8 20 00", "person name separator"),
        Point::keep(0x280abausize, "48 8b 41 60", "person culture"),
        Point::keep(0x27e8a7usize, "4c 8d 76 48", "culture key"),
        Point::keep(0x280952usize, "48 8b 91 88 00 00 00", "culture group"),
        Point::keep(0x27ca3cusize, "48 8d 71 38", "culture group key"),
        // The ruler's death lines: the heir (rbx) was the given name's owner.
        Point::keep(0x3191fausize, "48 8b cb", "heir for the death lines"),
        Point::keep(0x319252usize, "48 8d 4c 24 40", "heir name for succession"),
        Point::jump(0x319257usize, "e8 94 a0 d7 ff", "heir name and dynasty for succession", stub(heir_succeeds_stub), &SUCCEEDS_RESUME),
        Point::keep(0x31931fusize, "48 8d 4c 24 40", "heir name for regency"),
        Point::jump(0x319324usize, "e8 c7 9f d7 ff", "heir name and dynasty for regency", stub(heir_regency_stub), &REGENCY_RESUME),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn east_asian_cultures_put_the_family_first() {
        assert!(family_first(b"chihan", b"east_asian"));
        assert!(family_first(b"korean", b"korean_g"));
        assert!(family_first(b"japanese", b"japanese_g"));
        assert!(family_first(b"vietnamese", b"southeastasian_group"));
        assert!(!family_first(b"tibetan_new", b"east_asian"));
        assert!(!family_first(b"khmer", b"southeastasian_group"));
        assert!(!family_first(b"english", b"british"));
    }

    #[test]
    fn chinese_puts_the_family_first_and_closes_the_gap_between_characters() {
        let chinese = Language::SimpChinese;
        assert_eq!(reordered(chinese, b"Qizhen Zhu", b"Qizhen", b"Zhu").as_deref(), Some(&b"Zhu Qizhen"[..]));
        let cjk = game_text::from_str("祁镇 朱");
        let (given, family) = (game_text::from_str("祁镇"), game_text::from_str("朱"));
        assert_eq!(reordered(chinese, &cjk, &given, &family), Some(game_text::from_str("朱祁镇")));
        // A regency's title after the name stays where it is.
        assert_eq!(reordered(chinese, b"Qizhen Zhu (Regent)", b"Qizhen", b"Zhu").as_deref(), Some(&b"Zhu Qizhen (Regent)"[..]));
        assert_eq!(reordered(chinese, b"Qizhen", b"Qizhen", b"Zhu"), None);
        assert_eq!(reordered(Language::English, b"Qizhen Zhu", b"Qizhen", b"Zhu"), None);
    }
}
