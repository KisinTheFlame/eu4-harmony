//! The claims of test games and the queue, as data, and what a claim finds
//! when it looks at them: the decisions, without the files, the mutex or the
//! clock (docs/test-games.md).
//!
//! The record is `claims.json`: each claim, with its test game's folder, and
//! the queue. A checkout from before test games knows only `game-lock.json`,
//! its claim of the whole game; [`OLD_HOLDER`] is the holder that file names
//! while test games are held, so that such a checkout waits, and a record of
//! its own there, a claim that backed up the player's files, keeps every
//! test game waiting until it is released.
use serde_json::{json, Value};

/// A process, by its id and when it started: a later process given the same
/// id is not it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Process {
    pub id: u32,
    /// When it started, as Windows counts it.
    pub started: u64,
}

impl Process {
    fn from_json(value: &Value) -> Option<Self> {
        Some(Self { id: u32::try_from(value["id"].as_u64()?).ok()?, started: value["started"].as_u64()? })
    }

    fn to_json(self) -> Value {
        json!({ "id": self.id, "started": self.started })
    }
}

/// How one of a test game's games ended, as the process that followed it
/// saw: when it saw it had ended, and the game's exit code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameEnd {
    pub game: Process,
    /// The local time, `HH:MM:SS`.
    pub seen: String,
    pub code: u32,
}

impl GameEnd {
    /// What the exit code says, and whether it is a crash: the game exits
    /// with 0 when it quits, these commands end it with 1, the framework
    /// after a fatal error, and the proxy when the core it was told to load
    /// does not, with 3; Windows ends a process that faults with the fault's
    /// code, which a fault that bypasses the game's crash handler leaves no
    /// crash folder for.
    pub fn meaning(&self) -> (&'static str, bool) {
        match self.code {
            0 => ("it quit, or restarted", false),
            1 => ("ended by game stop, a release or a run", false),
            3 => ("the framework ended it after a fatal error, which core.log names, or its core did not load (proxy.log)", true),
            0xc000_0005 => ("an access violation", true),
            0xc000_0374 => ("heap corruption", true),
            0xc000_0409 => ("a fast fail: a failed check, or a stack buffer overrun", true),
            0xc000_00fd => ("a stack overflow", true),
            _ => ("an exit code these commands do not know", true),
        }
    }

    pub fn crashed(&self) -> bool {
        self.meaning().1
    }

    /// `eu4.exe 1234 had ended by 03:12:41: heap corruption (exit code
    /// 0xc0000374)`.
    pub fn describe(&self) -> String {
        format!("eu4.exe {} had ended by {}: {} (exit code 0x{:x})", self.game.id, self.seen, self.meaning().0, self.code)
    }

    /// Its line in the test game's record of its games' ends: the process's
    /// id and start, when it was seen ended, and the exit code.
    pub fn to_line(&self) -> String {
        format!("{}\t{}\t{}\t0x{:x}", self.game.id, self.game.started, self.seen, self.code)
    }

    pub fn from_line(line: &str) -> Option<Self> {
        let mut fields = line.split('\t');
        let id = fields.next()?.parse().ok()?;
        let started = fields.next()?.parse().ok()?;
        let seen = fields.next()?.to_string();
        let code = u32::from_str_radix(fields.next()?.trim().trim_start_matches("0x"), 16).ok()?;
        Some(Self { game: Process { id, started }, seen, code })
    }
}

/// What a look knows of the computer besides the record.
pub struct Seen<'a> {
    pub now: u64,
    /// The process that looks.
    pub me: Process,
    /// Whether a process recorded runs still.
    pub alive: Box<dyn Fn(&Process) -> bool + 'a>,
}

/// How many test games run at once at most: what a game takes in the late
/// game, about 6 GB of memory and 1.5 GB of the graphics card's, three times
/// over beside the player's own (docs/research/several-games.md).
pub const CAPACITY: usize = 3;
/// A waiting claim that has not said it still waits for this long has gone.
pub const WAITING_GONE_SECONDS: u64 = 60;
/// The holder `game-lock.json` names while test games are held, for the
/// checkouts from before them.
pub const OLD_HOLDER: &str = "test-games";

/// One claim of a test game.
#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    pub owner: String,
    pub purpose: String,
    pub checkout: String,
    /// When it was claimed, `yyyy-MM-dd HH:mm`.
    pub since: String,
    /// When the lease started last, in seconds since 1970.
    pub renewed: u64,
    pub lease_minutes: u64,
    /// The claim's folder, with the test game's user folder and core.
    pub folder: String,
    /// The process making the claim's folder while it is not `ready`: a
    /// claim whose process ended before then is made again.
    pub maker: Option<Process>,
    pub ready: bool,
    /// The test game's process, once it is started.
    pub game: Option<Process>,
}

impl Claim {
    pub fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().map(String::from);
        Some(Self {
            owner: text("owner")?,
            purpose: text("purpose").unwrap_or_default(),
            checkout: text("checkout").unwrap_or_default(),
            since: text("since").unwrap_or_default(),
            renewed: value["renewed"].as_u64()?,
            lease_minutes: value["leaseMinutes"].as_u64().unwrap_or(30),
            folder: text("folder")?,
            maker: Process::from_json(&value["maker"]),
            ready: value["ready"].as_bool().unwrap_or(false),
            game: Process::from_json(&value["game"]),
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "owner": self.owner,
            "purpose": self.purpose,
            "checkout": self.checkout,
            "since": self.since,
            "renewed": self.renewed,
            "leaseMinutes": self.lease_minutes,
            "folder": self.folder,
            "maker": self.maker.map(Process::to_json),
            "ready": self.ready,
            "game": self.game.map(Process::to_json),
        })
    }

    /// When the lease runs out, in seconds since 1970.
    pub fn lease_ends(&self) -> u64 {
        self.renewed + 60 * self.lease_minutes
    }

    /// A claim cut short while its folder was made: its process has ended.
    pub fn cut_short(&self, seen: &Seen) -> bool {
        !self.ready && self.maker.is_none_or(|p| p != seen.me && !(seen.alive)(&p))
    }

    /// Who holds the test game, for what, and its lease, in a line.
    pub fn describe(&self, now: u64) -> String {
        let left = self.lease_ends() as i64 - now as i64;
        let lease = if left > 0 { format!("lease ends in {} min", (left + 59) / 60) } else { format!("lease ran out {} min ago", -left / 60) };
        format!("{} since {} ({}); {lease}", self.owner, self.since, self.purpose)
    }
}

/// One claim waiting in the queue.
#[derive(Clone, Debug, PartialEq)]
pub struct Waiting {
    pub owner: String,
    /// When it asked, in seconds since 1970, which orders the queue.
    pub asked: u64,
    /// When it last said it still waits.
    pub seen: u64,
}

impl Waiting {
    pub fn from_json(value: &Value) -> Option<Self> {
        Some(Self { owner: value["owner"].as_str()?.to_string(), asked: value["asked"].as_u64()?, seen: value["seen"].as_u64()? })
    }

    pub fn to_json(&self) -> Value {
        json!({ "owner": self.owner, "asked": self.asked, "seen": self.seen })
    }
}

/// The claims and the queue, and the claim of the whole game a checkout from
/// before test games holds, as its own line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub claims: Vec<Claim>,
    pub waiting: Vec<Waiting>,
    pub old: Option<String>,
}

impl State {
    pub fn from_json(value: &Value) -> Self {
        let list = |key: &str| value[key].as_array().cloned().unwrap_or_default();
        Self {
            claims: list("claims").iter().filter_map(Claim::from_json).collect(),
            waiting: list("waiting").iter().filter_map(Waiting::from_json).collect(),
            old: None,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "claims": self.claims.iter().map(Claim::to_json).collect::<Vec<_>>(),
            "waiting": self.waiting.iter().map(Waiting::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn of(&self, owner: &str) -> Option<&Claim> {
        self.claims.iter().find(|c| c.owner == owner)
    }

    pub fn of_mut(&mut self, owner: &str) -> Option<&mut Claim> {
        self.claims.iter_mut().find(|c| c.owner == owner)
    }
}

/// What `game-lock.json` holds while test games are held: a holder that
/// checkouts from before them take for one with a lease of a year and its
/// backup done, so that they wait.
pub fn old_holder(now: u64, since: &str) -> Value {
    json!({
        "owner": OLD_HOLDER,
        "purpose": "test games run side by side; update this checkout from master to claim one (docs/test-games.md)",
        "checkout": "",
        "since": since,
        "renewed": now,
        "leaseMinutes": 525_600,
        "backup": "none: test games touch none of the player's files",
    })
}

/// The claim of the whole game `game-lock.json` holds, when it is a
/// checkout's from before test games and not [`OLD_HOLDER`]: who holds it,
/// in a line.
pub fn old_claim(value: &Value) -> Option<String> {
    let owner = value["owner"].as_str()?;
    (owner != OLD_HOLDER).then(|| {
        format!(
            "{owner}, a checkout from before test games, holds the whole game ({}); it backed up the player's files, and its own release puts them back",
            value["purpose"].as_str().unwrap_or("")
        )
    })
}

/// What a claim asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct Asking {
    pub owner: String,
    /// It waits in the queue for a test game.
    pub wait: bool,
}

/// What a claim finds.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// It holds a test game already.
    Held,
    /// Every test game is held, or others wait first, or a checkout from
    /// before test games holds the whole game.
    Busy(String),
    /// A test game is its, to make.
    Free,
    /// A test game is its once this claim, whose lease ran out, is taken
    /// from its holder, its game ended and its logs kept, by this look's
    /// process; the claim is out of the record already.
    Take(Claim),
}

/// The queue, in the order they asked; those that have gone are left out.
pub fn tidy(queue: &mut Vec<Waiting>, now: u64) {
    queue.retain(|w| now.saturating_sub(w.seen) <= WAITING_GONE_SECONDS);
    queue.sort_by_key(|w| w.asked);
}

/// What `asking` finds, the record as it is; changes it as it does: renews
/// its own claim, puts itself in the queue or takes itself out, and takes
/// out the claims cut short while they were made, which it returns for
/// their folders to be removed, and a claim it takes over.
pub fn look(asking: &Asking, state: &mut State, seen: &Seen) -> (Found, Vec<Claim>) {
    let now = seen.now;
    tidy(&mut state.waiting, now);
    let (cut, kept): (Vec<Claim>, Vec<Claim>) = state.claims.drain(..).partition(|c| c.cut_short(seen));
    state.claims = kept;
    if let Some(own) = state.of_mut(&asking.owner) {
        if !own.ready {
            return (Found::Busy(format!("{}'s claim is being made by process {}", own.owner, own.maker.map_or(0, |p| p.id))), cut);
        }
        own.renewed = now;
        state.waiting.retain(|w| w.owner != asking.owner);
        return (Found::Held, cut);
    }
    if asking.wait {
        match state.waiting.iter_mut().find(|w| w.owner == asking.owner) {
            Some(w) => w.seen = now,
            None => state.waiting.push(Waiting { owner: asking.owner.clone(), asked: now, seen: now }),
        }
        tidy(&mut state.waiting, now);
    }
    if let Some(old) = &state.old {
        return (Found::Busy(old.clone()), cut);
    }
    let first = state.waiting.first().map(|w| w.owner.clone());
    if let Some(first) = first.filter(|first| *first != asking.owner) {
        return (Found::Busy(format!("{first} waits first")), cut);
    }
    if state.claims.len() < CAPACITY {
        state.waiting.retain(|w| w.owner != asking.owner);
        return (Found::Free, cut);
    }
    let longest = state.claims.iter().enumerate().filter(|(_, c)| c.ready && now > c.lease_ends()).min_by_key(|(_, c)| c.lease_ends()).map(|(i, _)| i);
    match longest {
        Some(i) => {
            state.waiting.retain(|w| w.owner != asking.owner);
            (Found::Take(state.claims.remove(i)), cut)
        }
        None => {
            let held: Vec<String> = state.claims.iter().map(|c| c.describe(now)).collect();
            (Found::Busy(format!("all {CAPACITY} test games are held: {}", held.join("; "))), cut)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: Process = Process { id: 10, started: 1 };
    const OTHER: Process = Process { id: 20, started: 2 };
    const GONE: Process = Process { id: 30, started: 3 };

    fn seen(now: u64) -> Seen<'static> {
        Seen { now, me: ME, alive: Box::new(|p| *p != GONE) }
    }

    fn claim(owner: &str, renewed: u64) -> Claim {
        Claim {
            owner: owner.into(),
            purpose: "a run".into(),
            checkout: "D:\\c".into(),
            since: "2026-11-08 10:00".into(),
            renewed,
            lease_minutes: 30,
            folder: format!("C:\\claims\\{owner}"),
            maker: None,
            ready: true,
            game: None,
        }
    }

    fn asking(owner: &str, wait: bool) -> Asking {
        Asking { owner: owner.into(), wait }
    }

    fn state(claims: Vec<Claim>) -> State {
        State { claims, ..State::default() }
    }

    #[test]
    fn a_game_end_says_its_exit_code_and_whether_it_crashed() {
        let end = |code| GameEnd { game: Process { id: 1234, started: 99 }, seen: "03:12:41".into(), code };
        assert_eq!(end(0xc000_0374).describe(), "eu4.exe 1234 had ended by 03:12:41: heap corruption (exit code 0xc0000374)");
        assert!(end(0xc000_0374).crashed() && end(3).crashed() && end(0x1234).crashed());
        assert!(!end(0).crashed() && !end(1).crashed());
        assert_eq!(GameEnd::from_line(&end(0xc000_0005).to_line()), Some(end(0xc000_0005)));
        assert_eq!(GameEnd::from_line("not a line"), None);
    }

    #[test]
    fn the_record_reads_as_it_is_written() {
        let mut held = claim("a", 100);
        held.game = Some(OTHER);
        held.maker = Some(ME);
        let s = State { claims: vec![held], waiting: vec![Waiting { owner: "b".into(), asked: 5, seen: 6 }], old: None };
        assert_eq!(State::from_json(&s.to_json()), s);
    }

    /// Test games go to whoever asks, up to the capacity; then a claim is
    /// busy, and one that waits gets the first test game freed, before those
    /// who asked after it.
    #[test]
    fn test_games_go_to_the_first_who_ask_up_to_the_capacity() {
        let mut s = state((0..CAPACITY - 1).map(|i| claim(&format!("h{i}"), 1000)).collect());
        assert_eq!(look(&asking("a", false), &mut s, &seen(1000)).0, Found::Free);
        s.claims.push(claim("a", 1000));
        assert_eq!(look(&asking("a", false), &mut s, &seen(1010)).0, Found::Held);
        assert_eq!(s.of("a").unwrap().renewed, 1010);
        assert!(matches!(look(&asking("b", true), &mut s, &seen(1020)).0, Found::Busy(why) if why.contains("are held")));
        assert!(matches!(look(&asking("c", true), &mut s, &seen(1021)).0, Found::Busy(_)));
        assert!(matches!(look(&asking("d", false), &mut s, &seen(1022)).0, Found::Busy(_)));
        s.claims.retain(|c| c.owner != "h0");
        assert!(matches!(look(&asking("c", true), &mut s, &seen(1025)).0, Found::Busy(why) if why == "b waits first"));
        assert_eq!(look(&asking("b", true), &mut s, &seen(1026)).0, Found::Free);
        assert_eq!(s.waiting.iter().map(|w| w.owner.as_str()).collect::<Vec<_>>(), ["c"]);
    }

    /// A lease that ran out costs its holder nothing while a test game is
    /// free; once a claim waits with none free, it takes the one whose lease
    /// ran out longest ago.
    #[test]
    fn a_lease_run_out_is_taken_only_when_a_claim_needs_its_test_game() {
        let mut s = state(vec![claim("old", 0), claim("older", 0)]);
        s.claims[1].lease_minutes = 10;
        assert_eq!(look(&asking("a", false), &mut s, &seen(5000)).0, Found::Free);
        assert_eq!(s.claims.len(), 2);
        s.claims.push(claim("fresh", 5000));
        match look(&asking("a", false), &mut s, &seen(5000)).0 {
            Found::Take(taken) => assert_eq!(taken.owner, "older"),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.claims.iter().map(|c| c.owner.as_str()).collect::<Vec<_>>(), ["old", "fresh"]);
    }

    /// A claim whose process ended before its folder was made goes, and its
    /// folder is to be removed; one whose process still makes it keeps its
    /// owner waiting.
    #[test]
    fn a_claim_cut_short_while_it_was_made_goes() {
        let mut making = claim("a", 100);
        (making.ready, making.maker) = (false, Some(OTHER));
        let mut cut = claim("b", 100);
        (cut.ready, cut.maker) = (false, Some(GONE));
        let mut s = state(vec![making, cut]);
        let (found, gone) = look(&asking("a", false), &mut s, &seen(110));
        assert!(matches!(found, Found::Busy(why) if why.contains("being made by process 20")));
        assert_eq!(gone.iter().map(|c| c.owner.as_str()).collect::<Vec<_>>(), ["b"]);
        assert_eq!(s.claims.len(), 1);
    }

    /// A claim of the whole game by a checkout from before test games keeps
    /// every test game waiting; the holder test games write for those
    /// checkouts is not one.
    #[test]
    fn a_whole_game_claim_of_an_older_checkout_keeps_test_games_waiting() {
        assert_eq!(old_claim(&old_holder(100, "now")), None);
        let theirs = json!({ "owner": "checkout", "purpose": "perf", "renewed": 1 });
        let line = old_claim(&theirs).unwrap();
        let mut s = State { old: Some(line.clone()), ..State::default() };
        assert_eq!(look(&asking("a", true), &mut s, &seen(100)).0, Found::Busy(line));
        assert_eq!(s.waiting.len(), 1);
    }

    #[test]
    fn a_waiting_claim_that_stopped_saying_so_leaves_the_queue() {
        let mut s = state((0..CAPACITY).map(|i| claim(&format!("h{i}"), 10_000)).collect());
        let _ = look(&asking("gone", true), &mut s, &seen(1000));
        assert!(matches!(look(&asking("b", true), &mut s, &seen(1030)).0, Found::Busy(why) if why == "gone waits first"));
        assert!(matches!(look(&asking("b", true), &mut s, &seen(1061)).0, Found::Busy(why) if why.contains("are held")));
        assert_eq!(s.waiting.iter().map(|w| w.owner.as_str()).collect::<Vec<_>>(), ["b"]);
    }
}
