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
//!   an observe point on it must forward at least.
//!
//! Numbers are hex, with or without `0x`. Addresses are RVAs, branch targets
//! too; a RIP-relative operand gets its target and, when text is there, the
//! text.
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
    let mut formatter = IntelFormatter::new();
    formatter.options_mut().set_hex_prefix("0x");
    formatter.options_mut().set_hex_suffix("");
    formatter.options_mut().set_uppercase_hex(false);
    formatter.options_mut().set_branch_leading_zeros(false);
    formatter.options_mut().set_show_branch_size(false);
    formatter.options_mut().set_space_after_operand_separator(true);
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
            note = format!("  [{target:#x}]");
            if let Some(text) = text_at(image, target) {
                note += &format!(" {text:?}");
            }
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
            [rva, n] => disassemble(&image, hex(rva), Some(hex(n))),
            [rva] => disassemble(&image, hex(rva), None),
            _ => panic!("unknown argument {arg:?}"),
        }
    }
}
