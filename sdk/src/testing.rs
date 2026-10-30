//! An in-memory stand-in for the game, for unit tests of extensions.
//!
//! It stores data and records what the extension did; it does not simulate
//! game rules. Tests set up provinces, armies and routes, call the extension's
//! events with [`FakeGame::game`], and inspect [`FakeGame::commands`], the UI
//! state and the map. Orders have no effect unless the test applies them.
//! [`FakeGame::load_localisation`] gives it the mod's real texts.
use crate::{backend::Backend, game::*};
use eu4_harmony_loc::Language;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Move(ArmyId, ProvinceId),
    Stop(ArmyId),
    /// One regiment or ship of a kind recruited in a province.
    Recruit(ProvinceId, UnitKind),
    /// A question answered with an option.
    AnswerQuestion(QuestionId, u32),
    /// An offer accepted (true) or declined.
    AnswerOffer(OfferId, bool),
}

/// A diplomatic action the extension sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Diplomacy {
    /// A declaration of war, and the allies it was to call.
    DeclareWar(CountryId, WarGoal, CallAllies),
    OfferPeace(CountryId),
    ProposeAlliance(CountryId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    pub visible: Option<bool>,
    pub enabled: Option<bool>,
    pub text: Option<String>,
    pub tooltip: Option<String>,
}

#[derive(Default)]
pub struct FakeGame {
    pub session: Session,
    pub tags: HashMap<CountryId, String>,
    /// Unordered pairs of hostile countries.
    pub hostile: BTreeSet<(CountryId, CountryId)>,
    /// Indexed by province ID; index 0 is the placeholder.
    pub provinces: Vec<Province>,
    pub neighbors: BTreeMap<ProvinceId, Vec<Neighbor>>,
    pub own_armies: Vec<Army>,
    /// Visible armies per province, own and foreign.
    pub province_armies: BTreeMap<ProvinceId, Vec<Army>>,
    pub routes_of_armies: BTreeMap<ArmyId, Vec<ProvinceId>>,
    /// Provinces each army may not enter.
    pub closed: BTreeSet<(ArmyId, ProvinceId)>,
    pub closed_straits: BTreeSet<(ProvinceId, ProvinceId)>,
    /// Targets each army may not attack.
    pub protected: BTreeSet<(ArmyId, ArmyId)>,
    /// Routes `find_route` answers with; missing entries mean "no route".
    pub routes: BTreeMap<(ArmyId, ProvinceId), Vec<ProvinceId>>,
    /// Refuse every order with this message.
    pub reject_orders: Option<String>,
    pub selected: Option<ArmyId>,
    pub elements: BTreeMap<String, Element>,
    pub map: Option<Vec<MapColor>>,
    pub save_data: String,
    pub commands: Vec<Command>,
    /// The player language's Paradox name; English when empty.
    pub language: String,
    /// What `Game::text` answers, by key; see [`FakeGame::load_localisation`].
    pub texts: BTreeMap<String, String>,
    /// The map camera, which the camera's moves change; a zoom outside its
    /// `min_height` to `max_height` is refused. It starts at the unmodded
    /// game's heights, 50 to 3000, looking from 1000 ([`Camera`]'s default).
    pub camera: Camera,
    /// While set, the camera's moves are refused, as the core refuses them
    /// while the game holds the camera: after their arguments are checked.
    pub camera_locked: bool,
    /// The map position the camera looks at each province from; a province
    /// not here is not found.
    pub province_positions: BTreeMap<ProvinceId, (u32, u32)>,
    /// What `Game::province` answers, by province; missing entries are not
    /// found. Recruiting follows them as the core answers: in a province
    /// missing here, not found; of a kind its snapshot gives no unit type,
    /// leaves out (the core's lists every kind) or does not allow, refused
    /// in the core's words or with the snapshot's reasons.
    pub province_snapshots: BTreeMap<ProvinceId, ProvinceSnapshot>,
    /// The game console commands the extension ran, in order.
    pub console_commands: Vec<String>,
    /// What each game console command prints, by command as it was run;
    /// one not here is rejected as one the game console does not know. A
    /// command the core refuses before running it (empty, or with a `|` or
    /// a NUL) is refused as the core does, and not kept.
    pub console_output: BTreeMap<String, String>,
    /// What `Game::country` answers, by country.
    pub countries: BTreeMap<CountryId, Country>,
    /// The game speed, 1 to 5; 0, the default, reads as 1. Pausing and
    /// resuming set [`session`](Self::session)'s `paused`.
    pub speed: u32,
    /// The game's end date, a date as [`session`](Self::session)'s `date`:
    /// once the session's date is past it, resuming a paused game is refused,
    /// as the core refuses it once the game is over. None, the default: no
    /// end date, as for a game the player chose to play on past it.
    pub end_date: Option<i32>,
    /// The questions waiting for the player's answer; answering one records a
    /// command and leaves it here. An option not listed is `Rejected`, as the
    /// core answers an option the player may not choose: the fake knows no
    /// other options.
    pub questions: Vec<Question>,
    /// The offers waiting for the player's answer; answering one records a
    /// command and leaves it here.
    pub offers: Vec<Offer>,
    /// What `Game::war_goals` answers, by country; a declaration of war is
    /// refused for a war goal not here. Diplomacy, as the core, takes the
    /// countries [`tags`](Self::tags) or [`countries`](Self::countries) has
    /// but the player's ([`session`](Self::session)'s `player`), and finds
    /// no other.
    pub war_goals: BTreeMap<CountryId, Vec<WarGoal>>,
    /// The allies the declare war window lists, in its order, and whether it
    /// lets the player call each: a declaration calls them as the core does.
    pub allies: Vec<(CountryId, bool)>,
    /// The diplomatic actions the extension sent, in order; each one's
    /// ticket is its place in it, from 1.
    pub diplomacy: Vec<Diplomacy>,
    /// What `Game::diplomacy_answer` answers, by ticket; pending when not
    /// here.
    pub diplomacy_answers: BTreeMap<Ticket, DiplomacyAnswer>,
    /// Refuse every diplomatic action with this message, as the game's own
    /// checks refuse one: a declaration of war once its war goal is offered,
    /// before the allies named are checked, as the core asks them.
    pub reject_diplomacy: Option<String>,
}

fn not_found(what: impl Into<String>) -> Error {
    Error::new(Status::NotFound, what)
}

fn rejected(what: impl Into<String>) -> Error {
    Error::new(Status::Rejected, what)
}

/// A kind's name, as the core's refusals write it.
fn kind_key(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Infantry => "infantry",
        UnitKind::Cavalry => "cavalry",
        UnitKind::Artillery => "artillery",
        UnitKind::HeavyShip => "heavy_ship",
        UnitKind::LightShip => "light_ship",
        UnitKind::Galley => "galley",
        UnitKind::Transport => "transport",
        UnitKind::Unknown => "unknown",
    }
}

impl FakeGame {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn game(&mut self) -> Game<'_> {
        Game::new(self)
    }

    pub fn element(&self, name: &str) -> Element {
        self.elements.get(name).cloned().unwrap_or_default()
    }

    /// Sets the player language (a Paradox name such as `simp_chinese`) and
    /// fills [`texts`](Self::texts) from the mod at `mod_dir`, the way the
    /// core reads its `eu4-harmony/localisation/` files. Fails on files the
    /// core would leave out.
    pub fn load_localisation(&mut self, mod_dir: impl AsRef<Path>, language: &str) -> std::result::Result<(), String> {
        let language = Language::from_id(language).ok_or_else(|| format!("unknown language {language}"))?;
        let (catalog, problems) = eu4_harmony_loc::load_dir(&mod_dir.as_ref().join("eu4-harmony").join("localisation"));
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }
        self.language = language.id().to_string();
        self.texts = catalog.keys().into_iter().filter_map(|k| Some((k.to_string(), catalog.text(language, k)?))).collect();
        Ok(())
    }

    pub fn set_hostile(&mut self, a: CountryId, b: CountryId) {
        self.hostile.insert((a.min(b), a.max(b)));
    }

    /// Adds a land link both ways.
    pub fn link(&mut self, a: ProvinceId, b: ProvinceId) {
        self.neighbors.entry(a).or_default().push(Neighbor { to: b, strait: false });
        self.neighbors.entry(b).or_default().push(Neighbor { to: a, strait: false });
    }

    /// Refuses a move of the camera while it is locked, in the core's words.
    fn camera_movable(&self) -> Result<()> {
        if self.camera_locked {
            return Err(rejected("the camera is locked"));
        }
        Ok(())
    }

    fn own(&self, id: ArmyId) -> Result<&Army> {
        self.own_armies.iter().find(|a| a.id == id).ok_or_else(|| not_found(format!("no own army {}", id.0)))
    }
}

impl Backend for FakeGame {
    fn session(&mut self) -> Result<Session> {
        Ok(self.session)
    }
    fn country_tag(&mut self, country: CountryId) -> Result<String> {
        self.tags.get(&country).cloned().ok_or_else(|| not_found("unknown country"))
    }
    fn countries_hostile(&mut self, a: CountryId, b: CountryId) -> Result<bool> {
        Ok(a != b && self.hostile.contains(&(a.min(b), a.max(b))))
    }
    fn provinces(&mut self) -> Result<Vec<Province>> {
        Ok(self.provinces.clone())
    }
    fn province_neighbors(&mut self, province: ProvinceId) -> Result<Vec<Neighbor>> {
        Ok(self.neighbors.get(&province).cloned().unwrap_or_default())
    }
    fn army(&mut self, id: ArmyId) -> Result<Army> {
        self.own(id).copied()
    }
    fn own_armies(&mut self) -> Result<Vec<Army>> {
        Ok(self.own_armies.clone())
    }
    fn province_armies(&mut self, province: ProvinceId) -> Result<Vec<Army>> {
        if !self.session.visibility_current {
            return Err(Error::new(Status::NotReady, "visibility is not current yet"));
        }
        let visible = self.provinces.get(province as usize).is_some_and(|p| p.visible);
        Ok(if visible { self.province_armies.get(&province).cloned().unwrap_or_default() } else { Vec::new() })
    }
    fn army_route(&mut self, id: ArmyId) -> Result<Vec<ProvinceId>> {
        self.own(id)?;
        Ok(self.routes_of_armies.get(&id).cloned().unwrap_or_default())
    }
    fn army_can_enter(&mut self, id: ArmyId, province: ProvinceId) -> Result<bool> {
        self.own(id)?;
        Ok(!self.closed.contains(&(id, province)))
    }
    fn army_strait_open(&mut self, id: ArmyId, from: ProvinceId, to: ProvinceId) -> Result<bool> {
        self.own(id)?;
        Ok(!self.closed_straits.contains(&(from, to)))
    }
    fn army_can_attack(&mut self, id: ArmyId, target: ArmyId, _province: ProvinceId) -> Result<bool> {
        self.own(id)?;
        Ok(id != target && !self.protected.contains(&(id, target)))
    }
    fn find_route(&mut self, id: ArmyId, goal: ProvinceId) -> Result<Vec<ProvinceId>> {
        let army = *self.own(id)?;
        if goal == army.province {
            return Ok(Vec::new());
        }
        self.routes.get(&(id, goal)).cloned().ok_or_else(|| not_found(format!("no route to {goal}")))
    }
    fn army_move(&mut self, id: ArmyId, goal: ProvinceId) -> Result<()> {
        self.own(id)?;
        if let Some(reason) = &self.reject_orders {
            return Err(Error::new(Status::Rejected, reason.clone()));
        }
        self.commands.push(Command::Move(id, goal));
        Ok(())
    }
    fn army_stop(&mut self, id: ArmyId) -> Result<()> {
        self.own(id)?;
        if let Some(reason) = &self.reject_orders {
            return Err(Error::new(Status::Rejected, reason.clone()));
        }
        self.commands.push(Command::Stop(id));
        Ok(())
    }
    fn ui_selected_army(&mut self) -> Result<ArmyId> {
        self.selected.ok_or_else(|| not_found("no army selected"))
    }
    fn ui_set_visible(&mut self, element: &str, visible: bool) -> Result<()> {
        self.elements.entry(element.into()).or_default().visible = Some(visible);
        Ok(())
    }
    fn ui_set_enabled(&mut self, element: &str, enabled: bool) -> Result<()> {
        self.elements.entry(element.into()).or_default().enabled = Some(enabled);
        Ok(())
    }
    fn ui_set_text(&mut self, element: &str, text: &str) -> Result<()> {
        self.elements.entry(element.into()).or_default().text = Some(text.into());
        Ok(())
    }
    fn ui_set_tooltip(&mut self, element: &str, text: &str) -> Result<()> {
        self.elements.entry(element.into()).or_default().tooltip = Some(text.into());
        Ok(())
    }
    fn map_paint(&mut self, colors: &[MapColor]) -> Result<()> {
        self.map = Some(colors.to_vec());
        Ok(())
    }
    fn map_clear(&mut self) -> Result<()> {
        self.map = None;
        Ok(())
    }
    fn save_data_get(&mut self) -> Result<String> {
        Ok(self.save_data.clone())
    }
    fn save_data_set(&mut self, json: &str) -> Result<()> {
        self.save_data = json.into();
        Ok(())
    }
    fn language(&mut self) -> Result<String> {
        Ok(if self.language.is_empty() { "english".into() } else { self.language.clone() })
    }
    fn text(&mut self, key: &str) -> Result<String> {
        self.texts.get(key).cloned().ok_or_else(|| not_found(format!("no localisation text for {key}")))
    }
    fn camera(&mut self) -> Result<Camera> {
        Ok(self.camera)
    }
    fn camera_look_at_province(&mut self, province: ProvinceId) -> Result<Camera> {
        let &(x, y) = self.province_positions.get(&province).ok_or_else(|| not_found(format!("no province {province}")))?;
        self.camera_movable()?;
        self.camera = Camera { x, y, province: Some(province), ..self.camera };
        Ok(self.camera)
    }
    fn camera_look_at(&mut self, x: u32, y: u32) -> Result<Camera> {
        self.camera_movable()?;
        let province = self.province_positions.iter().find(|(_, &at)| at == (x, y)).map(|(&p, _)| p);
        self.camera = Camera { x, y, province, ..self.camera };
        Ok(self.camera)
    }
    fn camera_zoom(&mut self, height: u32) -> Result<Camera> {
        let Camera { min_height, max_height, .. } = self.camera;
        if !(min_height..=max_height).contains(&height) {
            return Err(Error::new(Status::InvalidArgument, format!("the height is from {min_height} to {max_height}")));
        }
        self.camera_movable()?;
        self.camera.height = height;
        Ok(self.camera)
    }
    fn province(&mut self, province: ProvinceId) -> Result<ProvinceSnapshot> {
        self.province_snapshots.get(&province).cloned().ok_or_else(|| not_found(format!("no province {province}")))
    }
    fn console_run(&mut self, command: &str) -> Result<String> {
        // The core's own refusals, in its words.
        let refused = if command.contains('|') {
            Some("a | sends the game console's output to a file; remote control and extensions get it in the answer, so leave it out")
        } else if command.contains('\0') {
            Some("a command cannot hold a NUL")
        } else if command.trim_matches([' ', '\t', '\r', '\n']).is_empty() {
            Some("no command")
        } else {
            None
        };
        if let Some(why) = refused {
            return Err(Error::new(Status::InvalidArgument, why));
        }
        self.console_commands.push(command.into());
        self.console_output.get(command).cloned().ok_or_else(|| Error::new(Status::Rejected, "Unknown command"))
    }
    fn country(&mut self, country: CountryId) -> Result<Country> {
        self.countries.get(&country).cloned().ok_or_else(|| not_found("unknown country"))
    }
    fn time_speed(&mut self) -> Result<u32> {
        Ok(self.speed.max(1))
    }
    fn time_set_speed(&mut self, speed: u32) -> Result<()> {
        if !(1..=5).contains(&speed) {
            return Err(Error::new(Status::InvalidArgument, format!("the speed is from 1 to 5, not {speed}")));
        }
        self.speed = speed;
        Ok(())
    }
    fn time_pause(&mut self) -> Result<()> {
        self.session.paused = true;
        Ok(())
    }
    fn time_resume(&mut self) -> Result<()> {
        if self.session.paused && self.end_date.is_some_and(|end| self.session.date > end) {
            return Err(rejected(
                "the game is over: it is past its end date, and stays paused, as for the player, until the end-of-game summary is closed to play on",
            ));
        }
        self.session.paused = false;
        Ok(())
    }
    fn recruit(&mut self, province: ProvinceId, kind: UnitKind) -> Result<()> {
        let snapshot = self.province_snapshots.get(&province).ok_or_else(|| not_found(format!("no province {province}")))?;
        // A kind the snapshot leaves out, which the core's never does, as one
        // the player has no unit type of.
        let Some(recruit) = snapshot.recruit.iter().find(|r| r.kind == kind && r.unit_type.is_some()) else {
            return Err(rejected(format!("the player has no unit type of the kind {}", kind_key(kind))));
        };
        if !recruit.allowed {
            return Err(rejected(if recruit.reasons.is_empty() {
                "the game does not let the player recruit it there, and gives no reason".to_string()
            } else {
                recruit.reasons.join("\n")
            }));
        }
        if let Some(reason) = &self.reject_orders {
            return Err(rejected(reason.clone()));
        }
        self.commands.push(Command::Recruit(province, kind));
        Ok(())
    }
    fn questions(&mut self) -> Result<Vec<Question>> {
        Ok(self.questions.clone())
    }
    fn answer_question(&mut self, question: QuestionId, option: u32) -> Result<()> {
        let q = self.questions.iter().find(|q| q.id == question).ok_or_else(|| not_found(format!("no question {}", question.0)))?;
        if !q.options.iter().any(|o| o.index == option) {
            return Err(Error::new(Status::Rejected, format!("the player may not choose option {option} of question {}", question.0)));
        }
        self.commands.push(Command::AnswerQuestion(question, option));
        Ok(())
    }
    fn offers(&mut self) -> Result<Vec<Offer>> {
        Ok(self.offers.clone())
    }
    fn answer_offer(&mut self, offer: OfferId, accept: bool) -> Result<()> {
        if !self.offers.iter().any(|o| o.id == offer) {
            return Err(not_found(format!("no offer {}", offer.0)));
        }
        self.commands.push(Command::AnswerOffer(offer, accept));
        Ok(())
    }
    fn war_goals(&mut self, country: CountryId) -> Result<Vec<WarGoal>> {
        self.other(country)?;
        Ok(self.war_goals.get(&country).cloned().unwrap_or_default())
    }
    fn declare_war(&mut self, country: CountryId, goal: &WarGoal, call: &CallAllies) -> Result<Declaration> {
        // In the core's order: the country, the casus belli named, the war
        // goal offered, the game's own refusal, then the allies.
        self.other(country)?;
        if goal.casus_belli.as_deref() == Some("") {
            return Err(Error::new(Status::InvalidArgument, "no casus belli: name one by its key, or none"));
        }
        let same = |g: &WarGoal| (&g.casus_belli, &g.war_goal, g.province, g.target) == (&goal.casus_belli, &goal.war_goal, goal.province, goal.target);
        if !self.war_goals.get(&country).is_some_and(|offered| offered.iter().any(same)) {
            return Err(Error::new(Status::Rejected, "the game offers no such war goal"));
        }
        self.allowed()?;
        let called = match call {
            CallAllies::All => self.allies.iter().filter(|&&(_, callable)| callable).map(|&(ally, _)| ally).collect(),
            CallAllies::None => Vec::new(),
            CallAllies::Only(named) => {
                if let Some(stranger) = named.iter().find(|n| !self.allies.iter().any(|(a, _)| a == *n)) {
                    return Err(Error::new(Status::InvalidArgument, format!("{} is not among the allies the declare war window lists", stranger.0)));
                }
                if let Some(refused) = named.iter().find(|n| self.allies.iter().any(|(a, callable)| a == *n && !callable)) {
                    return Err(rejected(format!("the declare war window does not let the player call {} to arms", refused.0)));
                }
                self.allies.iter().map(|&(a, _)| a).filter(|a| named.contains(a)).collect()
            }
        };
        let ticket = self.send(Diplomacy::DeclareWar(country, goal.clone(), call.clone()))?;
        Ok(Declaration { ticket, called })
    }
    fn offer_peace(&mut self, country: CountryId) -> Result<Ticket> {
        self.other(country)?;
        self.send(Diplomacy::OfferPeace(country))
    }
    fn propose_alliance(&mut self, country: CountryId) -> Result<Ticket> {
        self.other(country)?;
        self.send(Diplomacy::ProposeAlliance(country))
    }
    fn diplomacy_answer(&mut self, ticket: Ticket) -> Result<DiplomacyAnswer> {
        if ticket.0 == 0 || ticket.0 as usize > self.diplomacy.len() {
            return Err(not_found(format!("no diplomatic action {}", ticket.0)));
        }
        Ok(self.diplomacy_answers.get(&ticket).copied().unwrap_or(DiplomacyAnswer::Pending))
    }
}

impl FakeGame {
    /// A country diplomacy takes, as the core: one of the game's other than
    /// the player's.
    fn other(&self, country: CountryId) -> Result<()> {
        if !self.tags.contains_key(&country) && !self.countries.contains_key(&country) {
            return Err(not_found("unknown country"));
        }
        if country == self.session.player {
            return Err(Error::new(Status::InvalidArgument, "the player's own country"));
        }
        Ok(())
    }

    /// The game's own refusal, [`reject_diplomacy`](Self::reject_diplomacy).
    fn allowed(&self) -> Result<()> {
        match &self.reject_diplomacy {
            Some(message) => Err(Error::new(Status::Rejected, message.clone())),
            None => Ok(()),
        }
    }

    fn send(&mut self, action: Diplomacy) -> Result<Ticket> {
        self.allowed()?;
        self.diplomacy.push(action);
        Ok(Ticket(self.diplomacy.len() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_province_snapshot_is_what_the_test_set_up() {
        let mut fake = FakeGame::new();
        let snapshot = ProvinceSnapshot { id: 151, name: "君士坦丁堡".into(), fort: Fort { level: 2, mothballed: false }, ..Default::default() };
        fake.province_snapshots.insert(151, snapshot.clone());
        assert_eq!(fake.game().province(151).unwrap(), snapshot);
        assert_eq!(fake.game().province(152).unwrap_err().status, Status::NotFound);
    }

    /// No route is `None`; an army that is not the player's own stays an
    /// error, though the game says NotFound for both.
    #[test]
    fn a_route_is_none_only_for_an_own_army() {
        let mut fake = FakeGame::new();
        fake.own_armies.push(Army { id: ArmyId(7), province: 151, ..Default::default() });
        fake.routes.insert((ArmyId(7), 149), vec![150, 149]);
        assert_eq!(fake.game().find_route(ArmyId(7), 149).unwrap(), Some(vec![150, 149]));
        assert_eq!(fake.game().find_route(ArmyId(7), 1).unwrap(), None);
        let err = fake.game().find_route(ArmyId(8), 149).unwrap_err();
        assert_eq!((err.status, err.message.as_str()), (Status::NotFound, "no own army 8"));
    }

    #[test]
    fn game_console_commands_are_kept_and_answered_as_set() {
        let mut fake = FakeGame::new();
        fake.console_output.insert("cash 100".into(), "Gold changed by 100".into());
        assert_eq!(fake.game().console("cash 100").unwrap(), "Gold changed by 100");
        let err = fake.game().console("cahs 100").unwrap_err();
        assert_eq!((err.status, err.message.as_str()), (Status::Rejected, "Unknown command"));
        assert_eq!(fake.console_commands, ["cash 100", "cahs 100"]);
        // What the core refuses before it runs anything.
        for command in ["", " \t", "help | out.txt", "cash\0"] {
            assert_eq!(fake.game().console(command).unwrap_err().status, Status::InvalidArgument, "{command:?}");
        }
        assert_eq!(fake.console_commands.len(), 2);
    }

    #[test]
    fn diplomatic_actions_are_kept_and_answered_as_set() {
        let (tur, byz, wal, ven) = (CountryId(1), CountryId(2), CountryId(3), CountryId(4));
        let mut fake = FakeGame::new();
        fake.session.player = tur;
        for (id, tag) in [(tur, "TUR"), (byz, "BYZ"), (wal, "WAL")] {
            fake.tags.insert(id, tag.into());
        }
        fake.allies = vec![(wal, true), (ven, false)];
        let claim = WarGoal {
            casus_belli: Some("cb_claim".into()),
            war_goal: Some("take_claim".into()),
            province: Some(151),
            target: None,
            name: "Conquer Constantinople".into(),
        };
        fake.war_goals.insert(byz, vec![claim.clone()]);
        assert_eq!(fake.game().war_goals(byz).unwrap(), std::slice::from_ref(&claim));
        assert_eq!(fake.game().war_goals(wal).unwrap(), []);
        // The player's own country, and one the game does not have.
        assert_eq!(fake.game().war_goals(tur).unwrap_err().status, Status::InvalidArgument);
        assert_eq!(fake.game().propose_alliance(ven).unwrap_err().status, Status::NotFound);
        let war = fake.game().declare_war(byz, &claim, &CallAllies::All).unwrap();
        assert_eq!(war.called, [wal]);
        let war = war.ticket;
        let other = WarGoal { province: Some(149), ..claim.clone() };
        assert_eq!(fake.game().declare_war(byz, &other, &CallAllies::None).unwrap_err().status, Status::Rejected);
        // Allies named: one the window does not let the player call, and one
        // it does not list.
        assert_eq!(fake.game().declare_war(byz, &claim, &CallAllies::Only(vec![wal, ven])).unwrap_err().status, Status::Rejected);
        assert_eq!(fake.game().declare_war(byz, &claim, &CallAllies::Only(vec![byz])).unwrap_err().status, Status::InvalidArgument);
        let peace = fake.game().offer_peace(byz).unwrap();
        fake.diplomacy_answers.insert(war, DiplomacyAnswer::Declared);
        assert_eq!(fake.game().diplomacy_answer(war).unwrap(), DiplomacyAnswer::Declared);
        assert_eq!(fake.game().diplomacy_answer(peace).unwrap(), DiplomacyAnswer::Pending);
        assert_eq!(fake.game().diplomacy_answer(Ticket(9)).unwrap_err().status, Status::NotFound);
        // An empty casus belli, as the core takes it, before the war goals.
        let empty = WarGoal { casus_belli: Some(String::new()), ..claim.clone() };
        assert_eq!(fake.game().declare_war(byz, &empty, &CallAllies::None).unwrap_err().status, Status::InvalidArgument);
        // The game's own refusal comes before the allies named are checked.
        fake.reject_diplomacy = Some("Cannot ally with a rival.".into());
        assert_eq!(fake.game().propose_alliance(byz).unwrap_err().message, "Cannot ally with a rival.");
        assert_eq!(fake.game().declare_war(byz, &claim, &CallAllies::Only(vec![byz])).unwrap_err().message, "Cannot ally with a rival.");
        assert_eq!(fake.diplomacy, [Diplomacy::DeclareWar(byz, claim, CallAllies::All), Diplomacy::OfferPeace(byz)]);
    }

    #[test]
    fn the_fake_camera_moves_within_the_game_s_heights_unless_locked() {
        let mut fake = FakeGame::new();
        fake.province_positions.insert(151, (3270, 1366));
        assert_eq!(fake.game().camera_zoom(400).unwrap().height, 400);
        assert_eq!(fake.game().camera_zoom(49).unwrap_err().status, Status::InvalidArgument);
        assert_eq!(fake.game().camera_look_at_province(151).unwrap().province, Some(151));
        // Locked, a move is refused, once its arguments are right.
        fake.camera_locked = true;
        assert_eq!(fake.game().camera_zoom(3001).unwrap_err().status, Status::InvalidArgument);
        assert_eq!(fake.game().camera_look_at_province(152).unwrap_err().status, Status::NotFound);
        for refused in [fake.game().camera_zoom(500), fake.game().camera_look_at_province(151), fake.game().camera_look_at(1, 2)] {
            let err = refused.unwrap_err();
            assert_eq!((err.status, err.message.as_str()), (Status::Rejected, "the camera is locked"));
        }
        assert_eq!(fake.game().camera().unwrap().height, 400);
    }

    #[test]
    fn a_country_snapshot_is_what_the_test_set_up() {
        let mut fake = FakeGame::new();
        let country = Country {
            id: CountryId(7),
            treasury: 31.25,
            relations: vec![Relation { kind: RelationKind::Rival, country: CountryId(8), subject_type: None }],
            ..Default::default()
        };
        fake.countries.insert(CountryId(7), country.clone());
        assert_eq!(fake.game().country(CountryId(7)).unwrap(), country);
        assert_eq!(fake.game().country(CountryId(8)).unwrap_err().status, Status::NotFound);
    }

    #[test]
    fn the_fake_clock_pauses_resumes_and_keeps_its_speed() {
        let mut fake = FakeGame::new();
        let mut game = fake.game();
        assert_eq!(game.speed().unwrap(), 1);
        game.set_speed(4).unwrap();
        assert_eq!(game.set_speed(6).unwrap_err().status, Status::InvalidArgument);
        game.pause().unwrap();
        assert!(game.session().unwrap().paused);
        game.resume().unwrap();
        assert!(!game.session().unwrap().paused);
        assert_eq!(game.speed().unwrap(), 4);
    }

    #[test]
    fn the_fake_clock_does_not_resume_past_its_end_date() {
        let mut fake = FakeGame::new();
        fake.session.date = 1000;
        fake.session.paused = true;
        fake.end_date = Some(1000);
        // On the end date itself time still runs.
        fake.game().resume().unwrap();
        fake.session.date = 1001;
        // Running, resuming is nothing, as in the core.
        fake.game().resume().unwrap();
        fake.session.paused = true;
        let refused = fake.game().resume().unwrap_err();
        assert_eq!(refused.status, Status::Rejected);
        assert!(refused.message.starts_with("the game is over"), "{}", refused.message);
        assert!(fake.session.paused);
        // Played on: no end date.
        fake.end_date = None;
        fake.game().resume().unwrap();
        assert!(!fake.session.paused);
    }

    #[test]
    fn a_recruit_order_follows_the_province_s_snapshot() {
        let mut fake = FakeGame::new();
        let recruit = |kind, unit_type: Option<&str>, allowed, reasons: &[&str]| Recruit {
            kind,
            unit_type: unit_type.map(Into::into),
            allowed,
            reasons: reasons.iter().map(|r| r.to_string()).collect(),
        };
        let snapshot = ProvinceSnapshot {
            id: 219,
            recruit: vec![
                recruit(UnitKind::Infantry, Some("x"), false, &["没有足够的manpower。"]),
                recruit(UnitKind::Artillery, None, false, &[]),
                recruit(UnitKind::HeavyShip, Some("x"), false, &[]),
                recruit(UnitKind::Transport, Some("x"), true, &[]),
            ],
            ..Default::default()
        };
        fake.province_snapshots.insert(219, snapshot);
        let mut refused = |kind| {
            let err = fake.game().recruit(219, kind).unwrap_err();
            (err.status, err.message)
        };
        assert_eq!(refused(UnitKind::Infantry), (Status::Rejected, "没有足够的manpower。".into()));
        // The core's words where the snapshot has no reason.
        assert_eq!(refused(UnitKind::Artillery), (Status::Rejected, "the player has no unit type of the kind artillery".into()));
        // A kind the snapshot leaves out, as one with no unit type.
        assert_eq!(refused(UnitKind::Cavalry), (Status::Rejected, "the player has no unit type of the kind cavalry".into()));
        assert_eq!(refused(UnitKind::HeavyShip), (Status::Rejected, "the game does not let the player recruit it there, and gives no reason".into()));
        assert_eq!(refused(UnitKind::Unknown).0, Status::InvalidArgument);
        fake.game().recruit(219, UnitKind::Transport).unwrap();
        // Where the test set up no snapshot, there is no such province.
        let err = fake.game().recruit(1, UnitKind::Cavalry).unwrap_err();
        assert_eq!((err.status, err.message.as_str()), (Status::NotFound, "no province 1"));
        assert_eq!(fake.commands, [Command::Recruit(219, UnitKind::Transport)]);
        fake.reject_orders = Some("paused".into());
        assert_eq!(fake.game().recruit(219, UnitKind::Transport).unwrap_err().status, Status::Rejected);
    }

    #[test]
    fn answers_to_questions_and_offers_are_recorded() {
        let mut fake = FakeGame::new();
        fake.questions.push(Question {
            id: QuestionId(7),
            event: "flavor_tur.1".into(),
            options: vec![QuestionOption { index: 0, text: "Pay them".into() }, QuestionOption { index: 2, text: "Refuse".into() }],
            ..Default::default()
        });
        fake.offers.push(Offer { id: OfferId(3), from: CountryId(5), title: "Royal Marriage".into(), ..Default::default() });
        let mut game = fake.game();
        assert_eq!(game.questions().unwrap().len(), 1);
        game.answer_question(QuestionId(7), 2).unwrap();
        assert_eq!(game.answer_question(QuestionId(7), 1).unwrap_err().status, Status::Rejected);
        assert_eq!(game.answer_question(QuestionId(8), 0).unwrap_err().status, Status::NotFound);
        assert_eq!(game.offers().unwrap()[0].title, "Royal Marriage");
        game.accept_offer(OfferId(3)).unwrap();
        game.decline_offer(OfferId(3)).unwrap();
        assert_eq!(game.decline_offer(OfferId(4)).unwrap_err().status, Status::NotFound);
        assert_eq!(fake.commands, [Command::AnswerQuestion(QuestionId(7), 2), Command::AnswerOffer(OfferId(3), true), Command::AnswerOffer(OfferId(3), false)]);
        // The lists stay as the test set them.
        assert_eq!((fake.questions.len(), fake.offers.len()), (1, 1));
    }
}
