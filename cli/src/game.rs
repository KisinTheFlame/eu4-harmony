//! `eu4-harmony game`: a remote client on the command line (ADR 0016). Each
//! call sends the running game one request through remote control's pipe and
//! prints the answer as JSON.
//!
//! The command line knows the capabilities only from their descriptions in
//! the shared crate (`eu4_harmony_remote::capabilities`): a capability's
//! name, its arguments and their kinds make the request, and each kind says
//! how its value is written in JSON. A capability added to the crate so
//! appears here with no change to this file.
use crate::pipe::{self, Failure};
use eu4_harmony_remote::{capabilities, prose, Binary, Capability, Kind, Parameter, Request};
use serde_json::{Map, Value};
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// The game answered, and not with an error.
pub const EXIT_OK: i32 = 0;
/// The game answered with an error, which is printed.
pub const EXIT_ERROR: i32 = 1;
/// The command line is wrong; nothing was sent.
pub const EXIT_USAGE: i32 = 2;
/// The command could not finish: no answer came (the pipe did not open in
/// time, is not the game's, broke, or the time ran out), the line that came
/// is no answer (not JSON, or not of an answer's shape), or the bytes in it
/// could not be written.
pub const EXIT_NO_ANSWER: i32 = 3;

/// How long to wait for the game to open its pipe without `--timeout`.
const PIPE_WAIT: Duration = Duration::from_secs(10);

/// The command line's own options, which no capability's argument may be
/// named.
const OPTIONS: [&str; 4] = ["timeout", "out", "compact", "help"];

/// Runs `eu4-harmony game` with the arguments after `game`; returns the exit
/// code.
pub fn run(raw: &[String]) -> i32 {
    match parse(raw) {
        Err(e) => {
            eprintln!("error: {e}");
            EXIT_USAGE
        }
        Ok(Command::Help(None)) => {
            print!("{}", usage());
            EXIT_OK
        }
        Ok(Command::Help(Some(capability))) => {
            print!("{}", capability_help(&capability));
            EXIT_OK
        }
        Ok(Command::Ask(ask)) => {
            let outcome = ask.run(pipe::NAME, pipe::GAME);
            if let Some(text) = &outcome.stdout {
                println!("{text}");
            }
            if let Some(text) = &outcome.stderr {
                eprintln!("{text}");
            }
            outcome.code
        }
    }
}

/// What the command line asks for.
#[derive(Debug)]
enum Command {
    /// The capabilities, or one of them, described.
    Help(Option<Capability>),
    Ask(Ask),
}

/// A request to send, and how.
#[derive(Debug)]
struct Ask {
    request: Request,
    /// The longest the whole call may take.
    timeout: Option<Duration>,
    /// Where bytes in the answer go.
    out: Option<PathBuf>,
    /// The JSON on one line.
    compact: bool,
}

/// What a call prints, and its exit code.
#[derive(Debug, PartialEq)]
struct Outcome {
    code: i32,
    stdout: Option<String>,
    stderr: Option<String>,
}

impl Outcome {
    fn failed(code: i32, message: String) -> Self {
        Self { code, stdout: None, stderr: Some(format!("error: {message}")) }
    }
}

// ---------- the command line ----------

/// The command `raw` writes: the command line's own options wherever they
/// are, then the capability's name and its arguments. A `--` ends the
/// options: every word after it is a value, even one that starts with `--`.
fn parse(raw: &[String]) -> Result<Command, String> {
    let (mut timeout, mut out, mut compact, mut help) = (None, None, false, false);
    let mut words = Vec::new();
    let mut values: &[String] = &[];
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == "--" {
            values = &raw[i + 1..];
            break;
        }
        let option = raw[i].strip_prefix("--").map(|key| key.replace('-', "_")).filter(|key| OPTIONS.contains(&key.as_str()));
        match option.as_deref() {
            Some("timeout") => timeout = Some(seconds(option_value(raw, &mut i)?)?),
            Some("out") => out = Some(PathBuf::from(option_value(raw, &mut i)?)),
            Some("compact") => compact = true,
            Some(_) => help = true,
            None if raw[i] == "-h" => help = true,
            None => words.push(raw[i].clone()),
        }
        i += 1;
    }
    match words.first().map(String::as_str) {
        None if !values.is_empty() => Err("name the capability before --: eu4-harmony game <capability> -- <value>...".into()),
        None => Ok(Command::Help(None)),
        Some("help") if words.len() == 1 => Ok(Command::Help(None)),
        Some("help") => Ok(Command::Help(Some(name(&words[1..])?.0))),
        Some(_) => {
            let (capability, used) = name(&words)?;
            if help {
                return Ok(Command::Help(Some(capability)));
            }
            let request = request(&capability, &words[used..], values)?;
            Ok(Command::Ask(Ask { request, timeout, out, compact }))
        }
    }
}

/// The value after the option at `i`, which `i` moves on to.
fn option_value<'a>(raw: &'a [String], i: &mut usize) -> Result<&'a str, String> {
    let option = &raw[*i];
    *i += 1;
    raw.get(*i).map(String::as_str).ok_or_else(|| format!("{option} needs a value"))
}

/// A time in seconds, such as `30` or `0.5`.
fn seconds(text: &str) -> Result<Duration, String> {
    text.parse::<f64>().ok().filter(|s| s.is_finite() && *s > 0.0).and_then(|s| Duration::try_from_secs_f64(s).ok()).ok_or_else(|| format!("--timeout takes a number of seconds above 0, such as 30; {text:?} is not one"))
}

/// The capability the first words name, `army.move` or `army move`, and how
/// many words that took.
fn name(words: &[String]) -> Result<(Capability, usize), String> {
    let all = capabilities();
    let find = |name: &str| all.iter().find(|c| c.name() == name).copied();
    let first = &words[0];
    if first.starts_with('-') {
        return Err(format!("{first}: name the capability before its arguments, such as eu4-harmony game army.get --army <army>"));
    }
    if let Some(second) = words.get(1).filter(|w| !w.starts_with('-')) {
        if let Some(capability) = find(&format!("{first}.{second}")) {
            return Ok((capability, 2));
        }
    }
    if let Some(capability) = find(first) {
        return Ok((capability, 1));
    }
    let group = first.split('.').next().unwrap_or_default();
    let in_group: Vec<String> = all.iter().filter(|c| c.group == group).map(Capability::name).collect();
    if in_group.is_empty() {
        return Err(format!("no capability {first}; eu4-harmony game help lists them"));
    }
    // A group named, and a word after it that names nothing in it.
    let asked = match words.get(1) {
        Some(second) if !first.contains('.') && !second.starts_with('-') => format!("{first}.{second}"),
        _ => first.clone(),
    };
    Err(format!("no capability {asked}; the {group} group has {}", in_group.join(", ")))
}

/// The request for `capability` from the words after its name and the
/// values after `--`.
fn request(capability: &Capability, words: &[String], values: &[String]) -> Result<Request, String> {
    let fields = arguments(capability, words, values)?;
    let mut object = Map::new();
    object.insert("capability".into(), Value::String(capability.name()));
    object.extend(fields);
    serde_json::from_value(Value::Object(object)).map_err(|e| format!("{}: {e}", capability.name()))
}

/// The arguments `words` give `capability`, in JSON: `--name value`, the
/// name's `_` also written `-`, or values alone, which fill the arguments
/// not named, in order, the last of them, when it is a text, taking every
/// value left; `values`, the words after `--`, are values alone whatever
/// they look like. A boolean standing alone is true.
fn arguments(capability: &Capability, words: &[String], values: &[String]) -> Result<Map<String, Value>, String> {
    let name = capability.name();
    let mut fields = Map::new();
    let mut alone = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let Some(key) = words[i].strip_prefix("--") else {
            alone.push(&words[i]);
            i += 1;
            continue;
        };
        let key = key.replace('-', "_");
        let Some(arg) = capability.args.iter().find(|a| a.name == key) else {
            return Err(format!("{name} has no argument --{key}; {}", its_arguments(capability)));
        };
        if fields.contains_key(arg.name) {
            return Err(format!("--{} is given twice", arg.name));
        }
        let next = words.get(i + 1).filter(|w| !w.starts_with("--"));
        let text = match next {
            Some(next) if arg.kind != Kind::Boolean || Kind::Boolean.json(next).is_some() => {
                i += 1;
                next.as_str()
            }
            _ if arg.kind == Kind::Boolean => "true",
            _ => return Err(format!("--{} needs a value, a {} such as {}", arg.name, arg.kind.name(), arg.kind.example())),
        };
        fields.insert(arg.name.into(), value(arg, text)?);
        i += 1;
    }
    alone.extend(values);
    let unnamed: Vec<&Parameter> = capability.args.iter().filter(|a| !fields.contains_key(a.name)).collect();
    // The last argument not named, a text, takes every value left, a space
    // between each: `console run cash 100` runs `cash 100`.
    let rest = match unnamed.last() {
        Some(last) if last.kind == Kind::Text && alone.len() > unnamed.len() => {
            let rest: Vec<&str> = alone.split_off(unnamed.len() - 1).into_iter().map(String::as_str).collect();
            Some((*last, rest.join(" ")))
        }
        _ => None,
    };
    if alone.len() > unnamed.len() {
        return Err(format!("{:?} is one value too many: {}", alone[unnamed.len()], its_arguments(capability)));
    }
    for (arg, text) in unnamed.iter().zip(alone) {
        fields.insert(arg.name.into(), value(arg, text)?);
    }
    if let Some((arg, text)) = rest {
        fields.insert(arg.name.into(), value(arg, &text)?);
    }
    if let Some(missing) = capability.args.iter().find(|a| !a.optional && !fields.contains_key(a.name)) {
        return Err(format!("{name} needs --{} <{}>: {}", missing.name, missing.kind.name(), prose(missing.about)));
    }
    Ok(fields)
}

/// An argument's value from its text.
fn value(arg: &Parameter, text: &str) -> Result<Value, String> {
    arg.kind.json(text).ok_or_else(|| format!("--{} is a {}, such as {}; {text:?} is not one", arg.name, arg.kind.name(), arg.kind.example()))
}

fn its_arguments(capability: &Capability) -> String {
    if capability.args.is_empty() {
        format!("{} takes no arguments", capability.name())
    } else {
        format!("its arguments: {}", signature(capability))
    }
}

// ---------- help ----------

/// The arguments as written on the command line, the optional ones in
/// brackets.
fn signature(capability: &Capability) -> String {
    let args: Vec<String> = capability
        .args
        .iter()
        .map(|arg| {
            let placeholder = if arg.kind == Kind::Boolean { format!("[<{}>]", arg.kind.name()) } else { format!("<{}>", arg.kind.name()) };
            let written = format!("--{} {placeholder}", arg.name);
            if arg.optional {
                format!("[{written}]")
            } else {
                written
            }
        })
        .collect();
    args.join(" ")
}

/// The first sentence of a description.
fn first_sentence(about: &str) -> String {
    let text = prose(about);
    match text.find(". ") {
        Some(end) => text[..=end].to_string(),
        None => text,
    }
}

fn usage() -> String {
    let mut text = format!(
        "\
eu4-harmony game: use the running game through remote control

usage:
  eu4-harmony game <capability> [--<argument> <value>]... [--timeout <seconds>] [--out <file>] [--compact] [-- <value>...]
      Sends the game one request and prints the result as JSON. Values may
      also come without their names, in the order of the arguments, and a
      name's dot may be a space: eu4-harmony game army move <army> 151. The
      last argument, when it is a text, takes every value left, a space
      between each: eu4-harmony game console run cash 100.
  eu4-harmony game help [<capability>]
      Describes the capabilities this tool knows, or one of them;
      eu4-harmony game describe asks the running game instead.

options:
  --timeout <seconds>  Gives up after this long in all, waiting for the game to
                       open its pipe included; a request given up before it ran
                       never runs. Without it the pipe is waited for {} s, and
                       the answer as long as it takes.
  --out <file>         Writes bytes in the answer, such as a screenshot, to the
                       file, more of them to its name numbered before the
                       extension (shot.png, then shot-2.png, shot-3.png...),
                       and prints the files in their place.
  --compact            Prints the JSON on one line.
  --                   Ends the options: every word after it is a value, even
                       one that starts with --.

exit codes:
  0  the game answered; the result is printed
  1  the game answered with an error, printed as {{\"code\": ..., \"message\": ...}}
  2  the command line is wrong; nothing was sent
  3  no answer: the game's pipe did not open in time, is not served by the
     game run by this Windows user, broke, or the time ran out; the line the
     game sent is no answer (not JSON, or not of an answer's shape); or the
     bytes in the answer could not be written

capabilities (* only while a game is going on):
",
        PIPE_WAIT.as_secs()
    );
    for capability in capabilities() {
        let star = if capability.in_game { " *" } else { "" };
        let signature = signature(&capability);
        let space = if signature.is_empty() { "" } else { " " };
        let _ = writeln!(text, "  {}{space}{signature}{star}", capability.name());
        let _ = writeln!(text, "      {}", first_sentence(capability.about));
    }
    text
}

fn capability_help(capability: &Capability) -> String {
    let mut text = String::new();
    let signature = signature(capability);
    let space = if signature.is_empty() { "" } else { " " };
    let _ = writeln!(text, "eu4-harmony game {}{space}{signature}\n", capability.name());
    let _ = writeln!(text, "{}", prose(capability.about));
    let _ = writeln!(text, "{}", if capability.in_game { "Only while a game is going on." } else { "In the main menu too." });
    if !capability.args.is_empty() {
        let _ = writeln!(text, "\narguments:");
        for arg in capability.args {
            let optional = if arg.optional { ", optional" } else { "" };
            let _ = writeln!(text, "  --{} <{}{optional}>: {}", arg.name, arg.kind.name(), prose(arg.about));
        }
    }
    let _ = writeln!(text, "\nresult: {}", capability.result);
    text
}

// ---------- asking ----------

impl Ask {
    /// Sends the request down the pipe `name` served by the `game`, and
    /// says what to print.
    fn run(&self, name: &str, game: &str) -> Outcome {
        let started = Instant::now();
        let line = serde_json::to_string(&self.request).expect("a request is JSON");
        let answer = pipe::connect(name, game, started + self.timeout.unwrap_or(PIPE_WAIT)).and_then(|mut client| client.ask(&line, self.timeout.map(|t| started + t)));
        match answer {
            Ok(answer) => self.output(&answer),
            Err(failure) => Outcome::failed(EXIT_NO_ANSWER, self.explain(&failure)),
        }
    }

    /// A failure, with the time waited where that is what ran out.
    fn explain(&self, failure: &Failure) -> String {
        let waited = self.timeout.unwrap_or(PIPE_WAIT);
        match failure {
            Failure::NotOpen | Failure::TimedOut => format!("{failure} ({} s)", waited.as_secs_f64()),
            _ => failure.to_string(),
        }
    }

    /// What to print for the game's answer, a line of JSON.
    fn output(&self, answer: &str) -> Outcome {
        let response: Value = match serde_json::from_str(answer) {
            Ok(response) => response,
            Err(e) => return Outcome::failed(EXIT_NO_ANSWER, format!("the game's answer is not JSON ({e}): {answer}")),
        };
        match (response.get("ok").and_then(Value::as_bool), response.get("result"), response.get("error")) {
            (Some(true), Some(result), _) => {
                let mut result = result.clone();
                let mut stderr = None;
                if let Some(out) = &self.out {
                    match write_binaries(&mut result, out) {
                        Ok(0) => stderr = Some(format!("note: the answer holds no bytes; nothing was written to {}", out.display())),
                        Ok(_) => {}
                        Err(e) => return Outcome::failed(EXIT_NO_ANSWER, e),
                    }
                }
                Outcome { code: EXIT_OK, stdout: Some(self.json(&result)), stderr }
            }
            (Some(false), _, Some(error)) => Outcome { code: EXIT_ERROR, stdout: Some(self.json(error)), stderr: None },
            _ => Outcome::failed(EXIT_NO_ANSWER, format!("the game's answer is not one: {answer}")),
        }
    }

    fn json(&self, value: &Value) -> String {
        let text = if self.compact { serde_json::to_string(value) } else { serde_json::to_string_pretty(value) };
        text.expect("JSON is JSON")
    }
}

/// Writes every binary in `value` to a file, the first to `out`, the ones
/// after it to `out` numbered (`shot-2.png`), and puts in its place its
/// media type, its file and its size. Returns how many were written.
fn write_binaries(value: &mut Value, out: &Path) -> Result<usize, String> {
    let mut written = 0;
    replace_binaries(value, &mut |binary| {
        written += 1;
        let file = numbered(out, written);
        fs::write(&file, &binary.bytes).map_err(|e| format!("the answer's bytes cannot be written to {}: {e}", file.display()))?;
        let file = std::path::absolute(&file).unwrap_or(file);
        Ok(serde_json::json!({ "media_type": binary.media_type, "file": file.display().to_string(), "bytes": binary.bytes.len() }))
    })?;
    Ok(written)
}

fn replace_binaries(value: &mut Value, with: &mut impl FnMut(Binary) -> Result<Value, String>) -> Result<(), String> {
    if let Some(binary) = Binary::from_json(value) {
        *value = with(binary)?;
        return Ok(());
    }
    match value {
        Value::Array(items) => items.iter_mut().try_for_each(|item| replace_binaries(item, with)),
        Value::Object(fields) => fields.values_mut().try_for_each(|field| replace_binaries(field, with)),
        _ => Ok(()),
    }
}

/// `out` for the first file, then `out` with `-2`, `-3`... before its
/// extension.
fn numbered(out: &Path, number: usize) -> PathBuf {
    if number == 1 {
        return out.to_path_buf();
    }
    let stem = out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = match out.extension() {
        Some(extension) => format!("{stem}-{number}.{}", extension.to_string_lossy()),
        None => format!("{stem}-{number}"),
    };
    out.with_file_name(name)
}

#[cfg(test)]
mod tests;
