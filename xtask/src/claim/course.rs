//! The claim's course: claiming the game, keeping the claim going, starting
//! and stopping the game within it, and releasing it, which puts back what
//! its run changed. It reaches the computer only through [`Machine`], so
//! that the order of its writes of the record against what it does to the
//! game and the files is what the tests below follow.
use super::machine::{Framework, Machine};
use super::record::{self, Asking, Found, Process, Record, Seen};
use crate::Result;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// The lease of a claim, in minutes, without `--lease`.
pub const LEASE_MINUTES: u64 = 30;
/// How often a waiting claim looks again, and says it still waits.
const WAIT_EVERY: Duration = Duration::from_secs(5);
/// How often `watch` looks.
const WATCH_EVERY: Duration = Duration::from_secs(5);

/// The exit codes of `watch` besides 0 and 1.
pub const EXIT_LEASE_ENDING: i32 = 3;
pub const EXIT_PLAYER_TAKES: i32 = 4;

/// What a claim came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Claimed {
    /// The game is held by this claim now.
    Now,
    /// This owner held it already.
    Already,
    /// Another holds it, and the claim did not wait.
    Busy,
}

/// What one look at a claim found, as `watch` looks every few seconds.
#[derive(Debug, PartialEq, Eq)]
pub enum Look {
    /// Held, with this many seconds of the lease left.
    Held(u64),
    /// The player takes the game.
    Taken,
    /// The claim is not this owner's any more.
    Gone,
}

/// What this process sees now, for a look at the record.
fn seen<M: Machine>(m: &M) -> Seen<'_> {
    let games = m.games();
    Seen {
        now: m.now(),
        game_running: games.as_ref().map_or(true, |g| !g.is_empty()),
        games: games.unwrap_or_default(),
        me: m.me(),
        alive: Box::new(|p| m.alive(p)),
    }
}

/// Whether an `eu4.exe` runs, as it counts for what this guards: when it
/// cannot be told, it does.
fn game_running<M: Machine>(m: &M) -> bool {
    m.games().map_or(true, |g| !g.is_empty())
}

/// The game holds a claim of `owner`'s; the record if so.
pub fn held_by<M: Machine>(m: &M, owner: &str) -> Result<Record> {
    let held = m.locked(|s| s.held.clone())?;
    match held {
        Some(h) if h.owner == owner && h.unfinished() => {
            Err(format!("{owner}'s claim is not complete: its backup of the player's files is not done; cargo xtask game claim makes it again"))
        }
        Some(h) if h.owner == owner && h.putting_back.is_some() => {
            Err(format!("{owner}'s claim is being released, or a release of it did not end: cargo xtask game release"))
        }
        Some(h) if h.owner == owner => Ok(h),
        Some(h) => Err(format!("{owner} does not hold the game: {}; cargo xtask game claim", h.describe(m.now()))),
        None => Err(format!("{owner} does not hold the game; cargo xtask game claim")),
    }
}

/// Claims the game as `asking` asks, with a lease of `lease` minutes: the
/// record names the claim's folder and the process making it, then the
/// player's files and the game folder's framework files are backed up, and
/// only then is the claim complete. A holder gone, or the one the player
/// takes the game from, is put back first.
pub fn claim<M: Machine>(m: &M, asking: &Asking, purpose: &str, lease: u64, game_dir: &Path) -> Result<Claimed> {
    let claims = m.claims()?;
    let mut said = String::new();
    let mut folder = PathBuf::new();
    loop {
        let seen = seen(m);
        let found = m.locked(|s| {
            let found = record::look(asking, &mut s.held, &mut s.waiting, &seen);
            if found == Found::Free {
                let (since, stamp) = m.stamps();
                folder = claims.join(format!("{}-{stamp}", asking.owner));
                // The record names the claim's folder, and the process making
                // it, before the backup: a claim cut short is known as one.
                let session = !asking.player;
                s.held = Some(Record {
                    owner: asking.owner.clone(),
                    purpose: purpose.to_string(),
                    checkout: crate::repo().display().to_string(),
                    since,
                    renewed: m.now(),
                    lease_minutes: lease,
                    backup: None,
                    folder: session.then(|| folder.display().to_string()),
                    player: asking.player,
                    taken: None,
                    preparing: session.then(|| m.me()),
                    putting_back: None,
                    keeper: None,
                    game: None,
                });
            }
            found
        })?;
        match found {
            Found::Held => {
                m.say(&format!("already held: {}", held_by(m, &asking.owner)?.describe(m.now())));
                return Ok(Claimed::Already);
            }
            Found::Busy(who) if !asking.wait => {
                m.say(&format!("busy: held by {who}"));
                return Ok(Claimed::Busy);
            }
            Found::Busy(who) => {
                if who != said {
                    m.say(&format!("waiting: held by {who}"));
                    said = who;
                }
                m.wait(WAIT_EVERY);
            }
            Found::Take(holder) => {
                m.say(&format!("the player takes the game from {}: ending the game and releasing its claim", holder.describe(m.now())));
                put_back_for(m, &holder, || {
                    m.kill_game();
                    m.wait_closed()?;
                    put_back(m, &holder, game_dir, false).map(drop)
                })?;
                // The game is free now, and the player first.
                return claim(m, asking, purpose, lease, game_dir);
            }
            Found::Gone(holder) => {
                if holder.unfinished() {
                    m.say(&format!("{}: its claim was cut short before its backup was done", holder.describe(m.now())));
                } else {
                    m.say(&format!("{} has gone: putting back what its run changed", holder.describe(m.now())));
                }
                put_back_for(m, &holder, || {
                    // A holder gone with its game running left it running.
                    if game_running(m) {
                        if !holder.only_its_game_runs(&self::seen(m)) {
                            return Err("eu4.exe is running, and not as the gone holder's game: the player may be playing".into());
                        }
                        m.say("ending its game");
                        m.kill_game();
                        m.wait_closed()?;
                    }
                    put_back(m, &holder, game_dir, false).map(drop)
                })?;
                if holder.owner != asking.owner {
                    m.say(&format!("took over from {}", holder.describe(m.now())));
                }
                return claim(m, asking, purpose, lease, game_dir);
            }
            Found::Free => break,
        }
    }
    if asking.player {
        m.say(&format!("claimed for the player: {}", held_by(m, &asking.owner)?.describe(m.now())));
        return Ok(Claimed::Now);
    }
    let me = m.me();
    let making = |h: &Record| h.owner == asking.owner && h.folder.as_deref() == Some(&*folder.display().to_string()) && h.preparing == Some(me);
    match m.back_up(&folder, game_dir) {
        Ok(()) => {
            let held = m
                .locked(|s| {
                    let h = s.held.as_mut().filter(|h| making(h))?;
                    h.backup = Some(folder.join("backup").display().to_string());
                    h.preparing = None;
                    h.renewed = m.now();
                    Some(h.clone())
                })?
                .ok_or_else(|| format!("lost the claim while backing up; the backup is at {}", folder.display()))?;
            m.say(&format!("claimed: {}", held.describe(m.now())));
            m.say(&format!("the claim's folder: {}", folder.display()));
            Ok(Claimed::Now)
        }
        Err(e) => {
            // No claim without its backup: the game is given back, and what
            // was made of the claim's folder goes, a copy of files that
            // nothing changed.
            m.locked(|s| {
                if s.held.as_ref().is_some_and(making) {
                    s.held = None;
                }
            })?;
            let _ = m.remove_folder(&folder);
            Err(format!("not claimed: {e}; the game is free again"))
        }
    }
}

/// Puts back `holder`'s claim, which this process marked as its own to put
/// back, with `putting_back`; frees the game once all is put back, and
/// leaves the claim to be put back again otherwise.
fn put_back_for<M: Machine>(m: &M, holder: &Record, putting_back: impl FnOnce() -> Result<()>) -> Result<()> {
    let done = putting_back();
    let me = m.me();
    let ended = m.locked(|s| end_put_back(&mut s.held, holder, me, done.is_ok()))?;
    done.map_err(|e| format!("{e}; {} still holds the game, its claim not put back", holder.owner))?;
    if !ended {
        return Err(format!("{}'s claim was put back, but its record was changed meanwhile and stays: cargo xtask game status says by whom", holder.owner));
    }
    Ok(())
}

/// Ends `of`, which `me` put back: its record goes once all was put back,
/// and is no longer marked otherwise, so that it is put back again. A record
/// whose mark is gone is still `me`'s to end: xtask from before the marks,
/// still run by other checkouts, writes the record again without them.
/// Whether the record went.
fn end_put_back(held: &mut Option<Record>, of: &Record, me: Process, all: bool) -> bool {
    let Some(h) = held.as_mut().filter(|h| h.same_claim(of) && h.putting_back.is_none_or(|p| p == me)) else { return false };
    if all {
        *held = None;
    } else {
        h.putting_back = None;
    }
    all
}

/// Puts back what a claim's run changed: copies the logs and the run's crash
/// folders into `kept\`, restores the player's files, and the game folder's
/// framework files as `framework\` has them. Returns whether the run
/// crashed. A file not put back fails it, and the claim's folder stays.
fn put_back<M: Machine>(m: &M, holder: &Record, game_dir: &Path, keep_crashes: bool) -> Result<bool> {
    if holder.unfinished() {
        // Nothing ran under a claim whose backup was not done: what it made
        // is part of a copy of files nothing changed, and goes.
        if let Some(folder) = holder.folder.as_deref() {
            m.remove_folder(Path::new(folder))?;
        }
        m.say("the claim's backup was not done, so nothing ran under it: nothing to put back");
        return Ok(false);
    }
    let folder = holder.folder.as_ref().map(PathBuf::from);
    let crashed = m.keep_logs(folder.as_deref(), holder.backup.as_deref().map(Path::new), game_dir)?;
    if let Some(backup) = holder.backup.as_deref() {
        m.restore_player_files(Path::new(backup), game_dir, keep_crashes)?;
    }
    if let Some(folder) = &folder {
        match m.restore_framework_files(folder, game_dir)? {
            Framework::NoCopy => {}
            Framework::PutBack => m.say("the game folder's framework files are as they were"),
            Framework::Lost(lost) => {
                lost.iter().for_each(|line| m.say(&format!("NOT RESTORED {line}")));
                return Err(format!("{} file(s) of the game folder not put back; they are in {}", lost.len(), folder.join("framework").display()));
            }
        }
    }
    Ok(crashed)
}

/// What a holder's release finds of its claim.
#[derive(Clone, Debug, PartialEq)]
enum Release {
    /// The claim, marked as this process's to put back.
    Mine(Box<Record>),
    /// Another process puts it back: the player's claim taking the game, or
    /// one that took over.
    Busy(String),
    /// The game is not `owner`'s.
    NotHeld,
}

/// Marks `owner`'s claim as the looking process's to put back, unless
/// another process that runs makes it or puts it back already.
fn mark_release(owner: &str, held: &mut Option<Record>, seen: &Seen) -> Release {
    let Some(h) = held.as_mut().filter(|h| h.owner == owner) else { return Release::NotHeld };
    if let Some(p) = h.made_by_another(seen) {
        return Release::Busy(format!("the claim is being made: process {} backs up the player's files; release once it is claimed", p.id));
    }
    if let Some(p) = h.put_back_by_another(seen) {
        let whose = if h.taken.is_some() { "the player's claim, which takes the game," } else { "another claim" };
        return Release::Busy(format!("{whose} puts back what this claim's run changed (process {}), and frees the game", p.id));
    }
    h.putting_back = Some(seen.me);
    Release::Mine(Box::new(h.clone()))
}

/// Releases `owner`'s claim: marks it as this process's to put back, ends
/// the game, puts back what its run changed, and frees the game, which the
/// first in the queue then takes.
pub fn release<M: Machine>(m: &M, owner: &str, game_dir: &Path, keep_crashes: bool) -> Result<()> {
    let held = match m.locked(|s| mark_release(owner, &mut s.held, &seen(m)))? {
        Release::Mine(held) => held,
        Release::Busy(why) => return Err(why),
        Release::NotHeld => return Err(held_by(m, owner).err().unwrap_or_else(|| format!("{owner} does not hold the game"))),
    };
    let mut crashed = false;
    put_back_for(m, &held, || {
        if game_running(m) {
            if held.player {
                return Err("eu4.exe is running; close it first".into());
            }
            m.say("ending the game");
            m.kill_game();
            m.wait_closed()?;
        }
        crashed = put_back(m, &held, game_dir, keep_crashes)?;
        Ok(())
    })?;
    if held.unfinished() {
        m.say("released: the claim was not complete, its backup not done, so nothing was put back; the game is free");
        return Ok(());
    }
    m.say("released: the game is free");
    if !held.player {
        let since = held.folder.as_deref().and_then(|f| m.made_at(Path::new(f))).unwrap_or(held.renewed);
        let minutes = m.now().saturating_sub(since) / 60;
        m.say(&format!("held since {} (about {minutes} min); the game {}", held.since, if crashed { "crashed" } else { "did not crash" }));
        if let Some(folder) = &held.folder {
            m.say(&format!("kept: {}", Path::new(folder).join("kept").display()));
        }
    }
    Ok(())
}

/// Starts `owner`'s lease again.
pub fn renew<M: Machine>(m: &M, owner: &str) -> Result<()> {
    let held = m
        .locked(|s| {
            let h = s.held.as_mut().filter(|h| h.owner == owner)?;
            h.renewed = m.now();
            Some(h.clone())
        })?
        .ok_or_else(|| format!("{owner} does not hold the game"))?;
    m.say(&format!("renewed: {}", held.describe(m.now())));
    Ok(())
}

/// The look of the process keeping `owner`'s claim going, its `watch` or
/// the runner playing in it: it keeps the claim, unless another that runs
/// does already; starts its lease again while the game runs; and takes the
/// one `eu4.exe` that runs, as `started` finds it, for the claim's game when
/// the claim has none that runs: the game it is starting, or the process the
/// game started in its own place. The claim, if it is still `owner`'s.
fn keep(owner: &str, held: &mut Option<Record>, seen: &Seen, started: impl Fn(u32) -> Option<Process>) -> Option<Record> {
    let h = held.as_mut().filter(|h| h.owner == owner)?;
    if !h.kept(seen) {
        h.keeper = Some(seen.me);
    }
    if seen.game_running {
        h.renewed = seen.now;
    }
    if let [id] = seen.games.as_slice() {
        if !h.game.is_some_and(|g| (seen.alive)(&g)) {
            h.game = started(*id).or(h.game);
        }
    }
    Some(h.clone())
}

/// The look of the process keeping `owner`'s claim going; the claim, if it
/// is still `owner`'s.
fn keep_going<M: Machine>(m: &M, owner: &str) -> Result<Option<Record>> {
    let seen = seen(m);
    m.locked(|s| keep(owner, &mut s.held, &seen, |id| m.process(id)))
}

/// One look at `owner`'s claim by the process keeping it going, which keeps
/// the lease going while the game runs.
pub fn look<M: Machine>(m: &M, owner: &str) -> Result<Look> {
    Ok(match keep_going(m, owner)? {
        None => Look::Gone,
        Some(h) if h.taken.is_some() => Look::Taken,
        Some(h) => Look::Held(h.lease_ends().saturating_sub(m.now())),
    })
}

/// Keeps `owner`'s claim going until the lease is down to `warn` minutes,
/// with [`EXIT_LEASE_ENDING`], the player takes the game, with
/// [`EXIT_PLAYER_TAKES`], or the claim is not this owner's, with 0.
pub fn watch<M: Machine>(m: &M, owner: &str, warn: u64) -> Result<i32> {
    loop {
        let Some(held) = keep_going(m, owner)? else {
            m.say(&format!("{owner} no longer holds the game"));
            return Ok(0);
        };
        if let Some(taken) = held.taken {
            let left = (taken + record::TAKEN_GRACE_SECONDS).saturating_sub(m.now());
            m.say(&format!("the player takes the game in {left} s: keep what the run needs; the game is then ended and the claim released for you"));
            return Ok(EXIT_PLAYER_TAKES);
        }
        let left = held.lease_ends().saturating_sub(m.now());
        if left <= 60 * warn {
            m.say(&format!("the lease ends in {} min: cargo xtask game renew and watch again to keep the game, or release", left.div_ceil(60)));
            return Ok(EXIT_LEASE_ENDING);
        }
        m.wait(WATCH_EVERY);
    }
}

/// Says who holds the game, who waits, and what no claim refers to.
pub fn status<M: Machine>(m: &M) -> Result<()> {
    let seen = seen(m);
    let (held, mut waiting) = m.locked(|s| (s.held.clone(), s.waiting.clone()))?;
    let now = m.now();
    record::tidy(&mut waiting, now);
    match &held {
        Some(h) => {
            m.say(&format!("held by {}", h.describe(now)));
            if let Some(folder) = &h.folder {
                m.say(&format!("  the claim's folder: {folder}"));
            }
            if h.stale(&seen) {
                m.say("  the lease has run out: the next claim takes over");
            }
            match h.keeper {
                Some(k) if m.alive(&k) => m.say(&format!("  kept going by process {}, which keeps the lease going while the game runs", k.id)),
                _ => m.say("  no runner or watch keeps it going: the lease counts down while the game runs too"),
            }
            if let Some(taken) = h.taken {
                m.say(&format!("  the player takes the game in {} s", (taken + record::TAKEN_GRACE_SECONDS).saturating_sub(now)));
            }
            if let Some(p) = h.putting_back {
                let ended = if m.alive(&p) { "" } else { ", until it ended: the next release or claim puts it back" };
                m.say(&format!("  process {} puts back what its run changed{ended}", p.id));
            }
        }
        None => m.say("free"),
    }
    for (i, w) in waiting.iter().enumerate() {
        m.say(&format!("waiting {}: {}{}", i + 1, w.owner, if w.player { " (the player)" } else { "" }));
    }
    match m.games() {
        Ok(games) => m.say(&format!("eu4.exe running: {}", !games.is_empty())),
        Err(e) => m.say(&format!("eu4.exe running: cannot tell, so it counts as running ({e})")),
    }
    m.leftovers(held.as_ref())?.iter().for_each(|line| m.say(line));
    Ok(())
}

/// Starts the game hidden within `owner`'s claim, waiting up to `wait` for
/// its main menu, and names it in the record as the claim's game; a start
/// that does not reach it ends the game. The seconds it took.
pub fn start_game<M: Machine>(m: &M, owner: &str, game_dir: &Path, sound: bool, seed: Option<u32>, wait: Duration) -> Result<f64> {
    held_by(m, owner)?;
    renew(m, owner)?;
    let started = m.start_game(game_dir, sound, seed, wait);
    if let Ok((id, _)) = started {
        let game = m.process(id);
        m.locked(|s| {
            if let Some(h) = s.held.as_mut().filter(|h| h.owner == owner) {
                h.game = game;
            }
        })?;
    }
    if started.is_err() && game_running(m) {
        m.say("ending the game, which did not reach its main menu");
        m.kill_game();
        m.wait_closed()?;
    }
    started.map(|(_, seconds)| seconds)
}

/// Ends the game within `owner`'s claim.
pub fn stop<M: Machine>(m: &M, owner: &str) -> Result<()> {
    held_by(m, owner)?;
    m.kill_game();
    m.wait_closed()?;
    m.say("the game is closed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::machine::State;
    use super::super::record::Waiting;
    use super::*;
    use std::cell::{Cell, RefCell};

    /// What the claim did to the computer, in order.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Did {
        /// The record written, as [`described`] puts it.
        Wrote(String),
        /// The queue written, by owner.
        Queued(Vec<String>),
        BackUp,
        RemoveFolder,
        Keep,
        RestorePlayerFiles,
        RestoreFramework,
        Kill,
        WaitClosed,
        Start,
        Waited,
        Said(String),
    }

    fn wrote(record: &str) -> Did {
        Did::Wrote(record.into())
    }

    /// A record in a few words: its owner, a backup not done and the
    /// process making it, the process putting it back, and the player
    /// taking the game; `free` without one.
    fn described(held: &Option<Record>) -> String {
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
    const ME: Process = Process { id: 10, started: 1 };
    const HOLDER: Process = Process { id: 20, started: 2 };
    const DEAD: Process = Process { id: 30, started: 3 };
    const GAME: Process = Process { id: 40, started: 4 };
    const PLAYER: Process = Process { id: 50, started: 5 };
    const GAME_DIR: &str = "E:\\game";

    type Meanwhile = RefCell<Option<Box<dyn FnOnce(&Recorded)>>>;

    /// The computer, scripted and recorded: its clock moves only when the
    /// claim waits, the `eu4.exe` that run are those in `games`, a process
    /// runs until it is in `ended`, and the backup, the start and putting
    /// back the framework files fail when told to.
    struct Recorded {
        state: RefCell<State>,
        did: RefCell<Vec<Did>>,
        now: Cell<u64>,
        me: Cell<Process>,
        games: RefCell<Vec<Process>>,
        ended: RefCell<Vec<Process>>,
        back_up_fails: Option<&'static str>,
        start_fails: bool,
        /// The framework files a put back cannot put back, a line each.
        lost: Vec<String>,
        /// Done once, while the backup is made.
        while_backing_up: Meanwhile,
        /// Done once, while the player's files are put back.
        while_putting_back: Meanwhile,
    }

    fn machine(now: u64, held: Option<Record>, games: &[Process]) -> Recorded {
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
        }
    }

    impl Recorded {
        fn did(&self, did: Did) {
            self.did.borrow_mut().push(did);
        }

        /// What it did, besides what the claim said, its waits and the
        /// queue.
        fn steps(&self) -> Vec<Did> {
            self.did.borrow().iter().filter(|d| !matches!(d, Did::Said(_) | Did::Waited | Did::Queued(_))).cloned().collect()
        }

        fn said(&self) -> Vec<String> {
            self.did.borrow().iter().filter_map(|d| if let Did::Said(line) = d { Some(line.clone()) } else { None }).collect()
        }

        fn said_line(&self, starting: &str) -> bool {
            self.said().iter().any(|line| line.starts_with(starting))
        }

        fn held(&self) -> Option<Record> {
            self.state.borrow().held.clone()
        }

        fn waiting(&self) -> Vec<String> {
            self.state.borrow().waiting.iter().map(|w| w.owner.clone()).collect()
        }

        fn forget(&self) {
            self.did.borrow_mut().clear();
        }

        /// Runs `action` as the process `p`.
        fn as_process<T>(&self, p: Process, action: impl FnOnce() -> T) -> T {
            let me = self.me.replace(p);
            let value = action();
            self.me.set(me);
            value
        }

        fn meanwhile(hook: &Meanwhile, m: &Recorded) {
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
        fn kill_game(&self) {
            self.did(Did::Kill);
            let killed = self.games.take();
            self.ended.borrow_mut().extend(killed);
        }
        fn wait_closed(&self) -> Result<()> {
            self.did(Did::WaitClosed);
            if self.games.borrow().is_empty() {
                Ok(())
            } else {
                Err("eu4.exe did not end".into())
            }
        }
        fn start_game(&self, _: &Path, _: bool, _: Option<u32>, _: Duration) -> Result<(u32, f64)> {
            self.did(Did::Start);
            self.games.borrow_mut().push(GAME);
            if self.start_fails {
                Err("the game ended before its main menu".into())
            } else {
                Ok((GAME.id, 30.0))
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
    fn complete(owner: &str, renewed: u64) -> Record {
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

    /// `owner`'s claim whose backup `process` was making.
    fn unfinished(owner: &str, renewed: u64, process: Process) -> Record {
        Record { backup: None, preparing: Some(process), ..complete(owner, renewed) }
    }

    fn waiting(owner: &str, asked: u64, seen: u64) -> Waiting {
        Waiting { owner: owner.into(), asked, seen, player: false }
    }

    fn asking(owner: &str, wait: bool) -> Asking {
        Asking { owner: owner.into(), player: false, now: false, wait }
    }

    fn claim_as(m: &Recorded, owner: &str, wait: bool) -> Result<Claimed> {
        claim(m, &asking(owner, wait), "a test", LEASE_MINUTES, Path::new(GAME_DIR))
    }

    fn release_as(m: &Recorded, owner: &str) -> Result<()> {
        release(m, owner, Path::new(GAME_DIR), false)
    }

    /// What putting back a claim does once its game is closed.
    const PUT_BACK: [Did; 3] = [Did::Keep, Did::RestorePlayerFiles, Did::RestoreFramework];

    fn steps(parts: &[&[Did]]) -> Vec<Did> {
        parts.concat()
    }

    #[test]
    fn the_record_names_the_new_holder_complete_only_once_the_backup_is_done() {
        let m = machine(1000, None, &[]);
        assert_eq!(claim_as(&m, "a", false), Ok(Claimed::Now));
        assert_eq!(m.steps(), [wrote("a, made by 10"), Did::BackUp, wrote("a")]);
        let held = m.held().unwrap();
        assert_eq!((held.folder.as_deref(), held.backup.as_deref(), held.preparing), (Some("C:\\claims\\a-1000"), Some("C:\\claims\\a-1000\\backup"), None));
        assert!(m.said_line("claimed: a since t1000"));
    }

    #[test]
    fn a_backup_that_fails_gives_the_game_back_and_removes_what_it_made() {
        let mut m = machine(1000, None, &[]);
        m.back_up_fails = Some("eu4.exe is running; close it first");
        assert_eq!(claim_as(&m, "a", false), Err("not claimed: eu4.exe is running; close it first; the game is free again".into()));
        assert_eq!(m.steps(), [wrote("a, made by 10"), Did::BackUp, wrote("free"), Did::RemoveFolder]);
    }

    #[test]
    fn a_claim_lost_while_backing_up_is_not_taken_for_held_and_keeps_its_backup() {
        let m = machine(1000, None, &[]);
        *m.while_backing_up.borrow_mut() = Some(Box::new(|m| m.state.borrow_mut().held = Some(complete("b", 1000))));
        assert_eq!(claim_as(&m, "a", false), Err("lost the claim while backing up; the backup is at C:\\claims\\a-1000".into()));
        assert_eq!(m.steps(), [wrote("a, made by 10"), Did::BackUp]);
        assert_eq!(m.held().unwrap().owner, "b");
        assert!(!m.said_line("claimed"));
    }

    #[test]
    fn a_claim_cut_short_while_backing_up_is_made_afresh_by_its_owner() {
        let m = machine(1000, Some(unfinished("a", 500, DEAD)), &[]);
        m.ended.borrow_mut().push(DEAD);
        assert!(held_by(&m, "a").unwrap_err().contains("not complete"));
        assert_eq!(claim_as(&m, "a", false), Ok(Claimed::Now));
        assert_eq!(m.steps(), [wrote("a, made by 30, put back by 10"), Did::RemoveFolder, wrote("free"), wrote("a, made by 10"), Did::BackUp, wrote("a")]);
        assert!(m.said_line("a since t500 (a test); lease ends in 22 min: its claim was cut short before its backup was done"));
        assert!(held_by(&m, "a").is_ok());
    }

    #[test]
    fn a_release_of_a_claim_whose_backup_was_not_done_puts_nothing_back_and_says_so() {
        let m = machine(1000, Some(unfinished("a", 500, DEAD)), &[]);
        m.ended.borrow_mut().push(DEAD);
        assert_eq!(release_as(&m, "a"), Ok(()));
        assert_eq!(m.steps(), [wrote("a, made by 30, put back by 10"), Did::RemoveFolder, wrote("free")]);
        assert_eq!(m.said().last().unwrap(), "released: the claim was not complete, its backup not done, so nothing was put back; the game is free");
    }

    #[test]
    fn a_claim_another_process_is_making_is_neither_released_nor_taken_over_but_by_that_process() {
        let late = 500 + 30 * 60 + 1;
        let m = machine(late, Some(unfinished("a", 500, HOLDER)), &[]);
        assert!(release_as(&m, "a").unwrap_err().contains("the claim is being made: process 20 backs up the player's files"));
        assert_eq!(claim_as(&m, "b", false), Ok(Claimed::Busy));
        assert_eq!(m.steps(), []);
        assert_eq!(m.as_process(HOLDER, || release_as(&m, "a")), Ok(()));
        assert_eq!(m.steps(), [wrote("a, made by 20, put back by 20"), Did::RemoveFolder, wrote("free")]);
    }

    #[test]
    fn a_release_puts_back_then_frees_and_the_first_in_the_queue_takes_the_game() {
        let m = machine(1100, Some(complete("a", 1000)), &[GAME]);
        m.state.borrow_mut().waiting = vec![waiting("b", 1000, 1100), waiting("c", 1050, 1100)];
        assert_eq!(release_as(&m, "a"), Ok(()));
        assert_eq!(m.steps(), steps(&[&[wrote("a, put back by 10"), Did::Kill, Did::WaitClosed], &PUT_BACK, &[wrote("free")]]));
        let said = m.said();
        let at = |line: &str| said.iter().position(|l| l.starts_with(line)).unwrap_or_else(|| panic!("{line:?} not said: {said:?}"));
        assert!(at("ending the game") < at("the game folder's framework files are as they were"));
        assert!(at("the game folder's framework files are as they were") < at("released: the game is free"));
        assert!(m.said_line("kept: C:\\claims\\a-1000\\kept"));

        m.forget();
        assert_eq!(claim_as(&m, "c", false), Ok(Claimed::Busy));
        assert!(m.said_line("busy: held by nobody, but b waits first"));
        assert_eq!(claim_as(&m, "b", true), Ok(Claimed::Now));
        assert_eq!(m.steps(), [wrote("b, made by 10"), Did::BackUp, wrote("b")]);
        assert_eq!(m.waiting(), ["c"]);
    }

    #[test]
    fn a_file_a_release_cannot_put_back_is_reported_and_the_claim_stays_to_be_released_again() {
        let mut m = machine(1100, Some(complete("a", 1000)), &[]);
        m.lost = vec!["E:\\game\\version.dll: access denied".into()];
        assert_eq!(
            release_as(&m, "a"),
            Err("1 file(s) of the game folder not put back; they are in C:\\claims\\a-1000\\framework; a still holds the game, its claim not put back".into())
        );
        assert_eq!(m.steps(), steps(&[&[wrote("a, put back by 10")], &PUT_BACK, &[wrote("a")]]));
        assert!(m.said_line("NOT RESTORED E:\\game\\version.dll: access denied"));
        assert!(!m.said_line("released"));
        assert!(held_by(&m, "a").is_ok(), "still held, and no longer marked as being put back");

        m.lost.clear();
        assert_eq!(release_as(&m, "a"), Ok(()));
        assert_eq!(m.held(), None);
    }

    #[test]
    fn only_one_process_puts_back_a_claim_the_player_takes_the_game_from() {
        let m = machine(1000, Some(complete("a", 1000)), &[GAME]);
        m.me.set(PLAYER);
        // The holder's own release, while the player's claim puts it back.
        *m.while_putting_back.borrow_mut() = Some(Box::new(|m| {
            let refused = m.as_process(HOLDER, || release_as(m, "a"));
            assert_eq!(
                refused,
                Err("the player's claim, which takes the game, puts back what this claim's run changed (process 50), and frees the game".into())
            );
        }));
        let player = Asking { owner: "player".into(), player: true, now: true, wait: true };
        assert_eq!(claim(&m, &player, "playing", record::PLAYER_LEASE_MINUTES, Path::new(GAME_DIR)), Ok(Claimed::Now));
        assert_eq!(
            m.steps(),
            steps(&[&[wrote("a, taken"), wrote("a, put back by 50, taken"), Did::Kill, Did::WaitClosed], &PUT_BACK, &[wrote("free"), wrote("player")]])
        );
        assert_eq!(m.now(), 1000 + record::TAKEN_GRACE_SECONDS, "the holder had its grace");
        assert!(m.said_line("the player takes the game from a since t1000"));
        assert!(m.said_line("claimed for the player"));
    }

    #[test]
    fn the_player_waits_while_the_holder_puts_back_its_own_claim() {
        let m = machine(1000, Some(complete("a", 1000)), &[]);
        m.me.set(HOLDER);
        *m.while_putting_back.borrow_mut() = Some(Box::new(|m| {
            let player = Asking { owner: "player".into(), player: true, now: true, wait: false };
            let busy = m.as_process(PLAYER, || claim(m, &player, "playing", record::PLAYER_LEASE_MINUTES, Path::new(GAME_DIR)));
            assert_eq!(busy, Ok(Claimed::Busy));
        }));
        assert_eq!(release_as(&m, "a"), Ok(()));
        assert!(m.said_line("busy: held by a since t1000 (a test); lease ends in 30 min; process 20 puts back what its run changed"));
        assert_eq!(m.steps(), steps(&[&[wrote("a, put back by 20")], &PUT_BACK, &[wrote("free")]]));
    }

    #[test]
    fn a_release_whose_mark_an_older_xtask_dropped_still_frees_the_game() {
        let m = machine(1100, Some(complete("a", 1000)), &[]);
        // A waiter on the xtask of before the marks writes the record again.
        *m.while_putting_back.borrow_mut() = Some(Box::new(|m| m.state.borrow_mut().held.as_mut().unwrap().putting_back = None));
        assert_eq!(release_as(&m, "a"), Ok(()));
        assert_eq!(m.held(), None);
        assert!(m.said_line("released: the game is free"));
    }

    #[test]
    fn a_release_whose_record_another_process_marked_meanwhile_says_the_record_stays() {
        let m = machine(1100, Some(complete("a", 1000)), &[]);
        *m.while_putting_back.borrow_mut() = Some(Box::new(|m| m.state.borrow_mut().held.as_mut().unwrap().putting_back = Some(PLAYER)));
        assert_eq!(
            release_as(&m, "a"),
            Err("a's claim was put back, but its record was changed meanwhile and stays: cargo xtask game status says by whom".into())
        );
        assert!(!m.said_line("released"));
        assert_eq!(m.held().unwrap().putting_back, Some(PLAYER));
    }

    #[test]
    fn a_lease_run_out_hands_the_game_to_the_first_in_the_queue() {
        let late = 30 * 60 + 1;
        let m = machine(late, Some(complete("a", 0)), &[]);
        m.state.borrow_mut().waiting = vec![waiting("b", 100, late), waiting("c", 200, late)];
        assert_eq!(claim_as(&m, "c", false), Ok(Claimed::Busy));
        assert!(m.said_line("busy: held by a since t0 (a test); lease ran out 0 min ago, gone; b waits first"));
        assert_eq!(m.steps(), [], "the second in the queue puts nothing back");

        assert_eq!(claim_as(&m, "b", true), Ok(Claimed::Now));
        assert_eq!(m.steps(), steps(&[&[wrote("a, put back by 10")], &PUT_BACK, &[wrote("free"), wrote("b, made by 10"), Did::BackUp, wrote("b")]]));
        assert!(m.said_line("a since t0 (a test); lease ran out 0 min ago has gone: putting back what its run changed"));
        assert!(m.said_line("took over from a"));
        assert_eq!(m.waiting(), ["c"]);
    }

    #[test]
    fn the_lease_is_kept_going_while_the_game_runs_only_while_its_holder_lives() {
        let late = 30 * 60 + 1;
        let m = machine(late, Some(Record { keeper: Some(HOLDER), game: Some(GAME), ..complete("a", 0) }), &[GAME]);
        assert_eq!(claim_as(&m, "b", false), Ok(Claimed::Busy));
        assert_eq!(m.held().unwrap().renewed, late, "its watch runs: the game running starts the lease again");

        // Its watch ends, the game left running.
        m.ended.borrow_mut().push(HOLDER);
        m.now.set(late + 100);
        assert_eq!(claim_as(&m, "b", false), Ok(Claimed::Busy));
        assert_eq!(m.held().unwrap().renewed, late, "nothing keeps it going");

        m.forget();
        m.now.set(late + 30 * 60 + 1);
        assert_eq!(claim_as(&m, "b", false), Ok(Claimed::Now));
        assert_eq!(
            m.steps(),
            steps(&[&[wrote("a, put back by 10"), Did::Kill, Did::WaitClosed], &PUT_BACK, &[wrote("free"), wrote("b, made by 10"), Did::BackUp, wrote("b")]])
        );
        assert!(m.said_line("ending its game"));
    }

    #[test]
    fn a_game_running_that_is_not_the_gone_holder_s_keeps_the_holder() {
        let late = 30 * 60 + 1;
        let m = machine(late, Some(Record { keeper: Some(DEAD), game: Some(GAME), ..complete("a", 0) }), &[GAME, PLAYER]);
        m.ended.borrow_mut().push(DEAD);
        assert_eq!(claim_as(&m, "b", false), Ok(Claimed::Busy));
        assert_eq!(m.steps(), []);
    }

    #[test]
    fn its_watch_keeps_the_claim_going_and_follows_its_game() {
        let m = machine(100, Some(complete("a", 0)), &[GAME]);
        m.me.set(HOLDER);
        assert_eq!(look(&m, "a"), Ok(Look::Held(30 * 60)));
        let held = m.held().unwrap();
        assert_eq!((held.keeper, held.game, held.renewed), (Some(HOLDER), Some(GAME), 100));

        // The game starts again in its own place; another process looks.
        let again = Process { id: 41, started: 6 };
        m.ended.borrow_mut().push(GAME);
        *m.games.borrow_mut() = vec![again];
        m.as_process(ME, || look(&m, "a")).unwrap();
        let held = m.held().unwrap();
        assert_eq!((held.keeper, held.game), (Some(HOLDER), Some(again)), "its watch runs still and keeps it");

        m.state.borrow_mut().held.as_mut().unwrap().taken = Some(100);
        assert_eq!(look(&m, "a"), Ok(Look::Taken));
        assert_eq!(look(&m, "b"), Ok(Look::Gone));
    }

    #[test]
    fn a_watch_ends_when_the_lease_is_ending_when_the_player_takes_the_game_and_when_the_claim_is_gone() {
        let m = machine(0, Some(complete("a", 0)), &[]);
        assert_eq!(watch(&m, "a", 5), Ok(EXIT_LEASE_ENDING));
        assert_eq!(m.now(), 25 * 60);
        assert!(m.said().last().unwrap().starts_with("the lease ends in 5 min"));

        m.state.borrow_mut().held.as_mut().unwrap().taken = Some(m.now());
        assert_eq!(watch(&m, "a", 5), Ok(EXIT_PLAYER_TAKES));
        assert!(m.said().last().unwrap().starts_with("the player takes the game in 120 s"));

        m.state.borrow_mut().held = None;
        assert_eq!(watch(&m, "a", 5), Ok(0));
        assert_eq!(m.said().last().unwrap(), "a no longer holds the game");
    }

    #[test]
    fn a_gone_holder_whose_put_back_fails_keeps_its_record_until_the_next_puts_it_back() {
        let late = 30 * 60 + 1;
        let mut m = machine(late, Some(complete("a", 0)), &[]);
        m.lost = vec!["E:\\game\\version.dll: access denied".into()];
        assert!(claim_as(&m, "b", false).unwrap_err().ends_with("; a still holds the game, its claim not put back"));
        assert_eq!(described(&m.held()), "a", "a's still, to be put back again");

        // The next to put it back ends before it is done.
        m.lost.clear();
        m.state.borrow_mut().held.as_mut().unwrap().putting_back = Some(DEAD);
        m.ended.borrow_mut().push(DEAD);
        m.forget();
        assert_eq!(claim_as(&m, "c", false), Ok(Claimed::Now));
        assert_eq!(m.steps(), steps(&[&[wrote("a, put back by 10")], &PUT_BACK, &[wrote("free"), wrote("c, made by 10"), Did::BackUp, wrote("c")]]));
    }

    #[test]
    fn the_player_s_claim_is_released_only_with_the_game_closed() {
        let player = Record { player: true, backup: None, folder: None, ..complete("player", 1000) };
        let m = machine(1100, Some(player), &[GAME]);
        assert_eq!(release_as(&m, "player"), Err("eu4.exe is running; close it first; player still holds the game, its claim not put back".into()));
        assert_eq!(m.steps(), [wrote("player, put back by 10"), wrote("player")]);
        m.games.borrow_mut().clear();
        assert_eq!(release_as(&m, "player"), Ok(()));
        assert_eq!(m.said().last().unwrap(), "released: the game is free");
    }

    #[test]
    fn a_start_names_the_game_in_the_record_and_one_that_misses_its_main_menu_ends_the_game() {
        let mut m = machine(1000, Some(complete("a", 0)), &[]);
        assert_eq!(start_game(&m, "a", Path::new(GAME_DIR), false, None, Duration::from_secs(60)), Ok(30.0));
        assert_eq!(m.steps(), [wrote("a"), Did::Start, wrote("a")]);
        assert_eq!(m.held().unwrap().game, Some(GAME));

        m.games.borrow_mut().clear();
        m.start_fails = true;
        m.forget();
        assert!(start_game(&m, "a", Path::new(GAME_DIR), false, None, Duration::from_secs(60)).is_err());
        assert_eq!(m.steps(), [Did::Start, Did::Kill, Did::WaitClosed], "renewed already this second: nothing written");
    }
}
