//! Converts UTF-8 text from extensions into the bytes the game draws.
//!
//! With the Chinese/Japanese font plugin (matanki-saito's EU4dll, loaded as
//! `plugin64.dll`), CJK ideographs travel as 3-byte escapes; see
//! `convertWideTextToEscapedText` in its `escape_tool.cpp`. Without the plugin
//! the game draws Latin-1, so other characters become `?`.
use std::sync::OnceLock;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

/// Bytes the plugin reserves; ideograph bytes that collide are shifted.
const FORBIDDEN: &[u8] = &[
    0xa4, 0xa3, 0xa7, 0x24, 0x5b, 0x00, 0x5c, 0x20, 0x0d, 0x0a, 0x22, 0x7b, 0x7d, 0x40, 0x80, 0x7e, 0x2f, 0x5f, 0xbd, 0x3b,
    0x5d, 0x3d, 0x23, 0x3f, 0x3a, 0x3c, 0x3e, 0x2a, 0x7c,
];

pub fn plugin_loaded() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| {
        let name: Vec<u16> = "plugin64.dll\0".encode_utf16().collect();
        !unsafe { GetModuleHandleW(name.as_ptr()) }.is_null()
    })
}

/// Full-width punctuation the plugin cannot draw, with ASCII stand-ins.
fn ascii_punctuation(c: char) -> Option<char> {
    Some(match c {
        '，' | '、' => ',',
        '。' => '.',
        '！' => '!',
        '？' => '?',
        '：' => ':',
        '；' => ';',
        '（' => '(',
        '）' => ')',
        '“' | '”' => '"',
        '‘' | '’' => '\'',
        '—' | '－' => '-',
        '　' => ' ',
        _ => return None,
    })
}

/// Encodes `text`; the flag reports whether any character was replaced by `?`.
pub fn encode(text: &str, plugin: bool) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(text.len());
    let mut lossy = false;
    for c in text.chars() {
        let c = ascii_punctuation(c).unwrap_or(c);
        let cp = c as u32;
        match cp {
            0x0a | 0x20..=0x7e => out.push(cp as u8),
            // EU4's formatting marker, as in "§Y...§!".
            0xa7 => out.push(0xa7),
            0x4e00..=0x9fff if plugin => {
                let mut low = cp as u8;
                let mut high = (cp >> 8) as u8;
                let low_escape = FORBIDDEN.contains(&low);
                let high_escape = FORBIDDEN.contains(&high);
                let escape = 0x10 + u8::from(low_escape) + 2 * u8::from(high_escape);
                if low_escape {
                    low = low.wrapping_add(0x0e);
                }
                if high_escape {
                    high = high.wrapping_sub(0x09);
                }
                out.extend_from_slice(&[escape, low, high]);
            }
            0xa0..=0xff if !plugin => out.push(cp as u8),
            _ => {
                out.push(b'?');
                lossy = true;
            }
        }
    }
    (out, lossy)
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn ascii_and_markers_pass_through() {
        assert_eq!(encode("R 5\n§Y!", true).0, b"R 5\n\xa7Y!");
    }

    #[test]
    fn ideographs_use_plugin_escapes() {
        // 区 U+533A: low 0x3a and high 0x53; 0x3a is reserved, so it shifts by +0x0e.
        assert_eq!(encode("区", true).0, vec![0x11, 0x48, 0x53]);
        // 半 U+534A: neither byte is reserved.
        assert_eq!(encode("半", true).0, vec![0x10, 0x4a, 0x53]);
    }

    #[test]
    fn punctuation_and_unsupported_characters() {
        assert_eq!(encode("是，否。", true).0[3..4], *b",");
        let (bytes, lossy) = encode("é区", false);
        assert_eq!(bytes, vec![0xe9, b'?']);
        assert!(lossy);
    }
}
