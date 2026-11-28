use std::{env, fs, path::PathBuf};

#[path = "src/item_id.rs"]
mod item_id;

fn main() {
    println!("cargo:rerun-if-changed=../workshop/item.toml");
    println!("cargo:rerun-if-changed=../Cargo.toml");
    let item = fs::read_to_string("../workshop/item.toml").expect("workshop/item.toml");
    let id: u64 = item_id::parse_published_file_id(&item).expect("workshop/item.toml has no published_file_id = <number>");
    let beta_id: u64 = item_id::parse_beta_published_file_id(&item).unwrap_or(0);
    let cargo = fs::read_to_string("../Cargo.toml").expect("Cargo.toml");
    let codename = item_id::parse_workspace_codename(&cargo);
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("published_file_id.rs"), format!("{id}_u64\n")).unwrap();
    fs::write(out.join("beta_published_file_id.rs"), format!("{beta_id}_u64\n")).unwrap();
    let codename_expr = match codename {
        Some(name) => format!("Some({name:?})\n"),
        None => "None\n".to_string(),
    };
    fs::write(out.join("version_codename.rs"), codename_expr).unwrap();
}
