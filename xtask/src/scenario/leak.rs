//! `leak` and `growth`: rounds of sessions, and what the game's memory keeps
//! of them (docs/leak-finding.md). Each round, from the main menu:
//! 1. loads the first save, and lets the days run;
//! 2. loads the second save from that game, and lets a third of them run;
//! 3. returns to the main menu;
//! 4. starts a new game as the country, and lets a third of the days run;
//! 5. returns to the main menu.
//!
//! After each round it asks `memory`. Its sections: the rounds; where each
//! round's growth went, part by part and heap by heap, as the least-squares
//! slope over the rounds from the second on (the first loads what stays for
//! good, and one round's figures jump about).
//!
//! `growth` plays them on the build with the memory account only, which
//! costs the game nothing between rounds. `leak` plays them on the build
//! with the heap census, which slows the game and holds hundreds of
//! megabytes of its own, and asks `memory.leftovers` after each round as
//! well; after the last, what still points at the leftovers, which stops the
//! game for seconds, and `memory.pages`, what keeps the heap's free pages
//! committed, which holds it for about two. Its sections add the leftover
//! sites that leave the most per generation, what holds them and what keeps
//! the free pages.
//!
//! What the census does not count, other modules' blocks among them, `leak`
//! marks after the second round (the first, with one round only) and
//! compares with after the last (`memory.uncounted`): its sections say how
//! much of the game's heap that is, how much it grew, and the sizes that
//! grew the most, with samples of their blocks made since.
//!
//! With `watch` above 0, after round `watch_after` `leak` looks for what
//! holds the leftovers, and watches the `watch` steady sites that leave the
//! most and whose leftovers nothing, or only other leftovers, mostly points
//! at (`memory.watch`); its sections then say, for each, which owners were
//! let go while they last pointed at its leftovers, and the code that let
//! them go. With `watch_sites`, it watches those sites instead, as
//! `memory.leftovers` writes them, whatever their trend. Watching slows
//! every letting go of a block.
//!
//! With `quarantine`, the quarantine is on from the start
//! (`memory.quarantine`): what the framework's repairs let go is kept instead
//! of freed and filled with a pattern, and compared every two seconds. The
//! sections say what it kept by repair, and every block written to after a
//! repair let it go. A read of such a block can end the game at once; the
//! scenario then crashed, with what was found until then. The memory it
//! keeps counts in the game's heap, so a run with it measures no growth.
use super::{text, Build, Game, Parameter, Parameters, Played, Scenario};
use eu4_harmony_remote::{
    memory::{Leftovers, Memory, MemoryRequest, Pages, Quarantine, Trend, Uncounted, Watching},
    menu::{MenuRequest, Save},
    session::Session,
    CountryTag,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct Leak {
    /// With the heap census: `leak`; without: `growth`.
    pub census: bool,
}

pub const LEAK: Leak = Leak { census: true };
pub const GROWTH: Leak = Leak { census: false };

/// The parameters both have.
const ROUNDS: [Parameter; 5] = [
    Parameter { name: "rounds", default: "10", about: "rounds of sessions" },
    Parameter { name: "days", default: "30", about: "days a round's first game runs; its other two a third of them" },
    Parameter { name: "country", default: "FRA", about: "the country of each round's new game" },
    Parameter { name: "first_save", default: "England-1", about: "the save each round loads first, as menu.saves names it" },
    Parameter { name: "second_save", default: "英格兰1444_11_11", about: "the save each round loads from the first one's game" },
];

impl Scenario for Leak {
    fn name(&self) -> &'static str {
        if self.census {
            "leak"
        } else {
            "growth"
        }
    }

    fn about(&self) -> &'static str {
        if self.census {
            "rounds of sessions on the heap census, and the leftovers they leave, what holds them and what keeps the heap's free pages"
        } else {
            "rounds of sessions on the memory account, and where each round's growth goes"
        }
    }

    fn build(&self) -> Build {
        if self.census {
            Build::Census
        } else {
            Build::Memory
        }
    }

    fn minutes(&self) -> u32 {
        if self.census {
            25
        } else {
            20
        }
    }

    fn parameters(&self) -> &'static [Parameter] {
        const LEAK_ONLY: [Parameter; 10] = [
            ROUNDS[0],
            ROUNDS[1],
            ROUNDS[2],
            ROUNDS[3],
            ROUNDS[4],
            Parameter { name: "sites", default: "40", about: "the leftover sites to list, the most left a generation first" },
            Parameter { name: "watch", default: "0", about: "the steady sites nothing points at to watch from round watch_after" },
            Parameter { name: "watch_sites", default: "", about: "the sites to watch from round watch_after instead, separated by commas" },
            Parameter { name: "watch_after", default: "3", about: "the round after which to begin watching" },
            Parameter { name: "quarantine", default: "false", about: "keep what the repairs let go, and find what writes to it" },
        ];
        if self.census {
            &LEAK_ONLY
        } else {
            &ROUNDS
        }
    }

    fn saves(&self, parameters: &Parameters) -> Vec<String> {
        vec![parameters.text("first_save"), parameters.text("second_save")]
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let rounds = parameters.number("rounds")?;
        let days = parameters.number("days")?;
        let country = parameters.text("country");
        let tag = CountryTag::parse(&country).ok_or_else(|| super::Stop::Failed(format!("no country tag {country:?}")))?;
        let saves = [parameters.text("first_save"), parameters.text("second_save")];
        let (sites, watch, watch_sites, watch_after, quarantine) = if self.census {
            (
                Some(parameters.number("sites")?),
                parameters.number("watch")?,
                parameters.text("watch_sites"),
                parameters.number("watch_after")?,
                parameters.flag("quarantine")?,
            )
        } else {
            (None, 0, String::new(), 0, false)
        };

        let there: Vec<Save> = game.ask(&MenuRequest::Saves {})?;
        for save in &saves {
            game.check(there.iter().any(|s| &s.name == save), format!("no save {save} on this computer; menu.saves lists {}", names(&there)))?;
        }
        let start: Memory = game.ask(&MemoryRequest::Get {})?;
        game.check(start.census == self.census, if self.census { "the build has no heap census" } else { "the build has the heap census, which slows it" })?;
        *record = json!({ "saves": saves, "country": country, "days": days, "startup": start, "rounds": [], "quarantine": null });
        println!("startup: {}", line(&record["startup"]));
        if quarantine {
            let _: Quarantine = game.ask(&MemoryRequest::Quarantine { on: Some(true) })?;
        }
        let third = (days / 3).max(1);
        // The round after which the census's uncounted blocks are marked.
        let mark = if rounds >= 3 { 2 } else { 1 };
        for i in 1..=rounds {
            let began = Instant::now();
            let _: Session = game.ask(&MenuRequest::Load { save: saves[0].clone() })?;
            game.advance(days)?;
            let _: Session = game.ask(&MenuRequest::Load { save: saves[1].clone() })?;
            game.advance(third)?;
            game.return_to_menu()?;
            let _: Session = game.ask(&MenuRequest::Start { country: Some(tag) })?;
            game.advance(third)?;
            game.return_to_menu()?;
            std::thread::sleep(Duration::from_secs(5));
            let memory: Memory = game.ask(&MemoryRequest::Get {})?;
            let mut leftovers = Value::Null;
            let mut watching = Value::Null;
            let mut quarantined = Value::Null;
            let mut pages = Value::Null;
            let mut uncounted = Value::Null;
            if self.census {
                let held: Leftovers = game.ask(&MemoryRequest::Leftovers { sites, holders: Some(i == rounds) })?;
                leftovers = json!(held);
                if (watch > 0 || !watch_sites.is_empty()) && i == watch_after && i < rounds {
                    let lost: Vec<String> = if watch_sites.is_empty() {
                        let held: Leftovers = game.ask(&MemoryRequest::Leftovers { sites, holders: Some(true) })?;
                        held.sites
                            .iter()
                            .filter(|s| s.trend == Trend::Steady && s.holders.as_ref().is_some_and(|h| (h.by_nothing + h.by_leftovers) * 2 >= s.blocks))
                            .take(watch as usize)
                            .map(|s| s.site.clone())
                            .collect()
                    } else {
                        watch_sites.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                    };
                    if !lost.is_empty() {
                        let watched: Watching = game.ask(&MemoryRequest::Watch { sites: lost.join(",") })?;
                        println!("watching from round {i}: {} ({} live blocks)", watched.sites.join(", "), watched.blocks);
                        watching = json!(watched);
                    }
                }
                if quarantine {
                    let kept: Quarantine = game.ask(&MemoryRequest::Quarantine { on: None })?;
                    quarantined = json!(kept);
                }
                if i == rounds {
                    let walked: Pages = game.ask(&MemoryRequest::Pages { sites })?;
                    pages = json!(walked);
                }
                if i == mark || (i == rounds && rounds > mark) {
                    let found: Uncounted = game.ask(&MemoryRequest::Uncounted { sizes: None, samples: None })?;
                    if found.marked {
                        println!("not counted: {} MB, {} MB made since round {mark}", mb(&json!(found.bytes)), mb(&json!(found.made_bytes)));
                    }
                    uncounted = json!(found);
                }
            }
            let memory = json!(memory);
            let seconds = began.elapsed().as_secs();
            let rounds_so_far = list(&record["rounds"]);
            let grew =
                rounds_so_far.last().map_or(String::new(), |last| format!(" ({} MB)", signed(change_mb(&memory["private"], &last["memory"]["private"]), 1)));
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
            record["rounds"].as_array_mut().expect("the rounds").push(
                json!({ "round": i, "seconds": seconds, "memory": memory, "leftovers": leftovers, "watching": watching, "quarantine": quarantined, "pages": pages, "uncounted": uncounted }),
            );
        }
        if quarantine {
            let kept: Quarantine = game.ask(&MemoryRequest::Quarantine { on: Some(false) })?;
            record["quarantine"] = json!(kept);
        }
        Ok(())
    }

    fn report(&self, record: &Value) -> Vec<String> {
        sections(record)
    }
}

/// What the census does not count: as marked, and as it was after the last
/// round, compared with the mark.
fn uncounted_section(played: &[Value]) -> Vec<String> {
    let marked = played.iter().find(|r| !r["uncounted"].is_null() && !truthy(&r["uncounted"]["marked"]));
    let last = played.iter().rev().find(|r| truthy(&r["uncounted"]["marked"]));
    let (Some(marked), Some(last)) = (marked, last) else { return Vec::new() };
    let (from, to) = (number(&marked["round"]), number(&last["round"]));
    let (then, now) = (&marked["uncounted"], &last["uncounted"]);
    let rounds = (to - from).max(1.0);
    let heap = |r: &Value| number(&r["memory"]["game_heap"]);
    let mut md = vec![
        String::new(),
        "### What the census does not count".into(),
        String::new(),
        format!(
            "The game's heap's blocks with no site, other modules' among them, marked after round {from} and compared after round {to}. Walked in {} ms{}.",
            text(&now["took_ms"]),
            if truthy(&now["complete"]) { "" } else { ", not to its end" }
        ),
        String::new(),
        "| | After round {from} | After round {to} | A round |".replace("{from}", &from.to_string()).replace("{to}", &to.to_string()),
        "| --- | --- | --- | --- |".into(),
        format!(
            "| Not counted, MB | {} | {} | {} |",
            mb(&then["bytes"]),
            mb(&now["bytes"]),
            signed((number(&now["bytes"]) - number(&then["bytes"])) / 1048576.0 / rounds, 2)
        ),
        format!(
            "| The game's heap, MB | {} | {} | {} |",
            mb(&marked["memory"]["game_heap"]),
            mb(&last["memory"]["game_heap"]),
            signed((heap(last) - heap(marked)) / 1048576.0 / rounds, 2)
        ),
        String::new(),
        format!("Made since the mark and still there: {} blocks, {} MB.", text(&now["made_blocks"]), mb(&now["made_bytes"])),
        String::new(),
        "| Size | Blocks | Grew | Made since | KB grown | Samples |".into(),
        "| --- | --- | --- | --- | --- | --- |".into(),
    ];
    for s in list(&now["sizes"]) {
        let grown = number(&s["grew"]) * number(&s["size"]);
        let samples: Vec<String> = list(&s["samples"])
            .iter()
            .map(|x| {
                let points = list(&x["points"]).iter().map(text).collect::<Vec<_>>().join(", ");
                let text = text(&x["text"]).replace(['|', '`'], ".");
                if points.is_empty() {
                    format!("`{text}`")
                } else {
                    format!("`{text}` ({points})")
                }
            })
            .collect();
        md.push(format!(
            "| {} | {} | {} | {} | {} | {} |",
            text(&s["size"]),
            text(&s["blocks"]),
            signed(number(&s["grew"]), 0),
            text(&s["made"]),
            signed(grown / 1024.0, 0),
            samples.join("<br>")
        ));
    }
    md
}

/// The saves' names, for a save that is not among them.
fn names(saves: &[Save]) -> String {
    let names: Vec<&str> = saves.iter().map(|s| s.name.as_str()).collect();
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
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

/// The report's sections, from the record.
fn sections(record: &Value) -> Vec<String> {
    let played = list(&record["rounds"]);
    let start = &record["startup"];
    let census = truthy(&start["census"]);
    let saves: Vec<String> = list(&record["saves"]).iter().map(text).collect();
    let mut md = vec![
        format!(
            "Saves {}, new games as {}, {} days; {} round{}.",
            saves.join(" and "),
            text(&record["country"]),
            text(&record["days"]),
            played.len(),
            if played.len() == 1 { "" } else { "s" }
        ),
        String::new(),
    ];
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
            "### Where each round's growth goes".into(),
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
            "#### Each heap".into(),
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
            format!("### Leftovers at generation {}", text(&last["generation"])),
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
                "### What holds them".into(),
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
                    "### Who let them go".into(),
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

    let pages = played.last().map_or(Value::Null, |r| r["pages"].clone());
    if !pages.is_null() {
        md.extend([
            String::new(),
            "### What keeps the free pages".into(),
            String::new(),
            format!(
                "The game's heap after the last round, page by page: a page blocks are on stays committed however little of it they use. Walked in {} ms{}.",
                text(&pages["took_ms"]),
                if truthy(&pages["complete"]) { "" } else { ", not to its end" }
            ),
            String::new(),
            "| Pages | MB |".into(),
            "| --- | --- |".into(),
            format!("| Committed | {} |", mb(&pages["committed"])),
            format!("| Used by blocks | {} |", mb(&pages["used"])),
            format!("| Pages no block is on | {} |", mb(&pages["free_pages"])),
            format!("| Free on the pages blocks are on | {} |", mb(&pages["free_on_used_pages"])),
            format!("| of it, kept by leftovers | {} |", mb(&pages["by_leftovers"])),
            String::new(),
            format!(
                "The pages blocks are on, by how full: {}.",
                list(&pages["by_use"]).iter().enumerate().map(|(i, n)| format!("up to {}/8: {}", i + 1, text(n))).collect::<Vec<_>>().join(", ")
            ),
            String::new(),
            "| Site | Function | Blocks | KB | Free KB it keeps | Leftovers |".into(),
            "| --- | --- | --- | --- | --- | --- |".into(),
        ]);
        for s in list(&pages["sites"]) {
            md.push(format!(
                "| {} | {} | {} | {} | {} | {} |",
                text(&s["site"]),
                text(&s["function"]),
                text(&s["blocks"]),
                kb(&s["bytes"]),
                kb(&s["pinned"]),
                text(&s["leftovers"])
            ));
        }
    }

    md.extend(uncounted_section(&played));

    // What the quarantine said last: as it was turned off, or after the last
    // round played when the scenario ended early.
    let kept = if record["quarantine"].is_null() { played.last().map_or(Value::Null, |r| r["quarantine"].clone()) } else { record["quarantine"].clone() };
    if !kept.is_null() {
        md.extend([
            String::new(),
            "### What the quarantine kept".into(),
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
                "#### Written to after a repair let it go".into(),
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
    md
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

    /// The tables of a record without the census, and the census's when it
    /// has them.
    #[test]
    fn sections_write_the_rounds_their_growth_and_the_leftovers() {
        let mib = 1048576;
        let memory = |p: i64| {
            json!({ "private": p * mib, "game_heap": p * mib / 2, "free_in_heaps": 0, "outside_heaps": 0, "framework_heap": mib, "heaps_allocated": p * mib, "handles": 100,
                    "heaps": [{ "handle": "0x1", "game": true, "process": false, "allocated": p * mib, "committed": p * mib }] })
        };
        let rounds: Vec<Value> =
            (1..=3).map(|i| json!({ "round": i, "seconds": 60, "memory": memory(100 + i), "leftovers": null, "watching": null })).collect();
        let mut report = json!({ "saves": ["a", "b"], "country": "FRA", "days": 30, "startup": memory(100), "rounds": rounds });
        let md = sections(&report).join("\n");
        assert!(md.starts_with("Saves a and b, new games as FRA, 30 days; 3 rounds.\n"), "{md}");
        assert!(md.contains("| 2 | 60 | 102.0 | +1.0 | 51.0 | 0.0 | 0.0 | 1.0 | 100 |"), "{md}");
        assert!(md.contains("The least-squares slope over rounds 2 to 3.") && md.contains("| Private bytes | +1.00 |"), "{md}");
        assert!(md.contains("| Handles | 0.0 |") && md.contains("| 0x1 | yes |  | 103.0 | +1.00 | 103.0 | +1.00 |"), "{md}");
        assert!(!md.contains("### Leftovers"), "{md}");

        report["startup"]["census"] = json!(true);
        report["rounds"][2]["leftovers"] = json!({ "generation": 4, "blocks": 2, "bytes": 2048, "sites": [{
            "site": "0x10", "function": "0x20", "trend": "steady", "per_generation": 1024, "bytes": 2048, "blocks": 2,
            "freed_at": [{ "site": "0x30", "function": "0x40", "blocks": 1 }],
            "holders": { "by_data": 0, "by_blocks": 0, "by_leftovers": 1, "by_nothing": 1, "top": [{ "kind": "data", "at": "0x50", "references": 3 }],
                         "watched": true, "lost_with": [{ "owner": "0x60", "owner_function": "0x70", "let_go_at": "0x80", "let_go_function": "0x90", "blocks": 1 }], "unrecorded": 0 } }] });
        report["rounds"][1]["watching"] = json!({ "sites": ["0x10"], "blocks": 2 });
        let md = sections(&report).join("\n");
        assert!(md.contains("### Leftovers at generation 4\n\n2 blocks, 0.0 MB in all."), "{md}");
        assert!(md.contains("| 0x10 | 0x20 | steady | 1 | 2 | 2 | 0x30 in 0x40 (1) |"), "{md}");
        assert!(md.contains("| 0x10 | steady | 2 | 0 | 0 | 1 | 1 | data 0x50 (3) |"), "{md}");
        assert!(
            md.contains("Watched from round 2 on.") && md.contains("| 0x10 | 1 | 0x60 in 0x70 | 0x80 in 0x90 | 1 |\n| 0x10 | 1 | unrecorded | | 0 |"),
            "{md}"
        );
    }

    #[test]
    fn sections_say_what_the_quarantine_kept_when_the_scenario_ended_early() {
        let memory = json!({ "private": 1048576, "game_heap": 0, "free_in_heaps": 0, "outside_heaps": 0, "framework_heap": 0, "handles": 1, "heaps": [] });
        let quarantine = json!({ "checks": 7, "freed": 0, "repairs": [{ "repair": "text box", "blocks": 3, "bytes": 2097152 }], "written": [] });
        let mut report = json!({ "saves": ["a", "b"], "country": "FRA", "days": 30, "startup": memory,
                                 "rounds": [{ "round": 1, "seconds": 60, "memory": memory, "quarantine": quarantine }], "quarantine": null });
        let md = sections(&report).join("\n");
        assert!(md.contains("compared 7 times.") && md.contains("| text box | 3 | 2.0 |") && md.contains("Nothing was written to"));
        report["quarantine"] = json!({ "checks": 9, "freed": 1, "repairs": [], "written": [{ "repair": "text box", "site": "0x1", "function": "0x2", "size": 64,
                                       "offset": 8, "bytes": "00ff", "generation": 3, "found": 4 }] });
        let md = sections(&report).join("\n");
        assert!(md.contains("compared 9 times.") && md.contains("| text box | 0x1 in 0x2 | 64 | 8 | 00ff | 3 | 4 |"));
    }

    #[test]
    fn sections_say_what_the_census_does_not_count_and_what_grew() {
        let mib = 1048576;
        let memory = |heap: u64| json!({ "private": mib, "game_heap": heap * mib, "free_in_heaps": 0, "outside_heaps": 0, "framework_heap": 0, "handles": 1, "heaps": [] });
        let then =
            json!({ "blocks": 10, "bytes": 100 * mib, "marked": false, "made_blocks": 0, "made_bytes": 0, "sizes": [], "complete": true, "took_ms": 2000 });
        let now = json!({ "blocks": 20, "bytes": 130 * mib, "marked": true, "made_blocks": 12, "made_bytes": 31 * mib, "complete": true, "took_ms": 2100,
                          "sizes": [{ "size": 4096, "blocks": 9, "grew": 8, "made": 8,
                                      "samples": [{ "address": "0x1", "hex": "", "text": "ab|c", "points": ["d3d9.dll+0x10"] }, { "address": "0x2", "hex": "", "text": "..", "points": [] }] }] });
        let report = json!({ "saves": ["a", "b"], "country": "FRA", "days": 30, "startup": memory(1000),
                             "rounds": [{ "round": 1, "seconds": 60, "memory": memory(1000) },
                                        { "round": 2, "seconds": 60, "memory": memory(1010), "uncounted": then },
                                        { "round": 3, "seconds": 60, "memory": memory(1020) },
                                        { "round": 4, "seconds": 60, "memory": memory(1030), "uncounted": now }] });
        let md = sections(&report).join(
            "
",
        );
        assert!(md.contains("marked after round 2 and compared after round 4. Walked in 2100 ms."), "{md}");
        assert!(md.contains("| Not counted, MB | 100.0 | 130.0 | +15.00 |") && md.contains("| The game's heap, MB | 1,010.0 | 1,030.0 | +10.00 |"), "{md}");
        assert!(md.contains("Made since the mark and still there: 12 blocks, 31.0 MB."), "{md}");
        assert!(md.contains("| 4096 | 9 | +8 | 8 | +32 | `ab.c` (d3d9.dll+0x10)<br>`..` |"), "{md}");
    }

    #[test]
    fn sections_say_what_keeps_the_free_pages() {
        let mib = 1048576;
        let memory = json!({ "private": mib, "game_heap": 0, "free_in_heaps": 0, "outside_heaps": 0, "framework_heap": 0, "handles": 1, "heaps": [] });
        let pages = json!({ "committed": 3 * mib, "used": 2 * mib, "free_pages": mib / 2, "free_on_used_pages": mib, "by_leftovers": mib / 2,
                            "by_use": [1, 0, 0, 0, 0, 0, 0, 2], "complete": false, "took_ms": 1900,
                            "sites": [{ "site": "0x10", "function": "0x20", "blocks": 3, "bytes": 2048, "pinned": 6144.0, "leftovers": 2 }] });
        let report = json!({ "saves": ["a", "b"], "country": "FRA", "days": 30, "startup": memory,
                             "rounds": [{ "round": 1, "seconds": 60, "memory": memory, "pages": pages }] });
        let md = sections(&report).join("\n");
        assert!(md.contains("Walked in 1900 ms, not to its end."), "{md}");
        assert!(
            md.contains("| Committed | 3.0 |") && md.contains("| Pages no block is on | 0.5 |") && md.contains("| of it, kept by leftovers | 0.5 |"),
            "{md}"
        );
        assert!(md.contains("up to 1/8: 1, up to 2/8: 0") && md.contains("up to 8/8: 2."), "{md}");
        assert!(md.contains("| 0x10 | 0x20 | 3 | 2 | 6 | 2 |"), "{md}");
    }
}
