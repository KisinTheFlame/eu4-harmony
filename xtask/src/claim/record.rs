//! The claim's record and the queue, as data, and what a claim finds when it
//! looks at them: the decisions, without the files, the mutex or the clock.
//!
//! The record is the JSON the game-lock skill's script keeps too (`owner`,
//! `purpose`, `checkout`, `since`, `renewed`, `leaseMinutes`, `backup`), so
//! that a checkout still on the script and one on these commands never hold
//! the game at once; the fields it does not know (`folder`, `player`,
//! `taken`, `puttingBack`) it keeps as they are.
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
    pub game_running: bool,
    /// The process that looks.
    pub me: Process,
    /// Whether a process recorded runs still.
    pub alive: &'a dyn Fn(&Process) -> bool,
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
    /// The process putting back what the claim's run changed: while it
    /// runs, nothing else puts it back.
    pub putting_back: Option<Process>,
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
            putting_back: Process::from_json(&value["puttingBack"]),
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
            "puttingBack": self.putting_back.map(Process::to_json),
        })
    }

    /// When the lease runs out, in seconds since 1970.
    pub fn lease_ends(&self) -> u64 {
        self.renewed + 60 * self.lease_minutes
    }

    /// A holder that let its lease run out while the game was closed has gone.
    pub fn stale(&self, now: u64, game_running: bool) -> bool {
        !game_running && now > self.lease_ends()
    }

    /// The same claim as `other`: the same owner, from the same time, with
    /// the same folder.
    pub fn same_claim(&self, other: &Record) -> bool {
        self.owner == other.owner && self.since == other.since && self.folder == other.folder
    }

    /// Another process that runs puts this claim back.
    fn put_back_by_another(&self, seen: &Seen) -> Option<Process> {
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
    if let Some(h) = held.as_mut().filter(|h| h.owner == asking.owner) {
        h.renewed = now;
        queue.retain(|w| w.owner != asking.owner);
        return Found::Held;
    }
    // Every look at a claim while the game runs starts its lease again.
    if let Some(h) = held.as_mut().filter(|_| game_running) {
        h.renewed = now;
    }
    if asking.wait {
        wait_in(queue, asking, now);
    }
    let first = queue.first().map(|w| w.owner.clone());
    let others_first = first.as_ref().is_some_and(|owner| *owner != asking.owner);
    match held.clone() {
        Some(h) if !h.stale(now, game_running) => {
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

/// What a holder's release finds of its claim.
#[derive(Clone, Debug, PartialEq)]
pub enum Release {
    /// The claim, marked as this process's to put back.
    Mine(Record),
    /// Another process puts it back: the player's claim taking the game, or
    /// one that took over.
    Busy(String),
    /// The game is not `owner`'s.
    NotHeld,
}

/// Marks `owner`'s claim as the looking process's to put back, unless
/// another process that runs puts it back already.
pub fn release(owner: &str, held: &mut Option<Record>, seen: &Seen) -> Release {
    let Some(h) = held.as_mut().filter(|h| h.owner == owner) else { return Release::NotHeld };
    if let Some(p) = h.put_back_by_another(seen) {
        let whose = if h.taken.is_some() { "the player's claim, which takes the game," } else { "another claim" };
        return Release::Busy(format!("{whose} puts back what this claim's run changed (process {}), and frees the game", p.id));
    }
    h.putting_back = Some(seen.me);
    Release::Mine(h.clone())
}

/// Ends `of`, which `me` put back: its record goes once all was put back,
/// and is no longer marked otherwise, so that it is put back again.
pub fn put_back(held: &mut Option<Record>, of: &Record, me: Process, all: bool) {
    if let Some(h) = held.as_mut().filter(|h| h.same_claim(of) && h.putting_back == Some(me)) {
        if all {
            *held = None;
        } else {
            h.putting_back = None;
        }
    }
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
            backup: None,
            folder: None,
            player: false,
            taken: None,
            putting_back: None,
        }
    }

    /// The process that looks in these tests, and two others.
    const ME: Process = Process { id: 10, started: 1 };
    const HOLDER: Process = Process { id: 20, started: 2 };
    const DEAD: Process = Process { id: 30, started: 3 };

    fn alive(p: &Process) -> bool {
        *p != DEAD
    }

    fn at(now: u64, game_running: bool) -> Seen<'static> {
        Seen { now, game_running, me: ME, alive: &alive }
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

    /// The player's claim, taking the game, and the holder's own release
    /// never both put the claim back: the first marks it, the other waits
    /// for it or is told who does it.
    #[test]
    fn one_process_alone_puts_a_claim_back() {
        // The player's grace runs out first: its claim puts the holder's back.
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        look(&player(true), &mut held, &mut queue, &at(200, true));
        assert!(matches!(look(&player(true), &mut held, &mut queue, &at(320, true)), Found::Take(_)));
        assert!(matches!(release("a", &mut held, &by(HOLDER, 321, true)), Release::Busy(why) if why.contains("the player's claim")));
        let taken = held.clone().unwrap();
        put_back(&mut held, &taken, ME, true);
        assert_eq!(held, None);

        // The holder releases first: the player waits for it.
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        look(&player(true), &mut held, &mut queue, &at(200, true));
        let Release::Mine(mine) = release("a", &mut held, &by(HOLDER, 310, true)) else { panic!("the holder's to put back") };
        assert!(matches!(look(&player(true), &mut held, &mut queue, &at(320, true)), Found::Busy(why) if why.contains("process 20 puts back")));
        assert!(matches!(release("a", &mut held, &by(HOLDER, 321, true)), Release::Mine(_)), "the same process may go on");
        put_back(&mut held, &mine, HOLDER, true);
        assert_eq!(look(&player(true), &mut held, &mut queue, &at(330, false)), Found::Free);
    }

    /// A holder gone keeps its record until its claim is put back, so that a
    /// put back that fails leaves it to be put back again; one whose put
    /// back ended with its process is put back by the next.
    #[test]
    fn a_gone_holder_keeps_its_record_until_it_is_put_back() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        let late = 100 + 30 * 60 + 1;
        let Found::Gone(gone) = look(&asking("b", false), &mut held, &mut queue, &at(late, false)) else { panic!("a has gone") };
        assert_eq!(held.as_ref().unwrap().owner, "a");
        assert_eq!(held.as_ref().unwrap().putting_back, Some(ME));
        assert!(matches!(look(&asking("c", false), &mut held, &mut queue, &by(HOLDER, late, false)), Found::Busy(_)));
        put_back(&mut held, &gone, ME, false);
        assert_eq!(held.as_ref().unwrap().putting_back, None, "not put back: still a's, to be put back again");

        held.as_mut().unwrap().putting_back = Some(DEAD);
        let Found::Gone(gone) = look(&asking("c", false), &mut held, &mut queue, &by(HOLDER, late, false)) else { panic!("its put back ended") };
        put_back(&mut held, &gone, HOLDER, true);
        assert_eq!(held, None);
        assert_eq!(look(&asking("c", false), &mut held, &mut queue, &at(late, false)), Found::Free);
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
    fn a_holder_gone_is_taken_over_and_a_running_game_keeps_its_lease() {
        let mut held = Some(record("a", 100));
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
