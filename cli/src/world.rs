//! The fake game's world: what a test describes once with `mod:world{...}`,
//! and the answers the fake game gives from it (`answer`). Each answer is
//! the capability's result as the game writes it, in Lua's form.
//!
//! The world holds the player, the date, countries and which are hostile,
//! provinces with their neighbors, and armies. A route is the shortest over
//! land an army can stand on, across straits; a step is to a neighbor of
//! that kind; an attack is on an army of a country hostile to the player.
//! An order sets or clears the army's route, as this mod's; armies do not
//! move as days pass.
//!
//! What the world refuses, it refuses as the core does, through the same
//! checks and in the same words (`eu4_harmony_checks`): an ID of no army of
//! the player's, a fleet where a land army is asked for, an order to an army
//! the game would not let the framework order, a target the player does not
//! see. The game is never paused in the world.
use eu4_harmony_checks::{armies as army_checks, countries as country_checks, provinces as province_checks};
use eu4_harmony_remote::{armies::Blocked, Error};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The world as a test describes it.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    #[serde(default)]
    player: Option<String>,
    #[serde(default)]
    date: Option<Value>,
    /// Each country's fields as `country.get` answers them, by tag; those
    /// left out are zero or empty.
    #[serde(default)]
    countries: BTreeMap<String, Map<String, Value>>,
    /// Pairs of countries at war with each other.
    #[serde(default)]
    hostile: Vec<(String, String)>,
    #[serde(default)]
    provinces: Vec<Place>,
    #[serde(default)]
    armies: Vec<Army>,
    #[serde(default)]
    selected: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Place {
    id: u32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "land")]
    kind: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    controller: Option<String>,
    #[serde(default = "seen")]
    visible: bool,
    #[serde(default)]
    neighbors: Vec<u32>,
    /// Neighbors across a strait.
    #[serde(default)]
    straits: Vec<u32>,
    #[serde(default)]
    terrain: Option<String>,
    #[serde(default)]
    development: Option<Value>,
    #[serde(default)]
    fort: Option<Value>,
    #[serde(default)]
    buildings: Vec<String>,
}

fn land() -> String {
    "land".into()
}

fn seen() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Army {
    id: u64,
    owner: String,
    #[serde(default = "army")]
    kind: String,
    #[serde(default)]
    province: Option<u32>,
    #[serde(default)]
    regiments: u32,
    #[serde(default)]
    blocked: Vec<Blocked>,
    #[serde(default)]
    route: Vec<u32>,
    #[serde(default)]
    route_by: Option<String>,
    /// Its strength, 1 a full regiment; every regiment full when absent.
    #[serde(default)]
    strength: Option<f64>,
    /// Its morale; 3, every regiment's most, when absent.
    #[serde(default)]
    morale: Option<f64>,
    #[serde(default)]
    leader: Option<u32>,
}

fn army() -> String {
    "army".into()
}

impl Army {
    /// What of its state stops an order, as the core reads it.
    fn condition(&self) -> army_checks::Condition {
        let is = |why| self.blocked.contains(&why);
        army_checks::Condition {
            in_battle: is(Blocked::InBattle),
            sieging: is(Blocked::Sieging),
            retreating: is(Blocked::Retreating),
            exiled: is(Blocked::Exiled),
            movement_locked: is(Blocked::MovementLocked),
            attached: is(Blocked::Attached),
            has_followers: is(Blocked::HasFollowers),
            busy: is(Blocked::Busy),
            regiments: self.regiments,
        }
    }
}

/// The capabilities the world answers.
pub const ANSWERS: &[&str] = &[
    "session.get",
    "country.get",
    "country.hostile",
    "province.list",
    "province.get",
    "province.neighbors",
    "army.get",
    "army.list",
    "army.at",
    "army.selected",
    "army.route_to",
    "army.can_step",
    "army.can_attack",
    "army.move",
    "army.stop",
];

impl World {
    /// The world `json` describes (from the test's table), or why it is not
    /// one; each army and province checked against its capability's shape.
    pub fn new(json: Value) -> Result<Self, String> {
        let world: World = serde_json::from_value(json).map_err(|e| format!("the world: {e}"))?;
        for (tag, fields) in &world.countries {
            crate::fake::check("country.get", &Value::Object(fields.clone())).map_err(|e| format!("the world's country {tag}: {e}"))?;
        }
        for a in &world.armies {
            crate::fake::check("army.get", &world.army_json(a)).map_err(|e| format!("the world's army {}: {e}", a.id))?;
        }
        for p in &world.provinces {
            crate::fake::check("province.get", &world.province_json(p)).map_err(|e| format!("the world's province {}: {e}", p.id))?;
        }
        Ok(world)
    }

    fn player(&self) -> String {
        self.player.clone().unwrap_or_else(|| "CAS".into())
    }

    fn date(&self) -> Value {
        match &self.date {
            Some(Value::String(text)) => date(text),
            Some(other) => other.clone(),
            None => date("1444.11.11"),
        }
    }

    fn province(&self, id: u32) -> Result<&Place, Error> {
        self.provinces.iter().find(|p| p.id == id).ok_or_else(|| province_checks::not_found(id).into())
    }

    /// The player's army or fleet `id`.
    fn own(&self, id: u64) -> Result<&Army, Error> {
        let player = self.player();
        self.armies.iter().find(|a| a.id == id && a.owner == player).ok_or_else(|| army_checks::not_own(id).into())
    }

    /// The player's land army `id`.
    fn own_land(&self, id: u64) -> Result<&Army, Error> {
        let army = self.own(id)?;
        if army.kind != "army" {
            return Err(army_checks::not_land().into());
        }
        Ok(army)
    }

    /// The player's land army `id`, which the game lets the framework order.
    fn orderable(&self, id: u64) -> Result<&Army, Error> {
        let army = self.own_land(id)?;
        army_checks::orderable(&army.condition())?;
        Ok(army)
    }

    /// Whether the player sees the province: one the world does not have is
    /// seen nowhere.
    fn visible(&self, id: u32) -> bool {
        self.province(id).is_ok_and(|p| p.visible)
    }

    /// A land army the player sees, in a province not under fog.
    fn seen(&self, id: u64) -> Result<&Army, Error> {
        self.armies
            .iter()
            .find(|a| a.id == id && a.kind == "army" && a.province.is_some_and(|p| self.visible(p)))
            .ok_or_else(|| army_checks::not_seen(id).into())
    }

    fn country_known(&self, tag: &str) -> bool {
        tag == self.player()
            || self.countries.contains_key(tag)
            || self.hostile.iter().any(|(a, b)| a == tag || b == tag)
            || self.provinces.iter().any(|p| p.owner.as_deref() == Some(tag))
            || self.armies.iter().any(|a| a.owner == tag)
    }

    fn hostile(&self, a: &str, b: &str) -> bool {
        self.hostile.iter().any(|(x, y)| (x == a && y == b) || (x == b && y == a))
    }

    /// An army can stand there.
    fn passable(&self, id: u32) -> bool {
        self.province(id).is_ok_and(|p| p.kind == "land")
    }

    /// A province's neighbors, each with whether it is across a strait,
    /// both ways round whichever province listed them.
    fn neighbors(&self, id: u32) -> Vec<(u32, bool)> {
        let mut found: BTreeMap<u32, bool> = BTreeMap::new();
        for p in &self.provinces {
            for (list, strait) in [(&p.neighbors, false), (&p.straits, true)] {
                if p.id == id {
                    for n in list {
                        found.insert(*n, strait);
                    }
                } else if list.contains(&id) {
                    found.insert(p.id, strait);
                }
            }
        }
        found.into_iter().collect()
    }

    /// The route an army would take to `to`, as `army.route_to` answers it.
    fn route_from(&self, army: &Army, to: u32) -> Result<Vec<u32>, Error> {
        army.province.and_then(|from| self.route(from, to)).ok_or_else(|| army_checks::no_route(to).into())
    }

    /// The shortest route over land from `from` to `to`, `to` last; none
    /// when there is no way.
    fn route(&self, from: u32, to: u32) -> Option<Vec<u32>> {
        if from == to {
            return Some(Vec::new());
        }
        let mut came: BTreeMap<u32, u32> = BTreeMap::new();
        let mut seen: BTreeSet<u32> = BTreeSet::from([from]);
        let mut queue = VecDeque::from([from]);
        while let Some(at) = queue.pop_front() {
            for (next, _) in self.neighbors(at) {
                if !self.passable(next) || !seen.insert(next) {
                    continue;
                }
                came.insert(next, at);
                if next == to {
                    let mut route = vec![to];
                    let mut back = to;
                    while let Some(&before) = came.get(&back) {
                        if before == from {
                            break;
                        }
                        route.push(before);
                        back = before;
                    }
                    route.reverse();
                    return Some(route);
                }
                queue.push_back(next);
            }
        }
        None
    }

    fn army_json(&self, a: &Army) -> Value {
        let own = a.owner == self.player();
        let full = f64::from(a.regiments);
        let mut units = json!({"infantry": 0, "cavalry": 0, "artillery": 0, "heavy_ship": 0, "light_ship": 0, "galley": 0, "transport": 0});
        units[if a.kind == "navy" { "heavy_ship" } else { "infantry" }] = json!(a.regiments);
        json!({
            "id": a.id, "owner": a.owner, "kind": a.kind, "province": a.province, "regiments": a.regiments,
            "blocked": a.blocked, "route": if own { a.route.clone() } else { Vec::new() }, "route_by": a.route_by,
            "aboard": null, "carrying": 0, "units": units, "strength": a.strength.unwrap_or(full), "max_strength": full,
            "morale": a.morale.unwrap_or(3.0), "max_morale": 3.0, "leader": a.leader,
            "maintenance": if own { json!(0.25 * full) } else { Value::Null },
        })
    }

    fn province_json(&self, p: &Place) -> Value {
        json!({
            "id": p.id,
            "name": p.name.clone().unwrap_or_else(|| format!("Province {}", p.id)),
            "owner": p.owner,
            "controller": p.controller.clone().or_else(|| p.owner.clone()),
            "development": p.development.clone().unwrap_or_else(|| json!({"tax": 1.0, "production": 1.0, "manpower": 1.0})),
            "terrain": p.terrain,
            "fort": p.fort.clone().unwrap_or_else(|| json!({"level": 0, "mothballed": false})),
            "buildings": p.buildings,
            "autonomy": 0.0, "culture": null, "religion": null, "trade_good": null, "unrest": 0.0, "trade_node": null,
            "center_of_trade": 0, "trade_company": false, "state": p.owner.as_ref().map(|_| "state"), "edict": null,
            "cores": p.owner.iter().collect::<Vec<_>>(), "claims": [], "constructions": [], "missionary": null, "colony_size": null,
            "prosperity": p.owner.as_ref().map(|_| 0.0), "devastation": 0.0, "local_manpower": 0.25, "supply_limit": 6.0, "separatism": 0,
            "institutions": [],
        })
    }

    /// The answer to `capability` asked with `request`'s arguments, if the
    /// world answers it.
    pub fn answer(&mut self, capability: &str, request: &Value) -> Option<Result<Value, Error>> {
        let int = |name: &str| request.get(name).and_then(Value::as_u64).unwrap_or_default();
        let text = |name: &str| request.get(name).and_then(Value::as_str).unwrap_or_default().to_string();
        let province = |name: &str| int(name) as u32;
        Some(match capability {
            "session.get" => Ok(json!({"number": 1, "player": self.player(), "observing": false, "date": self.date()})),
            "country.get" => {
                let tag = text("country");
                if !self.country_known(&tag) {
                    return Some(Err(country_checks::not_found(&tag).into()));
                }
                let mut country = json!({
                    "tag": tag, "treasury": 0.0, "manpower": 0, "max_manpower": 0, "stability": 0.0, "prestige": 0.0,
                    "power": {"adm": 0, "dip": 0, "mil": 0}, "wars": [], "relations": [],
                    "technology": {"adm": 3, "dip": 3, "mil": 3}, "ideas": [], "governing_capacity": {"used": 0.0, "max": 0.0},
                    "inflation": 0.0, "mercantilism": 0.0, "loans": {"count": 0, "ducats": 0.0}, "rebels": [],
                });
                if let (Some(fields), Value::Object(all)) = (self.countries.get(&tag), &mut country) {
                    all.extend(fields.clone());
                }
                Ok(country)
            }
            "country.hostile" => {
                let (a, b) = (text("country"), text("other"));
                let unknown = [&a, &b].into_iter().find(|t| !self.country_known(t)).cloned();
                match unknown {
                    Some(unknown) => Err(country_checks::not_found(&unknown).into()),
                    None => Ok(json!(self.hostile(&a, &b))),
                }
            }
            "province.list" => {
                let row = |p: &Place| json!({"id": p.id, "kind": p.kind, "visible": p.visible, "owner": p.owner, "controller": p.controller.clone().or_else(|| p.owner.clone())});
                Ok(Value::Array(match request.get("provinces").and_then(Value::as_array) {
                    Some(ids) => ids.iter().filter_map(Value::as_u64).filter_map(|id| self.provinces.iter().find(|p| u64::from(p.id) == id)).map(row).collect(),
                    None => self.provinces.iter().map(row).collect(),
                }))
            }
            "province.get" => self.province(province("province")).map(|p| self.province_json(p)),
            "province.neighbors" => {
                let id = province("province");
                self.province(id).map(|_| {
                    let list: Vec<Value> = if self.passable(id) {
                        self.neighbors(id).into_iter().filter(|(n, _)| self.passable(*n)).map(|(n, strait)| json!({"province": n, "strait": strait})).collect()
                    } else {
                        Vec::new()
                    };
                    Value::Array(list)
                })
            }
            "army.get" => self.own(int("army")).map(|a| self.army_json(a)),
            "army.list" => {
                let player = self.player();
                Ok(Value::Array(self.armies.iter().filter(|a| a.owner == player).map(|a| self.army_json(a)).collect()))
            }
            "army.at" => {
                let id = province("province");
                // None under fog, as the game shows the player.
                self.province(id).map(|p| {
                    let seen = |a: &&Army| p.visible && a.province == Some(id) && a.kind == "army";
                    Value::Array(self.armies.iter().filter(seen).map(|a| self.army_json(a)).collect())
                })
            }
            "army.selected" => match self.selected {
                Some(id) => self.own_land(id).map(|a| json!(a.id)).map_err(|_| army_checks::none_selected().into()),
                None => Err(army_checks::panel_not_open().into()),
            },
            "army.route_to" => {
                let to = province("province");
                self.own_land(int("army")).and_then(|a| self.route_from(a, to).map(|r| json!(r)))
            }
            "army.can_step" => {
                let (from, to) = (province("from"), province("to"));
                self.own_land(int("army")).map(|_| json!(self.passable(to) && self.neighbors(from).iter().any(|(n, _)| *n == to)))
            }
            "army.can_attack" => {
                let player = self.player();
                self.own_land(int("army")).and_then(|own| {
                    let other = self.seen(int("target"))?;
                    Ok(json!(other.id != own.id && self.hostile(&player, &other.owner)))
                })
            }
            "army.move" => {
                let (id, to) = (int("army"), province("province"));
                let route = self.orderable(id).and_then(|a| self.route_from(a, to));
                route.map(|route| {
                    if let Some(a) = self.armies.iter_mut().find(|a| a.id == id) {
                        a.route_by = (!route.is_empty()).then(|| "this_mod".into());
                        a.route = route;
                    }
                    Value::Null
                })
            }
            "army.stop" => {
                let id = int("army");
                self.orderable(id).map(|_| ()).map(|()| {
                    if let Some(a) = self.armies.iter_mut().find(|a| a.id == id) {
                        a.route.clear();
                        a.route_by = None;
                    }
                    Value::Null
                })
            }
            _ => return None,
        })
    }
}

/// A date as Lua has it, from its text.
fn date(text: &str) -> Value {
    match eu4_harmony_checks::Date::parse(text) {
        Some(d) => eu4_harmony_checks::form::as_lua(|| serde_json::to_value(d)).unwrap_or(Value::Null),
        None => Value::String(text.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_remote::Code;

    /// Castile: Toledo between Madrid and Valencia, Seville off by a strait
    /// over a sea; Granada hostile.
    fn spain() -> World {
        World::new(json!({
            "player": "CAS",
            "hostile": [["CAS", "GRA"]],
            "provinces": [
                {"id": 217, "name": "Madrid", "owner": "CAS", "neighbors": [219]},
                {"id": 219, "name": "Toledo", "owner": "CAS", "neighbors": [220]},
                {"id": 220, "name": "Valencia", "owner": "ARA"},
                {"id": 223, "name": "Granada", "owner": "GRA", "straits": [219]},
                {"id": 1300, "kind": "sea", "neighbors": [217]},
            ],
            "armies": [
                {"id": 1, "owner": "CAS", "province": 217, "regiments": 10},
                {"id": 2, "owner": "GRA", "province": 223, "regiments": 6, "route": [219]},
            ],
            "selected": 1,
        }))
        .unwrap()
    }

    fn ask(world: &mut World, capability: &str, request: Value) -> Result<Value, Error> {
        let answer = world.answer(capability, &request).unwrap();
        if let Ok(value) = &answer {
            crate::fake::check(capability, value).unwrap();
        }
        answer
    }

    #[test]
    fn the_world_answers_with_the_game_s_shapes() {
        let mut w = spain();
        for capability in ANSWERS {
            let request = json!({"army": 1, "province": 219, "country": "CAS", "other": "GRA", "from": 217, "to": 219, "target": 2});
            let _ = ask(&mut w, capability, request);
        }
        assert_eq!(ask(&mut w, "session.get", json!({})).unwrap()["date"], json!({"year": 1444, "month": 11, "day": 11}));
        assert_eq!(ask(&mut w, "army.selected", json!({})).unwrap(), json!(1));
    }

    #[test]
    fn routes_go_over_land_and_straits_and_orders_set_them() {
        let mut w = spain();
        assert_eq!(ask(&mut w, "army.route_to", json!({"army": 1, "province": 223})).unwrap(), json!([219, 223]));
        assert_eq!(ask(&mut w, "army.route_to", json!({"army": 1, "province": 217})).unwrap(), json!([]));
        assert_eq!(ask(&mut w, "army.route_to", json!({"army": 1, "province": 1300})).unwrap_err().code, Code::NotFound);
        let neighbors = ask(&mut w, "province.neighbors", json!({"province": 219})).unwrap();
        assert_eq!(neighbors, json!([{"province": 217, "strait": false}, {"province": 220, "strait": false}, {"province": 223, "strait": true}]));
        assert_eq!(ask(&mut w, "army.can_step", json!({"army": 1, "from": 217, "to": 219})).unwrap(), json!(true));
        assert_eq!(ask(&mut w, "army.can_step", json!({"army": 1, "from": 217, "to": 220})).unwrap(), json!(false));
        let listed = ask(&mut w, "province.list", json!({"provinces": [223, 1, 217]})).unwrap();
        assert_eq!(listed.as_array().unwrap().iter().map(|p| p["id"].clone()).collect::<Vec<_>>(), vec![json!(223), json!(217)]);
        assert_eq!(ask(&mut w, "army.can_attack", json!({"army": 1, "target": 2})).unwrap(), json!(true));
        ask(&mut w, "army.move", json!({"army": 1, "province": 220})).unwrap();
        let army = ask(&mut w, "army.get", json!({"army": 1})).unwrap();
        assert_eq!((army["route"].clone(), army["route_by"].clone()), (json!([219, 220]), json!("this_mod")));
        ask(&mut w, "army.stop", json!({"army": 1})).unwrap();
        assert_eq!(ask(&mut w, "army.get", json!({"army": 1})).unwrap()["route"], json!([]));
        // Another country's army: seen where it is, its route not read, not ours to get.
        let at = ask(&mut w, "army.at", json!({"province": 223})).unwrap();
        assert_eq!((at[0]["id"].clone(), at[0]["route"].clone()), (json!(2), json!([])));
        assert_eq!(ask(&mut w, "army.get", json!({"army": 2})).unwrap_err().code, Code::NotFound);
        assert_eq!(ask(&mut w, "country.hostile", json!({"country": "GRA", "other": "CAS"})).unwrap(), json!(true));
        assert_eq!(ask(&mut w, "country.get", json!({"country": "ZZZ"})).unwrap_err().code, Code::NotFound);
    }

    #[test]
    fn the_world_refuses_as_the_core_does() {
        let mut w = World::new(json!({
            "player": "CAS",
            "hostile": [["CAS", "GRA"]],
            "provinces": [
                {"id": 217, "owner": "CAS", "neighbors": [219]},
                {"id": 219, "owner": "CAS", "neighbors": [300]},
                {"id": 300, "owner": "GRA", "visible": false},
                {"id": 1300, "kind": "sea", "neighbors": [217]},
            ],
            "armies": [
                {"id": 1, "owner": "CAS", "province": 217, "regiments": 10},
                {"id": 2, "owner": "GRA", "province": 219, "regiments": 6},
                {"id": 3, "owner": "CAS", "kind": "navy", "province": 1300},
                {"id": 4, "owner": "CAS", "province": 219, "regiments": 4, "blocked": ["sieging"]},
                {"id": 5, "owner": "GRA", "province": 300, "regiments": 3},
                {"id": 6, "owner": "CAS", "province": 217},
            ],
        }))
        .unwrap();
        let mut refused = |capability: &str, request: Value| {
            let e = ask(&mut w, capability, request).unwrap_err();
            (e.code, e.message)
        };
        let words = |refusal: eu4_harmony_checks::Refusal| {
            let e = Error::from(refusal);
            (e.code, e.message)
        };
        assert_eq!(refused("army.get", json!({"army": 2})), words(army_checks::not_own(2)));
        assert_eq!(refused("country.get", json!({"country": "ZZZ"})), words(country_checks::not_found("ZZZ")));
        assert_eq!(refused("province.get", json!({"province": 9})), words(province_checks::not_found(9)));
        assert_eq!(refused("army.move", json!({"army": 3, "province": 217})), words(army_checks::not_land()));
        assert_eq!(refused("army.move", json!({"army": 4, "province": 217})).1, "army is sieging");
        assert_eq!(refused("army.stop", json!({"army": 6})).1, "army has no regiments");
        assert_eq!(refused("army.route_to", json!({"army": 1, "province": 1300})), words(army_checks::no_route(1300)));
        assert_eq!(refused("army.can_attack", json!({"army": 1, "target": 5})), words(army_checks::not_seen(5)));
        assert_eq!(refused("army.selected", json!({})), words(army_checks::panel_not_open()));
        // Under fog the player sees no army there.
        assert_eq!(ask(&mut w, "army.at", json!({"province": 300})).unwrap(), json!([]));
        assert_eq!(ask(&mut w, "army.can_attack", json!({"army": 1, "target": 2})).unwrap(), json!(true));
    }

    #[test]
    fn a_world_not_of_the_game_s_shapes_is_refused() {
        assert!(World::new(json!({"armies": [{"id": 1, "owner": "CAS", "kind": "fleet"}]})).unwrap_err().contains("army 1"));
        assert!(World::new(json!({"countries": {"CAS": {"treasure": 5}}})).unwrap_err().contains("treasure is no field of eu4.Country"));
        assert!(World::new(json!({"provinces": [{"id": 1, "nieghbors": []}]})).unwrap_err().contains("nieghbors"));
    }
}
