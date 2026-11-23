//! Reads the installed game's code, for finding dependency points:
//!
//! ```text
//! cargo xtask disasm [--module <file>] [--game-dir <dir>] ARG...
//! ```
//!
//! Each ARG is one of
//!
//! - `RVA`: the function starting there, to its end (from the exception
//!   directory; a leaf function, which has no entry there, to its `ret`
//!   before padding, at most 40 instructions);
//! - `RVA:N`: N instructions from RVA, which need not start a function;
//! - `bytes:RVA:N`: N bytes from RVA in hex, as dependency points spell them;
//! - `args:RVA`: how many arguments the function at RVA reads, which is what
//!   an observe point on it must forward at least;
//! - `uses:RVA:VALUE...`: the instructions of the function at RVA that carry
//!   one of the VALUEs, as a field offset, a RIP-relative target or an
//!   immediate, with their bytes: the instructions a field's or a global's
//!   dependency point can keep;
//! - `refs:VALUE[:N]`: the first N (20) such instructions anywhere in the
//!   game's functions, for globals and vtables;
//! - `slots:VTABLE:OFFSET...`: the functions a vtable holds at those
//!   offsets;
//! - `vtable:RVA[:N]`: every slot of the vtable at RVA (up to N, or stopping
//!   automatically before non-code pointers or the next code-referenced table,
//!   capped at 128 slots);
//! - `callees:RVA[:depth]`: direct calls and tail jumps out of RVA as a tree
//!   (default depth 1, max 5);
//! - `data:RVA[:N]`: N (0x80) bytes at RVA as hex, ASCII, and relocated
//!   image pointers;
//! - `chain:GLOBAL:OFFSET[:N]`: the first N (5) places that load a pointer
//!   from the global and soon use OFFSET from it, with the bytes from the
//!   load to the use: one dependency point that keeps both;
//! - `callers:RVA[:N]`: the instructions that call the function at RVA, jump
//!   to it or take its address, and each table of code pointers (a vtable,
//!   mostly) that holds it with the instructions that use the table, the
//!   first N (50) of each;
//! - `str:TEXT`: the ASCII strings in the binary's data that hold TEXT (the
//!   rest of the argument, colons and all), each with the instructions that
//!   use it and the pointers to it;
//! - `stri:TEXT`: case-insensitive variant of `str:TEXT`;
//! - `wstr:TEXT`: UTF-16LE wide-string variant of `str:TEXT`.
//!
//! What `refs:`, `callers:`, `str:`, `stri:` and `wstr:` search are the
//! functions in the exception directory: a leaf function, which has no entry
//! there, is not read, so a call it makes as a jump at its end is not found.
//!
//! Numbers are hex, with or without `0x`. Addresses are RVAs, branch targets
//! too; a RIP-relative operand gets its target and, when text is there, the
//! text. For `eu4.exe`, a target named by `docs/game/addresses.toml` gets its
//! name in parentheses.
use eu4_harmony_analysis as analysis;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, Mnemonic, OpKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

type Names = HashMap<usize, String>;

fn hex(s: &str) -> crate::Result<usize> {
    let trimmed = s.trim_start_matches("0x").trim_start_matches("0X");
    usize::from_str_radix(trimmed, 16).map_err(|_| format!("not a hex number: {s:?}"))
}

/// Validates `--module`: must be a file name inside the game folder, not a path.
pub fn validate_module_name(name: &str) -> crate::Result<&str> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) || Path::new(name).is_absolute() {
        return Err(format!("--module {name:?} must be a file name inside the game folder"));
    }
    Ok(name)
}

struct ParsedArgs {
    module: String,
    game_dir: Option<PathBuf>,
    commands: Vec<String>,
}

fn parse_args(args: &[String]) -> crate::Result<ParsedArgs> {
    let mut module = String::from("eu4.exe");
    let mut game_dir = None;
    let mut commands = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--module" => {
                let value = args.get(i + 1).ok_or("--module needs a file name")?;
                module = validate_module_name(value)?.to_string();
                i += 2;
            }
            "--game-dir" => {
                let value = args.get(i + 1).ok_or("--game-dir needs a folder path")?;
                game_dir = Some(PathBuf::from(value));
                i += 2;
            }
            _ => {
                commands.push(args[i].clone());
                i += 1;
            }
        }
    }
    Ok(ParsedArgs { module, game_dir, commands })
}

/// The text at `rva`, when there is some: printable bytes up to a NUL.
fn text_at(image: &[u8], rva: usize) -> Option<String> {
    let bytes = image.get(rva..)?;
    let end = bytes.iter().take(80).position(|&b| b == 0)?;
    let text = &bytes[..end];
    (end > 2 && text.iter().all(|&b| (0x20..0x7f).contains(&b) || b >= 0x80)).then(|| String::from_utf8_lossy(text).into_owned())
}

/// The names the index of the game's addresses gives for `eu4.exe`, by RVA.
fn load_names(module: &str) -> Names {
    if !module.eq_ignore_ascii_case("eu4.exe") {
        return Names::new();
    }
    let Ok(index) = crate::addresses::load(&crate::repo()) else {
        return Names::new();
    };
    index.into_iter().filter(|(_, entry)| !entry.name.is_empty()).map(|(rva, entry)| (rva as usize, entry.name)).collect()
}

/// ` (name)` for an RVA the index names, else nothing.
fn named(names: &Names, rva: usize) -> String {
    names.get(&rva).map(|name| format!(" ({name})")).unwrap_or_default()
}

fn formatter() -> IntelFormatter {
    let mut formatter = IntelFormatter::new();
    formatter.options_mut().set_hex_prefix("0x");
    formatter.options_mut().set_hex_suffix("");
    formatter.options_mut().set_uppercase_hex(false);
    formatter.options_mut().set_branch_leading_zeros(false);
    formatter.options_mut().set_show_branch_size(false);
    formatter.options_mut().set_space_after_operand_separator(true);
    formatter
}

/// An instruction with its bytes, as a dependency point spells them.
fn with_bytes(image: &[u8], formatter: &mut IntelFormatter, instruction: &Instruction) -> String {
    let (start, end) = (instruction.ip() as usize, instruction.next_ip() as usize);
    let bytes: Vec<String> = image[start..end].iter().map(|b| format!("{b:02x}")).collect();
    let mut text = String::new();
    formatter.format(instruction, &mut text);
    format!("{start:8x}: {:<44} {text}", bytes.join(" "))
}

/// The pointer the image holds at `at`, as an RVA.
fn pointer_at(image: &[u8], preferred: u64, at: usize) -> usize {
    u64::from_le_bytes(image[at..at + 8].try_into().unwrap()).wrapping_sub(preferred) as usize
}

/// Every instruction in the game's functions that reaches one of `targets`:
/// one that branches there (`call`, `jmp`) or carries it as an operand
/// (`lea`, `mov`, an immediate). Each comes with the target it reaches and
/// the function (the exception directory's entry) it lies in.
fn reaching(image: &[u8], targets: &[usize]) -> Vec<(usize, usize, Instruction)> {
    let wanted: HashSet<u64> = targets.iter().map(|&t| t as u64).collect();
    let mut out = Vec::new();
    for range in analysis::functions(image) {
        for instruction in analysis::instructions(&image[range.clone()], range.start) {
            let reached = if instruction.op_count() > 0 && instruction.op0_kind() == OpKind::NearBranch64 {
                Some(instruction.near_branch_target()).filter(|t| wanted.contains(t))
            } else {
                analysis::values(&instruction).into_iter().find(|v| wanted.contains(v))
            };
            if let Some(target) = reached {
                out.push((target as usize, range.start, instruction));
            }
        }
    }
    out
}

/// `callers:RVA[:N]`: what calls the function at `rva` or jumps to it, what
/// takes its address, and the tables of code pointers (vtables, for the
/// most part) that hold it, each with what uses the table.
fn callers(image: &[u8], names: &Names, rva: usize, limit: usize) {
    let preferred = analysis::preferred_base(image);
    let pointers = analysis::pointers(image);
    let held: HashSet<usize> = pointers.iter().copied().collect();
    let code: Vec<_> = analysis::sections(image).filter(|(_, code)| *code).map(|(range, _)| range).collect();
    let to_code = |at: usize| held.contains(&at) && code.iter().any(|r| r.contains(&pointer_at(image, preferred, at)));
    // A pointer to the function, and the run of code pointers it lies in.
    // The game has almost no RTTI, so vtables lie one after another with no
    // pointer to a class's description between them; the table holding the
    // pointer begins at the last place in the run, up to the pointer, that
    // code uses (a constructor stores it, a caller loads it), or else where
    // the run begins.
    let runs: Vec<(usize, usize)> = pointers
        .iter()
        .filter(|&&at| pointer_at(image, preferred, at) == rva)
        .map(|&at| {
            let mut run = at;
            while run >= 8 && to_code(run - 8) {
                run -= 8;
            }
            (run, at)
        })
        .collect();
    let mut targets = vec![rva];
    for &(run, at) in &runs {
        targets.extend((run..at + 8).step_by(8));
    }
    let hits = reaching(image, &targets);
    let used: HashSet<usize> = hits.iter().map(|&(target, _, _)| target).collect();
    let slots: Vec<(usize, usize)> = runs.iter().map(|&(run, at)| ((run..at + 8).step_by(8).rev().find(|t| used.contains(t)).unwrap_or(run), at)).collect();
    let direct: Vec<_> = hits.iter().filter(|(target, _, _)| *target == rva).collect();
    let calls = direct.iter().filter(|(_, _, i)| i.mnemonic() == Mnemonic::Call).count();
    let jumps = direct.iter().filter(|(_, _, i)| i.op_count() > 0 && i.op0_kind() == OpKind::NearBranch64 && i.mnemonic() != Mnemonic::Call).count();
    let mut formatter = formatter();
    println!(
        "--- callers {rva:#x}{}: {calls} call(s), {jumps} jump(s), {} other use(s), {} pointer(s) in tables",
        named(names, rva),
        direct.len() - calls - jumps,
        slots.len()
    );
    let show = |hits: Vec<&(usize, usize, Instruction)>, formatter: &mut IntelFormatter| {
        let total = hits.len();
        for (_, start, instruction) in hits.into_iter().take(limit) {
            println!("{} (in {start:#x}{})", with_bytes(image, formatter, instruction), named(names, *start));
        }
        if total > limit {
            println!("  ... {} more", total - limit);
        }
    };
    show(direct, &mut formatter);
    for (table, at) in slots {
        let uses: Vec<_> = hits.iter().filter(|(target, _, _)| *target == table).collect();
        println!("--- table {table:#x}{} + {:#x} (the pointer at {at:#x}): {} use(s) of the table", named(names, table), at - table, uses.len());
        show(uses, &mut formatter);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VtableStop {
    NextTable(usize),
    EndOfCodePointers,
    Capped(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VtableScan {
    /// `(slot_offset, target_rva)`
    slots: Vec<(usize, usize)>,
    stop: Option<VtableStop>,
}

fn scan_vtable(image: &[u8], rva: usize, explicit: Option<usize>) -> crate::Result<VtableScan> {
    const DEFAULT_CAP: usize = 128;
    if rva.checked_add(8).is_none_or(|end| end > image.len()) {
        return Err(format!("{rva:#x} is outside image ({:#x} bytes)", image.len()));
    }
    let preferred = analysis::preferred_base(image);
    if let Some(count) = explicit {
        let mut slots = Vec::new();
        for i in 0..count {
            let Some(offset) = i.checked_mul(8) else { break };
            let Some(at) = rva.checked_add(offset).filter(|&a| a + 8 <= image.len()) else { break };
            slots.push((offset, pointer_at(image, preferred, at)));
        }
        return Ok(VtableScan { slots, stop: None });
    }
    let held: HashSet<usize> = analysis::pointers(image).into_iter().collect();
    let code_ranges: Vec<_> = analysis::sections(image).filter(|(_, code)| *code).map(|(range, _)| range).collect();
    let to_code = |at: usize| {
        at + 8 <= image.len() && held.contains(&at) && {
            let target = pointer_at(image, preferred, at);
            code_ranges.iter().any(|r| r.contains(&target))
        }
    };
    let mut slots = Vec::new();
    while slots.len() < DEFAULT_CAP {
        let offset = slots.len() * 8;
        let at = rva + offset;
        if !to_code(at) {
            break;
        }
        slots.push((offset, pointer_at(image, preferred, at)));
    }
    let stop = if slots.len() > 1 {
        let run_end = rva + slots.len() * 8;
        let candidates: Vec<usize> = (rva + 8..run_end).step_by(8).collect();
        let next_table = reaching(image, &candidates).into_iter().map(|(target, _, _)| target).min();
        if let Some(stop_at) = next_table {
            slots.truncate((stop_at - rva) / 8);
            Some(VtableStop::NextTable(stop_at))
        } else if slots.len() == DEFAULT_CAP && to_code(run_end) {
            Some(VtableStop::Capped(DEFAULT_CAP))
        } else {
            Some(VtableStop::EndOfCodePointers)
        }
    } else {
        Some(VtableStop::EndOfCodePointers)
    };
    Ok(VtableScan { slots, stop })
}

fn print_vtable(image: &[u8], names: &Names, rva: usize, explicit: Option<usize>) -> crate::Result<()> {
    let scan = scan_vtable(image, rva, explicit)?;
    let reason = match scan.stop {
        None => String::new(),
        Some(VtableStop::NextTable(at)) => format!(" (stopped at next table {at:#x})"),
        Some(VtableStop::EndOfCodePointers) => String::from(" (end of code pointers)"),
        Some(VtableStop::Capped(n)) => format!(" (capped at {n} slots)"),
    };
    println!("--- vtable {rva:#x}{}: {} slot(s){reason}", named(names, rva), scan.slots.len());
    for (offset, target) in scan.slots {
        println!("  +{offset:#x}: {target:#x}{}", named(names, target));
    }
    Ok(())
}

/// The range of instructions belonging to `rva`: `.pdata` bounds when present,
/// otherwise a leaf scan up to `ret` followed by `0xcc` (at most 40 instructions).
fn function_span(image: &[u8], rva: usize) -> std::ops::Range<usize> {
    if let Some(range) = analysis::bounds(image, rva) {
        return range;
    }
    let limit_end = rva.saturating_add(40 * 15).min(image.len());
    let mut decoder = Decoder::with_ip(64, &image[rva..limit_end], rva as u64, DecoderOptions::NONE);
    let mut instruction = Instruction::default();
    let mut end = rva;
    for _ in 0..40 {
        if !decoder.can_decode() {
            break;
        }
        decoder.decode_out(&mut instruction);
        end = instruction.next_ip() as usize;
        if instruction.mnemonic() == Mnemonic::Ret && image.get(end) == Some(&0xcc) {
            break;
        }
    }
    rva..end
}

/// Direct `call` targets and out-of-function `jmp` tail calls inside `rva`,
/// deduplicated in order of first appearance.
fn direct_callees(image: &[u8], rva: usize) -> Vec<(usize, bool)> {
    let range = function_span(image, rva);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for instruction in analysis::instructions(&image[range.clone()], range.start) {
        if instruction.is_call_near() {
            let target = instruction.near_branch_target() as usize;
            if seen.insert(target) {
                out.push((target, false));
            }
        } else if instruction.is_jmp_near() {
            let target = instruction.near_branch_target() as usize;
            if !range.contains(&target) && seen.insert(target) {
                out.push((target, true));
            }
        }
    }
    out
}

fn format_callees(image: &[u8], names: &Names, root: usize, max_depth: usize) -> Vec<String> {
    struct Walker<'a> {
        image: &'a [u8],
        names: &'a Names,
        max_depth: usize,
        ancestors: HashSet<usize>,
        expanded: HashSet<usize>,
        lines: Vec<String>,
    }

    impl Walker<'_> {
        fn visit(&mut self, node: usize, depth: usize) {
            self.ancestors.insert(node);
            for (target, tail) in direct_callees(self.image, node) {
                let indent = "  ".repeat(depth);
                let tail_note = if tail { " [tail]" } else { "" };
                let target_name = named(self.names, target);
                if self.ancestors.contains(&target) {
                    self.lines.push(format!("{indent}{target:#x}{target_name}{tail_note} (recursive)"));
                } else if depth < self.max_depth {
                    if !self.expanded.insert(target) {
                        self.lines.push(format!("{indent}{target:#x}{target_name}{tail_note} (shown above)"));
                    } else {
                        self.lines.push(format!("{indent}{target:#x}{target_name}{tail_note}"));
                        self.visit(target, depth + 1);
                    }
                } else {
                    self.lines.push(format!("{indent}{target:#x}{target_name}{tail_note}"));
                }
            }
            self.ancestors.remove(&node);
        }
    }

    let mut walker = Walker {
        image,
        names,
        max_depth,
        ancestors: HashSet::new(),
        expanded: HashSet::from([root]),
        lines: vec![format!("--- callees {root:#x}{} (depth {max_depth})", named(names, root))],
    };
    walker.visit(root, 1);
    walker.lines
}

fn print_callees(image: &[u8], names: &Names, rva: usize, depth: usize) -> crate::Result<()> {
    if !(1..=5).contains(&depth) {
        return Err(format!("callees depth must be 1..=5 (got {depth})"));
    }
    if rva >= image.len() {
        return Err(format!("{rva:#x} is outside image ({:#x} bytes)", image.len()));
    }
    for line in format_callees(image, names, rva, depth) {
        println!("{line}");
    }
    Ok(())
}

fn format_data(image: &[u8], names: &Names, rva: usize, len: usize) -> crate::Result<Vec<String>> {
    if len == 0 {
        return Err("data length must be greater than zero".into());
    }
    if rva >= image.len() {
        return Err(format!("{rva:#x} is outside image ({:#x} bytes)", image.len()));
    }
    let end = rva.saturating_add(len).min(image.len());
    let preferred = analysis::preferred_base(image);
    let relocs: HashSet<usize> = analysis::pointers(image).into_iter().collect();
    let mut lines = vec![format!("--- data {rva:#x} ({:#x} bytes){}", end - rva, named(names, rva))];
    let mut at = rva;
    while at < end {
        let row_end = (at + 8).min(end);
        let chunk = &image[at..row_end];
        let hex_col: String = chunk.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        let ascii: String = chunk.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect();
        let mut ptr_note = String::new();
        if chunk.len() == 8 && relocs.contains(&at) && at + 8 <= image.len() {
            let target = pointer_at(image, preferred, at);
            ptr_note = format!("  -> {target:#x}{}", named(names, target));
            if let Some(text) = text_at(image, target) {
                ptr_note.push_str(&format!(" {text:?}"));
            }
        }
        lines.push(format!("{at:8x}: {hex_col:<23}  |{ascii}|{ptr_note}"));
        at = row_end;
    }
    Ok(lines)
}

fn print_string_matches(image: &[u8], names: &Names, label: &str, text: &str, total: usize, matches: Vec<(usize, String)>) {
    const LIMIT: usize = 20;
    println!("--- {label} {text:?}: {total} string(s)");
    if total > LIMIT {
        println!("  showing the first {LIMIT}");
    }
    let starts: Vec<usize> = matches.iter().map(|(s, _)| *s).collect();
    let preferred = analysis::preferred_base(image);
    let pointers: Vec<(usize, usize)> =
        analysis::pointers(image).into_iter().map(|at| (pointer_at(image, preferred, at), at)).filter(|(to, _)| starts.contains(to)).collect();
    let hits = reaching(image, &starts);
    let mut formatter = formatter();
    for (start, display) in matches {
        let uses: Vec<_> = hits.iter().filter(|(target, _, _)| *target == start).collect();
        let held: Vec<_> = pointers.iter().filter(|(to, _)| *to == start).map(|&(_, at)| format!("{at:#x}")).collect();
        println!("{start:#x} {display:?}: {} use(s) in code, pointed to from [{}]", uses.len(), held.join(", "));
        for (_, function, instruction) in uses.iter().take(LIMIT) {
            println!("    {} (in {function:#x}{})", with_bytes(image, &mut formatter, instruction), named(names, *function));
        }
        if uses.len() > LIMIT {
            println!("    ... {} more", uses.len() - LIMIT);
        }
    }
}

fn find_ascii_strings(image: &[u8], text: &str, case_insensitive: bool) -> crate::Result<(usize, Vec<(usize, String)>)> {
    const LIMIT: usize = 20;
    if text.is_empty() {
        return Err(format!("{} needs search text", if case_insensitive { "stri:" } else { "str:" }));
    }
    let needle = text.as_bytes();
    let mut total = 0usize;
    let mut seen = HashSet::new();
    let mut matches = Vec::new();
    for (range, _) in analysis::sections(image).filter(|(_, code)| !*code) {
        let data = &image[range.clone()];
        for i in 0..data.len().saturating_sub(needle.len() - 1) {
            let matched = if case_insensitive { data[i..i + needle.len()].eq_ignore_ascii_case(needle) } else { data[i..i + needle.len()] == *needle };
            if !matched {
                continue;
            }
            let rel_start = data[..i].iter().rposition(|&b| b == 0).map_or(0, |nul| nul + 1);
            let start = range.start + rel_start;
            if !seen.insert(start) {
                continue;
            }
            total += 1;
            if matches.len() < LIMIT {
                let end = image[start..].iter().position(|&b| b == 0).map_or(image.len(), |n| start + n);
                matches.push((start, String::from_utf8_lossy(&image[start..end]).into_owned()));
            }
        }
    }
    Ok((total, matches))
}

fn is_printable_utf16(unit: u16) -> bool {
    unit != 0x7f && (unit >= 0x20 || matches!(unit, 0x09 | 0x0a | 0x0d))
}

fn find_wide_strings(image: &[u8], text: &str) -> crate::Result<(usize, Vec<(usize, String)>)> {
    const LIMIT: usize = 20;
    const MAX_UNITS: usize = 256;
    if text.is_empty() {
        return Err("wstr: needs search text".into());
    }
    let needle_bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut total = 0usize;
    let mut seen = HashSet::new();
    let mut matches = Vec::new();
    for (range, _) in analysis::sections(image).filter(|(_, code)| !*code) {
        let data = &image[range.clone()];
        for i in 0..data.len().saturating_sub(needle_bytes.len() - 1) {
            if data[i..i + needle_bytes.len()] != *needle_bytes {
                continue;
            }
            let mut rel_start = i;
            let mut back_units = 0usize;
            while rel_start >= 2 && back_units < MAX_UNITS {
                let unit = u16::from_le_bytes([data[rel_start - 2], data[rel_start - 1]]);
                if !is_printable_utf16(unit) {
                    break;
                }
                rel_start -= 2;
                back_units += 1;
            }
            let start = range.start + rel_start;
            if !seen.insert(start) {
                continue;
            }
            total += 1;
            if matches.len() < LIMIT {
                let mut units = Vec::new();
                let mut pos = start;
                while pos + 2 <= range.end && units.len() < MAX_UNITS {
                    let unit = u16::from_le_bytes([image[pos], image[pos + 1]]);
                    if unit == 0 {
                        break;
                    }
                    units.push(unit);
                    pos += 2;
                }
                matches.push((start, String::from_utf16_lossy(&units)));
            }
        }
    }
    Ok((total, matches))
}

/// The function at `rva`, from the exception directory or up to 40
/// instructions for a leaf.
fn function(image: &[u8], rva: usize) -> std::ops::Range<usize> {
    analysis::bounds(image, rva).unwrap_or(rva..(rva + 40 * 15).min(image.len()))
}

fn disassemble(image: &[u8], names: &Names, rva: usize, count: Option<usize>) -> crate::Result<()> {
    if rva >= image.len() {
        return Err(format!("{rva:#x} is outside image ({:#x} bytes)", image.len()));
    }
    // Without an entry (a leaf function) the function ends at a `ret`
    // followed by padding.
    let (end, limit, leaf, header) = match count {
        Some(n) => ((rva + n * 15).min(image.len()), n, false, format!("--- {rva:#x} ({n} instructions){}", named(names, rva))),
        None => match analysis::bounds(image, rva) {
            Some(range) => (range.end, usize::MAX, false, format!("--- {rva:#x}..{:#x} ({:#x} bytes){}", range.end, range.end - rva, named(names, rva))),
            None => ((rva + 40 * 15).min(image.len()), 40, true, format!("--- {rva:#x} (leaf){}", named(names, rva))),
        },
    };
    let mut decoder = Decoder::with_ip(64, &image[rva..end], rva as u64, DecoderOptions::NONE);
    let mut formatter = formatter();
    let (mut instruction, mut out) = (Instruction::default(), String::new());
    println!("{header}");
    for _ in 0..limit {
        if !decoder.can_decode() {
            break;
        }
        decoder.decode_out(&mut instruction);
        out.clear();
        formatter.format(&instruction, &mut out);
        let mut note = String::new();
        if instruction.is_ip_rel_memory_operand() {
            let target = instruction.ip_rel_memory_address() as usize;
            note = format!("  [{target:#x}]{}", named(names, target));
            if let Some(text) = text_at(image, target) {
                note += &format!(" {text:?}");
            }
        } else if instruction.is_call_near() || instruction.is_jmp_near() {
            note = named(names, instruction.near_branch_target() as usize);
        }
        println!("{:8x}: {out}{note}", instruction.ip());
        if leaf && instruction.mnemonic() == Mnemonic::Ret && image.get(instruction.next_ip() as usize) == Some(&0xcc) {
            break;
        }
    }
    Ok(())
}

pub fn run(args: &[String]) -> crate::Result<()> {
    let parsed = parse_args(args)?;
    if parsed.commands.is_empty() {
        return Err("disasm needs at least one argument".into());
    }
    let dir = eu4_harmony_folders::game_dir(parsed.game_dir.as_deref())?;
    let path = dir.join(&parsed.module);
    let file = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let image = analysis::map_file(&file);
    let names = load_names(&parsed.module);
    for arg in &parsed.commands {
        if let Some(text) = arg.strip_prefix("str:") {
            let (total, matches) = find_ascii_strings(&image, text, false)?;
            print_string_matches(&image, &names, "str", text, total, matches);
            continue;
        }
        if let Some(text) = arg.strip_prefix("stri:") {
            let (total, matches) = find_ascii_strings(&image, text, true)?;
            print_string_matches(&image, &names, "stri", text, total, matches);
            continue;
        }
        if let Some(text) = arg.strip_prefix("wstr:") {
            let (total, matches) = find_wide_strings(&image, text)?;
            print_string_matches(&image, &names, "wstr", text, total, matches);
            continue;
        }
        let parts: Vec<&str> = arg.split(':').collect();
        match parts.as_slice() {
            ["bytes", rva, n] => {
                let (rva, n) = (hex(rva)?, hex(n)?);
                let end = rva.checked_add(n).filter(|&e| e <= image.len()).ok_or_else(|| format!("{rva:#x}+{n:#x} is outside image"))?;
                let bytes: Vec<String> = image[rva..end].iter().map(|b| format!("{b:02x}")).collect();
                println!("{rva:#x} {}", bytes.join(" "));
            }
            ["args", rva] => {
                let rva = hex(rva)?;
                match analysis::bounds(&image, rva) {
                    Some(range) => println!("{rva:#x} reads {} argument(s)", analysis::arguments(&image[range], rva)),
                    None => println!("{rva:#x} starts no function in the exception directory"),
                }
            }
            ["uses", rva, values @ ..] => {
                let range = function(&image, hex(rva)?);
                let values: Vec<u64> = values.iter().map(|v| hex(v).map(|n| n as u64)).collect::<crate::Result<_>>()?;
                let mut formatter = formatter();
                println!("--- {:#x}{} uses {values:x?}", range.start, named(&names, range.start));
                for instruction in analysis::instructions(&image[range.clone()], range.start) {
                    if analysis::values(&instruction).iter().any(|v| values.contains(v)) {
                        println!("{}", with_bytes(&image, &mut formatter, &instruction));
                    }
                }
            }
            ["refs", value, rest @ ..] => {
                let value = hex(value)? as u64;
                let limit = rest.first().map_or(Ok(20), |n| hex(n))?;
                let mut formatter = formatter();
                println!("--- refs {value:#x}{}", named(&names, value as usize));
                let hits = analysis::functions(&image).flat_map(|range| {
                    analysis::instructions(&image[range.clone()], range.start)
                        .into_iter()
                        .filter(|i| analysis::values(i).contains(&value))
                        .map(move |i| (range.start, i))
                });
                for (start, instruction) in hits.take(limit) {
                    println!("{} (in {start:#x}{})", with_bytes(&image, &mut formatter, &instruction), named(&names, start));
                }
            }
            ["slots", vtable, offsets @ ..] => {
                let vtable = hex(vtable)?;
                let preferred = analysis::preferred_base(&image);
                for offset in offsets {
                    let offset = hex(offset)?;
                    let at = vtable.checked_add(offset).filter(|&a| a + 8 <= image.len()).ok_or_else(|| format!("{vtable:#x}+{offset:#x} is outside image"))?;
                    let target = pointer_at(&image, preferred, at);
                    println!("{vtable:#x} + {offset:#x}: {target:#x}{}", named(&names, target));
                }
            }
            ["vtable", rva] => print_vtable(&image, &names, hex(rva)?, None)?,
            ["vtable", rva, n] => print_vtable(&image, &names, hex(rva)?, Some(hex(n)?))?,
            ["callees", rva] => print_callees(&image, &names, hex(rva)?, 1)?,
            ["callees", rva, depth] => print_callees(&image, &names, hex(rva)?, hex(depth)?)?,
            ["data", rva] => {
                for line in format_data(&image, &names, hex(rva)?, 0x80)? {
                    println!("{line}");
                }
            }
            ["data", rva, n] => {
                for line in format_data(&image, &names, hex(rva)?, hex(n)?)? {
                    println!("{line}");
                }
            }
            ["callers", rva, rest @ ..] => {
                let limit = rest.first().map_or(Ok(50), |n| hex(n))?;
                callers(&image, &names, hex(rva)?, limit);
            }
            ["chain", global, offset, rest @ ..] => {
                let (global, offset) = (hex(global)? as u64, hex(offset)? as u64);
                let limit = rest.first().map_or(Ok(5), |n| hex(n))?;
                let mut formatter = formatter();
                println!("--- chain [{global:#x}] + {offset:#x}");
                let mut shown = 0;
                'all: for range in analysis::functions(&image) {
                    let code = analysis::instructions(&image[range.clone()], range.start);
                    for (i, load) in code.iter().enumerate() {
                        if load.mnemonic() != Mnemonic::Mov
                            || !load.is_ip_rel_memory_operand()
                            || load.ip_rel_memory_address() != global
                            || load.op_kind(0) != iced_x86::OpKind::Register
                        {
                            continue;
                        }
                        let held = load.op0_register();
                        for (j, next) in code.iter().enumerate().skip(i + 1).take(6) {
                            let uses =
                                next.op_kinds().any(|k| k == iced_x86::OpKind::Memory) && next.memory_base() == held && next.memory_displacement64() == offset;
                            if uses {
                                let (start, end) = (load.ip() as usize, next.next_ip() as usize);
                                let bytes: Vec<String> = image[start..end].iter().map(|b| format!("{b:02x}")).collect();
                                println!("{start:8x}: {}", bytes.join(" "));
                                for k in &code[i..=j] {
                                    println!("    {}", with_bytes(&image, &mut formatter, k));
                                }
                                shown += 1;
                                if shown == limit {
                                    break 'all;
                                }
                                break;
                            }
                            if next.op_kind(0) == iced_x86::OpKind::Register
                                && next.op0_register().full_register() == held.full_register()
                                && !matches!(next.mnemonic(), Mnemonic::Cmp | Mnemonic::Test)
                            {
                                break;
                            }
                        }
                    }
                }
            }
            [rva, n] => disassemble(&image, &names, hex(rva)?, Some(hex(n)?))?,
            [rva] => disassemble(&image, &names, hex(rva)?, None)?,
            _ => return Err(format!("unknown argument {arg:?}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal mapped PE64 buffer with `.text`, `.rdata`, `.pdata`,
    /// and `.reloc` sections for unit-testing disasm helpers without `eu4.exe`.
    fn synthetic_pe(text_bytes: &[u8], rdata_bytes: &[u8], pdata_entries: &[(u32, u32, u32)], relocs_rva: &[u32]) -> Vec<u8> {
        let mut img = vec![0u8; 0x5000];
        let pe = 0x80usize;
        img[0x3c..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
        let opt = pe + 24;
        // 4 sections (.text, .rdata, .pdata, .reloc)
        img[pe + 6..pe + 8].copy_from_slice(&4u16.to_le_bytes());
        img[pe + 20..pe + 22].copy_from_slice(&240u16.to_le_bytes());
        // Preferred image base 0x140000000
        img[opt + 24..opt + 32].copy_from_slice(&0x0001_4000_0000_u64.to_le_bytes());
        // SizeOfImage
        img[opt + 56..opt + 60].copy_from_slice(&0x5000u32.to_le_bytes());
        // DataDirectory[3] = Exception (.pdata at 0x3000)
        img[opt + 112 + 3 * 8..opt + 112 + 3 * 8 + 4].copy_from_slice(&0x3000u32.to_le_bytes());
        img[opt + 112 + 3 * 8 + 4..opt + 112 + 3 * 8 + 8].copy_from_slice(&((pdata_entries.len() * 12) as u32).to_le_bytes());

        // Build base relocations in .reloc at 0x4000 (sorted pages of IMAGE_REL_BASED_DIR64 = 0xa000)
        let mut reloc_pos = 0x4000usize;
        let mut by_page: std::collections::BTreeMap<u32, Vec<u16>> = std::collections::BTreeMap::new();
        for &rva in relocs_rva {
            by_page.entry(rva & !0xfff).or_default().push(0xa000 | (rva as u16 & 0xfff));
        }
        for (page, mut items) in by_page {
            if items.len() % 2 == 1 {
                items.push(0);
            }
            let block_len = (8 + items.len() * 2) as u32;
            img[reloc_pos..reloc_pos + 4].copy_from_slice(&page.to_le_bytes());
            img[reloc_pos + 4..reloc_pos + 8].copy_from_slice(&block_len.to_le_bytes());
            for (i, item) in items.iter().enumerate() {
                img[reloc_pos + 8 + i * 2..reloc_pos + 10 + i * 2].copy_from_slice(&item.to_le_bytes());
            }
            reloc_pos += block_len as usize;
        }
        let reloc_size = (reloc_pos - 0x4000) as u32;
        img[opt + 112 + 5 * 8..opt + 112 + 5 * 8 + 4].copy_from_slice(&0x4000u32.to_le_bytes());
        img[opt + 112 + 5 * 8 + 4..opt + 112 + 5 * 8 + 8].copy_from_slice(&reloc_size.to_le_bytes());

        let write_section = |buf: &mut [u8], idx: usize, name: &[u8; 8], vsize: u32, va: u32, chars: u32| {
            let s = opt + 240 + idx * 40;
            buf[s..s + 8].copy_from_slice(name);
            buf[s + 8..s + 12].copy_from_slice(&vsize.to_le_bytes());
            buf[s + 12..s + 16].copy_from_slice(&va.to_le_bytes());
            buf[s + 36..s + 40].copy_from_slice(&chars.to_le_bytes());
        };
        write_section(&mut img, 0, b".text\0\0\0", 0x1000, 0x1000, 0x6000_0020);
        write_section(&mut img, 1, b".rdata\0\0", 0x1000, 0x2000, 0x4000_0040);
        write_section(&mut img, 2, b".pdata\0\0", 0x1000, 0x3000, 0x4000_0040);
        write_section(&mut img, 3, b".reloc\0\0", 0x1000, 0x4000, 0x4200_0040);

        img[0x1000..0x1000 + text_bytes.len()].copy_from_slice(text_bytes);
        img[0x2000..0x2000 + rdata_bytes.len()].copy_from_slice(rdata_bytes);
        for (i, &(begin, end, unwind)) in pdata_entries.iter().enumerate() {
            let p = 0x3000 + i * 12;
            img[p..p + 4].copy_from_slice(&begin.to_le_bytes());
            img[p + 4..p + 8].copy_from_slice(&end.to_le_bytes());
            img[p + 8..p + 12].copy_from_slice(&unwind.to_le_bytes());
        }
        img
    }

    #[test]
    fn parses_module_and_preserves_string_search_colons() {
        let parsed = parse_args(&["--module".into(), "pops_api.dll".into(), "str:a:b".into(), "stri:--module".into()]).unwrap();
        assert_eq!(parsed.module, "pops_api.dll");
        assert_eq!(parsed.commands, ["str:a:b", "stri:--module"]);
        assert!(validate_module_name("../eu4.exe").is_err());
        assert!(validate_module_name("sub\\eu4.exe").is_err());
        assert!(validate_module_name("C:\\eu4.exe").is_err());
        assert!(validate_module_name("").is_err());
    }

    #[test]
    fn vtable_stops_at_next_code_referenced_table_and_non_code() {
        // Code at 0x1000..0x100a: lea rax, [rip + disp -> 0x2010] (7 bytes: 48 8d 05 09 10 00 00) + ret (c3)
        // Table 1 at 0x2000 (2 slots -> 0x1000, 0x1007), Table 2 at 0x2010 (1 slot -> 0x1000), followed by 0 at 0x2018.
        let text = [0x48, 0x8d, 0x05, 0x09, 0x10, 0x00, 0x00, 0xc3];
        let mut rdata = vec![0u8; 0x30];
        let write_ptr = |buf: &mut [u8], off: usize, rva: u64| buf[off..off + 8].copy_from_slice(&(0x0001_4000_0000 + rva).to_le_bytes());
        write_ptr(&mut rdata, 0x00, 0x1000);
        write_ptr(&mut rdata, 0x08, 0x1007);
        write_ptr(&mut rdata, 0x10, 0x1000);
        let img = synthetic_pe(&text, &rdata, &[(0x1000, 0x1008, 0x2040)], &[0x2000, 0x2008, 0x2010]);

        let scan1 = scan_vtable(&img, 0x2000, None).unwrap();
        assert_eq!(scan1.slots, [(0x0, 0x1000), (0x8, 0x1007)]);
        assert_eq!(scan1.stop, Some(VtableStop::NextTable(0x2010)));

        let scan2 = scan_vtable(&img, 0x2010, None).unwrap();
        assert_eq!(scan2.slots, [(0x0, 0x1000)]);
        assert_eq!(scan2.stop, Some(VtableStop::EndOfCodePointers));
    }

    #[test]
    fn callees_handles_tail_calls_cycles_and_diamonds() {
        // 0x1000 (0x10 bytes): call 0x1010; call 0x1020; ret
        //   at 0x1000: e8 0b 00 00 00 (next=0x1005 -> 0x1010)
        //   at 0x1005: e8 16 00 00 00 (next=0x100a -> 0x1020)
        //   at 0x100a: c3
        // 0x1010 (0x10 bytes): call 0x1020; jmp 0x1000 (tail)
        //   at 0x1010: e8 0b 00 00 00 (next=0x1015 -> 0x1020)
        //   at 0x1015: e9 e6 ff ff ff (next=0x101a -> 0x1000)
        // 0x1020 (0x01 byte): ret (c3)
        let mut text = vec![0xccu8; 0x30];
        text[0x00..0x0b].copy_from_slice(&[0xe8, 0x0b, 0x00, 0x00, 0x00, 0xe8, 0x16, 0x00, 0x00, 0x00, 0xc3]);
        text[0x10..0x1a].copy_from_slice(&[0xe8, 0x0b, 0x00, 0x00, 0x00, 0xe9, 0xe6, 0xff, 0xff, 0xff]);
        text[0x20] = 0xc3;
        let img = synthetic_pe(&text, &[], &[(0x1000, 0x100b, 0x2040), (0x1010, 0x101a, 0x2040), (0x1020, 0x1021, 0x2040)], &[]);
        let names = Names::from([(0x1020, "leaf_fn".into())]);
        let lines = format_callees(&img, &names, 0x1000, 3);
        assert_eq!(
            lines,
            ["--- callees 0x1000 (depth 3)", "  0x1010", "    0x1020 (leaf_fn)", "    0x1000 [tail] (recursive)", "  0x1020 (leaf_fn) (shown above)",]
        );
    }

    #[test]
    fn data_and_string_search_annotate_pointers_and_wide_strings() {
        let mut rdata = vec![0u8; 0x80];
        rdata[0x00..0x08].copy_from_slice(&(0x0001_4000_2010_u64).to_le_bytes());
        rdata[0x10..0x1a].copy_from_slice(b"HelloGame\0");
        let wide: Vec<u8> = "WidePath".encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
        rdata[0x30..0x30 + wide.len()].copy_from_slice(&wide);
        let img = synthetic_pe(&[0xc3], &rdata, &[(0x1000, 0x1001, 0x2040)], &[0x2000]);
        let names = Names::from([(0x2010, "greeting".into())]);

        let lines = format_data(&img, &names, 0x2000, 8).unwrap();
        assert_eq!(lines[0], "--- data 0x2000 (0x8 bytes)");
        assert!(lines[1].contains("-> 0x2010 (greeting) \"HelloGame\""), "{}", lines[1]);

        let (total_i, matches_i) = find_ascii_strings(&img, "hellogame", true).unwrap();
        assert_eq!(total_i, 1);
        assert_eq!(matches_i, [(0x2010, "HelloGame".into())]);

        let (total_w, matches_w) = find_wide_strings(&img, "Path").unwrap();
        assert_eq!(total_w, 1);
        assert_eq!(matches_w, [(0x2030, "WidePath".into())]);
    }
}
