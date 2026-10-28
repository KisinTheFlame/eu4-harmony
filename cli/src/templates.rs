//! Files `eu4-harmony new` writes. Most are the template mod in
//! `cli/template/` as they are, which the workspace builds and tests, named
//! `template-mod` ("Template Mod", `template_mod`, `TEMPLATE_MOD`); the rest
//! are written here, with `@@crate@@`, `@@lib@@` and `@@sdk@@` substituted.

pub const DESCRIPTOR: &str = include_str!("../template/descriptor.mod");
pub const MANIFEST: &str = include_str!("../template/eu4-harmony/manifest.json");
pub const BUTTON_GUI: &str = include_str!("../template/eu4-harmony/ui/button.gui");
pub const LOC_ENGLISH: &str = include_str!("../template/eu4-harmony/localisation/template_mod_l_english.yml");
pub const LOC_SIMP_CHINESE: &str = include_str!("../template/eu4-harmony/localisation/template_mod_l_simp_chinese.yml");
pub const GITIGNORE: &str = include_str!("../template/.gitignore");
pub const LIB_RS: &str = include_str!("../template/extension/src/lib.rs");

/// The template mod's names, in the forms its files use them.
pub const ID: &str = "template-mod";
pub const NAME: &str = "Template Mod";
pub const LIB: &str = "template_mod";
pub const KEY: &str = "TEMPLATE_MOD";

pub const CARGO_TOML: &str = r#"[package]
name = "@@crate@@"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "@@lib@@"
crate-type = ["cdylib"]

[dependencies]
eu4-harmony-sdk = { path = "@@sdk@@" }

[profile.release]
lto = "thin"
codegen-units = 1

# Not part of any parent workspace.
[workspace]
"#;

pub const CARGO_CONFIG: &str = r#"[build]
target = "x86_64-pc-windows-msvc"

[target.x86_64-pc-windows-msvc]
# Extensions run inside eu4.exe; a static CRT keeps the DLL self-contained.
rustflags = ["-C", "target-feature=+crt-static"]
"#;

/// `template` for the mod `id` named `name`: the template mod's names
/// replaced by its own, and the `@@...@@` values filled in.
pub fn fill(template: &str, id: &str, name: &str, values: &[(&str, &str)]) -> String {
    let lib = id.replace('-', "_");
    let renamed = template.replace(ID, id).replace(LIB, &lib).replace(KEY, &lib.to_ascii_uppercase()).replace(NAME, name);
    values.iter().fold(renamed, |text, (key, value)| text.replace(&format!("@@{key}@@"), value))
}
