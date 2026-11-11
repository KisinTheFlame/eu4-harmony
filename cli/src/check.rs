//! `eu4-harmony check`: what the framework reads of a mod, read as it reads
//! it, without the game: the manifest, the Lua files, the localisation and
//! where the files are. A mod that passes loads without these problems;
//! `package` checks the same first. The mod is read as the framework reads
//! it (`eu4_harmony_manifest::HarmonyMod`) and its Lua compiled as the Lua
//! host loads it (`eu4_harmony_lua::compile`).
use eu4_harmony_manifest::{HarmonyMod, Layout, Manifest};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// What checking a mod found, and the files it read.
pub struct Checked {
    pub manifest: Option<Manifest>,
    pub layout: Layout,
    /// The Lua files under `eu4-harmony/lua/`, and the `.yml` files under
    /// `eu4-harmony/localisation/`, in name order.
    pub scripts: Vec<PathBuf>,
    pub texts: Vec<PathBuf>,
    /// Each a line, saying where; none when the mod passes.
    pub problems: Vec<String>,
}

/// Checks the mod at `root`.
pub fn check(root: &Path) -> Checked {
    let harmony = root.join("eu4-harmony");
    let mut problems = Vec::new();
    let (manifest, layout) = match HarmonyMod::read(root) {
        Ok(read) => {
            problems.extend(read.problems(eu4_harmony_remote::API_VERSION));
            (Some(read.manifest), read.layout)
        }
        Err(e) => {
            problems.push(e);
            (None, eu4_harmony_manifest::layout(&harmony))
        }
    };
    let (scripts, script_problems) = eu4_harmony_lua::compile(&harmony);
    problems.extend(script_problems.into_iter().map(|p| format!("eu4-harmony/lua: {p}")));
    let (_, text_problems) = eu4_harmony_loc::load_dir(&harmony.join("localisation"));
    problems.extend(text_problems.into_iter().map(|p| format!("eu4-harmony/localisation/{p}")));
    let texts = files(&harmony.join("localisation"), "yml").into_iter().filter(|p| p.parent() == Some(&harmony.join("localisation"))).collect();
    Checked { manifest, layout, scripts, texts, problems }
}

/// The files with `extension` under `dir`, in name order.
fn files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, extension: &str, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for path in entries.filter_map(|e| e.ok()).map(|e| e.path()) {
            if path.is_dir() {
                walk(&path, extension, found);
            } else if path.extension().is_some_and(|x| x == extension) {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, extension, &mut found);
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_problem_is_listed_at_once() {
        let root = std::env::temp_dir().join(format!("eu4h-check-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let write = |file: &str, text: &str| {
            let path = root.join("eu4-harmony").join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write("manifest.toml", "id = \"m\"\napi_version = \"0.1\"");
        write("lua/main.lua", "return {}");
        write("lua/deep/broken.lua", "return {");
        write("localisation/m_l_english.yml", "l_english:\n M_X:0 missing quotes\n");
        write("ui/stray.gui", "x");
        let checked = check(&root);
        let problems = checked.problems.join("\n");
        assert_eq!(checked.problems.len(), 4, "{problems}");
        for part in ["written for api_version 0.1, this Harmony provides", "deep/broken.lua", "m_l_english.yml: line 2", "eu4-harmony/ui/stray.gui: not where"]
        {
            assert!(problems.contains(part), "{part} in {problems}");
        }
        assert_eq!(checked.scripts.len(), 2);
        write("manifest.toml", &format!("id = \"m\"\napi_version = \"{}\"", eu4_harmony_remote::API_VERSION));
        write("lua/deep/broken.lua", "return {}");
        write("localisation/m_l_english.yml", "l_english:\n M_X:0 \"x\"\n");
        fs::remove_file(root.join("eu4-harmony/ui/stray.gui")).unwrap();
        assert_eq!(check(&root).problems, Vec::<String>::new());
        let _ = fs::remove_dir_all(&root);
    }
}
