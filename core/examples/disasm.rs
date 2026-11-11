//! Reads the installed game's code, for finding dependency points:
//!
//! ```text
//! EU4_GAME_DIR=<install dir> cargo run --release -p eu4-harmony-core --features analysis --example disasm -- ARG...
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
//! - `chain:GLOBAL:OFFSET[:N]`: the first N (5) places that load a pointer
//!   from the global and soon use OFFSET from it, with the bytes from the
//!   load to the use: one dependency point that keeps both;
//! - `callers:RVA[:N]`: the instructions that call the function at RVA, jump
//!   to it or take its address, and each table of code pointers (a vtable,
//!   mostly) that holds it with the instructions that use the table, the
//!   first N (50) of each;
//! - `str:TEXT`: the strings in the game's data that hold TEXT (the rest of
//!   the argument, colons and all), each with the instructions that use it
//!   and the pointers to it.
//!
//! What `refs:`, `callers:` and `str:` search are the functions in the
//! exception directory: a leaf function, which has no entry there, is not
//! read, so a call it makes as a jump at its end is not found.
//!
//! Numbers are hex, with or without `0x`. Addresses are RVAs, branch targets
//! too; a RIP-relative operand gets its target and, when text is there, the
//! text. A target the index of the game's addresses names
//! (`docs/research/addresses.toml`) gets its name, in parentheses.
use eu4_harmony_core::analysis;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, Mnemonic, OpKind};
use std::collections::HashSet;

fn hex(s: &str) -> usize {
    usize::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or_else(|_| panic!("not a hex number: {s}"))
}

/// The text at `rva`, when there is some: printable bytes up to a NUL.
fn text_at(image: &[u8], rva: usize) -> Option<String> {
    let bytes = image.get(rva..)?;
    let end = bytes.iter().take(80).position(|&b| b == 0)?;
    let text = &bytes[..end];
    (end > 2 && text.iter().all(|&b| (0x20..0x7f).contains(&b) || b >= 0x80)).then(|| String::from_utf8_lossy(text).into_owned())
}

/// The names the index of the game's addresses gives, by RVA.
fn names() -> &'static std::collections::HashMap<usize, String> {
    static NAMES: std::sync::OnceLock<std::collections::HashMap<usize, String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/research/addresses.toml");
        let Ok(table) = std::fs::read_to_string(path).unwrap_or_default().parse::<toml::Table>() else { return Default::default() };
        table
            .get("addresses")
            .and_then(|a| a.as_table())
            .into_iter()
            .flatten()
            .filter_map(|(rva, entry)| Some((hex(rva), entry.get("name")?.as_str()?.to_string())))
            .collect()
    })
}

/// ` (name)` for an RVA the index names, else nothing.
fn named(rva: usize) -> String {
    names().get(&rva).map(|name| format!(" ({name})")).unwrap_or_default()
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
fn callers(image: &[u8], rva: usize, limit: usize) {
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
        named(rva),
        direct.len() - calls - jumps,
        slots.len()
    );
    let show = |hits: Vec<&(usize, usize, Instruction)>, formatter: &mut IntelFormatter| {
        let total = hits.len();
        for (_, start, instruction) in hits.into_iter().take(limit) {
            println!("{} (in {start:#x}{})", with_bytes(image, formatter, instruction), named(*start));
        }
        if total > limit {
            println!("  ... {} more", total - limit);
        }
    };
    show(direct, &mut formatter);
    for (table, at) in slots {
        let uses: Vec<_> = hits.iter().filter(|(target, _, _)| *target == table).collect();
        println!("--- table {table:#x}{} + {:#x} (the pointer at {at:#x}): {} use(s) of the table", named(table), at - table, uses.len());
        show(uses, &mut formatter);
    }
}

/// `str:TEXT`: the strings in the game's data that hold TEXT, each with the
/// instructions that use it and the pointers to it.
fn strings(image: &[u8], text: &str) {
    const LIMIT: usize = 20;
    let needle = text.as_bytes();
    assert!(!needle.is_empty(), "str: needs a text");
    let mut starts: Vec<usize> = Vec::new();
    for (range, _) in analysis::sections(image).filter(|(_, code)| !*code) {
        let data = &image[range.clone()];
        for at in (0..data.len().saturating_sub(needle.len() - 1)).filter(|&i| data[i..].starts_with(needle)) {
            // The whole string: from after the NUL before it.
            let start = range.start + data[..at].iter().rposition(|&b| b == 0).map_or(0, |nul| nul + 1);
            if starts.last() != Some(&start) {
                starts.push(start);
            }
        }
    }
    println!("--- str {text:?}: {} string(s)", starts.len());
    if starts.len() > LIMIT {
        println!("  showing the first {LIMIT}");
        starts.truncate(LIMIT);
    }
    let preferred = analysis::preferred_base(image);
    let pointers: Vec<(usize, usize)> =
        analysis::pointers(image).into_iter().map(|at| (pointer_at(image, preferred, at), at)).filter(|(to, _)| starts.contains(to)).collect();
    let hits = reaching(image, &starts);
    let mut formatter = formatter();
    for start in starts {
        let end = image[start..].iter().position(|&b| b == 0).map_or(image.len(), |n| start + n);
        let uses: Vec<_> = hits.iter().filter(|(target, _, _)| *target == start).collect();
        let held: Vec<_> = pointers.iter().filter(|(to, _)| *to == start).map(|&(_, at)| format!("{at:#x}")).collect();
        println!("{start:#x} {:?}: {} use(s) in code, pointed to from [{}]", String::from_utf8_lossy(&image[start..end]), uses.len(), held.join(", "));
        for (_, function, instruction) in uses.iter().take(LIMIT) {
            println!("    {} (in {function:#x}{})", with_bytes(image, &mut formatter, instruction), named(*function));
        }
        if uses.len() > LIMIT {
            println!("    ... {} more", uses.len() - LIMIT);
        }
    }
}

/// The function at `rva`, from the exception directory or up to 40
/// instructions for a leaf.
fn function(image: &[u8], rva: usize) -> std::ops::Range<usize> {
    analysis::bounds(image, rva).unwrap_or(rva..(rva + 40 * 15).min(image.len()))
}

fn disassemble(image: &[u8], rva: usize, count: Option<usize>) {
    // Without an entry (a leaf function) the function ends at a `ret`
    // followed by padding.
    let (end, limit, leaf) = match count {
        Some(n) => ((rva + n * 15).min(image.len()), n, false),
        None => match analysis::bounds(image, rva) {
            Some(range) => (range.end, usize::MAX, false),
            None => ((rva + 40 * 15).min(image.len()), 40, true),
        },
    };
    let mut decoder = Decoder::with_ip(64, &image[rva..end], rva as u64, DecoderOptions::NONE);
    let mut formatter = formatter();
    let (mut instruction, mut out) = (Instruction::default(), String::new());
    println!("--- {rva:#x}");
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
            note = format!("  [{target:#x}]{}", named(target));
            if let Some(text) = text_at(image, target) {
                note += &format!(" {text:?}");
            }
        } else if instruction.is_call_near() || instruction.is_jmp_near() {
            note = named(instruction.near_branch_target() as usize);
        }
        println!("{:8x}: {out}{note}", instruction.ip());
        if leaf && instruction.mnemonic() == Mnemonic::Ret && image.get(instruction.next_ip() as usize) == Some(&0xcc) {
            break;
        }
    }
}

fn main() {
    let dir = std::env::var("EU4_GAME_DIR").expect("set EU4_GAME_DIR to the game's install folder");
    let file = std::fs::read(std::path::Path::new(&dir).join("eu4.exe")).expect("read eu4.exe");
    let image = analysis::map_file(&file);
    for arg in std::env::args().skip(1) {
        if let Some(text) = arg.strip_prefix("str:") {
            strings(&image, text);
            continue;
        }
        let parts: Vec<&str> = arg.split(':').collect();
        match parts.as_slice() {
            ["bytes", rva, n] => {
                let (rva, n) = (hex(rva), hex(n));
                let bytes: Vec<String> = image[rva..rva + n].iter().map(|b| format!("{b:02x}")).collect();
                println!("{rva:#x} {}", bytes.join(" "));
            }
            ["args", rva] => {
                let rva = hex(rva);
                match analysis::bounds(&image, rva) {
                    Some(range) => println!("{rva:#x} reads {} argument(s)", analysis::arguments(&image[range], rva)),
                    None => println!("{rva:#x} starts no function in the exception directory"),
                }
            }
            ["uses", rva, values @ ..] => {
                let range = function(&image, hex(rva));
                let values: Vec<u64> = values.iter().map(|v| hex(v) as u64).collect();
                let mut formatter = formatter();
                println!("--- {:#x} uses {values:x?}", range.start);
                for instruction in analysis::instructions(&image[range.clone()], range.start) {
                    if analysis::values(&instruction).iter().any(|v| values.contains(v)) {
                        println!("{}", with_bytes(&image, &mut formatter, &instruction));
                    }
                }
            }
            ["refs", value, rest @ ..] => {
                let value = hex(value) as u64;
                let limit = rest.first().map_or(20, |n| hex(n));
                let mut formatter = formatter();
                println!("--- refs {value:#x}");
                let hits = analysis::functions(&image).flat_map(|range| {
                    analysis::instructions(&image[range.clone()], range.start)
                        .into_iter()
                        .filter(|i| analysis::values(i).contains(&value))
                        .map(move |i| (range.start, i))
                });
                for (start, instruction) in hits.take(limit) {
                    println!("{} (in {start:#x})", with_bytes(&image, &mut formatter, &instruction));
                }
            }
            ["slots", vtable, offsets @ ..] => {
                let vtable = hex(vtable);
                let preferred = analysis::preferred_base(&image);
                for offset in offsets.iter().map(|o| hex(o)) {
                    println!("{vtable:#x} + {offset:#x}: {:#x}", pointer_at(&image, preferred, vtable + offset));
                }
            }
            ["callers", rva, rest @ ..] => callers(&image, hex(rva), rest.first().map_or(50, |n| hex(n))),
            ["chain", global, offset, rest @ ..] => {
                let (global, offset) = (hex(global) as u64, hex(offset) as u64);
                let limit = rest.first().map_or(5, |n| hex(n));
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
            [rva, n] => disassemble(&image, hex(rva), Some(hex(n))),
            [rva] => disassemble(&image, hex(rva), None),
            _ => panic!("unknown argument {arg:?}"),
        }
    }
}
