//! Every dependency point of this build, in one place: the framework's own,
//! those of the development tools built in, and those dev.toml chooses at
//! startup. Startup installs [`installed`], all or none; the offline checks
//! and [`Memory::at`](crate::memory::Memory::at) take [`declared`], so that a
//! developer build's points are checked as a player's are. A module's
//! `points` only declares: it changes nothing, and what a tool needs to
//! start, startup does here.
use super::Point;
use crate::memory::Memory;

/// The framework's own points but the speed-ups', which every build
/// installs.
fn framework() -> Vec<Point> {
    let mut all = crate::physfs::points();
    all.extend(crate::files::points());
    all.extend(crate::objects::points());
    all.extend(crate::game_string::points());
    all.extend(crate::game_heap::points());
    all.extend(crate::game::points());
    all.extend(crate::ui::points());
    all.extend(crate::windows::points());
    all.extend(crate::map::points());
    all.extend(crate::saves::points());
    all.extend(crate::repairs::points());
    all.extend(crate::sessions::points());
    all.extend(crate::achievements::points());
    all.extend(crate::unicode::points());
    all.extend(crate::wording::points());
    all.extend(crate::framework_ui::hook::points());
    all.extend(crate::hidden::points());
    all.extend(crate::main_thread::points());
    all.extend(crate::sound::points());
    all
}

/// The points of the development tools built in that check the game's
/// bytes: on whenever the tool is built in.
#[cfg_attr(not(any(feature = "dev-memory", feature = "dev-heap-census")), expect(unused_mut, reason = "only these tools add points"))]
fn tools(mut all: Vec<Point>) -> Vec<Point> {
    #[cfg(feature = "dev-memory")]
    all.extend(crate::memory_account::points());
    #[cfg(feature = "dev-heap-census")]
    all.extend(crate::census::points());
    all
}

/// Every point this build declares, whether startup installs it or not:
/// what the offline checks check, and what the framework may use. The
/// speed-ups' are all among them, those dev.toml leaves to the game too. The
/// `agents-order` test switch's walks are not: they hook the bytes the army
/// searches' walk of the King's children does, in its place, and keep the
/// bytes the searches keep, which these check.
pub(crate) fn declared() -> Vec<Point> {
    let mut all = tools(framework());
    all.extend(crate::speed::points());
    all
}

/// What startup installs in the game whose image is `m`: the framework's
/// points, the tools', those of the speed-ups dev.toml does not leave to the
/// game, the `agents-order` test switch's walks in place of the army
/// searches' walk of the King's children, and the functions dev.toml has the
/// write watch and the call trace observe, which take the bytes the game
/// holds there now and check nothing. The tools that observe those functions
/// start here, before they are hooked.
#[cfg_attr(
    not(any(feature = "dev-write-watch", feature = "dev-call-trace")),
    expect(unused_variables, reason = "only the write watch and the call trace look at the game here")
)]
pub(crate) fn installed(m: Memory) -> Vec<Point> {
    let mut all = tools(framework());
    all.extend(crate::speed::installed());
    #[cfg(feature = "dev-test-switches")]
    if crate::agents_order::chosen() {
        all.retain(|p| !matches!(p.place, super::Place::Rva(crate::army_search::CHILDREN_WALK)));
        all.extend(crate::agents_order::points());
    }
    #[cfg(feature = "dev-write-watch")]
    {
        let watch = crate::write_watch::chosen(&m);
        all.extend(watch.points());
        watch.start();
    }
    #[cfg(feature = "dev-call-trace")]
    {
        let trace = crate::trace::chosen(&m);
        all.extend(trace.points());
        trace.start();
    }
    all
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    /// A source file's module path from the crate's root, `lib.rs` none.
    fn module_of(root: &Path, file: &Path) -> Vec<String> {
        let mut path: Vec<String> = file.strip_prefix(root).unwrap().iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let last = path.pop().unwrap();
        match last.trim_end_matches(".rs") {
            "lib" | "mod" => {}
            name => path.push(name.to_string()),
        }
        path
    }

    /// Each module that declares points has them collected: by its parent
    /// module (or, for a repair, its `REPAIR` that names them; for a
    /// capability group's module of the layer, the catalogue's list of the
    /// groups), whose own points are collected in turn, by the speed-ups'
    /// list (as its `SPEED_UP`), or here. A module's
    /// points that nothing collects are neither installed nor checked, and
    /// would fail only when the framework first used one of them.
    #[test]
    fn every_module_s_points_are_collected() {
        fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().map(std::result::Result::unwrap) {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        // Each module's text, by its path, an inline module's apart.
        let mut texts: BTreeMap<Vec<String>, String> = BTreeMap::new();
        let mut declaring = Vec::new();
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            let module = module_of(&root, file);
            let mut inline: Option<(String, Vec<String>)> = None;
            let mut previous = "";
            for line in text.lines() {
                // A file's tests, at its end, declare nothing of the game's.
                if previous == "#[cfg(test)]" && line.starts_with("mod tests") {
                    break;
                }
                previous = line;
                // An inline module, one level down: `    mod name {`.
                if let Some(name) = line.strip_prefix("mod ").or_else(|| line.strip_prefix("pub mod ")).and_then(|r| r.strip_suffix(" {")) {
                    let mut path = module.clone();
                    path.push(name.to_string());
                    inline = Some((line.to_string(), path));
                    continue;
                }
                if line == "}" {
                    inline = None;
                }
                let here = inline.as_ref().map_or(&module, |(_, path)| path);
                texts.entry(here.clone()).or_default().push_str(&format!("{line}\n"));
                let declares = line
                    .trim_start()
                    .strip_prefix("pub ")
                    .or_else(|| line.trim_start().strip_prefix("pub(super) "))
                    .or_else(|| line.trim_start().strip_prefix("pub(crate) "));
                if declares.is_some_and(|d| d.starts_with("fn points()")) || line.trim_start().starts_with("fn points()") {
                    declaring.push(here.clone());
                }
            }
        }
        // A capability group's module of the layer is collected by the
        // layer, from the catalogue's list of the groups, which names it.
        let catalogue = vec!["catalogue".to_string()];
        let listed = |module: &[String]| match module {
            [game, name] if game == "game" => {
                let list = &texts[&catalogue];
                list.contains(&format!("({name})")) || list.contains(&format!("({name}: Layer)"))
            }
            _ => false,
        };
        let collects = |collector: &[String], module: &[String]| {
            if collector == ["game"] && listed(module) {
                return true;
            }
            let text = &texts[collector];
            (0..module.len()).any(|from| {
                let path = module[from..].join("::");
                // A repair is collected as its `REPAIR`, and a speed-up as its
                // `SPEED_UP`, which names its points.
                text.contains(&format!("{path}::points")) || text.contains(&format!("{path}::REPAIR")) || text.contains(&format!("{path}::SPEED_UP"))
            })
        };
        let this = vec!["points".to_string(), "this_build".to_string()];
        // Every speed-up, wherever its module, is collected in `speed`'s list.
        let speed_ups = vec!["speed".to_string()];
        let mut loose = Vec::new();
        for module in &declaring {
            let mut at = module.clone();
            // Up the parents, each collecting the one below, to this module.
            let reached = loop {
                if collects(&this, &at) || (at != speed_ups && collects(&speed_ups, &at)) {
                    break true;
                }
                let Some(parent) = at.split_last().map(|(_, parent)| parent.to_vec()) else { break false };
                if !declaring.contains(&parent) || !collects(&parent, &at) {
                    break false;
                }
                at = parent;
            };
            if !reached {
                loose.push(module.join("::"));
            }
        }
        assert!(loose.is_empty(), "points nothing collects: {loose:#?}");
    }
}
