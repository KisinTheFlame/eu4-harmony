//! The index of the game's addresses (docs/plans/address-index.md): every
//! address of `eu4.exe` the research notes and the game's pages name, by its
//! RVA, with what it is, a short name where one is known, and the notes that
//! name it. `cargo xtask addresses` writes it again from the notes, keeping
//! each name; `cargo xtask what <rva>` looks an address up.
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The index, from the repository's root.
const INDEX: &str = "docs/research/addresses.toml";
/// The folders of notes the index covers, from the repository's root.
const NOTES: [&str; 2] = ["docs/research", "docs/game"];
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
    for folder in NOTES {
        let dir = repo.join(folder);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        for path in files {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let name = format!("{}/{}", folder.trim_start_matches("docs/"), path.file_name().unwrap().to_string_lossy());
            out.push((name, text));
        }
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
        index.insert(rva, Entry { kind: field("kind"), name: field("name"), notes });
    }
    Ok(index)
}

/// Writes the index, one address to a line, in the order of their RVAs.
pub fn write(index: &Index) -> String {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut out = String::from(
        "# The index of the game's addresses (docs/plans/address-index.md): every address of eu4.exe\n\
         # the research notes and the game's pages name, by its RVA. kind is function (one begins there),\n\
         # code (inside one), leaf (code the exception directory has no entry for: a leaf function or\n\
         # inside one) or data; name is what it is, in the game's terms, once a note has named it;\n\
         # notes are the notes that name it. `cargo xtask addresses` writes it again from the notes,\n\
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

/// `cargo xtask what <rva>`: the index's entry of the address, or of the
/// function it is in, and each line of the notes, the game's pages and the
/// code that names either.
pub fn what(repo: &Path, rva: &str) -> Result<()> {
    let rva = u64::from_str_radix(rva.trim_start_matches("0x"), 16).map_err(|_| format!("{rva:?} is not an address"))?;
    let index = read(&std::fs::read_to_string(repo.join(INDEX)).map_err(|e| format!("cannot read {INDEX}: {e}"))?)?;
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
    let mut files = Vec::new();
    for folder in ["docs", "core/src", "xtask/src"] {
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
    text: (u64, u64),
    /// Each entry's begin, end and the function it is part of, by begin.
    functions: Vec<(u64, u64, u64)>,
}

impl Image {
    fn read(path: &Path) -> Result<Self> {
        let file = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let u16_at = |o: usize| file.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
        let u32_at = |o: usize| file.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64);
        let bad = || "eu4.exe is not a PE image as expected".to_string();
        let pe = u32_at(0x3c).ok_or_else(bad)? as usize;
        let sections = u16_at(pe + 6).ok_or_else(bad)?;
        let optional = u16_at(pe + 20).ok_or_else(bad)?;
        let first = pe + 24 + optional;
        let mut table = Vec::new();
        for i in 0..sections {
            let s = first + 40 * i;
            let name = file.get(s..s + 8).ok_or_else(bad)?;
            let (size, va, raw) = (u32_at(s + 8).ok_or_else(bad)?, u32_at(s + 12).ok_or_else(bad)?, u32_at(s + 20).ok_or_else(bad)?);
            table.push((name.to_vec(), va, size, raw));
        }
        let at = |rva: u64| table.iter().find(|&&(_, va, size, _)| rva >= va && rva < va + size).map(|&(_, va, _, raw)| (rva - va + raw) as usize);
        let text = table.iter().find(|t| t.0.starts_with(b".text")).map(|t| (t.1, t.1 + t.2)).ok_or_else(bad)?;
        // The exception directory, the fourth of the data directories.
        let (pdata, pdata_size) = (u32_at(pe + 24 + 112 + 3 * 8).ok_or_else(bad)?, u32_at(pe + 24 + 112 + 3 * 8 + 4).ok_or_else(bad)?);
        let base = at(pdata).ok_or_else(bad)?;
        let mut entries = Vec::new();
        for i in 0..(pdata_size / 12) as usize {
            let o = base + 12 * i;
            let (begin, end, unwind) = (u32_at(o).ok_or_else(bad)?, u32_at(o + 4).ok_or_else(bad)?, u32_at(o + 8).ok_or_else(bad)?);
            // A part chained to the one before (UNW_FLAG_CHAININFO) names it
            // after its unwind codes.
            let u = at(unwind).ok_or_else(bad)?;
            let (flags, codes) = (file[u] >> 3, file[u + 2] as usize);
            let chained = (flags & 4 != 0).then(|| u32_at(u + 4 + 2 * ((codes + 1) & !1))).flatten();
            entries.push((begin, end, chained));
        }
        entries.sort();
        let mut first_of = BTreeMap::new();
        let mut functions = Vec::with_capacity(entries.len());
        for (begin, end, chained) in entries {
            let function = chained.map_or(begin, |c| first_of.get(&c).copied().unwrap_or(c));
            first_of.insert(begin, function);
            functions.push((begin, end, function));
        }
        Ok(Self { text, functions })
    }

    /// The function `rva` is in, by the exception directory: none for a
    /// leaf function, which has no entry there.
    fn function(&self, rva: u64) -> Option<u64> {
        let i = self.functions.partition_point(|f| f.0 <= rva).checked_sub(1)?;
        let (begin, end, function) = self.functions[i];
        (rva >= begin && rva < end).then_some(function)
    }

    fn kind(&self, rva: u64) -> &'static str {
        if rva < self.text.0 || rva >= self.text.1 {
            "data"
        } else {
            match self.function(rva) {
                Some(f) if f == rva => "function",
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
            Entry { kind: "function".into(), name: "the map's \"color\" fill".into(), notes: BTreeSet::from(["research/map-modes.md".into()]) },
        );
        index.insert(0x1fd1310, Entry { kind: String::new(), name: String::new(), notes: BTreeSet::from(["research/a.md".into(), "game/b.md".into()]) });
        assert_eq!(read(&write(&index)).unwrap(), index);
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
}
