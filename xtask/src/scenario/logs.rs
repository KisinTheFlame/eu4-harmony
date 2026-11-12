//! The lines a test game's logs gained while a scenario played. The shared
//! logger retains current, previous and two older segments. Find the marked
//! segment by its unique first line, then follow every newer segment. A mark
//! evicted by the bounded history cannot recover the overwritten lines.
use eu4_harmony_folders::UserFolder;
use std::path::{Path, PathBuf};

/// Lines of the framework's log that tell of a panic or a fatal error
/// (`log_panics` and `fatal` in core/src/lib.rs).
const TROUBLES: [&str; 2] = [" panic on thread ", " fatal error, ending the game: "];

/// Where a log stood at a moment: the first line of that start's log, and
/// how long the log was.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    first: Option<String>,
    length: usize,
    emergency: Option<Box<Mark>>,
}

impl Mark {
    /// Where the framework's log of the test game in `user` stands now.
    pub fn of(user: &UserFolder) -> Self {
        let mut mark = Self::of_file(&user.core_log());
        mark.emergency = Some(Box::new(Self::of_file(&user.logs().join("core-emergency.log"))));
        mark
    }

    /// Where the log `path` stands now.
    pub fn of_file(path: &Path) -> Self {
        let text = read(path);
        Self { first: text.lines().next().map(String::from), length: text.len(), emergency: None }
    }

    /// The lines of the framework's log since the mark that tell of a panic
    /// or a fatal error.
    pub fn troubles(&self, user: &UserFolder) -> Vec<String> {
        let mut lines = self.since(&user.core_log());
        if let Some(mark) = &self.emergency {
            lines.extend(mark.since(&user.logs().join("core-emergency.log")));
        }
        lines.into_iter().filter(|line| TROUBLES.iter().any(|t| line.contains(t))).collect()
    }

    /// The lines the log `path` gained since the mark.
    pub fn since(&self, path: &Path) -> Vec<String> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("log");
        let texts = [path.with_extension(format!("3.{ext}")), path.with_extension(format!("2.{ext}")), previous(path), path.to_path_buf()].map(|p| read(&p));
        let first = |text: &str| text.lines().next().map(String::from);
        let after = |text: &str| text.get(self.length..).unwrap_or_default().lines().map(String::from).collect::<Vec<_>>();
        let all = |text: &str| text.lines().map(String::from).collect::<Vec<_>>();
        if let Some(at) = texts.iter().position(|text| self.first.is_some() && first(text) == self.first) {
            let mut lines = after(&texts[at]);
            for text in &texts[at + 1..] {
                lines.extend(all(text));
            }
            return lines;
        }
        let mut lines = all(&texts[2]);
        lines.extend(all(&texts[3]));
        lines
    }
}

/// The log kept from the start before: `core.log`'s is `core.prev.log`.
fn previous(path: &Path) -> PathBuf {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("log");
    path.with_extension(format!("prev.{ext}"))
}

fn read(path: &Path) -> String {
    std::fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("eu4-harmony-logs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("eu4-harmony\\logs")).unwrap();
            Self(dir)
        }

        fn log(&self, name: &str, text: &str) {
            std::fs::write(self.0.join("eu4-harmony\\logs").join(name), text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_lines_since_a_mark_are_found_in_the_same_start_s_log() {
        let s = Scratch::new("same");
        let user = UserFolder::new(&s.0);
        s.log("core.log", "10:00 start\n10:01 panic on thread main (1): before\n");
        let mark = Mark::of(&user);
        s.log("core.log", "10:00 start\n10:01 panic on thread main (1): before\n10:02 a line\n10:03 fatal error, ending the game: boom\n");
        assert_eq!(mark.since(&user.core_log()), ["10:02 a line", "10:03 fatal error, ending the game: boom"]);
        assert_eq!(mark.troubles(&user), ["10:03 fatal error, ending the game: boom"]);
    }

    #[test]
    fn a_start_since_the_mark_keeps_the_earlier_log_s_lines_after_it_and_all_its_own() {
        let s = Scratch::new("restart");
        let user = UserFolder::new(&s.0);
        s.log("core.log", "10:00 start\n10:01 old\n");
        let mark = Mark::of(&user);
        s.log("core.prev.log", "10:00 start\n10:01 old\n10:02 new in the first\n");
        s.log("core.log", "11:00 start again\n11:01 panic on thread main (1): a panic\n");
        assert_eq!(mark.since(&user.core_log()), ["10:02 new in the first", "11:00 start again", "11:01 panic on thread main (1): a panic"]);
        assert_eq!(mark.troubles(&user).len(), 1);
    }

    #[test]
    fn a_new_emergency_panic_is_found_but_an_old_one_is_not_reported_again() {
        let s = Scratch::new("emergency");
        let user = UserFolder::new(&s.0);
        s.log("core.log", "start\n");
        s.log("core-emergency.log", "emergency start\nold panic on thread (1): before\n");
        let mark = Mark::of(&user);
        s.log("core-emergency.log", "emergency start\nold panic on thread (1): before\nnew panic on thread (2): after\n");
        assert_eq!(mark.troubles(&user), ["new panic on thread (2): after"]);
    }

    #[test]
    fn a_size_rollover_follows_the_mark_through_multiple_older_segments() {
        let s = Scratch::new("segments");
        let user = UserFolder::new(&s.0);
        s.log("core.log", "segment one\nold\n");
        let mark = Mark::of(&user);
        s.log("core.2.log", "segment one\nold\nnew before rotation\n");
        s.log("core.prev.log", "segment two\nnext\n");
        s.log("core.log", "segment three\nlast\n");
        assert_eq!(mark.since(&user.core_log()), ["new before rotation", "segment two", "next", "segment three", "last"]);
    }

    #[test]
    fn a_log_that_was_not_there_counts_from_its_start() {
        let s = Scratch::new("none");
        let user = UserFolder::new(&s.0);
        let mark = Mark::of(&user);
        s.log("core.log", "10:00 start\n");
        assert_eq!(mark.since(&user.core_log()), ["10:00 start"]);
        assert!(mark.troubles(&user).is_empty());
    }
}
