use super::*;
use serde_json::json;

fn request(value: Value) -> Result<Request, Error> {
    request_from_json(&value.to_string())
}

/// A value of each kind, as a client would write it from its example.
fn sample(kind: Kind) -> Value {
    kind.json(kind.example()).unwrap_or_else(|| panic!("the example of {kind:?} is not one"))
}

/// Each code is named as JSON writes it, and says what it means, which
/// `eu4-harmony wiki remote` shows.
#[test]
fn every_code_is_named_and_explained() {
    for code in Code::ALL {
        assert_eq!(serde_json::to_value(code).unwrap(), json!(code.name()));
        assert!(!code.about().is_empty(), "{} says nothing of what it means", code.name());
    }
}

/// The limits the descriptions state are those the game keeps to.
#[test]
fn descriptions_state_the_limits_kept() {
    let all = capabilities();
    let about = |name: &str, arg: &str| {
        let capability = all.iter().find(|c| c.name() == name).unwrap_or_else(|| panic!("no {name}"));
        prose(capability.args.iter().find(|a| a.name == arg).unwrap_or_else(|| panic!("{name} has no {arg}")).about)
    };
    let speeds = format!("{} (slowest) to {} (fastest)", time::SPEEDS.start(), time::SPEEDS.end());
    for (name, arg) in [("time.set_speed", "speed"), ("time.advance", "speed")] {
        assert!(about(name, arg).contains(&speeds), "{name} {arg}");
    }
    let time = all.iter().find(|c| c.name() == "time.get").unwrap();
    assert!(time.result.contains(&speeds), "time.get");
    assert!(about("time.advance", "days").contains(&format!("from 1 to {}", time::MOST_DAYS)));
    assert!(about("signals", "wait_ms").contains(&format!("at most {}", signals::LONGEST_WAIT)));
    assert!(about("screenshot", "scale").contains(&format!("{} to {}", screenshot::SCALES.start(), screenshot::SCALES.end())));
}

#[test]
fn every_capability_is_described_and_can_be_called_from_its_description() {
    let all = capabilities();
    let mut names: Vec<String> = all.iter().map(Capability::name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), all.len(), "two capabilities share a name");
    for capability in &all {
        let name = capability.name();
        assert!(!prose(capability.about).is_empty() && !capability.result.is_empty(), "{name} is not described");
        // With only its required arguments, and with all of them.
        for with_optional in [false, true] {
            let mut fields = Map::new();
            fields.insert("capability".into(), json!(name));
            for arg in capability.args.iter().filter(|a| with_optional || !a.optional) {
                assert!(!prose(arg.about).is_empty(), "{name} {} is not described", arg.name);
                fields.insert(arg.name.into(), sample(arg.kind));
            }
            let decoded = request(Value::Object(fields.clone())).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(decoded.name(), name);
            assert_eq!(decoded.in_game(), capability.in_game, "{name}");
            // Written back, it is the same object.
            assert_eq!(serde_json::to_value(&decoded).unwrap(), Value::Object(fields), "{name}");
        }
    }
}

#[test]
fn a_request_is_one_flat_object() {
    let wire = json!({"capability": "army.move", "army": "4294967377", "province": 151});
    let decoded = request(wire.clone()).unwrap();
    assert_eq!(decoded, Request::Army(armies::ArmyRequest::Move { army: ArmyId(4294967377), province: ProvinceId(151), transports: None }));
    assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);

    let session = request(json!({"capability": "session.get"})).unwrap();
    assert_eq!(session, Request::Session(session::SessionRequest::Get {}));
    assert_eq!(serde_json::to_value(&session).unwrap(), json!({"capability": "session.get"}));

    let signals = request(json!({"capability": "signals", "since": 5})).unwrap();
    assert_eq!(signals, Request::Signals(signals::SignalsRequest::Read { since: Some(5), wait_ms: None, process: None, kinds: None, countries: None }));
    assert_eq!(serde_json::to_value(&signals).unwrap(), json!({"capability": "signals", "since": 5}));
}

#[test]
fn a_request_that_cannot_be_read_says_why() {
    let bad = |value: Value, says: &str| {
        let error = request(value.clone()).unwrap_err();
        assert_eq!(error.code, Code::BadRequest, "{value}");
        assert!(error.message.contains(says), "{value}: {}", error.message);
    };
    bad(json!({"army": "1"}), "no capability named");
    bad(json!({"capability": 5}), "not a text");
    bad(json!({"capability": "army.fly"}), "no capability army.fly");
    bad(json!({"capability": "army"}), "no capability army");
    bad(json!({"capability": "army.move", "army": "1"}), "missing field `province`");
    bad(json!({"capability": "army.move", "army": "1", "province": 2, "speed": 3}), "unknown field `speed`");
    bad(json!({"capability": "army.get", "army": 4294967377u64}), "an army ID as a decimal string");
    bad(json!({"capability": "army.get", "army": "-1"}), "not an army ID");
    bad(json!({"capability": "country.hostile", "country": "FRANCE", "other": "ENG"}), "not a country tag");
    bad(json!({"capability": "diplomacy.declare_war", "country": "BYZ", "casus_belli": "none", "call_allies": "WAL;TUR"}), "\"WAL;TUR\" is none of them");
    assert_eq!(request_from_json("not json").unwrap_err().code, Code::BadRequest);
}

/// Allies to call are read as they are written: all, none, or tags.
#[test]
fn allies_to_call_are_read_and_written_back() {
    use diplomacy::CallAllies;
    assert_eq!(CallAllies::parse(" all "), Ok(CallAllies::All));
    assert_eq!(CallAllies::parse("none"), Ok(CallAllies::None));
    let named = CallAllies::parse("wal, TUR").unwrap();
    assert_eq!(named, CallAllies::Only(vec![CountryTag(*b"WAL"), CountryTag(*b"TUR")]));
    assert_eq!(named.to_string(), "WAL,TUR");
    assert!(CallAllies::parse("").is_err() && CallAllies::parse("WAL,").is_err());
    // In JSON, a word or a list of tags.
    assert_eq!(serde_json::to_value(&named).unwrap(), json!(["WAL", "TUR"]));
    assert_eq!(serde_json::to_value(CallAllies::All).unwrap(), json!("all"));
    assert_eq!(serde_json::from_value::<CallAllies>(json!(["WAL", "TUR"])).unwrap(), named);
    assert_eq!(serde_json::from_value::<CallAllies>(json!("none")).unwrap(), CallAllies::None);
}

/// One signal of each kind, in the order [`signals::SIGNALS`] describes them.
fn every_signal() -> Vec<signals::Signal> {
    use signals::{SessionEnd, Signal};
    let date = Date::from_ymd(1444, 11, 11).unwrap();
    vec![
        Signal::Session { session: 1 },
        Signal::SessionEnded { reason: SessionEnd::Return },
        Signal::CountryInView { country: Some(CountryTag(*b"TUR")) },
        Signal::Popup { popup: 3, from: "reforms".into() },
        Signal::Day { date },
        Signal::Month { date },
        Signal::Year { date },
        Signal::ArmyOrdered { army: ArmyId(1) },
        Signal::ArmyDefeated { army: ArmyId(1) },
        Signal::WarDeclared { war: "Ottoman-Byzantine War".into(), attacker: CountryTag(*b"TUR"), defender: CountryTag(*b"BYZ") },
        Signal::PeaceMade { war: "Ottoman-Byzantine War".into(), attackers: vec![], defenders: vec![CountryTag(*b"BYZ")], ended: false },
        Signal::ProvinceOwnerChanged { province: ProvinceId(151), from: Some(CountryTag(*b"BYZ")), to: Some(CountryTag(*b"TUR")) },
        Signal::BuildingFinished { province: ProvinceId(151), building: "marketplace".into(), country: CountryTag(*b"TUR") },
        Signal::Battle {
            province: ProvinceId(151),
            naval: false,
            attackers: vec![CountryTag(*b"TUR")],
            defenders: vec![CountryTag(*b"BYZ"), CountryTag(*b"VEN")],
            winner: signals::Winner::Attackers,
            attacker_losses: 1200,
            defender_losses: 3400,
        },
    ]
}

/// The signals described are those there are, by the names JSON gives them,
/// each with what it is and what each of its fields is.
#[test]
fn every_signal_is_described() {
    use signals::SIGNALS;
    let all = every_signal();
    // The countries a signal names are those its fields of countries hold,
    // in their order, for a read's countries.
    let named = |kind| all.iter().find(|e| e.kind() == kind).unwrap().countries();
    let tags = |tags: &[&[u8; 3]]| tags.iter().map(|t| CountryTag(**t)).collect::<Vec<_>>();
    assert_eq!(named(signals::Kind::Battle), tags(&[b"TUR", b"BYZ", b"VEN"]));
    assert_eq!(named(signals::Kind::ProvinceOwnerChanged), tags(&[b"BYZ", b"TUR"]));
    assert_eq!(named(signals::Kind::CountryInView), tags(&[b"TUR"]));
    assert_eq!(named(signals::Kind::Day), tags(&[]));
    // A signal is flattened beside its own number: none of its fields may
    // be named so.
    for signal in &all {
        assert!(serde_json::to_value(signal).unwrap().get("number").is_none(), "{signal:?} has a field named number");
    }
    let names: Vec<Value> = all.iter().map(|e| serde_json::to_value(e).unwrap()["signal"].clone()).collect();
    assert_eq!(names, SIGNALS.iter().map(|(name, _)| json!(name)).collect::<Vec<_>>());
    // A kind, as a read's kinds name it, is the name JSON gives the signal,
    // and the kind of that name.
    for signal in &all {
        assert_eq!(json!(signal.kind()), serde_json::to_value(signal).unwrap()["signal"]);
        assert_eq!(signals::Kind::named(signal.kind().name()), Some(signal.kind()));
    }
    assert_eq!(signals::Kind::named("months"), None);
    for (kind, about) in SIGNALS {
        assert!(!prose(about).is_empty(), "{kind} says not what it is");
        let fields = kind.fields();
        assert!(!fields.is_empty(), "{kind} has no fields");
        for field in fields {
            assert!(!field.about.is_empty(), "{kind}'s {} says not what it is", field.name);
        }
    }
}

/// Each signal calls the handler of its kind, with its fields, or with
/// itself as one table, as Lua has them; on_session's arguments the Lua host
/// gives.
#[test]
fn every_signal_calls_its_handler() {
    let calls: Vec<_> = every_signal().iter().filter_map(lua::handler_call).collect();
    let date = json!({"year": 1444, "month": 11, "day": 11});
    assert_eq!(
        calls.iter().map(|(name, args)| (*name, Value::Array(args.clone()))).collect::<Vec<_>>(),
        [
            ("on_session_ended", json!(["return"])),
            ("on_country_in_view", json!(["TUR"])),
            ("on_popup", json!([3, "reforms"])),
            ("on_day", json!([date])),
            ("on_month", json!([date])),
            ("on_year", json!([date])),
            ("on_army_ordered", json!([1])),
            ("on_army_defeated", json!([1])),
            ("on_war_declared", json!(["Ottoman-Byzantine War", "TUR", "BYZ"])),
            ("on_peace_made", json!(["Ottoman-Byzantine War", [], ["BYZ"], false])),
            ("on_province_owner_changed", json!([151, "BYZ", "TUR"])),
            ("on_building_finished", json!([151, "marketplace", "TUR"])),
            (
                "on_battle",
                json!([{"province": 151, "naval": false, "attackers": ["TUR"], "defenders": ["BYZ", "VEN"], "winner": "attackers", "attacker_losses": 1200, "defender_losses": 3400}])
            ),
        ]
    );
    // Each handler, with the names of its arguments after game, as mods
    // write them.
    let handlers: Vec<(&str, Vec<&str>)> = lua::handlers().iter().map(|h| (h.name, h.args.iter().map(|a| a.name).collect())).collect();
    assert_eq!(
        handlers,
        [
            ("on_session", vec!["saved"]),
            ("on_session_ended", vec!["reason"]),
            ("on_country_in_view", vec!["country"]),
            ("on_popup", vec!["popup", "from"]),
            ("on_day", vec!["date"]),
            ("on_month", vec!["date"]),
            ("on_year", vec!["date"]),
            ("on_army_ordered", vec!["army"]),
            ("on_army_defeated", vec!["army"]),
            ("on_war_declared", vec!["war", "attacker", "defender"]),
            ("on_peace_made", vec!["war", "attackers", "defenders", "ended"]),
            ("on_province_owner_changed", vec!["province", "from", "to"]),
            ("on_building_finished", vec!["province", "building", "country"]),
            ("on_battle", vec!["battle"]),
            ("on_button", vec!["name", "click"]),
            ("on_window", vec!["name", "open"]),
            ("on_map_lost", vec![]),
            ("on_frame", vec![]),
            ("save_data", vec![]),
        ]
    );
}

#[test]
fn identifiers_in_json() {
    assert_eq!(CountryTag::parse("fra"), Some(CountryTag(*b"FRA")));
    assert_eq!(CountryTag::parse("D01"), Some(CountryTag(*b"D01")));
    for bad in ["FR", "FRAN", "F-A", "中国"] {
        assert_eq!(CountryTag::parse(bad), None, "{bad}");
    }
    assert_eq!(serde_json::to_value(CountryTag(*b"TUR")).unwrap(), json!("TUR"));
    assert_eq!(serde_json::to_value(ArmyId(u64::MAX)).unwrap(), json!("18446744073709551615"));
    assert_eq!(serde_json::from_value::<ArmyId>(json!("18446744073709551615")).unwrap(), ArmyId(u64::MAX));
    assert_eq!(serde_json::to_value(ProvinceId(151)).unwrap(), json!(151));
}

#[test]
fn responses_in_json() {
    let ok: Response = Ok(Answer::Country(countries::CountryAnswer::Hostile(true)));
    assert_eq!(response_json(&ok), json!({"ok": true, "result": true}));
    let moved: Response = Ok(Answer::Army(armies::ArmyAnswer::Move(())));
    assert_eq!(response_json(&moved), json!({"ok": true, "result": null}));
    let error = Error::new(Code::NotInGame, "no game is running");
    assert_eq!(error.to_string(), "not_in_game: no game is running");
    let failed: Response = Err(error);
    assert_eq!(response_json(&failed), json!({"ok": false, "error": {"code": "not_in_game", "message": "no game is running"}}));

    let signals = signals::Signals {
        process: 1_761_700_000_000,
        signals: vec![
            signals::Numbered { number: 7, signal: signals::Signal::ArmyOrdered { army: ArmyId(12) } },
            signals::Numbered { number: 8, signal: signals::Signal::Day { date: Date::from_ymd(1444, 11, 12).unwrap() } },
        ],
        last: 8,
        missed: false,
    };
    assert_eq!(
        serde_json::to_value(&signals).unwrap(),
        json!({
            "process": 1_761_700_000_000u64,
            "signals": [{"number": 7, "signal": "army_ordered", "army": "12"}, {"number": 8, "signal": "day", "date": "1444.11.12"}],
            "last": 8,
            "missed": false,
        })
    );
}

#[test]
fn descriptions_in_json() {
    let all = serde_json::to_value(capabilities()).unwrap();
    let hostile = all.as_array().unwrap().iter().find(|c| c["capability"] == "country.hostile").unwrap();
    assert_eq!(
        *hostile,
        json!({
            "capability": "country.hostile",
            "about": "Whether two countries are at war with each other, or either is a rebel faction.",
            "in_game": true,
            "country": false,
            "extensions": true,
            "args": [
                {"name": "country", "kind": "country", "optional": false, "about": "One country."},
                {"name": "other", "kind": "country", "optional": false, "about": "The other."},
            ],
            "result": "whether they are hostile",
        })
    );
}

#[test]
fn kinds_from_text() {
    for &kind in Kind::ALL {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.name()));
        assert!(kind.json(kind.example()).is_some(), "{}'s example {} is not one", kind.name(), kind.example());
    }
    assert_eq!(sample(Kind::Province), json!(151));
    assert_eq!(sample(Kind::Army), json!("4294967377"));
    assert_eq!(Kind::Integer.json("-3"), Some(json!(-3)));
    assert_eq!(Kind::Integer.json("18446744073709551615"), Some(json!(u64::MAX)));
    assert_eq!(Kind::Boolean.json("false"), Some(json!(false)));
    assert_eq!(Kind::UnitKind.json("heavy_ship"), Some(json!("heavy_ship")));
    assert_eq!(sample(Kind::Provinces), json!([151, 152]));
    assert_eq!(Kind::Provinces.json("1, 2"), Some(json!([1, 2])));
    assert_eq!(Kind::Countries.json("fra, ENG"), Some(json!(["FRA", "ENG"])));
    assert_eq!(Kind::Words.json("day,month"), Some(json!(["day", "month"])));
    assert_eq!(Kind::Armies.json("12, 34"), Some(json!(["12", "34"])));
    assert_eq!(Kind::Terms.json(r#"[{"term":"demand_provinces","province":236}]"#), Some(json!([{"term": "demand_provinces", "province": 236}])));
    assert_eq!(Kind::Keys.json("Control+Return"), Some(json!("ctrl+enter")));
    assert_eq!(Kind::Button.json("right"), Some(json!("right")));
    assert_eq!(Kind::PowerKind.json("dip"), Some(json!("dip")));
    for (kind, bad) in [
        (Kind::Provinces, "151,Paris"),
        (Kind::Provinces, ""),
        (Kind::Countries, "FRA,France"),
        (Kind::Words, "day,,month"),
        (Kind::Armies, "12,first"),
        (Kind::Terms, "humiliate"),
        (Kind::Province, "Paris"),
        (Kind::Province, "1.5"),
        (Kind::Integer, "0x10"),
        (Kind::Integer, ""),
        (Kind::Boolean, "yes"),
        (Kind::UnitKind, "Infantry"),
        (Kind::UnitKind, "marines"),
        (Kind::Keys, "ctrl+banana"),
        (Kind::Button, "back"),
        (Kind::Direction, "sideways"),
        (Kind::PowerKind, "ADM"),
    ] {
        assert_eq!(kind.json(bad), None, "{kind:?} {bad}");
    }
    // Text kinds stay text, even when they look like numbers.
    assert_eq!(Kind::Country.json("D01"), Some(json!("D01")));
    assert_eq!(Kind::Army.json("12"), Some(json!("12")));
}

#[test]
fn binaries_in_json() {
    // RFC 4648, section 10.
    for (bytes, text) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
        assert_eq!(binary::encode(bytes.as_bytes()), text);
        assert_eq!(binary::decode(text).as_deref(), Some(bytes.as_bytes()), "{text}");
    }
    let every: Vec<u8> = (0..=255).collect();
    assert_eq!(binary::decode(&binary::encode(&every)), Some(every));
    for bad in ["Zg=", "Zg", "Z===", "Zg==Zg==", "Zh==", "Zm9=", "Zm9v!A==", "=Zm9"] {
        assert_eq!(binary::decode(bad), None, "{bad}");
    }

    let png = Binary::new("image/png", vec![0x89, b'P', b'N', b'G']);
    let value = serde_json::to_value(&png).unwrap();
    assert_eq!(value, json!({"media_type": "image/png", "base64": "iVBORw=="}));
    assert_eq!(Binary::from_json(&value), Some(png.clone()));
    assert_eq!(serde_json::from_value::<Binary>(value).unwrap(), png);
    // Only that shape is a binary.
    for other in [
        json!("iVBORw=="),
        json!({"base64": "iVBORw=="}),
        json!({"media_type": "image/png", "base64": "iVBORw==", "width": 1}),
        json!({"media_type": "image/png", "base64": "not base64"}),
    ] {
        assert_eq!(Binary::from_json(&other), None, "{other}");
    }
}

/// Every permission a function needs is one the manifest may declare.
#[test]
fn the_lua_descriptions_agree_with_the_permissions() {
    for f in lua::FUNCTIONS {
        if let Some(p) = f.permission {
            assert!(lua::PERMISSIONS.iter().any(|q| q.name == p), "{} needs {p}, which is no permission", f.name);
        }
        let top = f.name.split('.').next().unwrap();
        assert!(lua::GLOBALS.contains(&top) || top == "os" || top == "game", "{} is under no global", f.name);
    }
    for p in lua::PERMISSIONS {
        assert!(lua::FUNCTIONS.iter().any(|f| f.permission == Some(p.name)), "nothing needs {}", p.name);
    }
}
