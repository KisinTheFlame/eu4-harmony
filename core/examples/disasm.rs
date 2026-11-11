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
//!   load to the use: one dependency point that keeps both.
//!
//! Numbers are hex, with or without `0x`. Addresses are RVAs, branch targets
//! too; a RIP-relative operand gets its target and, when text is there, the
//! text. A target the index of the game's addresses names
//! (`docs/research/addresses.toml`) gets its name, in parentheses.
use eu4_harmony_core::analysis;
use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, Mnemonic};

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
                let preferred = u64::from_le_bytes(image[analysis::pe(&image) + 24 + 24..][..8].try_into().unwrap());
                for offset in offsets.iter().map(|o| hex(o)) {
                    let at = vtable + offset;
                    let target = u64::from_le_bytes(image[at..at + 8].try_into().unwrap()).wrapping_sub(preferred);
                    println!("{vtable:#x} + {offset:#x}: {target:#x}");
                }
            }
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
