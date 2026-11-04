//! A game for the engine's tests: the capability layer's answers for a small
//! made-up campaign, the Ottomans at the start, and a record of the calls.
use super::{
    armies, buildings, camera, centers_of_trade, colonists, console, countries, decisions, desktop, diplomacy, extensions, input, menu, merchants,
    missionaries, offers, provinces, questions, recruiting, screenshot, session, texts, time, trade_companies,
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
    /// The questions and offers answered.
    pub answered: Vec<(u32, u32)>,
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
        use game::countries::{Country, Relation, RelationKind, War};
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
            treasury: 125_500,
            manpower: 22_600,
            max_manpower: 30_250,
            stability: 1_000,
            prestige: -12_345,
            power: [50, 60, 70],
            wars: vec![War { name: "Ottoman Conquest of Constantinople".into(), attackers: vec![tur(), wal], defenders: vec![byz()] }],
            relations: vec![
                Relation { kind: RelationKind::Subject, country: wal, subject_type: Some("vassal".into()) },
                Relation { kind: RelationKind::Rival, country: byz(), subject_type: None },
            ],
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
        treasury: 31_250,
        manpower: 5_400,
        max_manpower: 8_250,
        stability: -1_000,
        prestige: 12_345,
        power: [120, 95, 143],
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

/// In Constantinople, a marketplace being built by the player, and a temple
/// standing; nothing can be built.
impl buildings::Layer for Fake {
    fn building_options(&mut self, province: u32) -> GResult<(Option<game::buildings::Constructing>, Vec<game::buildings::Offer>)> {
        use eu4_harmony_remote::choices::{Amount, Choice, Resource};
        self.call(format!("building options {province}"));
        let constructing = game::buildings::Constructing { building: "marketplace".into(), progress: 250, ends: 0x035D_7710, builder: byz() };
        let offers = vec![
            game::buildings::Offer { action: "cancel", building: "marketplace".into(), choice: Choice::can(Vec::new()) },
            game::buildings::Offer { action: "destroy", building: "temple".into(), choice: Choice::can(Vec::new()) },
            game::buildings::Offer {
                action: "build",
                building: "workshop".into(),
                choice: Choice::cannot(vec!["建造中".into()], vec![Amount::new(Resource::Ducats, 50.0)]),
            },
        ];
        Ok((Some(constructing), offers))
    }

    fn build(&mut self, province: u32, building: String) -> GResult<()> {
        self.call(format!("build {building} {province}"));
        Err(eu4_harmony_checks::administration::not_allowed("build it", &["建造中".into()]).into())
    }

    fn cancel_building(&mut self, province: u32) -> GResult<()> {
        self.call(format!("cancel building {province}"));
        Ok(())
    }

    fn destroy_building(&mut self, province: u32, building: String) -> GResult<()> {
        self.call(format!("destroy {building} {province}"));
        Ok(())
    }
}

/// Constantinople (151), Byzantine, as the Chinese language mod names it,
/// with the Ottomans able to recruit nothing there.
pub fn constantinople() -> game::provinces::Snapshot {
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

    fn move_to(&mut self, id: u64, goal: u32) -> GResult<()> {
        self.call(format!("move {id} {goal}"));
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
}

impl extensions::Layer for Fake {
    fn reload(&mut self, id: String) -> GResult<crate::lua::Reloaded> {
        self.call(format!("reload {id}"));
        match id.as_str() {
            "regional-combat" => Ok(crate::lua::Reloaded { session: true, lost: None }),
            _ => Err(game::Fail { status: game::Status::NotFound, message: format!("no extension of a mod {id:?} is loaded") }),
        }
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

/// The camera looking at Edirne (149) from 800 high.
pub fn view() -> game::camera::Camera {
    game::camera::Camera { x: 3248, y: 1376, height: 800, province: Some(149), min_height: 50, max_height: 3000 }
}

impl camera::Layer for Fake {
    fn camera(&mut self) -> GResult<game::camera::Camera> {
        self.call("camera".into());
        Ok(view())
    }

    fn look_at_province(&mut self, province: u32) -> GResult<game::camera::Camera> {
        self.call(format!("look at province {province}"));
        match province {
            151 => Ok(game::camera::Camera { x: 3270, y: 1366, province: Some(151), ..view() }),
            _ => Err(eu4_harmony_checks::provinces::not_found(province).into()),
        }
    }

    fn look_at(&mut self, x: u32, y: u32) -> GResult<game::camera::Camera> {
        self.call(format!("look at {x} {y}"));
        eu4_harmony_checks::camera::position(x, y, (5632, 2048))?;
        Ok(game::camera::Camera { x, y, province: None, ..view() })
    }

    fn zoom(&mut self, height: u32) -> GResult<game::camera::Camera> {
        self.call(format!("zoom {height}"));
        let looking = view();
        eu4_harmony_checks::camera::height(height, (looking.min_height, looking.max_height))?;
        Ok(game::camera::Camera { height, ..view() })
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

impl desktop::Layer for Fake {
    fn desktop(&mut self) -> GResult<eu4_harmony_remote::desktop::Desktop> {
        self.call("desktop".into());
        Ok(eu4_harmony_remote::desktop::Desktop { hidden: true, hidden_start: true, sound: false })
    }

    fn show(&mut self) -> GResult<eu4_harmony_remote::desktop::Desktop> {
        self.call("show".into());
        Ok(eu4_harmony_remote::desktop::Desktop { hidden: false, hidden_start: true, sound: false })
    }

    fn hide(&mut self) -> GResult<eu4_harmony_remote::desktop::Desktop> {
        self.call("hide".into());
        Ok(eu4_harmony_remote::desktop::Desktop { hidden: true, hidden_start: true, sound: false })
    }
}

/// One decision the player can take, and one they cannot.
impl decisions::Layer for Fake {
    fn decisions(&mut self) -> GResult<Vec<eu4_harmony_remote::decisions::Decision>> {
        use eu4_harmony_remote::{choices::Choice, decisions::Decision};
        self.call("decisions".into());
        Ok(vec![
            Decision { decision: "form_rum".into(), name: "组建罗姆".into(), major: true, choice: Choice::can(Vec::new()) },
            Decision {
                decision: "move_capital".into(), name: "迁都".into(), major: false, choice: Choice::cannot(vec!["拥有君士坦丁堡".into()], Vec::new())
            },
        ])
    }
}

/// A choice of part A8's, with the one reason when it cannot be made.
fn a8_choice(can: bool, reason: &str) -> eu4_harmony_remote::choices::Choice {
    use eu4_harmony_remote::choices::Choice;
    if can {
        Choice::can(Vec::new())
    } else {
        Choice::cannot(if reason.is_empty() { Vec::new() } else { vec![reason.into()] }, Vec::new())
    }
}

/// Part A8 knows Edirne (149) and Constantinople (151) only.
fn a8_province(province: u32) -> GResult<()> {
    match province {
        149 | 151 => Ok(()),
        _ => Err(eu4_harmony_checks::provinces::not_found(province).into()),
    }
}

/// The Ottomans may send a missionary to Constantinople, not to Edirne,
/// whose religion is theirs; the game sends one of their two by itself.
impl missionaries::Layer for Fake {
    fn missionary_options(&mut self, province: u32) -> GResult<eu4_harmony_remote::missionaries::MissionaryOptions> {
        a8_province(province)?;
        Ok(eu4_harmony_remote::missionaries::MissionaryOptions {
            province: eu4_harmony_remote::ProvinceId(province),
            working: false,
            progress: None,
            send: a8_choice(province == 151, "该省份已经信仰我们的宗教"),
            recall: a8_choice(false, ""),
        })
    }

    fn send_missionary(&mut self, province: u32) -> GResult<()> {
        self.call(format!("send_missionary {province}"));
        a8_province(province)?;
        if province == 151 {
            Ok(())
        } else {
            Err(eu4_harmony_checks::envoys::not_allowed("send a missionary there", &["该省份已经信仰我们的宗教".into()]).into())
        }
    }

    fn recall_missionary(&mut self, province: u32) -> GResult<()> {
        a8_province(province)?;
        Err(eu4_harmony_checks::envoys::none_there("missionary").into())
    }

    fn automatic_missionaries(&mut self) -> GResult<eu4_harmony_remote::missionaries::AutomaticMissionaries> {
        Ok(eu4_harmony_remote::missionaries::AutomaticMissionaries { missionaries: 2, automatic: 1, raise: a8_choice(true, ""), lower: a8_choice(true, "") })
    }

    fn set_automatic_missionaries(&mut self, raise: bool) -> GResult<()> {
        self.call(format!("automatic_missionaries {raise}"));
        Ok(())
    }
}

/// The Ottomans have no colonist.
impl colonists::Layer for Fake {
    fn colonist_options(&mut self, province: u32) -> GResult<eu4_harmony_remote::colonists::ColonistOptions> {
        a8_province(province)?;
        Ok(eu4_harmony_remote::colonists::ColonistOptions {
            province: eu4_harmony_remote::ProvinceId(province),
            working: false,
            size: None,
            send: a8_choice(false, "没有可用的殖民者"),
            recall: a8_choice(false, ""),
        })
    }

    fn send_colonist(&mut self, province: u32) -> GResult<()> {
        a8_province(province)?;
        Err(eu4_harmony_checks::envoys::not_allowed("send a colonist there", &["没有可用的殖民者".into()]).into())
    }

    fn recall_colonist(&mut self, province: u32) -> GResult<()> {
        a8_province(province)?;
        Err(eu4_harmony_checks::envoys::none_there("colonist").into())
    }
}

/// No province of the Ottomans' is in a trade company region.
impl trade_companies::Layer for Fake {
    fn trade_company_options(&mut self, province: u32) -> GResult<eu4_harmony_remote::trade_companies::TradeCompanyOptions> {
        a8_province(province)?;
        Ok(eu4_harmony_remote::trade_companies::TradeCompanyOptions {
            province: eu4_harmony_remote::ProvinceId(province),
            in_trade_company: false,
            add: a8_choice(false, "不在贸易公司区域内"),
            remove: a8_choice(false, ""),
            investments: Vec::new(),
        })
    }

    fn change_trade_company(&mut self, province: u32, _: bool) -> GResult<()> {
        a8_province(province)?;
        Err(eu4_harmony_checks::envoys::not_allowed("add the province to a trade company", &["不在贸易公司区域内".into()]).into())
    }

    fn invest_in_trade_company(&mut self, province: u32, investment: &str) -> GResult<()> {
        a8_province(province)?;
        Err(eu4_harmony_checks::envoys::unknown("trade company investment", investment).into())
    }
}

/// Constantinople has a center of trade of level 1 the Ottomans can upgrade.
impl centers_of_trade::Layer for Fake {
    fn center_of_trade_options(&mut self, province: u32) -> GResult<eu4_harmony_remote::centers_of_trade::CenterOfTradeOptions> {
        use eu4_harmony_remote::choices::{Amount, Choice, Resource};
        a8_province(province)?;
        Ok(eu4_harmony_remote::centers_of_trade::CenterOfTradeOptions {
            province: eu4_harmony_remote::ProvinceId(province),
            level: 1,
            upgrade: Choice::can(vec![Amount::new(Resource::Ducats, 200.0)]),
            downgrade: Choice::cannot(vec!["已经是最低等级".into()], vec![Amount::new(Resource::Prestige, 10.0)]),
        })
    }

    fn change_center_of_trade(&mut self, province: u32, up: bool) -> GResult<()> {
        self.call(format!("center_of_trade {province} {up}"));
        a8_province(province)?;
        if up {
            Ok(())
        } else {
            Err(eu4_harmony_checks::envoys::not_allowed("downgrade the center of trade", &["已经是最低等级".into()]).into())
        }
    }
}

/// The Ottomans have two merchants, one collecting in Constantinople's node.
impl merchants::Layer for Fake {
    fn merchants(&mut self) -> GResult<eu4_harmony_remote::merchants::Merchants> {
        use eu4_harmony_remote::merchants::{Merchants, Mission, Placed};
        Ok(Merchants {
            merchants: 2,
            free: 1,
            placed: vec![Placed { node: "constantinople".into(), name: "君士坦丁堡".into(), mission: Mission::Collect, arriving: false, steer_to: None }],
        })
    }

    fn merchant_options(&mut self, node: &str) -> GResult<Box<eu4_harmony_remote::merchants::MerchantOptions>> {
        use eu4_harmony_remote::merchants::{MerchantOptions, Mission};
        if node != "constantinople" {
            return Err(eu4_harmony_checks::envoys::unknown("trade node", node).into());
        }
        Ok(Box::new(MerchantOptions {
            node: node.into(),
            name: "君士坦丁堡".into(),
            mission: Some(Mission::Collect),
            arriving: false,
            collect: a8_choice(false, "已经在收集"),
            steer: a8_choice(false, "本国首都所在的贸易节点"),
            recall: a8_choice(true, ""),
            steer_to: Vec::new(),
        }))
    }

    fn send_merchant(&mut self, node: &str, _: eu4_harmony_remote::merchants::Mission) -> GResult<()> {
        self.merchant_options(node)?;
        Err(eu4_harmony_checks::envoys::not_allowed("send a merchant there", &["已经在收集".into()]).into())
    }

    fn recall_merchant(&mut self, node: &str) -> GResult<()> {
        self.call(format!("recall_merchant {node}"));
        self.merchant_options(node).map(|_| ())
    }

    fn steer_to(&mut self, node: &str, to: &str) -> GResult<()> {
        self.merchant_options(node)?;
        Err(eu4_harmony_checks::envoys::unknown("trade node downstream", to).into())
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

impl console::Layer for Fake {
    fn console(&mut self, command: &str) -> GResult<String> {
        self.call(format!("console {command}"));
        match command {
            "cash 100" => Ok("Gold changed by 100".into()),
            _ => fail(Status::Rejected, "Unknown command"),
        }
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

/// A question of the Ottomans', its second option closed to them.
pub fn question() -> game::questions::Question {
    game::questions::Question {
        id: 7,
        event: "flavor_tur.1".into(),
        title: "The Janissaries".into(),
        description: "Our elite troops ask for more.".into(),
        // 1445.3.11
        deadline: 0x035D_8250,
        options: vec![
            game::questions::QuestionOption { index: 0, text: "Pay them".into() },
            game::questions::QuestionOption { index: 2, text: "Refuse".into() },
        ],
    }
}

impl questions::Layer for Fake {
    fn questions(&mut self) -> GResult<Vec<game::questions::Question>> {
        self.call("questions".into());
        Ok(vec![question()])
    }

    fn answer_question(&mut self, question: u32, option: u32) -> GResult<()> {
        self.call(format!("answer question {question} {option}"));
        match (question, option) {
            (7, 0 | 2) => {
                self.answered.push((question, option));
                Ok(())
            }
            (7, 1) => Err(eu4_harmony_checks::questions::not_allowed(question, option).into()),
            (7, _) => Err(eu4_harmony_checks::questions::no_option(question, option).into()),
            _ => Err(eu4_harmony_checks::questions::not_found(question).into()),
        }
    }
}

impl offers::Layer for Fake {
    fn offers(&mut self) -> GResult<Vec<game::offers::Offer>> {
        self.call("offers".into());
        // 1444.11.11
        Ok(vec![game::offers::Offer { id: 3, from: byz(), date: 0x035D_7710, title: "Royal Marriage".into(), description: "皇室联姻".into() }])
    }

    fn answer_offer(&mut self, offer: u32, accept: bool) -> GResult<()> {
        self.call(format!("answer offer {offer} {accept}"));
        if offer != 3 {
            return Err(eu4_harmony_checks::offers::not_found(offer).into());
        }
        self.answered.push((offer, accept as u32));
        Ok(())
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

    fn answer(&mut self, ticket: game::diplomacy::Ticket) -> GResult<game::diplomacy::Answer> {
        self.call(format!("answer {}", ticket.0));
        self.answers.pop_front().unwrap_or(Ok(game::diplomacy::Answer::Pending))
    }
}
