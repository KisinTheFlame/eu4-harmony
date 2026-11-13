# Harmony's Workshop item

The sources of what the Steam Workshop shows of Harmony ([docs/plans/workshop-release.md](../docs/plans/workshop-release.md)). `cargo xtask release` packs the item from them into `target\workshop\`, and `cargo xtask workshop-upload` uploads it locally ([docs/dev-tools.md](../docs/dev-tools.md)). The version's existing local and remote tag must select the clean source being uploaded; upload never creates a tag. GitHub Releases use the separate framework and CLI ZIPs, with `--no-workshop` to omit this item and image rendering ([releases](../docs/releases.md)).

| File | What it is |
| --- | --- |
| `item.toml` | `published_file_id`: the item's id on the Workshop, 0 until `workshop-upload --create` makes the item and records it. The proxy is built with it |
| `descriptor.mod` | The item's descriptor, `{version}` filled in with the workspace's version, and `remote_file_id` added once the item has an id |
| `thumbnail.png` | The cover: square, under 1 MB, also the item's preview image |
| `description.<language>.bbcode` | The item page's description in Steam's BBCode, one file for each language by Steam's names (`english`, `schinese`), pasted on the page by language: the upload leaves the description alone |
| `comparison.<language>.html` | The comparison with the unmodified game, for each language, which the release renders into `target\workshop\images\comparison.<language>.png` with Microsoft Edge, in a window 1200×4000 unless the page names another in `<meta name="workshop-size" content="<width>x<height>">`, and then cut where the page's content ends: the rows at its foot all of the page's plain background go. The images are uploaded as preview images on the item's page and shown in the description from there |
