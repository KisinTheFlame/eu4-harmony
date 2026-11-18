use std::{env, fs, path::PathBuf};

#[path = "src/item_id.rs"]
mod item_id;

fn main() {
    println!("cargo:rerun-if-changed=../workshop/item.toml");
    let item = fs::read_to_string("../workshop/item.toml").expect("workshop/item.toml");
    let id: u64 = item_id::parse_published_file_id(&item).expect("workshop/item.toml has no published_file_id = <number>");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("published_file_id.rs"), format!("{id}_u64\n")).unwrap();
}
