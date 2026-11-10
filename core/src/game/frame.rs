//! What a game's safe frame heard (GLOSSARY: Signal). What the frame read of
//! the game, [`Observed`], and what the session's frames before it showed,
//! kept in a [`Recall`], decide what happened since the last frame: a
//! [`Frame`], which extensions and remote control hear ([`Frame::heard`]).
//! Reading the game is `game::begin_frame`'s; deciding is here alone.
use super::{buildings, named, signals, wars, Tag, World};
use crate::sessions::Seen;
use eu4_harmony_remote::signals::{Kind, Signal, Winner};

/// What a safe frame read of the game, before anything is made of it.
pub struct Observed {
    /// The game and its player, also with no country to act as.
    pub world: World,
    /// What the frame shows of sessions (`crate::sessions`).
    pub seen: Seen,
    /// Visibility reflects the frame's date.
    pub visibility_current: bool,
    /// What changed since the last frame (`game::signals`).
    pub changes: Vec<signals::Change>,
}

/// What the session's frames showed that the next one is heard against: the
/// last day announced, and the country in view. A new session's first frame
/// forgets what the last session's showed.
#[derive(Debug, Default)]
pub struct Recall {
    day: Option<i32>,
    /// In observe mode, the country in view at the last frame of the
    /// session (none for none in view); None when the player played a
    /// country then, or before the session's first frame.
    in_view: Option<Option<Tag>>,
}

impl Recall {
    pub const fn new() -> Self {
        Self { day: None, in_view: None }
    }

    /// The frame `observed`, heard after the frames before it. `listened`
    /// says whether anyone listens to a kind of signal: a month or a year no
    /// one listens to is not worked out. `wars` reads what the wars and the
    /// provinces' owners show happened since the day before, and is asked
    /// only at a new day.
    pub fn next(&mut self, observed: Observed, listened: impl Fn(Kind) -> bool, wars: impl FnOnce(&World) -> Vec<wars::Happened>) -> Frame {
        let Observed { world, seen, visibility_current, changes } = observed;
        if seen.new_session {
            *self = Self::new();
        }
        let now = world.player.observing().then(|| world.player.acting());
        let in_view = view_changed(std::mem::replace(&mut self.in_view, now), now);
        let previous = self.day;
        let new_day = visibility_current && self.day.replace(world.date) != Some(world.date);
        let (new_month, new_year) = eu4_harmony_checks::Date(world.date).turned(previous.filter(|_| new_day).map(eu4_harmony_checks::Date));
        let (new_month, new_year) = (new_month && listened(Kind::Month), new_year && listened(Kind::Year));
        let wars = if new_day { wars(&world) } else { Vec::new() };
        Frame { world, session: seen.session, new_session: seen.new_session, in_view, changes, new_day, new_month, new_year, wars }
    }
}

/// What happened since the last safe frame, as a [`Recall`] made it of what
/// the frame read.
pub struct Frame {
    world: World,
    session: u64,
    new_session: bool,
    /// In observe mode, the country in view changed since the last frame of
    /// the session, to this one (none for none in view).
    in_view: Option<Option<Tag>>,
    changes: Vec<signals::Change>,
    /// The date differs from the last day announced in this session, and
    /// visibility reflects it.
    new_day: bool,
    /// The new day is the first heard in a new month, and someone listens.
    new_month: bool,
    /// The new day is the first heard in a new year, and someone listens.
    new_year: bool,
    /// At a new day, the wars declared, the peaces made and the provinces
    /// that changed owner since the day before, of the kinds someone
    /// listens to.
    wars: Vec<wars::Happened>,
}

/// One thing that happened since the last safe frame, as the signal both
/// extensions' handlers and remote control's reads are made from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    pub signal: Signal,
    /// The player's view shows it. Extensions hear every signal; remote
    /// control, unless the setting gives it the full view, only these.
    pub seen: bool,
}

impl Frame {
    /// The session's number.
    pub fn session(&self) -> u64 {
        self.session
    }

    /// The session differs from the last frame's: a campaign was loaded or
    /// started, and what the capabilities kept from the old one is gone.
    pub fn new_session(&self) -> bool {
        self.new_session
    }

    /// What happened, in the order extensions hear it and remote control
    /// numbers it: a new session, the country in view, the changes, the
    /// wars', a new day, its month, its year. A building whose key cannot be
    /// read is logged and left out.
    pub fn heard(&self) -> Vec<Heard> {
        let seen = |signal| Heard { signal, seen: true };
        let date = eu4_harmony_remote::Date(self.world.date);
        let session = self.new_session.then(|| seen(Signal::Session { session: self.session }));
        let in_view = self.in_view.map(|country| seen(Signal::CountryInView { country: country.map(named) }));
        let changes = self.changes.iter().filter_map(changed);
        let wars = self.wars.iter().cloned().map(|happened| seen(war_signal(happened)));
        let day = self.new_day.then(|| seen(Signal::Day { date }));
        let month = self.new_month.then(|| seen(Signal::Month { date }));
        let year = self.new_year.then(|| seen(Signal::Year { date }));
        session.into_iter().chain(in_view).chain(changes).chain(wars).chain(day).chain(month).chain(year).collect()
    }
}

/// A change, as it is heard; none for a building whose key cannot be read.
fn changed(change: &signals::Change) -> Option<Heard> {
    use eu4_harmony_remote::{ArmyId, ProvinceId};
    use signals::Change;
    let seen = |signal| Some(Heard { signal, seen: true });
    match change {
        Change::Ordered(army) => seen(Signal::ArmyOrdered { army: ArmyId(*army) }),
        Change::Defeated(army) => seen(Signal::ArmyDefeated { army: ArmyId(*army) }),
        Change::BuildingFinished { province, building, country: owner } => match buildings::key_of(*building) {
            Ok(building) => seen(Signal::BuildingFinished { province: ProvinceId(*province), building, country: named(*owner) }),
            Err(e) => {
                log!("building finished: {}", e.message);
                None
            }
        },
        Change::Battle(battle) => Some(Heard { signal: battle_signal(battle), seen: battle.seen }),
    }
}

/// A war's happening, as its signal.
fn war_signal(happened: wars::Happened) -> Signal {
    use wars::Happened;
    let countries = |tags: Vec<Tag>| tags.into_iter().map(named).collect();
    match happened {
        Happened::WarDeclared { war, attacker, defender } => Signal::WarDeclared { war, attacker: named(attacker), defender: named(defender) },
        Happened::PeaceMade { war, attackers, defenders, ended } => {
            Signal::PeaceMade { war, attackers: countries(attackers), defenders: countries(defenders), ended }
        }
        Happened::OwnerChanged { province, from, to } => {
            Signal::ProvinceOwnerChanged { province: eu4_harmony_remote::ProvinceId(province), from: from.map(named), to: to.map(named) }
        }
    }
}

/// A battle, as the `battle` signal tells it.
fn battle_signal(battle: &signals::Battle) -> Signal {
    Signal::Battle {
        province: eu4_harmony_remote::ProvinceId(battle.province),
        naval: battle.naval,
        attackers: battle.attackers.iter().copied().map(named).collect(),
        defenders: battle.defenders.iter().copied().map(named).collect(),
        winner: if battle.attacker_won { Winner::Attackers } else { Winner::Defenders },
        attacker_losses: battle.attacker_losses.into(),
        defender_losses: battle.defender_losses.into(),
    }
}

/// The country in view a frame shows changed to, from the last frame's,
/// each in observe mode the country in view (none for none) and None when
/// the player played a country, or for the session's first frame: a change
/// only from one frame in observe mode to the next.
fn view_changed(before: Option<Option<Tag>>, now: Option<Option<Tag>>) -> Option<Option<Tag>> {
    match (before, now) {
        (Some(before), Some(now)) if before != now => Some(now),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        signals::{Battle, Change},
        wars::Happened,
        Heard, Kind, Observed, Recall, Seen, Signal, Tag, Winner, World,
    };
    use crate::game::player::Player;
    use eu4_harmony_remote::{ArmyId, CountryTag, Date, ProvinceId};
    use std::cell::Cell;

    fn tag(letters: &[u8; 3]) -> Tag {
        let [a, b, c] = *letters;
        Tag(u64::from_le_bytes([a, b, c, 0, 0, 0, 0, 1]))
    }

    fn day(y: i32, m: u32, d: u32) -> i32 {
        eu4_harmony_checks::Date::from_ymd(y, m, d).unwrap().0
    }

    /// A frame of session `session` at `date`, its visibility current, with
    /// nothing changed.
    fn at(session: u64, new_session: bool, date: i32, player: Player) -> Observed {
        Observed { world: World { world: 0, date, countries: 0, player }, seen: Seen { session, new_session }, visibility_current: true, changes: Vec::new() }
    }

    fn signals(recall: &mut Recall, observed: Observed) -> Vec<Signal> {
        recall.next(observed, |_| true, |_| Vec::new()).heard().into_iter().map(|heard| heard.signal).collect()
    }

    #[test]
    fn a_session_s_first_frame_hears_the_session_and_its_day_but_no_month() {
        let mut recall = Recall::new();
        let plays = Player::Plays(tag(b"TUR"));
        let date = day(1444, 11, 11);
        assert_eq!(signals(&mut recall, at(3, true, date, plays)), [Signal::Session { session: 3 }, Signal::Day { date: Date(date) }]);
        // The same day again is not another.
        assert!(signals(&mut recall, at(3, false, date, plays)).is_empty());
    }

    #[test]
    fn a_month_and_a_year_turn_with_the_first_day_heard_in_them() {
        let mut recall = Recall::new();
        let plays = Player::Plays(tag(b"TUR"));
        signals(&mut recall, at(3, true, day(1444, 11, 30), plays));
        let (december, january, next) = (day(1444, 12, 1), day(1445, 1, 2), day(1445, 1, 3));
        assert_eq!(signals(&mut recall, at(3, false, december, plays)), [Signal::Day { date: Date(december) }, Signal::Month { date: Date(december) }]);
        assert_eq!(
            signals(&mut recall, at(3, false, january, plays)),
            [Signal::Day { date: Date(january) }, Signal::Month { date: Date(january) }, Signal::Year { date: Date(january) }]
        );
        assert_eq!(signals(&mut recall, at(3, false, next, plays)), [Signal::Day { date: Date(next) }]);
    }

    #[test]
    fn a_month_no_one_listens_to_is_not_heard() {
        let mut recall = Recall::new();
        let plays = Player::Plays(tag(b"TUR"));
        signals(&mut recall, at(3, true, day(1444, 12, 31), plays));
        let only_months = |kind| kind == Kind::Month;
        let frame = recall.next(at(3, false, day(1445, 1, 1), plays), only_months, |_| Vec::new());
        let date = Date(day(1445, 1, 1));
        assert_eq!(frame.heard().into_iter().map(|h| h.signal).collect::<Vec<_>>(), [Signal::Day { date }, Signal::Month { date }]);
    }

    #[test]
    fn a_day_waits_for_visibility_to_reflect_it() {
        let mut recall = Recall::new();
        let plays = Player::Plays(tag(b"TUR"));
        signals(&mut recall, at(3, true, day(1444, 11, 30), plays));
        let december = day(1444, 12, 1);
        let stale = Observed { visibility_current: false, ..at(3, false, december, plays) };
        assert!(signals(&mut recall, stale).is_empty());
        // Heard once it does, still as the month's first.
        assert_eq!(signals(&mut recall, at(3, false, december, plays)), [Signal::Day { date: Date(december) }, Signal::Month { date: Date(december) }]);
    }

    #[test]
    fn the_wars_are_read_only_at_a_new_day() {
        let mut recall = Recall::new();
        let plays = Player::Plays(tag(b"TUR"));
        let read = Cell::new(0);
        let wars = |_: &World| {
            read.set(read.get() + 1);
            Vec::new()
        };
        recall.next(at(3, true, day(1444, 11, 11), plays), |_| true, wars);
        recall.next(at(3, false, day(1444, 11, 11), plays), |_| true, wars);
        recall.next(at(3, false, day(1444, 11, 12), plays), |_| true, wars);
        assert_eq!(read.get(), 2);
    }

    #[test]
    fn a_change_of_view_is_heard_once_between_frames_in_observe_mode() {
        let (own, fra, eng) = (tag(b"REB"), tag(b"FRA"), tag(b"ENG"));
        let observes = |in_view| Player::Observes { own, in_view };
        let date = day(1444, 11, 11);
        let in_view = |country: Option<Tag>| Signal::CountryInView { country: country.map(|t| CountryTag(t.letters())) };
        let mut recall = Recall::new();
        // A session's first frame is no change of view.
        assert_eq!(signals(&mut recall, at(3, true, date, observes(None))), [Signal::Session { session: 3 }, Signal::Day { date: Date(date) }]);
        assert_eq!(signals(&mut recall, at(3, false, date, observes(Some(fra)))), [in_view(Some(fra))]);
        assert!(signals(&mut recall, at(3, false, date, observes(Some(fra)))).is_empty());
        assert_eq!(signals(&mut recall, at(3, false, date, observes(Some(eng)))), [in_view(Some(eng))]);
        assert_eq!(signals(&mut recall, at(3, false, date, observes(None))), [in_view(None)]);
        // The player taking up a country, and leaving it, end a session
        // instead.
        assert!(signals(&mut recall, at(3, false, date, Player::Plays(fra))).is_empty());
        assert!(signals(&mut recall, at(3, false, date, observes(Some(eng)))).is_empty());
    }

    #[test]
    fn a_new_session_forgets_the_last_one_s_day_and_view() {
        let (own, fra) = (tag(b"REB"), tag(b"FRA"));
        let mut recall = Recall::new();
        signals(&mut recall, at(3, true, day(1444, 12, 31), Player::Observes { own, in_view: None }));
        // The same date, a new year and another country in view: the new
        // session's first day, with no month, no year and no change of view.
        let date = day(1445, 1, 1);
        assert_eq!(
            signals(&mut recall, at(4, true, date, Player::Observes { own, in_view: Some(fra) })),
            [Signal::Session { session: 4 }, Signal::Day { date: Date(date) }]
        );
    }

    #[test]
    fn a_frame_is_heard_session_and_view_first_then_the_changes_and_the_wars_and_day_last() {
        let (tur, byz, own) = (tag(b"TUR"), tag(b"BYZ"), tag(b"REB"));
        let battle = Battle {
            province: 151,
            naval: false,
            attackers: vec![tur],
            defenders: vec![byz],
            attacker_won: false,
            attacker_losses: 1200,
            defender_losses: 300,
            seen: false,
        };
        let seen = |signal| Heard { signal, seen: true };
        let (letters_tur, letters_byz) = (CountryTag(*b"TUR"), CountryTag(*b"BYZ"));
        let owner_changed = || vec![Happened::OwnerChanged { province: 151, from: None, to: Some(tur) }];
        let mut recall = Recall::new();
        let first = day(1444, 12, 31);
        let session = Observed {
            changes: vec![Change::Ordered(7), Change::Defeated(8), Change::Battle(battle)],
            ..at(3, true, first, Player::Observes { own, in_view: None })
        };
        assert_eq!(
            recall.next(session, |_| true, |_| owner_changed()).heard(),
            [
                seen(Signal::Session { session: 3 }),
                seen(Signal::ArmyOrdered { army: ArmyId(7) }),
                seen(Signal::ArmyDefeated { army: ArmyId(8) }),
                // A battle the player neither sees nor takes part in.
                Heard {
                    signal: Signal::Battle {
                        province: ProvinceId(151),
                        naval: false,
                        attackers: vec![letters_tur],
                        defenders: vec![letters_byz],
                        winner: Winner::Defenders,
                        attacker_losses: 1200,
                        defender_losses: 300
                    },
                    seen: false
                },
                seen(Signal::ProvinceOwnerChanged { province: ProvinceId(151), from: None, to: Some(letters_tur) }),
                seen(Signal::Day { date: Date(first) }),
            ]
        );
        let next = day(1445, 1, 1);
        let view = Observed { changes: vec![Change::Ordered(7)], ..at(3, false, next, Player::Observes { own, in_view: Some(tur) }) };
        let date = Date(next);
        assert_eq!(
            recall.next(view, |_| true, |_| owner_changed()).heard(),
            [
                seen(Signal::CountryInView { country: Some(letters_tur) }),
                seen(Signal::ArmyOrdered { army: ArmyId(7) }),
                seen(Signal::ProvinceOwnerChanged { province: ProvinceId(151), from: None, to: Some(letters_tur) }),
                seen(Signal::Day { date }),
                seen(Signal::Month { date }),
                seen(Signal::Year { date }),
            ]
        );
        let quiet = at(3, false, next, Player::Observes { own, in_view: Some(tur) });
        assert!(recall.next(quiet, |_| true, |_| owner_changed()).heard().is_empty());
    }
}
