//! A game for the tests of the catalogue's groups that do more than call the
//! capability layer, such as time's advance and the menu's waits, and of the
//! pipe: the layer's answers for a small made-up campaign, the Ottomans at
//! the start, and a record of the calls. A capability of a group it does not
//! script fails (`unscripted`).
use super::{
    armies, buildings, camera, centers_of_trade, colonists, console, countries, decisions, desktop, diplomacy, economy, estates, extensions, focus, forts,
    government, ideas, input, leaders, loans, menu, mercenaries, merchants, missionaries, missions, occupation, offers, parliament, peace, policies, provinces,
    questions, recruiting, screenshot, session, sieges, stability, subjects, technology, texts, time, trade, trade_companies,
};
use crate::game::{
    self, fail,
    menu::{Expected, ExpectedProbe, Launch},
    GResult, Status, Tag,
};
use eu4_harmony_remote::{response_json, CountryTag};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Time enough for whatever a test runs at one safe point.
pub const PLENTY: Duration = Duration::from_secs(10);

/// An engine's log, for a test that does not read it.
pub fn quiet(_: &str) {}

/// What a capability of the layer a fake game does not script answers.
pub fn unscripted(capability: &str) -> game::Fail {
    game::Fail { status: Status::Internal, message: format!("{capability} is not scripted in this test") }
}

/// What came back for a reply, in JSON; null when nothing came yet.
pub fn answer(reply: &crate::remote::Reply) -> Value {
    reply.wait(Duration::ZERO).map_or(Value::Null, |r| response_json(&r))
}

/// A successful answer, in JSON.
pub fn ok(result: Value) -> Value {
    json!({"ok": true, "result": result})
}

/// A tag as the game keeps it: the letters, the index, kind 0, valid.
pub fn game_tag(letters: &[u8; 3], index: u16) -> Tag {
    let [low, high] = index.to_le_bytes();
    Tag(u64::from_le_bytes([letters[0], letters[1], letters[2], 0, low, high, 0, 1]))
}

pub fn tur() -> Tag {
    game_tag(b"TUR", 154)
}

pub fn byz() -> Tag {
    game_tag(b"BYZ", 33)
}

pub fn wal() -> Tag {
    game_tag(b"WAL", 300)
}

/// The Ottomans' army in Edirne (149).
pub const ARMY: u64 = 4_294_967_377;
/// A fleet of theirs.
pub const FLEET: u64 = 4_294_967_400;
/// A Byzantine army in Constantinople (151).
pub const ENEMY: u64 = 4_294_967_500;

#[derive(Default)]
pub struct Fake {
    /// Every capability called, with its arguments.
    pub calls: Vec<String>,
    /// How long each call takes.
    pub delay: Duration,
    /// The army's route.
    pub route: Vec<u32>,
    /// The screenshot asked for, taken or not; a 2x1 image taken at once
    /// when None.
    pub shot: Option<Arc<game::screenshot::Shot>>,
    /// The clock the time capabilities read and set.
    pub clock: Clock,
    /// What came of the last load or start, as the game would say.
    pub launch: Arc<Mutex<Launch>>,
    /// The save `rewritten` as the game would read it now; the Ottomans'
    /// at 1444.11.1 until set.
    pub rewritten: Arc<Mutex<Option<Expected>>>,
    /// What came of a return to the main menu, when it is an in-process
    /// one; None when the game starts again.
    pub returning: Option<Arc<Mutex<game::menu::Returned>>>,
    /// What each diplomatic action sent comes to, asked one after another;
    /// pending once they run out.
    pub answers: std::collections::VecDeque<GResult<game::diplomacy::Answer>>,
    /// The inputs taken, in order.
    pub inputs: Vec<game::input::Action>,
    /// How many of them the game has had.
    pub inputs_handled: u64,
}

impl Fake {
    fn call(&mut self, what: String) {
        std::thread::sleep(self.delay);
        self.calls.push(what);
    }
}

// The groups whose tests script none of their capabilities.
impl buildings::Layer for Fake {}
impl camera::Layer for Fake {}
impl centers_of_trade::Layer for Fake {}
impl colonists::Layer for Fake {}
impl console::Layer for Fake {}
impl decisions::Layer for Fake {}
impl desktop::Layer for Fake {}
impl economy::Layer for Fake {}
impl estates::Layer for Fake {}
impl extensions::Layer for Fake {}
impl focus::Layer for Fake {}
impl forts::Layer for Fake {}
impl government::Layer for Fake {}
impl ideas::Layer for Fake {}
impl leaders::Layer for Fake {}
impl loans::Layer for Fake {}
impl mercenaries::Layer for Fake {}
impl merchants::Layer for Fake {}
impl missionaries::Layer for Fake {}
impl missions::Layer for Fake {}
impl offers::Layer for Fake {}
impl parliament::Layer for Fake {}
impl policies::Layer for Fake {}
impl questions::Layer for Fake {}
impl sieges::Layer for Fake {}
impl stability::Layer for Fake {}
impl subjects::Layer for Fake {}
impl technology::Layer for Fake {}
impl trade::Layer for Fake {}
impl trade_companies::Layer for Fake {}

pub fn army() -> game::armies::Army {
    game::armies::Army { id: ARMY, owner: tur(), province: 149, land: true, regiments: 12, ..Default::default() }
}

impl session::Layer for Fake {
    fn session(&mut self) -> GResult<game::session::Session> {
        self.call("session".into());
        // 1444.11.11
        Ok(game::session::Session { session: 2, date: 0x035D_7710, paused: true, visibility_current: true, player: tur() })
    }
}

impl countries::Layer for Fake {
    fn country(&mut self, tag: CountryTag) -> GResult<Tag> {
        self.call(format!("country {tag}"));
        match &tag.0 {
            b"TUR" => Ok(tur()),
            b"BYZ" => Ok(byz()),
            b"WAL" => Ok(wal()),
            _ => Err(eu4_harmony_checks::countries::not_found(&tag.to_string()).into()),
        }
    }

    fn hostile(&mut self, a: Tag, b: Tag) -> GResult<bool> {
        self.call(format!("hostile {:x} {:x}", a.0, b.0));
        Ok((a, b) == (tur(), byz()))
    }

    /// The Ottomans at war with Byzantium, with Wallachia as their vassal;
    /// Byzantium with nothing; [`wallachia`].
    fn snapshot(&mut self, country: Tag) -> GResult<game::countries::Country> {
        use eu4_harmony_remote::countries::{GoverningCapacity, IdeaGroup, Loans, Rebels, Technology};
        use game::countries::{Country, Own, Relation, RelationKind, War};
        self.call(format!("snapshot {:x}", country.0));
        let wal = wal();
        if country == byz() {
            return Ok(Country { tag: byz(), ..Default::default() });
        }
        if country == wal {
            return Ok(wallachia());
        }
        if country != tur() {
            return fail(Status::NotFound, "no such country");
        }
        Ok(Country {
            tag: tur(),
            treasury: Some(125_500),
            manpower: 22_600,
            max_manpower: 30_250,
            stability: 1_000,
            prestige: -12_345,
            power: Some([50, 60, 70]),
            wars: vec![War { name: "Ottoman Conquest of Constantinople".into(), attackers: vec![tur(), wal], defenders: vec![byz()] }],
            relations: vec![
                Relation { kind: RelationKind::Subject, country: wal, subject_type: Some("vassal".into()) },
                Relation { kind: RelationKind::Rival, country: byz(), subject_type: None },
            ],
            technology: Technology { adm: 3, dip: 3, mil: 4 },
            ideas: vec![
                IdeaGroup { group: "TUR_ideas".into(), ideas: 1, national: true },
                IdeaGroup { group: "quality_ideas".into(), ideas: 2, national: false },
            ],
            own: Some(Own {
                governing_capacity: GoverningCapacity { used: 512.5, max: 700.0 },
                inflation: 2_500,
                mercantilism: 10_000,
                loans: Loans { count: 1, ducats: 120.0 },
                rebels: vec![Rebels { kind: "nationalist_rebels".into(), name: "Albanian Nationalists".into(), progress: 12.5 }],
            }),
        })
    }

    /// The Ottomans' budget, one line of each table and a loan from the
    /// bank; another country's as the player's view leaves it.
    fn budget(&mut self, country: Tag) -> GResult<eu4_harmony_remote::countries::Budget> {
        use eu4_harmony_remote::countries::{Budget, Entry, Loan};
        self.call(format!("budget {:x}", country.0));
        let entry = |category: &str, name: &str, ducats| Entry { category: category.into(), name: name.into(), ducats };
        if country != tur() {
            return Ok(Budget { country: CountryTag(country.letters()), income: None, expenses: None, loans: None });
        }
        Ok(Budget {
            country: CountryTag(*b"TUR"),
            income: Some(vec![entry("INCOMETAX", "税收", 4.25)]),
            expenses: Some(vec![entry("EXPENSEAM", "陆军维护费", 3.5)]),
            loans: Some(vec![Loan { lender: None, ducats: 120.0, interest: 4.0, due: eu4_harmony_remote::Date(0x035d_7710 + 5 * 8760) }]),
        })
    }
}

/// Wallachia, the Ottomans' subject in their war on Byzantium, made up to
/// hold a relation of every kind (Moldavia its ally, Transylvania a march
/// of its own, the Ottomans its overlord, Hungary its rival) and a war on
/// either side: Hungary's war on it besides.
pub fn wallachia() -> game::countries::Country {
    use game::countries::{Country, Relation, RelationKind, War};
    let (mol, tra, hun) = (game_tag(b"MOL", 301), game_tag(b"TRA", 302), game_tag(b"HUN", 160));
    let relation = |kind, country, subject_type: Option<&str>| Relation { kind, country, subject_type: subject_type.map(Into::into) };
    Country {
        tag: wal(),
        treasury: Some(31_250),
        manpower: 5_400,
        max_manpower: 8_250,
        stability: -1_000,
        prestige: 12_345,
        power: Some([120, 95, 143]),
        wars: vec![
            War { name: "Ottoman Conquest of Constantinople".into(), attackers: vec![tur(), wal()], defenders: vec![byz()] },
            War { name: "Hungarian-Wallachian War".into(), attackers: vec![hun], defenders: vec![wal(), mol] },
        ],
        relations: vec![
            relation(RelationKind::Ally, mol, None),
            relation(RelationKind::Subject, tra, Some("march")),
            relation(RelationKind::Overlord, tur(), Some("vassal")),
            relation(RelationKind::Rival, hun, None),
        ],
        ..Default::default()
    }
}

impl provinces::Layer for Fake {
    fn provinces(&mut self) -> GResult<Vec<game::provinces::Province>> {
        self.call("provinces".into());
        let province = |id, owner: Tag, controller: Tag| game::provinces::Province {
            id,
            land: true,
            visible: true,
            owner: Some(owner),
            controller: Some(controller),
            ..Default::default()
        };
        Ok(vec![Default::default(), province(149, tur(), tur()), province(151, byz(), tur())])
    }

    fn named_provinces(&mut self, ids: Vec<u32>) -> GResult<Vec<game::provinces::Province>> {
        let all = self.provinces()?;
        Ok(ids.iter().filter_map(|id| all.iter().find(|p| p.id == *id && *id != 0).copied()).collect())
    }

    fn neighbors(&mut self, province: u32) -> GResult<Vec<game::provinces::Neighbor>> {
        self.call(format!("neighbors {province}"));
        Ok(vec![game::provinces::Neighbor { to: 151, strait: false }, game::provinces::Neighbor { to: 316, strait: true }])
    }

    fn province(&mut self, province: u32) -> GResult<game::provinces::Snapshot> {
        self.call(format!("province {province}"));
        if province != 151 {
            return Err(eu4_harmony_checks::provinces::not_found(province).into());
        }
        Ok(constantinople())
    }

    /// In Constantinople the player can develop tax for 50 administrative
    /// points and cannot core it; elsewhere nothing.
    fn province_options(&mut self, province: u32) -> GResult<Vec<eu4_harmony_remote::provinces::ProvinceOption>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            provinces::ProvinceOption,
        };
        self.call(format!("province options {province}"));
        if province != 151 {
            return Err(eu4_harmony_checks::provinces::not_found(province).into());
        }
        Ok(vec![
            ProvinceOption { action: "develop_tax".into(), edict: None, choice: Choice::can(vec![Amount::new(Resource::Adm, 50.0)]) },
            ProvinceOption { action: "set_edict".into(), edict: Some("edict_advancement_effort".into()), choice: Choice::cannot(Vec::new(), Vec::new()) },
            ProvinceOption {
                action: "core".into(),
                edict: None,
                choice: Choice::cannot(vec!["已经是核心省份".into()], vec![Amount::new(Resource::Adm, 36.0)]),
            },
        ])
    }

    /// The player may only develop.
    fn province_act(&mut self, province: u32, action: game::administration::Action) -> GResult<()> {
        self.call(format!("province {} {province}", action.name()));
        match action {
            game::administration::Action::Develop(_) => Ok(()),
            _ => Err(eu4_harmony_checks::administration::not_allowed("do it", &["已经是核心省份".into()]).into()),
        }
    }

    fn set_edict(&mut self, province: u32, edict: String) -> GResult<()> {
        self.call(format!("set edict {edict} {province}"));
        Err(eu4_harmony_checks::administration::no_such_edict(&edict).into())
    }
}

/// Constantinople (151), Byzantine, as the Chinese language mod names it,
/// with the Ottomans able to recruit nothing there.
pub fn constantinople() -> game::provinces::Snapshot {
    use eu4_harmony_remote::provinces::StateKind;
    use game::provinces::{Development, Fort, Recruit, Snapshot, UnitKind};
    let recruit = |kind, unit_type: Option<&str>, reasons: &[&str]| Recruit {
        kind,
        unit_type: unit_type.map(str::to_string),
        allowed: false,
        reasons: reasons.iter().map(|r| r.to_string()).collect(),
    };
    Snapshot {
        id: 151,
        name: "君士坦丁堡".into(),
        owner: Some(byz()),
        controller: Some(byz()),
        development: Development { tax: 10_000, production: 9_500, manpower: 6_000 },
        terrain: Some("farmlands".into()),
        fort: Fort { level: 2, mothballed: false },
        buildings: vec!["fort_15th".into(), "marketplace".into()],
        recruit: vec![
            recruit(UnitKind::Infantry, Some("eastern_medieval_infantry"), &["君士坦丁堡不是我们的领土"]),
            recruit(UnitKind::Cavalry, Some("eastern_knights"), &["君士坦丁堡不是我们的领土"]),
            recruit(UnitKind::Artillery, None, &[]),
            recruit(UnitKind::HeavyShip, Some("early_carrack"), &[]),
            recruit(UnitKind::LightShip, Some("early_war_galley"), &[]),
            recruit(UnitKind::Galley, Some("galley"), &[]),
            recruit(UnitKind::Transport, Some("cog"), &[]),
        ],
        autonomy: 25_000,
        culture: Some("greek".into()),
        religion: Some("orthodox".into()),
        trade_good: Some("silk".into()),
        unrest: None,
        trade_node: Some("constantinople".into()),
        center_of_trade: 3,
        trade_company: false,
        state: Some(StateKind::State),
        edict: None,
        cores: vec![CountryTag(*b"BYZ")],
        claims: vec![CountryTag(*b"TUR")],
        constructions: None,
        missionary: None,
        colony_size: None,
    }
}

impl armies::Layer for Fake {
    fn army(&mut self, id: u64) -> GResult<game::armies::Army> {
        self.call(format!("army {id}"));
        match id {
            ARMY => Ok(game::armies::Army { route_len: self.route.len() as u32, ..army() }),
            FLEET => Ok(game::armies::Army { id: FLEET, owner: tur(), ..Default::default() }),
            _ => Err(eu4_harmony_checks::armies::not_own(id).into()),
        }
    }

    fn own_armies(&mut self) -> GResult<Vec<game::armies::Army>> {
        self.call("own armies".into());
        Ok(vec![army(), game::armies::Army { id: FLEET, owner: tur(), ..Default::default() }])
    }

    fn armies_in(&mut self, province: u32) -> GResult<Vec<game::armies::Army>> {
        self.call(format!("armies in {province}"));
        Ok(vec![game::armies::Army {
            id: ENEMY,
            owner: byz(),
            province,
            land: true,
            sieging: true,
            route_source: game::armies::RouteSource::OtherExtension,
            ..Default::default()
        }])
    }

    fn route(&mut self, id: u64) -> GResult<Vec<u32>> {
        self.call(format!("route {id}"));
        Ok(self.route.clone())
    }

    fn can_step(&mut self, id: u64, from: u32, to: u32) -> GResult<bool> {
        self.call(format!("can step {id} {from} {to}"));
        Ok(to == 151)
    }

    fn can_attack(&mut self, id: u64, target: u64) -> GResult<bool> {
        self.call(format!("can attack {id} {target}"));
        Ok(target == ENEMY)
    }

    fn find_route(&mut self, id: u64, goal: u32) -> GResult<Vec<u32>> {
        self.call(format!("find route {id} {goal}"));
        Ok(vec![150, goal])
    }

    fn move_to(&mut self, id: u64, goal: u32, transports: bool) -> GResult<()> {
        self.call(format!("move {id} {goal}{}", if transports { " by sea" } else { "" }));
        if goal == 1 {
            return fail(Status::Rejected, "no route to 1");
        }
        self.route = vec![150, goal];
        Ok(())
    }

    fn stop(&mut self, id: u64) -> GResult<()> {
        self.call(format!("stop {id}"));
        self.route.clear();
        Ok(())
    }

    fn selected(&mut self) -> GResult<u64> {
        self.call("selected".into());
        fail(Status::NotFound, "army panel not open")
    }

    /// The army may split and drill; it is alone in its province.
    fn army_options(&mut self, id: u64) -> GResult<Vec<eu4_harmony_remote::armies::ArmyAction>> {
        use eu4_harmony_remote::{armies::ArmyAction, choices::Choice};
        self.call(format!("army options {id}"));
        Ok(vec![
            ArmyAction { action: "split".into(), choice: Choice::can(Vec::new()) },
            ArmyAction { action: "merge".into(), choice: Choice::cannot(vec!["至少需要两支部队".into()], Vec::new()) },
        ])
    }

    fn split(&mut self, id: u64) -> GResult<u64> {
        self.call(format!("split {id}"));
        Ok(id + 1)
    }

    fn merge(&mut self, ids: Vec<eu4_harmony_remote::ArmyId>) -> GResult<()> {
        self.call(format!("merge {}", ids.len()));
        match ids.len() {
            0 | 1 => fail(Status::Rejected, "至少需要两支部队"),
            _ => Ok(()),
        }
    }

    fn disband(&mut self, id: u64) -> GResult<()> {
        self.call(format!("disband {id}"));
        Ok(())
    }

    fn consolidate(&mut self, id: u64) -> GResult<()> {
        self.call(format!("consolidate {id}"));
        Ok(())
    }

    fn drill(&mut self, id: u64) -> GResult<()> {
        self.call(format!("drill {id}"));
        Ok(())
    }

    fn stop_drill(&mut self, id: u64) -> GResult<()> {
        self.call(format!("stop drill {id}"));
        fail(Status::Rejected, "the game does not let the player stop drilling: the army does not drill now, and gives no reason")
    }

    fn mission(&mut self, id: u64, mission: String, target: Option<u32>) -> GResult<()> {
        self.call(format!("mission {id} {mission} {target:?}"));
        Ok(())
    }

    fn stop_mission(&mut self, id: u64) -> GResult<()> {
        self.call(format!("stop mission {id}"));
        Ok(())
    }
}

impl menu::Layer for Fake {
    fn place(&mut self) -> GResult<game::menu::Place> {
        self.call("place".into());
        Ok(game::menu::Place::Game)
    }

    fn saves(&mut self) -> GResult<Vec<game::menu::Save>> {
        self.call("saves".into());
        Ok(vec![game::menu::Save {
            name: "奥斯曼1444_11_11".into(),
            file: "奥斯曼1444_11_11.eu4".into(),
            bytes: 3_266_935,
            modified: 1_761_700_000,
            player: Some(*b"TUR"),
            date: Some(0x035D_7710),
            missing_mods: vec!["彼岸乐园".into()],
        }])
    }

    fn load(&mut self, save: &str) -> GResult<ExpectedProbe> {
        self.call(format!("load {save}"));
        let expected = match save {
            "missing" => return fail(Status::NotFound, "no save \"missing\" in the save games folder"),
            // A save of Castile in 1470, which the game does not load.
            "Castilla-5" => Expected { player: *b"CAS", date: Some(eu4_harmony_remote::Date::from_ymd(1470, 3, 13).unwrap().0) },
            "rewritten" => {
                let rewritten = self.rewritten.clone();
                let first = Expected { player: *b"TUR", date: Some(eu4_harmony_remote::Date::from_ymd(1444, 11, 1).unwrap().0) };
                return Ok(Box::new(move || Ok(rewritten.lock().unwrap().unwrap_or(first))));
            }
            // 1444.11.11, as the session says.
            _ => Expected { player: *b"TUR", date: Some(0x035D_7710) },
        };
        Ok(Box::new(move || Ok(expected)))
    }

    fn start(&mut self, country: CountryTag) -> GResult<()> {
        self.call(format!("start {country}"));
        Ok(())
    }

    fn launch(&mut self) -> Box<dyn FnMut() -> Launch + Send> {
        let launch = self.launch.clone();
        Box::new(move || launch.lock().unwrap().clone())
    }

    fn return_to_main_menu(&mut self) -> GResult<Option<Box<dyn FnMut() -> game::menu::Returned + Send>>> {
        self.call("return".into());
        Ok(self.returning.clone().map(|returned| Box::new(move || returned.lock().unwrap().clone()) as Box<_>))
    }
}

impl texts::Layer for Fake {
    fn language(&mut self) -> GResult<&'static str> {
        self.call("language".into());
        Ok("simp_chinese")
    }
}

impl input::Layer for Fake {
    fn take(&mut self, action: game::input::Action, scale: u32) -> GResult<game::input::Taken> {
        self.call(format!("input {action:?} at scale {scale}"));
        self.inputs.push(action);
        Ok(game::input::Taken(self.inputs.len() as u64))
    }

    fn handled(&mut self, taken: game::input::Taken) -> bool {
        taken.0 <= self.inputs_handled
    }

    fn cursor(&mut self) -> GResult<game::input::Cursor> {
        self.call("cursor".into());
        Ok(game::input::Cursor { at: Some((10, 20)), ..Default::default() })
    }
}

impl screenshot::Layer for Fake {
    fn screenshot(&mut self, percent: u32) -> GResult<Arc<game::screenshot::Shot>> {
        self.call(format!("screenshot {percent}"));
        game::screenshot::check_scale(percent)?;
        Ok(self.shot.clone().unwrap_or_else(|| {
            let shot = game::screenshot::Shot::new(percent);
            shot.finish(Ok(game::screenshot::Image { png: vec![0x89, b'P', b'N', b'G'], width: 2, height: 1, frame_width: 2, frame_height: 1 }));
            shot
        }))
    }
}

/// The game's clock, as the time capabilities see it: time passes when a test
/// changes it. It starts at 1444.11.11, paused, at speed 2.
pub struct Clock {
    pub time: game::time::Time,
    /// How many questions wait for the player's answer.
    pub questions: usize,
    /// The dates time stops at, while advances hold them.
    pub stops: std::sync::Arc<std::sync::Mutex<Vec<i32>>>,
    /// A call that fails, as `calls` names it, and with what.
    pub failing: Option<(&'static str, Status)>,
}

impl Default for Clock {
    fn default() -> Self {
        let time = game::time::Time { session: 2, date: 0x035D_7710, speed: 2, pause: game::time::Pause::Player, over: None };
        Self { time, questions: 0, stops: Default::default(), failing: None }
    }
}

impl Clock {
    /// Fails when `call` is the one set to fail.
    fn check(&self, call: &str) -> GResult<()> {
        match self.failing {
            Some((failing, status)) if failing == call => fail(status, format!("{call} failed")),
            _ => Ok(()),
        }
    }
}

/// A date time stops at, kept in a [`Clock`] until released; dropped
/// unreleased, it is called off, which the clock keeps as `i32::MIN`.
struct Held(std::sync::Arc<std::sync::Mutex<Vec<i32>>>, i32, bool);

impl time::Held for Held {
    fn release(mut self: Box<Self>) {
        self.2 = true;
        let mut stops = self.0.lock().unwrap();
        if let Some(at) = stops.iter().position(|&d| d == self.1) {
            stops.remove(at);
        }
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if !self.2 {
            let mut stops = self.0.lock().unwrap();
            if let Some(at) = stops.iter().position(|&d| d == self.1) {
                stops[at] = i32::MIN;
            }
        }
    }
}

impl time::Layer for Fake {
    fn time(&mut self) -> GResult<game::time::Time> {
        self.call("time".into());
        self.clock.check("time")?;
        Ok(self.clock.time)
    }

    /// The player's pause, also when a window holds the game paused, as the
    /// layer's.
    fn pause(&mut self) -> GResult<()> {
        self.call("pause".into());
        self.clock.time.pause = game::time::Pause::Player;
        Ok(())
    }

    fn resume(&mut self) -> GResult<()> {
        self.call("resume".into());
        self.clock.check("resume")?;
        if let Some(end) = self.clock.time.over {
            return Err(eu4_harmony_checks::time::over(eu4_harmony_checks::Date(end)).into());
        }
        self.clock.time.pause = game::time::Pause::Running;
        Ok(())
    }

    fn set_speed(&mut self, speed: u32) -> GResult<()> {
        self.call(format!("speed {speed}"));
        eu4_harmony_checks::time::speed(speed)?;
        self.clock.time.speed = speed;
        Ok(())
    }

    fn questions_waiting(&mut self) -> GResult<usize> {
        self.call("questions waiting".into());
        Ok(self.clock.questions)
    }

    fn stop_at(&mut self, date: i32) -> GResult<Box<dyn time::Held>> {
        self.call(format!("stop at {date}"));
        self.clock.stops.lock().unwrap().push(date);
        Ok(Box::new(Held(self.clock.stops.clone(), date, false)))
    }
}

/// The Ottomans recruit in Edirne (149); in Constantinople (151), not
/// theirs, the game refuses with its reason.
impl recruiting::Layer for Fake {
    fn province_snapshot(&mut self, province: u32) -> GResult<game::provinces::Snapshot> {
        provinces::Layer::province(self, province)
    }

    fn recruit(&mut self, province: u32, kind: game::provinces::UnitKind) -> GResult<()> {
        self.call(format!("recruit {province} {}", kind.key()));
        match province {
            149 => Ok(()),
            151 => fail(Status::Rejected, "君士坦丁堡不是我们的领土"),
            _ => Err(eu4_harmony_checks::provinces::not_found(province).into()),
        }
    }
}

/// The war goals the Ottomans have against Byzantium: a claim on
/// Constantinople, and the declaration without a casus belli.
pub fn war_goals() -> Vec<game::diplomacy::WarGoal> {
    use game::diplomacy::WarGoal;
    vec![
        WarGoal {
            casus_belli: Some("cb_claim".into()),
            war_goal: Some("take_claim".into()),
            province: Some(151),
            target: None,
            name: "Conquer Constantinople".into(),
        },
        WarGoal { casus_belli: None, war_goal: Some("nowargoal".into()), province: None, target: None, name: "No War Goal".into() },
    ]
}

impl diplomacy::Layer for Fake {
    fn war_goals(&mut self, enemy: Tag) -> GResult<Vec<game::diplomacy::WarGoal>> {
        self.call(format!("war goals {:x}", enemy.0));
        Ok(if enemy == byz() { war_goals() } else { Vec::new() })
    }

    /// Wallachia is the ally the window lets the player call.
    fn declare_war(&mut self, enemy: Tag, choice: &game::diplomacy::Choice, calls: &game::diplomacy::Calls) -> GResult<game::diplomacy::Declaration> {
        use game::diplomacy::Calls;
        let calling = match calls {
            Calls::Only(tags) => format!("Only({:x?})", tags.iter().map(|t| t.0).collect::<Vec<_>>()),
            other => format!("{other:?}"),
        };
        self.call(format!(
            "declare war {:x} {:?} {:?} {:?} {:?} calling {calling}",
            enemy.0,
            choice.casus_belli,
            choice.war_goal,
            choice.province,
            choice.target.map(|t| t.0)
        ));
        if enemy == wal() {
            return fail(
                Status::Rejected,
                "They are our subject.
We have a truce with them.",
            );
        }
        let called = match calls {
            Calls::All => vec![wal()],
            Calls::None => Vec::new(),
            Calls::Only(tags) => tags.iter().copied().filter(|&t| t == wal()).collect(),
        };
        // Under every ally, Turkey is passed over, as the window's check
        // would not let the player call it.
        let passed_over = match calls {
            Calls::All => vec![game::diplomacy::PassedOver { country: tur(), reasons: vec!["They would not accept.".into()] }],
            _ => Vec::new(),
        };
        Ok(game::diplomacy::Declaration { ticket: game::diplomacy::Ticket(1), called, passed_over })
    }

    fn offer_peace(&mut self, enemy: Tag) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("offer peace {:x}", enemy.0));
        Ok(game::diplomacy::Ticket(2))
    }

    fn propose_alliance(&mut self, ally: Tag) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("propose alliance {:x}", ally.0));
        Ok(game::diplomacy::Ticket(3))
    }

    /// Toward Byzantium, an alliance the Byzantines would not accept, a
    /// warning the player can send, and a scripted action the player cannot.
    fn options(&mut self, country: Tag) -> GResult<Vec<game::diplomacy::Offered>> {
        use eu4_harmony_remote::choices::{Acceptance, Choice, Factor};
        use game::diplomacy::{Act, Offered, What};
        self.call(format!("options {:x}", country.0));
        let acceptance = Acceptance {
            accepts: false,
            total: -12.0,
            factors: vec![Factor { text: "Base reluctance".into(), value: -10.0 }, Factor { text: "Relations".into(), value: -2.0 }],
        };
        Ok(vec![
            Offered {
                what: Some(What::Act(Act::ProposeAlliance)),
                key: "allianceaction".into(),
                name: "Offer Alliance".into(),
                choice: Choice::can(Vec::new()),
                description: vec!["If a country accepts our Alliance offer it may come to our assistance during wars.".into()],
                acceptance: Some(acceptance),
            },
            Offered {
                what: Some(What::Act(Act::Warn)),
                key: "warningaction".into(),
                name: "Warn".into(),
                choice: Choice::can(Vec::new()),
                description: Vec::new(),
                acceptance: None,
            },
            Offered {
                what: Some(What::Scripted("sell_indulgence".into())),
                key: "sell_indulgence".into(),
                name: "Offer Indulgence".into(),
                choice: Choice::cannot(vec!["Is not the Papal Controller.".into()], Vec::new()),
                description: Vec::new(),
                acceptance: None,
            },
        ])
    }

    /// Warning Byzantium is sent; anything else is refused with the game's
    /// reason.
    fn act(&mut self, country: Tag, what: &game::diplomacy::What, _: &game::diplomacy::Details) -> GResult<game::diplomacy::Ticket> {
        use game::diplomacy::{Act, What};
        self.call(format!("act {:x} {what:?}", country.0));
        match what {
            What::Act(Act::Warn) if country == byz() => Ok(game::diplomacy::Ticket(4)),
            _ => fail(Status::Rejected, "Is not the Papal Controller."),
        }
    }

    /// Byzantium can be rivalled; no other country.
    fn add_rival(&mut self, rival: Tag) -> GResult<()> {
        self.call(format!("add rival {:x}", rival.0));
        if rival == byz() {
            Ok(())
        } else {
            fail(Status::Rejected, "the game's command refuses it, and gives no reason")
        }
    }

    fn remove_rival(&mut self, rival: Tag) -> GResult<()> {
        self.call(format!("remove rival {:x}", rival.0));
        fail(Status::Rejected, "the game's command refuses it, and gives no reason")
    }

    fn call_to_arms(&mut self, ally: Tag) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("call to arms {:x}", ally.0));
        Ok(game::diplomacy::Ticket(4))
    }

    fn answer(&mut self, ticket: game::diplomacy::Ticket) -> GResult<game::diplomacy::Answer> {
        self.call(format!("answer {}", ticket.0));
        self.answers.pop_front().unwrap_or(Ok(game::diplomacy::Answer::Pending))
    }
}

/// At war with Byzantium: Constantinople (151) to demand, and humiliating
/// it, which the view would not let the player add.
impl peace::Layer for Fake {
    fn peace_terms(&mut self, enemy: Tag, offer: bool) -> GResult<Vec<game::peace::TermOption>> {
        use eu4_harmony_remote::choices::{Amount, Choice, Resource};
        self.call(format!("peace terms {:x} {offer}", enemy.0));
        if enemy != byz() {
            return fail(Status::Rejected, "the player is not at war with WAL");
        }
        let term = |term: &str, province: Option<u32>, name: &str, choice: Choice| game::peace::TermOption {
            term: term.into(),
            province,
            country: province.map(|_| tur()),
            region: None,
            amount: None,
            name: name.into(),
            choice,
        };
        Ok(vec![
            term("demand_provinces", Some(151), "Constantinople", Choice::can(vec![Amount::new(Resource::WarScore, 40.0)])),
            term("humiliate", None, "Humiliate", Choice::cannot(vec!["Not our rival".into()], vec![Amount::new(Resource::WarScore, 10.0)])),
        ])
    }

    fn evaluate_peace(&mut self, enemy: Tag, terms: &[game::peace::Term], offer: bool) -> GResult<game::peace::Evaluation> {
        use eu4_harmony_remote::choices::{Acceptance, Amount, Choice, Factor, Resource};
        self.call(format!("evaluate peace {:x} {} {offer}", enemy.0, peace_terms(terms)));
        Ok(game::peace::Evaluation {
            choice: Choice::can(vec![Amount::new(Resource::WarScore, 40.0)]),
            acceptance: Acceptance { accepts: false, total: -5.0, factors: vec![Factor { text: "War score".into(), value: -5.0 }] },
        })
    }

    fn demand_peace(&mut self, enemy: Tag, terms: &[game::peace::Term]) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("demand peace {:x} {}", enemy.0, peace_terms(terms)));
        Ok(game::diplomacy::Ticket(5))
    }

    fn offer_terms(&mut self, enemy: Tag, terms: &[game::peace::Term]) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("offer terms {:x} {}", enemy.0, peace_terms(terms)));
        Ok(game::diplomacy::Ticket(6))
    }
}

/// Peace terms, as a call records them.
fn peace_terms(terms: &[game::peace::Term]) -> String {
    terms.iter().map(|t| format!("{}:{:?}:{:?}:{:?}", t.term, t.province, t.country.map(|c| c.0), t.amount)).collect::<Vec<_>>().join(",")
}

/// The Ottomans occupy Constantinople (151), which Wallachia may take.
impl occupation::Layer for Fake {
    fn transfers(&mut self, province: u32) -> GResult<game::occupation::Transfers> {
        use eu4_harmony_remote::choices::Choice;
        self.call(format!("transfers {province}"));
        Ok(match province {
            151 => game::occupation::Transfers { choice: Choice::can(Vec::new()), allies: vec![wal()] },
            _ => game::occupation::Transfers { choice: Choice::cannot(vec!["We are not the controller".into()], Vec::new()), allies: Vec::new() },
        })
    }

    fn transfer_occupation(&mut self, province: u32, ally: Tag) -> GResult<Option<Tag>> {
        self.call(format!("transfer occupation {province} {:x}", ally.0));
        Ok(Some(ally))
    }
}
