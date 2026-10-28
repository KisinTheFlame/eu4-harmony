//! Searching names: the diplomacy view's country filter and the province
//! finder lower-case a copy of the query and of each name, then fold accents
//! with the function at 0x1704d70 (`std::string*`, in place), which turns
//! every byte from 0xC0 up into an ASCII letter. That also hits the lead byte
//! of every UTF-8 sequence, so different Chinese characters collide and ASCII
//! letters match inside Chinese names. The replacement folds the module's
//! single bytes the same way and leaves UTF-8 sequences whole. It runs on the
//! province finder's worker thread too.
use crate::game_string;
use super::game_text;
use crate::points::{Function, Point};

const FOLD: usize = 0x1704d70;
type Fold = unsafe extern "C" fn(*mut u8);

static ORIGINAL: Function<Fold> = Function::new();

/// The ASCII letter the game folds a Windows-1252 letter from 0xC0 up to.
fn folded(byte: u8) -> u8 {
    match byte {
        0xc0..=0xc6 => b'A',
        0xc7 => b'C',
        0xc8..=0xcb => b'E',
        0xcc..=0xcf => b'I',
        0xd0 => b'D',
        0xd1 => b'N',
        0xd2..=0xd6 | 0xd8 => b'O',
        0xd9..=0xdc => b'U',
        0xdd => b'Y',
        0xde => b't',
        0xdf => b's',
        0xe0..=0xe6 => b'a',
        0xe7 => b'c',
        0xe8..=0xeb => b'e',
        0xec..=0xef => b'i',
        0xf0 => b'd',
        0xf1 => b'n',
        0xf2..=0xf6 | 0xf8 => b'o',
        0xf9..=0xfc => b'u',
        0xfd | 0xff => b'y',
        0xfe => b't',
        other => other,
    }
}

/// Folds `text` in place, keeping its length.
pub fn fold(text: &mut [u8]) {
    game_text::map_single_bytes(text, folded);
}

unsafe extern "C" fn fold_string(string: *mut u8) {
    crate::guarded("search fold", || {
        fold(game_string::text_mut(string));
    })
}

pub fn points() -> Vec<Point> {
    vec![Point::replace(FOLD, "8b 41 10 85 c0 0f 84 7a 02 00 00 45 33 c0 44 8b", "search accent fold", fold_string as Fold, &ORIGINAL)]
}

#[cfg(test)]
mod tests {
    use super::fold;

    #[test]
    fn folds_1252_letters_and_keeps_utf8_whole() {
        let mut text = b"\xc9cole \xe9t\xe9 ".to_vec();
        text.extend_from_slice("中席".as_bytes());
        fold(&mut text);
        let mut expected = b"Ecole ete ".to_vec();
        expected.extend_from_slice("中席".as_bytes());
        assert_eq!(text, expected);
    }
}
