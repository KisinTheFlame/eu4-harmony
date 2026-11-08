//! `sweep`: every capability remote control has, asked in every place, on
//! the player build (docs/whole-game.md). The places: the main menu; a game
//! as the country, paused; the same at speed; observe mode with no country
//! in view; and with the country in view.
//!
//! In each place it asks the capabilities that read first, those whose
//! answer is more than nothing, then those that act. Each is asked once
//! with the arguments a player would give, made from what the reads
//! answered: a value of a field named as the argument, one the player `can`
//! choose first, or else the player's province, army and a neighbor. Then
//! once for each argument with each value at the edge of its kind. Every
//! answer must be a result of the capability's own shape or a refusal with
//! one of the catalogue's codes but `internal` and `bad_request`; at the
//! main menu a capability that needs a game must be refused `not_in_game`,
//! and with no country in view one that acts as a country `no_country`.
//! What did not hold is a finding: the sweep goes on, back in its place
//! when a capability left it, and fails at its end when there was one.
//!
//! The catalogue is `remote/`'s as xtask is built, so a capability added
//! later is swept with no change here.
use super::{text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    capabilities,
    menu::{Menu, MenuRequest, Place as MenuPlace},
    questions::{Question, QuestionRequest},
    result_shape,
    session::{Session, SessionRequest},
    time::{Time, TimeRequest},
    Capability, Code, CountryTag, Error, GroupRequest, Kind,
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{collections::BTreeMap, time::Duration};

pub struct Sweep;

/// How long one capability may take to answer before it counts as one that
/// hung; the sweep then goes on if the game still answers.
const ANSWER: Duration = Duration::from_secs(90);

/// The capabilities left out: those that leave the place by design, give
/// the player's input or show the game on the desktop, and the developer
/// builds' own.
fn swept(c: &Capability) -> bool {
    let name = c.name();
    let left_out = matches!(name.as_str(), "menu.load" | "menu.start" | "menu.return" | "desktop.show");
    let input = c.group == "input" && name != "input.get";
    !left_out && !input && c.group != "memory"
}

/// A place the capabilities are asked in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    MainMenu,
    Paused,
    Running,
    Observing,
    InView,
}

impl Place {
    const ALL: [Place; 5] = [Self::MainMenu, Self::Paused, Self::Running, Self::Observing, Self::InView];

    fn name(self) -> &'static str {
        match self {
            Self::MainMenu => "main menu",
            Self::Paused => "paused",
            Self::Running => "at speed",
            Self::Observing => "observing",
            Self::InView => "in view",
        }
    }
}

impl Scenario for Sweep {
    fn name(&self) -> &'static str {
        "sweep"
    }

    fn about(&self) -> &'static str {
        "every capability in every place, with a player's arguments and with values at the edge of their kinds: each answered with its shape or a refusal the catalogue has"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Player
    }

    fn minutes(&self) -> u32 {
        40
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter { name: "country", default: "FRA", about: "the country played, and put in view in observe mode" },
            Parameter { name: "places", default: "main menu,paused,at speed,observing,in view", about: "the places to sweep, by commas" },
            Parameter { name: "only", default: "", about: "only the capabilities whose full names begin with one of these, by commas" },
        ]
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let country = parameters.text("country");
        let tag = CountryTag::parse(&country).ok_or_else(|| Stop::Failed(format!("no country tag {country:?}")))?;
        let asked_places = parameters.text("places");
        let places: Vec<Place> = Place::ALL.into_iter().filter(|p| asked_places.split(',').any(|w| w.trim() == p.name())).collect();
        let only: Vec<String> = parameters.text("only").split(',').map(str::trim).filter(|w| !w.is_empty()).map(String::from).collect();
        let mut all: Vec<Capability> =
            capabilities().into_iter().filter(swept).filter(|c| only.is_empty() || only.iter().any(|o| c.name().starts_with(o.as_str()))).collect();
        // Reads first, so that the acts have their values.
        all.sort_by_key(acts);
        *record =
            json!({ "capabilities": all.len(), "places": places.iter().map(|p| p.name()).collect::<Vec<_>>(), "answers": {}, "findings": [], "left": [] });
        let mut sweep = Swept { tag, findings: Vec::new(), answers: BTreeMap::new(), left: Vec::new(), asked: 0 };
        let mut outcome = Ok(());
        for place in places {
            outcome = sweep.place(game, place, &all, record);
            record["answers"] = json!(sweep.answers);
            record["findings"] = json!(sweep.findings);
            record["left"] = json!(sweep.left);
            record["asked"] = json!(sweep.asked);
            if outcome.is_err() {
                break;
            }
        }
        outcome?;
        game.at_main_menu()?;
        match sweep.findings.first() {
            Some(first) => Err(Stop::Failed(format!("{} answers did not hold; the first: {}", sweep.findings.len(), text(&first["what"])))),
            None => Ok(()),
        }
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let places: Vec<String> = record["places"].as_array().into_iter().flatten().map(text).collect();
        let mut md = vec![format!("{} capabilities in {} places, {} requests asked.", text(&record["capabilities"]), places.len(), text(&record["asked"]))];
        let findings: Vec<&Value> = record["findings"].as_array().into_iter().flatten().collect();
        md.extend([String::new(), format!("### What did not hold ({})", findings.len()), String::new()]);
        if findings.is_empty() {
            md.push("Every answer held.".into());
        }
        for f in findings.iter().take(200) {
            md.push(format!("- {} ({}), `{}`: {}", text(&f["capability"]), text(&f["place"]), text(&f["request"]), text(&f["what"])));
        }
        let left: Vec<String> = record["left"].as_array().into_iter().flatten().map(text).collect();
        if !left.is_empty() {
            md.extend([String::new(), format!("The capabilities after which the sweep went back to its place: {}.", left.join(", "))]);
        }
        md.extend([String::new(), "### The answers".into(), String::new()]);
        md.push(format!("| Capability | {} |", places.join(" | ")));
        md.push(format!("| --- |{}", " --- |".repeat(places.len())));
        if let Some(answers) = record["answers"].as_object() {
            for (name, by_place) in answers {
                let cells: Vec<String> = places
                    .iter()
                    .map(|p| {
                        let counts = &by_place[p];
                        counts.as_object().map_or(String::new(), |c| c.iter().map(|(k, n)| format!("{k} {}", text(n))).collect::<Vec<_>>().join(", "))
                    })
                    .collect();
                md.push(format!("| {name} | {} |", cells.join(" | ")));
            }
        }
        md
    }
}

/// Whether a capability acts: its answer is nothing.
fn acts(c: &Capability) -> bool {
    matches!(result_shape(&c.name()), Some(eu4_harmony_remote::shape::Shape::Nothing))
}

/// What the sweep found as it went.
struct Swept {
    tag: CountryTag,
    findings: Vec<Value>,
    /// For each capability, for each place, how many answers of each kind.
    answers: BTreeMap<String, BTreeMap<String, BTreeMap<String, u32>>>,
    /// The capabilities after which it went back to its place.
    left: Vec<String>,
    asked: u32,
}

impl Swept {
    /// Sweeps every capability of `all` in `place`.
    fn place(&mut self, game: &mut Game, place: Place, all: &[Capability], record: &mut Value) -> Played<()> {
        println!("sweeping {}", place.name());
        self.reach(game, place)?;
        let mut values = Values::of(game, place, self.tag)?;
        let mut last = String::from("the place's own reads");
        for c in all {
            for (variant, args) in values.requests(c) {
                self.ensure(game, place, &last)?;
                last = c.name();
                let request = Raw { name: c.name(), args };
                let written = serde_json::to_string(&request.args).unwrap_or_default();
                record["asking"] = json!(format!("{} {written} ({})", c.name(), place.name()));
                self.asked += 1;
                let answer = match game.try_ask::<Value>(&request, ANSWER) {
                    Err(Stop::Hung(why)) => {
                        self.found(c, place, &written, format!("no answer: {why}"));
                        // Gone on with only when the game still answers.
                        game.ask::<Menu>(&MenuRequest::State {})?;
                        self.count(c, place, "no answer");
                        continue;
                    }
                    other => other?,
                };
                match answer {
                    Ok(result) => {
                        self.count(c, place, "ok");
                        if let Some(shape) = result_shape(&c.name()) {
                            if let Err(why) = shape.check_json(&result) {
                                self.found(c, place, &written, format!("the answer is not of its shape: {why}"));
                            }
                        }
                        if variant == Variant::Real && place == Place::MainMenu && c.in_game {
                            self.found(c, place, &written, "answered at the main menu, where it needs a game".into());
                        }
                        if variant == Variant::Real && place == Place::Observing && c.country {
                            self.found(c, place, &written, "answered with no country in view, where it acts as one".into());
                        }
                        values.harvest(&result);
                    }
                    Err(Error { code, message }) => {
                        let word = serde_json::to_value(code).ok().map(|v| text(&v)).unwrap_or_default();
                        self.count(c, place, &word);
                        if matches!(code, Code::Internal | Code::BadRequest) {
                            self.found(c, place, &written, format!("refused {word}: {message}"));
                        }
                        // An argument may be refused first, the place after it.
                        let arguments = matches!(code, Code::InvalidArgument | Code::NotFound);
                        if variant == Variant::Real && place == Place::MainMenu && c.in_game && code != Code::NotInGame && !arguments {
                            self.found(c, place, &written, format!("refused {word} at the main menu, not not_in_game: {message}"));
                        }
                        if variant == Variant::Real && place == Place::Observing && c.in_game && c.country && code != Code::NoCountry && !arguments {
                            self.found(c, place, &written, format!("refused {word} with no country in view, not no_country: {message}"));
                        }
                    }
                }
            }
        }
        record["asking"] = Value::Null;
        Ok(())
    }

    fn found(&mut self, c: &Capability, place: Place, request: &str, what: String) {
        println!("  {} ({}) {request}: {what}", c.name(), place.name());
        self.findings.push(json!({ "capability": c.name(), "place": place.name(), "request": request, "what": what }));
    }

    fn count(&mut self, c: &Capability, place: Place, kind: &str) {
        *self.answers.entry(c.name()).or_default().entry(place.name().into()).or_default().entry(kind.into()).or_default() += 1;
    }

    /// Brings the game to `place` from wherever it is.
    fn reach(&mut self, game: &mut Game, place: Place) -> Played<()> {
        let menu: Menu = game.ask(&MenuRequest::State {})?;
        let tag = self.tag;
        match place {
            Place::MainMenu => game.at_main_menu(),
            Place::Paused | Place::Running => {
                let session = (menu.place == MenuPlace::Game).then(|| game.session()).transpose()?;
                if !session.is_some_and(|s| s.player == Some(tag) && !s.observing) {
                    let _: Session = game.ask(&MenuRequest::Start { country: Some(tag) })?;
                }
                self.pace(game, place)
            }
            Place::Observing | Place::InView => {
                let session = (menu.place == MenuPlace::Game).then(|| game.session()).transpose()?;
                if !session.is_some_and(|s| s.observing) {
                    let _: Session = game.ask(&MenuRequest::Start { country: None })?;
                }
                let session = game.session()?;
                match (place, session.player) {
                    (Place::Observing, Some(_)) => drop(game.ask::<Session>(&SessionRequest::LeaveView {})?),
                    (Place::InView, view) if view != Some(tag) => drop(game.ask::<Session>(&SessionRequest::View { country: tag })?),
                    _ => {}
                }
                let () = game.ask(&TimeRequest::Pause {})?;
                Ok(())
            }
        }
    }

    /// Paused, or running at speed 5 with the questions that stopped it
    /// answered.
    fn pace(&mut self, game: &mut Game, place: Place) -> Played<()> {
        let time: Time = game.ask(&TimeRequest::Get {})?;
        match (place, time.paused) {
            (Place::Running, true) => {
                let questions: Vec<Question> = game.ask(&QuestionRequest::List {})?;
                for q in questions {
                    if let Some(first) = q.options.first() {
                        let () = game.ask(&QuestionRequest::Answer { question: q.id, option: first.option })?;
                    }
                }
                let () = game.ask(&TimeRequest::SetSpeed { speed: 5 })?;
                let () = game.ask(&TimeRequest::Resume {})?;
            }
            (Place::Paused, false) => {
                let () = game.ask(&TimeRequest::Pause {})?;
            }
            _ => {}
        }
        Ok(())
    }

    /// The game in `place` again before the next request, noting the
    /// capability `after` which it was not.
    fn ensure(&mut self, game: &mut Game, place: Place, after: &str) -> Played<()> {
        let menu: Menu = game.ask(&MenuRequest::State {})?;
        let session = (menu.place == MenuPlace::Game).then(|| game.session()).transpose()?;
        let there = match place {
            Place::MainMenu => menu.place == MenuPlace::MainMenu,
            Place::Paused | Place::Running => session.is_some_and(|s| s.player == Some(self.tag) && !s.observing),
            Place::Observing => session.is_some_and(|s| s.observing && s.player.is_none()),
            Place::InView => session.is_some_and(|s| s.observing && s.player == Some(self.tag)),
        };
        if there {
            return match place {
                Place::Paused | Place::Running => self.pace(game, place),
                _ => Ok(()),
            };
        }
        if !self.left.iter().any(|l| l == after) {
            self.left.push(after.to_string());
        }
        self.reach(game, place)
    }
}

/// A request of any capability, by its full name, with its arguments as
/// JSON.
#[derive(Serialize)]
struct Raw {
    #[serde(skip)]
    name: String,
    #[serde(flatten)]
    args: Map<String, Value>,
}

impl GroupRequest for Raw {
    fn full_name(&self) -> String {
        self.name.clone()
    }
}

/// Which of a capability's requests: the player's arguments, or one at the
/// edge of its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    Real,
    Edge,
}

/// The values the requests are made of in one place: what the game
/// answered so far by field name, and the player's own things.
struct Values {
    /// For each field name, the values answers gave it, those of an entry
    /// the player `can` choose first.
    fields: BTreeMap<String, Vec<Value>>,
    player: CountryTag,
    other: CountryTag,
    province: u32,
    army: Option<String>,
    date: String,
}

impl Values {
    /// The player's own things in `place`: the country, a province of it, a
    /// neighbor's tag, an army, the date.
    fn of(game: &mut Game, place: Place, tag: CountryTag) -> Played<Self> {
        let mut values = Self {
            fields: BTreeMap::new(),
            player: tag,
            other: CountryTag::parse("ENG").expect("a tag"),
            province: 183,
            army: None,
            date: "1444.11.11".into(),
        };
        if place == Place::MainMenu {
            return Ok(values);
        }
        let ask = |game: &mut Game, name: &str, args: Value| -> Played<Option<Value>> {
            let args = args.as_object().cloned().unwrap_or_default();
            Ok(game.try_ask::<Value>(&Raw { name: name.into(), args }, ANSWER)?.ok())
        };
        if let Some(time) = ask(game, "time.get", json!({}))? {
            values.date = text(&time["date"]);
        }
        let provinces = ask(game, "province.list", json!({}))?.unwrap_or_default();
        let owned: Vec<&Value> = provinces.as_array().into_iter().flatten().filter(|p| p["owner"] == json!(tag.to_string())).collect();
        if let Some(first) = owned.first().and_then(|p| p["id"].as_u64()) {
            values.province = first as u32;
        }
        let owners = provinces.as_array().into_iter().flatten().filter_map(|p| p["owner"].as_str()).filter(|o| *o != tag.to_string());
        let neighbors = ask(game, "province.neighbors", json!({ "province": values.province }))?.unwrap_or_default();
        let near: Vec<u64> = neighbors.as_array().into_iter().flatten().filter_map(|n| n["province"].as_u64()).collect();
        let neighbor = provinces
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["id"].as_u64().is_some_and(|id| near.contains(&id)))
            .filter_map(|p| p["owner"].as_str())
            .find(|o| *o != tag.to_string());
        if let Some(other) = neighbor.or(owners.clone().next()).and_then(CountryTag::parse) {
            values.other = other;
        }
        if let Some(armies) = ask(game, "army.list", json!({}))? {
            values.army = armies.as_array().into_iter().flatten().find(|a| a["owner"] == json!(tag.to_string())).map(|a| text(&a["id"]));
        }
        Ok(values)
    }

    /// Keeps the fields of an answer, a value of an entry the player `can`
    /// choose before the others.
    fn harvest(&mut self, answer: &Value) {
        match answer {
            Value::Array(items) => items.iter().for_each(|item| self.harvest(item)),
            Value::Object(fields) => {
                let can = fields.get("can") == Some(&Value::Bool(true));
                for (name, value) in fields {
                    match value {
                        Value::String(_) | Value::Number(_) => {
                            let kept = self.fields.entry(name.clone()).or_default();
                            if !kept.contains(value) && kept.len() < 50 {
                                if can {
                                    kept.insert(0, value.clone());
                                } else {
                                    kept.push(value.clone());
                                }
                            }
                        }
                        Value::Array(_) | Value::Object(_) => self.harvest(value),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// The requests of `c`: once with the player's arguments, then once for
    /// each argument with each value at the edge of its kind.
    fn requests(&self, c: &Capability) -> Vec<(Variant, Map<String, Value>)> {
        let mut real = Map::new();
        for arg in c.args.iter().filter(|a| !a.optional) {
            real.insert(arg.name.into(), self.real(c, arg.name, arg.kind));
        }
        let mut requests = vec![(Variant::Real, real.clone())];
        for arg in c.args {
            for edge in edges(c, arg.name, arg.kind) {
                if real.get(arg.name) == Some(&edge) {
                    continue;
                }
                let mut args = real.clone();
                args.insert(arg.name.into(), edge);
                requests.push((Variant::Edge, args));
            }
        }
        requests
    }

    /// The value a player would give the argument `name` of kind `kind`.
    fn real(&self, c: &Capability, name: &str, kind: Kind) -> Value {
        match (c.name().as_str(), name) {
            ("time.advance", "days") => return json!(1),
            ("console.run", "command") => return json!("help"),
            _ => {}
        }
        let field = self.fields.get(name).and_then(|values| values.iter().find(|v| fits(kind, v))).cloned();
        if let Some(value) = field {
            return match (kind, value) {
                (Kind::Army, Value::Number(n)) => Value::String(n.to_string()),
                (_, value) => value,
            };
        }
        let army = self.army.clone().unwrap_or_else(|| Kind::Army.example().into());
        match kind {
            Kind::Country if name == "country" && c.group == "country" => json!(self.player.to_string()),
            Kind::Country => json!(self.other.to_string()),
            Kind::Province => json!(self.province),
            Kind::Army => json!(army),
            Kind::Date => json!(self.date),
            Kind::Integer => json!(1),
            Kind::Boolean => json!(false),
            Kind::Text => json!(""),
            Kind::Provinces => json!([self.province]),
            Kind::Countries => json!([self.other.to_string()]),
            Kind::Armies => json!([army]),
            Kind::Allies => json!("none"),
            other => other.json(other.example()).unwrap_or(Value::Null),
        }
    }
}

/// Whether a value an answer gave fits an argument of `kind`.
fn fits(kind: Kind, value: &Value) -> bool {
    match (kind, value) {
        (Kind::Country, Value::String(s)) => CountryTag::parse(s).is_some(),
        (Kind::Province | Kind::Integer, Value::Number(n)) => n.is_u64() && n.as_u64().is_some_and(|n| n < 1 << 31),
        (Kind::Army, Value::String(s)) => !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()),
        (Kind::Army, Value::Number(n)) => n.is_u64(),
        (Kind::Text, Value::String(_)) => true,
        (Kind::Date, Value::String(s)) => eu4_harmony_remote::Date::parse(s).is_some(),
        _ => false,
    }
}

/// The values at the edge of an argument's kind: what names nothing, the
/// least and the most, the empty and the long.
fn edges(c: &Capability, name: &str, kind: Kind) -> Vec<Value> {
    match (c.name().as_str(), name) {
        ("time.advance", "days") => return vec![json!(0), json!(36501)],
        (_, "wait_ms") => return vec![json!(0), json!(60001)],
        _ => {}
    }
    let long = "ж".repeat(2000);
    match kind {
        Kind::Country => vec![json!("ZZZ")],
        Kind::Province => vec![json!(0), json!(99999)],
        Kind::Army => vec![json!("0"), json!(u64::MAX.to_string())],
        Kind::Date => vec![json!("1.1.1"), json!("9999.12.31")],
        Kind::Integer => vec![json!(0), json!(i32::MAX)],
        Kind::Boolean => vec![json!(true)],
        Kind::Text => vec![json!(""), json!(long)],
        Kind::Provinces => vec![json!([]), json!([0, 99999])],
        Kind::Countries => vec![json!([]), json!(["ZZZ"])],
        Kind::Armies => vec![json!([]), json!(["0"])],
        Kind::Words => vec![json!([]), json!(["no_such_word"])],
        Kind::Allies => vec![json!("all"), json!(["ZZZ"])],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Values {
        Values {
            fields: BTreeMap::new(),
            player: CountryTag::parse("FRA").unwrap(),
            other: CountryTag::parse("BUR").unwrap(),
            province: 183,
            army: Some("4294967377".into()),
            date: "1444.11.11".into(),
        }
    }

    /// Every capability the sweep asks has a request of the player's, and
    /// every kind of argument but the words of a fixed list has values at
    /// its edge, so that a new kind fails here until it has them.
    #[test]
    fn every_capability_swept_has_its_requests_and_every_kind_its_edges() {
        let v = values();
        let all: Vec<Capability> = capabilities().into_iter().filter(swept).collect();
        assert!(all.len() > 250, "only {} capabilities swept", all.len());
        for c in &all {
            let requests = v.requests(c);
            assert_eq!(requests[0].0, Variant::Real);
            for arg in c.args.iter().filter(|a| !a.optional) {
                assert!(requests[0].1.contains_key(arg.name), "{} has no value for {}", c.name(), arg.name);
                let value = &requests[0].1[arg.name];
                assert!(!value.is_null(), "{}'s {} is null", c.name(), arg.name);
            }
        }
        let fixed = [Kind::UnitKind, Kind::Button, Kind::Direction, Kind::Keys, Kind::PowerKind, Kind::Terms];
        let any = capabilities().into_iter().next().unwrap();
        for kind in Kind::ALL.iter().filter(|k| !fixed.contains(k)) {
            assert!(!edges(&any, "x", *kind).is_empty(), "{kind:?} has no edge values");
        }
        assert!(!capabilities().iter().filter(|c| swept(c)).any(|c| c.name() == "menu.return" || c.group == "memory" || c.name() == "input.click"));
    }

    /// An answer's fields fill the arguments of their names, a choice the
    /// player can make first.
    #[test]
    fn the_player_s_arguments_come_from_what_the_game_answered() {
        let mut v = values();
        v.harvest(&json!({ "province": 183, "options": [
            { "building": "temple", "can": false },
            { "building": "marketplace", "can": true },
        ] }));
        let build = eu4_harmony_remote::capability("building.build").unwrap();
        let requests = v.requests(build);
        assert_eq!(requests[0].1["building"], json!("marketplace"));
        assert_eq!(requests[0].1["province"], json!(183));
        let edges: Vec<&Value> = requests[1..].iter().map(|(_, args)| &args["building"]).collect();
        assert!(edges.contains(&&json!("")), "{edges:?}");
        let get = eu4_harmony_remote::capability("country.get").unwrap();
        assert_eq!(v.requests(get)[0].1["country"], json!("FRA"), "a country's own read is of the player's");
        let advance = eu4_harmony_remote::capability("time.advance").unwrap();
        assert_eq!(v.requests(advance)[0].1["days"], json!(1));
    }

    #[test]
    fn a_capability_that_answers_nothing_acts() {
        assert!(acts(eu4_harmony_remote::capability("building.build").unwrap()));
        assert!(!acts(eu4_harmony_remote::capability("building.options").unwrap()));
    }
}
