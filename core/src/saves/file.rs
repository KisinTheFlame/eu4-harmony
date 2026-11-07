//! What a save says about itself: its player, its date and the mods it was
//! made with, from its header (`meta` in a compressed save), as the continue
//! box knows them before the save is loaded (docs/research/main-menu.md).
//!
//! A save is plain text (`EU4txt`) or a zip file whose `meta` entry is
//! deflated text or the game's binary form (`EU4bin`, ironman saves).
use crate::Result;
use std::{fs::File, io::Read, path::Path};

/// The observer's tag, a save of observe mode's player.
pub const OBSERVER: [u8; 3] = *b"O00";

/// What the header of a save says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// The player's country tag; None for a save of observe mode.
    pub player: Option<[u8; 3]>,
    /// It is a save of observe mode: its player is the observer's tag,
    /// [`OBSERVER`].
    pub observing: bool,
    /// The game date (docs/research/dates.md).
    pub date: Option<i32>,
    /// The mods the game was played with when it was saved, in the
    /// playset's order (`mods_enabled_names`).
    pub mods: Vec<SaveMod>,
}

/// A mod a save was made with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveMod {
    /// Its descriptor, as the playset names it: `mod/ugc_2976470733.mod`.
    pub file: String,
    /// Its name, as its descriptor says it.
    pub name: String,
}

/// The most of a header read: `meta` is a few kilobytes.
const MOST: usize = 1 << 20;
/// How much of a plain save is read for its header first: the header comes
/// first, and is a few kilobytes but for a playset of hundreds of mods. A
/// header whose list of mods runs on past it is read on, up to [`MOST`].
const TEXT_FIRST: usize = 64 * 1024;
/// Why a header is no use whose list of mods ends before it closes.
const CUT_SHORT: &str = "the save's list of mods is cut short";

/// Reads the header of the save at `path`.
pub fn read(path: &Path) -> Result<Header> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut start = [0u8; 6];
    file.read_exact(&mut start).map_err(|e| e.to_string())?;
    if start.starts_with(b"PK") {
        return parse(&crate::zip::find(&mut file, b"meta", MOST)?);
    }
    let mut head = start.to_vec();
    (&mut file).take(TEXT_FIRST as u64).read_to_end(&mut head).map_err(|e| e.to_string())?;
    match parse(&head) {
        Err(e) if e == CUT_SHORT => {
            file.take((MOST - head.len()) as u64).read_to_end(&mut head).map_err(|e| e.to_string())?;
            parse(&head)
        }
        read => read,
    }
}

/// The header in a save's `meta`, or the start of a plain one.
pub fn parse(meta: &[u8]) -> Result<Header> {
    if meta.starts_with(b"EU4txt") {
        let text = String::from_utf8_lossy(meta);
        let value = |key: &str| text.lines().find_map(|line| line.strip_prefix(key)).map(|v| v.trim().trim_matches('"').to_string());
        let player = value("player=").and_then(|p| <[u8; 3]>::try_from(p.as_bytes()).ok());
        let date = value("date=").and_then(|d| eu4_harmony_remote::Date::parse(&d)).map(|d| d.0);
        return Ok(Header { player: country(player), observing: player == Some(OBSERVER), date, mods: text_mods(&text)? });
    }
    if meta.starts_with(b"EU4bin") {
        // `player = "TAG"`: the token 0x2a38, `=`, a string of length 3.
        let player =
            find(meta, &[0x38, 0x2a, 0x01, 0x00, 0x0f, 0x00, 0x03, 0x00]).and_then(|at| meta.get(at..at + 3)).and_then(|p| <[u8; 3]>::try_from(p).ok());
        // `date = i32`: the token 0x284d, `=`, an integer.
        let date = find(meta, &[0x4d, 0x28, 0x01, 0x00, 0x0c, 0x00]).and_then(|at| meta.get(at..at + 4)).map(|d| i32::from_le_bytes(d.try_into().unwrap()));
        return Ok(Header { player: country(player), observing: player == Some(OBSERVER), date, mods: binary_mods(meta)? });
    }
    Err("not a save the game wrote".into())
}

/// The country a save's player tag names: none for no tag (`---`) and for
/// the observer's.
fn country(player: Option<[u8; 3]>) -> Option<[u8; 3]> {
    player.filter(|p| p != b"---" && *p != OBSERVER)
}

/// The mods in a text header: `mods_enabled_names={ { filename="..."
/// name="..." } ... }`; none when it has no such list, and [`CUT_SHORT`]
/// when the text ends before the list closes.
fn text_mods(text: &str) -> Result<Vec<SaveMod>> {
    let Some(start) = text.find("mods_enabled_names=") else { return Ok(Vec::new()) };
    let mut rest = text[start + "mods_enabled_names=".len()..].trim_start();
    let Some(list) = rest.strip_prefix('{') else { return Ok(Vec::new()) };
    rest = list;
    let (mut mods, mut depth, mut key) = (Vec::new(), 1, String::new());
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            '"' => {
                let Some((value, after)) = rest.split_once('"') else { return Err(CUT_SHORT.into()) };
                rest = after;
                match key.as_str() {
                    "filename" => mods.push(SaveMod { file: value.to_string(), name: String::new() }),
                    "name" => {
                        if let Some(last) = mods.last_mut() {
                            last.name = value.to_string();
                        }
                    }
                    _ => {}
                }
                key.clear();
            }
            '=' => {}
            c if c.is_whitespace() => {}
            c => {
                // A key, up to its `=`.
                let word: String = std::iter::once(c).chain(rest.chars().take_while(|c| !c.is_whitespace() && *c != '=')).collect();
                rest = &rest[word.len() - c.len_utf8()..];
                key = word;
            }
        }
    }
    if depth != 0 {
        return Err(CUT_SHORT.into());
    }
    Ok(mods)
}

/// The mods in a binary header: the token 0x3b51 (`mods_enabled_names`),
/// `=`, `{`, then for each mod `{`, its `filename` (token 0x03b6) and its
/// `name` (0x001b), each `=` and a string, and `}`; then `}`. None when it
/// has no such list.
fn binary_mods(meta: &[u8]) -> Result<Vec<SaveMod>> {
    const LIST: [u8; 6] = [0x51, 0x3b, 0x01, 0x00, 0x03, 0x00];
    let Some(mut at) = find(meta, &LIST) else { return Ok(Vec::new()) };
    let cut_short = || CUT_SHORT.to_string();
    let word = |at: &mut usize| -> Result<u16> {
        let value = meta.get(*at..*at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(cut_short)?;
        *at += 2;
        Ok(value)
    };
    let mut mods = Vec::new();
    loop {
        match word(&mut at)? {
            // `}`: the list's end.
            0x0004 => return Ok(mods),
            // `{`: a mod.
            0x0003 => {
                let mut one = SaveMod::default();
                loop {
                    let token = word(&mut at)?;
                    if token == 0x0004 {
                        break;
                    }
                    if word(&mut at)? != 0x0001 {
                        return Err("unexpected token in the save's list of mods".into());
                    }
                    // A string, quoted (0x000f) or not (0x0017).
                    if !matches!(word(&mut at)?, 0x000f | 0x0017) {
                        return Err("unexpected value in the save's list of mods".into());
                    }
                    let len = word(&mut at)? as usize;
                    let text = String::from_utf8_lossy(meta.get(at..at + len).ok_or_else(cut_short)?).into_owned();
                    at += len;
                    match token {
                        0x03b6 => one.file = text,
                        0x001b => one.name = text,
                        _ => {}
                    }
                }
                mods.push(one);
            }
            _ => return Err("unexpected token in the save's list of mods".into()),
        }
    }
}

/// Where the bytes after the first `pattern` in `data` start.
fn find(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len()).position(|w| w == pattern).map(|at| at + pattern.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `f` makes of `data`, which must not panic.
    fn calmly<T>(what: &str, data: &[u8], f: impl Fn(&[u8]) -> T) -> T {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(data))).unwrap_or_else(|_| panic!("{what} panicked"))
    }

    #[test]
    fn a_header_says_the_player_and_the_date() {
        let text = b"EU4txt\ndate=1470.3.13\nsave_game=\".eu4\"\nplayer=\"CAS\"\ndisplayed_country_name=\"x\"\n";
        let date = eu4_harmony_remote::Date::from_ymd(1470, 3, 13).unwrap().0;
        assert_eq!(parse(text).unwrap(), Header { player: Some(*b"CAS"), observing: false, date: Some(date), mods: Vec::new() });
        // Binary, as an ironman save's meta begins.
        let mut binary = b"EU4bin".to_vec();
        binary.extend_from_slice(&[0x4d, 0x28, 0x01, 0x00, 0x0c, 0x00]);
        binary.extend_from_slice(&0x0360_d9f8i32.to_le_bytes());
        binary.extend_from_slice(&[0x38, 0x2a, 0x01, 0x00, 0x0f, 0x00, 0x03, 0x00]);
        binary.extend_from_slice(b"CAS");
        assert_eq!(parse(&binary).unwrap(), Header { player: Some(*b"CAS"), observing: false, date: Some(0x0360_d9f8), mods: Vec::new() });
        // A save of observe mode: the observer's tag, which is no country.
        let observed = parse(b"EU4txt\ndate=1600.1.1\nplayer=\"O00\"\nnot_observer=no\n").unwrap();
        assert_eq!((observed.player, observed.observing), (None, true));
        assert!(!parse(b"EU4txt\ndate=1600.1.1\nplayer=\"---\"\n").unwrap().observing);
        assert_eq!(parse(b"EU4txt\ndate=1444.11.11\nplayer=\"---\"\n").unwrap().player, None);
        assert!(parse(b"PK\x03\x04").is_err());
    }

    fn save_mod(file: &str, name: &str) -> SaveMod {
        SaveMod { file: file.into(), name: name.into() }
    }

    #[test]
    fn a_plain_save_s_mods_are_read_past_the_first_read() {
        // A playset of a thousand mods takes more than the first read.
        let mut text = String::from("EU4txt\ndate=1445.7.19\nplayer=\"WUR\"\nmods_enabled_names={\n");
        for i in 0..1000 {
            text.push_str(&format!("\t{{\n\t\tfilename=\"mod/ugc_{i:010}.mod\"\n\t\tname=\"A mod with a long name, number {i}\"\n\t}}\n"));
        }
        text.push_str("}\nmulti_player=no\n");
        assert!(text.len() > TEXT_FIRST);
        let path = std::env::temp_dir().join(format!("eu4-harmony-test-{}-many-mods.eu4", std::process::id()));
        std::fs::write(&path, &text).unwrap();
        let header = read(&path);
        std::fs::remove_file(&path).unwrap();
        let mods = header.unwrap().mods;
        assert_eq!(mods.len(), 1000);
        assert_eq!(mods[999], save_mod("mod/ugc_0000000999.mod", "A mod with a long name, number 999"));
    }

    #[test]
    fn a_header_says_the_mods_the_save_was_made_with() {
        // As the game writes them; a name with braces and Chinese.
        let text = "EU4txt\ndate=1445.7.19\nplayer=\"WUR\"\nmods_enabled_names={\n\t{\n\t\tfilename=\"mod/ugc_3047072888.mod\"\n\t\tname=\"彼岸 {乐园}\"\n\t}\n {\n\t\tfilename=\"mod/ugc_2976470733.mod\"\n\t\tname=\"Chinese Language Mod for 1.37\"\n\t}\n  \n}\nmulti_player=no\n";
        let mods = vec![save_mod("mod/ugc_3047072888.mod", "彼岸 {乐园}"), save_mod("mod/ugc_2976470733.mod", "Chinese Language Mod for 1.37")];
        assert_eq!(parse(text.as_bytes()).unwrap().mods, mods);
        assert_eq!(parse(b"EU4txt\nmods_enabled_names={\n}\n").unwrap().mods, []);
        assert_eq!(parse(b"EU4txt\nplayer=\"FRA\"\n").unwrap().mods, []);
        // Cut short anywhere in the list, a name or a file name: an error,
        // never part of a name.
        let list = text.find("mods_enabled_names={").unwrap() + "mods_enabled_names={".len();
        let end = text.find("multi_player").unwrap();
        for len in (list..=end - 2).filter(|&len| text.is_char_boundary(len)) {
            assert_eq!(parse(&text.as_bytes()[..len]).unwrap_err(), CUT_SHORT, "the first {len} bytes");
        }

        // Binary, as an ironman save's meta has them.
        let mut binary = b"EU4bin".to_vec();
        binary.extend_from_slice(&[0x51, 0x3b, 0x01, 0x00, 0x03, 0x00]);
        for (file, name) in [("mod/ugc_253263609.mod", "Graphical Map Improvements"), ("mod/ugc_2830362508.mod", "Emoji Universalis 抽象风云")] {
            binary.extend_from_slice(&[0x03, 0x00, 0xb6, 0x03, 0x01, 0x00, 0x0f, 0x00]);
            binary.extend_from_slice(&(file.len() as u16).to_le_bytes());
            binary.extend_from_slice(file.as_bytes());
            binary.extend_from_slice(&[0x1b, 0x00, 0x01, 0x00, 0x0f, 0x00]);
            binary.extend_from_slice(&(name.len() as u16).to_le_bytes());
            binary.extend_from_slice(name.as_bytes());
            binary.extend_from_slice(&[0x04, 0x00]);
        }
        binary.extend_from_slice(&[0x04, 0x00]);
        let mods = vec![save_mod("mod/ugc_253263609.mod", "Graphical Map Improvements"), save_mod("mod/ugc_2830362508.mod", "Emoji Universalis 抽象风云")];
        assert_eq!(parse(&binary).unwrap().mods, mods);
        // Cut short anywhere in the list: an error, never a panic.
        for len in 12..binary.len() {
            assert!(calmly(&format!("reading the first {len} bytes"), &binary[..len], parse).is_err(), "the first {len} bytes read");
        }
        assert_eq!(parse(b"EU4bin").unwrap().mods, []);
    }
}
