//! The engine driven with typed requests, a fake game behind the capability
//! layer's traits.
use super::*;
use crate::catalogue::{
    dispatch,
    fake::{self, answer, ok, Fake, ARMY, ENEMY, FLEET, PLENTY},
    process, Outcome,
};
use eu4_harmony_remote::{
    armies::ArmyRequest,
    buildings::BuildingRequest,
    camera::CameraRequest,
    capabilities,
    centers_of_trade::CenterOfTradeRequest,
    choices::PowerKind,
    colonists::ColonistRequest,
    console::ConsoleRequest,
    countries::CountryRequest,
    decisions::DecisionRequest,
    describe::DescribeRequest,
    desktop::DesktopRequest,
    diplomacy::DiplomacyRequest,
    economy::EconomyRequest,
    estates::EstateRequest,
    events::Event,
    events::EventsRequest,
    extensions::ExtensionRequest,
    focus::FocusRequest,
    government::GovernmentRequest,
    ideas::IdeaRequest,
    input::Button,
    input::Direction,
    input::InputRequest,
    input::Keys,
    loans::LoanRequest,
    menu::MenuRequest,
    merchants::MerchantRequest,
    missionaries::MissionaryRequest,
    missions::MissionRequest,
    occupation::OccupationRequest,
    offers::OfferRequest,
    parliament::ParliamentRequest,
    peace::{PeaceRequest, Terms},
    policies::PolicyRequest,
    provinces::ProvinceRequest,
    questions::QuestionRequest,
    recruiting::RecruitRequest,
    recruiting::UnitKind,
    response_json,
    screenshot::ScreenshotRequest,
    session::SessionRequest,
    stability::StabilityRequest,
    subjects::SubjectRequest,
    technology::TechnologyRequest,
    texts::TextRequest,
    time::TimeRequest,
    trade::TradeRequest,
    trade_companies::TradeCompanyRequest,
    ArmyId, ArmyList, CountryTag, Kind, ProvinceId,
};
use eu4_harmony_remote::{forts::FortRequest, leaders::LeaderRequest, mercenaries::MercenaryRequest, sieges::SiegeRequest};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn remember(line: &str) {
    LOG.with(|l| l.borrow_mut().push(line.to_string()));
}

fn log() -> Vec<String> {
    LOG.with(|l| l.borrow().clone())
}

fn error(code: &str) -> impl Fn(&Value) -> bool + '_ {
    move |v: &Value| v["ok"] == false && v["error"]["code"] == code
}

fn keys(text: &str) -> Keys {
    Keys::parse(text).unwrap()
}

fn army(id: u64) -> ArmyId {
    ArmyId(id)
}

fn province(id: u32) -> ProvinceId {
    ProvinceId(id)
}

#[test]
fn every_capability_runs_at_a_game_frame() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let snapshot = json!({
        "id": ARMY.to_string(), "owner": "TUR", "kind": "army", "province": 149, "regiments": 12, "blocked": [], "route": [], "route_by": null, "aboard": null, "carrying": 0,
    });
    let fleet = json!({
        "id": FLEET.to_string(), "owner": "TUR", "kind": "navy", "province": null, "regiments": 0, "blocked": [], "route": [], "route_by": null, "aboard": null, "carrying": 0,
    });
    let province_json = |id: u32, owner: &str, controller: &str| json!({"id": id, "kind": "land", "visible": true, "owner": owner, "controller": controller});
    type Check = Box<dyn Fn(&Value) -> bool>;
    let mut cases: Vec<(Request, Check)> = vec![
        (Request::Session(SessionRequest::Get {}), Box::new(|v: &Value| *v == ok(json!({"number": 2, "player": "TUR", "date": "1444.11.11"})))),
        (Request::Time(TimeRequest::Get {}), Box::new(|v: &Value| *v == ok(json!({"date": "1444.11.11", "paused": true, "speed": 2})))),
        (Request::Time(TimeRequest::SetSpeed { speed: 3 }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Time(TimeRequest::Resume {}), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Time(TimeRequest::Pause {}), Box::new(|v: &Value| *v == ok(Value::Null))),
        // An advance answers at once when it cannot run; time::tests has the rest.
        (Request::Time(TimeRequest::Advance { days: 0, speed: None }), Box::new(error("invalid_argument"))),
        (Request::Country(CountryRequest::Hostile { country: CountryTag(*b"TUR"), other: CountryTag(*b"BYZ") }), Box::new(|v: &Value| *v == ok(json!(true)))),
        (Request::Country(CountryRequest::Hostile { country: CountryTag(*b"BYZ"), other: CountryTag(*b"TUR") }), Box::new(|v: &Value| *v == ok(json!(false)))),
        (Request::Country(CountryRequest::Hostile { country: CountryTag(*b"TUR"), other: CountryTag(*b"ZZZ") }), Box::new(error("not_found"))),
        (
            Request::Country(CountryRequest::Get { country: CountryTag(*b"TUR") }),
            Box::new(|v: &Value| {
                *v == ok(json!({
                    "tag": "TUR", "treasury": 125.5, "manpower": 22600, "max_manpower": 30250, "stability": 1.0, "prestige": -12.345,
                    "power": {"adm": 50, "dip": 60, "mil": 70},
                    "wars": [{"name": "Ottoman Conquest of Constantinople", "attackers": ["TUR", "WAL"], "defenders": ["BYZ"]}],
                    "relations": [{"kind": "subject", "country": "WAL", "subject_type": "vassal"}, {"kind": "rival", "country": "BYZ"}],
                    "technology": {"adm": 3, "dip": 3, "mil": 4},
                    "ideas": [{"group": "TUR_ideas", "ideas": 1, "national": true}, {"group": "quality_ideas", "ideas": 2, "national": false}],
                    "governing_capacity": {"used": 512.5, "max": 700.0}, "inflation": 2.5, "mercantilism": 10.0,
                    "loans": {"count": 1, "ducats": 120.0},
                    "rebels": [{"kind": "nationalist_rebels", "name": "Albanian Nationalists", "progress": 12.5}],
                }))
            }),
        ),
        (Request::Country(CountryRequest::Get { country: CountryTag(*b"ZZZ") }), Box::new(error("not_found"))),
        (
            Request::Country(CountryRequest::Budget { country: CountryTag(*b"TUR") }),
            Box::new(|v: &Value| {
                *v == ok(json!({
                    "country": "TUR",
                    "income": [{"category": "INCOMETAX", "name": "税收", "ducats": 4.25}],
                    "expenses": [{"category": "EXPENSEAM", "name": "陆军维护费", "ducats": 3.5}],
                    "loans": [{"lender": null, "ducats": 120.0, "interest": 4.0, "due": "1449.11.11"}],
                }))
            }),
        ),
        (
            Request::Country(CountryRequest::Budget { country: CountryTag(*b"BYZ") }),
            Box::new(|v: &Value| *v == ok(json!({"country": "BYZ", "income": null, "expenses": null, "loans": null}))),
        ),
        (Request::Trade(TradeRequest::Nodes {}), Box::new(|v: &Value| v["result"][1]["node"] == "venice")),
        (
            Request::Trade(TradeRequest::Node { node: "constantinople".into() }),
            Box::new(|v: &Value| {
                v["result"]["outgoing"] == json!([{"node": "venice", "ducats": 3.0}])
                    && v["result"]["countries"][0]
                        == json!({"country": "TUR", "power": 60.0, "share": 60.0, "collects": false, "home": false, "merchant": {"action": "steer", "toward": "venice"}})
            }),
        ),
        (Request::Trade(TradeRequest::Node { node: "nowhere".into() }), Box::new(error("not_found"))),
        (
            Request::Province(ProvinceRequest::List { provinces: None }),
            Box::new(move |v: &Value| *v == ok(json!([province_json(149, "TUR", "TUR"), province_json(151, "BYZ", "TUR")]))),
        ),
        (
            Request::Province(ProvinceRequest::Neighbors { province: province(149) }),
            Box::new(|v: &Value| *v == ok(json!([{"province": 151, "strait": false}, {"province": 316, "strait": true}]))),
        ),
        (
            Request::Province(ProvinceRequest::List { provinces: Some(eu4_harmony_remote::ProvinceList(vec![province(151), province(1), province(149)])) }),
            Box::new(move |v: &Value| *v == ok(json!([province_json(151, "BYZ", "TUR"), province_json(149, "TUR", "TUR")]))),
        ),
        (Request::Province(ProvinceRequest::Get { province: province(151) }), Box::new(|v: &Value| v["result"]["name"] == "君士坦丁堡")),
        (Request::Province(ProvinceRequest::Get { province: province(1) }), Box::new(error("not_found"))),
        (
            Request::Army(ArmyRequest::Get { army: army(ARMY) }),
            Box::new({
                let snapshot = snapshot.clone();
                move |v: &Value| *v == ok(snapshot.clone())
            }),
        ),
        (
            Request::Army(ArmyRequest::Get { army: army(FLEET) }),
            Box::new({
                let fleet = fleet.clone();
                move |v: &Value| *v == ok(fleet.clone())
            }),
        ),
        (Request::Army(ArmyRequest::Get { army: army(1) }), Box::new(error("not_found"))),
        (Request::Army(ArmyRequest::List {}), Box::new(move |v: &Value| *v == ok(json!([snapshot, fleet])))),
        (
            Request::Army(ArmyRequest::At { province: province(151) }),
            Box::new(|v: &Value| {
                let a = &v["result"][0];
                a["id"] == json!(ENEMY.to_string())
                    && a["owner"] == "BYZ"
                    && a["province"] == 151
                    && a["blocked"] == json!(["sieging"])
                    && a["route_by"] == "other_mod"
            }),
        ),
        (Request::Army(ArmyRequest::CanStep { army: army(ARMY), from: province(149), to: province(151) }), Box::new(|v: &Value| *v == ok(json!(true)))),
        (Request::Army(ArmyRequest::CanStep { army: army(ARMY), from: province(149), to: province(316) }), Box::new(|v: &Value| *v == ok(json!(false)))),
        (Request::Army(ArmyRequest::CanAttack { army: army(ARMY), target: army(ENEMY) }), Box::new(|v: &Value| *v == ok(json!(true)))),
        (Request::Army(ArmyRequest::RouteTo { army: army(ARMY), province: province(151) }), Box::new(|v: &Value| *v == ok(json!([150, 151])))),
        (Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(151), transports: None }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::Get { army: army(ARMY) }), Box::new(|v: &Value| v["result"]["route"] == json!([150, 151]))),
        (
            Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(1), transports: None }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "no route to 1"),
        ),
        (Request::Army(ArmyRequest::Stop { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::Get { army: army(ARMY) }), Box::new(|v: &Value| v["result"]["route"] == json!([]))),
        (Request::Army(ArmyRequest::Selected {}), Box::new(error("not_found"))),
        (Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(236), transports: Some(true) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Army(ArmyRequest::Options { army: army(ARMY) }),
            Box::new(|v: &Value| v["result"][1] == json!({"action": "merge", "can": false, "reasons": ["至少需要两支部队"], "cost": []})),
        ),
        (Request::Army(ArmyRequest::Split { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(json!((ARMY + 1).to_string())))),
        (Request::Army(ArmyRequest::Merge { armies: ArmyList(vec![army(ARMY)]) }), Box::new(error("rejected"))),
        (Request::Army(ArmyRequest::Disband { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::Consolidate { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::Drill { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::StopDrill { army: army(ARMY) }), Box::new(error("rejected"))),
        (Request::Army(ArmyRequest::Mission { army: army(FLEET), mission: "blockade".into(), target: Some(47) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Army(ArmyRequest::StopMission { army: army(FLEET) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Leader(LeaderRequest::Options {}),
            Box::new(|v: &Value| v["result"][0] == json!({"kind": "general", "can": true, "reasons": [], "cost": [{"resource": "mil", "amount": 50.0}]})),
        ),
        (Request::Leader(LeaderRequest::Recruit { kind: "general".into() }), Box::new(|v: &Value| v["result"]["name"] == "锡南")),
        (Request::Leader(LeaderRequest::List {}), Box::new(|v: &Value| v["result"][0]["id"] == 7 && v["result"][0]["army"].is_null())),
        (Request::Leader(LeaderRequest::Assign { leader: 7, army: army(ARMY) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Leader(LeaderRequest::Remove { army: army(ARMY) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Leader(LeaderRequest::Dismiss { leader: 8 }), Box::new(error("not_found"))),
        (Request::Mercenary(MercenaryRequest::List {}), Box::new(|v: &Value| v["result"][0]["company"] == "merc_black_army" && v["result"][0]["can"] == true)),
        (
            Request::Mercenary(MercenaryRequest::Hire { company: "merc_black_army".into(), province: province(149) }),
            Box::new(|v: &Value| *v == ok(Value::Null)),
        ),
        (
            Request::Fort(FortRequest::List {}),
            Box::new(|v: &Value| v["result"][0] == json!({"province": 149, "level": 2, "mothballed": false, "can": true, "reasons": [], "cost": []})),
        ),
        (Request::Fort(FortRequest::Mothball { province: province(149) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Fort(FortRequest::Reopen { province: province(149) }), Box::new(error("rejected"))),
        (
            Request::Siege(SiegeRequest::Options { province: province(151) }),
            Box::new(|v: &Value| v["result"][1]["reasons"] == json!(["没有缺口，无法强攻要塞。"])),
        ),
        (Request::Siege(SiegeRequest::Bombard { province: province(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Siege(SiegeRequest::NavalBombard { province: province(151) }), Box::new(error("rejected"))),
        (Request::Siege(SiegeRequest::Assault { province: province(151) }), Box::new(error("rejected"))),
        (Request::Text(TextRequest::Language {}), Box::new(|v: &Value| *v == ok(json!("simp_chinese")))),
        (Request::Console(ConsoleRequest::Run { command: "cash 100".into() }), Box::new(|v: &Value| *v == ok(json!("Gold changed by 100")))),
        (
            Request::Console(ConsoleRequest::Run { command: "cahs 100".into() }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "Unknown command"),
        ),
        (
            Request::Screenshot(ScreenshotRequest::Take { scale: Some(50) }),
            Box::new(|v: &Value| {
                *v == ok(json!({"image": {"media_type": "image/png", "base64": "iVBORw=="}, "width": 2, "height": 1, "frame_width": 2, "frame_height": 1}))
            }),
        ),
        (
            Request::Question(QuestionRequest::List {}),
            Box::new(|v: &Value| {
                *v == ok(json!([{
                    "id": 7, "event": "flavor_tur.1", "title": "The Janissaries", "description": "Our elite troops ask for more.",
                    "deadline": "1445.3.11", "options": [{"option": 0, "text": "Pay them"}, {"option": 2, "text": "Refuse"}],
                }]))
            }),
        ),
        (Request::Question(QuestionRequest::Answer { question: 7, option: 2 }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Question(QuestionRequest::Answer { question: 7, option: 1 }), Box::new(error("rejected"))),
        (Request::Question(QuestionRequest::Answer { question: 8, option: 0 }), Box::new(error("not_found"))),
        (
            Request::Offer(OfferRequest::List {}),
            Box::new(|v: &Value| *v == ok(json!([{"id": 3, "from": "BYZ", "date": "1444.11.11", "title": "Royal Marriage", "description": "皇室联姻"}]))),
        ),
        (Request::Offer(OfferRequest::Accept { offer: 3 }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Offer(OfferRequest::Decline { offer: 3 }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Offer(OfferRequest::Decline { offer: 4 }), Box::new(error("not_found"))),
        (
            Request::Events(EventsRequest::Read { since: None, wait_ms: None, process: None, kinds: None, countries: None }),
            Box::new(|v: &Value| *v == ok(json!({"process": process(), "events": [], "last": 0, "missed": false}))),
        ),
        (
            Request::Camera(CameraRequest::Get {}),
            Box::new(|v: &Value| *v == ok(json!({"x": 3248, "y": 1376, "height": 800, "province": 149, "min_height": 50, "max_height": 3000}))),
        ),
        (
            Request::Camera(CameraRequest::LookAt { province: Some(province(151)), x: None, y: None }),
            Box::new(|v: &Value| v["result"]["province"] == 151 && v["result"]["x"] == 3270),
        ),
        (Request::Camera(CameraRequest::LookAt { province: Some(province(1)), x: None, y: None }), Box::new(error("not_found"))),
        (Request::Camera(CameraRequest::LookAt { province: Some(province(151)), x: Some(1), y: None }), Box::new(error("invalid_argument"))),
        (
            Request::Camera(CameraRequest::LookAt { province: None, x: Some(100), y: Some(200) }),
            Box::new(|v: &Value| v["result"]["x"] == 100 && v["result"]["y"] == 200 && v["result"]["province"].is_null()),
        ),
        (Request::Camera(CameraRequest::LookAt { province: None, x: Some(6000), y: Some(200) }), Box::new(error("invalid_argument"))),
        (Request::Camera(CameraRequest::Zoom { height: 1200 }), Box::new(|v: &Value| v["result"]["height"] == 1200 && v["result"]["province"] == 149)),
        (Request::Camera(CameraRequest::Zoom { height: 10 }), Box::new(error("invalid_argument"))),
        (
            Request::Recruit(RecruitRequest::Options { province: province(151) }),
            Box::new(|v: &Value| v["result"].as_array().map(Vec::len) == Some(7) && v["result"][0]["kind"] == "infantry" && v["result"][0]["allowed"] == false),
        ),
        (Request::Recruit(RecruitRequest::Order { province: province(149), kind: UnitKind::Infantry }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Recruit(RecruitRequest::Order { province: province(151), kind: UnitKind::Cavalry }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "君士坦丁堡不是我们的领土"),
        ),
        (Request::Recruit(RecruitRequest::Order { province: province(1), kind: UnitKind::Galley }), Box::new(error("not_found"))),
        (Request::Diplomacy(DiplomacyRequest::WarGoals { country: CountryTag(*b"BYZ") }), Box::new(|v: &Value| v["result"][1]["casus_belli"] == "none")),
        (
            Request::Diplomacy(DiplomacyRequest::DeclareWar {
                country: CountryTag(*b"WAL"),
                casus_belli: "none".into(),
                war_goal: None,
                province: None,
                target: None,
                call_allies: None,
            }),
            Box::new(error("rejected")),
        ),
        (Request::Diplomacy(DiplomacyRequest::OfferPeace { country: CountryTag(*b"QQQ") }), Box::new(error("not_found"))),
        (Request::Diplomacy(DiplomacyRequest::ProposeAlliance { country: CountryTag(*b"QQQ") }), Box::new(error("not_found"))),
        (Request::Diplomacy(DiplomacyRequest::CallToArms { country: CountryTag(*b"QQQ") }), Box::new(error("not_found"))),
        (Request::Peace(PeaceRequest::Terms { country: CountryTag(*b"WAL"), offer: None }), Box::new(error("rejected"))),
        (Request::Peace(PeaceRequest::Evaluate { country: CountryTag(*b"QQQ"), terms: Terms(Vec::new()), offer: None }), Box::new(error("not_found"))),
        (Request::Peace(PeaceRequest::Demand { country: CountryTag(*b"QQQ"), terms: Terms(Vec::new()) }), Box::new(error("not_found"))),
        (Request::Peace(PeaceRequest::Offer { country: CountryTag(*b"QQQ"), terms: Terms(Vec::new()) }), Box::new(error("not_found"))),
        (
            Request::Occupation(OccupationRequest::Options { province: ProvinceId(151) }),
            Box::new(|v: &Value| *v == ok(json!({"can": true, "reasons": [], "cost": [], "allies": ["WAL"]}))),
        ),
        (
            Request::Occupation(OccupationRequest::Transfer { province: ProvinceId(151), country: CountryTag(*b"WAL") }),
            Box::new(|v: &Value| *v == ok(json!({"controller": "WAL"}))),
        ),
        (
            Request::Describe(DescribeRequest::All {}),
            Box::new(|v: &Value| {
                v["result"]["api_version"] == eu4_harmony_remote::API_VERSION
                    && v["result"]["capabilities"].as_array().map(Vec::len) == Some(capabilities().len())
                    && v["result"]["capabilities"][0]["capability"] == "session.get"
            }),
        ),
        (Request::Menu(MenuRequest::State {}), Box::new(|v: &Value| *v == ok(json!({"process": process(), "place": "game"})))),
        (
            Request::Menu(MenuRequest::Saves {}),
            Box::new(|v: &Value| {
                *v == ok(json!([{
                    "name": "奥斯曼1444_11_11", "file": "奥斯曼1444_11_11.eu4", "bytes": 3_266_935, "modified": 1_761_700_000, "player": "TUR", "date": "1444.11.11", "missing_mods": ["彼岸乐园"],
                }]))
            }),
        ),
        // A load and a start answer at the game's first frame after the one
        // they ran at (a later test).
        (Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }), Box::new(Value::is_null)),
        (Request::Menu(MenuRequest::Start { country: CountryTag(*b"FRA") }), Box::new(Value::is_null)),
        (Request::Menu(MenuRequest::Return {}), Box::new(|v: &Value| *v == ok(json!({"process": process(), "place": "ending"})))),
        (Request::Extension(ExtensionRequest::Reload { id: "regional-combat".into() }), Box::new(|v: &Value| *v == ok(json!({"session": true, "lost": null})))),
        (Request::Desktop(DesktopRequest::Get {}), Box::new(|v: &Value| *v == ok(json!({"hidden": true, "hidden_start": true, "sound": false})))),
        (Request::Desktop(DesktopRequest::Show {}), Box::new(|v: &Value| v["result"]["hidden"] == false)),
        (Request::Desktop(DesktopRequest::Hide {}), Box::new(|v: &Value| v["result"]["hidden"] == true)),
        (
            Request::Decision(DecisionRequest::List {}),
            Box::new(|v: &Value| {
                v["result"][1] == json!({"decision": "move_capital", "name": "迁都", "major": false, "can": false, "reasons": ["拥有君士坦丁堡"], "cost": []})
            }),
        ),
        (Request::Decision(DecisionRequest::Take { decision: "form_rum".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Decision(DecisionRequest::Take { decision: "move_capital".into() }), Box::new(|v: &Value| v["error"]["message"] == "拥有君士坦丁堡")),
        (
            Request::Idea(IdeaRequest::List {}),
            Box::new(|v: &Value| {
                v["result"][0]
                    == json!({"group": "defensive_ideas", "name": "防御理念", "category": "mil", "taken": true, "ideas": 2, "of": 7, "button": "buy",
                        "next": "improved_manuever", "can": true, "reasons": [], "cost": [{"resource": "mil", "amount": 400.0}]})
            }),
        ),
        (Request::Idea(IdeaRequest::TakeGroup { group: "unknown".into() }), Box::new(|v: &Value| v["error"]["code"] == "not_found")),
        (Request::Idea(IdeaRequest::Buy { group: "defensive_ideas".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Idea(IdeaRequest::AbandonGroup { group: "no".into() }), Box::new(|v: &Value| v["error"]["code"] == "rejected")),
        (Request::Policy(PolicyRequest::List {}), Box::new(|v: &Value| v["result"][0]["policy"] == "the_combination_act" && v["result"][0]["can"] == true)),
        (Request::Policy(PolicyRequest::Enable { policy: "the_combination_act".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Policy(PolicyRequest::Disable { policy: "no".into() }), Box::new(|v: &Value| v["error"]["message"] == "需要和平")),
        (
            Request::Government(GovernmentRequest::Options {}),
            Box::new(|v: &Value| {
                v["result"]["rank"] == 2 && v["result"]["raise_rank"] == json!({"can": false, "reasons": ["需要至少 1000 发展度"], "cost": []})
            }),
        ),
        (
            Request::Government(GovernmentRequest::Reforms {}),
            Box::new(|v: &Value| v["result"][0]["reforms"][1]["cost"] == json!([{"resource": "reform_progress", "amount": 50.0}])),
        ),
        (Request::Government(GovernmentRequest::AdoptReform { reform: "autocracy_reform".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Government(GovernmentRequest::RaiseRank {}), Box::new(|v: &Value| v["error"]["code"] == "rejected")),
        (
            Request::Estate(EstateRequest::List {}),
            Box::new(|v: &Value| v["result"][0]["privileges"][0]["granted"] == true && v["result"][0]["loyalty"] == 56.864),
        ),
        (Request::Estate(EstateRequest::Interactions {}), Box::new(|v: &Value| v["result"][0]["reasons"] == json!(["冷却中"]))),
        (
            Request::Estate(EstateRequest::GrantPrivilege { estate: "estate_nobles".into(), privilege: "unknown".into() }),
            Box::new(|v: &Value| v["error"]["code"] == "not_found"),
        ),
        (
            Request::Estate(EstateRequest::RevokePrivilege { estate: "estate_nobles".into(), privilege: "estate_nobles_land_rights".into() }),
            Box::new(|v: &Value| *v == ok(Value::Null)),
        ),
        (Request::Estate(EstateRequest::Interact { interaction: "seize_land".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Parliament(ParliamentRequest::Options {}),
            Box::new(|v: &Value| *v == ok(json!({"parliament": false, "debating": null, "enacted": null, "issues": []}))),
        ),
        (Request::Parliament(ParliamentRequest::Debate { issue: "administrative_reform".into() }), Box::new(|v: &Value| v["error"]["code"] == "rejected")),
        (Request::Parliament(ParliamentRequest::AddSeat { province: ProvinceId(149) }), Box::new(|v: &Value| v["error"]["code"] == "rejected")),
        (Request::Parliament(ParliamentRequest::RemoveSeat { province: ProvinceId(149) }), Box::new(|v: &Value| v["error"]["code"] == "rejected")),
        (Request::Mission(MissionRequest::List {}), Box::new(|v: &Value| v["result"][0]["column"] == 0 && v["result"][0]["completed"] == false)),
        (Request::Mission(MissionRequest::Complete { mission: "tur_conquer_constantinople".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Technology(TechnologyRequest::Options {}),
            Box::new(|v: &Value| {
                v["result"][2]
                    == json!({"technology": "mil", "level": 3, "can": false, "reasons": ["我们预计在1446年5月1日拥有足够的点数。"], "cost": [{"resource": "mil", "amount": 600.0}]})
            }),
        ),
        (Request::Technology(TechnologyRequest::Research { technology: PowerKind::Adm }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Technology(TechnologyRequest::Research { technology: PowerKind::Mil }),
            Box::new(|v: &Value| v["error"]["code"] == "rejected" && v["error"]["message"] == "我们预计在1446年5月1日拥有足够的点数。"),
        ),
        (
            Request::Focus(FocusRequest::Options {}),
            Box::new(|v: &Value| {
                v["result"]["focus"] == "mil"
                    && v["result"]["choose"][0] == json!({"focus": "adm", "can": false, "reasons": ["你每5年才能更改一次国家重心。"], "cost": []})
            }),
        ),
        (Request::Focus(FocusRequest::Choose { focus: PowerKind::Mil }), Box::new(|v: &Value| v["error"]["message"] == "the national focus is on mil already")),
        (Request::Focus(FocusRequest::Cancel {}), Box::new(|v: &Value| v["error"]["code"] == "rejected")),
        (
            Request::Stability(StabilityRequest::Options {}),
            Box::new(|v: &Value| *v == ok(json!([{"action": "raise", "can": true, "reasons": [], "cost": [{"resource": "adm", "amount": 100.0}]}]))),
        ),
        (Request::Stability(StabilityRequest::Raise {}), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Economy(EconomyRequest::Options {}),
            Box::new(|v: &Value| {
                v["result"][0] == json!({"action": "take_loan", "can": true, "reasons": [], "cost": [], "gives": [{"resource": "ducats", "amount": 85.0}]})
            }),
        ),
        (Request::Economy(EconomyRequest::TakeLoan {}), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Economy(EconomyRequest::ReduceInflation {}), Box::new(|v: &Value| v["error"]["message"] == "你目前的通货膨胀不够高。")),
        (Request::Economy(EconomyRequest::RaiseMercantilism {}), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Loan(LoanRequest::List {}),
            Box::new(|v: &Value| {
                v["result"][0]
                    == json!({"loan": 7, "lender": null, "estate": false, "amount": 85.0, "interest": 4.0, "expires": "1449.11.11", "can": true, "reasons": [], "cost": [{"resource": "ducats", "amount": 101.5}]})
            }),
        ),
        (Request::Loan(LoanRequest::Repay { loan: 7 }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Loan(LoanRequest::Repay { loan: 8 }), Box::new(|v: &Value| v["error"]["code"] == "not_found")),
        (
            Request::Province(ProvinceRequest::Options { province: ProvinceId(151) }),
            Box::new(|v: &Value| {
                v["result"][0] == json!({"action": "develop_tax", "edict": null, "can": true, "reasons": [], "cost": [{"resource": "adm", "amount": 50.0}]})
                    && v["result"][1]["edict"] == "edict_advancement_effort"
                    && v["result"][2]["reasons"] == json!(["已经是核心省份"])
            }),
        ),
        (Request::Province(ProvinceRequest::DevelopTax { province: ProvinceId(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Province(ProvinceRequest::DevelopProduction { province: ProvinceId(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Province(ProvinceRequest::DevelopManpower { province: ProvinceId(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Province(ProvinceRequest::ExploitTax { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::ExploitProduction { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::ExploitManpower { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::RaiseAutonomy { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::LowerAutonomy { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::MakeState { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::MakeTerritory { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::SetEdict { province: ProvinceId(151), edict: "edict_x".into() }), Box::new(error("invalid_argument"))),
        (Request::Province(ProvinceRequest::CancelEdict { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (
            Request::Province(ProvinceRequest::Core { province: ProvinceId(151) }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "已经是核心省份"),
        ),
        (Request::Province(ProvinceRequest::CancelCore { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::ChangeCulture { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (Request::Province(ProvinceRequest::CancelCulture { province: ProvinceId(151) }), Box::new(error("rejected"))),
        (
            Request::Building(BuildingRequest::Options { province: ProvinceId(151) }),
            Box::new(|v: &Value| {
                v["result"]["constructing"] == json!({"building": "marketplace", "progress": 0.25, "finished": "1444.11.11", "builder": "BYZ"})
                    && v["result"]["options"][1] == json!({"action": "destroy", "building": "temple", "can": true, "reasons": [], "cost": []})
                    && v["result"]["options"][2]["cost"] == json!([{"resource": "ducats", "amount": 50.0}])
            }),
        ),
        (
            Request::Building(BuildingRequest::Build { province: ProvinceId(151), building: "workshop".into() }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "建造中"),
        ),
        (Request::Building(BuildingRequest::Cancel { province: ProvinceId(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Building(BuildingRequest::Destroy { province: ProvinceId(151), building: "temple".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Missionary(MissionaryRequest::Options { province: province(151) }),
            Box::new(|v: &Value| v["result"]["send"] == json!({"can": true, "reasons": [], "cost": []}) && v["result"]["recall"]["can"] == false),
        ),
        (Request::Missionary(MissionaryRequest::Send { province: province(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Missionary(MissionaryRequest::Send { province: province(149) }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "该省份已经信仰我们的宗教"),
        ),
        (Request::Missionary(MissionaryRequest::Recall { province: province(1) }), Box::new(error("not_found"))),
        (Request::Missionary(MissionaryRequest::Automatic {}), Box::new(|v: &Value| v["result"]["missionaries"] == 2 && v["result"]["automatic"] == 1)),
        (Request::Missionary(MissionaryRequest::RaiseAutomatic {}), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Missionary(MissionaryRequest::LowerAutomatic {}), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Colonist(ColonistRequest::Send { province: province(149) }), Box::new(error("rejected"))),
        (Request::TradeCompany(TradeCompanyRequest::Add { province: province(149) }), Box::new(error("rejected"))),
        (Request::TradeCompany(TradeCompanyRequest::Remove { province: province(149) }), Box::new(error("rejected"))),
        (Request::Merchant(MerchantRequest::Collect { node: "constantinople".into() }), Box::new(error("rejected"))),
        (Request::Merchant(MerchantRequest::Steer { node: "constantinople".into() }), Box::new(error("rejected"))),
        (
            Request::Colonist(ColonistRequest::Options { province: province(151) }),
            Box::new(|v: &Value| v["result"]["send"]["reasons"] == json!(["没有可用的殖民者"])),
        ),
        (Request::Colonist(ColonistRequest::Recall { province: province(151) }), Box::new(error("rejected"))),
        (Request::TradeCompany(TradeCompanyRequest::Options { province: province(149) }), Box::new(|v: &Value| v["result"]["investments"] == json!([]))),
        (Request::TradeCompany(TradeCompanyRequest::Invest { province: province(149), investment: "nothing".into() }), Box::new(error("not_found"))),
        (
            Request::CenterOfTrade(CenterOfTradeRequest::Options { province: province(151) }),
            Box::new(|v: &Value| v["result"]["level"] == 1 && v["result"]["upgrade"]["cost"] == json!([{"resource": "ducats", "amount": 200.0}])),
        ),
        (Request::CenterOfTrade(CenterOfTradeRequest::Upgrade { province: province(151) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::CenterOfTrade(CenterOfTradeRequest::Downgrade { province: province(151) }), Box::new(error("rejected"))),
        (Request::Merchant(MerchantRequest::List {}), Box::new(|v: &Value| v["result"]["placed"][0]["mission"] == "collect")),
        (Request::Merchant(MerchantRequest::Options { node: "nowhere".into() }), Box::new(error("not_found"))),
        (Request::Merchant(MerchantRequest::Recall { node: "constantinople".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Merchant(MerchantRequest::SteerTo { node: "constantinople".into(), to: "ragusa".into() }), Box::new(error("not_found"))),
        (
            Request::Merchant(MerchantRequest::Policies { node: "constantinople".into() }),
            Box::new(|v: &Value| {
                v["result"]["current"] == "maximize_profit"
                    && v["result"]["policies"][1]["can"] == false
                    && v["result"]["policies"][1]["policy"] == "hostile_trading"
            }),
        ),
        (
            Request::Merchant(MerchantRequest::SetPolicy { node: "constantinople".into(), policy: "hostile_trading".into() }),
            Box::new(|v: &Value| error("rejected")(v) && v["error"]["message"] == "你需要等到1445年1月1日才能在此贸易节点中选择新的贸易政策。"),
        ),
        (Request::Merchant(MerchantRequest::SetPolicy { node: "constantinople".into(), policy: "maximize_profit".into() }), Box::new(error("rejected"))),
        (Request::Merchant(MerchantRequest::SetPolicy { node: "constantinople".into(), policy: "nothing".into() }), Box::new(error("not_found"))),
        (
            Request::Colonist(ColonistRequest::Options { province: province(151) }),
            Box::new(|v: &Value| {
                v["result"]["expel_from"] == json!([])
                    && v["result"]["growing"] == false
                    && v["result"]["start_growth"]["reasons"] == json!(["没有可用的殖民者"])
            }),
        ),
        (Request::Colonist(ColonistRequest::ExpelMinorities { province: province(151), from: province(149) }), Box::new(error("rejected"))),
        (Request::Colonist(ColonistRequest::StartSettlementGrowth { province: province(151) }), Box::new(error("rejected"))),
        (Request::Colonist(ColonistRequest::StopSettlementGrowth { province: province(151) }), Box::new(error("rejected"))),
        // The cursor first: a request after an input waits for the game to
        // have it (input::tests).
        (Request::Input(InputRequest::Get {}), Box::new(|v: &Value| *v == ok(json!({"x": 10, "y": 20, "buttons": [], "keys": []})))),
        (
            Request::Input(InputRequest::Click { x: 1, y: 2, button: None, count: None, modifiers: None, scale: None }),
            Box::new(|v: &Value| *v == ok(Value::Null)),
        ),
        (Request::Input(InputRequest::Move { x: 1, y: 2, modifiers: None, scale: Some(50) }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (
            Request::Input(InputRequest::Drag { from_x: 1, from_y: 2, to_x: 3, to_y: 4, button: Some(Button::Right), modifiers: None, scale: None }),
            Box::new(|v: &Value| *v == ok(Value::Null)),
        ),
        (
            Request::Input(InputRequest::Scroll { x: 1, y: 2, direction: Direction::Up, amount: Some(3), modifiers: None, scale: None }),
            Box::new(|v: &Value| *v == ok(Value::Null)),
        ),
        (Request::Input(InputRequest::Key { keys: keys("f1"), repeat: None }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Input(InputRequest::Type { text: "中文".into() }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Input(InputRequest::MouseDown { x: 1, y: 2, button: None, scale: None }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Input(InputRequest::MouseUp { x: 1, y: 2, button: None, scale: None }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Input(InputRequest::KeyDown { key: keys("shift") }), Box::new(|v: &Value| *v == ok(Value::Null))),
        (Request::Input(InputRequest::KeyUp { key: keys("shift") }), Box::new(|v: &Value| *v == ok(Value::Null))),
    ];
    // A developer build's memory account, and its census, which counts
    // nothing here; first, ahead of the input, which waits for frames of its
    // own.
    #[cfg(feature = "dev-memory")]
    {
        let census = cfg!(feature = "dev-heap-census");
        use eu4_harmony_remote::memory::MemoryRequest;
        let memory: Vec<(Request, Check)> = vec![
            (
                Request::Memory(MemoryRequest::Get {}),
                Box::new(move |v: &Value| {
                    let r = &v["result"];
                    r["private"].as_u64() > Some(0)
                        && r["generations"] == json!([])
                        && r["census"] == census
                        && r["heaps"].as_array().is_some_and(|h| !h.is_empty())
                }),
            ),
            (
                Request::Memory(MemoryRequest::Leftovers { sites: None, holders: Some(true) }),
                Box::new(move |v: &Value| if census { v["result"]["sites"] == json!([]) && v["result"]["bytes"] == 0 } else { error("unavailable")(v) }),
            ),
            (
                Request::Memory(MemoryRequest::Generation {}),
                Box::new(move |v: &Value| if census { v["result"].as_u64() > Some(0) } else { error("unavailable")(v) }),
            ),
            // Not a site, so that nothing begins or stops being watched.
            (
                Request::Memory(MemoryRequest::Watch { sites: "0x10,nothing".into() }),
                Box::new(move |v: &Value| error(if census { "invalid_argument" } else { "unavailable" })(v)),
            ),
            // Asked how it is, so that it stays off.
            (
                Request::Memory(MemoryRequest::Quarantine { on: None }),
                Box::new(move |v: &Value| if census { v["result"]["on"] == false && v["result"]["blocks"] == 0 } else { error("unavailable")(v) }),
            ),
            // No game here, and so no heap of its own to walk.
            (Request::Memory(MemoryRequest::Pages { sites: None }), Box::new(error("unavailable"))),
            (Request::Memory(MemoryRequest::Uncounted { sizes: None, samples: None }), Box::new(error("unavailable"))),
        ];
        cases.splice(0..0, memory);
    }
    // The diplomacy panel's options, and each of its actions toward
    // Wallachia, which the fake game refuses, every argument an example of
    // its kind; ahead of the input, which waits for frames of its own.
    let mut panel: Vec<(Request, Check)> = Vec::new();
    panel.push((
        Request::Diplomacy(DiplomacyRequest::Options { country: CountryTag(*b"BYZ") }),
        Box::new(|v: &Value| v["result"]["actions"][0]["acceptance"]["accepts"] == false),
    ));
    let named: BTreeSet<String> = cases.iter().chain(&panel).map(|(r, _)| r.name()).collect();
    for capability in capabilities().iter().filter(|c| c.group == "diplomacy" && !named.contains(&c.name())) {
        let mut request = serde_json::Map::new();
        request.insert("capability".into(), capability.name().into());
        for arg in capability.args.iter().filter(|a| !a.optional) {
            let example = match arg.kind {
                Kind::Country => "WAL",
                kind => kind.example(),
            };
            request.insert(arg.name.into(), arg.kind.json(example).unwrap());
        }
        let request = eu4_harmony_remote::request_from_json(&Value::Object(request).to_string()).unwrap();
        panel.push((request, Box::new(error("rejected"))));
    }
    panel.push((Request::Subject(SubjectRequest::Options { subject: CountryTag(*b"WAL") }), Box::new(|v: &Value| v["result"][1]["active"] == true)));
    panel.push((
        Request::Subject(SubjectRequest::Interact { subject: CountryTag(*b"WAL"), interaction: "scutage".into(), third_party: None, province: None }),
        Box::new(|v: &Value| *v == ok(Value::Null)),
    ));
    cases.splice(0..0, panel);
    // Every capability is among them.
    let asked: BTreeSet<String> = cases.iter().map(|(r, _)| r.name()).collect();
    let all: BTreeSet<String> = capabilities().iter().map(|c| c.name()).collect();
    assert_eq!(asked, all);

    let replies: Vec<Reply> = cases.iter().map(|(r, _)| engine.submit(r.clone())).collect();
    assert!(replies.iter().all(|r| answer(r).is_null()), "nothing runs before a safe point");
    engine.run(&mut game, &Place::Game, PLENTY);
    for ((request, expected), reply) in cases.iter().zip(&replies) {
        let got = answer(reply);
        assert!(expected(&got), "{}: {got}", request.name());
    }
    // Every answer has the shape its capability describes, as Lua has it.
    let mut shaped = BTreeSet::new();
    for (request, _) in &cases {
        let name = request.name();
        if let Outcome::Done(Ok(answer)) = dispatch(request.clone(), &mut Fake::default(), &events::Buffer::new(), std::time::Instant::now()) {
            let lua = eu4_harmony_checks::form::as_lua(|| serde_json::to_value(&answer)).unwrap();
            let shape = eu4_harmony_remote::result_shape(&name).unwrap_or_else(|| panic!("{name} has no result shape"));
            if let Err(why) = shape.check(&lua) {
                panic!(
                    "{name}'s answer is not its shape: {why}
{lua}"
                );
            }
            shaped.insert(name);
        }
    }
    assert!(shaped.len() > 25, "only {} answers were checked against their shapes", shaped.len());
    // The layer was called with what the requests said, in their order.
    let byz = format!("{:x}", fake::byz().0);
    let tur = format!("{:x}", fake::tur().0);
    for call in
        [format!("hostile {tur} {byz}"), format!("move {ARMY} 151"), format!("stop {ARMY}"), format!("can step {ARMY} 149 316"), "console cash 100".into()]
    {
        assert!(game.calls.contains(&call), "{call} not in {:#?}", game.calls);
    }
    for call in ["look at province 151", "look at 100 200", "zoom 1200", "recruit 149 infantry", "recruit 151 cavalry", "recruit 1 galley"] {
        assert!(game.calls.iter().any(|c| c == call), "{call} not in {:#?}", game.calls);
    }
    let moved = game.calls.iter().position(|c| *c == format!("move {ARMY} 151")).unwrap();
    let stopped = game.calls.iter().position(|c| *c == format!("stop {ARMY}")).unwrap();
    assert!(moved < stopped);
    // Answers reach the layer as asked: an accept is not a decline.
    assert_eq!(game.answered, [(7, 2), (3, 1), (3, 0)]);
}

#[test]
fn a_province_snapshot_carries_its_name_s_characters_and_every_kind() {
    let engine = Engine::new(remember);
    let reply = engine.submit(Request::Province(ProvinceRequest::Get { province: province(151) }));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    let recruit = |kind: &str, unit_type: Value, reasons: Value| json!({"kind": kind, "unit_type": unit_type, "allowed": false, "reasons": reasons});
    let not_ours = json!(["君士坦丁堡不是我们的领土"]);
    let expected = ok(json!({
        "id": 151, "name": "君士坦丁堡", "owner": "BYZ", "controller": "BYZ",
        "development": {"tax": 10.0, "production": 9.5, "manpower": 6.0},
        "terrain": "farmlands", "fort": {"level": 2, "mothballed": false},
        "buildings": ["fort_15th", "marketplace"],
        "autonomy": 25.0, "culture": "greek", "religion": "orthodox", "trade_good": "silk", "unrest": null,
        "trade_node": "constantinople", "center_of_trade": 3, "trade_company": false, "state": "state", "edict": null,
        "cores": ["BYZ"], "claims": ["TUR"], "constructions": null, "missionary": null, "colony_size": null,
    }));
    assert_eq!(answer(&reply), expected);
    // What can be recruited there is recruit.options', every kind in order.
    let options = engine.submit(Request::Recruit(RecruitRequest::Options { province: province(151) }));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    assert_eq!(
        answer(&options),
        ok(json!([
            recruit("infantry", json!("eastern_medieval_infantry"), not_ours.clone()),
            recruit("cavalry", json!("eastern_knights"), not_ours),
            recruit("artillery", Value::Null, json!([])),
            recruit("heavy_ship", json!("early_carrack"), json!([])),
            recruit("light_ship", json!("early_war_galley"), json!([])),
            recruit("galley", json!("galley"), json!([])),
            recruit("transport", json!("cog"), json!([])),
        ]))
    );
    // On the pipe the name is its characters, not escapes.
    let line = response_json(&reply_again(&engine)).to_string();
    assert!(line.contains(r#""name":"君士坦丁堡""#), "{line}");
}

/// The snapshot of 151 asked once more.
fn reply_again(engine: &Engine) -> Response {
    let reply = engine.submit(Request::Province(ProvinceRequest::Get { province: province(151) }));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    reply.wait(Duration::ZERO).unwrap()
}

#[test]
fn a_recruit_order_from_json_reaches_the_layer_with_its_kind() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let replies: Vec<Reply> = ["infantry", "cavalry", "artillery", "heavy_ship", "light_ship", "galley", "transport"]
        .iter()
        .map(|kind| {
            engine.submit(eu4_harmony_remote::request_from_json(&json!({"capability": "recruit.order", "province": 149, "kind": kind}).to_string()).unwrap())
        })
        .collect();
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(replies.iter().all(|r| answer(r) == ok(Value::Null)));
    let keys: Vec<String> = game::provinces::UnitKind::ALL.iter().map(|k| format!("recruit 149 {}", k.key())).collect();
    assert_eq!(game.calls, keys);
    let unknown = eu4_harmony_remote::request_from_json(r#"{"capability": "recruit", "province": 149, "kind": "marines"}"#).unwrap_err();
    assert_eq!(unknown.code, Code::BadRequest);
}

#[test]
fn every_request_is_written_to_the_log() {
    let engine = Engine::new(remember);
    let reply = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(1), transports: None }));
    let fine = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    assert!(answer(&reply)["ok"] == false && answer(&fine)["ok"] == true);
    let lines = log();
    let line = |text: &str| lines.iter().any(|l| l.contains(text));
    assert!(line(&format!(r#"{{"capability":"army.move","army":"{ARMY}","province":1}}: rejected: no route to 1 in "#)), "{lines:#?}");
    assert!(line(r#"{"capability":"session.get"}: ok in "#), "{lines:#?}");
}

#[test]
fn outside_a_game_requests_that_need_one_wait_or_are_answered() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let session = engine.submit(Request::Session(SessionRequest::Get {}));
    let language = engine.submit(Request::Text(TextRequest::Language {}));
    let stop = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));

    // While a game is being set up: what needs no game runs, the rest waits.
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert_eq!(answer(&language), ok(json!("simp_chinese")));
    assert!(answer(&session).is_null() && answer(&stop).is_null());
    assert_eq!(game.calls, ["language"]);

    // In the main menu they are answered without the game being asked.
    let menu = Place::Outside(Some(Error::new(Code::NotInGame, "no game is running")));
    let later = engine.submit(Request::Army(ArmyRequest::List {}));
    engine.run(&mut game, &menu, PLENTY);
    for reply in [&session, &stop, &later] {
        assert_eq!(answer(reply), json!({"ok": false, "error": {"code": "not_in_game", "message": "no game is running"}}));
    }
    assert_eq!(game.calls, ["language"]);
}

#[test]
fn waiting_requests_keep_their_order_and_run_at_the_game_frame() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let moved = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(151), transports: None }));
    let stopped = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    let read = engine.submit(Request::Army(ArmyRequest::Get { army: army(ARMY) }));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.calls, [format!("move {ARMY} 151"), format!("stop {ARMY}"), format!("army {ARMY}")]);
    assert_eq!((answer(&moved), answer(&stopped)), (ok(Value::Null), ok(Value::Null)));
    assert_eq!(answer(&read)["result"]["route"], json!([]));
}

#[test]
fn the_time_budget_ends_a_frame_and_the_rest_runs_at_the_next() {
    let engine = Engine::new(remember);
    let mut game = Fake { delay: Duration::from_millis(20), ..Default::default() };
    let replies: Vec<Reply> = (0..3).map(|_| engine.submit(Request::Session(SessionRequest::Get {}))).collect();
    // At least one request runs, however small the budget, and once it is
    // spent the rest wait for the next frame. (Each takes 20 ms at least,
    // however loaded the machine, so the budget is spent after the first.)
    engine.run(&mut game, &Place::Game, Duration::from_millis(1));
    assert_eq!(game.calls.len(), 1);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.calls.len(), 3);
    assert!(replies.iter().all(|r| answer(r)["ok"] == true));
}

#[test]
fn a_request_given_up_before_it_runs_never_runs() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    drop(engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(151), transports: None })));
    let kept = engine.submit(Request::Army(ArmyRequest::Get { army: army(ARMY) }));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.calls, [format!("army {ARMY}")]);
    assert_eq!(answer(&kept)["result"]["route"], json!([]));
    let given_up = format!(r#"{{"capability":"army.move","army":"{ARMY}","province":151}}: given up by its client before it ran"#);
    assert!(log().iter().any(|l| l.contains(&given_up)), "{:#?}", log());
}

#[test]
fn a_read_that_waits_is_answered_at_a_later_safe_point() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let read = |wait_ms| Request::Events(EventsRequest::Read { since: Some(0), wait_ms: Some(wait_ms), process: None, kinds: None, countries: None });

    let next = engine.submit(read(10_000));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert!(answer(&next).is_null());
    // Requests behind it are not held up.
    let language = engine.submit(Request::Text(TextRequest::Language {}));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(answer(&next).is_null());
    assert_eq!(answer(&language)["ok"], true);

    engine.record([Event::ArmyOrdered { army: army(ARMY) }]);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(
        answer(&next),
        ok(json!({"process": process(), "events": [{"number": 1, "event": "army_ordered", "army": ARMY.to_string()}], "last": 1, "missed": false}))
    );
    assert!(log().iter().any(|l| l.contains("events: ok after ")));

    // With something to read already, it answers at once; with nothing, when
    // its time is up, in the main menu too.
    let at_once = engine.submit(read(10_000));
    let short = engine.submit(Request::Events(EventsRequest::Read { since: Some(1), wait_ms: Some(1), process: None, kinds: None, countries: None }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert_eq!(answer(&at_once)["result"]["last"], 1);
    std::thread::sleep(Duration::from_millis(5));
    engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "no game"))), PLENTY);
    assert_eq!(answer(&short), ok(json!({"process": process(), "events": [], "last": 1, "missed": false})));

    let too_long = engine.submit(read(events::LONGEST_WAIT + 1));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(error("invalid_argument")(&answer(&too_long)));

    // A waiting read given up is dropped.
    let given_up = engine.submit(Request::Events(EventsRequest::Read { since: Some(1), wait_ms: Some(10_000), process: None, kinds: None, countries: None }));
    engine.run(&mut game, &Place::Game, PLENTY);
    drop(given_up);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(log().iter().any(|l| l.contains("events: given up by its client while it waited")));
}

#[test]
fn requests_from_other_threads_are_answered_on_the_one_that_runs_the_engine() {
    static ENGINE: Engine = Engine::new(remember);
    let clients: Vec<_> = (0..4)
        .map(|i| {
            std::thread::spawn(move || {
                let mut got = Vec::new();
                for _ in 0..5 {
                    let reply = ENGINE.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(200 + i), transports: None }));
                    got.push(reply.wait(PLENTY).map(|r| response_json(&r)));
                }
                got
            })
        })
        .collect();
    let mut game = Fake::default();
    while !clients.iter().all(|c| c.is_finished()) {
        ENGINE.run(&mut game, &Place::Game, PLENTY);
        std::thread::sleep(Duration::from_millis(1));
    }
    for client in clients {
        assert!(client.join().unwrap().iter().all(|r| *r == Some(ok(Value::Null))));
    }
    assert_eq!(game.calls.len(), 20);
}

#[test]
fn what_a_game_frame_tells_extensions_is_kept_as_events() {
    let frame = game::Frame {
        ctx: game::Ctx { world: 0, date: 0x035D_7710, player: fake::tur(), country: 0, countries: 0 },
        session: 3,
        new_session: true,
        visibility_current: true,
        // A battle the player neither sees nor takes part in, which remote
        // control, reading the player's view, does not hear.
        changes: vec![
            game::events::Change::Ordered(ARMY),
            game::events::Change::Defeated(ARMY),
            game::events::Change::Battle(game::events::Battle {
                province: 151,
                naval: false,
                attackers: vec![fake::wal()],
                defenders: vec![fake::byz()],
                attacker_won: true,
                attacker_losses: 100,
                defender_losses: 900,
                seen: false,
            }),
        ],
        new_day: true,
        new_month: true,
        new_year: false,
        wars: vec![
            game::wars::Happened::WarDeclared { war: "Ottoman-Byzantine War".into(), attacker: fake::tur(), defender: fake::byz() },
            game::wars::Happened::OwnerChanged { province: 151, from: Some(fake::byz()), to: None },
        ],
    };
    let engine = Engine::new(remember);
    engine.record(happened(&frame));
    let read = engine.submit(Request::Events(EventsRequest::Read { since: None, wait_ms: None, process: None, kinds: None, countries: None }));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    assert_eq!(
        answer(&read)["result"]["events"],
        json!([
            {"number": 1, "event": "session", "session": 3},
            {"number": 2, "event": "army_ordered", "army": ARMY.to_string()},
            {"number": 3, "event": "army_defeated", "army": ARMY.to_string()},
            {"number": 4, "event": "war_declared", "war": "Ottoman-Byzantine War", "attacker": "TUR", "defender": "BYZ"},
            {"number": 5, "event": "province_owner_changed", "province": 151, "from": "BYZ", "to": null},
            {"number": 6, "event": "day", "date": "1444.11.11"},
            {"number": 7, "event": "month", "date": "1444.11.11"},
        ])
    );
    // Only those naming a country asked for, and those naming none.
    let read = engine.submit(Request::Events(EventsRequest::Read {
        since: None,
        wait_ms: None,
        process: None,
        kinds: None,
        countries: Some(eu4_harmony_remote::CountryList(vec![CountryTag(*b"TUR")])),
    }));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    let numbers: Vec<u64> = answer(&read)["result"]["events"].as_array().unwrap().iter().map(|e| e["number"].as_u64().unwrap()).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 6, 7]);
    let quiet = game::Frame { new_session: false, changes: vec![], new_day: false, new_month: false, wars: vec![], ..frame };
    assert!(happened(&quiet).is_empty());
}

#[test]
fn the_layer_s_failures_keep_their_meaning() {
    let cases = [
        (game::Status::NotInGame, Code::NotInGame),
        (game::Status::NotReady, Code::NotReady),
        (game::Status::NotFound, Code::NotFound),
        (game::Status::Rejected, Code::Rejected),
        (game::Status::InvalidArgument, Code::InvalidArgument),
        (game::Status::Unavailable, Code::Unavailable),
        (game::Status::WrongContext, Code::Internal),
        (game::Status::Internal, Code::Internal),
    ];
    for (status, code) in cases {
        assert_eq!(Error::from(game::Fail { status, message: "why".into() }), Error::new(code, "why"));
    }
}

#[test]
fn a_load_or_start_answers_with_the_session_once_the_game_is_ready() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let menu = Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open")));
    let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
    // Asked for in the main menu; the main menu's frames go by while the
    // game takes it up, then a save loads, with no safe point at all.
    engine.run(&mut game, &menu, PLENTY);
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert!(answer(&load).is_null());
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&load), ok(json!({"number": 2, "player": "TUR", "date": "1444.11.11"})));
    assert_eq!(game.calls, ["load 奥斯曼1444_11_11", "session"]);

    // A load that comes to another game than its save's says so.
    let castile = engine.submit(Request::Menu(MenuRequest::Load { save: "Castilla-5".into() }));
    engine.run(&mut game, &menu, PLENTY);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(
        answer(&castile),
        json!({"ok": false, "error": {"code": "rejected", "message": "the game did not load the save: it began a game of 1444.11.11 as TUR, where the save is of 1470.3.13 as CAS; that game is going on"}})
    );

    // What the game refuses is answered at once, or as soon as it says so.
    let missing = engine.submit(Request::Menu(MenuRequest::Load { save: "missing".into() }));
    let start = engine.submit(Request::Menu(MenuRequest::Start { country: CountryTag(*b"FRA") }));
    engine.run(&mut game, &menu, PLENTY);
    assert!(error("not_found")(&answer(&missing)));
    assert!(answer(&start).is_null());
    *game.launch.lock().unwrap() = game::menu::Launch::Failed("the single player setup did not take the country".into());
    engine.run(&mut game, &menu, PLENTY);
    assert_eq!(answer(&start), json!({"ok": false, "error": {"code": "rejected", "message": "the single player setup did not take the country"}}));
}

#[test]
fn loading_starting_and_returning_wait_while_it_is_not_known_where_the_game_is() {
    // A frame the game is not ready in, between a game's frames: neither the
    // main menu nor a game is known. Where the game is answers; loading and
    // returning, which do one thing in the main menu and another in a game,
    // wait for a frame that knows which.
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let state = engine.submit(Request::Menu(MenuRequest::State {}));
    let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
    let back = engine.submit(Request::Menu(MenuRequest::Return {}));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert_eq!(answer(&state)["ok"], true);
    assert!(answer(&load).is_null() && answer(&back).is_null());
    assert_eq!(game.calls, ["place"]);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(game.calls, ["place", "load 奥斯曼1444_11_11", "return"]);
    assert_eq!(answer(&back), ok(json!({"process": process(), "place": "ending"})));
}

#[test]
fn a_load_or_start_whose_game_cannot_be_played_says_so_at_its_first_frame() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
    let start = engine.submit(Request::Menu(MenuRequest::Start { country: CountryTag(*b"FRA") }));
    engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
    // Every frame of the game that came fails: without this answer, they
    // would wait until the process ends.
    let unplayable = Place::Unplayable(Error::new(Code::NotInGame, "no controllable player"));
    let session = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.run(&mut game, &unplayable, PLENTY);
    for got in [answer(&load), answer(&start)] {
        assert_eq!(
            got,
            json!({"ok": false, "error": {"code": "rejected", "message": "the game began, but the player cannot play it (no controllable player): menu says game, and it goes on"}})
        );
    }
    assert_eq!(answer(&session), json!({"ok": false, "error": {"code": "not_in_game", "message": "no controllable player"}}));
    assert_eq!(game.calls, ["load 奥斯曼1444_11_11", "start FRA"]);
}

#[test]
fn a_load_is_compared_with_its_save_as_the_game_read_it() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let load = engine.submit(Request::Menu(MenuRequest::Load { save: "rewritten".into() }));
    engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
    // The load wrote the save before the game read it, as an in-game load
    // writes the ironman exit save of the session it leaves.
    *game.rewritten.lock().unwrap() = Some(game::menu::Expected { player: *b"TUR", date: Some(0x035D_7710) });
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&load), ok(json!({"number": 2, "player": "TUR", "date": "1444.11.11"})));
}

#[test]
fn a_load_the_game_could_not_read_is_refused_though_a_game_comes() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let load = engine.submit(Request::Menu(MenuRequest::Load { save: "奥斯曼1444_11_11".into() }));
    engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
    // The game's loader failed, and the game starts from what it read.
    *game.launch.lock().unwrap() = game::menu::Launch::Failed("the game could not read the save".into());
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&load), json!({"ok": false, "error": {"code": "rejected", "message": "the game could not read the save"}}));
    assert_eq!(game.calls, ["load 奥斯曼1444_11_11"]);
}

#[test]
fn returning_to_the_main_menu_is_answered_before_the_process_ends() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let waiting = engine.submit(Request::Events(EventsRequest::Read { since: Some(0), wait_ms: Some(10_000), process: None, kinds: None, countries: None }));
    engine.run(&mut game, &Place::Game, PLENTY);
    let back = engine.submit(Request::Menu(MenuRequest::Return {}));
    let after = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&back), ok(json!({"process": process(), "place": "ending"})));
    // Until the game starts eu4.exe again, the process answers as before.
    assert_eq!(answer(&after)["ok"], true);
    // At the restart, whoever asked for the return (the player too), what
    // waits or is queued is told the game starts again, at once, and so is
    // whatever comes later: no safe point comes any more.
    let queued = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.end(menu::ending(), Duration::ZERO);
    let later = engine.submit(Request::Text(TextRequest::Language {}));
    for got in [answer(&waiting), answer(&queued), answer(&later)] {
        assert!(error("not_ready")(&got), "{got}");
        assert!(got["error"]["message"].as_str().unwrap().contains("returning to the main menu"), "{got}");
    }
    assert_eq!(game.calls, ["return", "session"]);
}

#[test]
fn an_in_process_return_is_answered_once_the_main_menu_is_up() {
    let engine = Engine::new(remember);
    let returned = Arc::new(Mutex::new(game::menu::Returned::Waiting));
    let mut game = Fake { returning: Some(returned.clone()), ..Fake::default() };
    let back = engine.submit(Request::Menu(MenuRequest::Return {}));
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(back.wait(Duration::ZERO).is_none());
    // The main menu's safe points: the same process answers.
    *returned.lock().unwrap() = game::menu::Returned::Done;
    engine.run(&mut game, &Place::Outside(Some(Error::new(Code::NotInGame, "the main menu is open"))), PLENTY);
    assert_eq!(answer(&back), ok(json!({"process": process(), "place": "main_menu"})));
    // One whose ironman exit save failed leaves the session going on.
    *returned.lock().unwrap() = game::menu::Returned::Waiting;
    let again = engine.submit(Request::Menu(MenuRequest::Return {}));
    engine.run(&mut game, &Place::Game, PLENTY);
    *returned.lock().unwrap() = game::menu::Returned::Failed("the game could not write the ironman exit save".into());
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&again), json!({"ok": false, "error": {"code": "rejected", "message": "the game could not write the ironman exit save"}}));
    assert_eq!(game.calls, ["return", "return"]);
}

#[test]
fn a_return_needs_no_player() {
    // Observing, where what needs a game is refused, the layer decides.
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    let observing = Place::Outside(Some(Error::new(Code::NotInGame, "no controllable player")));
    let session = engine.submit(Request::Session(SessionRequest::Get {}));
    let back = engine.submit(Request::Menu(MenuRequest::Return {}));
    engine.run(&mut game, &observing, PLENTY);
    assert!(error("not_in_game")(&answer(&session)));
    assert_eq!(answer(&back), ok(json!({"process": process(), "place": "ending"})));
    assert_eq!(game.calls, ["return"]);
}

#[test]
fn a_request_sent_in_one_session_never_runs_in_the_next() {
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    engine.enter(2);
    let old = engine.submit(Request::Army(ArmyRequest::Move { army: army(ARMY), province: province(151), transports: None }));
    let reads = engine.submit(Request::Text(TextRequest::Language {}));
    // A save begins to load; what is sent while it loads is for the next.
    engine.leave();
    let new = engine.submit(Request::Session(SessionRequest::Get {}));
    engine.enter(3);
    engine.run(&mut game, &Place::Game, PLENTY);
    let old = answer(&old);
    assert!(error("session_changed")(&old), "{old}");
    assert_eq!(old["error"]["message"], "it was sent in session 2, which ended before it ran; this is session 3");
    assert_eq!(answer(&reads)["ok"], true, "what needs no game has no session");
    assert_eq!(answer(&new)["ok"], true);
    assert_eq!(game.calls, ["language", "session"]);
    // In the same session it runs, also after waiting outside a game frame.
    let same = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert_eq!(answer(&same), ok(Value::Null));
}

#[test]
fn a_request_sent_once_its_session_ended_belongs_to_the_next() {
    use eu4_harmony_remote::events::SessionEnd;
    let engine = Engine::new(remember);
    let mut game = Fake::default();
    engine.enter(2);
    let old = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    // An in-process return: the session ends, and no save is read after it.
    engine.session_ended(SessionEnd::Return);
    // Sent while a new game is set up, it waits for that game.
    let new = engine.submit(Request::Army(ArmyRequest::Stop { army: army(ARMY) }));
    engine.run(&mut game, &Place::Outside(None), PLENTY);
    assert!(answer(&old).is_null() && answer(&new).is_null());
    engine.enter(3);
    engine.run(&mut game, &Place::Game, PLENTY);
    assert!(error("session_changed")(&answer(&old)), "{}", answer(&old));
    assert_eq!(answer(&new), ok(Value::Null));
    assert_eq!(game.calls, [format!("stop {ARMY}")]);
}

#[test]
fn events_say_which_process_numbered_them() {
    let engine = Engine::new(remember);
    engine.record([Event::ArmyOrdered { army: army(ARMY) }, Event::ArmyOrdered { army: army(FLEET) }]);
    let read = |since, from| {
        engine.submit(Request::Events(EventsRequest::Read { since: Some(since), wait_ms: Some(10_000), process: Some(from), kinds: None, countries: None }))
    };
    let (here, other) = (read(1, process()), read(1, process() - 1));
    engine.run(&mut Fake::default(), &Place::Game, PLENTY);
    // From this process, the events after since; from another, every event
    // kept, at once, with missed: since meant something else there.
    let (here, other) = (answer(&here), answer(&other));
    assert_eq!(here["result"]["events"].as_array().map(Vec::len), Some(1));
    assert_eq!(here["result"]["missed"], false);
    assert_eq!(other["result"]["events"].as_array().map(Vec::len), Some(2));
    assert_eq!(other["result"]["missed"], true);
    assert_eq!(other["result"]["process"], process());
}

#[test]
fn what_happens_inside_a_request_is_kept_as_events() {
    // A console command that loads a save ends the session inside the
    // request that runs it, and remote control keeps the end there.
    use eu4_harmony_remote::events::SessionEnd;
    static ENGINE: Engine = Engine::new(fake::quiet);
    let (send, got) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let ended = || ENGINE.record([Event::SessionEnded { reason: SessionEnd::Load }]);
        let mut game = Fake { console_does: Some(Box::new(ended)), ..Fake::default() };
        let run = ENGINE.submit(Request::Console(ConsoleRequest::Run { command: "cash 100".into() }));
        ENGINE.run(&mut game, &Place::Game, PLENTY);
        let read = ENGINE.submit(Request::Events(EventsRequest::Read { since: Some(0), wait_ms: None, process: None, kinds: None, countries: None }));
        ENGINE.run(&mut game, &Place::Game, PLENTY);
        let _ = send.send((answer(&run), answer(&read)));
    });
    let (run, read) = got.recv_timeout(Duration::from_secs(10)).expect("the engine held itself up keeping an event from inside a request");
    assert_eq!(run, ok(json!("Gold changed by 100")));
    assert_eq!(read["result"]["events"], json!([{"number": 1, "event": "session_ended", "reason": "load"}]), "{read}");
}
