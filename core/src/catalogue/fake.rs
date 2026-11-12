//! A game for the tests of the catalogue's groups that do more than call the
//! capability layer, such as time's advance and the menu's waits, and of the
//! pipe: the layer's answers for a small made-up campaign, the Ottomans at
//! the start, and a record of the calls. A capability of a group it does not
//! script fails (`unscripted`), as every request of a group bound to the
//! layer (`catalogue::bind`) does.
// Every group's module, by its name.
use super::*;
use crate::game::{
    self, fail,
    menu::{Expected, ExpectedProbe, Launch},
    GResult, Status, Tag,
};
use eu4_harmony_remote::{countries::Country, response_json, CountryTag};
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
    /// The player, as the session gives it; the Ottomans played when None.
    pub player: Option<game::player::Player>,
}

impl Fake {
    fn call(&mut self, what: String) {
        std::thread::sleep(self.delay);
        self.calls.push(what);
    }
}

// The capability layer of the made-up campaign, called as no one: the
// bound groups' requests fail here, as a capability it does not script does.
impl Game for Fake {
    fn as_game(&mut self) -> &mut dyn Game {
        self
    }

    fn caller(&self) -> Option<game::Caller> {
        None
    }
}

/// An army of the made-up campaign, as a capability answers it.
fn army(id: u64, owner: &[u8; 3], kind: eu4_harmony_remote::armies::ArmyKind, province: Option<u32>, regiments: u32) -> eu4_harmony_remote::armies::Army {
    eu4_harmony_remote::armies::Army {
        id: eu4_harmony_remote::ArmyId(id),
        owner: CountryTag(*owner),
        kind,
        province: province.map(eu4_harmony_remote::ProvinceId),
        regiments,
        blocked: Vec::new(),
        route: Vec::new(),
        route_by: None,
        aboard: None,
        carrying: 0,
        units: eu4_harmony_remote::armies::Units { infantry: regiments, ..Default::default() },
        strength: f64::from(regiments),
        max_strength: f64::from(regiments),
        morale: 3.0,
        max_morale: 3.0,
        leader: None,
        maintenance: Some(0.5 * f64::from(regiments)),
    }
}

impl session::Layer for Fake {
    fn session(&mut self, _: eu4_harmony_remote::session::args::Get) -> GResult<game::session::Session> {
        self.call("session".into());
        // 1444.11.11
        let player = self.player.unwrap_or(game::player::Player::Plays(tur()));
        Ok(game::session::Session { session: 2, date: 0x035D_7710, paused: true, visibility_current: true, player })
    }

    fn leave_view(&mut self, _: eu4_harmony_remote::session::args::LeaveView) -> GResult<game::session::Session> {
        self.call("leave view".into());
        if let Some(game::player::Player::Observes { in_view, .. }) = &mut self.player {
            *in_view = None;
        }
        session::Layer::session(self, eu4_harmony_remote::session::args::Get {})
    }
}

impl countries::Layer for Fake {
    fn hostile(&mut self, eu4_harmony_remote::countries::args::Hostile { country, other }: eu4_harmony_remote::countries::args::Hostile) -> GResult<bool> {
        self.call(format!("hostile {country} {other}"));
        let (a, b) = (found(country)?, found(other)?);
        Ok((a, b) == (tur(), byz()))
    }

    /// The Ottomans at war with Byzantium, with Wallachia as their vassal;
    /// Byzantium with nothing; [`wallachia`].
    fn country(&mut self, eu4_harmony_remote::countries::args::Get { country }: eu4_harmony_remote::countries::args::Get) -> GResult<Box<Country>> {
        use eu4_harmony_remote::countries::{
            GoverningCapacity, IdeaGroup, Legitimacy, LegitimacyKind, Loans, ModifierValue, NamedModifier, Person, Power, Rebels, RelationKind, Technology,
        };
        self.call(format!("country {country}"));
        let found = found(country)?;
        if found == byz() {
            return Ok(Box::new(bare(country)));
        }
        if found == wal() {
            return Ok(Box::new(wallachia()));
        }
        Ok(Box::new(Country {
            tag: country,
            treasury: Some(125.5),
            manpower: 22_600,
            max_manpower: 30_250,
            stability: 1.0,
            prestige: -12.345,
            power: Some(Power { adm: 50, dip: 60, mil: 70 }),
            wars: vec![war("Ottoman Conquest of Constantinople", &["TUR", "WAL"], &["BYZ"])],
            relations: vec![relation(RelationKind::Subject, "WAL", Some("vassal")), relation(RelationKind::Rival, "BYZ", None)],
            technology: Technology { adm: 3, dip: 3, mil: 4 },
            ideas: vec![
                IdeaGroup { group: "TUR_ideas".into(), ideas: 1, national: true },
                IdeaGroup { group: "quality_ideas".into(), ideas: 2, national: false },
            ],
            governing_capacity: Some(GoverningCapacity { used: 512.5, max: 700.0 }),
            inflation: Some(2.5),
            mercantilism: Some(10.0),
            loans: Some(Loans { count: 1, ducats: 120.0 }),
            rebels: Some(vec![Rebels { kind: "nationalist_rebels".into(), name: "Albanian Nationalists".into(), progress: 12.5 }]),
            ruler: Some(Person { name: "Mehmed II".into(), dynasty: Some("Osmanoğlu".into()), adm: 5, dip: 4, mil: 6, age: 21 }),
            heir: None,
            consort: None,
            legitimacy: Some(Legitimacy {
                kind: LegitimacyKind::Legitimacy,
                value: 90.0,
                max: 100.0,
                effects: Some(vec![ModifierValue { modifier: "global_unrest".into(), value: -0.8 }]),
            }),
            corruption: Some(1.5),
            absolutism: Some(20.0),
            army_professionalism: Some(12.5),
            army_tradition: 35.25,
            navy_tradition: 10.0,
            named_modifiers: Some(vec![NamedModifier { key: "janissary_decadence".into(), expires: None }]),
        }))
    }

    /// The Ottomans' budget, one line of each table and a loan from the
    /// bank; another country's as the player's view leaves it.
    fn budget(
        &mut self,
        eu4_harmony_remote::countries::args::Budget { country }: eu4_harmony_remote::countries::args::Budget,
    ) -> GResult<eu4_harmony_remote::countries::Budget> {
        use eu4_harmony_remote::countries::{Budget, Entry, Loan};
        self.call(format!("budget {country}"));
        let entry = |category: &str, name: &str, ducats| Entry { category: category.into(), name: name.into(), ducats };
        if found(country)? != tur() {
            return Ok(Budget { country, income: None, expenses: None, loans: None });
        }
        Ok(Budget {
            country,
            income: Some(vec![entry("INCOMETAX", "税收", 4.25)]),
            expenses: Some(vec![entry("EXPENSEAM", "陆军维护费", 3.5)]),
            loans: Some(vec![Loan { lender: None, ducats: 120.0, interest: 4.0, due: eu4_harmony_remote::Date(0x035d_7710 + 5 * 8760) }]),
        })
    }
}

/// The country of the made-up campaign a tag names: the Ottomans, Byzantium
/// or Wallachia, refused as the layer refuses any other.
pub fn found(tag: CountryTag) -> GResult<Tag> {
    match &tag.0 {
        b"TUR" => Ok(tur()),
        b"BYZ" => Ok(byz()),
        b"WAL" => Ok(wal()),
        _ => Err(eu4_harmony_checks::countries::not_found(&tag.to_string()).into()),
    }
}

/// A war of these sides, by their tags.
fn war(name: &str, attackers: &[&str], defenders: &[&str]) -> eu4_harmony_remote::countries::War {
    let tags = |tags: &[&str]| tags.iter().map(|t| letters(t)).collect();
    eu4_harmony_remote::countries::War { name: name.into(), attackers: tags(attackers), defenders: tags(defenders) }
}

/// A relation to the country of this tag.
fn relation(kind: eu4_harmony_remote::countries::RelationKind, country: &str, subject_type: Option<&str>) -> eu4_harmony_remote::countries::Relation {
    eu4_harmony_remote::countries::Relation { kind, country: letters(country), subject_type: subject_type.map(Into::into) }
}

/// A country of whom nothing is known but its tag.
fn bare(tag: CountryTag) -> Country {
    Country {
        tag,
        treasury: None,
        manpower: 0,
        max_manpower: 0,
        stability: 0.0,
        prestige: 0.0,
        power: None,
        wars: Vec::new(),
        relations: Vec::new(),
        technology: Default::default(),
        ideas: Vec::new(),
        governing_capacity: None,
        inflation: None,
        mercantilism: None,
        loans: None,
        rebels: None,
        ruler: None,
        heir: None,
        consort: None,
        legitimacy: None,
        corruption: None,
        absolutism: None,
        army_professionalism: None,
        army_tradition: 0.0,
        navy_tradition: 0.0,
        named_modifiers: None,
    }
}

/// A tag of three letters.
fn letters(tag: &str) -> CountryTag {
    CountryTag(tag.as_bytes().try_into().unwrap())
}

/// Wallachia, the Ottomans' subject in their war on Byzantium, made up to
/// hold a relation of every kind (Moldavia its ally, Transylvania a march
/// of its own, the Ottomans its overlord, Hungary its rival) and a war on
/// either side: Hungary's war on it besides.
pub fn wallachia() -> Country {
    use eu4_harmony_remote::countries::{Power, RelationKind};
    Country {
        treasury: Some(31.25),
        manpower: 5_400,
        max_manpower: 8_250,
        stability: -1.0,
        prestige: 12.345,
        power: Some(Power { adm: 120, dip: 95, mil: 143 }),
        wars: vec![war("Ottoman Conquest of Constantinople", &["TUR", "WAL"], &["BYZ"]), war("Hungarian-Wallachian War", &["HUN"], &["WAL", "MOL"])],
        relations: vec![
            relation(RelationKind::Ally, "MOL", None),
            relation(RelationKind::Subject, "TRA", Some("march")),
            relation(RelationKind::Overlord, "TUR", Some("vassal")),
            relation(RelationKind::Rival, "HUN", None),
        ],
        ..bare(letters("WAL"))
    }
}

impl provinces::Layer for Fake {
    /// Edirne, the Ottomans', and Constantinople, Byzantine and occupied by
    /// them.
    fn provinces(
        &mut self,
        eu4_harmony_remote::provinces::args::List { provinces }: eu4_harmony_remote::provinces::args::List,
    ) -> GResult<Vec<eu4_harmony_remote::provinces::Province>> {
        use eu4_harmony_remote::provinces::{Province, ProvinceKind};
        self.call("provinces".into());
        let province = |id, owner: &[u8; 3], controller: &[u8; 3]| Province {
            id: eu4_harmony_remote::ProvinceId(id),
            kind: ProvinceKind::Land,
            visible: true,
            owner: Some(CountryTag(*owner)),
            controller: Some(CountryTag(*controller)),
        };
        let all = [province(149, b"TUR", b"TUR"), province(151, b"BYZ", b"TUR")];
        Ok(match provinces {
            None => all.to_vec(),
            Some(asked) => asked.0.iter().filter_map(|id| all.iter().find(|p| p.id == *id).copied()).collect(),
        })
    }

    fn neighbors(
        &mut self,
        eu4_harmony_remote::provinces::args::Neighbors { province: eu4_harmony_remote::ProvinceId(province) }: eu4_harmony_remote::provinces::args::Neighbors,
    ) -> GResult<Vec<eu4_harmony_remote::provinces::Neighbor>> {
        use eu4_harmony_remote::{provinces::Neighbor, ProvinceId};
        self.call(format!("neighbors {province}"));
        Ok(vec![Neighbor { province: ProvinceId(151), strait: false }, Neighbor { province: ProvinceId(316), strait: true }])
    }

    fn province(
        &mut self,
        eu4_harmony_remote::provinces::args::Get { province: eu4_harmony_remote::ProvinceId(province) }: eu4_harmony_remote::provinces::args::Get,
    ) -> GResult<Box<eu4_harmony_remote::provinces::ProvinceSnapshot>> {
        self.call(format!("province {province}"));
        if province != 151 {
            return Err(eu4_harmony_checks::provinces::not_found(province).into());
        }
        Ok(Box::new(constantinople()))
    }

    fn province_area(&mut self, args: eu4_harmony_remote::provinces::args::Area) -> GResult<eu4_harmony_remote::provinces::AreaSnapshot> {
        use eu4_harmony_remote::provinces::{AreaSnapshot, StateKind};
        self.call(format!("province area {}", args.province.0));
        Ok(AreaSnapshot {
            area: "thrace_area".into(),
            owner: letters("TUR"),
            provinces: vec![args.province],
            development: 25.5,
            trade_company: false,
            state: StateKind::Territory,
            governing_cost: 6375,
            make_state_cost: Some(6375),
            governing_capacity_used: 512500,
            governing_capacity_max: 700000,
            unavailable_reason: None,
        })
    }

    /// In Constantinople the player can develop tax for 50 administrative
    /// points and cannot core it; elsewhere nothing.
    fn province_options(
        &mut self,
        eu4_harmony_remote::provinces::args::Options { province: eu4_harmony_remote::ProvinceId(province) }: eu4_harmony_remote::provinces::args::Options,
    ) -> GResult<Vec<eu4_harmony_remote::provinces::ProvinceOption>> {
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
            _ => Err(crate::game::action::not_allowed("do it there", &["已经是核心省份".into()])),
        }
    }

    fn set_edict(
        &mut self,
        eu4_harmony_remote::provinces::args::SetEdict { province: eu4_harmony_remote::ProvinceId(province), edict }: eu4_harmony_remote::provinces::args::SetEdict,
    ) -> GResult<()> {
        self.call(format!("set edict {edict} {province}"));
        Err(crate::game::administration::checks::no_such_edict(&edict))
    }
}

/// Constantinople (151), Byzantine, as the Chinese language mod names it.
pub fn constantinople() -> eu4_harmony_remote::provinces::ProvinceSnapshot {
    use eu4_harmony_remote::provinces::{Development, Fort, ProvinceSnapshot, StateKind};
    ProvinceSnapshot {
        id: eu4_harmony_remote::ProvinceId(151),
        name: "君士坦丁堡".into(),
        owner: Some(CountryTag(*b"BYZ")),
        controller: Some(CountryTag(*b"BYZ")),
        development: Development { tax: 10.0, production: 9.5, manpower: 6.0 },
        terrain: Some("farmlands".into()),
        fort: Fort { level: 2, mothballed: false },
        buildings: vec!["fort_15th".into(), "marketplace".into()],
        autonomy: 25.0,
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
        named_modifiers: Vec::new(),
        prosperity: Some(40.0),
        devastation: 0.0,
        local_manpower: 1.5,
        supply_limit: 12.5,
        separatism: 0,
        institutions: vec![eu4_harmony_remote::provinces::InstitutionProgress { institution: "feudalism".into(), progress: 100.0 }],
    }
}

/// The Ottomans' army in Edirne, on its route, and their fleet nowhere.
impl armies::Layer for Fake {
    fn army(
        &mut self,
        eu4_harmony_remote::armies::args::Get { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Get,
    ) -> GResult<eu4_harmony_remote::armies::Army> {
        use eu4_harmony_remote::armies::ArmyKind;
        self.call(format!("army {id}"));
        match id {
            ARMY => Ok(eu4_harmony_remote::armies::Army {
                route: self.route.iter().copied().map(eu4_harmony_remote::ProvinceId).collect(),
                ..army(ARMY, b"TUR", ArmyKind::Army, Some(149), 12)
            }),
            FLEET => Ok(army(FLEET, b"TUR", ArmyKind::Navy, None, 0)),
            _ => Err(eu4_harmony_checks::armies::not_own(id).into()),
        }
    }

    fn armies(&mut self, _: eu4_harmony_remote::armies::args::List) -> GResult<Vec<eu4_harmony_remote::armies::Army>> {
        use eu4_harmony_remote::armies::ArmyKind;
        self.call("own armies".into());
        Ok(vec![army(ARMY, b"TUR", ArmyKind::Army, Some(149), 12), army(FLEET, b"TUR", ArmyKind::Navy, None, 0)])
    }

    /// A Byzantine army besieging there, that another extension moved.
    fn armies_at(
        &mut self,
        eu4_harmony_remote::armies::args::At { province: eu4_harmony_remote::ProvinceId(province) }: eu4_harmony_remote::armies::args::At,
    ) -> GResult<Vec<eu4_harmony_remote::armies::Army>> {
        use eu4_harmony_remote::armies::{ArmyKind, Blocked, RouteBy};
        self.call(format!("armies in {province}"));
        Ok(vec![eu4_harmony_remote::armies::Army {
            blocked: vec![Blocked::Sieging],
            route_by: Some(RouteBy::OtherMod),
            ..army(ENEMY, b"BYZ", ArmyKind::Army, Some(province), 0)
        }])
    }

    fn can_step(
        &mut self,
        eu4_harmony_remote::armies::args::CanStep {
            army: eu4_harmony_remote::ArmyId(id),
            from: eu4_harmony_remote::ProvinceId(from),
            to: eu4_harmony_remote::ProvinceId(to),
        }: eu4_harmony_remote::armies::args::CanStep,
    ) -> GResult<bool> {
        self.call(format!("can step {id} {from} {to}"));
        Ok(to == 151)
    }

    fn can_attack(
        &mut self,
        eu4_harmony_remote::armies::args::CanAttack { army: eu4_harmony_remote::ArmyId(id), target: eu4_harmony_remote::ArmyId(target) }: eu4_harmony_remote::armies::args::CanAttack,
    ) -> GResult<bool> {
        self.call(format!("can attack {id} {target}"));
        Ok(target == ENEMY)
    }

    fn find_route(
        &mut self,
        eu4_harmony_remote::armies::args::RouteTo { army: eu4_harmony_remote::ArmyId(id), province: eu4_harmony_remote::ProvinceId(goal) }: eu4_harmony_remote::armies::args::RouteTo,
    ) -> GResult<Vec<u32>> {
        self.call(format!("find route {id} {goal}"));
        Ok(vec![150, goal])
    }

    fn move_to(
        &mut self,
        eu4_harmony_remote::armies::args::Move { army: eu4_harmony_remote::ArmyId(id), province: eu4_harmony_remote::ProvinceId(goal), transports }: eu4_harmony_remote::armies::args::Move,
    ) -> GResult<()> {
        self.call(format!("move {id} {goal}{}", if transports.unwrap_or(false) { " by sea" } else { "" }));
        if goal == 1 {
            return fail(Status::Rejected, "no route to 1");
        }
        self.route = vec![150, goal];
        Ok(())
    }

    fn stop(&mut self, eu4_harmony_remote::armies::args::Stop { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Stop) -> GResult<()> {
        self.call(format!("stop {id}"));
        self.route.clear();
        Ok(())
    }

    fn selected(&mut self, _: eu4_harmony_remote::armies::args::Selected) -> GResult<u64> {
        self.call("selected".into());
        fail(Status::NotFound, "army panel not open")
    }

    /// The army may split and drill; it is alone in its province.
    fn army_options(
        &mut self,
        eu4_harmony_remote::armies::args::Options { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Options,
    ) -> GResult<Vec<eu4_harmony_remote::armies::ArmyAction>> {
        use eu4_harmony_remote::{armies::ArmyAction, choices::Choice};
        self.call(format!("army options {id}"));
        Ok(vec![ArmyAction::new("split", Choice::can(Vec::new())), ArmyAction::new("merge", Choice::cannot(vec!["至少需要两支部队".into()], Vec::new()))])
    }

    fn detach_kind(
        &mut self,
        eu4_harmony_remote::armies::args::DetachKind { army: eu4_harmony_remote::ArmyId(id), kind }: eu4_harmony_remote::armies::args::DetachKind,
    ) -> GResult<u64> {
        self.call(format!("detach kind {id} {kind:?}"));
        Ok(id + 1)
    }

    fn attach(
        &mut self,
        eu4_harmony_remote::armies::args::Attach { army: eu4_harmony_remote::ArmyId(id), target: eu4_harmony_remote::ArmyId(target) }: eu4_harmony_remote::armies::args::Attach,
    ) -> GResult<()> {
        self.call(format!("attach {id} {target}"));
        Ok(())
    }

    fn detach(
        &mut self,
        eu4_harmony_remote::armies::args::Detach { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Detach,
    ) -> GResult<()> {
        self.call(format!("detach {id}"));
        fail(Status::Rejected, "the unit is not attached")
    }

    fn attach_to_transport(
        &mut self,
        eu4_harmony_remote::armies::args::AttachToTransport { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::AttachToTransport,
    ) -> GResult<()> {
        self.call(format!("attach to transport {id}"));
        Ok(())
    }

    fn detach_from_transport(
        &mut self,
        eu4_harmony_remote::armies::args::DetachFromTransport { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::DetachFromTransport,
    ) -> GResult<()> {
        self.call(format!("detach from transport {id}"));
        Ok(())
    }

    fn detach_special_units(
        &mut self,
        eu4_harmony_remote::armies::args::DetachSpecialUnits { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::DetachSpecialUnits,
    ) -> GResult<u64> {
        self.call(format!("detach special units {id}"));
        Ok(id + 1)
    }

    fn auto_transport(
        &mut self,
        eu4_harmony_remote::armies::args::AutoTransport { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::AutoTransport,
    ) -> GResult<()> {
        self.call(format!("auto transport {id}"));
        Ok(())
    }

    fn stop_auto_transport(
        &mut self,
        eu4_harmony_remote::armies::args::StopAutoTransport { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::StopAutoTransport,
    ) -> GResult<()> {
        self.call(format!("stop auto transport {id}"));
        Ok(())
    }

    fn split(
        &mut self,
        eu4_harmony_remote::armies::args::Split { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Split,
    ) -> GResult<u64> {
        self.call(format!("split {id}"));
        Ok(id + 1)
    }

    fn merge(
        &mut self,
        eu4_harmony_remote::armies::args::Merge { armies: eu4_harmony_remote::ArmyList(ids) }: eu4_harmony_remote::armies::args::Merge,
    ) -> GResult<()> {
        self.call(format!("merge {}", ids.len()));
        match ids.len() {
            0 | 1 => fail(Status::Rejected, "至少需要两支部队"),
            _ => Ok(()),
        }
    }

    fn disband(
        &mut self,
        eu4_harmony_remote::armies::args::Disband { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Disband,
    ) -> GResult<()> {
        self.call(format!("disband {id}"));
        Ok(())
    }

    fn consolidate(
        &mut self,
        eu4_harmony_remote::armies::args::Consolidate { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Consolidate,
    ) -> GResult<()> {
        self.call(format!("consolidate {id}"));
        Ok(())
    }

    fn drill(
        &mut self,
        eu4_harmony_remote::armies::args::Drill { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::Drill,
    ) -> GResult<()> {
        self.call(format!("drill {id}"));
        Ok(())
    }

    fn stop_drill(
        &mut self,
        eu4_harmony_remote::armies::args::StopDrill { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::StopDrill,
    ) -> GResult<()> {
        self.call(format!("stop drill {id}"));
        fail(Status::Rejected, "the game does not let the player stop drilling: the army does not drill now, and gives no reason")
    }

    fn mission(
        &mut self,
        eu4_harmony_remote::armies::args::Mission { army: eu4_harmony_remote::ArmyId(id), mission, target }: eu4_harmony_remote::armies::args::Mission,
    ) -> GResult<()> {
        self.call(format!("mission {id} {mission} {target:?}"));
        Ok(())
    }

    fn stop_mission(
        &mut self,
        eu4_harmony_remote::armies::args::StopMission { army: eu4_harmony_remote::ArmyId(id) }: eu4_harmony_remote::armies::args::StopMission,
    ) -> GResult<()> {
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

    fn start(&mut self, country: Option<CountryTag>) -> GResult<()> {
        self.call(country.map_or("start observing".into(), |country| format!("start {country}")));
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
    fn language(&mut self, _: eu4_harmony_remote::texts::args::Language) -> GResult<&'static str> {
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
/// theirs, the game refuses with its reason, and they can recruit nothing
/// there.
impl recruiting::Layer for Fake {
    fn recruit_options(
        &mut self,
        eu4_harmony_remote::recruiting::args::Options { province: eu4_harmony_remote::ProvinceId(province) }: eu4_harmony_remote::recruiting::args::Options,
    ) -> GResult<Vec<eu4_harmony_remote::recruiting::Recruit>> {
        use eu4_harmony_remote::recruiting::{Recruit, UnitKind};
        self.call(format!("recruit options {province}"));
        if province != 151 {
            return Err(eu4_harmony_checks::provinces::not_found(province).into());
        }
        let recruit = |kind, unit_type: Option<&str>, reasons: &[&str]| Recruit {
            kind,
            unit_type: unit_type.map(str::to_string),
            allowed: false,
            reasons: reasons.iter().map(|r| r.to_string()).collect(),
        };
        Ok(vec![
            recruit(UnitKind::Infantry, Some("eastern_medieval_infantry"), &["君士坦丁堡不是我们的领土"]),
            recruit(UnitKind::Cavalry, Some("eastern_knights"), &["君士坦丁堡不是我们的领土"]),
            recruit(UnitKind::Artillery, None, &[]),
            recruit(UnitKind::HeavyShip, Some("early_carrack"), &[]),
            recruit(UnitKind::LightShip, Some("early_war_galley"), &[]),
            recruit(UnitKind::Galley, Some("galley"), &[]),
            recruit(UnitKind::Transport, Some("cog"), &[]),
        ])
    }

    fn recruit(
        &mut self,
        eu4_harmony_remote::recruiting::args::Order { province: eu4_harmony_remote::ProvinceId(province), kind }: eu4_harmony_remote::recruiting::args::Order,
    ) -> GResult<()> {
        self.call(format!("recruit {province} {}", game::provinces::UnitKind::from(kind).key()));
        match province {
            149 => Ok(()),
            151 => fail(Status::Rejected, "君士坦丁堡不是我们的领土"),
            _ => Err(eu4_harmony_checks::provinces::not_found(province).into()),
        }
    }
}

/// The war goals the Ottomans have against Byzantium: a claim on
/// Constantinople, and the declaration without a casus belli.
pub fn war_goals() -> Vec<eu4_harmony_remote::diplomacy::WarGoal> {
    use eu4_harmony_remote::{diplomacy::WarGoal, ProvinceId};
    vec![
        WarGoal {
            casus_belli: "cb_claim".into(),
            war_goal: Some("take_claim".into()),
            province: Some(ProvinceId(151)),
            target: None,
            name: "Conquer Constantinople".into(),
        },
        WarGoal { casus_belli: "none".into(), war_goal: Some("nowargoal".into()), province: None, target: None, name: "No War Goal".into() },
    ]
}

impl diplomacy::Layer for Fake {
    fn war_goals(
        &mut self,
        eu4_harmony_remote::diplomacy::args::WarGoals { country }: eu4_harmony_remote::diplomacy::args::WarGoals,
    ) -> GResult<Vec<eu4_harmony_remote::diplomacy::WarGoal>> {
        self.call(format!("war goals {country}"));
        Ok(if found(country)? == byz() { war_goals() } else { Vec::new() })
    }

    /// Wallachia is the ally the window lets the player call.
    fn declare_war(
        &mut self,
        eu4_harmony_remote::diplomacy::args::DeclareWar { country, casus_belli, war_goal, province, target, call_allies }: eu4_harmony_remote::diplomacy::args::DeclareWar,
    ) -> GResult<game::diplomacy::Declaration> {
        use eu4_harmony_remote::diplomacy::{CallAllies, PassedOver};
        let calls = call_allies.unwrap_or_default();
        let calling = match &calls {
            CallAllies::Only(tags) => format!("Only([{}])", tags.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")),
            CallAllies::All => "All".into(),
            CallAllies::None => "None".into(),
        };
        self.call(format!(
            "declare war {country} {casus_belli:?} {war_goal:?} {:?} {:?} calling {calling}",
            province.map(|p| p.0),
            target.map(|t| t.to_string())
        ));
        let enemy = found(country)?;
        target.map(found).transpose()?;
        if enemy == wal() {
            return fail(
                Status::Rejected,
                "They are our subject.
We have a truce with them.",
            );
        }
        let wallachia = CountryTag(*b"WAL");
        let called = match &calls {
            CallAllies::All => vec![wallachia],
            CallAllies::None => Vec::new(),
            CallAllies::Only(tags) => tags.iter().map(|&t| found(t).map(|_| t)).collect::<GResult<Vec<_>>>()?.into_iter().filter(|&t| t == wallachia).collect(),
        };
        // Under every ally, Turkey is passed over, as the window's check
        // would not let the player call it.
        let passed_over = match calls {
            CallAllies::All => vec![PassedOver { country: CountryTag(*b"TUR"), reasons: vec!["They would not accept.".into()] }],
            _ => Vec::new(),
        };
        Ok(game::diplomacy::Declaration { ticket: game::diplomacy::Ticket(1), called, passed_over })
    }

    fn offer_peace(
        &mut self,
        eu4_harmony_remote::diplomacy::args::OfferPeace { country }: eu4_harmony_remote::diplomacy::args::OfferPeace,
    ) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("offer peace {country}"));
        found(country)?;
        Ok(game::diplomacy::Ticket(2))
    }

    fn propose_alliance(
        &mut self,
        eu4_harmony_remote::diplomacy::args::ProposeAlliance { country }: eu4_harmony_remote::diplomacy::args::ProposeAlliance,
    ) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("propose alliance {country}"));
        found(country)?;
        Ok(game::diplomacy::Ticket(3))
    }

    /// Toward Byzantium, an alliance the Byzantines would not accept, a
    /// warning the player can send, and a scripted action the player cannot.
    fn options(
        &mut self,
        eu4_harmony_remote::diplomacy::args::Options { country }: eu4_harmony_remote::diplomacy::args::Options,
    ) -> GResult<eu4_harmony_remote::diplomacy::Options> {
        use eu4_harmony_remote::{
            choices::{Acceptance, Choice, Factor},
            diplomacy::{Offered, Options},
        };
        self.call(format!("options {country}"));
        found(country)?;
        let acceptance = Acceptance {
            accepts: false,
            total: -12.0,
            factors: vec![Factor { text: "Base reluctance".into(), value: -10.0 }, Factor { text: "Relations".into(), value: -2.0 }],
        };
        let actions = vec![
            Offered {
                action: "propose_alliance".into(),
                key: "allianceaction".into(),
                name: "Offer Alliance".into(),
                choice: Choice::can(Vec::new()),
                description: vec!["If a country accepts our Alliance offer it may come to our assistance during wars.".into()],
                acceptance: Some(acceptance),
            },
            Offered {
                action: "warn".into(),
                key: "warningaction".into(),
                name: "Warn".into(),
                choice: Choice::can(Vec::new()),
                description: Vec::new(),
                acceptance: None,
            },
            Offered {
                action: "scripted_action".into(),
                key: "sell_indulgence".into(),
                name: "Offer Indulgence".into(),
                choice: Choice::cannot(vec!["Is not the Papal Controller.".into()], Vec::new()),
                description: Vec::new(),
                acceptance: None,
            },
        ];
        Ok(Options { country, actions })
    }

    /// Warning Byzantium is sent; anything else is refused with the game's
    /// reason.
    fn act(&mut self, act: game::diplomacy::Act, country: CountryTag, _: game::diplomacy::Details<CountryTag>) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("act {country} Act({act:?})"));
        match act {
            game::diplomacy::Act::Warn if found(country)? == byz() => Ok(game::diplomacy::Ticket(4)),
            _ => fail(Status::Rejected, "Is not the Papal Controller."),
        }
    }

    fn scripted_action(
        &mut self,
        eu4_harmony_remote::diplomacy::args::ScriptedAction { country, action }: eu4_harmony_remote::diplomacy::args::ScriptedAction,
    ) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("act {country} Scripted({action:?})"));
        fail(Status::Rejected, "Is not the Papal Controller.")
    }

    /// Byzantium can be rivalled; no other country.
    fn add_rival(&mut self, eu4_harmony_remote::diplomacy::args::AddRival { country }: eu4_harmony_remote::diplomacy::args::AddRival) -> GResult<()> {
        self.call(format!("add rival {country}"));
        if found(country)? == byz() {
            Ok(())
        } else {
            fail(Status::Rejected, "the game's command refuses it, and gives no reason")
        }
    }

    fn remove_rival(&mut self, eu4_harmony_remote::diplomacy::args::RemoveRival { country }: eu4_harmony_remote::diplomacy::args::RemoveRival) -> GResult<()> {
        self.call(format!("remove rival {country}"));
        found(country)?;
        fail(Status::Rejected, "the game's command refuses it, and gives no reason")
    }

    fn call_to_arms(
        &mut self,
        eu4_harmony_remote::diplomacy::args::CallToArms { country }: eu4_harmony_remote::diplomacy::args::CallToArms,
    ) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("call to arms {country}"));
        found(country)?;
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
    fn peace_terms(
        &mut self,
        eu4_harmony_remote::peace::args::TermOptions { country, offer }: eu4_harmony_remote::peace::args::TermOptions,
    ) -> GResult<Vec<eu4_harmony_remote::peace::TermOption>> {
        use eu4_harmony_remote::{
            choices::{Amount, Choice, Resource},
            peace::TermOption,
            ProvinceId,
        };
        self.call(format!("peace terms {country} {}", offer.unwrap_or(false)));
        if found(country)? != byz() {
            return fail(Status::Rejected, "the player is not at war with WAL");
        }
        let term = |term: &str, province: Option<u32>, name: &str, choice: Choice| TermOption {
            term: term.into(),
            province: province.map(ProvinceId),
            country: province.map(|_| CountryTag(*b"TUR")),
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

    fn evaluate_peace(
        &mut self,
        eu4_harmony_remote::peace::args::Evaluate { country, terms, offer }: eu4_harmony_remote::peace::args::Evaluate,
    ) -> GResult<eu4_harmony_remote::peace::Evaluation> {
        use eu4_harmony_remote::choices::{Acceptance, Amount, Choice, Factor, Resource};
        self.call(format!("evaluate peace {country} {} {}", peace_terms(&terms), offer.unwrap_or(false)));
        found(country)?;
        terms_found(&terms)?;
        Ok(eu4_harmony_remote::peace::Evaluation {
            choice: Choice::can(vec![Amount::new(Resource::WarScore, 40.0)]),
            acceptance: Acceptance { accepts: false, total: -5.0, factors: vec![Factor { text: "War score".into(), value: -5.0 }] },
        })
    }

    fn demand_peace(
        &mut self,
        eu4_harmony_remote::peace::args::Demand { country, terms }: eu4_harmony_remote::peace::args::Demand,
    ) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("demand peace {country} {}", peace_terms(&terms)));
        found(country)?;
        terms_found(&terms)?;
        Ok(game::diplomacy::Ticket(5))
    }

    fn offer_terms(
        &mut self,
        eu4_harmony_remote::peace::args::Offer { country, terms }: eu4_harmony_remote::peace::args::Offer,
    ) -> GResult<game::diplomacy::Ticket> {
        self.call(format!("offer terms {country} {}", peace_terms(&terms)));
        found(country)?;
        terms_found(&terms)?;
        Ok(game::diplomacy::Ticket(6))
    }
}

/// Peace terms, as a call records them.
fn peace_terms(terms: &eu4_harmony_remote::peace::Terms) -> String {
    let term = |t: &eu4_harmony_remote::peace::Term| format!("{}:{:?}:{:?}:{:?}", t.term, t.province.map(|p| p.0), t.country.map(|c| c.to_string()), t.amount);
    terms.0.iter().map(term).collect::<Vec<_>>().join(",")
}

/// Refused as the layer refuses a term naming a country there is not.
fn terms_found(terms: &eu4_harmony_remote::peace::Terms) -> GResult<()> {
    terms.0.iter().filter_map(|t| t.country).try_for_each(|c| found(c).map(|_| ()))
}
