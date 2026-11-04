//! `eu4-harmony`: create, register and package EU4 Harmony mods, and use the
//! running game through remote control (`game`, in `game.rs`).
//!
//! A mod is a folder with `descriptor.mod` and `eu4-harmony/manifest.toml`;
//! in `eu4-harmony/` its extension in `lua/`, interface fragments in `ui/`,
//! texts in `localisation/` and its own files in `data/`
//! (ADR manifest-in-toml-layout-by-convention); game resources (`interface/`,
//! `gfx/`, ...) sit next to them as in any Paradox mod.
mod check;
mod fake;
mod game;
mod reload;
mod skills;
mod templates;
mod test;
mod types;
mod wiki;
mod world;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use eu4_harmony_client::{backup, pipe, start};

type Result<T> = std::result::Result<T, String>;

const USAGE: &str = "\
eu4-harmony: create, register and package EU4 Harmony mods, and use the
running game through remote control

usage:
  eu4-harmony new <id> [--name <display name>] [--dir <parent dir>]
      Creates the mod folder <id>/ with a Lua extension, one button, its
      texts in English and Chinese, the editor's type definitions and an
      AGENTS.md pointing a coding agent at eu4-harmony skills.
  eu4-harmony link [<mod dir>] [--user-dir <EU4 user dir>] [--game-dir <game dir>]
      Registers the mod folder with the Paradox launcher (enable it in a playset),
      in the user dir the game folder's userdir.txt names, as the game finds it;
      the game folder is --game-dir, EU4_GAME_DIR, the one this tool is in or
      the Steam library that holds the game.
  eu4-harmony test [<mod dir>]
      Runs the mod's eu4-harmony/tests/*.lua against a fake game, with the
      game's Lua host and sandbox; eu4-harmony skills testing says how.
  eu4-harmony reload [<mod dir>] [--watch]
      Reloads the mod's Lua code and texts in the running game, its state
      carried through save_data; with --watch, whenever a file under
      eu4-harmony/lua/ or eu4-harmony/localisation/ changes, until stopped.
      Interface fragments and the manifest take a restart of the game.
  eu4-harmony types [<mod dir>]
      Writes the editor's type definitions for this tool's catalogue again,
      in .eu4-harmony/types/, and a .luarc.json pointing at them where the
      mod has none.
  eu4-harmony check [<mod dir>]
      Reads the mod as the framework does, without the game: the manifest
      (and that its api_version is this tool's), every Lua file compiling,
      every localisation file, and no file under ui/ where the layout puts
      nothing. Lists each problem, and fails when there is one.
  eu4-harmony package [<mod dir>] [--out <dir>]
      Writes a clean folder for upload, once eu4-harmony check passes.
  eu4-harmony backup <new dir> [--user-dir <EU4 user dir>] [--game-dir <game dir>]
      Backs up the player's files a run of the game may change, before a
      test that loads a save: save games\\, the framework's eu4-harmony\\ and
      every file at the top of the user dir (the settings, the login cache,
      the playset), into <new dir>, a folder outside the game and user dirs.
  eu4-harmony restore <dir> [--keep] [--keep-crashes] [--user-dir <EU4 user dir>] [--game-dir <game dir>]
      After the run: lists what it changed, puts every file back as the
      backup has it and compares each, removes the crash folders it left
      (--keep-crashes keeps them) and the backup (--keep keeps it). A file it
      could not put back is a NOT RESTORED line, and the backup stays.
      Both refuse while the game runs.
  eu4-harmony start [--hidden] [--sound] [--timeout <seconds>] [--game-dir <game dir>]
      Starts the game from its folder and waits until its main menu is up
      (300 seconds without --timeout), printing its process id; says why when
      it ends first. With --hidden (the game's switch -harmony_hidden) nothing
      of it reaches the desktop and it plays no sound, unless --sound
      (-harmony_sound); eu4-harmony game desktop show shows it. Steam must be
      running; a game already running is refused.
  eu4-harmony game <capability> [--<argument> <value>]...
      Sends the running game one request through remote control and prints
      the answer as JSON; eu4-harmony game -h describes the command line.
  eu4-harmony wiki [<topic>...]
      The reference: every capability, how Lua and the command line call
      it, what it answers; a mod's handlers, globals and manifest; how a
      remote client plays a game; errors and events.
  eu4-harmony skills [<topic>...]
      Guides for making a mod, for a coding agent or a person: the steps,
      and how to tell each is done.

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
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let result = match raw.first().map(String::as_str) {
        Some("new") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_new(&a)),
        Some("link") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_link(&a)),
        Some("package") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_package(&a)),
        Some("check") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_check(&a)),
        Some("types") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_types(&a)),
        Some("test") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_test(&a)),
        Some("reload") => Args::parse(&raw[1..], &["watch"]).and_then(|a| cmd_reload(&a)),
        Some("backup") => Args::parse(&raw[1..], &[]).and_then(|a| cmd_backup(&a)),
        Some("restore") => Args::parse(&raw[1..], &["keep", "keep-crashes"]).and_then(|a| cmd_restore(&a)),
        Some("start") => Args::parse(&raw[1..], &["hidden", "sound"]).and_then(|a| cmd_start(&a)),
        Some("game") => std::process::exit(game::run(&raw[1..])),
        Some("wiki") => wiki::page(&raw[1..]).map(|page| print!("{page}")),
        Some("skills") => skills::page(&raw[1..]).map(|page| print!("{page}")),
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

/// `eu4-harmony reload`: the mod at or above the start folder, by its id.
fn cmd_reload(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    let id = read_manifest(&root)?.id;
    if args.options.contains_key("watch") {
        reload::watch(&root, &id);
    }
    println!("{}", reload::once(&id)?);
    Ok(())
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
        if d.join("descriptor.mod").is_file() && d.join("eu4-harmony").join(eu4_harmony_manifest::FILE).is_file() {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }
    Err(format!("{} is not inside an EU4 Harmony mod (no descriptor.mod with eu4-harmony/{})", start.display(), eu4_harmony_manifest::FILE))
}

fn start_dir(args: &Args) -> Result<PathBuf> {
    match args.positional.first() {
        Some(p) => Ok(PathBuf::from(p)),
        None => std::env::current_dir().map_err(|e| e.to_string()),
    }
}

fn read_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join("eu4-harmony").join(eu4_harmony_manifest::FILE);
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

fn create_mod(parent: &Path, id: &str, name: &str) -> Result<PathBuf> {
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
    let english = format!("eu4-harmony/localisation/{lib}_l_english.yml");
    let chinese = format!("eu4-harmony/localisation/{lib}_l_simp_chinese.yml");
    let files = [
        ("descriptor.mod", templates::DESCRIPTOR),
        ("AGENTS.md", templates::AGENTS),
        ("eu4-harmony/manifest.toml", templates::MANIFEST),
        ("eu4-harmony/lua/main.lua", templates::MAIN_LUA),
        ("eu4-harmony/tests/main_test.lua", templates::MAIN_TEST),
        ("eu4-harmony/ui/single_unitpanel/buttons/button.gui", templates::BUTTON_GUI),
        (english.as_str(), templates::LOC_ENGLISH),
        (chinese.as_str(), templates::LOC_SIMP_CHINESE),
    ];
    for (relative, template) in files {
        write(&root.join(relative), &templates::fill(template, id, name))?;
    }
    types::write(&root)?;
    Ok(root)
}

fn cmd_new(args: &Args) -> Result<()> {
    let id = args.positional.first().ok_or("usage: eu4-harmony new <id>")?;
    let parent = match args.value("dir") {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let root = create_mod(&parent, id, args.value("name").unwrap_or(id))?;
    println!("created {}", root.display());
    println!("next: cd {id} && eu4-harmony link, then enable the mod in the Paradox launcher");
    Ok(())
}

// ---------- test ----------

fn cmd_test(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    let manifest = read_manifest(&root)?;
    let descriptor = fs::read_to_string(root.join("descriptor.mod")).map_err(|e| e.to_string())?;
    let name =
        descriptor.lines().find_map(|l| descriptor_entry(l).filter(|(k, _)| *k == "name").map(|(_, v)| v.to_string())).unwrap_or_else(|| manifest.id.clone());
    let outcome = test::run(&root, manifest, name)?;
    print!("{}", outcome.report);
    if outcome.failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

// ---------- types ----------

fn cmd_types(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    for file in types::write(&root)? {
        println!("wrote {}", root.join(file).display());
    }
    Ok(())
}

// ---------- link ----------

/// The game folder: `--game-dir`, else `EU4_GAME_DIR`, else the one this
/// tool is installed in (`<game>\eu4-harmony\eu4-harmony.exe`), else the
/// Steam library that holds the game.
fn game_dir(args: &Args) -> Option<PathBuf> {
    let named = args.value("game-dir").map(PathBuf::from).or_else(|| std::env::var_os("EU4_GAME_DIR").map(PathBuf::from));
    named.or_else(|| std::env::current_exe().ok().and_then(|exe| installed_in(&exe))).or_else(eu4_harmony_manifest::steam_game_dir)
}

/// The game folder holding the tool at `exe`, when it is installed there.
fn installed_in(exe: &Path) -> Option<PathBuf> {
    let game = exe.parent()?.parent()?;
    game.join("eu4.exe").is_file().then(|| game.to_path_buf())
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

/// `--user-dir`, else the user dir of the game folder.
fn user_dir(args: &Args) -> Result<PathBuf> {
    match args.value("user-dir") {
        Some(d) => Ok(PathBuf::from(d)),
        None => eu4_harmony_manifest::user_dir(game_dir(args).as_deref()),
    }
}

fn cmd_link(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    let file = link_mod(&root, &user_dir(args)?)?;
    println!("wrote {}", file.display());
    println!("enable the mod in a Paradox launcher playset to use it");
    Ok(())
}

// ---------- backup, restore ----------

fn cmd_backup(args: &Args) -> Result<()> {
    let [dir] = args.positional.as_slice() else { return Err("usage: eu4-harmony backup <new dir>".into()) };
    eu4_harmony_client::game_not_running()?;
    backup::backup(&backup::user_folder(&user_dir(args)?)?, Path::new(dir))
}

fn cmd_restore(args: &Args) -> Result<()> {
    let [dir] = args.positional.as_slice() else { return Err("usage: eu4-harmony restore <dir>".into()) };
    eu4_harmony_client::game_not_running()?;
    let (keep, keep_crashes) = (args.options.contains_key("keep"), args.options.contains_key("keep-crashes"));
    backup::restore(&backup::user_folder(&user_dir(args)?)?, Path::new(dir), keep, keep_crashes)
}

// ---------- start ----------

fn cmd_start(args: &Args) -> Result<()> {
    if !args.positional.is_empty() {
        return Err("usage: eu4-harmony start [--hidden] [--sound] [--timeout <seconds>]".into());
    }
    let game = game_dir(args).ok_or("no game folder: pass --game-dir or set EU4_GAME_DIR")?;
    let (hidden, sound) = (args.options.contains_key("hidden"), args.options.contains_key("sound"));
    if sound && !hidden {
        return Err("--sound is for --hidden: a game started shown plays its sound anyway".into());
    }
    let wait = match args.value("timeout") {
        Some(text) => std::time::Duration::from_secs(text.parse().map_err(|_| format!("--timeout {text}: not a number of seconds"))?),
        None => start::WAIT,
    };
    let log = user_dir(args)?.join("eu4-harmony").join("logs").join("core.log");
    start::start(&game, &log, hidden, sound, wait).map(drop)
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
/// resource folders, and in `eu4-harmony/` the manifest, the Lua files, the
/// interface fragments where the layout puts them, the texts and the mod's
/// own files in `data/`.
fn package_mod(root: &Path, out: &Path) -> Result<Vec<PathBuf>> {
    let checked = check::check(root);
    if !checked.problems.is_empty() {
        return Err(format!("eu4-harmony check finds problems:\n  {}", checked.problems.join("\n  ")));
    }
    let check::Checked { manifest, layout, scripts, texts, .. } = checked;
    let manifest = manifest.ok_or("the manifest does not read")?;
    let harmony = root.join("eu4-harmony");
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
        let skip = name.starts_with('.') || name == "eu4-harmony";
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() && !skip {
            copy_tree(&entry.path(), &out.join(&name), &mut files)?;
        }
    }
    let inside = out.join("eu4-harmony");
    copy_file(&harmony.join(eu4_harmony_manifest::FILE), &inside.join(eu4_harmony_manifest::FILE), &mut files)?;
    for script in scripts {
        let relative = script.strip_prefix(&harmony).map_err(|e| e.to_string())?;
        copy_file(&script, &inside.join(relative), &mut files)?;
    }
    for ui in &layout.ui {
        copy_file(&harmony.join(&ui.file), &inside.join(&ui.file), &mut files)?;
    }
    for path in texts {
        copy_file(&path, &inside.join("localisation").join(path.file_name().unwrap()), &mut files)?;
    }
    if harmony.join("data").is_dir() {
        copy_tree(&harmony.join("data"), &inside.join("data"), &mut files)?;
    }
    Ok(files)
}

/// The `.lua` files under a mod's `eu4-harmony/lua/`, once each of them
/// compiles.
/// `eu4-harmony check`: each problem a line, and a failure when there is one.
fn cmd_check(args: &Args) -> Result<()> {
    let root = mod_root(&start_dir(args)?)?;
    let checked = check::check(&root);
    if checked.problems.is_empty() {
        println!(
            "ok: {} Lua file(s) compile, {} localisation file(s) read, the manifest is for api_version {}",
            checked.scripts.len(),
            checked.texts.len(),
            eu4_harmony_remote::API_VERSION
        );
        return Ok(());
    }
    for problem in &checked.problems {
        println!("{problem}");
    }
    Err(format!("{} problem(s)", checked.problems.len()))
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

    /// The template mod is written for the catalogue the framework serves.
    #[test]
    fn the_template_mod_is_for_this_catalogue_version() {
        let manifest = Manifest::parse(templates::MANIFEST).unwrap();
        assert_eq!((manifest.id.as_str(), manifest.api_version.as_str()), (templates::ID, eu4_harmony_remote::API_VERSION));
    }

    #[test]
    fn new_mod_has_every_file_filled_in() {
        let dir = temp("new");
        let root = create_mod(&dir, "my-mod", "My Mod").unwrap();
        let manifest = read_manifest(&root).unwrap();
        assert_eq!((manifest.id.as_str(), manifest.api_version.as_str()), ("my-mod", eu4_harmony_remote::API_VERSION));
        assert_eq!(manifest.description.as_deref(), Some("MY_MOD_DESCRIPTION"));
        let layout = eu4_harmony_manifest::layout(&root.join("eu4-harmony"));
        assert_eq!(layout.extension.as_deref(), Some("lua/main.lua"));
        assert_eq!(layout.ui.len(), 1);
        assert_eq!((layout.ui[0].into.as_str(), layout.ui[0].area.as_deref()), ("single_unitpanel", Some("buttons")));
        let english = "eu4-harmony/localisation/my_mod_l_english.yml";
        for f in [
            "descriptor.mod",
            "AGENTS.md",
            "eu4-harmony/manifest.toml",
            "eu4-harmony/lua/main.lua",
            "eu4-harmony/tests/main_test.lua",
            "eu4-harmony/ui/single_unitpanel/buttons/button.gui",
            english,
            "eu4-harmony/localisation/my_mod_l_simp_chinese.yml",
        ] {
            let text = fs::read_to_string(root.join(f)).unwrap();
            for name in [templates::ID, templates::NAME, templates::LIB, templates::KEY] {
                assert!(!text.contains(name), "{f} still names the template mod");
            }
        }
        let main = fs::read_to_string(root.join("eu4-harmony/lua/main.lua")).unwrap();
        assert!(main.contains("text.get(\"MY_MOD_TOOLTIP\")") && main.contains("my-mod.log"));
        assert_eq!(check::check(&root).problems, Vec::<String>::new());
        assert!(root.join(types::FILE).is_file() && root.join(types::LUARC).is_file());
        let (texts, problems) = eu4_harmony_loc::load_dir(&root.join("eu4-harmony/localisation"));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(texts.text(eu4_harmony_loc::Language::English, "MY_MOD_TOOLTIP").as_deref(), Some("§YMy Mod§!\nClick to write a line to the log."));
        assert!(texts.text(eu4_harmony_loc::Language::SimpChinese, "MY_MOD_DESCRIPTION").is_some());
        assert!(create_mod(&dir, "my-mod", "My Mod").is_err());
        assert!(create_mod(&dir, "Bad", "x").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_new_mod_s_own_tests_pass() {
        let dir = temp("tests-new");
        let root = create_mod(&dir, "tested", "Tested").unwrap();
        let outcome = test::run(&root, read_manifest(&root).unwrap(), "Tested".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (3, 0), "{}", outcome.report);
        assert!(!outcome.report.contains("note:"), "{}", outcome.report);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn tests_drive_waiting_tasks_and_requests_and_report_what_fails() {
        let dir = temp("tests-drive");
        let root = create_mod(&dir, "driven", "Driven").unwrap();
        write(&root.join("eu4-harmony/manifest.toml"), "id = \"driven\"\napi_version = \"0.3\"\npermissions = { network = [\"api.example.com\"] }\n").unwrap();
        write(
            &root.join("eu4-harmony/lua/main.lua"),
            r#"
            return {
              on_session = function(game)
                spawn(function()
                  local answer, err = game.diplomacy.propose_alliance("POR")
                  log("alliance " .. (answer and answer.answer or err.code))
                end)
                spawn(function()
                  local r, err = http.fetch("https://api.example.com/score", { method = "POST", body = "1" })
                  log("score " .. (r and r.status or err.code))
                end)
              end,
              on_day = function(game, date)
                game.map.paint({ [151] = 0xFF0000 })
                log("day " .. tostring(date) .. " " .. tostring(select(2, game.country.get("FRA")).code))
              end,
              save_data = function(game) return { kept = true } end,
            }
            "#,
        )
        .unwrap();
        write(
            &root.join("eu4-harmony/tests/main_test.lua"),
            r#"
            test("a proposal and a request wait for their answers", function()
              local mod = open()
              mod:later("diplomacy.propose_alliance")
              mod:session()
              assert(mod:waiting() == 2)
              assert(mod:requests()[1].url == "https://api.example.com:443/score" and mod:requests()[1].method == "POST")
              mod:reply("diplomacy.propose_alliance", { answer = "accepted" })
              mod:respond(1, { status = 201 })
              mod:poll()
              local log = mod:log()
              assert(mod:waiting() == 0 and log[1] == "alliance accepted" and log[2] == "score 201", table.concat(log, "; "))
              assert(mod:calls("diplomacy.propose_alliance")[1].country == "POR")
            end)
            test("a day paints the map, and an unanswered capability says so", function()
              local mod = open()
              mod:day("1444.11.12")
              assert(mod:map()[151].r == 255 and mod:map()[151].g == 0)
              assert(mod:log()[1] == "day 1444.11.12 not_ready", mod:log()[1])
              local data = mod:save()
              assert(data.kept == true)
            end)
            test("a failing test is reported", function()
              assert(1 == 2, "one is not two")
            end)
            "#,
        )
        .unwrap();
        write(&root.join("eu4-harmony/tests/broken_test.lua"), "test(").unwrap();
        let outcome = test::run(&root, read_manifest(&root).unwrap(), "Driven".into()).unwrap();
        let report = &outcome.report;
        assert_eq!((outcome.passed, outcome.failed), (2, 2), "{report}");
        assert!(report.contains("ok      a proposal and a request wait"), "{report}");
        assert!(report.contains("note: country.get had no answer"), "{report}");
        assert!(report.contains("FAILED  a failing test is reported") && report.contains("one is not two"), "{report}");
        assert!(report.contains("eu4-harmony/tests/broken_test.lua\n  FAILED  (the file)"), "{report}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_answer_not_of_its_capability_s_shape_fails_the_test_at_its_line() {
        let dir = temp("tests-shape");
        let root = create_mod(&dir, "shaped", "Shaped").unwrap();
        write(&root.join("eu4-harmony/lua/main.lua"), "return { on_frame = function(game) ARMY = game.army.get(1) end }").unwrap();
        write(
            &root.join("eu4-harmony/tests/main_test.lua"),
            "test(\"a field it does not have\", function()
  local mod = open()
  mod:answer(\"army.get\", { id = 1, regiment = 3 })
end)
             test(\"a field left out\", function()
  local mod = open()
  mod:answer(\"army.get\", { id = 1 })
  mod:frame()
end)
             test(\"a function's answer of the wrong type\", function()
  local mod = open()
  mod:answer(\"army.get\", function(request) return { province = \"x\" } end)
  mod:frame()
end)
",
        )
        .unwrap();
        let outcome = test::run(&root, read_manifest(&root).unwrap(), "Shaped".into()).unwrap();
        let report = &outcome.report;
        assert_eq!((outcome.passed, outcome.failed), (1, 2), "{report}");
        assert!(report.contains("main_test.lua:3: army.get's answer: regiment is no field of eu4.Army"), "{report}");
        assert!(report.contains("army.get's answer: province: an integer, not a string"), "{report}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_world_shared_by_require_answers_and_takes_orders() {
        let dir = temp("tests-world");
        let root = create_mod(&dir, "worldly", "Worldly").unwrap();
        write(
            &root.join("eu4-harmony/lua/main.lua"),
            r#"
            return {
              on_button = function(game)
                local army = game.army.get(game.army.selected())
                local route = game.army.route_to(army.id, 220)
                log("route " .. table.concat(route, ","))
                game.army.move(army.id, 220)
                log("to " .. game.province.get(220).name .. ", " .. tostring(game.country.hostile("CAS", "GRA")))
              end,
            }
            "#,
        )
        .unwrap();
        write(
            &root.join("eu4-harmony/tests/helpers/spain.lua"),
            r#"
            return {
              player = "CAS", hostile = { { "CAS", "GRA" } },
              provinces = {
                { id = 217, name = "Madrid", owner = "CAS", neighbors = { 219 } },
                { id = 219, name = "Toledo", owner = "CAS", neighbors = { 220 } },
                { id = 220, name = "Valencia", owner = "ARA" },
              },
              armies = { { id = 1, owner = "CAS", province = 217, regiments = 10 } },
              selected = 1,
            }
            "#,
        )
        .unwrap();
        write(
            &root.join("eu4-harmony/tests/main_test.lua"),
            r#"
            local spain = require("helpers.spain")
            local steps = require("steps")
            test("a module of the mod's own is tested on its own", function()
              assert(steps.count({ 1, 2, 3 }) == 3)
            end)
            test("the world routes, names and takes the order", function()
              local mod = open()
              mod:world(spain)
              mod:button("button")
              local log = mod:log()
              assert(log[1] == "route 219,220" and log[2] == "to Valencia, true", table.concat(log, "; "))
              assert(mod:calls("army.move")[1].province == 220)
            end)
            test("a test's own answer goes before the world's", function()
              local mod = open()
              mod:world(spain)
              mod:answer("province.get", { name = "Elsewhere" })
              mod:button("button")
              assert(mod:log()[2] == "to Elsewhere, true", mod:log()[2])
            end)
            "#,
        )
        .unwrap();
        write(&root.join("eu4-harmony/lua/steps.lua"), "return { count = function(t) return #t end }").unwrap();
        let outcome = test::run(&root, read_manifest(&root).unwrap(), "Worldly".into()).unwrap();
        assert_eq!((outcome.passed, outcome.failed), (3, 0), "{}", outcome.report);
        assert!(!outcome.report.contains("note:"), "{}", outcome.report);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn link_points_the_launcher_at_the_mod_folder() {
        let dir = temp("link");
        let root = create_mod(&dir, "linked", "Linked").unwrap();
        let user = dir.join("user");
        fs::create_dir_all(&user).unwrap();
        let file = link_mod(&root, &user).unwrap();
        assert_eq!(file, user.join("mod").join("eu4-harmony-linked.mod"));
        let text = fs::read_to_string(file).unwrap();
        assert!(text.contains("name=\"Linked\"") && text.ends_with(&format!("path=\"{}\"\n", forward_slashes(&root))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_game_folder_is_the_one_the_tool_is_installed_in() {
        let dir = temp("installed");
        let exe = dir.join("eu4-harmony").join("eu4-harmony.exe");
        write(&exe, "MZ").unwrap();
        assert_eq!(installed_in(&exe), None);
        write(&dir.join("eu4.exe"), "MZ").unwrap();
        assert_eq!(installed_in(&exe), Some(dir.clone()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn package_keeps_only_what_players_need() {
        let dir = temp("package");
        let root = create_mod(&dir, "pkg", "Pkg").unwrap();
        let out = dir.join("pkg-package");
        write(&root.join("eu4-harmony/lua/range/bfs.lua"), "return {}").unwrap();
        write(&root.join("eu4-harmony/data/units.json"), "{}").unwrap();
        write(&root.join("eu4-harmony/notes.txt"), "not where the layout puts anything").unwrap();
        write(&root.join("interface/pkg.gfx"), "spriteTypes = {}").unwrap();
        write(&root.join(".git/HEAD"), "ref").unwrap();
        write(&root.join(".luarc.json"), "{}").unwrap();
        write(&root.join("eu4-harmony/lua/broken.lua"), "return {").unwrap();
        assert!(package_mod(&root, &out).unwrap_err().contains("broken.lua"));
        fs::remove_file(root.join("eu4-harmony/lua/broken.lua")).unwrap();
        write(&root.join("eu4-harmony/localisation/broken_l_english.yml"), "l_english:\n PKG_X:0 missing quotes\n").unwrap();
        assert!(package_mod(&root, &out).unwrap_err().contains("broken_l_english.yml: line 2"));
        fs::remove_file(root.join("eu4-harmony/localisation/broken_l_english.yml")).unwrap();
        write(&root.join("eu4-harmony/ui/stray.gui"), "x").unwrap();
        assert!(package_mod(&root, &out).unwrap_err().contains("eu4-harmony/ui/stray.gui: not where the layout puts anything"));
        fs::remove_file(root.join("eu4-harmony/ui/stray.gui")).unwrap();
        let mut files: Vec<String> = package_mod(&root, &out).unwrap().iter().map(|f| forward_slashes(f.strip_prefix(&out).unwrap())).collect();
        files.sort();
        assert_eq!(
            files,
            [
                "descriptor.mod",
                "eu4-harmony/data/units.json",
                "eu4-harmony/localisation/pkg_l_english.yml",
                "eu4-harmony/localisation/pkg_l_simp_chinese.yml",
                "eu4-harmony/lua/main.lua",
                "eu4-harmony/lua/range/bfs.lua",
                "eu4-harmony/manifest.toml",
                "eu4-harmony/ui/single_unitpanel/buttons/button.gui",
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
}
