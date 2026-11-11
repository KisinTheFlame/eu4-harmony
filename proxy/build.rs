//! Generates one forwarding stub per version.dll export and tells the linker to
//! export it under the original name and ordinal. The stub list comes from
//! `version-exports.txt`, extracted from the System32 version.dll export table.
use std::{env, fmt::Write as _, fs, path::PathBuf};

fn main() {
    assert_eq!(env::var("CARGO_CFG_TARGET_OS").unwrap(), "windows");
    assert_eq!(env::var("CARGO_CFG_TARGET_ARCH").unwrap(), "x86_64");
    println!("cargo:rerun-if-changed=version-exports.txt");
    let list = fs::read_to_string("version-exports.txt").expect("version-exports.txt");
    let mut names = Vec::new();
    for line in list.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("# ") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap().to_string();
        let ordinal: u32 = parts.next().unwrap().parse().expect("ordinal");
        names.push((name, ordinal));
    }
    let count = names.len();

    let mut asm = String::from(".text\n");
    let mut table = String::new();
    for (i, (name, ordinal)) in names.iter().enumerate() {
        // Lazily resolve the real export on first call, preserving the caller's
        // integer argument registers; version.dll takes no floating-point arguments.
        write!(
            asm,
            "\
.globl eu4h_stub_{i}
.p2align 4
eu4h_stub_{i}:
    mov rax, qword ptr [rip + {{table}} + {off}]
    test rax, rax
    jnz 1f
    push rcx
    push rdx
    push r8
    push r9
    sub rsp, 40
    mov ecx, {i}
    call {{resolve}}
    add rsp, 40
    pop r9
    pop r8
    pop rdx
    pop rcx
1:  jmp rax
",
            off = i * 8
        )
        .unwrap();
        let export = if let Some(ordinal_only) = name.strip_prefix('#') {
            format!("eu4h_ordinal_{ordinal_only}=eu4h_stub_{i},@{ordinal},NONAME")
        } else {
            format!("{name}=eu4h_stub_{i},@{ordinal}")
        };
        println!("cargo:rustc-cdylib-link-arg=/EXPORT:{export}");
        writeln!(table, "    {:?},", name).unwrap();
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(
        out.join("stubs.rs"),
        format!(
            "pub const EXPORT_COUNT: usize = {count};\n\
             pub static EXPORT_NAMES: [&str; {count}] = [\n{table}];\n\
             ::core::arch::global_asm!(r#\"\n{asm}\"#, table = sym TABLE, resolve = sym resolve);\n"
        ),
    )
    .unwrap();

    // Harmony's published file id, so that the proxy loads the core from its
    // own Workshop item and no other.
    println!("cargo:rerun-if-changed=../workshop/item.toml");
    let item = fs::read_to_string("../workshop/item.toml").expect("workshop/item.toml");
    let id: u64 = published_file_id(&item).expect("workshop/item.toml has no published_file_id = <number>");
    fs::write(out.join("published_file_id.rs"), format!("{id}_u64\n")).unwrap();
}

/// The `published_file_id = <number>` of `workshop/item.toml`, read without
/// a TOML parser: the file holds that key, comments and blank lines.
fn published_file_id(item: &str) -> Option<u64> {
    item.lines().find_map(|line| {
        let (key, value) = line.split('#').next()?.split_once('=')?;
        (key.trim() == "published_file_id").then(|| value.trim().parse().ok())?
    })
}
