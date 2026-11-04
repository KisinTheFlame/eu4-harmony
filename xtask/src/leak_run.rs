//! `cargo xtask leak-run`: plays rounds of sessions on a developer build, and
//! reports what the game's memory keeps of them (docs/leak-finding.md).
//!
//! It needs a developer build with the memory account installed
//! (`--features eu4-harmony-core/dev-memory`, then `cargo xtask install`),
//! which costs the game nothing between rounds; with the heap census
//! (`eu4-harmony-core/dev-heap-census`) the report adds the leftovers by site
//! and what holds them, but the census slows the game and holds hundreds of
//! megabytes of its own. And the game claimed and its player's files backed
//! up: the run loads saves and starts games, which autosave. It starts
//! eu4.exe unless it runs, and plays its rounds through remote control. Each
//! round:
//! 1. loads the first save from the main menu, and lets the days run;
//! 2. loads the second save from that game, and lets a third of them run;
//! 3. returns to the main menu;
//! 4. starts a new game as the country, and lets a third of the days run;
//! 5. returns to the main menu.
//!
//! After each round it asks `memory`, and with the census
//! `memory.leftovers`, and prints a line; after the last, `memory.leftovers`
//! looks for what still points at the leftovers too, which stops the game
//! for seconds. At the end it writes `report.json`, every answer, and
//! `report.md`: the rounds; where each round's growth went, part by part and
//! heap by heap, as the least-squares slope over the rounds from the second
//! on (the first loads what stays for good, and one round's figures jump
//! about); and with the census the leftover sites that leave the most per
//! generation, and what holds them.
//!
//! With the census and `--watch N`, after round `--watch-after` it looks for
//! what holds the leftovers, and watches the N steady sites that leave the
//! most and whose leftovers nothing, or only other leftovers, mostly points
//! at (`memory.watch`); the report then says, for each, which owners were
//! let go while they last pointed at its leftovers, and the code that let
//! them go. Watching slows every letting go of a block.
//!
//! With the census and `--quarantine`, the quarantine is on from the start
//! (`memory.quarantine`): what the framework's repairs let go is kept instead
//! of freed and filled with a pattern, and compared every two seconds. The
//! report says what it kept by repair, and every block written to after a
//! repair let it go. A read of such a block can end the game at once; the
//! report then says in which round, and what was found until then. The
//! memory it keeps counts in the game's heap, so a run with it measures no
//! growth.
use crate::{game, Args, Result};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub fn run(args: &Args) -> Result<()> {
    let out = PathBuf::from(args.value("out").ok_or("--out names the folder for the report")?);
    std::fs::create_dir_all(&out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;
    let report = if args.flag("from-report") {
        let path = out.join("report.json");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", path.display()))?
    } else {
        let report = play(args)?;
        write(&out.join("report.json"), &serde_json::to_string_pretty(&report).expect("JSON"))?;
        if !args.flag("keep-game") {
            game::kill();
        }
        report
    };
    write(&out.join("report.md"), &markdown(&report, &now()))?;
    println!("report: {}", out.join("report.md").display());
    Ok(())
}

/// Plays the rounds; the report.
fn play(args: &Args) -> Result<Value> {
    let rounds = args.number("rounds", 10)?;
    let country = args.value("country").unwrap_or("FRA").to_string();
    let days = args.number("days", 30)?;
    let sites = args.number("sites", 40)?.to_string();
    let watch = args.number("watch", 0)?;
    let watch_after = args.number("watch-after", 3)?;
    let game_dir = args.game_dir()?;
    let cli = game_dir.join("eu4-harmony").join("eu4-harmony.exe");
    if !cli.is_file() {
        return Err(format!("no command line tool at {}: install the developer build with cargo xtask install", cli.display()));
    }
    let game = Game { cli };
    if !game::running() {
        game::start(&game_dir)?;
    }
    game.main_menu()?;
    game.ask(&["memory"]).map_err(|e| format!("the game has no memory group: install a developer build with the memory account ({e})"))?;
    let saves: Vec<String> = match args.positional.as_slice() {
        [] => game.ask(&["menu.saves"])?.as_array().into_iter().flatten().take(2).filter_map(|s| s["name"].as_str().map(String::from)).collect(),
        given => given.to_vec(),
    };
    if saves.len() != 2 {
        return Err("two saves are needed: name them, or have two in the save games folder".into());
    }
    println!("saves: {}; new games as {country}; {days} days", saves.join(", "));

    let start = game.ask(&["memory"])?;
    let census = truthy(&start["census"]);
    println!("startup: {}; the census {} built in", line(&start), if census { "is" } else { "is not" });
    let quarantine = args.flag("quarantine");
    if quarantine {
        if !census {
            return Err("--quarantine needs the census built in".into());
        }
        game.ask(&["memory.quarantine", "--on", "true"])?;
    }
    let full = days.to_string();
    let third = (days / 3).max(1).to_string();
    let mut played: Vec<Value> = Vec::new();
    let mut kept = Value::Null;
    // A round that fails, the game gone among others, ends the run with what
    // the rounds before found.
    let mut play_rounds = || -> Result<()> {
        for i in 1..=rounds {
            let began = Instant::now();
            game.ask(&["menu.load", &saves[0]])?;
            game.advance(&full)?;
            game.ask(&["menu.load", &saves[1]])?;
            game.advance(&third)?;
            game.ask(&["menu.return"])?;
            game.main_menu()?;
            game.ask(&["menu.start", &country])?;
            game.advance(&third)?;
            game.ask(&["menu.return"])?;
            game.main_menu()?;
            std::thread::sleep(Duration::from_secs(5));
            let memory = game.ask(&["memory"])?;
            let mut leftovers = Value::Null;
            if census {
                let mut asked = vec!["memory.leftovers", "--sites", &sites];
                if i == rounds {
                    asked.extend(["--holders", "true"]);
                }
                leftovers = game.ask(&asked)?;
            }
            let mut watching = Value::Null;
            if census && watch > 0 && i == watch_after && i < rounds {
                let held = game.ask(&["memory.leftovers", "--sites", &sites, "--holders", "true"])?;
                let lost: Vec<String> = list(&held["sites"])
                    .iter()
                    .filter(|s| {
                        s["trend"] == "steady" && (number(&s["holders"]["by_nothing"]) + number(&s["holders"]["by_leftovers"])) * 2.0 >= number(&s["blocks"])
                    })
                    .take(watch as usize)
                    .map(|s| text(&s["site"]))
                    .collect();
                if !lost.is_empty() {
                    watching = game.ask(&["memory.watch", "--sites", &lost.join(",")])?;
                    let watched: Vec<String> = list(&watching["sites"]).iter().map(text).collect();
                    println!("watching from round {i}: {} ({} live blocks)", watched.join(", "), text(&watching["blocks"]));
                }
            }
            let quarantined = if quarantine { game.ask(&["memory.quarantine"])? } else { Value::Null };
            let seconds = began.elapsed().as_secs();
            let grew = played.last().map_or(String::new(), |last| format!(" ({} MB)", signed(change_mb(&memory["private"], &last["memory"]["private"]), 1)));
            let held = if quarantined.is_null() {
                String::new()
            } else {
                format!(
                    "; quarantine {} blocks, {} MB, {} written to",
                    text(&quarantined["blocks"]),
                    mb(&quarantined["bytes"]),
                    list(&quarantined["written"]).len()
                )
            };
            println!("round {i} ({seconds} s){grew}: {}{held}", line(&memory));
            played.push(json!({ "round": i, "seconds": seconds, "memory": memory, "leftovers": leftovers, "watching": watching, "quarantine": quarantined }));
        }
        if quarantine {
            kept = game.ask(&["memory.quarantine", "--on", "false"])?;
        }
        Ok(())
    };
    let failed = play_rounds().err().map(|e| format!("round {}: {e}", played.len() + 1));
    if let Some(failed) = &failed {
        println!("stopped in {failed}");
    }
    Ok(json!({ "saves": saves, "country": country, "days": days, "startup": start, "rounds": played, "failed": failed, "quarantine": kept }))
}

/// The installed command line tool, which asks the game.
struct Game {
    cli: PathBuf,
}

impl Game {
    /// The answer, or what the game said instead.
    fn ask(&self, words: &[&str]) -> Result<Value> {
        self.ask_within(words, 600)
    }

    fn ask_within(&self, words: &[&str], timeout: u32) -> Result<Value> {
        let output = std::process::Command::new(&self.cli)
            .arg("game")
            .args(words)
            .args(["--compact", "--timeout", &timeout.to_string()])
            .output()
            .map_err(|e| format!("cannot run {}: {e}", self.cli.display()))?;
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let text = text.trim();
        if !output.status.success() {
            return Err(format!("game {}: {text}", words.join(" ")));
        }
        serde_json::from_str(text).map_err(|e| format!("game {}: {e}: {text}", words.join(" ")))
    }

    fn advance(&self, days: &str) -> Result<()> {
        self.ask(&["time.advance", days, "--speed", "5"]).map(drop)
    }

    /// Waits up to five minutes for the main menu.
    fn main_menu(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(300);
        while Instant::now() < deadline {
            if !game::running() {
                return Err("the game is gone".into());
            }
            if self.ask_within(&["menu"], 10).is_ok_and(|m| m["place"] == "main_menu") {
                return Ok(());
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        Err("the main menu did not come up in 5 minutes; whether the framework started is in the user folder's eu4-harmony\\logs\\core.log".into())
    }
}

/// A round's memory in a line.
fn line(m: &Value) -> String {
    format!(
        "private {} MB, game heap {} MB, free in heaps {} MB, outside heaps {} MB, framework {} MB, handles {}",
        mb(&m["private"]),
        mb(&m["game_heap"]),
        mb(&m["free_in_heaps"]),
        mb(&m["outside_heaps"]),
        mb(&m["framework_heap"]),
        text(&m["handles"])
    )
}

/// The report's tables, dated `date`.
fn markdown(report: &Value, date: &str) -> String {
    let played = list(&report["rounds"]);
    let start = &report["startup"];
    let census = truthy(&start["census"]);
    let saves: Vec<String> = list(&report["saves"]).iter().map(text).collect();
    let mut md = vec![
        format!("# Leak run, {date}"),
        String::new(),
        format!("Saves {}, new games as {}, {} days; {} rounds.", saves.join(" and "), text(&report["country"]), text(&report["days"]), played.len()),
        String::new(),
    ];
    if let Some(failed) = report["failed"].as_str() {
        md.extend([format!("**The run stopped** in {failed}"), String::new()]);
    }
    md.extend([
        "| Round | Seconds | Private MB | Change | Game heap MB | Free in heaps MB | Outside heaps MB | Framework MB | Handles |".into(),
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- |".into(),
    ]);
    let mut before = start["private"].clone();
    for r in &played {
        let m = &r["memory"];
        md.push(format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            text(&r["round"]),
            text(&r["seconds"]),
            mb(&m["private"]),
            signed(change_mb(&m["private"], &before), 1),
            mb(&m["game_heap"]),
            mb(&m["free_in_heaps"]),
            mb(&m["outside_heaps"]),
            mb(&m["framework_heap"]),
            text(&m["handles"])
        ));
        before = m["private"].clone();
    }

    if played.len() >= 2 {
        // The rounds from the second on, or both when there are two.
        let from = if played.len() >= 3 { 1 } else { 0 };
        let rounds = &played[from..];
        let per = |of: &dyn Fn(&Value) -> f64| signed(slope(rounds, of) / 1048576.0, 2);
        let field = |key: &'static str| move |m: &Value| number(&m[key]);
        md.extend([
            String::new(),
            "## Where each round's growth goes".into(),
            String::new(),
            format!("The least-squares slope over rounds {} to {}.", from + 1, played.len()),
            String::new(),
            "| Part | MB a round |".into(),
            "| --- | --- |".into(),
            format!("| Private bytes | {} |", per(&field("private"))),
            format!("| The game's heap, what the game holds | {} |", per(&field("game_heap"))),
            format!(
                "| The other heaps, what they hold | {} |",
                per(&|m: &Value| number(&m["heaps_allocated"]) - number(&m["game_heap"]) - number(&m["framework_heap"]))
            ),
            format!("| The heaps, committed and unused | {} |", per(&field("free_in_heaps"))),
            format!("| Outside the heaps (Direct3D, the driver, whole pages) | {} |", per(&field("outside_heaps"))),
            format!("| The framework's heap | {} |", per(&field("framework_heap"))),
            format!("| Handles | {} |", signed(slope(rounds, &field("handles")), 1)),
            String::new(),
            "### Each heap".into(),
            String::new(),
            "At the end of the last round, in MB, and the slope as above.".into(),
            String::new(),
            "| Heap | Game's | Default | Allocated | Allocated a round | Committed | Committed a round |".into(),
            "| --- | --- | --- | --- | --- | --- | --- |".into(),
        ]);
        let last = played.last().expect("two rounds or more");
        for h in list(&last["memory"]["heaps"]) {
            let handle = h["handle"].clone();
            let of = |key: &'static str| {
                let handle = handle.clone();
                move |m: &Value| list(&m["heaps"]).iter().find(|x| x["handle"] == handle).map_or(0.0, |x| number(&x[key]))
            };
            md.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                text(&handle),
                if truthy(&h["game"]) { "yes" } else { "" },
                if truthy(&h["process"]) { "yes" } else { "" },
                mb(&h["allocated"]),
                per(&of("allocated")),
                mb(&h["committed"]),
                per(&of("committed"))
            ));
        }
    }

    if let (true, Some(last)) = (census, played.last().map(|r| &r["leftovers"])) {
        let sites = list(&last["sites"]);
        md.extend([
            String::new(),
            format!("## Leftovers at generation {}", text(&last["generation"])),
            String::new(),
            format!("{} blocks, {} MB in all.", text(&last["blocks"]), mb(&last["bytes"])),
            String::new(),
            "| Site | Function | Trend | Per generation KB | Leftover KB | Blocks | Let go at (blocks) |".into(),
            "| --- | --- | --- | --- | --- | --- | --- |".into(),
        ]);
        for s in &sites {
            let freed: Vec<String> =
                list(&s["freed_at"]).iter().map(|f| format!("{} in {} ({})", text(&f["site"]), text(&f["function"]), text(&f["blocks"]))).collect();
            md.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                text(&s["site"]),
                text(&s["function"]),
                text(&s["trend"]),
                kb(&s["per_generation"]),
                kb(&s["bytes"]),
                text(&s["blocks"]),
                freed.join("; ")
            ));
        }
        if sites.first().is_some_and(|s| truthy(&s["holders"])) {
            md.extend([
                String::new(),
                "## What holds them".into(),
                String::new(),
                "Each leftover counted once, by the best that points at it: the game's data, other live blocks, only the leftovers listed, or nothing.".into(),
                String::new(),
                "| Site | Trend | Leftovers | By data | By blocks | By leftovers | By nothing | Held most by (pointers) |".into(),
                "| --- | --- | --- | --- | --- | --- | --- | --- |".into(),
            ]);
            for s in &sites {
                let h = &s["holders"];
                let top: Vec<String> = list(&h["top"])
                    .iter()
                    .map(|t| match t["kind"].as_str() {
                        Some("data") => format!("data {} ({})", text(&t["at"]), text(&t["references"])),
                        _ => format!("{} in {} ({})", text(&t["at"]), text(&t["function"]), text(&t["references"])),
                    })
                    .collect();
                md.push(format!(
                    "| {} | {} | {} | {} | {} | {} | {} | {} |",
                    text(&s["site"]),
                    text(&s["trend"]),
                    text(&s["blocks"]),
                    text(&h["by_data"]),
                    text(&h["by_blocks"]),
                    text(&h["by_leftovers"]),
                    text(&h["by_nothing"]),
                    top.join("; ")
                ));
            }
            let watched: Vec<&Value> = sites.iter().filter(|s| truthy(&s["holders"]["watched"])).collect();
            if !watched.is_empty() {
                let since = played.iter().find(|r| truthy(&r["watching"])).map_or(String::new(), |r| text(&r["round"]));
                md.extend([
                    String::new(),
                    "## Who let them go".into(),
                    String::new(),
                    format!("Watched from round {since} on. For the leftovers of each watched site that nothing points at: the owners let go while they last pointed at them, and the code that let them go. Unrecorded: lost before watching began, or the pointer was written over."),
                    String::new(),
                    "| Site | Nothing points at | Owner (site in function) | Let go at (in function) | Leftovers |".into(),
                    "| --- | --- | --- | --- | --- |".into(),
                ]);
                for s in watched {
                    let h = &s["holders"];
                    for l in list(&h["lost_with"]) {
                        md.push(format!(
                            "| {} | {} | {} in {} | {} in {} | {} |",
                            text(&s["site"]),
                            text(&h["by_nothing"]),
                            text(&l["owner"]),
                            text(&l["owner_function"]),
                            text(&l["let_go_at"]),
                            text(&l["let_go_function"]),
                            text(&l["blocks"])
                        ));
                    }
                    md.push(format!("| {} | {} | unrecorded | | {} |", text(&s["site"]), text(&h["by_nothing"]), text(&h["unrecorded"])));
                }
            }
        }
    }

    // What the quarantine said last: as it was turned off, or after the last
    // round played when the run stopped.
    let kept = if report["quarantine"].is_null() { played.last().map_or(Value::Null, |r| r["quarantine"].clone()) } else { report["quarantine"].clone() };
    if !kept.is_null() {
        md.extend([
            String::new(),
            "## What the quarantine kept".into(),
            String::new(),
            format!(
                "What the framework's repairs let go, kept instead of freed and filled with a pattern; compared {} times. {} blocks were freed again to keep it under its most.",
                text(&kept["checks"]),
                text(&kept["freed"])
            ),
            String::new(),
            "| Repair | Blocks | MB |".into(),
            "| --- | --- | --- |".into(),
        ]);
        for r in list(&kept["repairs"]) {
            md.push(format!("| {} | {} | {} |", text(&r["repair"]), text(&r["blocks"]), mb(&r["bytes"])));
        }
        let written = list(&kept["written"]);
        if written.is_empty() {
            md.extend([String::new(), "Nothing was written to after a repair let it go.".into()]);
        } else {
            md.extend([
                String::new(),
                "### Written to after a repair let it go".into(),
                String::new(),
                "The first write to each block, as found; the generations are the census's.".into(),
                String::new(),
                "| Repair | Site in function | Size | Offset | Bytes from it | Kept at | Found at |".into(),
                "| --- | --- | --- | --- | --- | --- | --- |".into(),
            ]);
            for w in written {
                md.push(format!(
                    "| {} | {} in {} | {} | {} | {} | {} | {} |",
                    text(&w["repair"]),
                    text(&w["site"]),
                    text(&w["function"]),
                    text(&w["size"]),
                    text(&w["offset"]),
                    text(&w["bytes"]),
                    text(&w["generation"]),
                    text(&w["found"])
                ));
            }
        }
    }
    md.iter().map(|l| format!("{l}\n")).collect()
}

/// The least-squares slope per round of what `of` reads of each round's
/// memory.
fn slope(rounds: &[Value], of: &dyn Fn(&Value) -> f64) -> f64 {
    let x: Vec<f64> = rounds.iter().map(|r| number(&r["round"])).collect();
    let y: Vec<f64> = rounds.iter().map(|r| of(&r["memory"])).collect();
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let (mx, my) = (mean(&x), mean(&y));
    let sxy: f64 = x.iter().zip(&y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = x.iter().map(|a| (a - mx) * (a - mx)).sum();
    sxy / sxx
}

/// The array `v` is, or none.
fn list(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}

/// A string as itself, null as nothing, anything else as JSON.
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or_default()
}

/// Whether `v` is anything but null, false, zero, an empty string or array.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

/// Bytes as megabytes, one decimal, thousands grouped.
fn mb(bytes: &Value) -> String {
    grouped(number(bytes) / 1048576.0, 1)
}

/// Bytes as kilobytes, whole, thousands grouped.
fn kb(bytes: &Value) -> String {
    grouped(number(bytes) / 1024.0, 0)
}

/// The change from `before` to `now`, in megabytes.
fn change_mb(now: &Value, before: &Value) -> f64 {
    (number(now) - number(before)) / 1048576.0
}

/// `value` with `decimals` decimals and its sign, `+` too; plain zero when it
/// rounds to zero.
fn signed(value: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    if text.chars().all(|c| c == '0' || c == '.') {
        text
    } else {
        format!("{}{text}", if value < 0.0 { "-" } else { "+" })
    }
}

/// `value` with `decimals` decimals, its thousands grouped with commas.
fn grouped(value: f64, decimals: usize) -> String {
    let text = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = text.split_once('.').map_or((text.as_str(), None), |(w, f)| (w, Some(f)));
    let mut out = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(f) = fraction {
        out = format!("{out}.{f}");
    }
    if value < 0.0 && !text.chars().all(|c| c == '0' || c == '.') {
        out.insert(0, '-');
    }
    out
}

/// The local date and time, as `yyyy-MM-dd HH:mm`.
fn now() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:04}-{:02}-{:02} {:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute)
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_as_the_reports_always_wrote_them() {
        assert_eq!(grouped(1234567.891, 1), "1,234,567.9");
        assert_eq!(grouped(999.0, 0), "999");
        assert_eq!(grouped(-1500.0, 0), "-1,500");
        assert_eq!(grouped(-0.01, 1), "0.0");
        assert_eq!(signed(2.0, 1), "+2.0");
        assert_eq!(signed(-1.04, 1), "-1.0");
        assert_eq!(signed(-0.004, 2), "0.00");
        assert_eq!(signed(0.0, 1), "0.0");
    }

    #[test]
    fn slope_fits_a_line() {
        let rounds: Vec<Value> = [(1, 10.0), (2, 12.0), (3, 14.0)].iter().map(|(r, p)| json!({ "round": r, "memory": { "private": p } })).collect();
        assert!((slope(&rounds, &|m| number(&m["private"])) - 2.0).abs() < 1e-9);
    }

    /// The tables a report without the census holds, and the census's when
    /// it holds them.
    #[test]
    fn markdown_writes_the_rounds_their_growth_and_the_leftovers() {
        let mib = 1048576;
        let memory = |p: i64| {
            json!({ "private": p * mib, "game_heap": p * mib / 2, "free_in_heaps": 0, "outside_heaps": 0, "framework_heap": mib, "heaps_allocated": p * mib, "handles": 100,
                    "heaps": [{ "handle": "0x1", "game": true, "process": false, "allocated": p * mib, "committed": p * mib }] })
        };
        let rounds: Vec<Value> =
            (1..=3).map(|i| json!({ "round": i, "seconds": 60, "memory": memory(100 + i), "leftovers": null, "watching": null })).collect();
        let mut report = json!({ "saves": ["a", "b"], "country": "FRA", "days": 30, "startup": memory(100), "rounds": rounds });
        let md = markdown(&report, "2026-11-04 12:00");
        assert!(md.starts_with("# Leak run, 2026-11-04 12:00\n\nSaves a and b, new games as FRA, 30 days; 3 rounds.\n"), "{md}");
        assert!(md.contains("| 2 | 60 | 102.0 | +1.0 | 51.0 | 0.0 | 0.0 | 1.0 | 100 |"), "{md}");
        assert!(md.contains("The least-squares slope over rounds 2 to 3.") && md.contains("| Private bytes | +1.00 |"), "{md}");
        assert!(md.contains("| Handles | 0.0 |") && md.contains("| 0x1 | yes |  | 103.0 | +1.00 | 103.0 | +1.00 |"), "{md}");
        assert!(!md.contains("## Leftovers"), "{md}");

        report["startup"]["census"] = json!(true);
        report["rounds"][2]["leftovers"] = json!({ "generation": 4, "blocks": 2, "bytes": 2048, "sites": [{
            "site": "0x10", "function": "0x20", "trend": "steady", "per_generation": 1024, "bytes": 2048, "blocks": 2,
            "freed_at": [{ "site": "0x30", "function": "0x40", "blocks": 1 }],
            "holders": { "by_data": 0, "by_blocks": 0, "by_leftovers": 1, "by_nothing": 1, "top": [{ "kind": "data", "at": "0x50", "references": 3 }],
                         "watched": true, "lost_with": [{ "owner": "0x60", "owner_function": "0x70", "let_go_at": "0x80", "let_go_function": "0x90", "blocks": 1 }], "unrecorded": 0 } }] });
        report["rounds"][1]["watching"] = json!({ "sites": ["0x10"], "blocks": 2 });
        let md = markdown(&report, "2026-11-04 12:00");
        assert!(md.contains("## Leftovers at generation 4\n\n2 blocks, 0.0 MB in all."), "{md}");
        assert!(md.contains("| 0x10 | 0x20 | steady | 1 | 2 | 2 | 0x30 in 0x40 (1) |"), "{md}");
        assert!(md.contains("| 0x10 | steady | 2 | 0 | 0 | 1 | 1 | data 0x50 (3) |"), "{md}");
        assert!(
            md.contains("Watched from round 2 on.") && md.contains("| 0x10 | 1 | 0x60 in 0x70 | 0x80 in 0x90 | 1 |\n| 0x10 | 1 | unrecorded | | 0 |"),
            "{md}"
        );
    }

    #[test]
    fn markdown_says_where_a_run_stopped_and_what_the_quarantine_kept() {
        let memory = json!({ "private": 1048576, "game_heap": 0, "free_in_heaps": 0, "outside_heaps": 0, "framework_heap": 0, "handles": 1, "heaps": [] });
        let quarantine = json!({ "checks": 7, "freed": 0, "repairs": [{ "repair": "text box", "blocks": 3, "bytes": 2097152 }], "written": [] });
        let mut report = json!({ "saves": ["a", "b"], "country": "FRA", "days": 30, "startup": memory, "failed": "round 2: the game is gone",
                                 "rounds": [{ "round": 1, "seconds": 60, "memory": memory, "quarantine": quarantine }], "quarantine": null });
        let md = markdown(&report, "now");
        assert!(md.contains("**The run stopped** in round 2: the game is gone\n\n| Round |"));
        assert!(md.contains("compared 7 times.") && md.contains("| text box | 3 | 2.0 |") && md.contains("Nothing was written to"));
        report["quarantine"] = json!({ "checks": 9, "freed": 1, "repairs": [], "written": [{ "repair": "text box", "site": "0x1", "function": "0x2", "size": 64,
                                       "offset": 8, "bytes": "00ff", "generation": 3, "found": 4 }] });
        let md = markdown(&report, "now");
        assert!(md.contains("compared 9 times.") && md.contains("| text box | 0x1 in 0x2 | 64 | 8 | 00ff | 3 | 4 |"));
    }
}
