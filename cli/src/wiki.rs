//! `eu4-harmony wiki`: the reference, made from the framework's own
//! descriptions (`eu4_harmony_remote`), never written by hand
//! (ADR one-capability-catalogue). Topics nest: `wiki`, `wiki army`, `wiki army
//! move`.
use eu4_harmony_remote::{capabilities, groups, lua, prose, signals::SIGNALS, Capability, Code, Group, API_VERSION};
use std::fmt::Write as _;

/// The page for `topics`, or why there is none.
pub fn page(topics: &[String]) -> Result<String, String> {
    let words: Vec<&str> = topics.iter().flat_map(|t| t.split('.')).filter(|w| !w.is_empty()).collect();
    match words.as_slice() {
        [] => Ok(index()),
        ["remote"] => Ok(remote()),
        ["lua"] => Ok(lua_page()),
        ["manifest"] => Ok(manifest_page()),
        ["errors"] => Ok(errors()),
        ["signals"] => Ok(signals()),
        [group] => match groups().into_iter().find(|g| g.name == *group) {
            Some(g) => Ok(group_page(&g)),
            None => Err(format!("no topic {group}; eu4-harmony wiki lists them")),
        },
        [group, rest @ ..] => {
            let name = format!("{group}.{}", rest.join("."));
            match capabilities().into_iter().find(|c| c.name() == name) {
                Some(c) => Ok(capability_page(&c)),
                None => Err(format!("no capability {name}; eu4-harmony wiki {group} lists the group's")),
            }
        }
    }
}

fn index() -> String {
    let mut text = format!(
        "\
eu4-harmony wiki: the reference for capability catalogue {API_VERSION}

usage: eu4-harmony wiki [<topic>...]

topics:
  lua       what a mod's Lua has: handlers, globals, the sandbox
  manifest  eu4-harmony/manifest.toml, and the permissions it declares
  remote    how a remote client plays a game, and the command line's exit codes
  errors    the codes a failed capability answers
  signals    what extensions hear and remote control numbers, and reading them

capability groups (eu4-harmony wiki <group>, then <group> <capability>):
"
    );
    let width = groups().iter().map(|g| g.name.len()).max().unwrap_or(0) + 2;
    for g in groups() {
        let _ = writeln!(text, "  {:<width$}{}", g.name, prose(g.about));
    }
    text
}

/// A group's capabilities; a capability the group's name alone names (such
/// as `screenshot`) has its whole page first.
fn group_page(group: &Group) -> String {
    let (unnamed, named): (Vec<_>, Vec<_>) = capabilities().into_iter().filter(|c| c.group == group.name).partition(|c| c.action.is_empty());
    let mut text = match unnamed.first() {
        Some(c) if named.is_empty() => return capability_page(c),
        Some(c) => format!("{}\n", capability_page(c)),
        None => format!("{}: {}\n\n", group.name, prose(group.about)),
    };
    for c in named {
        let side = if c.extensions { "" } else { "  (remote control only)" };
        let _ = writeln!(text, "  {}{side}", c.name());
        let _ = writeln!(text, "      {}", first_sentence(c.about));
    }
    text
}

/// One capability: what it does, how it is called from Lua and from the
/// command line, its arguments and its result.
pub fn capability_page(c: &Capability) -> String {
    let mut text = format!("{}\n\n{}\n", c.name(), prose(c.about));
    let _ = writeln!(text, "{}", if c.in_game { "Only while a game is going on." } else { "In the main menu too." });
    if c.country {
        let _ = writeln!(
            text,
            "It acts as a country: the one the player plays, or in observe mode the country in view; with none in view it fails with no_country."
        );
    }
    let _ = writeln!(text, "\ncalled:");
    if c.extensions {
        let _ = writeln!(text, "  Lua:          {}", lua_call(c));
    }
    let _ = writeln!(text, "  command line: eu4-harmony game {}", command(c));
    if !c.extensions {
        let _ = writeln!(text, "  Remote control only.");
    }
    if !c.args.is_empty() {
        let _ = writeln!(text, "\narguments:");
        for arg in c.args {
            let optional = if arg.optional { ", optional" } else { "" };
            let _ = writeln!(text, "  {} ({}{optional}): {}", arg.name, arg.kind.name(), prose(arg.about));
        }
    }
    let _ = writeln!(text, "\nresult: {}", c.result);
    text
}

/// How Lua calls it: the required arguments in order, the optional ones in
/// a table last.
fn lua_call(c: &Capability) -> String {
    let mut args: Vec<String> = c.args.iter().filter(|a| !a.optional).map(|a| a.name.to_string()).collect();
    let optional: Vec<String> = c.args.iter().filter(|a| a.optional).map(|a| format!("{} = ...", a.name)).collect();
    if !optional.is_empty() {
        args.push(format!("{{ {} }}", optional.join(", ")));
    }
    // Texts are not the game's state: Lua has them as the global `text`.
    let owner = if c.group == "text" { "" } else { "game." };
    format!("{owner}{}.{}({})", c.group, c.action, args.join(", "))
}

/// How the command line names it, with its arguments.
fn command(c: &Capability) -> String {
    let signature = crate::game::signature(c);
    let space = if signature.is_empty() { "" } else { " " };
    format!("{}{space}{signature}", c.name().replace('.', " "))
}

fn first_sentence(about: &str) -> String {
    let text = prose(about);
    match text.find(". ") {
        Some(end) => text[..=end].to_string(),
        None => text,
    }
}

/// What a mod's Lua has besides the capabilities.
fn lua_page() -> String {
    let mut text = String::from(
        "\
lua: what a mod's Lua has

main.lua runs once, at startup, and returns a table of handlers, one for
each signal the mod hears. Each handler call runs as a task, which may wait
for an answer (a function marked \"waits\") while the game goes on; game, the
same table every time, reaches the capabilities as game.<group>.<name>(...)
(eu4-harmony wiki <group>) only while a handler or a task runs. A
capability or function that fails as the game or the system may make it
fail returns nil, err; one called wrongly raises an error, which ends the
handler's call, is logged, and the mod is called again with the next signal.

handlers (each gets game first):
",
    );
    for h in lua::handlers() {
        let args: Vec<&str> = std::iter::once("game").chain(h.args.iter().map(|a| a.name)).collect();
        let returns = h.returns.as_ref().map_or(String::new(), |r| format!(" -> {}", r.name));
        let _ = writeln!(text, "  {}({}){returns}\n      {}", h.name, args.join(", "), h.about);
        for a in h.args.iter().chain(h.returns.iter()) {
            let _ = writeln!(text, "      {}: {}", a.name, a.about);
        }
    }
    let _ = writeln!(text, "\nglobals:");
    let line = |text: &mut String, name: &str, f: &lua::Function| {
        let args: Vec<String> = f.args.iter().map(|a| if a.optional { format!("[{}]", a.name) } else { a.name.to_string() }).collect();
        let returns: Vec<&str> = f.returns.iter().map(|r| r.name).collect();
        let returns = if returns.is_empty() { String::new() } else { format!(" -> {}", returns.join(", ")) };
        let mut marks = Vec::new();
        if f.waits {
            marks.push("waits".to_string());
        }
        if let Some(p) = f.permission {
            marks.push(format!("needs {p}"));
        }
        let marks = if marks.is_empty() { String::new() } else { format!("  ({})", marks.join("; ")) };
        let _ = writeln!(text, "  {name}({}){returns}{marks}\n      {}", args.join(", "), f.about);
    };
    for f in lua::FUNCTIONS {
        line(&mut text, f.name, f);
    }
    let _ = writeln!(text, "\na task's methods (t = spawn(fn), or an async function's call; eu4-harmony skills tasks):");
    for f in lua::TASK_METHODS {
        line(&mut text, &format!("t:{}", f.name), f);
    }
    let _ = write!(text, "\ninterface elements:\n  {}\n\nthe sandbox:\n  {}\n", lua::ELEMENTS, lua::SANDBOX);
    text
}

/// The manifest's fields and the permissions it may declare.
fn manifest_page() -> String {
    let mut text = String::from("manifest: eu4-harmony/manifest.toml, what cannot be told from where the mod's files are\n\nfields:\n");
    for f in lua::MANIFEST {
        let required = if f.required { "required" } else { "optional" };
        let _ = writeln!(text, "  {} ({required})\n      {}\n      {}", f.name, f.about, f.example);
    }
    let _ = writeln!(
        text,
        "\npermissions: the sensitive capabilities a mod may use beyond the game. Using one\nthe manifest does not declare raises an error; the player sees each in the mods window.\n"
    );
    for p in lua::PERMISSIONS {
        let _ = writeln!(text, "  {}: {}\n      gives {}; declared as {}", p.name, p.about, p.gives, p.declared);
    }
    text
}

fn remote() -> String {
    "\
remote: playing a game through remote control

A remote client uses the running game through the framework's capabilities,
as the player would, over the named pipe \\\\.\\pipe\\eu4-harmony; the command
line is eu4-harmony game (eu4-harmony game -h for its form).

how a game goes:
  menu says where the game is and which process answers. From the main
  menu, menu.saves lists the saves, and menu.load <save> or
  menu.start <country> goes into a game, answering once it is ready. There
  session.get, country.get, province.get, army.list and screenshot show the
  game; question.list and offer.list what waits for the player's answer,
  which question.answer, offer.accept and offer.decline give. time.advance
  <days> lets time run and pauses the game again, stopping early for a
  question: answer it, and advance again. Orders (army.move, recruit.order,
  diplomacy) are given while the game is paused too. menu.return goes back
  to the main menu; with the framework settings returning without
  restarting it answers from the same process, otherwise ask menu until its
  process differs and its place is main_menu.

exit codes of eu4-harmony game:
  0  the game answered; the result is printed
  1  the game answered with an error, printed as {\"code\": ..., \"message\": ...}
     (eu4-harmony wiki errors)
  2  the command line is wrong; nothing was sent
  3  no answer: the game's pipe did not open in time, is not served by the
     game run by this Windows user, broke, or the time ran out; the line the
     game sent is no answer (not JSON, or not of an answer's shape); or the
     bytes in the answer could not be written
"
    .to_string()
}

fn errors() -> String {
    let mut text = String::from("errors: the codes a failed capability answers, with its message\n\n");
    for code in Code::ALL {
        let _ = writeln!(text, "  {:<18}{}", code.name(), code.about());
    }
    text
}

fn signals() -> String {
    let mut text = String::from("signals: what extensions hear (as on_<signal> handlers) and remote control numbers\n\n");
    for (kind, about) in SIGNALS {
        let _ = writeln!(text, "  {kind:<24}{}", prose(about));
        for field in kind.fields() {
            let _ = writeln!(text, "      {}: {}", field.name, field.about);
        }
    }
    let _ = write!(text, "\nA remote client reads them with the capability below.\n\n{}", group_page(&group("signals")));
    text
}

fn group(name: &str) -> Group {
    groups().into_iter().find(|g| g.name == name).expect("a group of the catalogue")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn every_group_and_capability_has_a_page() {
        assert!(page(&[]).unwrap().contains("army"));
        for g in groups() {
            assert!(page(&words(g.name)).unwrap().starts_with(g.name));
        }
        for c in capabilities() {
            let page = page(&words(&c.name().replace('.', " "))).unwrap();
            let shown = format!("\n{}\n", c.name());
            assert!(page.starts_with(&shown[1..]) || page.contains(&shown), "{page}");
            assert_eq!(page.contains("Lua:"), c.extensions, "{page}");
        }
    }

    #[test]
    fn a_capability_is_found_by_its_dotted_name_too() {
        assert_eq!(page(&words("army.move")), page(&words("army move")));
    }

    #[test]
    fn lua_takes_required_arguments_in_order_and_optional_ones_in_a_table() {
        let declare = capabilities().into_iter().find(|c| c.name() == "diplomacy.declare_war").unwrap();
        assert_eq!(lua_call(&declare), "game.diplomacy.declare_war(country, casus_belli, { war_goal = ..., province = ..., target = ..., call_allies = ... })");
        let get = capabilities().into_iter().find(|c| c.name() == "session.get").unwrap();
        assert_eq!(lua_call(&get), "game.session.get()");
        let language = capabilities().into_iter().find(|c| c.name() == "text.language").unwrap();
        assert_eq!(lua_call(&language), "text.language()");
    }

    #[test]
    fn the_lua_and_manifest_topics_name_every_handler_global_field_and_permission() {
        let lua_page = page(&words("lua")).unwrap();
        assert!(lua::handlers().iter().all(|h| lua_page.contains(&format!("  {}(game", h.name))), "{lua_page}");
        assert!(lua::FUNCTIONS.iter().all(|f| lua_page.contains(&format!("  {}(", f.name))), "{lua_page}");
        assert!(lua::TASK_METHODS.iter().all(|f| lua_page.contains(&format!("  t:{}(", f.name))), "{lua_page}");
        assert!(lua_page.contains("http.fetch(url, [options]) -> response, err  (waits; needs network)"), "{lua_page}");
        let manifest = page(&words("manifest")).unwrap();
        assert!(lua::MANIFEST.iter().all(|f| manifest.contains(&format!("  {} (", f.name))), "{manifest}");
        assert!(lua::PERMISSIONS.iter().all(|p| manifest.contains(&format!("  {}: ", p.name))), "{manifest}");
    }

    #[test]
    fn unknown_topics_are_refused() {
        assert!(page(&words("armee")).is_err());
        assert!(page(&words("army fly")).is_err());
    }

    #[test]
    fn every_error_code_and_signal_is_explained() {
        let errors = page(&words("errors")).unwrap();
        assert!(Code::ALL.iter().all(|c| errors.contains(c.name())));
        let signals = page(&words("signals")).unwrap();
        assert!(signals.contains("session_ended"));
        // Each signal's fields, with what each is.
        assert!(signals.contains("      attacker_losses: What the attackers lost"), "{signals}");
    }
}
