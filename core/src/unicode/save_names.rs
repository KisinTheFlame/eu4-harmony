//! Save file names. Before a save's name becomes a path, the game turns it to
//! ASCII in place (0x1705180, `std::string*`): þ Þ Ð ð become t T D d and
//! every other byte from 0x80 up goes through the table at 0x1c37af0
//! (letters lose their accents, the rest becomes `?`). On UTF-8 that turns
//! every continuation byte into `?`, which a file name cannot hold. The
//! replacement keeps UTF-8 sequences, which the game's file system (PhysFS)
//! takes as they are, and turns the Windows-1252 bytes to ASCII as before, so
//! a name without other characters gives the same file as it always did.
use crate::game_string;
use super::game_text;
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicUsize, Ordering};

const TRANSLITERATE: usize = 0x1705180;
const TABLE: usize = 0x1c37af0;

type Transliterate = unsafe extern "C" fn(*mut u8);

static ORIGINAL: Function<Transliterate> = Function::new();
static TABLE_AT: AtomicUsize = AtomicUsize::new(0);

/// The ASCII byte the game gives a Windows-1252 byte from 0x80 up.
fn ascii(byte: u8, table: &[u8; 128]) -> u8 {
    match byte {
        0xfe => b't',
        0xde => b'T',
        0xd0 => b'D',
        0xf0 => b'd',
        _ => table[(byte - 0x80) as usize],
    }
}

/// Transliterates `name` in place, keeping its length.
pub fn transliterate(name: &mut [u8], table: &[u8; 128]) {
    game_text::map_single_bytes(name, |byte| if byte >= 0x80 { ascii(byte, table) } else { byte });
}

unsafe extern "C" fn transliterate_string(string: *mut u8) {
    crate::guarded("save name", || {
        let table = &*(TABLE_AT.load(Ordering::Acquire) as *const [u8; 128]);
        transliterate(game_string::text_mut(string), table);
    })
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(TRANSLITERATE, "8b 41 10 85 c0 0f 84 0c 01 00 00 45 33 c0 4c 8d", "save name to ASCII", transliterate_string as Transliterate, &ORIGINAL),
        Point::keep(TABLE, "3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f", "ASCII table for bytes from 0x80").recorded(&TABLE_AT),
    ]
}

#[cfg(test)]
mod tests {
    use super::transliterate;

    #[test]
    fn keeps_utf8_and_turns_1252_to_ascii() {
        let mut table = [b'?'; 128];
        table[0xe9 - 0x80] = b'e';
        let mut name = b"Caf\xe9 \xfe".to_vec();
        name.extend_from_slice("中国".as_bytes());
        transliterate(&mut name, &table);
        let mut expected = b"Cafe t".to_vec();
        expected.extend_from_slice("中国".as_bytes());
        assert_eq!(name, expected);
    }
}
