//! A computer for the claim's tests, scripted and recorded: what the claim
//! did to it, in order, with a clock that moves only when the claim waits.
//! The claim's course and the scenario runner's game are tested over it.
use super::course::Starting;
use super::machine::Machine;
use super::record::{Asking, Claim, GameEnd, Process, State};
use crate::Result;
use eu4_harmony_client::start::Start;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

/// What the claim did to the computer, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Did {
    /// The record written: each claim's owner, `?` after one not ready.
    Wrote(String),
    MakeFolder(String),
    RemoveFolder(String),
    Prepare,
    Start,
    Follow(u32),
    StopFollowing,
    End(u32),
    Keep(String),
    Waited,
}

pub(crate) const ME: Process = Process { id: 10, started: 1 };
pub(crate) const GAME: Process = Process { id: 40, started: 4 };
pub(crate) const GAME_DIR: &str = "E:\\game";

/// What changes the record while the claim waits.
pub(crate) type Meanwhile = Box<dyn FnOnce(&mut State) + Send>;

/// The computer, scripted and recorded: its clock moves only when the
/// claim waits, the games serving pipes are those in `serving`.
pub(crate) struct Recorded {
    pub(crate) state: Mutex<State>,
    pub(crate) did: Mutex<Vec<Did>>,
    pub(crate) now: Mutex<u64>,
    pub(crate) serving: Mutex<Vec<(String, u32)>>,
    pub(crate) proxy: bool,
    pub(crate) fail_folder: bool,
    pub(crate) said: Mutex<Vec<String>>,
    /// Run once, the first time the claim waits.
    pub(crate) meanwhile: Mutex<Option<Meanwhile>>,
    /// How the claim's games ended, as its record has it.
    pub(crate) ended: Mutex<Vec<GameEnd>>,
}

impl Recorded {
    pub(crate) fn new(state: State) -> Self {
        Self {
            state: Mutex::new(state),
            did: Mutex::new(Vec::new()),
            now: Mutex::new(1000),
            serving: Mutex::new(Vec::new()),
            proxy: true,
            fail_folder: false,
            said: Mutex::new(Vec::new()),
            meanwhile: Mutex::new(None),
            ended: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn did(&self) -> Vec<Did> {
        self.did.lock().unwrap().clone()
    }

    pub(crate) fn push(&self, did: Did) {
        self.did.lock().unwrap().push(did);
    }

    pub(crate) fn said(&self) -> String {
        self.said.lock().unwrap().join("\n")
    }
}

pub(crate) fn described(state: &State) -> String {
    state.claims.iter().map(|c| format!("{}{}", c.owner, if c.ready { "" } else { "?" })).collect::<Vec<_>>().join(",")
}

impl Machine for Recorded {
    fn locked<T>(&self, action: impl FnOnce(&mut State) -> T) -> Result<T> {
        let mut state = self.state.lock().unwrap();
        let before = described(&state);
        let value = action(&mut state);
        let after = described(&state);
        if after != before {
            self.push(Did::Wrote(after));
        }
        Ok(value)
    }
    fn claims(&self) -> Result<PathBuf> {
        Ok(PathBuf::from("C:\\claims"))
    }
    fn now(&self) -> u64 {
        *self.now.lock().unwrap()
    }
    fn stamps(&self) -> (String, String) {
        ("2026-11-08 10:00".into(), format!("{}", self.now()))
    }
    fn wait(&self, time: Duration) {
        *self.now.lock().unwrap() += time.as_secs();
        self.push(Did::Waited);
        if let Some(meanwhile) = self.meanwhile.lock().unwrap().take() {
            meanwhile(&mut self.state.lock().unwrap());
        }
    }
    fn me(&self) -> Process {
        ME
    }
    fn process(&self, id: u32) -> Option<Process> {
        (id == GAME.id).then_some(GAME)
    }
    fn alive(&self, p: &Process) -> bool {
        *p == ME || self.serving.lock().unwrap().iter().any(|(_, id)| *id == p.id)
    }
    fn games(&self) -> Result<Vec<u32>> {
        Ok(self.serving.lock().unwrap().iter().map(|(_, id)| *id).collect())
    }
    fn serving(&self, name: &str) -> Option<u32> {
        self.serving.lock().unwrap().iter().find(|(pipe, _)| pipe == name).map(|(_, id)| *id)
    }
    fn end_game(&self, id: u32) -> Result<()> {
        self.serving.lock().unwrap().retain(|(_, g)| *g != id);
        self.push(Did::End(id));
        Ok(())
    }
    fn proxy_loads_cores(&self, _: &Path) -> bool {
        self.proxy
    }
    fn make_folder(&self, folder: &Path, _: &Path) -> Result<()> {
        self.push(Did::MakeFolder(folder.display().to_string()));
        if self.fail_folder {
            return Err("the disk is full".into());
        }
        Ok(())
    }
    fn prepare_start(&self, folder: &Path, _: &Path, _: &Starting) -> Result<PathBuf> {
        self.push(Did::Prepare);
        Ok(folder.join("core").join("eu4_harmony_core.dll"))
    }
    fn start_game(&self, _: &Path, _: &Path, start: &Start, _: Duration) -> Result<(u32, f64)> {
        self.push(Did::Start);
        self.serving.lock().unwrap().push((start.pipe()?, GAME.id));
        Ok((GAME.id, 30.0))
    }
    fn follow(&self, _: &Path, id: u32, _: &str) -> Result<()> {
        self.push(Did::Follow(id));
        Ok(())
    }
    fn stop_following(&self, _: &Path) {
        self.push(Did::StopFollowing);
    }
    fn ends(&self, _: &Path) -> Vec<GameEnd> {
        self.ended.lock().unwrap().clone()
    }
    fn keep_and_remove(&self, folder: &Path) -> Result<bool> {
        self.push(Did::Keep(folder.display().to_string()));
        Ok(false)
    }
    fn remove_folder(&self, folder: &Path) -> Result<()> {
        self.push(Did::RemoveFolder(folder.display().to_string()));
        Ok(())
    }
    fn leftovers(&self, _: &State) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    fn say(&self, line: &str) {
        self.said.lock().unwrap().push(line.to_string());
    }
}

pub(crate) fn held(owner: &str) -> Claim {
    Claim {
        owner: owner.into(),
        purpose: "a run".into(),
        checkout: String::new(),
        since: "2026-11-08 09:00".into(),
        folder: format!("C:\\claims\\{owner}-1"),
        maker: None,
        ready: true,
        game: None,
    }
}

pub(crate) fn asking(owner: &str, wait: bool) -> Asking {
    Asking { owner: owner.into(), wait }
}

pub(crate) fn game_dir() -> &'static Path {
    Path::new(GAME_DIR)
}

/// The record with `claims` and no queue.
pub(crate) fn state(claims: Vec<Claim>) -> State {
    State { claims, ..State::default() }
}
