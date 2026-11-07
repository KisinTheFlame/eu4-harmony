//! A computer for the claim's tests, scripted and recorded: what the claim
//! did to it, in order, with a clock that moves only when the claim waits.
//! The claim's course and the scenario runner's game are tested over it.
use super::machine::{Framework, Machine, State};
use super::record::{GameEnd, Process, Record};
use crate::Result;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    time::Duration,
};

/// What the claim did to the computer, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Did {
    /// The record written, as [`described`] puts it.
    Wrote(String),
    /// The queue written, by owner.
    Queued(Vec<String>),
    BackUp,
    RemoveFolder,
    Keep,
    RestorePlayerFiles,
    RestoreFramework,
    EndGames,
    Start,
    Waited,
    Said(String),
}

pub(crate) fn wrote(record: &str) -> Did {
    Did::Wrote(record.into())
}

/// A record in a few words: its owner, a backup not done and the
/// process making it, the process putting it back, and the player
/// taking the game; `free` without one.
pub(crate) fn described(held: &Option<Record>) -> String {
    let Some(h) = held else { return "free".into() };
    let mut line = h.owner.clone();
    if h.unfinished() {
        line += &h.preparing.map_or(", not made".into(), |p| format!(", made by {}", p.id));
    }
    if let Some(p) = h.putting_back {
        line += &format!(", put back by {}", p.id);
    }
    if h.taken.is_some() {
        line += ", taken";
    }
    line
}

/// The process the tests run as, unless they say otherwise, and others.
pub(crate) const ME: Process = Process { id: 10, started: 1 };
pub(crate) const HOLDER: Process = Process { id: 20, started: 2 };
pub(crate) const DEAD: Process = Process { id: 30, started: 3 };
pub(crate) const GAME: Process = Process { id: 40, started: 4 };
pub(crate) const PLAYER: Process = Process { id: 50, started: 5 };
pub(crate) const GAME_DIR: &str = "E:\\game";

pub(crate) type Meanwhile = RefCell<Option<Box<dyn FnOnce(&Recorded)>>>;

/// The computer, scripted and recorded: its clock moves only when the
/// claim waits, the `eu4.exe` that run are those in `games`, a process
/// runs until it is in `ended`, and the backup, the start and putting
/// back the framework files fail when told to.
pub(crate) struct Recorded {
    pub(crate) state: RefCell<State>,
    pub(crate) did: RefCell<Vec<Did>>,
    pub(crate) now: Cell<u64>,
    pub(crate) me: Cell<Process>,
    pub(crate) games: RefCell<Vec<Process>>,
    pub(crate) ended: RefCell<Vec<Process>>,
    pub(crate) back_up_fails: Option<&'static str>,
    pub(crate) start_fails: bool,
    /// The framework files a put back cannot put back, a line each.
    pub(crate) lost: Vec<String>,
    /// Done once, while the backup is made.
    pub(crate) while_backing_up: Meanwhile,
    /// Done once, while the player's files are put back.
    pub(crate) while_putting_back: Meanwhile,
    /// The ends the next look at the games sees, and those noted.
    pub(crate) ending: RefCell<Vec<GameEnd>>,
    pub(crate) noted: RefCell<Vec<GameEnd>>,
}

pub(crate) fn machine(now: u64, held: Option<Record>, games: &[Process]) -> Recorded {
    Recorded {
        state: RefCell::new(State { held, waiting: Vec::new() }),
        did: RefCell::default(),
        now: Cell::new(now),
        me: Cell::new(ME),
        games: RefCell::new(games.to_vec()),
        ended: RefCell::default(),
        back_up_fails: None,
        start_fails: false,
        lost: Vec::new(),
        while_backing_up: RefCell::default(),
        while_putting_back: RefCell::default(),
        ending: RefCell::default(),
        noted: RefCell::default(),
    }
}

impl Recorded {
    pub(crate) fn did(&self, did: Did) {
        self.did.borrow_mut().push(did);
    }

    /// What it did, besides what the claim said, its waits and the
    /// queue.
    pub(crate) fn steps(&self) -> Vec<Did> {
        self.did.borrow().iter().filter(|d| !matches!(d, Did::Said(_) | Did::Waited | Did::Queued(_))).cloned().collect()
    }

    pub(crate) fn said(&self) -> Vec<String> {
        self.did.borrow().iter().filter_map(|d| if let Did::Said(line) = d { Some(line.clone()) } else { None }).collect()
    }

    pub(crate) fn said_line(&self, starting: &str) -> bool {
        self.said().iter().any(|line| line.starts_with(starting))
    }

    pub(crate) fn held(&self) -> Option<Record> {
        self.state.borrow().held.clone()
    }

    pub(crate) fn waiting(&self) -> Vec<String> {
        self.state.borrow().waiting.iter().map(|w| w.owner.clone()).collect()
    }

    pub(crate) fn forget(&self) {
        self.did.borrow_mut().clear();
    }

    /// Runs `action` as the process `p`.
    pub(crate) fn as_process<T>(&self, p: Process, action: impl FnOnce() -> T) -> T {
        let me = self.me.replace(p);
        let value = action();
        self.me.set(me);
        value
    }

    pub(crate) fn meanwhile(hook: &Meanwhile, m: &Recorded) {
        let action = hook.borrow_mut().take();
        if let Some(action) = action {
            action(m);
        }
    }
}

impl Machine for Recorded {
    fn locked<T>(&self, action: impl FnOnce(&mut State) -> T) -> Result<T> {
        let mut state = self.state.borrow_mut();
        let before = state.clone();
        let value = action(&mut state);
        if state.held != before.held {
            self.did(Did::Wrote(described(&state.held)));
        }
        if state.waiting != before.waiting {
            self.did(Did::Queued(state.waiting.iter().map(|w| w.owner.clone()).collect()));
        }
        Ok(value)
    }
    fn claims(&self) -> Result<PathBuf> {
        Ok(PathBuf::from("C:\\claims"))
    }
    fn now(&self) -> u64 {
        self.now.get()
    }
    fn stamps(&self) -> (String, String) {
        (format!("t{}", self.now.get()), self.now.get().to_string())
    }
    fn wait(&self, time: Duration) {
        self.did(Did::Waited);
        self.now.set(self.now.get() + time.as_secs());
    }
    fn me(&self) -> Process {
        self.me.get()
    }
    fn process(&self, id: u32) -> Option<Process> {
        self.games.borrow().iter().find(|g| g.id == id).copied()
    }
    fn alive(&self, p: &Process) -> bool {
        !self.ended.borrow().contains(p)
    }
    fn games(&self) -> Result<Vec<u32>> {
        Ok(self.games.borrow().iter().map(|g| g.id).collect())
    }
    fn end_games(&self) -> Result<()> {
        self.did(Did::EndGames);
        let ended = self.games.take();
        self.ended.borrow_mut().extend(ended);
        Ok(())
    }
    fn game_ends(&self) -> Vec<GameEnd> {
        self.ending.take()
    }
    fn note_ends(&self, _: &Path, ends: &[GameEnd]) {
        self.noted.borrow_mut().extend_from_slice(ends);
    }
    fn ends(&self, _: &Path) -> Vec<GameEnd> {
        self.noted.borrow().clone()
    }
    fn start_game(&self, _: &Path, _: bool, _: Option<u32>, _: Duration) -> Result<(Process, f64)> {
        self.did(Did::Start);
        self.games.borrow_mut().push(GAME);
        if self.start_fails {
            Err("the game ended before its main menu".into())
        } else {
            Ok((GAME, 30.0))
        }
    }
    fn back_up(&self, _: &Path, _: &Path) -> Result<()> {
        self.did(Did::BackUp);
        Self::meanwhile(&self.while_backing_up, self);
        self.back_up_fails.map_or(Ok(()), |e| Err(e.into()))
    }
    fn remove_folder(&self, _: &Path) -> Result<()> {
        self.did(Did::RemoveFolder);
        Ok(())
    }
    fn keep_logs(&self, _: Option<&Path>, _: Option<&Path>, _: &Path) -> Result<bool> {
        self.did(Did::Keep);
        Ok(false)
    }
    fn restore_player_files(&self, _: &Path, _: &Path, _: bool) -> Result<()> {
        self.did(Did::RestorePlayerFiles);
        Self::meanwhile(&self.while_putting_back, self);
        Ok(())
    }
    fn restore_framework_files(&self, _: &Path, _: &Path) -> Result<Framework> {
        self.did(Did::RestoreFramework);
        Ok(if self.lost.is_empty() { Framework::PutBack } else { Framework::Lost(self.lost.clone()) })
    }
    fn made_at(&self, _: &Path) -> Option<u64> {
        None
    }
    fn leftovers(&self, _: Option<&Record>) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    fn say(&self, line: &str) {
        self.did(Did::Said(line.into()));
    }
}

/// `owner`'s complete claim, its lease started at `renewed`.
pub(crate) fn complete(owner: &str, renewed: u64) -> Record {
    let folder = format!("C:\\claims\\{owner}-{renewed}");
    Record {
        owner: owner.into(),
        purpose: "a test".into(),
        checkout: String::new(),
        since: format!("t{renewed}"),
        renewed,
        lease_minutes: 30,
        backup: Some(format!("{folder}\\backup")),
        folder: Some(folder),
        player: false,
        taken: None,
        preparing: None,
        putting_back: None,
        keeper: None,
        game: None,
    }
}
