/// Parses `published_file_id = <number>` from `workshop/item.toml`, ignoring
/// blank lines and `#` comments.
pub fn parse_published_file_id(item: &str) -> Option<u64> {
    item.lines().find_map(|line| {
        let (key, value) = line.split('#').next()?.split_once('=')?;
        (key.trim() == "published_file_id").then(|| value.trim().parse().ok())?
    })
}
