# Harmony's Workshop item

The sources of what the Steam Workshop shows of Harmony ([docs/releases.md](../docs/releases.md)). `cargo xtask release` packs the item from them into `target\workshop\`, and `cargo xtask workshop-upload` uploads it locally ([docs/dev-tools.md](../docs/dev-tools.md)). The version's existing local and remote tag must select the clean source being uploaded; upload never creates a tag. GitHub Releases use the separate framework and CLI ZIPs, with `--no-workshop` to omit this item ([releases](../docs/releases.md)).

| File | What it is |
| --- | --- |
| `item.toml` | `published_file_id`: the item's id on the Workshop, 0 until `workshop-upload --create` makes the item and records it. The proxy is built with it |
| `descriptor.mod` | The item's descriptor, `{version}` filled in with the workspace's version, and `remote_file_id` added once the item has an id |
| `media/raw/` | Clean text-free raw source artwork (`thumbnail-base.png`, `banner-base.png`) and unannotated in-game captures. Generated text-overlaid images are never committed |
| `media/build.py` | Deterministic Pillow build script (`python workshop/media/build.py`) that renders the finished covers (`thumbnail.png`, `thumbnail-zh.png`, 1024×1024, < 1 MiB), the Chinese README banner (`banner.zh-CN.jpg`), and 5 showcase slides (`01-cjk-fonts.png`–`05-install-guide.png`) into `target/workshop-media/`. Run it before `cargo xtask release` so the built `thumbnail.png` is packed into `target\workshop\harmony\` |
| `description.<language>.bbcode` | The item page's description in Steam's BBCode (including native `[table]` comparisons, Featured Harmony Mods, and FAQ), one file per Steam language (`english`, `schinese`, `tchinese`, `japanese`, `korean`), pasted on the page by language: the upload leaves localized titles and descriptions alone |
