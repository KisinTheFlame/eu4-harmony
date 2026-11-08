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
    let entries: Vec<_> = files.iter().map(|f| crate::zip::Entry::stored(f.path.as_bytes(), &f.data)).collect();
    crate::zip::write(entries.iter().zip(files).map(|(e, f)| (e, f.data.as_slice())))
}

pub fn points() -> Vec<Point> {
    vec![Point::keep(c"PHYSFS_mountMemory", "48 89 6c 24 18 56 57 41 56 48 83 ec 20 49 8b f9", "PhysFS mount from memory").recorded(MOUNT_MEMORY.slot())]
}

#[cfg(test)]
mod tests {
    use super::{zip, File};
    use std::io::Cursor;

    #[test]
    fn files_come_back_out_of_the_archive() {
        let files =
            vec![File { path: "interface/a.gfx".into(), data: b"bitmapfonts = {}".to_vec() }, File { path: "gfx/fonts/b.tga".into(), data: vec![0; 50] }];
        let archive = zip(&files);
        let find = |path: &str| crate::zip::find(&mut Cursor::new(&archive), path.as_bytes(), archive.len());
        assert_eq!(find("interface/a.gfx").unwrap(), b"bitmapfonts = {}");
        assert_eq!(find("gfx/fonts/b.tga").unwrap(), [0u8; 50]);
        assert!(find("c").is_err());
    }
}
