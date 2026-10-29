//! What a save says about itself: its player, its date and the mods it was
//! made with, from its header (`meta` in a compressed save), as the continue
//! box knows them before the save is loaded (docs/research/main-menu.md).
//!
//! A save is plain text (`EU4txt`) or a zip file whose `meta` entry is
//! deflated text or the game's binary form (`EU4bin`, ironman saves).
use crate::Result;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

/// What the header of a save says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// The player's country tag; None for a save of an observer.
    pub player: Option<[u8; 3]>,
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

/// Reads the header of the save at `path`.
pub fn read(path: &Path) -> Result<Header> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut start = [0u8; 6];
    file.read_exact(&mut start).map_err(|e| e.to_string())?;
    let meta = if start.starts_with(b"PK") {
        zip_entry(&mut file, b"meta")?
    } else {
        let mut head = start.to_vec();
        file.take(64 * 1024).read_to_end(&mut head).map_err(|e| e.to_string())?;
        head
    };
    parse(&meta)
}

/// The header in a save's `meta`, or the start of a plain one.
pub fn parse(meta: &[u8]) -> Result<Header> {
    if meta.starts_with(b"EU4txt") {
        let text = String::from_utf8_lossy(meta);
        let value = |key: &str| text.lines().find_map(|line| line.strip_prefix(key)).map(|v| v.trim().trim_matches('"').to_string());
        let player = value("player=").and_then(|p| <[u8; 3]>::try_from(p.as_bytes()).ok()).filter(|p| p != b"---");
        let date = value("date=").and_then(|d| eu4_harmony_remote::Date::parse(&d)).map(|d| d.0);
        return Ok(Header { player, date, mods: text_mods(&text) });
    }
    if meta.starts_with(b"EU4bin") {
        // `player = "TAG"`: the token 0x2a38, `=`, a string of length 3.
        let player = find(meta, &[0x38, 0x2a, 0x01, 0x00, 0x0f, 0x00, 0x03, 0x00])
            .and_then(|at| meta.get(at..at + 3))
            .and_then(|p| <[u8; 3]>::try_from(p).ok())
            .filter(|p| p != b"---");
        // `date = i32`: the token 0x284d, `=`, an integer.
        let date = find(meta, &[0x4d, 0x28, 0x01, 0x00, 0x0c, 0x00]).and_then(|at| meta.get(at..at + 4)).map(|d| i32::from_le_bytes(d.try_into().unwrap()));
        return Ok(Header { player, date, mods: binary_mods(meta)? });
    }
    Err("not a save the game wrote".into())
}

/// The mods in a text header: `mods_enabled_names={ { filename="..."
/// name="..." } ... }`; none when it has no such list.
fn text_mods(text: &str) -> Vec<SaveMod> {
    let Some(start) = text.find("mods_enabled_names=") else { return Vec::new() };
    let mut rest = text[start + "mods_enabled_names=".len()..].trim_start();
    let Some(list) = rest.strip_prefix('{') else { return Vec::new() };
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
                let value = rest.split('"').next().unwrap_or_default();
                rest = rest.get(value.len() + 1..).unwrap_or_default();
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
    mods
}

/// The mods in a binary header: the token 0x3b51 (`mods_enabled_names`),
/// `=`, `{`, then for each mod `{`, its `filename` (token 0x03b6) and its
/// `name` (0x001b), each `=` and a string, and `}`; then `}`. None when it
/// has no such list.
fn binary_mods(meta: &[u8]) -> Result<Vec<SaveMod>> {
    const LIST: [u8; 6] = [0x51, 0x3b, 0x01, 0x00, 0x03, 0x00];
    let Some(mut at) = find(meta, &LIST) else { return Ok(Vec::new()) };
    let cut_short = || "the save's list of mods is cut short".to_string();
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

fn u16_at(data: &[u8], at: usize) -> Result<usize> {
    data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or_else(|| "zip record cut short".to_string())
}

fn u32_at(data: &[u8], at: usize) -> Result<usize> {
    data.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).ok_or_else(|| "zip record cut short".to_string())
}

fn read_at(file: &mut (impl Read + Seek), at: u64, len: usize) -> Result<Vec<u8>> {
    let mut out = vec![0; len];
    file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    file.read_exact(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// The entry `name` of the zip file `file`, found through its central
/// directory, stored or deflated, and checked against its CRC.
fn zip_entry(file: &mut (impl Read + Seek), name: &[u8]) -> Result<Vec<u8>> {
    let size = file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let tail_len = size.min(66_000) as usize;
    let tail = read_at(file, size - tail_len as u64, tail_len)?;
    let end = tail.windows(4).rposition(|w| w == [0x50, 0x4b, 0x05, 0x06]).ok_or("no zip directory")?;
    // The end record is 22 bytes, its comment's length last.
    u16_at(&tail, end + 20)?;
    let (entries, directory_len, directory_at) = (u16_at(&tail, end + 10)?, u32_at(&tail, end + 12)?, u32_at(&tail, end + 16)?);
    if directory_len > MOST || directory_at as u64 + directory_len as u64 > size {
        return Err("zip directory out of bounds".into());
    }
    let directory = read_at(file, directory_at as u64, directory_len)?;
    let mut at = 0;
    for _ in 0..entries {
        if u32_at(&directory, at)? != 0x0201_4b50 {
            return Err("bad zip directory entry".into());
        }
        let (method, crc, packed, unpacked) = (u16_at(&directory, at + 10)?, u32_at(&directory, at + 16)?, u32_at(&directory, at + 20)?, u32_at(&directory, at + 24)?);
        let (name_len, extra_len, comment_len, local) =
            (u16_at(&directory, at + 28)?, u16_at(&directory, at + 30)?, u16_at(&directory, at + 32)?, u32_at(&directory, at + 42)?);
        if directory.get(at + 46..at + 46 + name_len) == Some(name) {
            if packed > MOST || unpacked > MOST {
                return Err("zip entry too large".into());
            }
            let header = read_at(file, local as u64, 30)?;
            let data_at = local as u64 + 30 + u16_at(&header, 26)? as u64 + u16_at(&header, 28)? as u64;
            let data = read_at(file, data_at, packed)?;
            let out = match method {
                0 => data,
                8 => inflate(&data, MOST)?,
                _ => return Err(format!("zip method {method}")),
            };
            return if out.len() != unpacked {
                Err("zip entry of the wrong size".into())
            } else if crc32(&out) as usize != crc {
                Err("zip entry damaged: its CRC does not match".into())
            } else {
                Ok(out)
            };
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    Err(format!("no {} in the zip file", String::from_utf8_lossy(name)))
}

/// CRC-32 as zip files use it (ISO 3309, reflected).
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

// ---------- inflate (RFC 1951) ----------

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    held: u32,
    count: u32,
}

impl Bits<'_> {
    fn take(&mut self, n: u32) -> Result<u32> {
        while self.count < n {
            let byte = *self.data.get(self.at).ok_or("deflate stream cut short")?;
            self.at += 1;
            self.held |= u32::from(byte) << self.count;
            self.count += 8;
        }
        let value = self.held & ((1u64 << n) - 1) as u32;
        self.held = if n == 32 { 0 } else { self.held >> n };
        self.count -= n;
        Ok(value)
    }
}

/// A canonical Huffman code: how many codes each length has, and the
/// symbols in code order.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for l in 1..15 {
            offsets[l + 1] = offsets[l] + counts[l];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offsets[l as usize] as usize] = symbol as u16;
                offsets[l as usize] += 1;
            }
        }
        Self { counts, symbols }
    }

    fn decode(&self, bits: &mut Bits) -> Result<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..16 {
            code |= bits.take(1)? as i32;
            let count = i32::from(self.counts[length]);
            if code - count < first {
                return self.symbols.get((index + code - first) as usize).copied().ok_or_else(|| "bad deflate code".to_string());
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("bad deflate code".into())
    }
}

const LENGTH_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DISTANCE_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DISTANCE_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The data a raw deflate stream holds, at most `most` bytes.
pub fn inflate(data: &[u8], most: usize) -> Result<Vec<u8>> {
    let mut bits = Bits { data, at: 0, held: 0, count: 0 };
    let mut out = Vec::new();
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => {
                bits.held = 0;
                bits.count = 0;
                let header = data.get(bits.at..bits.at + 4).ok_or("deflate stream cut short")?;
                let len = u16::from_le_bytes([header[0], header[1]]) as usize;
                if len != !u16::from_le_bytes([header[2], header[3]]) as usize {
                    return Err("bad stored block".into());
                }
                let block = data.get(bits.at + 4..bits.at + 4 + len).ok_or("deflate stream cut short")?;
                out.extend_from_slice(block);
                bits.at += 4 + len;
            }
            1 => {
                let mut lengths = [0u8; 288];
                lengths[..144].fill(8);
                lengths[144..256].fill(9);
                lengths[256..280].fill(7);
                lengths[280..].fill(8);
                codes(&mut bits, &mut out, &Huffman::new(&lengths), &Huffman::new(&[5; 30]), most)?;
            }
            2 => {
                let (literals, distances, lengths) = (bits.take(5)? as usize + 257, bits.take(5)? as usize + 1, bits.take(4)? as usize + 4);
                let mut code_lengths = [0u8; 19];
                for &i in &ORDER[..lengths] {
                    code_lengths[i] = bits.take(3)? as u8;
                }
                let code = Huffman::new(&code_lengths);
                let mut all = Vec::with_capacity(literals + distances);
                while all.len() < literals + distances {
                    let (repeat, value) = match code.decode(&mut bits)? {
                        symbol @ 0..=15 => (1, symbol as u8),
                        16 => (3 + bits.take(2)?, *all.last().ok_or("bad deflate lengths")?),
                        17 => (3 + bits.take(3)?, 0),
                        _ => (11 + bits.take(7)?, 0),
                    };
                    all.extend(std::iter::repeat_n(value, repeat as usize));
                }
                if all.len() != literals + distances {
                    return Err("bad deflate lengths".into());
                }
                codes(&mut bits, &mut out, &Huffman::new(&all[..literals]), &Huffman::new(&all[literals..]), most)?;
            }
            _ => return Err("bad deflate block".into()),
        }
        if out.len() > most {
            return Err("deflated data too large".into());
        }
        if last {
            return Ok(out);
        }
    }
}

/// One block of literals and copies, up to its end code.
fn codes(bits: &mut Bits, out: &mut Vec<u8>, literals: &Huffman, distances: &Huffman, most: usize) -> Result<()> {
    loop {
        let symbol = literals.decode(bits)? as usize;
        match symbol {
            0..=255 => out.push(symbol as u8),
            256 => return Ok(()),
            _ => {
                let i = symbol - 257;
                let len = *LENGTH_BASE.get(i).ok_or("bad deflate length")? as usize + bits.take(u32::from(LENGTH_EXTRA[i]))? as usize;
                let d = distances.decode(bits)? as usize;
                let distance = *DISTANCE_BASE.get(d).ok_or("bad deflate distance")? as usize + bits.take(u32::from(DISTANCE_EXTRA[d]))? as usize;
                let from = out.len().checked_sub(distance).ok_or("deflate distance too far back")?;
                for k in 0..len {
                    out.push(out[from + k]);
                }
            }
        }
        if out.len() > most {
            return Err("deflated data too large".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A zip file holding one entry, `name`, as `data` with `method`.
    fn zip(name: &[u8], method: u16, crc: u32, data: &[u8], unpacked: usize) -> Vec<u8> {
        let fields = |out: &mut Vec<u8>| {
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]); // time and date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(unpacked as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
        };
        let mut out = b"PK\x03\x04".to_vec();
        fields(&mut out);
        out.extend_from_slice(name);
        out.extend_from_slice(data);
        let directory_at = out.len();
        out.extend_from_slice(b"PK\x01\x02");
        out.extend_from_slice(&20u16.to_le_bytes()); // version made by
        fields(&mut out);
        out.extend_from_slice(&[0; 10]); // comment, disk, attributes
        out.extend_from_slice(&0u32.to_le_bytes()); // the local header
        out.extend_from_slice(name);
        let directory_len = out.len() - directory_at;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0; 4]); // disks
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&(directory_len as u32).to_le_bytes());
        out.extend_from_slice(&(directory_at as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment
        out
    }

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn inflates_stored_fixed_and_dynamic_blocks() {
        let short = b"EU4txt\ndate=1444.11.11\nplayer=\"FRA\"\n";
        let fixed = bytes("730d3529a928e14a492c49b535343131d133340422ae829cc4cad4225b25b72047252e00");
        assert_eq!(inflate(&fixed, MOST).unwrap(), short);
        let stored = bytes("012400dbff4555347478740a646174653d313434342e31312e31310a706c617965723d22465241220a");
        assert_eq!(inflate(&stored, MOST).unwrap(), short);
        let (long, dynamic) = dynamic();
        assert_eq!(inflate(&dynamic, MOST).unwrap(), long);
        assert!(inflate(&dynamic, 100).is_err());
        assert!(inflate(&fixed[..10], MOST).is_err());
    }

    /// The text the dynamic block test data holds, and the data.
    fn dynamic() -> (Vec<u8>, Vec<u8>) {
        let long: String = (0..300).map(|i| format!("province={i} owner=\"FRA\" development={}\n", i * 7 % 31)).collect();
        (long.into_bytes(), bytes(include_str!("save_file/dynamic.hex").trim()))
    }

    /// What `f` makes of `data`, which must not panic.
    fn calmly<T>(what: &str, data: &[u8], f: impl Fn(&[u8]) -> T) -> T {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(data))).unwrap_or_else(|_| panic!("{what} panicked"))
    }

    #[test]
    fn a_damaged_deflate_stream_fails_cleanly() {
        let (long, dynamic) = dynamic();
        // Every bit flipped: no panic, and what comes out is bounded.
        for at in 0..dynamic.len() {
            for bit in 0..8 {
                let mut data = dynamic.clone();
                data[at] ^= 1 << bit;
                if let Ok(out) = calmly(&format!("inflating with bit {bit} of byte {at} flipped"), &data, |d| inflate(d, MOST)) {
                    assert!(out.len() <= MOST);
                }
            }
        }
        // Cut short anywhere: an error.
        for len in 0..dynamic.len() {
            let out = calmly(&format!("inflating the first {len} bytes"), &dynamic[..len], |d| inflate(d, MOST));
            assert!(out.is_err(), "the first {len} bytes inflated");
        }
        // And within a smaller bound.
        assert!(calmly("inflating into too little", &dynamic, |d| inflate(d, long.len() - 1)).is_err());
    }

    #[test]
    fn a_damaged_zip_file_fails_cleanly() {
        let (long, dynamic) = dynamic();
        let file = zip(b"meta", 8, crc32(&long), &dynamic, long.len());
        assert_eq!(zip_entry(&mut Cursor::new(&file), b"meta").unwrap(), long);
        let entry = |d: &[u8]| zip_entry(&mut Cursor::new(d), b"meta");
        // Every bit flipped, in the headers and in the data: an error, or,
        // where the bit does not matter (a byte of a header the reader does
        // not use, the padding after the last block), the entry unchanged.
        for at in 0..file.len() {
            for bit in 0..8 {
                let mut data = file.clone();
                data[at] ^= 1 << bit;
                if let Ok(out) = calmly(&format!("reading with bit {bit} of byte {at} flipped"), &data, entry) {
                    assert_eq!(out, long, "bit {bit} of byte {at} flipped");
                }
            }
        }
        // Cut short anywhere: an error.
        for len in 0..file.len() {
            assert!(calmly(&format!("reading the first {len} bytes"), &file[..len], entry).is_err(), "the first {len} bytes read");
        }
        // Broken headers: the end record's directory beyond the file or too
        // large, more entries than the directory holds (the one sought not
        // among them), a local header beyond the file, an entry too large, a
        // method the reader lacks.
        let end = file.len() - 22;
        let directory = end - (46 + 4);
        let broken = |patches: &[(usize, &[u8])]| {
            let mut data = file.clone();
            for &(at, value) in patches {
                data[at..at + value.len()].copy_from_slice(value);
            }
            data
        };
        for (what, data) in [
            ("a directory beyond the file", broken(&[(end + 16, &u32::MAX.to_le_bytes())])),
            ("a directory too large", broken(&[(end + 12, &(MOST as u32 + 1).to_le_bytes())])),
            ("more entries than there are", broken(&[(end + 10, &u16::MAX.to_le_bytes()), (directory + 46, b"mets")])),
            ("a local header beyond the file", broken(&[(directory + 42, &u32::MAX.to_le_bytes())])),
            ("an entry too large", broken(&[(directory + 24, &u32::MAX.to_le_bytes())])),
            ("a name running past the directory", broken(&[(directory + 28, &u16::MAX.to_le_bytes())])),
            ("a method the reader lacks", broken(&[(directory + 10, &12u16.to_le_bytes())])),
            ("a stored entry that is deflated", broken(&[(directory + 10, &0u16.to_le_bytes())])),
            ("no zip at all", b"PK\x03\x04".to_vec()),
            ("nothing", Vec::new()),
        ] {
            assert!(calmly(what, &data, entry).is_err(), "{what} was read");
        }
    }

    #[test]
    fn a_zip_entry_is_checked_against_its_crc() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        let meta = b"EU4txt\ndate=1444.11.11\nplayer=\"FRA\"\n";
        let mut file = zip(b"meta", 0, crc32(meta), meta, meta.len());
        assert_eq!(zip_entry(&mut Cursor::new(&file), b"meta").unwrap(), meta);
        let at = file.windows(6).position(|w| w == b"EU4txt").unwrap();
        file[at + 6] = b'X';
        assert!(zip_entry(&mut Cursor::new(&file), b"meta").unwrap_err().contains("CRC"));
    }

    #[test]
    fn a_header_says_the_player_and_the_date() {
        let text = b"EU4txt\ndate=1470.3.13\nsave_game=\".eu4\"\nplayer=\"CAS\"\ndisplayed_country_name=\"x\"\n";
        let date = eu4_harmony_remote::Date::from_ymd(1470, 3, 13).unwrap().0;
        assert_eq!(parse(text).unwrap(), Header { player: Some(*b"CAS"), date: Some(date), mods: Vec::new() });
        // Binary, as an ironman save's meta begins.
        let mut binary = b"EU4bin".to_vec();
        binary.extend_from_slice(&[0x4d, 0x28, 0x01, 0x00, 0x0c, 0x00]);
        binary.extend_from_slice(&0x0360_d9f8i32.to_le_bytes());
        binary.extend_from_slice(&[0x38, 0x2a, 0x01, 0x00, 0x0f, 0x00, 0x03, 0x00]);
        binary.extend_from_slice(b"CAS");
        assert_eq!(parse(&binary).unwrap(), Header { player: Some(*b"CAS"), date: Some(0x0360_d9f8), mods: Vec::new() });
        assert_eq!(parse(b"EU4txt\ndate=1444.11.11\nplayer=\"---\"\n").unwrap().player, None);
        assert!(parse(b"PK\x03\x04").is_err());
    }

    fn save_mod(file: &str, name: &str) -> SaveMod {
        SaveMod { file: file.into(), name: name.into() }
    }

    #[test]
    fn a_header_says_the_mods_the_save_was_made_with() {
        // As the game writes them; a name with braces and Chinese.
        let text = "EU4txt\ndate=1445.7.19\nplayer=\"WUR\"\nmods_enabled_names={\n\t{\n\t\tfilename=\"mod/ugc_3047072888.mod\"\n\t\tname=\"彼岸 {乐园}\"\n\t}\n {\n\t\tfilename=\"mod/ugc_2976470733.mod\"\n\t\tname=\"Chinese Language Mod for 1.37\"\n\t}\n  \n}\nmulti_player=no\n";
        let mods = vec![save_mod("mod/ugc_3047072888.mod", "彼岸 {乐园}"), save_mod("mod/ugc_2976470733.mod", "Chinese Language Mod for 1.37")];
        assert_eq!(parse(text.as_bytes()).unwrap().mods, mods);
        assert_eq!(parse(b"EU4txt\nmods_enabled_names={\n}\n").unwrap().mods, []);
        assert_eq!(parse(b"EU4txt\nplayer=\"FRA\"\n").unwrap().mods, []);

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
