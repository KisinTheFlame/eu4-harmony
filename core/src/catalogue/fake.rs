//! A game for the engine's tests: the capability layer's answers for a small
//! made-up campaign, the Ottomans at the start, and a record of the calls.
use super::{
    armies, buildings, camera, console, countries, decisions, desktop, diplomacy, economy, extensions, focus, forts, input, leaders, loans, menu, mercenaries,
    occupation, offers, peace, provinces, questions, recruiting, screenshot, session, sieges, stability, technology, texts, time,
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

/// A general the player can recruit and one leader, Sinan, leading no army.
impl leaders::Layer for Fake {
    fn leader_options(&mut self) -> GResult<Vec<eu4_harmony_remote::leaders::LeaderOption>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            leaders::{LeaderKind, LeaderOption},
        };
        self.call("leader options".into());
        Ok(vec![
            LeaderOption { kind: LeaderKind::General, choice: Choice::can(vec![Amount::new(Resource::Mil, 50.0)]) },
            LeaderOption { kind: LeaderKind::Explorer, choice: Choice::cannot(vec!["需要探索理念".into()], vec![Amount::new(Resource::Dip, 50.0)]) },
        ])
    }

    fn recruit_leader(&mut self, kind: String) -> GResult<eu4_harmony_remote::leaders::Leader> {
        self.call(format!("recruit leader {kind}"));
        match kind.as_str() {
            "general" => Ok(sinan()),
            _ => fail(Status::Rejected, "需要探索理念"),
        }
    }

    fn leaders(&mut self) -> GResult<Vec<eu4_harmony_remote::leaders::Leader>> {
        self.call("leaders".into());
        Ok(vec![sinan()])
    }

    fn assign_leader(&mut self, leader: u32, army: u64) -> GResult<()> {
        self.call(format!("assign leader {leader} {army}"));
        Ok(())
    }

    fn remove_leader(&mut self, army: u64) -> GResult<()> {
        self.call(format!("remove leader {army}"));
        Ok(())
    }

    fn dismiss_leader(&mut self, leader: u32) -> GResult<()> {
        self.call(format!("dismiss leader {leader}"));
        match leader {
            7 => Ok(()),
            _ => fail(Status::NotFound, "the player has no leader"),
        }
    }
}

fn sinan() -> eu4_harmony_remote::leaders::Leader {
    use eu4_harmony_remote::leaders::{Leader, LeaderKind};
    Leader { id: 7, name: "锡南".into(), kind: LeaderKind::General, fire: 2, shock: 3, maneuver: 1, siege: 0, army: None }
}

impl mercenaries::Layer for Fake {
    fn companies(&mut self) -> GResult<Vec<eu4_harmony_remote::mercenaries::Company>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            mercenaries::Company,
        };
        self.call("companies".into());
        Ok(vec![Company {
            company: "merc_black_army".into(),
            name: "黑军".into(),
            regiments: 8,
            upkeep: 2.5,
            choice: Choice::can(vec![Amount::new(Resource::Ducats, 120.0)]),
        }])
    }

    fn hire(&mut self, company: String, province: u32) -> GResult<()> {
        self.call(format!("hire {company} {province}"));
        match company.as_str() {
            "merc_black_army" => Ok(()),
            _ => fail(Status::NotFound, "the mercenary view lists no such company"),
        }
    }
}

/// Edirne's fort, open.
impl forts::Layer for Fake {
    fn forts(&mut self) -> GResult<Vec<eu4_harmony_remote::forts::Fort>> {
        use eu4_harmony_remote::{choices::Choice, forts::Fort, ProvinceId};
        self.call("forts".into());
        Ok(vec![Fort { province: ProvinceId(149), level: 2, mothballed: false, choice: Choice::can(Vec::new()) }])
    }

    fn mothball_fort(&mut self, province: u32) -> GResult<()> {
        self.call(format!("mothball fort {province}"));
        Ok(())
    }

    fn reopen_fort(&mut self, province: u32) -> GResult<()> {
        self.call(format!("reopen fort {province}"));
        fail(Status::Rejected, "the fort is open already")
    }
}

/// A siege of Constantinople with no breach yet.
impl sieges::Layer for Fake {
    fn siege_options(&mut self, province: u32) -> GResult<Vec<eu4_harmony_remote::sieges::SiegeAction>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            sieges::SiegeAction,
        };
        self.call(format!("siege options {province}"));
        Ok(vec![
            SiegeAction { action: "bombard".into(), choice: Choice::can(vec![Amount::new(Resource::Mil, 50.0)]) },
            SiegeAction {
                action: "assault".into(), choice: Choice::cannot(vec!["没有缺口，无法强攻要塞。".into()], vec![Amount::new(Resource::Mil, 5.0)])
            },
        ])
    }

    fn bombard(&mut self, province: u32) -> GResult<()> {
        self.call(format!("bombard {province}"));
        Ok(())
    }

    fn naval_bombard(&mut self, province: u32) -> GResult<()> {
        self.call(format!("naval bombard {province}"));
        fail(Status::Rejected, "附近海域没有舰队")
    }

    fn assault(&mut self, province: u32) -> GResult<()> {
        self.call(format!("assault {province}"));
        fail(Status::Rejected, "没有缺口，无法强攻要塞。")
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

/// The Ottomans' technologies: administrative and diplomatic they can
/// research, military they cannot afford.
impl technology::Layer for Fake {
    fn technologies(&mut self) -> GResult<Vec<eu4_harmony_remote::technology::Technology>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, PowerKind, Resource},
            technology::Technology,
        };
        self.call("technologies".into());
        let at = |technology, level, choice| Technology { technology, level, choice };
        Ok(vec![
            at(PowerKind::Adm, 3, Choice::can(vec![Amount::new(Resource::Adm, 600.0)])),
            at(PowerKind::Dip, 3, Choice::can(vec![Amount::new(Resource::Dip, 600.0)])),
            at(PowerKind::Mil, 3, Choice::cannot(vec![LATER_POINTS.into()], vec![Amount::new(Resource::Mil, 600.0)])),
        ])
    }

    fn research(&mut self, technology: eu4_harmony_remote::choices::PowerKind) -> GResult<()> {
        self.call(format!("research {}", game::technology::key(technology)));
        match technology {
            eu4_harmony_remote::choices::PowerKind::Mil => Err(eu4_harmony_checks::choices::not_allowed("research it", &[LATER_POINTS.into()]).into()),
            _ => Ok(()),
        }
    }
}

/// The technology view's line for the military technology the fake
/// Ottomans cannot afford.
const LATER_POINTS: &str = "我们预计在1446年5月1日拥有足够的点数。";
/// The court view's line for a focus that cannot change yet.
const FOCUS_LATER: &str = "你每5年才能更改一次国家重心。";

/// The Ottomans' focus on the military, which cannot change yet.
impl focus::Layer for Fake {
    fn focus(&mut self) -> GResult<eu4_harmony_remote::focus::Focus> {
        use eu4_harmony_remote::{
            choices::{Choice, PowerKind},
            focus::{Focus, FocusChoice},
        };
        self.call("focus".into());
        let later = || Choice::cannot(vec![FOCUS_LATER.into()], Vec::new());
        Ok(Focus {
            focus: Some(PowerKind::Mil),
            choose: vec![
                FocusChoice { focus: PowerKind::Adm, choice: later() },
                FocusChoice { focus: PowerKind::Dip, choice: later() },
                FocusChoice { focus: PowerKind::Mil, choice: Choice::cannot(Vec::new(), Vec::new()) },
            ],
            cancel: later(),
        })
    }

    fn choose_focus(&mut self, focus: eu4_harmony_remote::choices::PowerKind) -> GResult<()> {
        self.call(format!("choose focus {}", game::technology::key(focus)));
        if focus == eu4_harmony_remote::choices::PowerKind::Mil {
            return Err(eu4_harmony_checks::focus::already("mil").into());
        }
        Err(eu4_harmony_checks::choices::not_allowed("change the national focus", &[FOCUS_LATER.into()]).into())
    }

    fn cancel_focus(&mut self) -> GResult<()> {
        self.call("cancel focus".into());
        Err(eu4_harmony_checks::choices::not_allowed("change the national focus", &[FOCUS_LATER.into()]).into())
    }
}

/// Stability the Ottomans can raise.
impl stability::Layer for Fake {
    fn stability_options(&mut self) -> GResult<Vec<eu4_harmony_remote::stability::StabilityOption>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            stability::{StabilityAction, StabilityOption},
        };
        self.call("stability options".into());
        Ok(vec![StabilityOption { action: StabilityAction::Raise, choice: Choice::can(vec![Amount::new(Resource::Adm, 100.0)]) }])
    }

    fn raise_stability(&mut self) -> GResult<()> {
        self.call("raise stability".into());
        Ok(())
    }
}

/// The economy view's line for inflation too low to reduce.
const NO_INFLATION: &str = "你目前的通货膨胀不够高。";

/// A loan the Ottomans can take; no inflation to reduce.
impl economy::Layer for Fake {
    fn economy_options(&mut self) -> GResult<Vec<eu4_harmony_remote::economy::EconomyOption>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            economy::{EconomyAction, EconomyOption},
        };
        self.call("economy options".into());
        Ok(vec![
            EconomyOption { action: EconomyAction::TakeLoan, choice: Choice::can(Vec::new()), gives: vec![Amount::new(Resource::Ducats, 85.0)] },
            EconomyOption {
                action: EconomyAction::ReduceInflation,
                choice: Choice::cannot(vec![NO_INFLATION.into()], vec![Amount::new(Resource::Adm, 75.0)]),
                gives: Vec::new(),
            },
            EconomyOption { action: EconomyAction::RaiseMercantilism, choice: Choice::can(vec![Amount::new(Resource::Dip, 100.0)]), gives: Vec::new() },
        ])
    }

    fn take_loan(&mut self) -> GResult<()> {
        self.call("take loan".into());
        Ok(())
    }

    fn reduce_inflation(&mut self) -> GResult<()> {
        self.call("reduce inflation".into());
        Err(eu4_harmony_checks::choices::not_allowed("reduce inflation", &[NO_INFLATION.into()]).into())
    }

    fn raise_mercantilism(&mut self) -> GResult<()> {
        self.call("raise mercantilism".into());
        Ok(())
    }
}

/// One bank loan of the Ottomans', which they can repay.
impl loans::Layer for Fake {
    fn loans(&mut self) -> GResult<Vec<eu4_harmony_remote::loans::Loan>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            loans::Loan,
            Date,
        };
        self.call("loans".into());
        let expires = Date::from_ymd(1449, 11, 11).unwrap();
        Ok(vec![Loan {
            loan: 7,
            lender: None,
            estate: false,
            amount: 85.0,
            interest: 4.0,
            expires,
            repay: Choice::can(vec![Amount::new(Resource::Ducats, 101.5)]),
        }])
    }

    fn repay_loan(&mut self, loan: u32) -> GResult<()> {
        self.call(format!("repay loan {loan}"));
        if loan == 7 {
            Ok(())
        } else {
            Err(eu4_harmony_checks::loans::not_found(loan).into())
        }
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
