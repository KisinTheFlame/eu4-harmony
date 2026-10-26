//! Decides whether the running `eu4.exe` is a build the core's addresses fit.
//!
//! Rule (docs/v1.md): the original sections must sit exactly where they do in
//! a registered baseline (extra sections appended by a binary patch are fine),
//! and every dependency point must still hold its expected bytes.
use crate::{memory::Memory, Result};

/// A place in the game image the core relies on.
pub struct Dependency {
    pub rva: usize,
    /// Expected bytes, as space-separated hex.
    pub bytes: &'static str,
    pub what: &'static str,
}

struct Baseline {
    name: &'static str,
    /// (name, virtual address, virtual size) of the original sections.
    sections: &'static [(&'static str, u32, u32)],
}

/// Vanilla 1.37.5 (`9ad3efe1…`). The unrestricted build (`97dd63…`) keeps
/// this layout and appends `.mnutst` and `.mnstate`, so it matches too.
const BASELINES: &[Baseline] = &[Baseline {
    name: "1.37.5",
    sections: &[
        (".text", 0x1000, 0x1b64d78),
        (".rdata", 0x1b66000, 0x44999a),
        (".data", 0x1fb0000, 0x4b4fb4),
        (".pdata", 0x2465000, 0xea1f8),
        ("_RDATA", 0x2550000, 0xf4),
        (".rsrc", 0x2551000, 0x5acb4),
        (".reloc", 0x25ac000, 0x33898),
    ],
}];

fn sections(m: &Memory) -> Result<Vec<(String, u32, u32)>> {
    let pe = m.base + m.u32(m.base + 0x3c)? as usize;
    if m.u32(pe)? != 0x4550 {
        return Err("no PE signature in the game image".into());
    }
    let count = m.u16(pe + 6)? as usize;
    let optional = m.u16(pe + 20)? as usize;
    let table = pe + 24 + optional;
    (0..count.min(96))
        .map(|i| {
            let s = table + 40 * i;
            let raw = m.bytes(s, 8)?;
            let name = String::from_utf8_lossy(&raw).trim_end_matches('\0').to_string();
            Ok((name, m.u32(s + 12)?, m.u32(s + 8)?))
        })
        .collect()
}

fn parse_hex(hex: &str) -> Vec<u8> {
    hex.split_whitespace().map(|b| u8::from_str_radix(b, 16).expect("dependency bytes")).collect()
}

/// Returns the matched baseline's name, or what did not match.
pub fn check(m: &Memory, dependencies: &[&[Dependency]]) -> Result<&'static str> {
    let actual = sections(m)?;
    let baseline = BASELINES
        .iter()
        .find(|b| b.sections.iter().enumerate().all(|(i, (n, va, vs))| actual.get(i).is_some_and(|(an, ava, avs)| an == n && ava == va && avs == vs)))
        .ok_or_else(|| format!("section layout matches no registered baseline: {actual:?}"))?;
    for dep in dependencies.iter().flat_map(|d| d.iter()) {
        let expected = parse_hex(dep.bytes);
        let found = m.bytes(m.at(dep.rva), expected.len())?;
        if found != expected {
            return Err(format!("{} at {:#x} differs from baseline {}", dep.what, dep.rva, baseline.name));
        }
    }
    Ok(baseline.name)
}
