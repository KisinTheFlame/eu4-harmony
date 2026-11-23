//! Formats Ghidra decompilation output with RVA identifiers and `addresses.toml` annotations.
use super::bridge::DecompileItemResult;
use crate::addresses::Index;
use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;

/// Formats one decompiled function item for terminal output.
///
/// For `eu4.exe`, `index` supplies known names and `docs/game/` links from
/// `docs/game/addresses.toml`. For other modules, pass an empty `Index`.
pub fn format_item(module: &str, item: &DecompileItemResult, image_base: u64, image_size: u64, index: &Index) -> String {
    let mut out = String::new();
    if !item.ok {
        let _ = writeln!(out, "=== {module} {:#x} ERROR ===", item.requested_rva);
        let _ = writeln!(out, "{}", item.error.as_deref().unwrap_or("unknown decompilation error"));
        return out;
    }

    let entry_rva = item.entry_rva.unwrap_or(item.requested_rva);
    let offset = item.offset_in_function.unwrap_or(0);
    let entry_meta = index.get(&entry_rva);
    let entry_name = entry_meta.map(|e| e.name.as_str()).filter(|n| !n.is_empty());

    let mut header = format!("=== {module} {entry_rva:#x}");
    if offset > 0 {
        let _ = write!(header, " (+{offset:#x} from {:#x})", item.requested_rva);
    } else if offset < 0 {
        let _ = write!(header, " (-{:#x} from {:#x})", offset.unsigned_abs(), item.requested_rva);
    }
    if let Some(name) = entry_name {
        let _ = write!(header, " ({name})");
    }
    header.push_str(" ===");
    let _ = writeln!(out, "{header}");

    if let Some(meta) = entry_meta {
        if !meta.notes.is_empty() {
            let notes: Vec<&str> = meta.notes.iter().map(String::as_str).collect();
            let _ = writeln!(out, "notes: {}", notes.join(", "));
        }
    }

    let raw_sig = item.signature.as_deref().unwrap_or("");
    if !raw_sig.is_empty() {
        let (rewritten_sig, _) = rewrite_code(raw_sig, image_base, image_size);
        let sig_line: String = rewritten_sig.split_whitespace().collect::<Vec<_>>().join(" ");
        let _ = writeln!(out, "signature: {sig_line}");
    } else if let Some(ghidra_name) = item.ghidra_name.as_deref() {
        let (rewritten_name, _) = rewrite_code(ghidra_name, image_base, image_size);
        let _ = writeln!(out, "ghidra: {rewritten_name}");
    }
    for warning in &item.warnings {
        let _ = writeln!(out, "note: {warning}");
    }

    let raw_c = item.c_code.as_deref().unwrap_or("");
    let (rewritten_c, _) = rewrite_code(raw_c, image_base, image_size);

    let mut all_refs: BTreeSet<u64> = BTreeSet::new();
    let mut seen_lbrace = false;
    for line in rewritten_c.lines() {
        let line_rvas = extract_line_rvas(line, image_base, image_size);
        let in_header = !seen_lbrace;
        if line.contains('{') {
            seen_lbrace = true;
        }
        let mut annotations = Vec::new();
        let mut seen_on_line = HashSet::new();
        for rva in line_rvas {
            if rva != entry_rva && index.contains_key(&rva) {
                all_refs.insert(rva);
            }
            if in_header && rva == entry_rva {
                continue;
            }
            if let Some(meta) = index.get(&rva) {
                if !meta.name.is_empty() && seen_on_line.insert(rva) {
                    annotations.push(format!("{rva:#x}: {}", meta.name));
                }
            }
        }
        if annotations.is_empty() {
            let _ = writeln!(out, "{line}");
        } else {
            let _ = writeln!(out, "{line}  // {}", annotations.join("; "));
        }
    }

    if !all_refs.is_empty() {
        let _ = writeln!(out, "--- referenced addresses");
        for rva in all_refs {
            let meta = &index[&rva];
            let notes: Vec<&str> = meta.notes.iter().map(String::as_str).collect();
            match (meta.name.is_empty(), notes.is_empty()) {
                (false, false) => {
                    let _ = writeln!(out, "  {rva:#x}: {} [{}]", meta.name, notes.join(", "));
                }
                (false, true) => {
                    let _ = writeln!(out, "  {rva:#x}: {}", meta.name);
                }
                (true, false) => {
                    let _ = writeln!(out, "  {rva:#x}: [{}]", notes.join(", "));
                }
                (true, true) => {
                    let _ = writeln!(out, "  {rva:#x}");
                }
            }
        }
    }

    out
}

/// Rewrites Ghidra module-VA default identifiers inside `code` to RVA form,
/// leaving string and character literals untouched.
pub fn rewrite_code(code: &str, image_base: u64, image_size: u64) -> (String, Vec<u64>) {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut refs = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            let start = i;
            let quote = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                let cur = bytes[i];
                i += 1;
                if cur == quote {
                    break;
                }
            }
            out.push_str(&code[start..i]);
            continue;
        }

        if is_ident_start(b) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            let ident = &code[start..i];
            if let Some((rewritten, rva)) = rewrite_ident(ident, image_base, image_size) {
                out.push_str(&rewritten);
                refs.push(rva);
            } else {
                out.push_str(ident);
            }
            continue;
        }

        if b == b'0' && i + 2 < bytes.len() && matches!(bytes[i + 1], b'x' | b'X') && bytes[i + 2].is_ascii_hexdigit() {
            let start = i;
            i += 2;
            let hex_start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            let hex_end = i;
            while i < bytes.len() && matches!(bytes[i], b'u' | b'U' | b'l' | b'L') {
                i += 1;
            }
            if let Ok(va) = u64::from_str_radix(&code[hex_start..hex_end], 16) {
                if va >= image_base.saturating_add(0x1000) && va < image_base.saturating_add(image_size) {
                    refs.push(va - image_base);
                }
            }
            out.push_str(&code[start..i]);
            continue;
        }

        if b.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            out.push_str(&code[start..i]);
            continue;
        }

        let ch_len = code[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&code[i..i + ch_len]);
        i += ch_len;
    }

    (out, refs)
}

/// Extracts RVAs referenced on a single already-rewritten line (both `FUN_0x...`
/// rewritten identifiers and bare `0x...` constants falling in module range).
fn extract_line_rvas(line: &str, image_base: u64, image_size: u64) -> Vec<u64> {
    let bytes = line.as_bytes();
    let mut rvas = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            let quote = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                let cur = bytes[i];
                i += 1;
                if cur == quote {
                    break;
                }
            }
            continue;
        }
        if is_ident_start(b) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            let ident = &line[start..i];
            if let Some(rva) = rva_from_rewritten_ident(ident, image_size) {
                rvas.push(rva);
            }
            continue;
        }
        if b == b'0' && i + 2 < bytes.len() && matches!(bytes[i + 1], b'x' | b'X') && bytes[i + 2].is_ascii_hexdigit() {
            i += 2;
            let hex_start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            if let Ok(va) = u64::from_str_radix(&line[hex_start..i], 16) {
                if va >= image_base.saturating_add(0x1000) && va < image_base.saturating_add(image_size) {
                    rvas.push(va - image_base);
                }
            }
            while i < bytes.len() && matches!(bytes[i], b'u' | b'U' | b'l' | b'L') {
                i += 1;
            }
            continue;
        }
        if b.is_ascii_digit() {
            i += 1;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            continue;
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    rvas
}

const VA_PREFIXES: [&str; 5] = ["FUN_", "DAT_", "LAB_", "switchD_", "joined_r0x"];

fn rewrite_ident(ident: &str, image_base: u64, image_size: u64) -> Option<(String, u64)> {
    for prefix in VA_PREFIXES {
        if let Some(rest) = ident.strip_prefix(prefix) {
            if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
                if let Ok(va) = u64::from_str_radix(rest, 16) {
                    if va >= image_base && va < image_base.saturating_add(image_size) {
                        let rva = va - image_base;
                        let rewritten = if prefix == "joined_r0x" { format!("{prefix}{rva:x}") } else { format!("{prefix}{rva:#x}") };
                        return Some((rewritten, rva));
                    }
                }
            }
        }
    }
    if ident.starts_with("PTR_") || ident.starts_with("s_") || ident.starts_with("u_") {
        if let Some((head, tail)) = ident.rsplit_once('_') {
            if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_hexdigit()) {
                if let Ok(va) = u64::from_str_radix(tail, 16) {
                    if va >= image_base && va < image_base.saturating_add(image_size) {
                        let rva = va - image_base;
                        return Some((format!("{head}_{rva:#x}"), rva));
                    }
                }
            }
        }
    }
    None
}

fn rva_from_rewritten_ident(ident: &str, image_size: u64) -> Option<u64> {
    for prefix in VA_PREFIXES {
        if let Some(rest) = ident.strip_prefix(prefix) {
            let hex = if prefix == "joined_r0x" { Some(rest) } else { rest.strip_prefix("0x") };
            if let Some(hex) = hex {
                if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    if let Ok(rva) = u64::from_str_radix(hex, 16) {
                        if rva < image_size {
                            return Some(rva);
                        }
                    }
                }
            }
        }
    }
    if ident.starts_with("PTR_") || ident.starts_with("s_") || ident.starts_with("u_") {
        if let Some((_, tail)) = ident.rsplit_once('_') {
            if let Some(hex) = tail.strip_prefix("0x") {
                if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    if let Ok(rva) = u64::from_str_radix(hex, 16) {
                        if rva < image_size {
                            return Some(rva);
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addresses::Entry;

    #[test]
    fn rewrites_va_identifiers_while_preserving_strings() {
        let base = 0x0001_4000_0000_u64;
        let size = 0x25e_0000u64;
        let code = r#"
void FUN_14008fa80(longlong param_1)
{
  char *s = "FUN_14008f450 should stay literal";
  if (DAT_141fd1310 == 0) {
    FUN_14008f450(PTR_FUN_140001000_141fd1318, s_hello_141abc000);
    goto LAB_14008fa94;
  }
  switchD_14008fab0:
  joined_r0x14008fac0();
  return;
}
"#;
        let (rewritten, refs) = rewrite_code(code, base, size);
        assert!(rewritten.contains("void FUN_0x8fa80("), "{rewritten}");
        assert!(rewritten.contains("\"FUN_14008f450 should stay literal\""), "{rewritten}");
        assert!(rewritten.contains("DAT_0x1fd1310 == 0"), "{rewritten}");
        assert!(rewritten.contains("FUN_0x8f450(PTR_FUN_140001000_0x1fd1318, s_hello_0x1abc000)"), "{rewritten}");
        assert!(rewritten.contains("goto LAB_0x8fa94;"), "{rewritten}");
        assert!(rewritten.contains("switchD_0x8fab0:"), "{rewritten}");
        assert!(rewritten.contains("joined_r0x8fac0();"), "{rewritten}");
        assert!(refs.contains(&0x8f450));
        assert!(refs.contains(&0x1fd1318));
    }

    #[test]
    fn formats_header_inline_annotations_and_referenced_table() {
        let base = 0x0001_4000_0000_u64;
        let size = 0x25e_0000u64;
        let mut index = Index::new();
        index.insert(
            0x8fa80,
            Entry { kind: "function".into(), name: "string insertion".into(), notes: BTreeSet::from(["game/interface/stream-formatting.md".into()]) },
        );
        index.insert(0x8f450, Entry { kind: "leaf".into(), name: "answers no".into(), notes: BTreeSet::from(["game/countries/advisors.md".into()]) });
        index.insert(0x1fd1310, Entry { kind: "data".into(), name: String::new(), notes: BTreeSet::from(["game/program/startup.md".into()]) });

        let item = DecompileItemResult {
            ok: true,
            requested_rva: 0x8fa94,
            error: None,
            entry_rva: Some(0x8fa80),
            offset_in_function: Some(0x14),
            ghidra_name: Some("FUN_14008fa80".into()),
            signature: Some("void FUN_14008fa80(longlong param_1)".into()),
            c_code: Some("void FUN_14008fa80(longlong param_1)\n{\n  FUN_14008f450(param_1, 0x141fd1310);\n}\n".into()),
            warnings: vec![],
        };

        let formatted = format_item("eu4.exe", &item, base, size, &index);
        assert!(formatted.contains("=== eu4.exe 0x8fa80 (+0x14 from 0x8fa94) (string insertion) ==="), "{formatted}");
        assert!(formatted.contains("notes: game/interface/stream-formatting.md"), "{formatted}");
        assert!(formatted.contains("signature: void FUN_0x8fa80(longlong param_1)"), "{formatted}");
        assert!(formatted.contains("FUN_0x8f450(param_1, 0x141fd1310);  // 0x8f450: answers no"), "{formatted}");
        assert!(!formatted.contains("void FUN_0x8fa80(longlong param_1)  //"), "{formatted}");
        assert!(formatted.contains("--- referenced addresses"), "{formatted}");
        assert!(formatted.contains("  0x8f450: answers no [game/countries/advisors.md]"), "{formatted}");
        assert!(formatted.contains("  0x1fd1310: [game/program/startup.md]"), "{formatted}");

        let dll_formatted = format_item("pops_api.dll", &item, base, size, &Index::new());
        assert!(!dll_formatted.contains("answers no"), "{dll_formatted}");
        assert!(!dll_formatted.contains("referenced addresses"), "{dll_formatted}");
    }
}
