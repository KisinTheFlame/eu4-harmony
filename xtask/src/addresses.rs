//! The index of the game's addresses: every address of `eu4.exe` the game's
//! system pages name, by its RVA, with what it is, a short name where one is
//! known, and the pages that
//! name it. `cargo xtask addresses` writes it again from the notes, keeping
//! each name; `cargo xtask what <rva>` looks an address up.
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The index, from the repository's root.
const INDEX: &str = "docs/game/addresses.toml";
/// The size of the 1.37.5 `eu4.exe`'s image: an address is below it.
const IMAGE_SIZE: u64 = 0x25e_0000;
/// Numbers the notes write in backticks as an address would be written,
/// which are none: an address is not taken from them.
const NOT_ADDRESSES: [u64; 3] = [0x186a0, 0x20000, 0x10000];

/// One address's entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// `function` (a function begins there), `code` (inside one), `leaf`
    /// (code with no entry in the exception directory: a leaf function, or
    /// code in one), `data`;
    /// empty where the game was not at hand to tell.
    pub kind: String,
    /// What it is, in the game's terms; empty where no note has named it yet.
    pub name: String,
    /// The notes that name it, from the repository's `docs/`.
    pub notes: BTreeSet<String>,
}

/// The index, by RVA.
pub type Index = BTreeMap<u64, Entry>;

/// The addresses a note's text names: each `0x` number in backticks of five
/// to seven hex digits, below the image's size, not an offset (a `+` or `-`
/// before it) nor one of [`NOT_ADDRESSES`].
pub fn named_in(text: &str) -> BTreeSet<u64> {
    let mut found = BTreeSet::new();
    for (i, span) in text.split('`').enumerate() {
        if i % 2 == 0 {
            continue;
        }
        let bytes = span.as_bytes();
        let mut at = 0;
        while let Some(p) = span[at..].find("0x") {
            let start = at + p;
            let digits = span[start + 2..].bytes().take_while(u8::is_ascii_hexdigit).count();
            at = start + 2 + digits;
            let joined = start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
            let before = span[..start].trim_end().as_bytes().last().copied();
            let offset = matches!(before, Some(b'+' | b'-' | b'*'));
            let after = span.as_bytes().get(at).copied();
            let longer = after.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
            if joined || offset || longer || !(5..=7).contains(&digits) {
                continue;
            }
            let Ok(v) = u64::from_str_radix(&span[start + 2..at], 16) else { continue };
            if (0x1000..IMAGE_SIZE).contains(&v) && !NOT_ADDRESSES.contains(&v) && span[start + 2..at].bytes().all(|b| !b.is_ascii_uppercase()) {
                found.insert(v);
            }
        }
    }
    found
}

/// Every note the index covers, by its path from `docs/`, with its text.
fn notes(repo: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let dir = repo.join("docs/game");
    if !dir.is_dir() {
        return Err(format!("cannot read {}", dir.display()));
    }
    let mut files = Vec::new();
    collect(&dir, &mut files);
    for path in files.into_iter().filter(|p| p.extension().is_some_and(|x| x == "md")) {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let name = path.strip_prefix(repo.join("docs")).unwrap().to_string_lossy().replace('\\', "/");
        out.push((name, text));
    }
    Ok(out)
}

/// The addresses the notes name, each with the notes that name it.
fn named(repo: &Path) -> Result<BTreeMap<u64, BTreeSet<String>>> {
    let mut by = BTreeMap::<u64, BTreeSet<String>>::new();
    for (name, text) in notes(repo)? {
        for rva in named_in(&text) {
            by.entry(rva).or_default().insert(name.clone());
        }
    }
    Ok(by)
}

/// Reads the index: one line `"0x..." = { kind = "...", name = "...", notes = [...] }`
/// for each address, after comments.
pub fn read(text: &str) -> Result<Index> {
    let table: toml::Table = text.parse().map_err(|e| format!("{INDEX}: {e}"))?;
    let mut index = Index::new();
    for (key, value) in table.get("addresses").and_then(|a| a.as_table()).into_iter().flatten() {
        let rva = key.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok()).ok_or_else(|| format!("{INDEX}: {key:?} is not an address"))?;
        let field = |name: &str| value.get(name).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let notes = value.get("notes").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|n| n.as_str().map(String::from)).collect();
        if !valid_notes(&notes) {
            return Err(format!("{INDEX}: {key} must link only to Markdown pages under game/"));
        }
        index.insert(rva, Entry { kind: field("kind"), name: field("name"), notes });
    }
    Ok(index)
}

fn valid_notes(notes: &BTreeSet<String>) -> bool {
    !notes.is_empty()
        && notes.iter().all(|note| {
            note.starts_with("game/") && note.ends_with(".md") && !note.contains('\\') && !note.split('/').any(|part| part == ".." || part.is_empty())
        })
}

/// Writes the index, one address to a line, in the order of their RVAs.
pub fn write(index: &Index) -> String {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut out = String::from(
        "# The index of the game's addresses: every address of eu4.exe named in docs/game/,\n\
         # by its RVA. kind is function (one begins there),\n\
         # code (inside one), leaf (code the exception directory has no entry for: a leaf function or\n\
         # inside one) or data; name is what it is, in the game's terms, once a note has named it;\n\
         # notes link only to the game's system pages, including their topic pages.\n\
         # `cargo xtask addresses` writes it again from those pages,\n\
         # keeping each name and kind; a name is written here by hand.\n\n[addresses]\n",
    );
    for (rva, e) in index {
        let notes: Vec<String> = e.notes.iter().map(|n| quote(n)).collect();
        let mut fields = Vec::new();
        if !e.kind.is_empty() {
            fields.push(format!("kind = {}", quote(&e.kind)));
        }
        if !e.name.is_empty() {
            fields.push(format!("name = {}", quote(&e.name)));
        }
        fields.push(format!("notes = [{}]", notes.join(", ")));
        out.push_str(&format!("\"{rva:#x}\" = {{ {} }}\n", fields.join(", ")));
    }
    out
}

#[cfg(test)]
/// What is wrong between the index and the notes: an address a note names
/// that the index does not list with that note, or a note the index lists
/// that does not name it.
pub fn differences(index: &Index, named: &BTreeMap<u64, BTreeSet<String>>) -> Vec<String> {
    let mut problems = Vec::new();
    for (rva, notes) in named {
        let listed = index.get(rva).map(|e| &e.notes);
        for note in notes.iter().filter(|n| listed.is_none_or(|l| !l.contains(*n))) {
            problems.push(format!("{rva:#x} is named in {note}, and the index does not list it there"));
        }
    }
    for (rva, e) in index {
        for note in e.notes.iter().filter(|n| named.get(rva).is_none_or(|ns| !ns.contains(*n))) {
            problems.push(format!("the index lists {rva:#x} in {note}, which does not name it"));
        }
    }
    problems
}

/// `cargo xtask addresses`: the index written again from the notes, with
/// each name kept, and the kind of each address told from the game's
/// `eu4.exe` where it is found. Names of addresses no note names any more
/// are dropped, and said.
pub fn update(repo: &Path) -> Result<()> {
    let path = repo.join(INDEX);
    let old = match std::fs::read_to_string(&path) {
        Ok(text) => read(&text)?,
        Err(_) => Index::new(),
    };
    let image = crate::game::dir().ok().and_then(|dir| Image::read(&dir.join("eu4.exe")).ok());
    let mut index = Index::new();
    for (rva, notes) in named(repo)? {
        let kept = old.get(&rva).cloned().unwrap_or_default();
        let kind = image.as_ref().map(|i| i.kind(rva).to_string()).unwrap_or(kept.kind);
        index.insert(rva, Entry { kind, name: kept.name, notes });
    }
    for (rva, e) in old.iter().filter(|(rva, e)| !e.name.is_empty() && !index.contains_key(rva)) {
        println!("dropped {rva:#x} ({}), which no note names any more", e.name);
    }
    let named_count = index.values().filter(|e| !e.name.is_empty()).count();
    std::fs::write(&path, write(&index)).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    println!("{} addresses, {named_count} named, in {}", index.len(), path.display());
    Ok(())
}

/// Loads the index of the game's addresses from `docs/game/addresses.toml`.
pub fn load(repo: &Path) -> Result<Index> {
    read(&std::fs::read_to_string(repo.join(INDEX)).map_err(|e| format!("cannot read {INDEX}: {e}"))?)
}

/// `cargo xtask what <rva | search terms>`: looks up one address (and its
/// containing function) or searches the index by name and documentation path.
pub fn what(repo: &Path, query: &[String]) -> Result<()> {
    let index = load(repo)?;
    if let Some(rva) = is_rva_query(query) {
        return what_rva(repo, &index, rva);
    }
    what_search(repo, &index, query)
}

/// Parses `query` as a single RVA (`0x...`, `0X...`, or 5–7 hex digits).
/// Shorter bare hex words such as `cafe` or `dead` are treated as search terms.
pub fn is_rva_query(query: &[String]) -> Option<u64> {
    let [raw] = query else { return None };
    let trimmed = raw.trim();
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return u64::from_str_radix(hex, 16).ok();
        }
        return None;
    }
    if (5..=7).contains(&trimmed.len()) && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return u64::from_str_radix(trimmed, 16).ok();
    }
    None
}

/// Splits `query` on whitespace into ASCII-lowercased search terms.
fn search_terms(query: &[String]) -> Result<Vec<String>> {
    let terms: Vec<String> = query.iter().flat_map(|part| part.split_whitespace()).filter(|t| !t.is_empty()).map(str::to_ascii_lowercase).collect();
    if terms.is_empty() {
        return Err("what needs an address or search terms".into());
    }
    Ok(terms)
}

/// Entries whose `name` or `notes` match every term (case-insensitive AND).
fn matching_entries<'a>(index: &'a Index, terms: &[String]) -> Vec<(u64, &'a Entry)> {
    index
        .iter()
        .filter(|(_, entry)| {
            let name = entry.name.to_ascii_lowercase();
            let notes: Vec<String> = entry.notes.iter().map(|n| n.to_ascii_lowercase()).collect();
            terms.iter().all(|term| name.contains(term) || notes.iter().any(|n| n.contains(term)))
        })
        .map(|(&rva, entry)| (rva, entry))
        .collect()
}

fn what_search(repo: &Path, index: &Index, query: &[String]) -> Result<()> {
    const LIMIT: usize = 50;
    let terms = search_terms(query)?;
    let joined = query.join(" ");
    let matches = matching_entries(index, &terms);
    match matches.as_slice() {
        [] => println!("no address matches {joined:?}"),
        [(rva, _)] => return what_rva(repo, index, *rva),
        _ => {
            println!("{} address(es) match {joined:?}:", matches.len());
            for (rva, entry) in matches.iter().take(LIMIT) {
                let addr = format!("{rva:#x}");
                let kind = if entry.kind.is_empty() { "-" } else { entry.kind.as_str() };
                let name = if entry.name.is_empty() { "-" } else { entry.name.as_str() };
                let notes: Vec<&str> = entry.notes.iter().map(String::as_str).collect();
                println!("{addr:<9}  {kind:<8}  {name}  [{}]", notes.join(", "));
            }
            if matches.len() > LIMIT {
                println!("  ... {} more (narrow the search terms)", matches.len() - LIMIT);
            }
        }
    }
    Ok(())
}

/// Looks up one RVA in the index, its explanations in the game's system
/// pages, and then code references. Investigations are reached through the
/// system pages.
fn what_rva(repo: &Path, index: &Index, rva: u64) -> Result<()> {
    let function = crate::game::dir().ok().and_then(|dir| Image::read(&dir.join("eu4.exe")).ok()).and_then(|i| i.function(rva));
    let mut asked = vec![rva];
    match function {
        Some(f) if f != rva => {
            println!("{rva:#x} is inside the function at {f:#x}, {:#x} into it", rva - f);
            asked.push(f);
        }
        _ => {}
    }
    for a in &asked {
        match index.get(a) {
            Some(e) => println!(
                "{a:#x}: {}{}{}",
                if e.kind.is_empty() { String::new() } else { format!("{}, ", e.kind) },
                if e.name.is_empty() { "not named yet".to_string() } else { e.name.clone() },
                if e.notes.is_empty() { String::new() } else { format!(" (in {})", e.notes.iter().cloned().collect::<Vec<_>>().join(", ")) }
            ),
            None => println!("{a:#x}: not in the index"),
        }
    }
    let needles: Vec<String> = asked.iter().map(|a| format!("{a:#x}")).collect();
    let pages: BTreeSet<&String> = asked.iter().filter_map(|a| index.get(a)).flat_map(|e| &e.notes).collect();
    for page in pages {
        let path = repo.join("docs").join(page);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        for (line, explanation) in explanations(&text, &needles) {
            println!("\ndocs/{page}:{line}\n{explanation}");
        }
    }
    let mut files = Vec::new();
    for folder in ["analysis/src", "core/src", "xtask/src"] {
        collect(&repo.join(folder), &mut files);
    }
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        for (n, line) in text.lines().enumerate() {
            for needle in &needles {
                if let Some(p) = mentions(line, needle) {
                    let shown = excerpt(line, p, 300);
                    println!("\n{}:{}\n  {shown}", file.strip_prefix(repo).unwrap_or(&file).display(), n + 1);
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Matching paragraphs with their section, so lookup preserves the game's
/// explanation rather than presenting isolated hits from incident records.
fn explanations(text: &str, needles: &[String]) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    let mut heading = "";
    let mut at = 0;
    while at < lines.len() {
        if lines[at].starts_with('#') {
            heading = lines[at];
        }
        if lines[at].is_empty() {
            at += 1;
            continue;
        }
        let start = at;
        while at < lines.len() && !lines[at].is_empty() {
            at += 1;
        }
        let paragraph = lines[start..at].join("\n");
        if let Some(position) = needles.iter().find_map(|needle| mentions(&paragraph, needle)) {
            let context = excerpt(&paragraph, position, 2400);
            found.push((start + 1, if paragraph.starts_with(heading) { context } else { format!("{heading}\n\n{context}") }));
        }
    }
    found
}

/// Where `line` names the address `needle` as a whole number.
fn mentions(line: &str, needle: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(p) = line[from..].find(needle) {
        let at = from + p;
        let end = at + needle.len();
        let before = line.as_bytes().get(at.wrapping_sub(1)).copied();
        let after = line.as_bytes().get(end).copied();
        let part = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !part(before) && !part(after) {
            return Some(at);
        }
        from = end;
    }
    None
}

/// The text of `line` around byte `at`, at most `width` bytes of it, cut at
/// characters.
fn excerpt(line: &str, at: usize, width: usize) -> String {
    let line = line.trim();
    if line.len() <= width {
        return line.to_string();
    }
    let at = at.min(line.len());
    let mut start = at.saturating_sub(width / 2);
    while !line.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + width).min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}{}", if start > 0 { "…" } else { "" }, &line[start..end], if end < line.len() { "…" } else { "" })
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|x| x == "md" || x == "rs") {
            out.push(path);
        }
    }
}

/// What of `eu4.exe` tells an address's kind: its code's bounds and its
/// functions, from the exception directory, each part chained to the one
/// before folded into the function it is part of.
struct Image {
    text: std::ops::Range<u32>,
    exceptions: eu4_harmony_analysis::ExceptionTable,
}

impl Image {
    fn read(path: &Path) -> Result<Self> {
        let file = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let bad = || "eu4.exe is not a PE image as expected".to_string();
        let pe = eu4_harmony_analysis::PeImage::from_file(file).ok_or_else(bad)?;
        let text = pe.section_bounds(b".text").ok_or_else(bad)?;
        let exceptions = eu4_harmony_analysis::ExceptionTable::from_image(&pe).ok_or_else(bad)?;
        Ok(Self { text, exceptions })
    }

    /// The function `rva` is in, by the exception directory: none for a
    /// leaf function, which has no entry there.
    fn function(&self, rva: u64) -> Option<u64> {
        u32::try_from(rva).ok().and_then(|r| self.exceptions.primary(r)).map(u64::from)
    }

    fn kind(&self, rva: u64) -> &'static str {
        let Ok(rva32) = u32::try_from(rva) else {
            return "data";
        };
        if !self.text.contains(&rva32) {
            "data"
        } else {
            match self.exceptions.primary(rva32) {
                Some(f) if f == rva32 => "function",
                Some(_) => "code",
                // No entry of its own: a leaf function, or code in one.
                None => "leaf",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_told_from_an_offset_and_a_number() {
        let text = "`0x12fd9b0`, `[0x1fd1310]`, `[[idler + 0x330] + 0x360]`, `graphics+0x32690`, `0x2e10`, `0x186a0`, `0x2aaaaaaaaaaaaaab`, `rbx+0x1270`, `0x12DD0C0`; 0x8f440 outside";
        assert_eq!(named_in(text), BTreeSet::from([0x12fd9b0, 0x1fd1310]));
    }

    #[test]
    fn the_index_reads_what_it_writes() {
        let mut index = Index::new();
        index.insert(
            0x12fd9b0,
            Entry { kind: "function".into(), name: "the map's \"color\" fill".into(), notes: BTreeSet::from(["game/provinces/map-modes.md".into()]) },
        );
        index.insert(
            0x1fd1310,
            Entry { kind: String::new(), name: String::new(), notes: BTreeSet::from(["game/interface/a.md".into(), "game/program/b.md".into()]) },
        );
        assert_eq!(read(&write(&index)).unwrap(), index);
    }

    #[test]
    fn the_index_rejects_evidence_pages_and_paths_outside_game() {
        for note in ["research/crash.md", "game/../research/crash.md", "game/a.rs", "game//a.md", "game\\a.md"] {
            let text = format!("[addresses]\n\"0x12345\" = {{ notes = [{note:?}] }}");
            assert!(read(&text).is_err(), "accepted {note}");
        }
    }

    #[test]
    fn lookup_keeps_the_system_explanation_and_its_section() {
        let text = "# Text\n\n## Removing colors\n\n`0x17046a0` walks the source.\nIt removes a marker and its selector.\n\nUnrelated text.\n";
        assert_eq!(
            explanations(text, &["0x17046a0".into()]),
            vec![(5, "## Removing colors\n\n`0x17046a0` walks the source.\nIt removes a marker and its selector.".into())]
        );
    }

    /// Every address the notes name is in the index with each note that
    /// names it, and the index lists no note that does not.
    #[test]
    fn the_index_lists_every_address_the_notes_name() {
        let repo = crate::repo();
        let index = read(&std::fs::read_to_string(repo.join(INDEX)).unwrap()).unwrap();
        let problems = differences(&index, &named(&repo).unwrap());
        assert!(problems.is_empty(), "run cargo xtask addresses, then name what is new:\n{}", problems.join("\n"));
    }

    #[test]
    fn rva_query_distinguishes_addresses_from_short_words() {
        let s = |v: &str| vec![v.to_string()];
        assert_eq!(is_rva_query(&s("0x8f450")), Some(0x8f450));
        assert_eq!(is_rva_query(&s("0X1000")), Some(0x1000));
        assert_eq!(is_rva_query(&s("8f450")), Some(0x8f450));
        assert_eq!(is_rva_query(&s("12fd9b0")), Some(0x12fd9b0));
        assert_eq!(is_rva_query(&s("cafe")), None);
        assert_eq!(is_rva_query(&s("dead")), None);
        assert_eq!(is_rva_query(&s("advisor")), None);
        assert_eq!(is_rva_query(&s("12345678")), None);
        assert_eq!(is_rva_query(&["0x8f450".into(), "extra".into()]), None);
        assert_eq!(is_rva_query(&["color".into(), "fill".into()]), None);
    }

    #[test]
    fn keyword_search_matches_name_and_notes_with_and_logic() {
        let mut index = Index::new();
        index.insert(
            0x12fd9b0,
            Entry { kind: "function".into(), name: "the map's color fill".into(), notes: BTreeSet::from(["game/provinces/map-modes.md".into()]) },
        );
        index.insert(0x8f450, Entry { kind: "leaf".into(), name: "answers no".into(), notes: BTreeSet::from(["game/countries/advisors.md".into()]) });
        index.insert(0x8f440, Entry { kind: "leaf".into(), name: "a leader is one".into(), notes: BTreeSet::from(["game/countries/advisors.md".into()]) });

        let hits = matching_entries(&index, &search_terms(&["COLOR".into(), "fill".into()]).unwrap());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 0x12fd9b0);

        let hits = matching_entries(&index, &search_terms(&["advisor".into()]).unwrap());
        assert_eq!(hits.iter().map(|(rva, _)| *rva).collect::<Vec<_>>(), [0x8f440, 0x8f450]);

        let hits = matching_entries(&index, &search_terms(&["advisor".into(), "leader".into()]).unwrap());
        assert_eq!(hits.iter().map(|(rva, _)| *rva).collect::<Vec<_>>(), [0x8f440]);
    }
}
