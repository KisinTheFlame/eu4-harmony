/// Parses `<key> = <number>` from `workshop/item.toml`, ignoring blank lines
/// and `#` comments.
fn parse_key(item: &str, expected: &str) -> Option<u64> {
    item.lines().find_map(|line| {
        let (key, value) = line.split('#').next()?.split_once('=')?;
        (key.trim() == expected).then(|| value.trim().parse().ok())?
    })
}

/// Parses `published_file_id = <number>` from `workshop/item.toml`, ignoring
/// blank lines and `#` comments.
pub fn parse_published_file_id(item: &str) -> Option<u64> {
    parse_key(item, "published_file_id")
}

/// Parses `beta_published_file_id = <number>` from `workshop/item.toml`,
/// ignoring blank lines and `#` comments.
pub fn parse_beta_published_file_id(item: &str) -> Option<u64> {
    parse_key(item, "beta_published_file_id")
}

/// Parses `codename = "<name>"` from `[workspace.metadata.harmony]` in the
/// root `Cargo.toml`. Returns `None` when the section or key is omitted.
pub fn parse_workspace_codename(cargo_toml: &str) -> Option<String> {
    let mut in_section = false;
    for raw in cargo_toml.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_section = line == "[workspace.metadata.harmony]";
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if key.trim() != "codename" {
            continue;
        }
        let value = value.trim();
        let inner = value.strip_prefix('"')?.strip_suffix('"')?.trim();
        if !inner.is_empty() && !inner.contains(['\\', '"', '\n', '\r']) {
            return Some(inner.to_string());
        }
        return None;
    }
    None
}
