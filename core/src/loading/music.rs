//! Music the game knows at startup without reading it (docs/loading.md).
//!
//! For each music asset, `0x16a4930` adds the music and calls `0x16a9a40`
//! with its sound: that opens the file, reads all of it into memory, opens it
//! with Vorbis, keeps the channels, the rate, the length in samples and the
//! duration in nanoseconds in the sound (`+0xd8`, `+0xdc`, `+0xc8`, `+0xd0`), and the caller
//! lets the rest go. The DLC's music is 3 GB deflated in zips, which every
//! start inflated whole for those four numbers (docs/research/loading.md).
//!
//! The framework keeps the four numbers of each file in the user folder, with
//! the place the game reads the file from (PhysFS's real directory, a zip or a
//! folder) and that place's size and time; the next start fills them in from
//! there while the place is as it was, and the game reads only what is new.
use crate::points::Point;
use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

/// In `0x16a4930`, after the music is added: `mov dl, 1; lea rcx, [rbx +
/// 0x10]; call 0x16a9a40`, the sound read whole; then `mov rdi, rax` and the
/// decoder let go when there is one.
const READ_WHOLE: usize = 0x16a4c62;
const READ_WHOLE_NEXT: usize = 0x16a4c6d;
const READ: usize = 0x16a9a40;
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

/// What the game kept of one file, and the place it read it from as it was.
#[derive(Clone, Debug, PartialEq)]
struct Kept {
    place_size: u64,
    place_time: u64,
    samples: u64,
    nanoseconds: u64,
    channels: u32,
    rate: u32,
}

/// The numbers kept, by the place and the file's name.
static KEPT_MUSIC: Mutex<Option<HashMap<(String, String), Kept>>> = Mutex::new(None);

extern "C" fn read_whole(sound: usize) -> usize {
    let m = crate::hooked_memory();
    let read = || m.call(m.at(READ), &[sound, 1]).unwrap_or(0);
    crate::guarded("music", || {
        let name = unsafe { CStr::from_ptr((sound + SOUND_NAME) as *const c_char) };
        if super::game_loads() {
            return read();
        }
        let Some((place, size, time)) = place_of(name) else { return read() };
        let key = (place, name.to_string_lossy().into_owned());
        let mut all = KEPT_MUSIC.lock().unwrap_or_else(|e| e.into_inner());
        let kept = all.get_or_insert_with(load);
        if let Some(k) = kept.get(&key).filter(|k| k.place_size == size && k.place_time == time).cloned() {
            if !super::checking() {
                put(sound, &k);
                return 0;
            }
            drop(all);
            let decoder = read();
            let game = taken(sound, size, time);
            if game.as_ref() != Some(&k) {
                log!("music: check-loading: {:?} kept as {k:?}, read as {game:?}", key.1);
            }
            return decoder;
        }
        drop(all);
        let decoder = read();
        if decoder != 0 {
            if let Some(k) = taken(sound, size, time) {
                let mut all = KEPT_MUSIC.lock().unwrap_or_else(|e| e.into_inner());
                let kept = all.get_or_insert_with(HashMap::new);
                kept.insert(key, k);
                save(kept);
            }
        }
        decoder
    })
}

/// Where the game reads the file from, and that place's size and time: a
/// zip's for a file in one, the file's own in a folder.
fn place_of(name: &CStr) -> Option<(String, u64, u64)> {
    let place = crate::physfs::real_dir(name.as_ptr())?;
    let path = Path::new(&place);
    let meta = if path.is_dir() { std::fs::metadata(path.join(name.to_str().ok()?)) } else { std::fs::metadata(path) }.ok()?;
    let time = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos() as u64;
    Some((place, meta.len(), time))
}

fn put(sound: usize, k: &Kept) {
    unsafe {
        *((sound + SOUND_SAMPLES) as *mut u64) = k.samples;
        *((sound + SOUND_NANOSECONDS) as *mut u64) = k.nanoseconds;
        *((sound + SOUND_CHANNELS) as *mut u32) = k.channels;
        *((sound + SOUND_RATE) as *mut u32) = k.rate;
    }
}

fn taken(sound: usize, place_size: u64, place_time: u64) -> Option<Kept> {
    let m = crate::hooked_memory();
    Some(Kept {
        place_size,
        place_time,
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

/// One line a file: the place's size and time, the four numbers, the place
/// and the name, apart by tabs.
fn render(kept: &HashMap<(String, String), Kept>) -> String {
    let mut lines: Vec<String> = kept
        .iter()
        .map(|((place, name), k)| format!("{}\t{}\t{}\t{}\t{}\t{}\t{place}\t{name}", k.place_size, k.place_time, k.samples, k.nanoseconds, k.channels, k.rate))
        .collect();
    lines.sort();
    lines.join("\n") + "\n"
}

fn parse(text: &str) -> HashMap<(String, String), Kept> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [size, time, samples, nanoseconds, channels, rate, place, name] = f[..] else { return None };
            Some((
                (place.to_string(), name.to_string()),
                Kept {
                    place_size: size.parse().ok()?,
                    place_time: time.parse().ok()?,
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
            place_size: 795_376_489,
            place_time: 1_700_000_000_123_456_789,
            samples: 7_654_321,
            nanoseconds: 173_567_000_000,
            channels: 2,
            rate: 44_100,
        };
        kept.insert((r"E:\games\eu4\dlc\dlc125_lessons_japan\dlc125.zip".to_string(), "music/japan 1.ogg".to_string()), k.clone());
        kept.insert(("E:\\x".to_string(), "music/b.ogg".to_string()), Kept { channels: 1, ..k });
        assert_eq!(parse(&render(&kept)), kept);
        // A line not of the form is left out, not the rest.
        assert_eq!(parse(&format!("broken line\n{}", render(&kept))).len(), 2);
    }
}
