//! Zip files, as the game writes its compressed saves and the DLC their
//! music, and as the framework writes its own files (`crate::files`): the
//! central directory at the end, each entry stored or deflated, checked
//! against its CRC-32.
//!
//! Every reader and writer of zip files in the framework is this one: the
//! continue box's save header (`crate::saves::file`), the saves compressed
//! in the background (`crate::saving::background`), the framework files and
//! the music cache (`crate::loading::music`).
use crate::Result;
use std::io::{Read, Seek, SeekFrom};

/// An entry's method: its data as it is.
pub const STORED: u16 = 0;
/// An entry's method: its data deflated (RFC 1951).
pub const DEFLATED: u16 = 8;

/// An entry of a zip file, as its central directory has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: Vec<u8>,
    pub method: u16,
    /// Its general purpose flags; bit 0 is an encrypted entry.
    pub flags: u16,
    /// When it was last changed, in MS-DOS's form.
    pub time: u16,
    pub date: u16,
    pub crc: u32,
    /// Its size in the file, and its size unpacked.
    pub packed: usize,
    pub unpacked: usize,
    /// Its external attributes.
    pub attributes: u32,
    /// Where its local header is.
    pub local: u64,
}

impl Entry {
    /// An entry `name` storing `data` as it is; what [`write`] needs of it.
    pub fn stored(name: &[u8], data: &[u8]) -> Self {
        Self {
            name: name.to_vec(),
            method: STORED,
            flags: 0,
            time: 0,
            // 1980-01-01, the first day MS-DOS's dates have.
            date: 0x21,
            crc: crc32(data),
            packed: data.len(),
            unpacked: data.len(),
            attributes: 0,
            local: 0,
        }
    }
}

/// The local header's signature, with which a zip file begins.
pub const LOCAL: &[u8; 4] = b"PK\x03\x04";
const DIRECTORY: &[u8; 4] = b"PK\x01\x02";
const END: &[u8; 4] = b"PK\x05\x06";
const ZIP64_END: &[u8; 4] = b"PK\x06\x06";
const ZIP64_LOCATOR: &[u8; 4] = b"PK\x06\x07";
const DESCRIPTOR: &[u8; 4] = b"PK\x07\x08";
/// The flag of an entry whose CRC and sizes follow its data.
const DATA_DESCRIPTOR: u16 = 1 << 3;
/// The end record, 22 bytes and a comment of at most 65535: the most of the
/// file's end searched for it.
const END_SEARCH: u64 = 22 + 0xffff;

fn u16_at(data: &[u8], at: usize) -> Result<u16> {
    data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| "zip record cut short".to_string())
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "zip record cut short".to_string())
}

fn u64_at(data: &[u8], at: usize) -> Result<u64> {
    let bytes = data.get(at..at + 8).ok_or("zip record cut short")?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_at(file: &mut (impl Read + Seek), at: u64, len: usize) -> Result<Vec<u8>> {
    let mut out = vec![0; len];
    file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    file.read_exact(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// The entries of the zip file `file`, from its central directory, when
/// that is no larger than `most` bytes.
pub fn directory(file: &mut (impl Read + Seek), most: usize) -> Result<Vec<Entry>> {
    let size = file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let tail_len = size.min(END_SEARCH) as usize;
    let tail = read_at(file, size - tail_len as u64, tail_len)?;
    let end = tail.windows(4).rposition(|w| w == END).ok_or("no zip directory")?;
    // The end record is 22 bytes, its comment's length last.
    u16_at(&tail, end + 20)?;
    let (mut entries, mut directory_len, mut directory_at) =
        (u64::from(u16_at(&tail, end + 10)?), u64::from(u32_at(&tail, end + 12)?), u64::from(u32_at(&tail, end + 16)?));
    if entries == u64::from(u16::MAX) || directory_len == u64::from(u32::MAX) || directory_at == u64::from(u32::MAX) {
        let end_at = size - tail_len as u64 + end as u64;
        let locator_at = end_at.checked_sub(20).ok_or("no ZIP64 locator")?;
        let locator = read_at(file, locator_at, 20)?;
        if locator.get(..4) != Some(ZIP64_LOCATOR) || u32_at(&locator, 4)? != 0 || u32_at(&locator, 16)? != 1 {
            return Err("bad ZIP64 locator".into());
        }
        let record_at = u64_at(&locator, 8)?;
        if record_at.checked_add(56).is_none_or(|end| end > locator_at) {
            return Err("ZIP64 end record out of bounds".into());
        }
        let record = read_at(file, record_at, 56)?;
        if record.get(..4) != Some(ZIP64_END) || u64_at(&record, 4)? < 44 || u32_at(&record, 16)? != 0 || u32_at(&record, 20)? != 0 {
            return Err("bad ZIP64 end record".into());
        }
        entries = u64_at(&record, 32)?;
        directory_len = u64_at(&record, 40)?;
        directory_at = u64_at(&record, 48)?;
    }
    if directory_len > most as u64 || directory_at.checked_add(directory_len).is_none_or(|end| end > size) || entries > directory_len / 46 {
        return Err("zip directory out of bounds".into());
    }
    let directory = read_at(file, directory_at, directory_len as usize)?;
    let mut out = Vec::with_capacity(entries as usize);
    let mut at = 0;
    for _ in 0..entries {
        if directory.get(at..at + 4) != Some(DIRECTORY) {
            return Err("bad zip directory entry".into());
        }
        let (name_len, extra_len, comment_len) =
            (u16_at(&directory, at + 28)? as usize, u16_at(&directory, at + 30)? as usize, u16_at(&directory, at + 32)? as usize);
        let extra_at = at + 46 + name_len;
        let extra = directory.get(extra_at..extra_at + extra_len).ok_or("zip record cut short")?;
        let mut packed = u64::from(u32_at(&directory, at + 20)?);
        let mut unpacked = u64::from(u32_at(&directory, at + 24)?);
        let mut local = u64::from(u32_at(&directory, at + 42)?);
        if [packed, unpacked, local].contains(&u64::from(u32::MAX)) {
            let mut field = 0;
            let values = loop {
                let id = u16_at(extra, field)?;
                let len = usize::from(u16_at(extra, field + 2)?);
                let value = extra.get(field + 4..field + 4 + len).ok_or("zip extra field cut short")?;
                if id == 1 {
                    break value;
                }
                field += 4 + len;
            };
            let mut value = 0;
            for size in [&mut unpacked, &mut packed, &mut local] {
                if *size == u64::from(u32::MAX) {
                    *size = u64_at(values, value)?;
                    value += 8;
                }
            }
        }
        out.push(Entry {
            name: directory.get(at + 46..extra_at).ok_or("zip record cut short")?.to_vec(),
            flags: u16_at(&directory, at + 8)?,
            method: u16_at(&directory, at + 10)?,
            time: u16_at(&directory, at + 12)?,
            date: u16_at(&directory, at + 14)?,
            crc: u32_at(&directory, at + 16)?,
            packed: usize::try_from(packed).map_err(|_| "zip entry too large for this platform")?,
            unpacked: usize::try_from(unpacked).map_err(|_| "zip entry too large for this platform")?,
            attributes: u32_at(&directory, at + 38)?,
            local,
        });
        at = extra_at + extra_len + comment_len;
        if at > directory.len() {
            return Err("zip record cut short".into());
        }
    }
    Ok(out)
}

/// The data of `entry` as the file holds it: stored, or deflated.
pub fn packed(file: &mut (impl Read + Seek), entry: &Entry) -> Result<Vec<u8>> {
    let header = read_at(file, entry.local, 30)?;
    if header.get(..4) != Some(LOCAL) {
        return Err("bad zip local header".into());
    }
    let data_at = entry.local + 30 + u64::from(u16_at(&header, 26)?) + u64::from(u16_at(&header, 28)?);
    read_at(file, data_at, entry.packed)
}

/// What `entry` holds, from its data as the file holds it (`packed`),
/// checked against its size and its CRC; an entry larger than `most` bytes,
/// packed or not, is refused.
pub fn unpack(entry: &Entry, packed: &[u8], most: usize) -> Result<Vec<u8>> {
    let name = String::from_utf8_lossy(&entry.name);
    if entry.flags & 1 != 0 {
        return Err(format!("{name}: an encrypted zip entry"));
    }
    if entry.packed > most || entry.unpacked > most {
        return Err(format!("{name}: zip entry too large"));
    }
    let data = match entry.method {
        STORED => packed.to_vec(),
        DEFLATED => inflate(packed, entry.unpacked)?,
        method => return Err(format!("{name}: zip method {method}")),
    };
    if data.len() != entry.unpacked {
        Err(format!("{name}: zip entry of the wrong size"))
    } else if crc32(&data) != entry.crc {
        Err(format!("{name} damaged: its CRC does not match"))
    } else {
        Ok(data)
    }
}

/// What `entry` of `file` holds, checked; see [`unpack`].
pub fn read(file: &mut (impl Read + Seek), entry: &Entry, most: usize) -> Result<Vec<u8>> {
    if entry.packed > most {
        return Err(format!("{}: zip entry too large", String::from_utf8_lossy(&entry.name)));
    }
    let data = packed(file, entry)?;
    unpack(entry, &data, most)
}

/// What the entry `name` of `file` holds, found through its central
/// directory, checked; the directory and the entry at most `most` bytes.
pub fn find(file: &mut (impl Read + Seek), name: &[u8], most: usize) -> Result<Vec<u8>> {
    let entry = directory(file, most)?.into_iter().find(|e| e.name == name).ok_or_else(|| format!("no {} in the zip file", String::from_utf8_lossy(name)))?;
    read(file, &entry, most)
}

/// A zip file of `entries`, each with its data as the file is to hold it, in
/// their order. The entries' sizes in the file and local headers are those
/// of the data given; the rest of each is written as it is, but for the flag
/// of a data descriptor after the data (bit 3), which is never written: the
/// headers hold the CRC and the sizes.
pub fn write<'a>(entries: impl IntoIterator<Item = (&'a Entry, &'a [u8])>) -> Vec<u8> {
    let entries: Vec<_> = entries.into_iter().collect();
    let mut out = Vec::with_capacity(entries.iter().map(|(e, d)| d.len() + 2 * e.name.len() + 128).sum());
    let mut directory = Vec::new();
    // Version needed, flags, method, time, date, CRC, sizes, name length,
    // extra length: the fields both headers have.
    let common = |out: &mut Vec<u8>, e: &Entry, packed: usize| {
        for v in [20u16, e.flags & !DATA_DESCRIPTOR, e.method, e.time, e.date] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.crc, packed as u32, e.unpacked as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
    };
    for &(e, data) in &entries {
        let local = out.len() as u32;
        out.extend_from_slice(LOCAL);
        common(&mut out, e, data.len());
        out.extend_from_slice(&e.name);
        out.extend_from_slice(data);
        directory.extend_from_slice(DIRECTORY);
        // Version made by.
        directory.extend_from_slice(&20u16.to_le_bytes());
        common(&mut directory, e, data.len());
        // Comment length, disk, internal attributes.
        for v in [0u16, 0, 0] {
            directory.extend_from_slice(&v.to_le_bytes());
        }
        directory.extend_from_slice(&e.attributes.to_le_bytes());
        directory.extend_from_slice(&local.to_le_bytes());
        directory.extend_from_slice(&e.name);
    }
    let at = out.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(END);
    for v in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    // Comment length.
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

mod stream;
pub use stream::Stream;

/// The data a raw deflate stream holds, at most `most` bytes.
///
/// It is inflated in one go into a buffer of `most` bytes, which callers
/// give as the size the entry says it has: faster than a buffer that grows,
/// for a save's few kilobytes of `meta` and its tens of megabytes of
/// `gamestate` alike. No more is set aside than the stream could hold,
/// deflate's 1032 bytes for each of its own, whatever `most` says.
pub fn inflate(data: &[u8], most: usize) -> Result<Vec<u8>> {
    use miniz_oxide::inflate::core::{decompress, inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF, DecompressorOxide};
    use miniz_oxide::inflate::TINFLStatus;
    let mut out = vec![0; most.min(data.len().saturating_mul(1032).saturating_add(258))];
    let mut state = Box::<DecompressorOxide>::default();
    let (status, _, written) = decompress(&mut state, data, &mut out, 0, TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF);
    match status {
        TINFLStatus::Done => {
            out.truncate(written);
            Ok(out)
        }
        TINFLStatus::HasMoreOutput => Err("deflated data too large".into()),
        TINFLStatus::FailedCannotMakeProgress | TINFLStatus::NeedsMoreInput => Err("deflate stream cut short".into()),
        _ => Err("bad deflate stream".into()),
    }
}

/// CRC-32 as zip files use it (ISO 3309, reflected).
pub fn crc32(data: &[u8]) -> u32 {
    !crc32_update(!0, data)
}

/// Eight tables of the CRC's remainders, the first of a byte, each next of a
/// byte eight bits further: what lets [`crc32_update`] take eight bytes at a
/// step.
const CRC_TABLES: [[u32; 256]; 8] = {
    let mut tables = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
            bit += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }
    let mut t = 1;
    while t < 8 {
        let mut i = 0;
        while i < 256 {
            let before = tables[t - 1][i];
            tables[t][i] = (before >> 8) ^ tables[0][(before & 0xff) as usize];
            i += 1;
        }
        t += 1;
    }
    tables
};

/// The CRC's register after `data`, from the register `crc` (`!0` at the
/// start; the CRC is the last register inverted).
pub fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    let t = &CRC_TABLES;
    let mut eights = data.chunks_exact(8);
    for c in &mut eights {
        let low = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) ^ crc;
        let high = u32::from_le_bytes([c[4], c[5], c[6], c[7]]);
        crc = t[7][(low & 0xff) as usize]
            ^ t[6][(low >> 8 & 0xff) as usize]
            ^ t[5][(low >> 16 & 0xff) as usize]
            ^ t[4][(low >> 24) as usize]
            ^ t[3][(high & 0xff) as usize]
            ^ t[2][(high >> 8 & 0xff) as usize]
            ^ t[1][(high >> 16 & 0xff) as usize]
            ^ t[0][(high >> 24) as usize];
    }
    for &byte in eights.remainder() {
        crc = t[0][((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    crc
}

/// The CRC of `a` followed by `b`, from the CRC of each and the length of
/// `b`: `a`'s CRC moved past as many zero bytes, then `b`'s added.
pub fn crc32_combine(a: u32, b: u32, b_len: usize) -> u32 {
    // A zero bit more moves the register by one step of the polynomial: as a
    // matrix over GF(2), `odd`. Its squares move it by 2, 4, 8... bits.
    let mut odd = [0u32; 32];
    odd[0] = 0xedb8_8320;
    for (i, row) in odd.iter_mut().enumerate().skip(1) {
        *row = 1 << (i - 1);
    }
    let mut even = gf2_square(&odd);
    odd = gf2_square(&even);
    let (mut a, mut len) = (a, b_len);
    while len != 0 {
        // A byte's zeros the first time, then twice as many each time.
        even = gf2_square(&odd);
        if len & 1 != 0 {
            a = gf2_times(&even, a);
        }
        len >>= 1;
        if len == 0 {
            break;
        }
        odd = gf2_square(&even);
        if len & 1 != 0 {
            a = gf2_times(&odd, a);
        }
        len >>= 1;
    }
    a ^ b
}

fn gf2_times(matrix: &[u32; 32], mut vector: u32) -> u32 {
    let mut sum = 0;
    let mut row = 0;
    while vector != 0 {
        if vector & 1 != 0 {
            sum ^= matrix[row];
        }
        vector >>= 1;
        row += 1;
    }
    sum
}

fn gf2_square(matrix: &[u32; 32]) -> [u32; 32] {
    std::array::from_fn(|i| gf2_times(matrix, matrix[i]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_streaming_writer_matches_the_existing_writer_without_retaining_payloads() {
        let a = Entry::stored(b"a.txt", b"first");
        let b = Entry::stored(b"b.txt", b"second");
        let mut stream = Stream::new(Vec::new());
        stream.entry(&a, b"first").unwrap();
        stream.entry(&b, b"second").unwrap();
        let bytes = stream.finish().unwrap();
        assert_eq!(bytes, write([(&a, b"first".as_slice()), (&b, b"second".as_slice())]));
        let mut file = std::io::Cursor::new(bytes);
        let entries = directory(&mut file, 4096).unwrap();
        assert_eq!(read(&mut file, &entries[1], 4096).unwrap(), b"second");
    }

    #[test]
    fn the_streaming_writer_reports_an_output_error() {
        let mut bytes = [0u8; 8];
        let mut stream = Stream::new(std::io::Cursor::new(bytes.as_mut_slice()));
        assert!(stream.entry(&Entry::stored(b"a", b"data"), b"data").is_err());
    }
    use std::io::Cursor;

    const MOST: usize = 1 << 20;

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    }

    /// The text the dynamic block test data holds, and the data.
    fn dynamic() -> (Vec<u8>, Vec<u8>) {
        let long: String = (0..300).map(|i| format!("province={i} owner=\"FRA\" development={}\n", i * 7 % 31)).collect();
        (long.into_bytes(), bytes(include_str!("zip/dynamic.hex").trim()))
    }

    /// What `f` makes of `data`, which must not panic.
    fn calmly<T>(what: &str, data: &[u8], f: impl Fn(&[u8]) -> T) -> T {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(data))).unwrap_or_else(|_| panic!("{what} panicked"))
    }

    /// A zip file of the one entry `name`, deflated.
    fn deflated(name: &[u8], data: &[u8]) -> Vec<u8> {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let entry = Entry { method: DEFLATED, packed: packed.len(), ..Entry::stored(name, data) };
        write([(&entry, packed.as_slice())])
    }

    #[test]
    fn entries_written_read_back_through_the_directory() {
        let (long, _) = dynamic();
        let a = Entry { time: 0x6a2b, date: 0x5b64, attributes: 0x20, ..Entry::stored(b"interface/a.gfx", b"bitmapfonts = {}") };
        let b = Entry::stored(b"gfx/fonts/b.tga", &[0; 50]);
        let packed = miniz_oxide::deflate::compress_to_vec(&long, 3);
        let c = Entry { method: DEFLATED, packed: packed.len(), ..Entry::stored(b"gamestate", &long) };
        let file = write([(&a, &b"bitmapfonts = {}"[..]), (&b, &[0; 50][..]), (&c, packed.as_slice())]);
        let read = directory(&mut Cursor::new(&file), MOST).unwrap();
        assert_eq!(read.len(), 3);
        for (written, read) in [&a, &b, &c].into_iter().zip(&read) {
            assert_eq!(Entry { local: 0, ..read.clone() }, *written);
        }
        assert_eq!(find(&mut Cursor::new(&file), b"interface/a.gfx", MOST).unwrap(), b"bitmapfonts = {}");
        assert_eq!(find(&mut Cursor::new(&file), b"gfx/fonts/b.tga", MOST).unwrap(), [0; 50]);
        assert_eq!(find(&mut Cursor::new(&file), b"gamestate", MOST).unwrap(), long);
        assert_eq!(packed_of(&file, &read[2]), packed);
        assert!(find(&mut Cursor::new(&file), b"c", MOST).is_err());
        // A data descriptor's flag read is not written.
        let d = Entry { flags: DATA_DESCRIPTOR, ..b.clone() };
        assert_eq!(directory(&mut Cursor::new(write([(&d, &[0; 50][..])])), MOST).unwrap()[0].flags, 0);
        // An empty archive.
        assert_eq!(directory(&mut Cursor::new(write([])), MOST).unwrap(), []);
    }

    fn packed_of(file: &[u8], entry: &Entry) -> Vec<u8> {
        super::packed(&mut Cursor::new(file), entry).unwrap()
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
        assert_eq!(inflate(&dynamic, long.len()).unwrap(), long);
        assert!(inflate(&dynamic, 100).is_err());
        assert!(inflate(&fixed[..10], MOST).is_err());
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
        let (long, _) = dynamic();
        let file = deflated(b"meta", &long);
        assert_eq!(find(&mut Cursor::new(&file), b"meta", MOST).unwrap(), long);
        let entry = |d: &[u8]| find(&mut Cursor::new(d), b"meta", MOST);
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
        // method the reader lacks, an encrypted entry.
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
            ("an encrypted entry", broken(&[(directory + 8, &1u16.to_le_bytes())])),
            ("no zip at all", LOCAL.to_vec()),
            ("nothing", Vec::new()),
        ] {
            assert!(calmly(what, &data, entry).is_err(), "{what} was read");
        }
    }

    #[test]
    fn an_entry_is_checked_against_its_crc() {
        let meta = b"EU4txt\ndate=1444.11.11\nplayer=\"FRA\"\n";
        let mut file = write([(&Entry::stored(b"meta", meta), &meta[..])]);
        assert_eq!(find(&mut Cursor::new(&file), b"meta", MOST).unwrap(), meta);
        let at = file.windows(6).position(|w| w == b"EU4txt").unwrap();
        file[at + 6] = b'X';
        assert!(find(&mut Cursor::new(&file), b"meta", MOST).unwrap_err().contains("CRC"));
    }

    #[test]
    fn the_crc_eight_bytes_at_a_step_is_the_crc_a_bit_at_a_time() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
        let bitwise = |data: &[u8]| {
            let mut crc = !0u32;
            for &byte in data {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
                }
            }
            !crc
        };
        let data: Vec<u8> = (0u32..10_007).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        for len in [0, 1, 7, 8, 9, 63, 64, 65, 10_007] {
            assert_eq!(crc32(&data[..len]), bitwise(&data[..len]), "{len} bytes");
        }
        // In pieces, carrying the register.
        let (a, b) = data.split_at(1_001);
        assert_eq!(!crc32_update(crc32_update(!0, a), b), crc32(&data));
    }

    #[test]
    fn two_crcs_combine_into_the_crc_of_both() {
        let data: Vec<u8> = (0u32..70_001).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect();
        for at in [0, 1, 8, 1_000, 65_536, 70_000, 70_001] {
            let (a, b) = data.split_at(at);
            assert_eq!(crc32_combine(crc32(a), crc32(b), b.len()), crc32(&data), "split at {at}");
        }
    }
}
