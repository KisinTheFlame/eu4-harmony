//! The claim's course: claiming a test game, starting and stopping it,
//! releasing it, which keeps its logs and removes the rest of it; at most
//! [`AT_ONCE`] test games running at once, a start waiting for a place
//! while they run; and a run,
//! such as the scenario runner's, played within a claim from holding it to
//! giving it back (docs/plans/test-games.md). It reaches the computer only through
//! [`Machine`], so that the order of its writes of the record against what
//! it does to the games and the folders is what the tests below follow.
use super::machine::Machine;
use super::record::{self, Claim, Found, GameEnd, Process, Seen, State, AT_ONCE};
use crate::Result;
use eu4_harmony_client::start::{Start, TestGame};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// What a claim came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Claimed {
    /// A test game is this claim's now.
    Now,
    /// This owner held one already.
    Already,
}

/// How a test game is started.
#[derive(Clone, Debug, Default)]
pub struct Starting {
    pub sound: bool,
    pub seed: Option<u32>,
    /// The saves to copy from the player's into the test game's user folder.
    pub saves: Vec<String>,
    /// The developer tools' options to start the core with, in place of this
    /// checkout's `dev.toml`.
    pub options: Option<String>,
    /// Test mods, folders of Harmony mods, put in the test game's playset
    /// (docs/plans/whole-game.md); a start with none takes them out.
    pub mods: Vec<PathBuf>,
    /// The folder of the build to start the core of, in place of this
    /// checkout's last build: a copy of a build of its, made aside so that
    /// another build may follow while this one plays.
    pub build: Option<PathBuf>,
}

/// What this process sees now, for a look at the record.
fn seen<M: Machine>(m: &M) -> Seen<'_> {
    Seen { me: m.me(), alive: Box::new(|p| m.alive(p)) }
}

/// The pipe of `owner`'s test game.
pub fn pipe_of(owner: &str) -> Result<String> {
    eu4_harmony_remote::pipe_of(Some(owner))
}

/// The claim's test game's user folder.
pub fn user_dir(claim: &Claim) -> PathBuf {
    Path::new(&claim.folder).join("user")
}

/// `owner`'s claim, once its folder is made.
pub fn held_by<M: Machine>(m: &M, owner: &str) -> Result<Claim> {
    match m.locked(|s| s.of(owner).cloned())? {
        Some(c) if c.ready => Ok(c),
        Some(_) => Err(format!("{owner}'s claim is being made; cargo xtask game claim")),
        None => Err(format!("{owner} holds no test game; cargo xtask game claim")),
    }
}

/// The process of the claim's test game while it runs: the one it was
/// started as, or the one serving its pipe, which a restart started.
fn game_of<M: Machine>(m: &M, claim: &Claim) -> Option<u32> {
    claim.game.filter(|g| m.alive(g)).map(|g| g.id).or_else(|| m.serving(&pipe_of(&claim.owner).ok()?))
}

/// The process of `owner`'s test game while it runs.
pub fn game_of_owner<M: Machine>(m: &M, owner: &str) -> Option<u32> {
    game_of(m, &held_by(m, owner).ok()?)
}

/// How the games of `owner`'s test game have ended so far, as its follower
/// wrote them; none when `owner` holds no test game.
pub fn ends_of<M: Machine>(m: &M, owner: &str) -> Vec<GameEnd> {
    held_by(m, owner).map(|c| m.ends(Path::new(&c.folder))).unwrap_or_default()
}

/// Claims a test game for `owner`: the record names the claim's folder and
/// the process making it, then the folder is made, and only then is the
/// claim ready. It holds the test game until it is released.
pub fn claim<M: Machine>(m: &M, owner: &str, purpose: &str, game_dir: &Path) -> Result<Claimed> {
    if !m.proxy_loads_cores(game_dir) {
        return Err(format!(
            "the framework's proxy in {} cannot load a test game's core: install a build from test games on (cargo xtask install, with no game running)",
            game_dir.display()
        ));
    }
    let claims = m.claims()?;
    let seen = seen(m);
    let mut folder = PathBuf::new();
    let (found, cut) = m.locked(|s| {
        let (found, cut) = record::look(owner, s, &seen);
        if found == Found::Free {
            let (since, stamp) = m.stamps();
            folder = claims.join(format!("{owner}-{stamp}"));
            s.claims.push(Claim {
                owner: owner.to_string(),
                purpose: purpose.to_string(),
                checkout: crate::repo().display().to_string(),
                since,
                folder: folder.display().to_string(),
                maker: Some(m.me()),
                ready: false,
                game: None,
                starting: None,
            });
        }
        (found, cut)
    })?;
    for c in &cut {
        m.say(&format!("{}'s claim was cut short while it was made; its folder goes", c.owner));
        m.remove_folder(Path::new(&c.folder))?;
    }
    match found {
        Found::Held => {
            m.say(&format!("already held: {}", held_by(m, owner)?.describe()));
            Ok(Claimed::Already)
        }
        Found::BeingMade(why) => Err(why),
        Found::Free => make(m, owner, &folder, game_dir),
    }
}

/// Makes the claim's folder and marks the claim ready; a folder that cannot
/// be made gives the test game back.
fn make<M: Machine>(m: &M, owner: &str, folder: &Path, game_dir: &Path) -> Result<Claimed> {
    if let Err(e) = m.make_folder(folder, game_dir) {
        let _ = m.remove_folder(folder);
        m.locked(|s| s.claims.retain(|c| c.owner != owner))?;
        return Err(format!("the test game's folder could not be made, so it is not claimed: {e}"));
    }
    m.locked(|s| {
        if let Some(c) = s.of_mut(owner) {
            c.ready = true;
            c.maker = None;
        }
    })?;
    m.say(&format!("claimed a test game for {owner}: {}", folder.display()));
    m.say(&format!("  ask it with eu4-harmony game --test-game {owner} ..., or with EU4_HARMONY_TEST_GAME={owner}"));
    Ok(Claimed::Now)
}

/// Ends the game of a claim being released and keeps its logs.
fn end_and_keep<M: Machine>(m: &M, claim: &Claim) -> Result<(Vec<GameEnd>, bool)> {
    let folder = Path::new(&claim.folder);
    if let Some(id) = game_of(m, claim) {
        m.end_game(id)?;
    }
    m.stop_following(folder);
    let ends = m.ends(folder);
    let crashed = m.keep_and_remove(folder)?;
    Ok((ends, crashed))
}

/// How often a start waiting for a place looks whether one is free.
const PLACE_LOOKS_EVERY: Duration = Duration::from_secs(5);

/// The owners of the test games that run or are being started.
fn running<M: Machine>(m: &M, state: &State) -> Vec<String> {
    let runs = |c: &&Claim| game_of(m, c).is_some() || c.starting.is_some_and(|p| m.alive(&p));
    state.claims.iter().filter(runs).map(|c| c.owner.clone()).collect()
}

/// Takes one of the [`AT_ONCE`] places of running test games for `owner`'s
/// start, waiting while the others have them all, until `given_up`.
fn take_place<M: Machine>(m: &M, owner: &str, given_up: &dyn Fn() -> bool) -> Result<()> {
    let me = m.me();
    let mut told = false;
    loop {
        let others = m.locked(|s| {
            let others: Vec<String> = running(m, s).into_iter().filter(|o| o != owner).collect();
            if others.len() < AT_ONCE {
                if let Some(c) = s.of_mut(owner) {
                    c.starting = Some(me);
                }
            }
            others
        })?;
        if others.len() < AT_ONCE {
            return Ok(());
        }
        if given_up() {
            return Err(format!("given up waiting for one of the {AT_ONCE} test games that run at once"));
        }
        if !told {
            m.say(&format!("{AT_ONCE} test games run already ({}); waiting for one of them to end", others.join(", ")));
            told = true;
        }
        m.wait(PLACE_LOOKS_EVERY);
    }
}

/// Starts `owner`'s test game hidden, with this checkout's built core and
/// the saves `starting` names, waiting up to `wait` for its main menu, and
/// names it in the record, followed by `follow.exe`. While [`AT_ONCE`] other
/// test games run, it waits for one of them to end first, until
/// `given_up`. The seconds it took.
pub fn start_game<M: Machine>(m: &M, owner: &str, game_dir: &Path, starting: &Starting, wait: Duration, given_up: &dyn Fn() -> bool) -> Result<f64> {
    let claim = held_by(m, owner)?;
    if let Some(id) = game_of(m, &claim) {
        return Err(format!("{owner}'s test game runs already (eu4.exe {id}); cargo xtask game stop ends it"));
    }
    take_place(m, owner, given_up)?;
    let started = start_in_place(m, owner, &claim, game_dir, starting, wait);
    m.locked(|s| s.of_mut(owner).map(|c| c.starting = None))?;
    started
}

/// Starts the claim's test game, which has a place to run in.
fn start_in_place<M: Machine>(m: &M, owner: &str, claim: &Claim, game_dir: &Path, starting: &Starting, wait: Duration) -> Result<f64> {
    let folder = Path::new(&claim.folder);
    let core = m.prepare_start(folder, game_dir, starting)?;
    let user = user_dir(claim);
    let log = user.join("eu4-harmony").join("logs").join("core.log");
    let test_game = TestGame { name: owner.to_string(), user_dir: user, core };
    let start = Start { hidden: true, sound: starting.sound, seed: starting.seed, test_game: Some(test_game) };
    // A game that ends before its main menu is followed by no `follow.exe`,
    // so the start records its end, for the release to tell of.
    let (id, seconds) = m.start_game(game_dir, &log, &start, wait).map_err(|failed| {
        if let Some(ended) = failed.ended {
            m.record_end(folder, Process { id: ended.id, started: ended.started }, ended.code);
        }
        failed.message
    })?;
    let game = m.process(id);
    m.locked(|s| s.of_mut(owner).map(|c| c.game = game))?;
    if let Err(e) = m.follow(folder, id, &start.pipe()?) {
        m.say(&format!("its end will not be followed: {e}"));
    }
    Ok(seconds)
}

/// Ends `owner`'s test game.
pub fn stop<M: Machine>(m: &M, owner: &str) -> Result<()> {
    let claim = held_by(m, owner)?;
    match game_of(m, &claim) {
        Some(id) => {
            m.end_game(id)?;
            m.say("the test game is closed");
        }
        None => m.say("the test game was not running"),
    }
    Ok(())
}

/// Releases `owner`'s claim: ends its game, keeps its logs, removes the rest
/// of its folder, and only then takes it out of the record, so that a
/// release cut short is done again by the next.
pub fn release<M: Machine>(m: &M, owner: &str) -> Result<()> {
    let claim = m.locked(|s| s.of(owner).cloned())?.ok_or_else(|| format!("{owner} holds no test game"))?;
    let (ends, crashed) = end_and_keep(m, &claim)?;
    m.locked(|s| s.claims.retain(|c| c.owner != owner))?;
    m.say(&format!("released {owner}'s test game, held since {}", claim.since));
    for end in &ends {
        m.say(&format!("  {}", end.describe()));
    }
    let crashed = crashed || ends.iter().any(GameEnd::crashed);
    m.say(if crashed { "  the game crashed" } else { "  the game did not crash" });
    m.say(&format!("  kept: {}", Path::new(&claim.folder).join("kept").display()));
    Ok(())
}

/// Says which test games are held, which `eu4.exe` are no test game's, and
/// what no claim refers to.
pub fn status<M: Machine>(m: &M) -> Result<()> {
    let state: State = m.locked(|s| s.clone())?;
    m.say(&format!("{} test games held, {} of them running, {AT_ONCE} at most", state.claims.len(), running(m, &state).len()));
    let mut theirs = Vec::new();
    for c in &state.claims {
        m.say(&format!("  {}", c.describe()));
        m.say(&format!("    folder: {}", c.folder));
        match game_of(m, c) {
            Some(id) => {
                m.say(&format!("    eu4.exe {id}, on {}", pipe_of(&c.owner)?));
                theirs.push(id);
            }
            None if c.ready => m.say("    not running"),
            None => m.say("    being made"),
        }
    }
    match m.games() {
        Ok(games) => {
            let others: Vec<String> = games.iter().filter(|g| !theirs.contains(g)).map(u32::to_string).collect();
            if !others.is_empty() {
                m.say(&format!("eu4.exe no test game's (the player's, or an older checkout's): {}", others.join(", ")));
            }
        }
        Err(e) => m.say(&format!("the eu4.exe that run cannot be told: {e}")),
    }
    m.leftovers(&state)?.iter().for_each(|line| m.say(line));
    Ok(())
}

/// The claim a run plays in, such as the scenario runner's: one the run
/// made, which it releases when it ends.
#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub owner: String,
}

/// How often a run looks whether it is interrupted.
const RUN_LOOKS_EVERY: Duration = Duration::from_secs(1);

/// Claims a test game for a run of `owner`'s; one `owner` holds already is
/// another's, and refused.
pub fn claim_for_run<M: Machine>(m: &M, owner: &str, purpose: &str, game_dir: &Path) -> Result<Run> {
    if let Ok(held) = held_by(m, owner) {
        return Err(format!("{owner} holds a test game already, so a run cannot claim it: {}", held.describe()));
    }
    claim(m, owner, purpose, game_dir)?;
    Ok(Run { owner: owner.into() })
}

/// Ends the run's test game, if it runs.
pub fn end_run_game<M: Machine>(m: &M, run: &Run) {
    if let Ok(claim) = held_by(m, &run.owner) {
        if let Some(id) = game_of(m, &claim) {
            let _ = m.end_game(id);
        }
    }
}

/// Plays `play` within `run`'s claim, while another thread ends the test
/// game once the run is `interrupted`, so that the step waiting on it ends;
/// then, whatever `play`
/// comes to, an error, a panic or `interrupted`, ends the test game and
/// releases the run's claim. An error comes back with the release's
/// own; a panic goes on once the claim is released.
pub fn play_within<M: Machine + Sync, T>(m: &M, run: &Run, interrupted: &(dyn Fn() -> bool + Sync), play: impl FnOnce() -> Result<T>) -> Result<T> {
    let done = AtomicBool::new(false);
    let played = std::thread::scope(|s| {
        s.spawn(|| {
            let mut ended = false;
            while !done.load(Ordering::SeqCst) {
                if interrupted() && !ended {
                    end_run_game(m, run);
                    ended = true;
                }
                m.wait(RUN_LOOKS_EVERY);
            }
        });
        let played = std::panic::catch_unwind(std::panic::AssertUnwindSafe(play));
        end_run_game(m, run);
        done.store(true, Ordering::SeqCst);
        played
    });
    let given = release(m, &run.owner);
    given_back(played, given)
}

/// What a run came to, `played`, with what releasing its claim came to,
/// `given`: an error comes back with the release's own, and a panic goes on.
fn given_back<T>(played: std::thread::Result<Result<T>>, given: Result<()>) -> Result<T> {
    match (played, given) {
        (Err(panic), given) => {
            if let Err(e) = given {
                eprintln!("{e}");
            }
            std::panic::resume_unwind(panic)
        }
        (Ok(Err(e)), Err(not_given)) => Err(format!("{e}; releasing the test game: {not_given}")),
        (Ok(_), Err(not_given)) => Err(not_given),
        (Ok(played), Ok(())) => played,
    }
}

#[cfg(test)]
mod tests {
    use super::super::recorded::*;
    use super::*;

    /// A claim names its folder and its maker before the folder is made, and
    /// is ready only after; a start copies the core in, starts the test
    /// game and follows it; a release ends it and keeps its logs before the
    /// claim leaves the record.
    #[test]
    fn a_claim_s_course_from_claiming_to_releasing() {
        let m = Recorded::new(State::default());
        assert_eq!(claim(&m, "a", "a run", game_dir()).unwrap(), Claimed::Now);
        start_game(&m, "a", game_dir(), &Starting::default(), Duration::from_secs(300), &|| false).unwrap();
        assert!(start_game(&m, "a", game_dir(), &Starting::default(), Duration::from_secs(300), &|| false).unwrap_err().contains("runs already"));
        release(&m, "a").unwrap();
        let folder = "C:\\claims\\a-1000".to_string();
        assert_eq!(
            m.did(),
            [
                Did::Wrote("a?".into()),
                Did::MakeFolder(folder.clone()),
                Did::Wrote("a".into()),
                Did::Prepare,
                Did::Start,
                Did::Follow(GAME.id),
                Did::End(GAME.id),
                Did::StopFollowing,
                Did::Keep(folder),
                Did::Wrote(String::new()),
            ]
        );
        assert!(m.said().contains("--test-game a"));
    }

    /// A folder that cannot be made gives the test game back.
    #[test]
    fn a_folder_that_cannot_be_made_gives_the_test_game_back() {
        let mut m = Recorded::new(State::default());
        m.fail_folder = true;
        assert!(claim(&m, "a", "", game_dir()).unwrap_err().contains("the disk is full"));
        assert_eq!(m.state.lock().unwrap().claims.len(), 0);
        assert!(m.did().contains(&Did::RemoveFolder("C:\\claims\\a-1000".into())));
    }

    /// No claim while the game folder's proxy cannot load a test game's
    /// core.
    #[test]
    fn a_proxy_from_before_test_games_is_refused() {
        let mut m = Recorded::new(State::default());
        m.proxy = false;
        assert!(claim(&m, "a", "", game_dir()).unwrap_err().contains("cargo xtask install"));
        assert_eq!(m.did(), []);
    }

    /// However many test games are held, a claim gets one at once, and no
    /// other's game is ended for it.
    #[test]
    fn a_claim_gets_a_test_game_however_many_are_held() {
        let claims = (0..10).map(|i| held(&format!("h{i}"))).collect();
        let m = Recorded::new(State { claims });
        m.serving.lock().unwrap().push((pipe_of("h1").unwrap(), 77));
        assert_eq!(claim(&m, "a", "", game_dir()).unwrap(), Claimed::Now);
        assert_eq!(m.state.lock().unwrap().claims.len(), 11);
        assert!(!m.did().iter().any(|d| matches!(d, Did::End(_) | Did::Keep(_) | Did::Waited)));
    }

    /// A game that crashes before its main menu has no follower, so its
    /// start records its end, and the release tells of the crash.
    #[test]
    fn a_crash_before_the_main_menu_is_told_at_the_release() {
        let mut m = Recorded::new(State::default());
        m.ends_before_menu = Some(0xc000_0005);
        claim(&m, "a", "a run", game_dir()).unwrap();
        let error = start_game(&m, "a", game_dir(), &Starting::default(), Duration::from_secs(300), &|| false).unwrap_err();
        assert!(error.contains("before its main menu"));
        release(&m, "a").unwrap();
        assert!(m.did().contains(&Did::RecordEnd(0xc000_0005)));
        assert!(!m.did().contains(&Did::Follow(GAME.id)));
        let said = m.said();
        assert!(said.contains("eu4.exe 40 had ended by 10:00:00: an access violation (exit code 0xc0000005)"), "{said}");
        assert!(said.contains("the game crashed"), "{said}");
    }

    /// Only the claim's own game is ended, never another's.
    #[test]
    fn stopping_ends_only_the_claim_s_own_game() {
        let m = Recorded::new(State { claims: vec![held("a"), held("b")] });
        m.serving.lock().unwrap().extend([(pipe_of("a").unwrap(), 1), (pipe_of("b").unwrap(), 2), (eu4_harmony_remote::PIPE.into(), 3)]);
        stop(&m, "a").unwrap();
        assert_eq!(m.did(), [Did::End(1)]);
        release(&m, "b").unwrap();
        assert!(m.did().contains(&Did::End(2)) && !m.did().contains(&Did::End(3)));
    }

    /// The record with `a` and four others, whose games run as `ids`.
    fn four_running(ids: [u32; 4]) -> Recorded {
        let owners = ["h0", "h1", "h2", "h3"];
        let m = Recorded::new(State { claims: owners.iter().map(|o| held(o)).chain([held("a")]).collect() });
        m.serving.lock().unwrap().extend(owners.iter().zip(ids).map(|(o, id)| (pipe_of(o).unwrap(), id)));
        m
    }

    /// While four other test games run, a start waits, saying for which,
    /// and starts once one of them ends; its place is given up after.
    #[test]
    fn a_start_waits_while_four_test_games_run() {
        let m = four_running([1, 2, 3, 4]);
        m.end_while_waiting.lock().unwrap().push(3);
        start_game(&m, "a", game_dir(), &Starting::default(), Duration::from_secs(300), &|| false).unwrap();
        assert_eq!(m.did()[..3], [Did::Waited, Did::Prepare, Did::Start]);
        assert!(m.said().contains("4 test games run already (h0, h1, h2, h3); waiting"), "{}", m.said());
        assert!(m.state.lock().unwrap().claims.iter().all(|c| c.starting.is_none()));
    }

    /// A start that gives up waiting starts nothing.
    #[test]
    fn a_start_that_gives_up_waiting_starts_nothing() {
        let m = four_running([1, 2, 3, 4]);
        let error = start_game(&m, "a", game_dir(), &Starting::default(), Duration::from_secs(300), &|| true).unwrap_err();
        assert!(error.contains("given up waiting"), "{error}");
        assert!(!m.did().contains(&Did::Start));
    }

    /// A game being started holds its place while the process starting it
    /// runs, and gives it up once that process has ended.
    #[test]
    fn a_game_being_started_holds_its_place_while_its_starter_runs() {
        let m = four_running([1, 2, 3, 4]);
        m.serving.lock().unwrap().retain(|(_, id)| *id != 4);
        m.serving.lock().unwrap().push(("a starter's".into(), 99));
        m.state.lock().unwrap().of_mut("h3").unwrap().starting = Some(Process { id: 99, started: 9 });
        m.end_while_waiting.lock().unwrap().push(99);
        start_game(&m, "a", game_dir(), &Starting::default(), Duration::from_secs(300), &|| false).unwrap();
        assert_eq!(m.did()[..2], [Did::Waited, Did::Prepare]);
    }

    /// A run releases its claim whatever it came to, and claims no test
    /// game its owner holds already.
    #[test]
    fn a_run_releases_its_claim_and_takes_none_held() {
        let m = Recorded::new(State::default());
        let run = claim_for_run(&m, "a", "scenarios", game_dir()).unwrap();
        let failed: Result<()> = play_within(&m, &run, &|| false, || Err("a check failed".into()));
        assert_eq!(failed.unwrap_err(), "a check failed");
        assert!(m.state.lock().unwrap().claims.is_empty());

        let m = Recorded::new(State { claims: vec![held("a")] });
        assert!(claim_for_run(&m, "a", "scenarios", game_dir()).unwrap_err().contains("holds a test game already"));
        assert_eq!(m.state.lock().unwrap().claims.len(), 1);
    }
}
