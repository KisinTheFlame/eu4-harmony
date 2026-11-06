//! The claim's record and the queue, as data, and what a claim finds when it
//! looks at them: the decisions, without the files, the mutex or the clock.
//!
//! The record is the JSON the game-lock skill's script kept too (`owner`,
//! `purpose`, `checkout`, `since`, `renewed`, `leaseMinutes`, `backup`), so
//! that a checkout from before these commands, still on the script, and one
//! on them never hold the game at once; the fields it does not know (`folder`,
//! `player`, `taken`, `preparing`, `puttingBack`, `keeper`, `game`) it keeps as
//! they are. xtask from before `preparing`, `puttingBack`, `keeper` and `game`
//! drops them when it writes the record, as it does while it waits in the
//! queue, so a record may lose them while it is held.
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

/// What a look knows of the computer besides the record and the queue.
pub struct Seen<'a> {
    pub now: u64,
    /// An `eu4.exe` runs, or it cannot be told whether one does.
    pub game_running: bool,
    /// The ids of the `eu4.exe` that run, as far as they can be told.
    pub games: Vec<u32>,
    /// The process that looks.
    pub me: Process,
    /// Whether a process recorded runs still.
    pub alive: Box<dyn Fn(&Process) -> bool + 'a>,
}

/// The player's claim lasts this long without a renewal: ten hours.
pub const PLAYER_LEASE_MINUTES: u64 = 600;
/// A waiting claim that has not said it still waits for this long has gone.
pub const WAITING_GONE_SECONDS: u64 = 60;
/// How long a holder has, once the player takes the game, to keep what it
/// needs.
pub const TAKEN_GRACE_SECONDS: u64 = 120;

/// Who holds the game.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub owner: String,
    pub purpose: String,
    pub checkout: String,
    pub since: String,
    /// When the lease started last, in seconds since 1970.
    pub renewed: u64,
    pub lease_minutes: u64,
    /// The player's files backed up, once the backup is complete.
    pub backup: Option<String>,
    /// The claim's folder; none for the player's, or a claim the script made.
    pub folder: Option<String>,
    pub player: bool,
    /// When the player asked to take the game from this holder.
    pub taken: Option<u64>,
    /// The process making the claim's backup. A claim whose backup is not
    /// done is not one to use: it is made again, or released with nothing
    /// to put back.
    pub preparing: Option<Process>,
    /// The process putting back what the claim's run changed: while it
    /// runs, nothing else puts it back.
    pub putting_back: Option<Process>,
    /// The process keeping the claim going: its `watch`. While it runs, the
    /// game running keeps the lease going.
    pub keeper: Option<Process>,
    /// The claim's game: the one `eu4.exe` its keeper found running, followed
    /// to the process it starts in its place.
    pub game: Option<Process>,
}

impl Record {
    pub fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().map(String::from);
        Some(Self {
            owner: text("owner")?,
            purpose: text("purpose").unwrap_or_default(),
            checkout: text("checkout").unwrap_or_default(),
            since: text("since").unwrap_or_default(),
            renewed: value["renewed"].as_u64()?,
            lease_minutes: value["leaseMinutes"].as_u64().unwrap_or(30),
            backup: text("backup"),
            folder: text("folder"),
            player: value["player"].as_bool().unwrap_or(false),
            taken: value["taken"].as_u64(),
            preparing: Process::from_json(&value["preparing"]),
            putting_back: Process::from_json(&value["puttingBack"]),
            keeper: Process::from_json(&value["keeper"]),
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
            "backup": self.backup,
            "folder": self.folder,
            "player": self.player,
            "taken": self.taken,
            "preparing": self.preparing.map(Process::to_json),
            "puttingBack": self.putting_back.map(Process::to_json),
            "keeper": self.keeper.map(Process::to_json),
            "game": self.game.map(Process::to_json),
        })
    }

    /// When the lease runs out, in seconds since 1970.
    pub fn lease_ends(&self) -> u64 {
        self.renewed + 60 * self.lease_minutes
    }

    /// A holder that let its lease run out has gone: with the game closed,
    /// or with only the claim's own game running and nothing that keeps the
    /// claim going, its `watch`, running any more. A game that runs and is
    /// not the claim's, the player's, keeps the holder until it closes.
    pub fn stale(&self, seen: &Seen) -> bool {
        seen.now > self.lease_ends() && (!seen.game_running || self.only_its_game_runs(seen) && !self.kept(seen))
    }

    /// Its keeper runs.
    pub fn kept(&self, seen: &Seen) -> bool {
        self.keeper.is_some_and(|k| (seen.alive)(&k))
    }

    /// Every `eu4.exe` that runs is the claim's game.
    pub fn only_its_game_runs(&self, seen: &Seen) -> bool {
        self.game.is_some_and(|g| (seen.alive)(&g) && !seen.games.is_empty() && seen.games.iter().all(|&id| id == g.id))
    }

    /// A claim of a session's whose backup is not done: its process made the
    /// record and was backing up, or was cut short doing it. The player's
    /// claim has no backup.
    pub fn unfinished(&self) -> bool {
        !self.player && self.backup.is_none()
    }

    /// Another process that runs makes this claim's backup.
    pub fn made_by_another(&self, seen: &Seen) -> Option<Process> {
        self.preparing.filter(|p| self.unfinished() && *p != seen.me && (seen.alive)(p))
    }

    /// The same claim as `other`: the same owner, from the same time, with
    /// the same folder.
    pub fn same_claim(&self, other: &Record) -> bool {
        self.owner == other.owner && self.since == other.since && self.folder == other.folder
    }

    /// Another process that runs puts this claim back.
    pub fn put_back_by_another(&self, seen: &Seen) -> Option<Process> {
        self.putting_back.filter(|p| *p != seen.me && (seen.alive)(p))
    }

    /// Who holds the game, for what, and its lease, in a line.
    pub fn describe(&self, now: u64) -> String {
        let left = self.lease_ends() as i64 - now as i64;
        let lease = if left > 0 { format!("lease ends in {} min", (left + 59) / 60) } else { format!("lease ran out {} min ago", -left / 60) };
        let who = if self.player { format!("{} (the player)", self.owner) } else { self.owner.clone() };
        format!("{who} since {} ({}); {lease}", self.since, self.purpose)
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
    pub player: bool,
}

impl Waiting {
    pub fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            owner: value["owner"].as_str()?.to_string(),
            asked: value["asked"].as_u64()?,
            seen: value["seen"].as_u64()?,
            player: value["player"].as_bool().unwrap_or(false),
        })
    }

    pub fn to_json(&self) -> Value {
        json!({ "owner": self.owner, "asked": self.asked, "seen": self.seen, "player": self.player })
    }
}

/// The queue: the player first, then in the order they asked; those that
/// have gone are left out.
pub fn tidy(queue: &mut Vec<Waiting>, now: u64) {
    queue.retain(|w| now.saturating_sub(w.seen) <= WAITING_GONE_SECONDS);
    queue.sort_by_key(|w| (!w.player, w.asked));
}

/// What a claim asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct Asking {
    pub owner: String,
    pub player: bool,
    /// The player takes the game from its holder.
    pub now: bool,
    /// It waits in the queue for the game.
    pub wait: bool,
}

/// What a claim finds.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// It holds the game already.
    Held,
    /// The game is someone else's, or others wait first.
    Busy(String),
    /// The game is its: nobody had it.
    Free,
    /// The holder whose record this is has gone: its claim is to be put back
    /// for it, by this look's process alone, before the game is anyone's.
    Gone(Record),
    /// The player takes the game from this holder, whose grace ran out: its
    /// claim is to be released for it, by this look's process alone.
    Take(Record),
}

/// What `asking` finds, the record and the queue as they are; changes them
/// as it does: renews its own claim, puts itself in the queue or takes it
/// out, marks a holder the player takes the game from, and marks a claim it
/// is to put back as its own to put back.
pub fn look(asking: &Asking, held: &mut Option<Record>, queue: &mut Vec<Waiting>, seen: &Seen) -> Found {
    let (now, game_running) = (seen.now, seen.game_running);
    tidy(queue, now);
    if let Some(h) = held.as_ref() {
        if let Some(p) = h.put_back_by_another(seen) {
            if asking.wait {
                wait_in(queue, asking, now);
            }
            return Found::Busy(format!("{}; process {} puts back what its run changed", h.describe(now), p.id));
        }
    }
    if let Some((who, p)) = held.as_ref().and_then(|h| Some((h.owner.clone(), h.made_by_another(seen)?))) {
        if asking.wait {
            wait_in(queue, asking, now);
        }
        return Found::Busy(format!("{who}, whose claim is being made: process {} backs up the player's files", p.id));
    }
    if let Some(h) = held.as_mut().filter(|h| h.owner == asking.owner) {
        // Its own claim cut short while it was made: what it made goes, and
        // it is made again.
        if h.unfinished() {
            h.putting_back = Some(seen.me);
            return Found::Gone(h.clone());
        }
        h.renewed = now;
        queue.retain(|w| w.owner != asking.owner);
        return Found::Held;
    }
    // Every look at a claim kept going while the game runs starts its lease
    // again.
    if let Some(h) = held.as_mut().filter(|h| game_running && h.kept(seen)) {
        h.renewed = now;
    }
    if asking.wait {
        wait_in(queue, asking, now);
    }
    let first = queue.first().map(|w| w.owner.clone());
    let others_first = first.as_ref().is_some_and(|owner| *owner != asking.owner);
    match held.clone() {
        // A claim cut short while it was made has gone, whatever its lease.
        Some(h) if !(h.stale(seen) || h.unfinished() && h.preparing.is_some()) => {
            if asking.player && asking.now && !h.player {
                let h = held.as_mut().expect("held");
                let taken = *h.taken.get_or_insert(now);
                if now >= taken + TAKEN_GRACE_SECONDS {
                    h.putting_back = Some(seen.me);
                    return Found::Take(h.clone());
                }
                return Found::Busy(format!(
                    "{}; the player takes the game {TAKEN_GRACE_SECONDS} s after asking, once it has kept what it needs",
                    h.describe(now)
                ));
            }
            Found::Busy(h.describe(now))
        }
        None if game_running => Found::Busy("nobody, but eu4.exe is running: the player has it".into()),
        gone if others_first && !(asking.player && asking.now) => {
            let first = first.expect("someone waits");
            Found::Busy(match gone {
                Some(h) => format!("{}, gone; {first} waits first", h.describe(now)),
                None => format!("nobody, but {first} waits first"),
            })
        }
        Some(_) => {
            let h = held.as_mut().expect("held");
            h.putting_back = Some(seen.me);
            Found::Gone(h.clone())
        }
        None => {
            queue.retain(|w| w.owner != asking.owner);
            Found::Free
        }
    }
}

/// Puts `asking` in the queue, or says it still waits there.
fn wait_in(queue: &mut Vec<Waiting>, asking: &Asking, now: u64) {
    match queue.iter_mut().find(|w| w.owner == asking.owner) {
        Some(w) => w.seen = now,
        None => queue.push(Waiting { owner: asking.owner.clone(), asked: now, seen: now, player: asking.player }),
    }
    tidy(queue, now);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(owner: &str, renewed: u64) -> Record {
        Record {
            owner: owner.into(),
            purpose: "a test".into(),
            checkout: String::new(),
            since: String::new(),
            renewed,
            lease_minutes: 30,
            backup: Some("backup".into()),
            folder: None,
            player: false,
            taken: None,
            preparing: None,
            putting_back: None,
            keeper: None,
            game: None,
        }
    }

    /// `owner`'s claim, whose backup `process` makes, or made until it was
    /// cut short.
    fn unfinished(owner: &str, renewed: u64, process: Process) -> Record {
        Record { backup: None, preparing: Some(process), ..record(owner, renewed) }
    }

    /// The process that looks in these tests, and two others.
    const ME: Process = Process { id: 10, started: 1 };
    const HOLDER: Process = Process { id: 20, started: 2 };
    const DEAD: Process = Process { id: 30, started: 3 };
    /// The claim's game, which runs.
    const GAME: Process = Process { id: 40, started: 4 };

    fn alive(p: &Process) -> bool {
        *p != DEAD
    }

    /// What the looking process sees; a game that runs is not a claim's.
    fn at(now: u64, game_running: bool) -> Seen<'static> {
        Seen { now, game_running, games: if game_running { vec![99] } else { Vec::new() }, me: ME, alive: Box::new(alive) }
    }

    /// What it sees with these `eu4.exe` running.
    fn with_games(now: u64, games: &[u32]) -> Seen<'static> {
        Seen { games: games.to_vec(), ..at(now, !games.is_empty()) }
    }

    /// What `process` sees.
    fn by(process: Process, now: u64, game_running: bool) -> Seen<'static> {
        Seen { me: process, ..at(now, game_running) }
    }

    fn asking(owner: &str, wait: bool) -> Asking {
        Asking { owner: owner.into(), player: false, now: false, wait }
    }

    fn player(now: bool) -> Asking {
        Asking { owner: "player".into(), player: true, now, wait: true }
    }

    #[test]
    fn the_record_reads_as_the_script_writes_it_and_keeps_what_it_adds() {
        let script =
            json!({"owner": "a", "purpose": "p", "checkout": "D:\\a", "since": "2026-11-05 10:00", "renewed": 100, "leaseMinutes": 30, "backup": null});
        let read = Record::from_json(&script).unwrap();
        assert_eq!((read.owner.as_str(), read.renewed, read.lease_minutes, read.backup.clone(), read.player), ("a", 100, 30, None, false));
        let mut ours = read.clone();
        ours.folder = Some("D:\\claims\\a".into());
        ours.taken = Some(150);
        ours.putting_back = Some(HOLDER);
        assert_eq!(Record::from_json(&ours.to_json()).unwrap(), ours);
    }

    /// A claim whose runner or `watch` ended, cut short, keeps its lease no
    /// longer while its game runs: once the lease runs out, the next claim
    /// takes over, the claim's game ended. A game that is not the claim's,
    /// as one the player started, keeps it until it closes.
    #[test]
    fn a_claim_left_with_its_game_running_goes_stale() {
        let late = 100 + 30 * 60 + 1;
        let left = || Some(Record { keeper: Some(DEAD), game: Some(GAME), ..record("a", 100) });
        let mut queue = Vec::new();
        let mut held = left();
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &with_games(200, &[GAME.id])), Found::Busy(_)));
        assert_eq!(held.as_ref().unwrap().renewed, 100, "nothing keeps it going");
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &with_games(late, &[GAME.id])), Found::Gone(h) if h.owner == "a"));
        let mut held = left();
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &with_games(late, &[GAME.id, 41])), Found::Busy(_)));
        let mut held = Some(Record { keeper: None, ..left().unwrap() });
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &with_games(late, &[GAME.id])), Found::Gone(_)), "a claim no watch kept");
        // One kept going is not.
        let mut held = Some(Record { keeper: Some(HOLDER), ..left().unwrap() });
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &with_games(late, &[GAME.id])), Found::Busy(_)));
        assert_eq!(held.unwrap().renewed, late);
    }

    /// A claim whose backup is not done is never held: its own owner, asking
    /// again once it was cut short, makes it afresh, another takes over from
    /// it at once, and while its process still backs up, everyone waits.
    #[test]
    fn a_claim_without_its_backup_is_not_held() {
        let mut queue = Vec::new();
        let mut held = Some(unfinished("a", 100, DEAD));
        assert!(matches!(look(&asking("a", false), &mut held, &mut queue, &at(101, false)), Found::Gone(h) if h.owner == "a"));
        assert_eq!(held.as_ref().unwrap().putting_back, Some(ME));
        let mut held = Some(unfinished("a", 100, DEAD));
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &at(101, false)), Found::Gone(h) if h.owner == "a"), "gone at once");
        let mut held = Some(unfinished("a", 100, HOLDER));
        let late = 100 + 30 * 60 + 1;
        assert!(matches!(look(&asking("a", false), &mut held, &mut queue, &at(late, false)), Found::Busy(why) if why.contains("being made")));
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &at(late, false)), Found::Busy(why) if why.contains("being made")));
    }

    /// A holder gone keeps its record, marked as the process's that puts it
    /// back, while that process runs; one whose put back ended with its
    /// process is put back by the next.
    #[test]
    fn a_gone_holder_keeps_its_record_marked_while_it_is_put_back() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        let late = 100 + 30 * 60 + 1;
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &at(late, false)), Found::Gone(h) if h.owner == "a"));
        assert_eq!(held.as_ref().unwrap().owner, "a");
        assert_eq!(held.as_ref().unwrap().putting_back, Some(ME));
        assert!(matches!(look(&asking("c", false), &mut held, &mut queue, &by(HOLDER, late, false)), Found::Busy(_)));
        held.as_mut().unwrap().putting_back = Some(DEAD);
        assert!(matches!(look(&asking("c", false), &mut held, &mut queue, &by(HOLDER, late, false)), Found::Gone(h) if h.putting_back == Some(HOLDER)));
    }

    #[test]
    fn a_free_game_goes_to_the_first_in_the_queue_only() {
        let (mut held, mut queue) = (None, Vec::new());
        assert_eq!(look(&asking("a", true), &mut held, &mut queue, &at(100, false)), Found::Free);
        let mut held = Some(record("a", 100));
        assert!(matches!(look(&asking("b", true), &mut held, &mut queue, &at(101, false)), Found::Busy(_)));
        assert!(matches!(look(&asking("c", true), &mut held, &mut queue, &at(102, false)), Found::Busy(_)));
        assert_eq!(queue.iter().map(|w| w.owner.as_str()).collect::<Vec<_>>(), ["b", "c"]);
        // a releases: c, second, is busy; b, first, gets it.
        let mut held = None;
        assert!(matches!(look(&asking("c", true), &mut held, &mut queue, &at(110, false)), Found::Busy(why) if why.contains("b waits first")));
        assert_eq!(look(&asking("b", true), &mut held, &mut queue, &at(111, false)), Found::Free);
        assert_eq!(queue.iter().map(|w| w.owner.as_str()).collect::<Vec<_>>(), ["c"]);
        // Without --wait, one that waits is ahead.
        assert!(matches!(look(&asking("d", false), &mut held, &mut queue, &at(112, false)), Found::Busy(_)));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn one_that_stopped_waiting_leaves_the_queue() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        look(&asking("b", true), &mut held, &mut queue, &at(100, false));
        look(&asking("c", true), &mut held, &mut queue, &at(101, false));
        // b says nothing more; c keeps waiting.
        look(&asking("c", true), &mut held, &mut queue, &at(150, false));
        let mut held = None;
        assert_eq!(look(&asking("c", true), &mut held, &mut queue, &at(100 + WAITING_GONE_SECONDS + 1, false)), Found::Free);
        assert!(queue.is_empty());
    }

    #[test]
    fn the_player_goes_first_and_waits_for_the_holder() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        look(&asking("b", true), &mut held, &mut queue, &at(100, false));
        assert!(matches!(look(&player(false), &mut held, &mut queue, &at(105, false)), Found::Busy(_)));
        assert_eq!(queue[0].owner, "player");
        assert_eq!(held.as_ref().unwrap().taken, None, "without --now the holder keeps the game");
        let mut held = None;
        assert!(matches!(look(&asking("b", true), &mut held, &mut queue, &at(110, false)), Found::Busy(_)));
        assert_eq!(look(&player(false), &mut held, &mut queue, &at(111, false)), Found::Free);
    }

    #[test]
    fn the_player_takes_the_game_once_the_holder_s_grace_ran_out() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        assert!(matches!(look(&player(true), &mut held, &mut queue, &at(200, true)), Found::Busy(why) if why.contains("takes the game 120 s after asking")));
        assert_eq!(held.as_ref().unwrap().taken, Some(200));
        assert!(matches!(look(&player(true), &mut held, &mut queue, &at(250, true)), Found::Busy(_)));
        assert!(matches!(look(&player(true), &mut held, &mut queue, &at(320, true)), Found::Take(h) if h.owner == "a" && h.taken == Some(200)));
    }

    #[test]
    fn a_holder_gone_is_taken_over_and_a_running_game_keeps_a_kept_lease() {
        let mut held = Some(Record { keeper: Some(HOLDER), ..record("a", 100) });
        let mut queue = Vec::new();
        let late = 100 + 30 * 60 + 1;
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &at(late, true)), Found::Busy(_)));
        assert_eq!(held.as_ref().unwrap().renewed, late, "the game runs: the lease starts again");
        let later = late + 30 * 60 + 1;
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, &at(later, false)), Found::Gone(h) if h.owner == "a"));
    }

    #[test]
    fn its_own_claim_is_renewed_and_a_running_game_with_no_claim_is_the_player_s() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        assert_eq!(look(&asking("a", true), &mut held, &mut queue, &at(200, false)), Found::Held);
        assert_eq!(held.unwrap().renewed, 200);
        let mut held = None;
        assert!(matches!(look(&asking("a", true), &mut held, &mut queue, &at(200, true)), Found::Busy(why) if why.contains("the player has it")));
    }
}
