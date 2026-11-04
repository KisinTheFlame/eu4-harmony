//! The claim's record and the queue, as data, and what a claim finds when it
//! looks at them: the decisions, without the files, the mutex or the clock.
//!
//! The record is the JSON the game-lock skill's script keeps too (`owner`,
//! `purpose`, `checkout`, `since`, `renewed`, `leaseMinutes`, `backup`), so
//! that a checkout still on the script and one on these commands never hold
//! the game at once; the fields it does not know (`folder`, `player`,
//! `taken`) it keeps as they are.
use serde_json::{json, Value};

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
    /// The game is its: nobody had it, or the holder had gone, whose record
    /// this is.
    Free(Option<Record>),
    /// The player takes the game from this holder, whose grace ran out: its
    /// claim is to be released for it.
    Take(Record),
}

/// What `asking` finds, the record and the queue as they are; changes them
/// as it does: renews its own claim, puts itself in the queue or takes it
/// out, marks a holder the player takes the game from.
pub fn look(asking: &Asking, held: &mut Option<Record>, queue: &mut Vec<Waiting>, now: u64, game_running: bool) -> Found {
    tidy(queue, now);
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
        match queue.iter_mut().find(|w| w.owner == asking.owner) {
            Some(w) => w.seen = now,
            None => queue.push(Waiting { owner: asking.owner.clone(), asked: now, seen: now, player: asking.player }),
        }
        tidy(queue, now);
    }
    let first = queue.first().map(|w| w.owner.clone());
    let others_first = first.as_ref().is_some_and(|owner| *owner != asking.owner);
    match held.clone() {
        Some(h) if !h.stale(now, game_running) => {
            if asking.player && asking.now && !h.player {
                let taken = *held.as_mut().expect("held").taken.get_or_insert(now);
                if now >= taken + TAKEN_GRACE_SECONDS {
                    return Found::Take(h);
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
        gone => {
            queue.retain(|w| w.owner != asking.owner);
            Found::Free(gone)
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
        }
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
        assert_eq!(Record::from_json(&ours.to_json()).unwrap(), ours);
    }

    #[test]
    fn a_free_game_goes_to_the_first_in_the_queue_only() {
        let (mut held, mut queue) = (None, Vec::new());
        assert_eq!(look(&asking("a", true), &mut held, &mut queue, 100, false), Found::Free(None));
        let mut held = Some(record("a", 100));
        assert!(matches!(look(&asking("b", true), &mut held, &mut queue, 101, false), Found::Busy(_)));
        assert!(matches!(look(&asking("c", true), &mut held, &mut queue, 102, false), Found::Busy(_)));
        assert_eq!(queue.iter().map(|w| w.owner.as_str()).collect::<Vec<_>>(), ["b", "c"]);
        // a releases: c, second, is busy; b, first, gets it.
        let mut held = None;
        assert!(matches!(look(&asking("c", true), &mut held, &mut queue, 110, false), Found::Busy(why) if why.contains("b waits first")));
        assert_eq!(look(&asking("b", true), &mut held, &mut queue, 111, false), Found::Free(None));
        assert_eq!(queue.iter().map(|w| w.owner.as_str()).collect::<Vec<_>>(), ["c"]);
        // Without --wait, one that waits is ahead.
        assert!(matches!(look(&asking("d", false), &mut held, &mut queue, 112, false), Found::Busy(_)));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn one_that_stopped_waiting_leaves_the_queue() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        look(&asking("b", true), &mut held, &mut queue, 100, false);
        look(&asking("c", true), &mut held, &mut queue, 101, false);
        // b says nothing more; c keeps waiting.
        look(&asking("c", true), &mut held, &mut queue, 150, false);
        let mut held = None;
        assert_eq!(look(&asking("c", true), &mut held, &mut queue, 100 + WAITING_GONE_SECONDS + 1, false), Found::Free(None));
        assert!(queue.is_empty());
    }

    #[test]
    fn the_player_goes_first_and_waits_for_the_holder() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        look(&asking("b", true), &mut held, &mut queue, 100, false);
        assert!(matches!(look(&player(false), &mut held, &mut queue, 105, false), Found::Busy(_)));
        assert_eq!(queue[0].owner, "player");
        assert_eq!(held.as_ref().unwrap().taken, None, "without --now the holder keeps the game");
        let mut held = None;
        assert!(matches!(look(&asking("b", true), &mut held, &mut queue, 110, false), Found::Busy(_)));
        assert_eq!(look(&player(false), &mut held, &mut queue, 111, false), Found::Free(None));
    }

    #[test]
    fn the_player_takes_the_game_once_the_holder_s_grace_ran_out() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        assert!(matches!(look(&player(true), &mut held, &mut queue, 200, true), Found::Busy(why) if why.contains("takes the game 120 s after asking")));
        assert_eq!(held.as_ref().unwrap().taken, Some(200));
        assert!(matches!(look(&player(true), &mut held, &mut queue, 250, true), Found::Busy(_)));
        assert!(matches!(look(&player(true), &mut held, &mut queue, 320, true), Found::Take(h) if h.owner == "a" && h.taken == Some(200)));
    }

    #[test]
    fn a_holder_gone_is_taken_over_and_a_running_game_keeps_its_lease() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        let late = 100 + 30 * 60 + 1;
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, late, true), Found::Busy(_)));
        assert_eq!(held.as_ref().unwrap().renewed, late, "the game runs: the lease starts again");
        let later = late + 30 * 60 + 1;
        assert!(matches!(look(&asking("b", false), &mut held, &mut queue, later, false), Found::Free(Some(h)) if h.owner == "a"));
    }

    #[test]
    fn its_own_claim_is_renewed_and_a_running_game_with_no_claim_is_the_player_s() {
        let mut held = Some(record("a", 100));
        let mut queue = Vec::new();
        assert_eq!(look(&asking("a", true), &mut held, &mut queue, 200, false), Found::Held);
        assert_eq!(held.unwrap().renewed, 200);
        let mut held = None;
        assert!(matches!(look(&asking("a", true), &mut held, &mut queue, 200, true), Found::Busy(why) if why.contains("the player has it")));
    }
}
