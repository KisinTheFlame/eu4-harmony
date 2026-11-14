//! The background compression's course: a local save under way on the main
//! thread, its entries copied at level 0, the files it moved collected, and
//! the hand-over to the worker that compresses them afterwards (`super`).
//!
//! The hooks tell the course their moments — a save beginning and ending, an
//! entry about to be deflated, a file moved — and the course decides whether
//! the entry is copied, which files are collected, and what is handed to the
//! worker when the save ends. The worker and the file system are reached
//! through [`Game`]: the real thread and folder in [`Real`], and a recorder
//! in the tests. The zip compaction itself stays the deep module it is
//! (`super::compact`).
use super::{COPY_LEVEL, DEFLATED, GAME_LEVEL};
use eu4_harmony_folders::SAVE_GAMES;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// The worker and the file system, as the course reaches them.
pub(super) trait Game {
    /// Removes the temporary files a compression the process's end cut short
    /// left in `dir`; how many.
    fn remove_leftovers(&mut self, dir: &Path) -> usize;
    /// Hands a file to the worker, which compresses it in the background.
    fn hand_over(&mut self, path: PathBuf);
    /// Returns once every file handed to the worker is done with.
    fn wait(&mut self);
    /// What the course did, for the log.
    fn note(&mut self, message: String);
}

/// A local save under way, and what it has moved into place.
pub(super) struct Course {
    /// A local save is under way on the main thread.
    saving: bool,
    began: Option<Instant>,
    /// The files the save under way moved into place, by their paths in the
    /// folder the game writes to.
    moved: Vec<String>,
    /// The folder the game writes to: the user folder.
    folder: Option<PathBuf>,
}

impl Course {
    pub(super) const fn new() -> Self {
        Self { saving: false, began: None, moved: Vec::new(), folder: None }
    }

    /// Where the game writes its files, once the user folder is known; the
    /// temporary files a compression the process's end cut short left are
    /// removed.
    fn set_folder(&mut self, g: &mut impl Game, user_dir: &Path) {
        self.folder = Some(user_dir.to_path_buf());
        let removed = g.remove_leftovers(&user_dir.join(SAVE_GAMES));
        if removed > 0 {
            g.note(format!("saves: removed {removed} temporary file(s) left by a compression the process's end cut short"));
        }
    }

    /// A local save begins: the worker is waited for first, so it never puts
    /// an older file over one the game wrote since, and the save is under way
    /// only if it runs on the main thread.
    fn save_begins(&mut self, g: &mut impl Game, on_main_thread: bool) {
        g.wait();
        self.moved.clear();
        self.saving = on_main_thread;
        self.began = Some(Instant::now());
    }

    /// The level an entry about to be deflated is copied at, while a local
    /// save on the main thread deflates it at the game's level; None to leave
    /// it as the game asks.
    fn copy_level(&self, on_main_thread: bool, method: u32, level: u32) -> Option<usize> {
        (self.saving && on_main_thread && method == DEFLATED as u32 && level == GAME_LEVEL as u32).then_some(COPY_LEVEL)
    }

    /// A file the save moved into place, collected for the worker; a path
    /// that is not UTF-8 stays uncompressed.
    fn file_moved(&mut self, g: &mut impl Game, to: Option<String>) {
        if !self.saving {
            return;
        }
        match to {
            Some(to) => self.moved.push(to),
            None => g.note("saves: a file moved to a path that is not UTF-8 stays uncompressed".into()),
        }
    }

    /// The local save ends: how long it stopped the game, and each file it
    /// moved handed to the worker.
    fn save_ends(&mut self, g: &mut impl Game) {
        if !std::mem::take(&mut self.saving) {
            return;
        }
        if let Some(began) = self.began.take() {
            g.note(format!("saves: a local save stopped the game for {:.0} ms", began.elapsed().as_secs_f64() * 1000.0));
        }
        let moved = std::mem::take(&mut self.moved);
        let Some(folder) = &self.folder else { return };
        for path in moved {
            g.hand_over(folder.join(path));
        }
    }
}

/// The course the hooks tell their moments to.
static COURSE: Mutex<Course> = Mutex::new(Course::new());

/// Where the game writes its files (`super::set_folder`).
pub fn set_folder(user_dir: &Path) {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).set_folder(&mut Real, user_dir);
}

/// A local save begins (`super::save_before`).
pub(super) fn save_begins(on_main_thread: bool) {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).save_begins(&mut Real, on_main_thread);
}

/// The level an entry about to be deflated is copied at (`super::entry_before`).
pub(super) fn copy_level(on_main_thread: bool, method: u32, level: u32) -> Option<usize> {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).copy_level(on_main_thread, method, level)
}

/// A file the save moved into place (`super::moved`).
pub(super) fn file_moved(to: Option<String>) {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).file_moved(&mut Real, to);
}

/// The local save ends (`super::save_after`).
pub(super) fn save_ends() {
    COURSE.lock().unwrap_or_else(|e| e.into_inner()).save_ends(&mut Real);
}

/// The real worker and file system: the thread of `super` and the folder it
/// writes to.
pub(super) struct Real;

impl Game for Real {
    fn remove_leftovers(&mut self, dir: &Path) -> usize {
        super::remove_leftovers(dir)
    }

    fn hand_over(&mut self, path: PathBuf) {
        super::hand_over(path);
    }

    fn wait(&mut self) {
        super::wait();
    }

    fn note(&mut self, message: String) {
        log!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::STORED;

    /// The worker and the file system as the tests tell them: what was handed
    /// over, waited for, removed and logged, in order.
    #[derive(Default)]
    struct Fake {
        removed: Vec<PathBuf>,
        removes: usize,
        handed: Vec<PathBuf>,
        waits: usize,
        notes: Vec<String>,
    }

    impl Game for Fake {
        fn remove_leftovers(&mut self, dir: &Path) -> usize {
            self.removed.push(dir.to_path_buf());
            self.removes
        }

        fn hand_over(&mut self, path: PathBuf) {
            self.handed.push(path);
        }

        fn wait(&mut self) {
            self.waits += 1;
        }

        fn note(&mut self, message: String) {
            self.notes.push(message);
        }
    }

    impl Fake {
        fn said(&self, part: &str) -> bool {
            self.notes.iter().any(|n| n.contains(part))
        }
    }

    #[test]
    fn a_save_hands_its_moved_files_to_the_worker() {
        let mut g = Fake { removes: 2, ..Default::default() };
        let mut c = Course::new();
        c.set_folder(&mut g, Path::new("user"));
        assert_eq!(g.removed, vec![Path::new("user").join(SAVE_GAMES)]);
        assert!(g.said("removed 2 temporary"));
        c.save_begins(&mut g, true);
        assert_eq!(g.waits, 1);
        assert_eq!(c.copy_level(true, DEFLATED as u32, GAME_LEVEL as u32), Some(COPY_LEVEL));
        c.file_moved(&mut g, Some("autosave.eu4".into()));
        c.file_moved(&mut g, Some("Austria.eu4".into()));
        c.save_ends(&mut g);
        assert_eq!(g.handed, vec![Path::new("user").join("autosave.eu4"), Path::new("user").join("Austria.eu4")]);
        assert!(g.said("a local save stopped the game for"));
        // The next save waits for the worker first.
        c.save_begins(&mut g, true);
        assert_eq!(g.waits, 2);
    }

    #[test]
    fn only_a_save_on_the_main_thread_copies_and_collects() {
        let mut g = Fake::default();
        let mut c = Course::new();
        c.set_folder(&mut g, Path::new("user"));
        c.save_begins(&mut g, false);
        assert_eq!(c.copy_level(true, DEFLATED as u32, GAME_LEVEL as u32), None);
        c.file_moved(&mut g, Some("autosave.eu4".into()));
        c.save_ends(&mut g);
        assert!(g.handed.is_empty());
    }

    #[test]
    fn the_copy_is_only_for_the_games_deflate_at_its_level() {
        let mut g = Fake::default();
        let mut c = Course::new();
        c.save_begins(&mut g, true);
        assert_eq!(c.copy_level(true, DEFLATED as u32, GAME_LEVEL as u32), Some(COPY_LEVEL));
        assert_eq!(c.copy_level(false, DEFLATED as u32, GAME_LEVEL as u32), None);
        assert_eq!(c.copy_level(true, STORED as u32, GAME_LEVEL as u32), None);
        assert_eq!(c.copy_level(true, DEFLATED as u32, 6), None);
    }

    #[test]
    fn a_moved_path_that_is_not_utf8_stays_uncompressed() {
        let mut g = Fake::default();
        let mut c = Course::new();
        c.save_begins(&mut g, true);
        c.file_moved(&mut g, None);
        assert!(g.said("not UTF-8"));
        c.save_ends(&mut g);
        assert!(g.handed.is_empty());
    }

    #[test]
    fn a_save_with_no_folder_hands_nothing_over() {
        let mut g = Fake::default();
        let mut c = Course::new();
        c.save_begins(&mut g, true);
        c.file_moved(&mut g, Some("autosave.eu4".into()));
        c.save_ends(&mut g);
        assert!(g.handed.is_empty());
    }

    #[test]
    fn files_moved_before_a_save_are_not_collected() {
        let mut g = Fake::default();
        let mut c = Course::new();
        c.file_moved(&mut g, Some("stray.eu4".into()));
        c.save_begins(&mut g, true);
        c.save_ends(&mut g);
        assert!(g.handed.is_empty());
    }

    #[test]
    fn an_end_without_a_begin_hands_nothing_over() {
        let mut g = Fake::default();
        let mut c = Course::new();
        c.set_folder(&mut g, Path::new("user"));
        c.save_ends(&mut g);
        assert!(g.handed.is_empty());
        assert!(g.notes.is_empty());
    }

    #[test]
    fn each_save_collects_its_own_moved_files() {
        let mut g = Fake::default();
        let mut c = Course::new();
        c.set_folder(&mut g, Path::new("user"));
        c.save_begins(&mut g, true);
        c.file_moved(&mut g, Some("first.eu4".into()));
        c.save_ends(&mut g);
        c.save_begins(&mut g, true);
        c.file_moved(&mut g, Some("second.eu4".into()));
        c.save_ends(&mut g);
        assert_eq!(g.handed, vec![Path::new("user").join("first.eu4"), Path::new("user").join("second.eu4")]);
    }
}
