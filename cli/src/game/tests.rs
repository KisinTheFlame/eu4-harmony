use super::*;
use crate::pipe::tests::{answer, pipe_name, serve_one, this_program};
use eu4_harmony_remote::{armies::ArmyRequest, events::EventsRequest, session::SessionRequest, ArmyId, ProvinceId};
use serde_json::json;
use std::io::Read as _;

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(String::from).collect()
}

fn ask(text: &str) -> Ask {
    match parse(&words(text)) {
        Ok(Command::Ask(ask)) => ask,
        other => panic!("{text}: {other:?}"),
    }
}

fn help(text: &str) -> Option<String> {
    match parse(&words(text)) {
        Ok(Command::Help(capability)) => capability.map(|c| c.name()),
        other => panic!("{text}: {other:?}"),
    }
}

fn wrong(text: &str) -> String {
    match parse(&words(text)) {
        Err(e) => e,
        Ok(other) => panic!("{text} was taken as {other:?}"),
    }
}

/// An ask for the session, printing as `out` and `compact` say.
fn session(out: Option<PathBuf>, compact: bool) -> Ask {
    Ask { request: Request::Session(SessionRequest::Get {}), timeout: None, out, compact }
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eu4h-cli-game-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn arguments_by_name_or_in_order() {
    let moved = Request::Army(ArmyRequest::Move { army: ArmyId(4294967377), goal: ProvinceId(151) });
    for text in ["army.move --army 4294967377 --goal 151", "army move --goal 151 --army 4294967377", "army move 4294967377 151", "army.move --goal 151 4294967377"] {
        assert_eq!(ask(text).request, moved, "{text}");
    }
    let read = |since, wait_ms| Request::Events(EventsRequest::Read { since, wait_ms, process: None });
    assert_eq!(ask("events").request, read(None, None));
    assert_eq!(ask("events 3").request, read(Some(3), None));
    assert_eq!(ask("events --since 3 --wait-ms 500").request, read(Some(3), Some(500)));
    assert_eq!(ask("events --wait_ms 500").request, read(None, Some(500)));
    assert_eq!(ask("session").request, Request::Session(SessionRequest::Get {}));
}

#[test]
fn the_command_lines_own_options_go_anywhere() {
    let a = ask("--timeout 2.5 session --compact");
    assert_eq!((a.timeout, a.compact, a.out), (Some(Duration::from_millis(2500)), true, None));
    let a = ask("army get --out shot.png 4294967377");
    assert_eq!((a.timeout, a.compact, a.out), (None, false, Some(PathBuf::from("shot.png"))));
    assert_eq!(a.request, Request::Army(ArmyRequest::Get { army: ArmyId(4294967377) }));
}

#[test]
fn help_from_the_descriptions() {
    for text in ["", "help", "--help", "-h"] {
        assert_eq!(help(text), None, "{text}");
    }
    for text in ["help army move", "help army.move", "army.move --help", "army move -h"] {
        assert_eq!(help(text).as_deref(), Some("army.move"), "{text}");
    }
    let usage = usage();
    for capability in capabilities() {
        assert!(usage.contains(&format!("\n  {}", capability.name())), "{} is not listed", capability.name());
        let text = capability_help(&capability);
        assert!(text.contains(capability.result) && text.contains(&prose(capability.about)), "{text}");
        for arg in capability.args {
            assert!(text.contains(&format!("--{} <{}", arg.name, arg.kind.name())), "{text}");
        }
    }
    assert!(usage.contains("  army.move --army <army> --goal <province> *\n      Orders an army of the player's along the game's route, as the player would.\n"));
    assert!(usage.contains("  events [--since <integer>] [--wait_ms <integer>] [--process <integer>]\n"));
}

#[test]
fn a_wrong_command_line_says_why() {
    for (text, says) in [
        ("fly", "no capability fly; eu4-harmony game help lists them"),
        ("army fly", "no capability army.fly; the army group has army.get, army.list"),
        ("army.fly --army 1", "no capability army.fly"),
        ("--army 1 army.get", "name the capability before its arguments"),
        ("army.move 1", "army.move needs --goal <province>: Where it should go."),
        ("army.move 1 2 3", "\"3\" is one value too many: its arguments: --army <army> --goal <province>"),
        ("army.move --army 1 --army 2 --goal 3", "--army is given twice"),
        ("army.move --army 1 --speed 3", "army.move has no argument --speed"),
        ("army.move --army 1 --goal", "--goal needs a value, a province such as 151"),
        ("army.move --army 1 --goal Paris", "--goal is a province, such as 151; \"Paris\" is not one"),
        ("army.get x", "\"x\" is not an army ID"),
        ("country.hostile FRANCE ENG", "\"FRANCE\" is not a country tag"),
        ("events --since -1", "events: invalid value: integer `-1`"),
        ("session 1", "session takes no arguments"),
        ("session --timeout 0", "--timeout takes a number of seconds above 0"),
        ("session --timeout soon", "--timeout takes a number of seconds"),
        ("session --timeout", "--timeout needs a value"),
        ("session --out", "--out needs a value"),
    ] {
        let error = wrong(text);
        assert!(error.contains(says), "{text}: {error}");
    }
}

/// A capability of the tests' own, with a boolean.
const FLAGS: Capability = Capability {
    group: "test",
    action: "flags",
    about: " Flags.",
    in_game: false,
    args: &[
        Parameter { name: "paused", kind: Kind::Boolean, optional: true, about: " Paused." },
        Parameter { name: "province", kind: Kind::Province, optional: true, about: " A province." },
    ],
    result: "nothing",
};

#[test]
fn a_boolean_alone_is_true() {
    let fields = |text: &str| Value::Object(arguments(&FLAGS, &words(text), &[]).unwrap());
    assert_eq!(fields("--paused"), json!({"paused": true}));
    assert_eq!(fields("--paused false"), json!({"paused": false}));
    assert_eq!(fields("--paused --province 5"), json!({"paused": true, "province": 5}));
    assert_eq!(fields("--paused 5"), json!({"paused": true, "province": 5}));
    assert_eq!(fields("true 5"), json!({"paused": true, "province": 5}));
    assert_eq!(fields(""), json!({}));
    assert!(arguments(&FLAGS, &words("yes"), &[]).unwrap_err().contains("--paused is a boolean, such as true; \"yes\" is not one"));
    assert_eq!(signature(&FLAGS), "[--paused [<boolean>]] [--province <province>]");
}

/// A capability of the tests' own, with a text.
const SAY: Capability = Capability {
    group: "test",
    action: "say",
    about: " Says a text.",
    in_game: false,
    args: &[
        Parameter { name: "text", kind: Kind::Text, optional: false, about: " The text." },
        Parameter { name: "times", kind: Kind::Integer, optional: true, about: " How often." },
    ],
    result: "nothing",
};

#[test]
fn after_a_double_dash_every_word_is_a_value() {
    let said = |text: &str| {
        let words = words(text);
        let at = words.iter().position(|w| w == "--").unwrap_or(words.len());
        Value::Object(arguments(&SAY, &words[..at], words.get(at + 1..).unwrap_or_default()).unwrap())
    };
    // Words the command line would take as its own options, or as names.
    for text in ["--out", "--timeout", "--compact", "--help", "-h", "--times", "--", "--text"] {
        assert_eq!(said(&format!("-- {text}")), json!({"text": text}), "{text}");
    }
    assert_eq!(said("--times 2 -- --help"), json!({"text": "--help", "times": 2}));
    assert_eq!(said("-- --out 2"), json!({"text": "--out", "times": 2}));
    assert!(arguments(&SAY, &[], &words("a 2 3")).unwrap_err().contains("\"3\" is one value too many"));

    // The command line's own options before it still count, and nothing after it does.
    let a = ask("--compact army move --timeout 2 -- 4294967377 151");
    assert_eq!(a.request, Request::Army(ArmyRequest::Move { army: ArmyId(4294967377), goal: ProvinceId(151) }));
    assert_eq!((a.compact, a.timeout), (true, Some(Duration::from_secs(2))));
    assert_eq!(ask("events -- 3").request, Request::Events(EventsRequest::Read { since: Some(3), wait_ms: None, process: None }));
    assert!(wrong("session -- --compact").contains("session takes no arguments"));
    assert!(wrong("events -- --help").contains("--since is a integer, such as 3; \"--help\" is not one"));
    assert!(wrong("-- session").contains("name the capability before --"));
    assert_eq!(help("--help army move --"), Some("army.move".into()));
}

#[test]
fn a_text_last_takes_every_value_left() {
    use eu4_harmony_remote::{console::ConsoleRequest, menu::MenuRequest};
    let run = |command: &str| Request::Console(ConsoleRequest::Run { command: command.into() });
    assert_eq!(ask("console run cash 100").request, run("cash 100"));
    assert_eq!(ask("console.run cash 100 FRA").request, run("cash 100 FRA"));
    assert_eq!(ask("console run -- cash 100").request, run("cash 100"));
    assert_eq!(ask("console run --compact -- help --all").request, run("help --all"));
    assert_eq!(ask("menu load Castilla 5").request, Request::Menu(MenuRequest::Load { save: "Castilla 5".into() }));
    // Once named, a text takes one value, as any argument does.
    assert!(wrong("console run --command cash 100").contains("\"100\" is one value too many"));
    // The last argument not named, when it is a text: --times named, the
    // values left all go to the text.
    assert_eq!(Value::Object(arguments(&SAY, &words("--times 2 a b c"), &[]).unwrap()), json!({"text": "a b c", "times": 2}));
}

/// Every capability, and so every one added later, can be called from the
/// command line, as its description says.
#[test]
fn every_capability_can_be_called_from_the_command_line() {
    for capability in capabilities() {
        let name = capability.name();
        let named: Vec<String> = capability.args.iter().map(|a| format!("--{} {}", a.name, a.kind.example())).collect();
        let alone: Vec<&str> = capability.args.iter().map(|a| a.kind.example()).collect();
        let spaced = if capability.action.is_empty() { name.clone() } else { format!("{} {}", capability.group, capability.action) };
        let wanted = ask(&format!("{name} {}", named.join(" "))).request;
        assert_eq!(wanted.name(), name);
        assert_eq!(ask(&format!("{spaced} {}", alone.join(" "))).request, wanted, "{name}");
        let required: Vec<String> = capability.args.iter().filter(|a| !a.optional).map(|a| format!("--{} {}", a.name, a.kind.example())).collect();
        assert_eq!(ask(&format!("{name} {}", required.join(" "))).request.name(), name);
        for arg in capability.args {
            assert!(!OPTIONS.contains(&arg.name), "{name}'s argument --{} is one of the command line's own options", arg.name);
        }
    }
}

#[test]
fn prints_the_result_or_the_error() {
    let pretty = session(None, false).output(r#"{"ok":true,"result":{"session":1,"player":"TUR"}}"#);
    assert_eq!(pretty, Outcome { code: EXIT_OK, stdout: Some("{\n  \"player\": \"TUR\",\n  \"session\": 1\n}".into()), stderr: None });
    let compact = session(None, true).output(r#"{"ok":true,"result":[1,2]}"#);
    assert_eq!(compact.stdout.as_deref(), Some("[1,2]"));
    let moved = session(None, true).output(r#"{"ok":true,"result":null}"#);
    assert_eq!((moved.code, moved.stdout.as_deref()), (EXIT_OK, Some("null")));
    let failed = session(None, true).output(r#"{"error":{"code":"not_in_game","message":"the main menu is open"},"ok":false}"#);
    assert_eq!(failed, Outcome { code: EXIT_ERROR, stdout: Some(r#"{"code":"not_in_game","message":"the main menu is open"}"#.into()), stderr: None });
    for bad in ["not json", r#"{"ok":true}"#, r#"{"ok":false}"#, "[]"] {
        let outcome = session(None, false).output(bad);
        assert_eq!((outcome.code, outcome.stdout), (EXIT_NO_ANSWER, None), "{bad}");
        assert!(outcome.stderr.unwrap().starts_with("error: the game's answer is not"), "{bad}");
    }
}

#[test]
fn bytes_in_the_answer_go_to_the_file_out_names() {
    let dir = temp("out");
    let out = dir.join("shot.png");
    let answer = json!({"ok": true, "result": {
        "image": {"media_type": "image/png", "base64": "iVBORw=="},
        "thumbnails": [{"media_type": "image/png", "base64": "AAEC"}],
        "width": 1,
    }});
    let outcome = session(Some(out.clone()), true).output(&answer.to_string());
    assert_eq!((outcome.code, &outcome.stderr), (EXIT_OK, &None));
    assert_eq!(fs::read(&out).unwrap(), [0x89, b'P', b'N', b'G']);
    let second = dir.join("shot-2.png");
    assert_eq!(fs::read(&second).unwrap(), [0, 1, 2]);
    let printed: Value = serde_json::from_str(outcome.stdout.as_deref().unwrap()).unwrap();
    assert_eq!(
        printed,
        json!({
            "image": {"media_type": "image/png", "file": std::path::absolute(&out).unwrap().display().to_string(), "bytes": 4},
            "thumbnails": [{"media_type": "image/png", "file": std::path::absolute(&second).unwrap().display().to_string(), "bytes": 3}],
            "width": 1,
        })
    );

    // A result that is bytes alone.
    let alone = dir.join("alone");
    let outcome = session(Some(alone.clone()), true).output(r#"{"ok":true,"result":{"media_type":"image/png","base64":"AAEC"}}"#);
    assert_eq!(outcome.code, EXIT_OK);
    assert_eq!(fs::read(&alone).unwrap(), [0, 1, 2]);

    // Without --out the bytes are printed as they came.
    let outcome = session(None, true).output(&answer.to_string());
    assert_eq!(serde_json::from_str::<Value>(&outcome.stdout.unwrap()).unwrap(), answer["result"]);

    // No bytes: nothing written, and a note.
    let none = dir.join("none.png");
    let outcome = session(Some(none.clone()), true).output(r#"{"ok":true,"result":{"session":1}}"#);
    assert_eq!((outcome.code, outcome.stdout.as_deref()), (EXIT_OK, Some(r#"{"session":1}"#)));
    assert!(outcome.stderr.unwrap().contains("the answer holds no bytes; nothing was written to"));
    assert!(!none.exists());

    // Bytes that cannot be written.
    let outcome = session(Some(dir.join("no such folder").join("shot.png")), true).output(&answer.to_string());
    assert_eq!((outcome.code, outcome.stdout), (EXIT_NO_ANSWER, None));
    assert!(outcome.stderr.unwrap().contains("the answer's bytes cannot be written to"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn files_after_the_first_are_numbered() {
    assert_eq!(numbered(Path::new("shots/a.png"), 1), PathBuf::from("shots/a.png"));
    assert_eq!(numbered(Path::new("shots/a.png"), 2), PathBuf::from("shots/a-2.png"));
    assert_eq!(numbered(Path::new("a"), 3), PathBuf::from("a-3"));
}

#[test]
fn the_usage_names_the_files_as_they_are_written() {
    let files: Vec<String> = (1..=3).map(|n| numbered(Path::new("shot.png"), n).display().to_string()).collect();
    assert!(usage().contains(&format!("({}, then {}, {}...)", files[0], files[1], files[2])), "{}", usage());
}

#[test]
fn asks_the_game_down_its_pipe() {
    let name = pipe_name("game-asks");
    let served = serve_one(&name, |pipe, line| {
        assert_eq!(line, "{\"capability\":\"army.move\",\"army\":\"4294967377\",\"goal\":151}\n");
        answer(pipe, "{\"ok\":true,\"result\":null}\n");
    });
    let outcome = ask("army move 4294967377 151 --compact").run(&name, &this_program());
    assert_eq!(outcome, Outcome { code: EXIT_OK, stdout: Some("null".into()), stderr: None });
    served.join().unwrap();
}

#[test]
fn no_answer_in_time_says_how_long_it_waited() {
    // No pipe at all.
    let outcome = ask("session --timeout 0.3").run(&pipe_name("game-none"), &this_program());
    assert_eq!((outcome.code, outcome.stdout), (EXIT_NO_ANSWER, None));
    let error = outcome.stderr.unwrap();
    assert!(error.starts_with("error: no game with EU4 Harmony opened") && error.ends_with("(0.3 s)"), "{error}");

    // A pipe served by something that does not answer.
    let name = pipe_name("game-silent");
    let served = serve_one(&name, |pipe, _| {
        let mut rest = Vec::new();
        let _ = { pipe }.read_to_end(&mut rest);
    });
    let started = Instant::now();
    let outcome = ask("events --wait-ms 60000 --timeout 0.3").run(&name, &this_program());
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(outcome.code, EXIT_NO_ANSWER);
    let error = outcome.stderr.unwrap();
    assert!(error.contains("no answer in time; the request was given up") && error.ends_with("(0.3 s)"), "{error}");
    served.join().unwrap();

    // A pipe served by something other than the game.
    let name = pipe_name("game-other");
    let served = serve_one(&name, |_, line| assert_eq!(line, ""));
    let outcome = ask("session").run(&name, pipe::GAME);
    assert_eq!(outcome.code, EXIT_NO_ANSWER);
    assert!(outcome.stderr.unwrap().contains("nothing was sent"));
    served.join().unwrap();
}
