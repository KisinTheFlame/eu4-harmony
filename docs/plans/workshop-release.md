# Workshop release plan

Status: decided 2026-11-11; W1 to W7 built 2026-11-11, W4 and W5 and the core's side of W1 tried in the game ([The settings window without explanations, and the name Harmony](../prototype-log/2026-11-11-harmony-settings-window.md)); the proxy's own side of W1, and W8 as far as the first install, tried 2026-11-11 ([Harmony installed from its Workshop item](../prototype-log/2026-11-11-harmony-from-the-workshop.md)); W8 done 2026-11-11 but for uninstalling, not yet tried: Harmony (3837775887) and Regional Combat (3837793510) visible to friends, Regional Combat requiring Harmony. The decision to ship the framework on the Steam Workshop is [ADR the-framework-ships-on-the-workshop](../adr/2026-11-11-the-framework-ships-on-the-workshop.md), which supersedes the line of [v1.md](v1.md) that kept it off the Workshop. Adds Harmony to [GLOSSARY.md](../../GLOSSARY.md). The original upload-then-tag sequence is superseded by [automated-releases.md](automated-releases.md), which keeps local Workshop uploads and uses existing version tags.

## Goal and scope

- **Goal**: players get the framework, and mods built on it, by subscribing on the Steam Workshop, and get its updates the same way; everything a player or a mod author reads about it is ready for a public release. The first release goes to the owner's friends, as a Workshop item visible to friends only, but nothing a player reads says so.
- **In scope**: where the proxy finds the core; the Workshop folder the release build packs, and its upload; the name players read; the settings window's texts; README, the Workshop descriptions, covers and the comparison with the unmodified game; a changelog; Regional Combat's Workshop item, in its own repository.
- **Out of scope**: shipping the command line tool to mod authors (they build it from source for now); updating `version.dll` itself; detecting other text patches; languages beyond those the framework's texts already have.

## Behavior

### Installing and uninstalling, as a player

1. Uninstall any other patch that draws the game's Chinese, Japanese or Korean text, as that patch says.
2. Subscribe to Harmony on the Workshop.
3. Copy `version.dll` from Harmony's Workshop folder into the game's root folder, next to `eu4.exe`; the Workshop description shows the steps. A `version.dll` already there that is not Harmony's is moved aside first.

Harmony works from then on, in every playset: it is not enabled in the launcher. Its updates arrive with the Workshop's. Unsubscribing turns it off, the game running as the unmodified game; deleting `version.dll` from the root folder removes the rest. A mod built on Harmony is enabled in the playset as any mod, and names Harmony as a required item on the Workshop.

### Where the proxy finds the core

In this order, the first that exists:

1. the core `-harmony_core=<path>` names on the command line (test games), which must load;
2. `eu4-harmony\eu4_harmony_core.dll` beside `version.dll` (a developer's `cargo xtask install`);
3. `eu4-harmony\eu4_harmony_core.dll` in Harmony's own Workshop item: `<library>\steamapps\workshop\content\236850\<id>\`, where `<library>` holds the game folder and `<id>` is Harmony's published file id, built into the proxy.

Only Harmony's own item is looked at, never any other mod's folder: any mod on the Workshop could ship a file of that name, and the proxy runs whatever core it loads with every right in the game.

The proxy writes its log into the user folder's `eu4-harmony\logs\proxy.log`, beside the core's, so the game's root folder holds nothing but `version.dll`.

The proxy and the core agree on a version of what passes between them. A core that finds a proxy of another version starts nothing and tells the player, in the startup failure notice and the startup report, to copy `version.dll` from Harmony's Workshop folder again.

### The name

Players and mod authors read "Harmony": the Workshop title, README's title, the main menu's buttons ("Harmony settings", "Harmony mods"), the windows' titles, the notices and the startup report, the template mod's comments and the skills' texts. Paths, folders, commands and keys keep `eu4-harmony`: the mod folder `eu4-harmony\`, `eu4-harmony.exe`, `EU4HARMONY_` keys, the user folder's `eu4-harmony\`.

A mod built on Harmony may say so in its Workshop title ("Regional Combat · powered by Harmony"); its `descriptor.mod` name does not.

### The settings window

A setting's label says what it does; the window has no explanations under its settings. That a change takes effect at the next start shows beside the save button only while such a change is pending. The window's foot shows Harmony's version and where to report a problem (GitHub Issues).

### The Workshop items

| | Harmony | Regional Combat |
| --- | --- | --- |
| Title | Harmony | Regional Combat · powered by Harmony / 区域战斗 · 由 Harmony 驱动 |
| Holds | `descriptor.mod`, `thumbnail.png`, `version.dll`, `eu4-harmony\eu4_harmony_core.dll`; no `.exe` | the mod |
| Uploaded by | `cargo xtask workshop-upload` (SteamCMD) | SteamCMD, from the folder `eu4-harmony package` writes |
| Requires | | Harmony |

`supported_version` is `v1.37.5` in both, and in the template `eu4-harmony new` writes.

Each item has a description in English and one in Simplified Chinese, kept in its repository and pasted on the item's page by language. Harmony's opens with a comparison with the unmodified game, an image rendered for each language and uploaded as a preview image of the item; README has the same comparison as a table.

## How it fits together

- `proxy/`: the order above, the published file id, the log's folder, the interface version.
- `xtask/`: `release` packs `target\workshop\harmony\` beside the player package; `workshop-upload` writes SteamCMD's build file, takes the change note from `CHANGELOG.md`, and tags the version uploaded.
- `workshop/`: the sources of what the Workshop shows: the cover, the descriptions, the comparison images' pages, and the item's published file id.
- `core/localisation/`: the name and the settings window's texts.

## Parts

- **W1 The proxy finds the core in Harmony's Workshop item**, logs into the user folder, and checks the interface version.
- **W2 `cargo xtask release` packs the Workshop folder**, with the cover and the descriptor from `workshop/`.
- **W3 `cargo xtask workshop-upload`**, and `CHANGELOG.md`: an upload needs the version's section in the changelog, no tag of that version yet, a clean checkout of master; it tags `vX.Y.Z` after.
- **W4 The name Harmony** in what players and mod authors read; `supported_version` `v1.37.5`.
- **W5 The settings window without explanations**, its pending restart line and its foot.
- **W6 README, the Workshop descriptions, the comparison**, in English and Simplified Chinese.
- **W7 Regional Combat's Workshop item**, in its repository: name, cover, descriptions, changelog, `supported_version`.
- **W8 The first release**: a private upload, the owner subscribing, installing from the description, playing and uninstalling; then visible to friends.

Every part is done when it is built, checked as AGENTS.md says, and, for W1 and W8, tried in the game with the run in [prototype-log/](../prototype-log/).
