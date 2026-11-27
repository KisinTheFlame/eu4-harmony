//! A game for the tests of the catalogue's stateful groups (time, menu,
//! screenshot, input, diplomacy, peace) and of the pipe: scripted layer
//! slices and a record of the calls. A capability of a group it does not
//! script fails (`unscripted`), as every request of a bound group
//! (`catalogue::bind`) does.
use super::buffers::{byz, tur, wal};
use crate::capabilities::catalogue::*;
use crate::capabilities::game::{
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

/// Handler for bound-group requests in [`Fake`] tests.
pub type ScriptedBound = Box<dyn FnMut(&eu4_harmony_remote::Request) -> Option<eu4_harmony_remote::Response> + Send>;

#[derive(Default)]
pub struct Fake {
    /// Every capability called, with its arguments.
    pub calls: Vec<String>,
    /// The screenshot asked for, taken or not; a 2x1 image taken at once
    /// when None.
    pub shot: Option<Arc<crate::render::screenshot::Shot>>,
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
    pub inputs: Vec<crate::render::input::Action>,
    /// How many of them the game has had.
    pub inputs_handled: u64,
    /// The player, as the session gives it; the Ottomans played when None.
    pub player: Option<game::player::Player>,
    /// Optional handler for bound-group requests (such as `countries.*`,
    /// `provinces.*`, `console.run`) that have no dedicated `Layer` trait.
    pub scripted: Option<ScriptedBound>,
}

impl Fake {
    fn call(&mut self, what: String) {
        self.calls.push(what);
    }
}

// Called as no one: bound-group requests go through `scripted` when present,
// or fail as unscripted otherwise.
impl Game for Fake {
    fn as_game(&mut self) -> &mut dyn Game {
        self
    }

    fn caller(&self) -> Option<game::Caller> {
        None
    }

    fn scripted_bound(&mut self, request: &eu4_harmony_remote::Request) -> Option<eu4_harmony_remote::Response> {
        self.scripted.as_mut().and_then(|f| f(request))
    }
}

/// The country of the made-up campaign a tag names: the Ottomans, Byzantium
/// or Wallachia, refused as the layer refuses any other.
pub fn found(tag: CountryTag) -> GResult<Tag> {
    match &tag.0 {
        b"TUR" => Ok(tur()),
        b"BYZ" => Ok(byz()),
        b"WAL" => Ok(wal()),
        _ => Err(eu4_harmony_remote::checks::countries::not_found(&tag.to_string()).into()),
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

    fn session(&mut self) -> GResult<game::session::Session> {
        self.call("session".into());
        // 1444.11.11
        let player = self.player.unwrap_or(game::player::Player::Plays(tur()));
        Ok(game::session::Session { session: 2, date: 0x035D_7710, paused: true, visibility_current: true, player })
    }

    fn leave_view(&mut self) -> GResult<game::session::Session> {
        self.call("leave view".into());
        if let Some(game::player::Player::Observes { in_view, .. }) = &mut self.player {
            *in_view = None;
        }
        menu::Layer::session(self)
    }
}

impl input::Layer for Fake {
    fn take(&mut self, action: crate::render::input::Action, scale: u32) -> GResult<crate::render::input::Taken> {
        self.call(format!("input {action:?} at scale {scale}"));
        self.inputs.push(action);
        Ok(crate::render::input::Taken(self.inputs.len() as u64))
    }

    fn handled(&mut self, taken: crate::render::input::Taken) -> bool {
        taken.0 <= self.inputs_handled
    }

    fn cursor(&mut self) -> GResult<crate::render::input::Cursor> {
        self.call("cursor".into());
        Ok(crate::render::input::Cursor { at: Some((10, 20)), ..Default::default() })
    }
}

impl screenshot::Layer for Fake {
    fn screenshot(&mut self, percent: u32) -> GResult<Arc<crate::render::screenshot::Shot>> {
        self.call(format!("screenshot {percent}"));
        crate::render::screenshot::check_scale(percent)?;
        Ok(self.shot.clone().unwrap_or_else(|| {
            let shot = crate::render::screenshot::Shot::new(percent);
            shot.finish(Ok(crate::render::screenshot::Image { png: vec![0x89, b'P', b'N', b'G'], width: 2, height: 1, frame_width: 2, frame_height: 1 }));
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
            return Err(eu4_harmony_remote::checks::time::over(eu4_harmony_remote::Date(end)).into());
        }
        self.clock.time.pause = game::time::Pause::Running;
        Ok(())
    }

    fn set_speed(&mut self, speed: u32) -> GResult<()> {
        self.call(format!("speed {speed}"));
        eu4_harmony_remote::checks::time::speed(speed)?;
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
    fn panel_action(&mut self, request: eu4_harmony_remote::diplomacy::DiplomacyRequest) -> GResult<(game::diplomacy::Ticket, game::diplomacy::Act)> {
        let Some((country, act)) = game::diplomacy::panel_target_and_act(request) else {
            return fail(Status::InvalidArgument, "request is not a diplomacy panel action");
        };
        self.call(format!("act {country} Act({act:?})"));
        match act {
            game::diplomacy::Act::Warn if found(country)? == byz() => Ok((game::diplomacy::Ticket(4), act)),
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
