//! The framework's own files (ADR framework-files-in-the-game-file-system):
//! files the game reads as it reads its own and the mods', such as the
//! definition and the font files of each framework font. They are made in
//! memory at startup, packed as a zip archive, and mounted into the game's file
//! system, PhysFS, as the game initializes it, after every place already there.
//! Their names are the framework's own, under `eu4harmony` names no game or mod
//! file has, so they add files and never stand in for one.
use crate::points::{Function, Point};
use std::ffi::{c_char, c_void};

type MountMemoryFn = unsafe extern "C" fn(*const c_void, u64, Option<unsafe extern "C" fn(*mut c_void)>, *const c_char, *const c_char, i32) -> i32;

static MOUNT_MEMORY: Function<MountMemoryFn> = Function::new();

/// One file: its path in the game's file system and its bytes.
pub struct File {
    pub path: String,
    pub data: Vec<u8>,
}

/// Every framework file.
fn all() -> Vec<File> {
    crate::unicode::framework_fonts::files()
}

/// Mounts the framework's files: once PhysFS is initialized, before the
/// game reads its interface.
pub fn mount() {
    let files = all();
    let archive = zip(&files);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    // PhysFS reads the archive for as long as it is mounted, which is as long
    // as the game runs.
    let archive: &'static [u8] = Box::leak(archive.into_boxed_slice());
    let mounted = unsafe { MOUNT_MEMORY.get()(archive.as_ptr().cast(), archive.len() as u64, None, c"eu4harmony.zip".as_ptr(), std::ptr::null(), 1) };
    if mounted == 0 {
        log!("framework files: not mounted; the game reads none of {paths:?}");
    } else {
        log!("framework files: mounted {} bytes: {paths:?}", archive.len());
    }
}

/// The files packed as a zip archive, each stored as it is.
fn zip(files: &[File]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for file in files {
        let offset = out.len() as u32;
        let (crc, size, name) = (crc32(&file.data), file.data.len() as u32, file.path.as_bytes());
        // version needed, flags, method (stored), time, date, crc, sizes,
        // name length, extra length
        let fields = |out: &mut Vec<u8>| {
            for half in [10u16, 0, 0, 0, 0x21] {
                out.extend(half.to_le_bytes());
            }
            for word in [crc, size, size] {
                out.extend(word.to_le_bytes());
            }
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
        };
        out.extend(0x0403_4b50u32.to_le_bytes());
        fields(&mut out);
        out.extend(name);
        out.extend(&file.data);
        directory.extend(0x0201_4b50u32.to_le_bytes());
        directory.extend(10u16.to_le_bytes());
        fields(&mut directory);
        // comment length, disk, internal and external attributes, offset
        for half in [0u16, 0, 0] {
            directory.extend(half.to_le_bytes());
        }
        directory.extend(0u32.to_le_bytes());
        directory.extend(offset.to_le_bytes());
        directory.extend(name);
    }
    let (start, size) = (out.len() as u32, directory.len() as u32);
    out.extend(directory);
    out.extend(0x0605_4b50u32.to_le_bytes());
    for half in [0u16, 0, files.len() as u16, files.len() as u16] {
        out.extend(half.to_le_bytes());
    }
    out.extend(size.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { crc >> 1 ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

pub fn points() -> Vec<Point> {
    vec![Point::keep(c"PHYSFS_mountMemory", "48 89 6c 24 18 56 57 41 56 48 83 ec 20 49 8b f9", "PhysFS mount from memory").recorded(MOUNT_MEMORY.slot())]
}

/// For tests: a file of the archive, by its path, as a zip reader finds it
/// through the central directory.
#[cfg(test)]
fn unzip<'a>(archive: &'a [u8], path: &str) -> crate::Result<&'a [u8]> {
    let u16_at = |at: usize| u16::from_le_bytes([archive[at], archive[at + 1]]) as usize;
    let u32_at = |at: usize| u32::from_le_bytes(archive[at..at + 4].try_into().unwrap()) as usize;
    let end = archive.len() - 22;
    if u32_at(end) != 0x0605_4b50 {
        return Err("no end of central directory".into());
    }
    let mut at = u32_at(end + 16);
    for _ in 0..u16_at(end + 10) {
        let (size, name_len, local) = (u32_at(at + 24), u16_at(at + 28), u32_at(at + 42));
        if &archive[at + 46..at + 46 + name_len] == path.as_bytes() {
            let data = local + 30 + u16_at(local + 26) + u16_at(local + 28);
            let found = &archive[data..data + size];
            return if crc32(found) as usize == u32_at(at + 16) { Ok(found) } else { Err("bad crc".into()) };
        }
        at += 46 + name_len + u16_at(at + 30) + u16_at(at + 32);
    }
    Err(format!("{path} not found"))
}

#[cfg(test)]
mod tests {
    use super::{crc32, unzip, zip, File};

    #[test]
    fn crc_is_zip_s() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn files_come_back_out_of_the_archive() {
        let files =
            vec![File { path: "interface/a.gfx".into(), data: b"bitmapfonts = {}".to_vec() }, File { path: "gfx/fonts/b.tga".into(), data: vec![0; 50] }];
        let archive = zip(&files);
        assert_eq!(unzip(&archive, "interface/a.gfx").unwrap(), b"bitmapfonts = {}");
        assert_eq!(unzip(&archive, "gfx/fonts/b.tga").unwrap(), &[0u8; 50][..]);
        assert!(unzip(&archive, "c").is_err());
    }
}
