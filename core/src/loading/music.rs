//! Music the game knows at startup without reading it (docs/loading.md).
//!
//! For each music asset, `0x16a4930` adds the music and calls `0x16a9a40`
//! with its sound: that opens the file, reads all of it into memory, opens it
//! with Vorbis, keeps the channels, the rate, the length in samples and the
//! duration in nanoseconds in the sound (`+0xd8`, `+0xdc`, `+0xc8`,
//! `+0xd0`), and the caller lets the rest go: of the four, the songs' list
//! reads the duration for the music player, and a song's play takes all four
//! again. The DLC's music is 3 GB deflated in zips, which every
//! start inflated whole for those four numbers (docs/research/loading.md).
//!
//! The framework keeps the four numbers of each file in the user folder, by
//! the place the game reads the file from (PhysFS's real directory, a zip or a
//! folder) and the file's name, with what tells the file as it is: for an
//! entry of a zip, its CRC-32 and sizes from the zip's directory at its end;
//! for a file in a folder, its size and time. The next start fills the
//! numbers in from there while the file is as it was, and the game reads only
//! what is new or changed.
use crate::points::{GameFunction, Point};
use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

/// In `0x16a4930`, after the music is added: `mov dl, 1; lea rcx, [rbx +
/// 0x10]; call 0x16a9a40`, the sound read whole; then `mov rdi, rax` and the
/// decoder let go when there is one.
const READ_WHOLE: usize = 0x16a4c62;
const READ_WHOLE_NEXT: usize = 0x16a4c6d;
const READ: GameFunction<2> = GameFunction::at(0x16a9a40);
/// The sound's name, which the game opens the file by, and what it keeps.
const SOUND_NAME: usize = 0x40;
const SOUND_SAMPLES: usize = 0xc8;
const SOUND_NANOSECONDS: usize = 0xd0;
const SOUND_CHANNELS: usize = 0xd8;
const SOUND_RATE: usize = 0xdc;

/// Where the numbers are kept, in the framework's folder of the user folder.
const CACHE: &str = "music.txt";

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(READ_WHOLE, "b2 01 48 8d 4b 10 e8 d3 4d 00 00", "music read whole at startup", read_whole_stub as *const () as usize, &READ_WHOLE_RESUME),
        Point::keep(READ, "48 89 5c 24 08 48 89 74 24 20 55 57 41 54 41 56", "music read"),
        Point::keep(READ_WHOLE_NEXT, "48 8b f8 48 85 c0 74 15 48 8b c8 e8 03 1e 16 00", "music's decoder let go"),
        Point::keep(SOUND_NAME_USE, "48 8d 56 40", "music opened by its name").uses(&[SOUND_NAME]),
        // What the read keeps: the channels as 1 for mono and 2 otherwise,
        // the rate, the samples and the duration.
        Point::keep(
            KEPT,
            "ba ff ff ff ff e8 bf ce 15 00 48 85 c0 74 5c 41 8b cc 83 78 04 01 0f 95 c1 ff c1 89 8e d8 00 00 00 8b 40 08 89 86 dc 00 00 00 ba ff \
             ff ff ff 48 8b cf e8 62 d9 15 00 48 89 86 c8 00 00 00 ba ff ff ff ff 48 8b cf e8 be e3 15 00 f2 0f 59 05 26 7b 71 00 f2 48 0f 2c c0 \
             48 89 86 d0 00 00 00",
            "music's numbers kept",
        )
        .uses(&[SOUND_CHANNELS, SOUND_RATE, SOUND_SAMPLES, SOUND_NANOSECONDS]),
    ]
}

/// In `0x16a9a40`: `lea rdx, [rsi + 0x40]` for the open by name, and where
/// the numbers are kept.
const SOUND_NAME_USE: usize = 0x16a9acd;
const KEPT: usize = 0x16a9cd7;

crate::resume!(READ_WHOLE_RESUME: READ_WHOLE_AT = READ_WHOLE_NEXT);

/// Replaces the read: `rax` is the decoder to let go, 0 for none.
#[unsafe(naked)]
extern "C" fn read_whole_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("lea rcx, [rbx + 0x10]", call read_whole, "mov rax, [rsp + 0x48]"),
        "jmp qword ptr [rip + {next}]",
        read_whole = sym read_whole,
        next = sym READ_WHOLE_AT,
    );
}

static FOLDER: OnceLock<PathBuf> = OnceLock::new();

pub fn set_folder(user_dir: &Path) {
    let _ = FOLDER.set(user_dir.join("eu4-harmony"));
}

/// What tells a music file as it is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Print {
    /// An entry of a zip: its CRC-32 and its sizes, packed and not.
    Entry { crc: u32, packed: u64, unpacked: u64 },
    /// A file in a folder: its size and the time it was last written.
    File { size: u64, time: u64 },
}

/// What the game kept of one file, and the file as it was.
#[derive(Clone, Debug, PartialEq)]
struct Kept {
    print: Print,
    samples: u64,
    nanoseconds: u64,
    channels: u32,
    rate: u32,
}

/// The numbers kept, by the place and the file's name.
static KEPT_MUSIC: Mutex<Option<HashMap<(String, String), Kept>>> = Mutex::new(None);

extern "C" fn read_whole(sound: usize) -> usize {
    let m = crate::hooked_memory();
    let read = || READ.call(&m, [sound, 1]).unwrap_or(0);
    crate::guarded("music", || {
        let name = unsafe { CStr::from_ptr((sound + SOUND_NAME) as *const c_char) };
        if super::game_loads() {
            return read();
        }
        let Some((place, print)) = place_of(name) else { return read() };
        let key = (place, name.to_string_lossy().into_owned());
        let mut all = KEPT_MUSIC.lock().unwrap_or_else(|e| e.into_inner());
        let kept = all.get_or_insert_with(load);
        if let Some(k) = kept.get(&key).filter(|k| k.print == print).cloned() {
            if !super::checking() {
                put(sound, &k);
                return 0;
            }
            drop(all);
            let decoder = read();
            let game = taken(sound, print);
            if game.as_ref() != Some(&k) {
                log!("music: check-loading: {:?} kept as {k:?}, read as {game:?}", key.1);
            }
            return decoder;
        }
        drop(all);
        let decoder = read();
        if decoder != 0 {
            if let Some(k) = taken(sound, print) {
                let mut all = KEPT_MUSIC.lock().unwrap_or_else(|e| e.into_inner());
                let kept = all.get_or_insert_with(HashMap::new);
                kept.insert(key, k);
                save(kept);
            }
        }
        decoder
    })
}

/// Where the game reads the file from, and what tells the file as it is.
fn place_of(name: &CStr) -> Option<(String, Print)> {
    let place = crate::physfs::real_dir(name.as_ptr())?;
    let path = Path::new(&place);
    let print = if path.is_dir() {
        let meta = std::fs::metadata(path.join(name.to_str().ok()?)).ok()?;
        let time = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos() as u64;
        Print::File { size: meta.len(), time }
    } else {
        entry_print(&*directory(&place)?, name.to_bytes())?
    };
    Some((place, print))
}

/// The entry of that name, or of that name in another case when none has it
/// as it is.
fn entry_print(entries: &HashMap<Vec<u8>, Print>, name: &[u8]) -> Option<Print> {
    entries.get(name).or_else(|| entries.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, p)| p)).copied()
}

/// Each zip's entries by their names, read from its directory once a process.
type Directory = Arc<HashMap<Vec<u8>, Print>>;
static DIRECTORIES: Mutex<Option<HashMap<String, Option<Directory>>>> = Mutex::new(None);
/// A zip directory larger than this is not read.
const MOST_DIRECTORY: usize = 64 << 20;

fn directory(zip: &str) -> Option<Directory> {
    let mut all = DIRECTORIES.lock().unwrap_or_else(|e| e.into_inner());
    all.get_or_insert_with(HashMap::new)
        .entry(zip.to_string())
        .or_insert_with(|| {
            let read = std::fs::File::open(zip).map_err(|e| e.to_string()).and_then(|mut f| crate::saves::file::zip_directory(&mut f, MOST_DIRECTORY));
            match read {
                Ok(entries) => Some(Arc::new(prints(entries))),
                Err(e) => {
                    log!("music: the directory of {zip} cannot be read: {e}");
                    None
                }
            }
        })
        .clone()
}

fn prints(entries: Vec<crate::saves::file::ZipEntry>) -> HashMap<Vec<u8>, Print> {
    entries.into_iter().map(|e| (e.name, Print::Entry { crc: e.crc, packed: e.packed as u64, unpacked: e.unpacked as u64 })).collect()
}

fn put(sound: usize, k: &Kept) {
    unsafe {
        *((sound + SOUND_SAMPLES) as *mut u64) = k.samples;
        *((sound + SOUND_NANOSECONDS) as *mut u64) = k.nanoseconds;
        *((sound + SOUND_CHANNELS) as *mut u32) = k.channels;
        *((sound + SOUND_RATE) as *mut u32) = k.rate;
    }
}

fn taken(sound: usize, print: Print) -> Option<Kept> {
    let m = crate::hooked_memory();
    Some(Kept {
        print,
        samples: m.u64(sound + SOUND_SAMPLES).ok()?,
        nanoseconds: m.u64(sound + SOUND_NANOSECONDS).ok()?,
        channels: m.u32(sound + SOUND_CHANNELS).ok()?,
        rate: m.u32(sound + SOUND_RATE).ok()?,
    })
}

fn file() -> Option<PathBuf> {
    FOLDER.get().map(|f| f.join(CACHE))
}

fn load() -> HashMap<(String, String), Kept> {
    file().and_then(|f| std::fs::read_to_string(f).ok()).map(|t| parse(&t)).unwrap_or_default()
}

fn save(kept: &HashMap<(String, String), Kept>) {
    let Some(path) = file() else { return };
    if let Err(e) = std::fs::write(&path, render(kept)) {
        log!("music: {} cannot be written: {e}", path.display());
    }
}

/// One line a file, apart by tabs: what tells the file (`entry` with its CRC
/// and sizes, or `file` with its size and time), the four numbers, the place
/// and the name.
fn render(kept: &HashMap<(String, String), Kept>) -> String {
    let mut lines: Vec<String> = kept
        .iter()
        .map(|((place, name), k)| {
            let print = match k.print {
                Print::Entry { crc, packed, unpacked } => format!("entry\t{crc:08x}\t{packed}\t{unpacked}"),
                Print::File { size, time } => format!("file\t{size}\t{time}\t-"),
            };
            format!("{print}\t{}\t{}\t{}\t{}\t{place}\t{name}", k.samples, k.nanoseconds, k.channels, k.rate)
        })
        .collect();
    lines.sort();
    lines.join("\n") + "\n"
}

fn parse(text: &str) -> HashMap<(String, String), Kept> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [kind, a, b, c, samples, nanoseconds, channels, rate, place, name] = f[..] else { return None };
            let print = match kind {
                "entry" => Print::Entry { crc: u32::from_str_radix(a, 16).ok()?, packed: b.parse().ok()?, unpacked: c.parse().ok()? },
                "file" => Print::File { size: a.parse().ok()?, time: b.parse().ok()? },
                _ => return None,
            };
            Some((
                (place.to_string(), name.to_string()),
                Kept {
                    print,
                    samples: samples.parse().ok()?,
                    nanoseconds: nanoseconds.parse().ok()?,
                    channels: channels.parse().ok()?,
                    rate: rate.parse().ok()?,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_kept_reads_back_as_written() {
        let mut kept = HashMap::new();
        let k = Kept {
            print: Print::Entry { crc: 0x0badcafe, packed: 34_279_611, unpacked: 34_679_865 },
            samples: 7_654_321,
            nanoseconds: 173_567_000_000,
            channels: 2,
            rate: 44_100,
        };
        kept.insert((r"E:\games\eu4\dlc\dlc125_lessons_japan\dlc125.zip".to_string(), "music/japan 1.ogg".to_string()), k.clone());
        kept.insert(
            ("E:\\x".to_string(), "music/b.ogg".to_string()),
            Kept { print: Print::File { size: 10, time: 1_700_000_000_123_456_789 }, channels: 1, ..k },
        );
        assert_eq!(parse(&render(&kept)), kept);
        // A line not of the form, as one of an older form, is left out, not
        // the rest.
        let older = "10236285\t1730731614759244500\t11863488\t269013333333\t2\t44100\tE:/dlc014.zip\tmusic/pdxmascarol.ogg\n";
        assert_eq!(parse(&format!("{older}broken line\n{}", render(&kept))).len(), 2);
    }

    #[test]
    fn an_entry_is_told_by_its_name_in_any_case() {
        let entry = |crc| crate::saves::file::ZipEntry { name: Vec::new(), method: 8, crc, packed: 5, unpacked: 9, local: 0 };
        let entries = prints(vec![
            crate::saves::file::ZipEntry { name: b"music/MainTheme.ogg".to_vec(), ..entry(1) },
            crate::saves::file::ZipEntry { name: b"music/other.ogg".to_vec(), ..entry(2) },
        ]);
        assert_eq!(entry_print(&entries, b"music/MainTheme.ogg"), Some(Print::Entry { crc: 1, packed: 5, unpacked: 9 }));
        assert_eq!(entry_print(&entries, b"music/maintheme.ogg"), Some(Print::Entry { crc: 1, packed: 5, unpacked: 9 }));
        assert_eq!(entry_print(&entries, b"music/missing.ogg"), None);
    }
}
