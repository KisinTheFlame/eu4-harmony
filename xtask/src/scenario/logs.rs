//! The lines a test game's logs gained while a scenario played, delegating
//! segment order, header recognition, source filtering and trouble matching to
//! `eu4_harmony_logging`.
use eu4_harmony_folders::UserFolder;
use eu4_harmony_logging::{extension_source, troubles_since, Artifact, SourceMark, CORE, CORE_EMERGENCY};
use std::path::Path;

/// Where a log stood at a moment, across its segments and any fallback stems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mark {
    source: SourceMark,
    emergency: Option<SourceMark>,
}

impl Mark {
    /// Where the framework's core and emergency logs of `user` stand now.
    pub fn of(user: &UserFolder) -> Self {
        let dir = user.logs();
        Self { source: SourceMark::of(&dir, CORE), emergency: Some(SourceMark::of(&dir, CORE_EMERGENCY)) }
    }

    /// Where the extension `id`'s journal in `user` stands now.
    pub fn of_extension(user: &UserFolder, id: &str) -> Self {
        Self { source: SourceMark::of(&user.logs(), &extension_source(id)), emergency: None }
    }

    /// The lines of the framework's core and emergency logs since the mark that
    /// tell of a panic, a fatal error, or a `crash:` record.
    pub fn troubles(&self, user: &UserFolder) -> Vec<String> {
        let empty = SourceMark::default();
        troubles_since(&user.logs(), &self.source, self.emergency.as_ref().unwrap_or(&empty))
    }

    /// Messages written by extension `id` in `user` since the mark, without
    /// timestamps or headers.
    pub fn extension_messages(&self, user: &UserFolder, id: &str) -> Vec<String> {
        self.source.messages_since(&user.logs(), &extension_source(id))
    }

    /// Messages from `source` in `path`'s folder since the mark.
    pub fn messages(&self, path: &Path, source: &str) -> Vec<String> {
        match path.parent() {
            Some(dir) => self.source.messages_since(dir, source),
            None => Vec::new(),
        }
    }

    /// Raw lines added to `path`'s journal stem since the mark.
    pub fn since(&self, path: &Path) -> Vec<String> {
        let Some(dir) = path.parent() else { return Vec::new() };
        let stem = match path.file_name().and_then(|n| n.to_str()).map(eu4_harmony_logging::classify_file) {
            Some(Artifact::Segment { stem, .. }) => stem,
            _ => CORE,
        };
        self.source.lines_since(dir, stem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_logging::{Journal, Kind};
    use std::path::PathBuf;

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
    fn a_new_emergency_panic_or_crash_is_found_but_an_old_one_is_not_reported_again() {
        let s = Scratch::new("emergency");
        let user = UserFolder::new(&s.0);
        s.log("core.log", "start\n");
        s.log("core-emergency.log", "emergency start\nold panic on thread (1): before\n");
        let mark = Mark::of(&user);
        s.log("core-emergency.log", "emergency start\nold panic on thread (1): before\nnew panic on thread (2): after\n2026 run=1 pid=1 tid=1 up_ms=1 source=core-emergency crash: 0xc0000005\n");
        assert_eq!(mark.troubles(&user), ["new panic on thread (2): after", "2026 run=1 pid=1 tid=1 up_ms=1 source=core-emergency crash: 0xc0000005"]);
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
    fn an_evicted_mark_still_checks_all_retained_segments_for_failures() {
        let s = Scratch::new("evicted-mark");
        let user = UserFolder::new(&s.0);
        s.log("core.log", "evicted segment\n");
        let mark = Mark::of(&user);
        s.log("core.3.log", "oldest retained\n1 panic on thread main (1): retained\n");
        s.log("core.2.log", "next retained\n2 fatal error, ending the game: retained\n");
        s.log("core.prev.log", "previous retained\n");
        s.log("core.log", "current retained\n");
        assert_eq!(mark.troubles(&user), ["1 panic on thread main (1): retained", "2 fatal error, ending the game: retained"]);
        assert_eq!(mark.since(&user.core_log()).last().unwrap(), "current retained");
    }

    #[test]
    fn real_extension_journals_round_trip_across_rotation_and_ignore_other_sources() {
        let s = Scratch::new("messages");
        let user = UserFolder::new(&s.0);
        let dir = user.logs();
        {
            let mut first = Journal::open(&dir, &extension_source("qa-windows")).unwrap();
            first.write(Kind::Event, "before").unwrap();
        }
        let mark = Mark::of_extension(&user, "qa-windows");
        {
            let mut second = Journal::open(&dir, &extension_source("qa-windows")).unwrap();
            second.write(Kind::Event, "qa-windows completed 1").unwrap();
            let mut other = Journal::open(&dir, &extension_source("qa-windows-extra")).unwrap();
            other.write(Kind::Event, "wrong source").unwrap();
        }
        {
            let mut third = Journal::open(&dir, &extension_source("qa-windows")).unwrap();
            third.write(Kind::Event, "qa-windows completed 2").unwrap();
        }
        assert_eq!(mark.extension_messages(&user, "qa-windows"), ["qa-windows completed 1", "qa-windows completed 2"]);
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
