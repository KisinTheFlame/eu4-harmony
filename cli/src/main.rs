//! `eu4-harmony`: create, build, register and package EU4 Harmony mods.
//!
//! A mod is a folder with `descriptor.mod`, `eu4-harmony/manifest.json`, its
//! texts in `eu4-harmony/localisation/` and an `extension/` Rust crate; game
//! resources (`interface/`, `gfx/`, ...) sit next to them as in any Paradox mod.
mod templates;

use eu4_harmony_abi as abi;
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, String>;

const USAGE: &str = "\
eu4-harmony: create, build, register and package EU4 Harmony mods

usage:
  eu4-harmony new <id> [--name <display name>] [--dir <parent dir>] [--sdk <sdk dir>]
      Creates the mod folder <id>/ with a Rust extension, one button and its
      texts in English and Chinese.
  eu4-harmony build [<mod dir>] [--debug]
      Compiles the extension and copies the DLL to where the manifest says.
  eu4-harmony link [<mod dir>] [--user-dir <EU4 user dir>]
      Registers the mod folder with the Paradox launcher (enable it in a playset).
  eu4-harmony package [<mod dir>] [--out <dir>]
      Writes a clean folder for upload: no sources, no build output.

<mod dir> defaults to the current folder or the mod folder containing it.
";

use eu4_harmony_manifest::{descriptor_entry, Manifest, LOCATION_KEYS};

/// Positional arguments and `--key value` / `--flag` options.
struct Args {
    positional: Vec<String>,
    options: HashMap<String, Option<String>>,
}

impl Args {
    fn parse(raw: &[String], flags: &[&str]) -> Result<Self> {
        let mut args = Args { positional: Vec::new(), options: HashMap::new() };
        let mut i = 0;
        while i < raw.len() {
            if let Some(key) = raw[i].strip_prefix("--") {
                if flags.contains(&key) {
                    args.options.insert(key.to_string(), None);
                } else {
                    let value = raw.get(i + 1).ok_or_else(|| format!("--{key} needs a value"))?;
                    args.options.insert(key.to_string(), Some(value.clone()));
                    i += 1;
                }
            } else {
                args.positional.push(raw[i].clone());
            }
            i += 1;
        }
        Ok(args)
    }

    fn value(&self, key: &str) -> Option<&str> {
        self.options.get(key).and_then(|v| v.as_deref())
    }

    fn flag(&self, key: &str) -> bool {
        self.options.contains_key(key)
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let result = match raw.first().map(String::as_str) {
        Some("new") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_new(&a)),
        Some("build") => Args::parse(&raw[1..], &["debug"]).and_then(|a| cmd_build(&a)),
        Some("link") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_link(&a)),
        Some("package") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_package(&a)),
        Some("help" | "-h" | "--help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ---------- shared ----------

/// A mod id names the manifest entry, log file and element prefix.
fn valid_id(id: &str) -> bool {
    id.len() <= 64
        && id.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && !id.ends_with('-')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Absolute path without the `\\?\` prefix `canonicalize` adds on Windows.
fn plain(path: &Path) -> Result<PathBuf> {
    let full = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = full.to_string_lossy();
    Ok(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()))
}

fn forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The mod folder at or above `start`.
fn mod_root(start: &Path) -> Result<PathBuf> {
    let start = plain(start)?;
    let mut dir = Some(start.as_path());
    while let Some(d) = dir {
        if d.join("descriptor.mod").is_file() && d.join("eu4-harmony").join("manifest.json").is_file() {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }
    Err(format!("{} is not inside an EU4 Harmony mod (no descriptor.mod with eu4-harmony/manifest.json)", start.display()))
}

fn start_dir(args: &Args) -> Result<PathBuf> {
    match args.positional.first() {
        Some(p) => Ok(PathBuf::from(p)),
        None => std::env::current_dir().map_err(|e| e.to_string()),
    }
}

fn read_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join("eu4-harmony").join("manifest.json");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Manifest::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Descriptor text without the `path=`/`archive=` lines the launcher adds.
fn strip_location(descriptor: &str) -> String {
    let mut out = String::new();
    for line in descriptor.lines() {
        if !descriptor_entry(line).is_some_and(|(key, _)| LOCATION_KEYS.contains(&key)) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

// ---------- new ----------

/// The SDK crate: `--sdk`, `EU4_HARMONY_SDK`, or the framework checkout this
/// executable was built in.
fn find_sdk(explicit: Option<&str>) -> Result<PathBuf> {
    let is_sdk = |d: &Path| fs::read_to_string(d.join("Cargo.toml")).is_ok_and(|t| t.contains("name = \"eu4-harmony-sdk\""));
    if let Some(p) = explicit.map(str::to_string).or_else(|| std::env::var("EU4_HARMONY_SDK").ok()) {
        let p = plain(Path::new(&p))?;
        return if is_sdk(&p) { Ok(p) } else { Err(format!("{} is not the eu4-harmony-sdk crate", p.display())) };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut dir = exe.parent();
    while let Some(d) = dir {
        if is_sdk(&d.join("sdk")) {
            return plain(&d.join("sdk"));
        }
        dir = d.parent();
    }
    Err("cannot find the EU4 Harmony SDK; pass --sdk <path to eu4-harmony/sdk> or set EU4_HARMONY_SDK".into())
}

fn create_mod(parent: &Path, id: &str, name: &str, sdk: &Path) -> Result<PathBuf> {
    if !valid_id(id) {
        return Err(format!("invalid id {id:?}: use lowercase letters, digits and '-', starting with a letter"));
    }
    if name.is_empty() || name.contains(['"', '\\', '\n', '@']) {
        return Err("the display name must not be empty or contain quotes, backslashes or '@'".into());
    }
    let root = parent.join(id);
    if root.exists() {
        return Err(format!("{} already exists", root.display()));
    }
    let lib = id.replace('-', "_");
    let krate = format!("{id}-extension");
    let sdk = forward_slashes(sdk);
    let values = [("lib", lib.as_str()), ("crate", krate.as_str()), ("sdk", sdk.as_str())];
    let english = format!("eu4-harmony/localisation/{lib}_l_english.yml");
    let chinese = format!("eu4-harmony/localisation/{lib}_l_simp_chinese.yml");
    let files = [
        ("descriptor.mod", templates::DESCRIPTOR),
        (".gitignore", templates::GITIGNORE),
        ("eu4-harmony/manifest.json", templates::MANIFEST),
        ("eu4-harmony/ui/button.gui", templates::BUTTON_GUI),
        (english.as_str(), templates::LOC_ENGLISH),
        (chinese.as_str(), templates::LOC_SIMP_CHINESE),
        ("extension/Cargo.toml", templates::CARGO_TOML),
        ("extension/.cargo/config.toml", templates::CARGO_CONFIG),
        ("extension/src/lib.rs", templates::LIB_RS),
    ];
    for (relative, template) in files {
        write(&root.join(relative), &templates::fill(template, id, name, &values))?;
    }
    Ok(root)
}

fn cmd_new(args: &Args) -> Result<()> {
    let id = args.positional.first().ok_or("usage: eu4-harmony new <id>")?;
    let parent = match args.value("dir") {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let sdk = find_sdk(args.value("sdk"))?;
    let root = create_mod(&parent, id, args.value("name").unwrap_or(id), &sdk)?;
    println!("created {}", root.display());
    println!("next: cd {id} && eu4-harmony build && eu4-harmony link, then enable the mod in the Paradox launcher");
    Ok(())
}

// ---------- build ----------

/// The cdylib built for the crate at `manifest_path`, from cargo's JSON messages.
fn built_dll(messages: &[serde_json::Value], manifest_path: &Path) -> Option<PathBuf> {
    let wanted = forward_slashes(manifest_path).to_lowercase();
    messages.iter().rev().find_map(|m| {
        if m["reason"] != "compiler-artifact" {
            return None;
        }
        let path = forward_slashes(Path::new(m["manifest_path"].as_str()?)).to_lowercase();
        let cdylib = m["target"]["kind"].as_array()?.iter().any(|k| k == "cdylib");
        if path != wanted || !cdylib {
            return None;
        }
        m["filenames"].as_array()?.iter().filter_map(|f| f.as_str()).find(|f| f.to_lowercase().ends_with(".dll")).map(PathBuf::from)
    })
}

fn cmd_build(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    let manifest = read_manifest(&root)?;
    let target = manifest.extension.as_deref().ok_or("the manifest declares no extension")?;
    let crate_dir = root.join("extension");
    let cargo_toml = crate_dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Err(format!("no extension crate at {}", crate_dir.display()));
    }
    let mut cargo = Command::new("cargo");
    cargo.arg("build").arg("--message-format=json-render-diagnostics").current_dir(&crate_dir).stdout(Stdio::piped());
    if !args.flag("debug") {
        cargo.arg("--release");
    }
    let mut child = cargo.spawn().map_err(|e| format!("cannot run cargo: {e}"))?;
    let mut messages = Vec::new();
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
            messages.push(value);
        }
    }
    if !child.wait().map_err(|e| e.to_string())?.success() {
        return Err("cargo build failed".into());
    }
    let dll = built_dll(&messages, &plain(&cargo_toml)?).ok_or("cargo built no DLL for the extension crate (is crate-type cdylib?)")?;
    let dest = target.split(['/', '\\']).fold(root.join("eu4-harmony"), |p, part| p.join(part));
    fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::copy(&dll, &dest).map_err(|e| format!("copy {} to {}: {e}", dll.display(), dest.display()))?;
    println!("built {}", dest.display());
    Ok(())
}

// ---------- link ----------

fn documents() -> Result<PathBuf> {
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    };
    let mut out: *mut u16 = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut out) };
    if hr < 0 || out.is_null() {
        return Err(format!("cannot find the Documents folder: {hr:#x}"));
    }
    let len = (0..).take_while(|&i| unsafe { *out.add(i) } != 0).count();
    let path = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, len) });
    unsafe { CoTaskMemFree(out as _) };
    Ok(PathBuf::from(path))
}

/// Writes `<user dir>/mod/eu4-harmony-<id>.mod` pointing at the mod folder.
fn link_mod(root: &Path, user_dir: &Path) -> Result<PathBuf> {
    let manifest = read_manifest(root)?;
    if !user_dir.is_dir() {
        return Err(format!("{} does not exist; pass --user-dir", user_dir.display()));
    }
    let descriptor = fs::read_to_string(root.join("descriptor.mod")).map_err(|e| e.to_string())?;
    let file = user_dir.join("mod").join(format!("eu4-harmony-{}.mod", manifest.id));
    write(&file, &format!("{}path=\"{}\"\n", strip_location(&descriptor), forward_slashes(root)))?;
    Ok(file)
}

fn cmd_link(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    let user_dir = match args.value("user-dir") {
        Some(d) => PathBuf::from(d),
        None => documents()?.join("Paradox Interactive").join("Europa Universalis IV"),
    };
    let file = link_mod(&root, &user_dir)?;
    println!("wrote {}", file.display());
    println!("enable the mod in a Paradox launcher playset to use it");
    Ok(())
}

// ---------- package ----------

fn copy_tree(from: &Path, to: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&entry.path(), &target, files)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
            files.push(target);
        }
    }
    Ok(())
}

fn copy_file(from: &Path, to: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::copy(from, to).map_err(|e| format!("{}: {e}", from.display()))?;
    files.push(to.to_path_buf());
    Ok(())
}

/// Copies what players need into `out`: the descriptor, thumbnail, game
/// resource folders and the framework files the manifest names.
fn package_mod(root: &Path, out: &Path) -> Result<Vec<PathBuf>> {
    let manifest = read_manifest(root)?;
    if manifest.api_version != abi::API_VERSION {
        return Err(format!("manifest api_version {} does not match this EU4 Harmony ({})", manifest.api_version, abi::API_VERSION));
    }
    let harmony = root.join("eu4-harmony");
    if let Some(ext) = &manifest.extension {
        let dll = fs::read(harmony.join(ext)).map_err(|_| format!("eu4-harmony/{ext} is missing; run eu4-harmony build first"))?;
        if !dll.windows(abi::ENTRY_SYMBOL.len()).any(|w| w == abi::ENTRY_SYMBOL.as_bytes()) {
            return Err(format!("eu4-harmony/{ext} does not export {}", abi::ENTRY_SYMBOL));
        }
    }
    for ui in &manifest.ui {
        if !harmony.join(&ui.file).is_file() {
            return Err(format!("UI fragment eu4-harmony/{} is missing", ui.file));
        }
    }
    let texts = localisation_files(&harmony.join("localisation"))?;
    if out.exists() {
        // Only replace an earlier package of this same mod.
        let ours = read_manifest(out).is_ok_and(|m| m.id == manifest.id) && out.join("descriptor.mod").is_file();
        if !ours {
            return Err(format!("{} exists and is not a package of this mod", out.display()));
        }
        fs::remove_dir_all(out).map_err(|e| e.to_string())?;
    }
    let mut files = Vec::new();
    let descriptor = fs::read_to_string(root.join("descriptor.mod")).map_err(|e| e.to_string())?;
    write(&out.join("descriptor.mod"), &strip_location(&descriptor))?;
    files.push(out.join("descriptor.mod"));
    if root.join("thumbnail.png").is_file() {
        copy_file(&root.join("thumbnail.png"), &out.join("thumbnail.png"), &mut files)?;
    }
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let skip = name.starts_with('.') || ["extension", "eu4-harmony", "target"].contains(&name.as_str());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() && !skip {
            copy_tree(&entry.path(), &out.join(&name), &mut files)?;
        }
    }
    copy_file(&harmony.join("manifest.json"), &out.join("eu4-harmony").join("manifest.json"), &mut files)?;
    for ui in &manifest.ui {
        copy_file(&harmony.join(&ui.file), &out.join("eu4-harmony").join(&ui.file), &mut files)?;
    }
    if let Some(ext) = &manifest.extension {
        copy_file(&harmony.join(ext), &out.join("eu4-harmony").join(ext), &mut files)?;
    }
    for path in texts {
        copy_file(&path, &out.join("eu4-harmony").join("localisation").join(path.file_name().unwrap()), &mut files)?;
    }
    Ok(files)
}

/// The `.yml` files of a mod's `eu4-harmony/localisation/`, once each of them
/// is one the framework reads.
fn localisation_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let (_, problems) = eu4_harmony_loc::load_dir(dir);
    if !problems.is_empty() {
        return Err(format!("eu4-harmony/localisation has problems:\n  {}", problems.join("\n  ")));
    }
    let Ok(entries) = fs::read_dir(dir) else { return Ok(Vec::new()) };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "yml")).collect();
    files.sort();
    Ok(files)
}

fn cmd_package(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    let manifest = read_manifest(&root)?;
    let out = match args.value("out") {
        Some(d) => PathBuf::from(d),
        None => root.parent().ok_or("mod folder has no parent")?.join(format!("{}-package", manifest.id)),
    };
    let files = package_mod(&root, &out)?;
    let bytes: u64 = files.iter().filter_map(|f| fs::metadata(f).ok()).map(|m| m.len()).sum();
    println!("packaged {} file(s), {:.1} MB, into {}", files.len(), bytes as f64 / 1e6, out.display());
    println!("upload that folder with the Paradox launcher's mod tools");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu4h-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ids() {
        assert!(valid_id("regional-combat") && valid_id("a1"));
        for bad in ["", "Hello", "1a", "a_b", "a-", "-a", "a b"] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn descriptor_location_is_stripped() {
        let d = "name=\"X\"\npath=\"D:/m\"\n archive = \"x.zip\"\nversion=\"1\"\n";
        assert_eq!(strip_location(d), "name=\"X\"\nversion=\"1\"\n");
    }

    /// The template mod, which the workspace builds, claims the interface
    /// version the framework implements.
    #[test]
    fn the_template_mod_is_for_this_interface_version() {
        let manifest = Manifest::parse(templates::MANIFEST).unwrap();
        assert_eq!((manifest.id.as_str(), manifest.api_version.as_str()), (templates::ID, abi::API_VERSION));
    }

    #[test]
    fn new_mod_has_every_file_filled_in() {
        let dir = temp("new");
        let root = create_mod(&dir, "my-mod", "My Mod", Path::new("D:/sdk")).unwrap();
        let manifest = read_manifest(&root).unwrap();
        assert_eq!((manifest.id.as_str(), manifest.api_version.as_str(), manifest.extension.as_deref()), ("my-mod", abi::API_VERSION, Some("win64/my_mod.dll")));
        let english = "eu4-harmony/localisation/my_mod_l_english.yml";
        for f in ["descriptor.mod", ".gitignore", "eu4-harmony/ui/button.gui", english, "eu4-harmony/localisation/my_mod_l_simp_chinese.yml", "extension/Cargo.toml", "extension/.cargo/config.toml", "extension/src/lib.rs"] {
            let text = fs::read_to_string(root.join(f)).unwrap();
            assert!(!text.contains("@@"), "{f} has an unfilled placeholder");
            for name in [templates::ID, templates::NAME, templates::LIB, templates::KEY] {
                assert!(!text.contains(name), "{f} still names the template mod");
            }
        }
        let lib = fs::read_to_string(root.join("extension/src/lib.rs")).unwrap();
        assert!(lib.contains("game.text(\"MY_MOD_TOOLTIP\")") && lib.contains("log!(\"{name} clicked"));
        let (texts, problems) = eu4_harmony_loc::load_dir(&root.join("eu4-harmony/localisation"));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(texts.text(eu4_harmony_loc::Language::English, "MY_MOD_TOOLTIP").as_deref(), Some("§YMy Mod§!\nClick to write a line to the log."));
        assert!(fs::read_to_string(root.join("extension/Cargo.toml")).unwrap().contains("path = \"D:/sdk\""));
        assert!(create_mod(&dir, "my-mod", "My Mod", Path::new("D:/sdk")).is_err());
        assert!(create_mod(&dir, "Bad", "x", Path::new("D:/sdk")).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn link_points_the_launcher_at_the_mod_folder() {
        let dir = temp("link");
        let root = create_mod(&dir, "linked", "Linked", Path::new("D:/sdk")).unwrap();
        let user = dir.join("user");
        fs::create_dir_all(&user).unwrap();
        let file = link_mod(&root, &user).unwrap();
        assert_eq!(file, user.join("mod").join("eu4-harmony-linked.mod"));
        let text = fs::read_to_string(file).unwrap();
        assert!(text.contains("name=\"Linked\"") && text.ends_with(&format!("path=\"{}\"\n", forward_slashes(&root))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn package_keeps_only_what_players_need() {
        let dir = temp("package");
        let root = create_mod(&dir, "pkg", "Pkg", Path::new("D:/sdk")).unwrap();
        let out = dir.join("pkg-package");
        assert!(package_mod(&root, &out).unwrap_err().contains("run eu4-harmony build"));
        write(&root.join("eu4-harmony/win64/pkg.dll"), "MZ...eu4h_extension_main...").unwrap();
        write(&root.join("eu4-harmony/notes.txt"), "not declared").unwrap();
        write(&root.join("interface/pkg.gfx"), "spriteTypes = {}").unwrap();
        write(&root.join("extension/target/debug/junk.txt"), "x").unwrap();
        write(&root.join(".git/HEAD"), "ref").unwrap();
        write(&root.join("eu4-harmony/localisation/broken_l_english.yml"), "l_english:\n PKG_X:0 missing quotes\n").unwrap();
        assert!(package_mod(&root, &out).unwrap_err().contains("broken_l_english.yml: line 2"));
        fs::remove_file(root.join("eu4-harmony/localisation/broken_l_english.yml")).unwrap();
        let mut files: Vec<String> = package_mod(&root, &out).unwrap().iter().map(|f| forward_slashes(f.strip_prefix(&out).unwrap())).collect();
        files.sort();
        assert_eq!(
            files,
            [
                "descriptor.mod",
                "eu4-harmony/localisation/pkg_l_english.yml",
                "eu4-harmony/localisation/pkg_l_simp_chinese.yml",
                "eu4-harmony/manifest.json",
                "eu4-harmony/ui/button.gui",
                "eu4-harmony/win64/pkg.dll",
                "interface/pkg.gfx"
            ]
        );
        // A second run replaces the earlier package; an unrelated folder is never removed.
        assert!(package_mod(&root, &out).is_ok());
        let other = dir.join("other");
        fs::create_dir_all(&other).unwrap();
        assert!(package_mod(&root, &other).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn finds_the_dll_of_the_extension_crate() {
        let crate_toml = Path::new("D:/m/extension/Cargo.toml");
        let messages = vec![
            serde_json::json!({"reason":"compiler-artifact","manifest_path":"D:\\sdk\\Cargo.toml","target":{"kind":["lib"]},"filenames":["D:/t/libsdk.rlib"]}),
            serde_json::json!({"reason":"compiler-artifact","manifest_path":"D:\\m\\extension\\Cargo.toml","target":{"kind":["cdylib"]},"filenames":["D:/m/extension/target/release/m.dll","D:/m/extension/target/release/m.dll.lib"]}),
            serde_json::json!({"reason":"build-finished","success":true}),
        ];
        assert_eq!(built_dll(&messages, crate_toml), Some(PathBuf::from("D:/m/extension/target/release/m.dll")));
        assert_eq!(built_dll(&messages[..1], crate_toml), None);
    }
}
