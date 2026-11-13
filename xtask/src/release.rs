//! A release's source, version and change note, checked before packaging or
//! publishing. Tags select an immutable source already merged into master;
//! master's tip may advance without changing that selection.
use crate::{repo, Args, Result};
use serde::Serialize;
use std::{fs, path::Path, process::Command};

#[derive(Debug, Serialize)]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub commit: String,
    pub notes: String,
    pub framework_asset: String,
    pub cli_asset: String,
}

pub fn artifact_names(version: &str) -> (String, String) {
    (format!("eu4-harmony-{version}-windows-x64.zip"), format!("eu4-harmony-cli-{version}-windows-x64.zip"))
}

/// Three canonical decimal components, without a prefix or a prerelease.
fn version_number(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) && (part.len() == 1 || !part.starts_with('0')) && part.parse::<u64>().is_ok()
        })
}

/// The version's own change note, never another version's prefix.
pub fn changelog_section(text: &str, version: &str) -> Option<String> {
    let mut lines = text.lines().skip_while(|line| {
        let Some(rest) = line.strip_prefix("## ") else { return true };
        let rest = rest.trim().trim_start_matches('[');
        !rest.strip_prefix(version).is_some_and(|after| after.is_empty() || after.starts_with([']', ' ', '\t']))
    });
    lines.next()?;
    let body = lines.take_while(|line| !line.starts_with("## ")).collect::<Vec<_>>().join("\n").trim().to_string();
    (!body.is_empty()).then_some(body)
}

fn read(root: &Path, name: &str) -> Result<String> {
    fs::read_to_string(root.join(name)).map_err(|e| format!("cannot read {name}: {e}"))
}

fn metadata(root: &Path) -> Result<Release> {
    let manifest: toml::Value = toml::from_str(&read(root, "Cargo.toml")?).map_err(|e| format!("Cargo.toml: {e}"))?;
    let version = manifest
        .get("workspace")
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or("Cargo.toml has no workspace.package.version")?;
    if !version_number(version) {
        return Err(format!("{version:?} is not a canonical X.Y.Z release version"));
    }
    let lock: toml::Value = toml::from_str(&read(root, "Cargo.lock")?).map_err(|e| format!("Cargo.lock: {e}"))?;
    let packages = lock.get("package").and_then(toml::Value::as_array).ok_or("Cargo.lock has no packages")?;
    let mut internal = 0;
    for package in packages.iter().filter(|p| p.get("source").is_none()) {
        internal += 1;
        if package.get("version").and_then(toml::Value::as_str) != Some(version) {
            return Err(format!("Cargo.lock: internal package {:?} is not version {version}", package.get("name").and_then(toml::Value::as_str)));
        }
    }
    if internal == 0 {
        return Err("Cargo.lock has no internal packages".into());
    }
    let notes = changelog_section(&read(root, "CHANGELOG.md")?, version).ok_or_else(|| format!("CHANGELOG.md has no nonempty section for {version}"))?;
    let (framework_asset, cli_asset) = artifact_names(version);
    Ok(Release { version: version.into(), tag: format!("v{version}"), commit: String::new(), notes, framework_asset, cli_asset })
}

pub fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").current_dir(root).args(args).output().map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

/// The exact source a release may use. Without a tag, checks the preparation
/// run before the first tag is pushed; it still needs clean, merged source.
pub fn check(root: &Path, tag: Option<&str>) -> Result<Release> {
    let mut release = metadata(root)?;
    if tag.is_some_and(|tag| tag != release.tag) {
        return Err(format!("tag {:?} does not match workspace version {} (expected {})", tag.unwrap(), release.version, release.tag));
    }
    if !git(root, &["status", "--porcelain"])?.is_empty() {
        return Err("release source has uncommitted or untracked changes".into());
    }
    git(root, &["fetch", "--quiet", "origin", "master"])?;
    release.commit = git(root, &["rev-parse", "HEAD"])?;
    let status = Command::new("git")
        .current_dir(root)
        .args(["merge-base", "--is-ancestor", &release.commit, "refs/remotes/origin/master"])
        .status()
        .map_err(|e| format!("cannot check master ancestry: {e}"))?;
    if !status.success() {
        return Err("release source is not a commit merged into origin/master".into());
    }
    if let Some(tag) = tag {
        let local = git(root, &["rev-parse", "--verify", &format!("refs/tags/{tag}^{{commit}}")])?;
        if local != release.commit {
            return Err(format!("{tag} does not point to the checked-out source"));
        }
        let plain = format!("refs/tags/{tag}");
        let peeled = format!("{plain}^{{}}");
        let remote = git(root, &["ls-remote", "--tags", "origin", &plain, &peeled])?;
        let refs: Vec<_> = remote.lines().filter_map(|line| line.split_once('\t')).collect();
        let target = refs.iter().find(|(_, name)| *name == peeled).or_else(|| refs.iter().find(|(_, name)| *name == plain)).map(|(sha, _)| *sha);
        if target != Some(release.commit.as_str()) {
            return Err(format!("origin's {tag} is missing or does not point to the checked-out source"));
        }
    }
    Ok(release)
}

pub fn run(args: &Args) -> Result<()> {
    let root = repo();
    let release = check(&root, args.value("tag"))?;
    let out = args.value("out").map_or_else(|| root.join("target").join("dist"), Into::into);
    fs::create_dir_all(&out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;
    let json = serde_json::to_string_pretty(&release).map_err(|e| format!("release metadata: {e}"))?;
    fs::write(out.join("release.json"), format!("{json}\n")).map_err(|e| format!("cannot write release.json: {e}"))?;
    println!("checked {} from {} (merged into master)", release.tag, release.commit);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture {
        folder: PathBuf,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let folder = std::env::temp_dir().join(format!("harmony-release-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&folder).unwrap();
            let root = folder.join("source");
            fs::create_dir_all(&root).unwrap();
            git(&root, &["init", "-b", "master"]).unwrap();
            git(&root, &["config", "user.email", "release-test@example.invalid"]).unwrap();
            git(&root, &["config", "user.name", "Release test"]).unwrap();
            fs::write(root.join("Cargo.toml"), "[workspace.package]\nversion = \"0.2.0\"\n").unwrap();
            fs::write(root.join("Cargo.lock"), "version = 4\n[[package]]\nname = \"xtask\"\nversion = \"0.2.0\"\n[[package]]\nname = \"external\"\nversion = \"9.0.0\"\nsource = \"registry\"\n").unwrap();
            fs::write(root.join("CHANGELOG.md"), "# Changelog\n\n## 0.2.0\n\n- Ready.\n\n## 0.1.5\n\n- Earlier.\n").unwrap();
            fs::write(root.join(".gitignore"), "target/\n").unwrap();
            git(&root, &["add", "."]).unwrap();
            git(&root, &["commit", "-m", "Prepare the release"]).unwrap();
            let remote = folder.join("origin.git");
            git(&folder, &["init", "--bare", remote.to_str().unwrap()]).unwrap();
            git(&root, &["remote", "add", "origin", remote.to_str().unwrap()]).unwrap();
            git(&root, &["push", "-u", "origin", "master"]).unwrap();
            Self { folder, root }
        }
        fn tag(&self) {
            git(&self.root, &["tag", "-a", "v0.2.0", "-m", "Release 0.2.0"]).unwrap();
            git(&self.root, &["push", "origin", "v0.2.0"]).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            #[expect(
                clippy::permissions_set_readonly_false,
                reason = "These Windows-only tests clear Git's read-only object attribute before deleting their temporary repositories."
            )]
            fn writable(path: &Path) {
                for entry in fs::read_dir(path).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        writable(&path);
                    } else {
                        let mut permissions = path.metadata().unwrap().permissions();
                        if permissions.readonly() {
                            permissions.set_readonly(false);
                            fs::set_permissions(path, permissions).unwrap();
                        }
                    }
                }
            }
            writable(&self.folder);
            fs::remove_dir_all(&self.folder).unwrap();
        }
    }

    #[test]
    fn versions_and_change_notes_are_exact() {
        for valid in ["0.2.0", "1.20.345"] {
            assert!(version_number(valid));
        }
        for invalid in ["v0.2.0", "0.2", "0.02.0", "0.2.0-rc.1", "0.2.0.1", "0.2.+0", "0.2. 0"] {
            assert!(!version_number(invalid));
        }
        let notes = "## 0.2.01\nWrong.\n## [0.2.0] (today)\n\n- Right.\n\n## 0.1.5\nEarlier.\n";
        assert_eq!(changelog_section(notes, "0.2.0").as_deref(), Some("- Right."));
        assert!(changelog_section("## 0.2.0\n\n## 0.1.5\nEarlier.", "0.2.0").is_none());
    }

    #[test]
    fn internal_versions_and_the_change_note_must_match() {
        let f = Fixture::new();
        assert_eq!(metadata(&f.root).unwrap().version, "0.2.0");
        let lock = read(&f.root, "Cargo.lock").unwrap();
        fs::write(f.root.join("Cargo.lock"), lock.replace("version = \"0.2.0\"", "version = \"0.1.5\"")).unwrap();
        assert!(metadata(&f.root).unwrap_err().contains("internal package"));
        fs::write(f.root.join("Cargo.lock"), lock).unwrap();
        fs::write(f.root.join("CHANGELOG.md"), "## 0.1.5\nEarlier.\n").unwrap();
        assert!(metadata(&f.root).unwrap_err().contains("nonempty section"));
    }

    #[test]
    fn preparation_needs_merged_clean_source_but_not_a_tag() {
        let f = Fixture::new();
        assert!(check(&f.root, None).is_ok());
        assert!(check(&f.root, Some("v0.1.5")).unwrap_err().contains("does not match workspace"));
        assert!(check(&f.root, Some("v0.2.0")).is_err());
        fs::write(f.root.join("untracked.txt"), "pending").unwrap();
        assert!(check(&f.root, None).unwrap_err().contains("uncommitted or untracked"));
        fs::remove_file(f.root.join("untracked.txt")).unwrap();
        git(&f.root, &["checkout", "-b", "pending"]).unwrap();
        fs::write(f.root.join("pending.txt"), "pending").unwrap();
        git(&f.root, &["add", "."]).unwrap();
        git(&f.root, &["commit", "-m", "Not merged"]).unwrap();
        assert!(check(&f.root, None).unwrap_err().contains("not a commit merged"));
    }

    #[test]
    fn an_annotated_tag_in_a_detached_checkout_survives_master_advancing() {
        let f = Fixture::new();
        f.tag();
        let before = check(&f.root, Some("v0.2.0")).unwrap();
        fs::write(f.root.join("later.txt"), "later").unwrap();
        git(&f.root, &["add", "."]).unwrap();
        git(&f.root, &["commit", "-m", "Continue development"]).unwrap();
        git(&f.root, &["push", "origin", "master"]).unwrap();
        git(&f.root, &["checkout", "--detach", "v0.2.0"]).unwrap();
        assert_eq!(check(&f.root, Some("v0.2.0")).unwrap().commit, before.commit);
        git(&f.root, &["push", "origin", ":refs/tags/v0.2.0"]).unwrap();
        assert!(check(&f.root, Some("v0.2.0")).unwrap_err().contains("origin's v0.2.0 is missing"));
    }
}
