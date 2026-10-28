use super::*;
use serde_json::json;

fn request(value: Value) -> Result<Request, Error> {
    request_from_json(&value.to_string())
}

/// A value of each kind, as a client would write it from its example.
fn sample(kind: Kind) -> Value {
    kind.json(kind.example()).unwrap_or_else(|| panic!("the example of {kind:?} is not one"))
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
    let wire = json!({"capability": "army.move", "army": "4294967377", "goal": 151});
    let decoded = request(wire.clone()).unwrap();
    assert_eq!(decoded, Request::Army(armies::ArmyRequest::Move { army: ArmyId(4294967377), goal: ProvinceId(151) }));
    assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);

    let session = request(json!({"capability": "session"})).unwrap();
    assert_eq!(session, Request::Session(session::SessionRequest::Get {}));
    assert_eq!(serde_json::to_value(&session).unwrap(), json!({"capability": "session"}));

    let events = request(json!({"capability": "events", "since": 5})).unwrap();
    assert_eq!(events, Request::Events(events::EventsRequest::Read { since: Some(5), wait_ms: None }));
    assert_eq!(serde_json::to_value(&events).unwrap(), json!({"capability": "events", "since": 5}));
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
    bad(json!({"capability": "army.move", "army": "1"}), "missing field `goal`");
    bad(json!({"capability": "army.move", "army": "1", "goal": 2, "speed": 3}), "unknown field `speed`");
    bad(json!({"capability": "army.get", "army": 4294967377u64}), "an army ID as a decimal string");
    bad(json!({"capability": "army.get", "army": "-1"}), "not an army ID");
    bad(json!({"capability": "country.hostile", "country": "FRANCE", "other": "ENG"}), "not a country tag");
    assert_eq!(request_from_json("not json").unwrap_err().code, Code::BadRequest);
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
fn dates_as_the_game_keeps_them() {
    // docs/research/dates.md: 1444.11.11 is 0x035D7710.
    let start = Date(0x035D_7710);
    assert_eq!(start.ymd(), (1444, 11, 11));
    assert_eq!(start.to_string(), "1444.11.11");
    assert_eq!(Date::parse("1444.11.11"), Some(start));
    assert_eq!(Date::parse("1445.01.01"), Date::from_ymd(1445, 1, 1));
    assert_eq!(Date::from_ymd(1445, 1, 1).unwrap().0 - start.0, 51 * 24);
    // Every day of a year comes back as itself, and the hour is ignored.
    let first = Date::from_ymd(1450, 1, 1).unwrap();
    for day in 0..365 {
        let date = Date(first.0 + day * 24 + 13);
        let (y, m, d) = date.ymd();
        assert_eq!(Date::from_ymd(y, m, d), Some(Date(first.0 + day * 24)));
    }
    assert_eq!(Date(first.0 + 364 * 24).to_string(), "1450.12.31");
    for bad in ["1444.2.29", "1444.13.1", "1444.0.1", "1444.1.0", "1444.11", "x.1.1", "-5001.1.1"] {
        assert_eq!(Date::parse(bad), None, "{bad}");
    }
    assert_eq!(serde_json::to_value(start).unwrap(), json!("1444.11.11"));
}

#[test]
fn responses_in_json() {
    let ok: Response = Ok(Answer::Country(countries::CountryAnswer::Hostile(true)));
    assert_eq!(response_json(&ok), json!({"ok": true, "result": true}));
    let moved: Response = Ok(Answer::Army(armies::ArmyAnswer::Move(())));
    assert_eq!(response_json(&moved), json!({"ok": true, "result": null}));
    let failed: Response = Err(Error::new(Code::NotInGame, "no game is running"));
    assert_eq!(response_json(&failed), json!({"ok": false, "error": {"code": "not_in_game", "message": "no game is running"}}));
    assert_eq!(failed.unwrap_err().to_string(), "not_in_game: no game is running");

    let events = events::Events {
        events: vec![
            events::Numbered { number: 7, event: events::Event::ArmyOrdered { army: ArmyId(12) } },
            events::Numbered { number: 8, event: events::Event::Day { date: Date::from_ymd(1444, 11, 12).unwrap() } },
        ],
        last: 8,
        missed: false,
    };
    assert_eq!(
        serde_json::to_value(&events).unwrap(),
        json!({"events": [{"number": 7, "event": "army_ordered", "army": "12"}, {"number": 8, "event": "day", "date": "1444.11.12"}], "last": 8, "missed": false})
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
    let all = [Kind::Country, Kind::Province, Kind::Army, Kind::Date, Kind::Integer, Kind::Boolean, Kind::Text];
    for kind in all {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.name()));
    }
    assert_eq!(sample(Kind::Province), json!(151));
    assert_eq!(sample(Kind::Army), json!("4294967377"));
    assert_eq!(Kind::Integer.json("-3"), Some(json!(-3)));
    assert_eq!(Kind::Integer.json("18446744073709551615"), Some(json!(u64::MAX)));
    assert_eq!(Kind::Boolean.json("false"), Some(json!(false)));
    for (kind, bad) in [(Kind::Province, "Paris"), (Kind::Province, "1.5"), (Kind::Integer, "0x10"), (Kind::Integer, ""), (Kind::Boolean, "yes")] {
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
    for other in [json!("iVBORw=="), json!({"base64": "iVBORw=="}), json!({"media_type": "image/png", "base64": "iVBORw==", "width": 1}), json!({"media_type": "image/png", "base64": "not base64"})] {
        assert_eq!(Binary::from_json(&other), None, "{other}");
    }
}
