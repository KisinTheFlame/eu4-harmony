//! Static architecture guard for the top-level submodule DAG of
//! `eu4-harmony-core`.
//!
//! Enforces:
//! 1. Every file or directory directly under `core/src/` (other than `lib.rs`)
//!    is registered as an explicit DAG node.
//! 2. The declared production dependency graph is strictly acyclic.
//! 3. Production code in each node only imports `crate::<target>` from its
//!    declared allowed dependency list (unit tests are relaxed; `capabilities`
//!    internals are intentionally not hard-checked).
//! 4. `lib.rs` keeps all top-level submodules private (`mod`, never `pub mod`).
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Explicit allowed dependency list for each of the 16 production DAG nodes.
const PRODUCTION_DAG: &[(&str, &[&str])] = &[
    ("base", &[]),
    ("native", &["base"]),
    ("text", &["base", "native"]),
    ("render", &["base", "native"]),
    ("repairs", &["base", "native"]),
    ("speed", &["base", "native"]),
    ("tweaks", &["base", "native", "render"]),
    ("session", &["base", "native", "render"]),
    ("interface", &["base", "native", "text", "render", "session"]),
    ("world", &["base", "native", "text"]),
    ("capabilities", &["base", "native", "text", "render", "repairs", "tweaks", "session", "interface", "world"]),
    ("lua", &["base", "native", "interface", "session", "world", "capabilities"]),
    ("remote", &["base", "session", "world", "capabilities"]),
    ("framework_ui", &["base", "native", "text", "render", "tweaks", "capabilities"]),
    ("dev", &["base", "native", "text", "render", "session", "repairs", "speed", "world"]),
    (
        "app",
        &[
            "base",
            "native",
            "text",
            "render",
            "repairs",
            "speed",
            "tweaks",
            "session",
            "interface",
            "world",
            "capabilities",
            "lua",
            "remote",
            "framework_ui",
            "dev",
        ],
    ),
];

/// Test-only node allowed dependencies.
const FAKE_GAME_ALLOWED: &[&str] = &["base", "native", "session", "world", "capabilities", "remote"];

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn is_test_only_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name == "tests.rs" || name.ends_with("_tests.rs") || path.components().any(|c| c.as_os_str() == "tests")
}

/// Strips comments, string/char literals, and `#[cfg(test)]`-gated items/blocks
/// so only production Rust tokens remain.
fn strip_non_production(src: &str) -> String {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut cleaned = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        // Line comment.
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Nested block comment.
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let mut depth = 1usize;
            i += 2;
            while i + 1 < len && depth > 0 {
                if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    if bytes[i] == b'\n' {
                        cleaned.push(b'\n');
                    }
                    i += 1;
                }
            }
            cleaned.push(b' ');
            continue;
        }
        // Raw string literal: r"..." or r#"..."# (and br"..." / br#"..."#).
        let raw_start = if bytes[i] == b'r' {
            Some(i + 1)
        } else if i + 1 < len && bytes[i] == b'b' && bytes[i + 1] == b'r' {
            Some(i + 2)
        } else {
            None
        };
        if let Some(mut j) = raw_start {
            let prev_ident = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            if !prev_ident {
                let mut hashes = 0usize;
                while j < len && bytes[j] == b'#' {
                    hashes += 1;
                    j += 1;
                }
                if j < len && bytes[j] == b'"' {
                    j += 1;
                    while j < len {
                        if bytes[j] == b'"' {
                            let mut k = 0;
                            while k < hashes && j + 1 + k < len && bytes[j + 1 + k] == b'#' {
                                k += 1;
                            }
                            if k == hashes {
                                j += 1 + hashes;
                                break;
                            }
                        }
                        if bytes[j] == b'\n' {
                            cleaned.push(b'\n');
                        }
                        j += 1;
                    }
                    cleaned.extend_from_slice(b"\"\"");
                    i = j;
                    continue;
                }
            }
        }
        // Normal string literal.
        if bytes[i] == b'"' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                if bytes[i] == b'\n' {
                    cleaned.push(b'\n');
                }
                i += 1;
            }
            cleaned.extend_from_slice(b"\"\"");
            continue;
        }
        // Character literal (not a lifetime like `'a`).
        if bytes[i] == b'\'' {
            if i + 2 < len && bytes[i + 1] == b'\\' {
                let mut j = i + 2;
                while j < len && bytes[j] != b'\'' && bytes[j] != b'\n' {
                    j += 1;
                }
                if j < len && bytes[j] == b'\'' {
                    cleaned.extend_from_slice(b"''");
                    i = j + 1;
                    continue;
                }
            } else if i + 2 < len && bytes[i + 2] == b'\'' {
                cleaned.extend_from_slice(b"''");
                i += 3;
                continue;
            }
        }
        cleaned.push(bytes[i]);
        i += 1;
    }

    // Remove `#[cfg(test)]` attributes and the single item/block/statement each governs.
    let clen = cleaned.len();
    let mut out = Vec::with_capacity(clen);
    let mut pos = 0;
    while pos < clen {
        if cleaned[pos..].starts_with(b"#[cfg(test)]") {
            pos += b"#[cfg(test)]".len();
            // Skip whitespace and any stacked attributes on the same item.
            loop {
                while pos < clen && cleaned[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if cleaned[pos..].starts_with(b"#[") {
                    let mut b = 0usize;
                    while pos < clen {
                        if cleaned[pos] == b'[' {
                            b += 1;
                        } else if cleaned[pos] == b']' {
                            b -= 1;
                            if b == 0 {
                                pos += 1;
                                break;
                            }
                        }
                        pos += 1;
                    }
                    continue;
                }
                break;
            }
            // Skip the governed item: up to matching `}` or `;` at brace depth 0.
            let mut depth = 0usize;
            let mut saw_brace = false;
            while pos < clen {
                match cleaned[pos] {
                    b'{' => {
                        saw_brace = true;
                        depth += 1;
                        pos += 1;
                    }
                    b'}' => {
                        pos += 1;
                        if depth > 0 {
                            depth -= 1;
                            if saw_brace && depth == 0 {
                                break;
                            }
                        }
                    }
                    b';' if !saw_brace && depth == 0 => {
                        pos += 1;
                        break;
                    }
                    _ => pos += 1,
                }
            }
            out.push(b' ');
            continue;
        }
        out.push(cleaned[pos]);
        pos += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Extracts every top-level target module referenced via `crate::<target>` or
/// `crate::{ ... }` in already-cleaned Rust source.
fn crate_targets(code: &str) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let bytes = code.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 7 <= len {
        if &bytes[i..i + 7] == b"crate::" && (i == 0 || !is_ident_char(bytes[i - 1])) {
            let mut j = i + 7;
            while j < len && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < len && bytes[j] == b'{' {
                // Parse top-level comma-separated segments inside `{ ... }`.
                j += 1;
                let mut depth = 1usize;
                let mut expect_ident = true;
                while j < len && depth > 0 {
                    match bytes[j] {
                        b'{' => {
                            depth += 1;
                            j += 1;
                        }
                        b'}' => {
                            depth -= 1;
                            j += 1;
                        }
                        b',' if depth == 1 => {
                            expect_ident = true;
                            j += 1;
                        }
                        b if depth == 1 && expect_ident && (b.is_ascii_alphabetic() || b == b'_') => {
                            let start = j;
                            while j < len && is_ident_char(bytes[j]) {
                                j += 1;
                            }
                            let ident = &code[start..j];
                            if ident != "self" {
                                targets.insert(ident.to_string());
                            }
                            expect_ident = false;
                        }
                        _ => j += 1,
                    }
                }
                i = j;
                continue;
            } else if j < len && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                let start = j;
                while j < len && is_ident_char(bytes[j]) {
                    j += 1;
                }
                let ident = &code[start..j];
                if ident != "self" {
                    targets.insert(ident.to_string());
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    targets
}

#[test]
fn top_level_nodes_match_dag_manifest() {
    let root = src_root();
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(&root).unwrap().map(Result::unwrap) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "lib.rs" {
            continue;
        }
        let stem = name.strip_suffix(".rs").unwrap_or(&name).to_string();
        actual.insert(stem);
    }
    let mut expected: BTreeSet<String> = PRODUCTION_DAG.iter().map(|(n, _)| (*n).to_string()).collect();
    expected.insert("fake_game".to_string());
    assert_eq!(actual, expected, "top-level entries under core/src/ must match the declared DAG nodes");
}

#[test]
fn production_dag_is_strictly_acyclic() {
    let nodes: BTreeSet<&str> = PRODUCTION_DAG.iter().map(|(n, _)| *n).collect();
    let mut indegree: BTreeMap<&str, usize> = nodes.iter().map(|&n| (n, 0)).collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for &(node, deps) in PRODUCTION_DAG {
        for &dep in deps {
            assert!(nodes.contains(dep), "node `{node}` lists unknown dependency `{dep}`");
            assert_ne!(node, dep, "node `{node}` cannot depend on itself");
            *indegree.get_mut(node).unwrap() += 1;
            dependents.entry(dep).or_default().push(node);
        }
    }
    let mut queue: VecDeque<&str> = indegree.iter().filter(|&(_, &deg)| deg == 0).map(|(&n, _)| n).collect();
    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(nexts) = dependents.get(node) {
            for &next in nexts {
                let deg = indegree.get_mut(next).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(next);
                }
            }
        }
    }
    assert_eq!(visited, nodes.len(), "declared production DAG contains a cycle");
}

#[test]
fn production_imports_respect_dag_edges() {
    let root = src_root();
    let mut violations = Vec::new();
    for &(node, allowed_list) in PRODUCTION_DAG {
        let allowed: BTreeSet<&str> = allowed_list.iter().copied().collect();
        let mut files = Vec::new();
        collect_rs(&root.join(node), &mut files);
        for file in files {
            if is_test_only_file(&file) {
                continue;
            }
            let text = fs::read_to_string(&file).unwrap();
            let cleaned = strip_non_production(&text);
            for target in crate_targets(&cleaned) {
                if target == node {
                    continue;
                }
                if !allowed.contains(target.as_str()) {
                    let rel = file.strip_prefix(&root).unwrap().display();
                    violations.push(format!("{node} ({rel}) -> crate::{target}"));
                }
            }
        }
    }
    assert!(violations.is_empty(), "DAG dependency violations:\n{}", violations.join("\n"));
}

#[test]
fn fake_game_imports_respect_its_allowlist() {
    let root = src_root();
    let allowed: BTreeSet<&str> = FAKE_GAME_ALLOWED.iter().copied().collect();
    let mut files = Vec::new();
    collect_rs(&root.join("fake_game"), &mut files);
    let mut violations = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        let cleaned = strip_non_production(&text);
        for target in crate_targets(&cleaned) {
            if target == "fake_game" {
                continue;
            }
            if !allowed.contains(target.as_str()) {
                let rel = file.strip_prefix(&root).unwrap().display();
                violations.push(format!("fake_game ({rel}) -> crate::{target}"));
            }
        }
    }
    assert!(violations.is_empty(), "fake_game dependency violations:\n{}", violations.join("\n"));
}

#[test]
fn crate_root_exposes_only_bootstrap() {
    let lib = fs::read_to_string(src_root().join("lib.rs")).unwrap();
    let cleaned = strip_non_production(&lib);
    for line in cleaned.lines().map(str::trim) {
        if line.starts_with("pub ") && line.contains(" mod ") {
            panic!("lib.rs must keep top-level submodules private: `{line}`");
        }
        if line.starts_with("pub(crate) mod ") {
            panic!("lib.rs top-level submodules should be private `mod`: `{line}`");
        }
    }
}
