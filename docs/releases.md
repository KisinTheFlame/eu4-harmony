# Releases

Merging a version PR into master requests a GitHub Release. GitHub checks the merged source, builds the packages, creates its `vX.Y.Z` tag and publishes the completed release. No manual tag command or second approval is needed. Ordinary code PRs do not publish. The design records are [releases-after-version-prs.md](releases.md) and the original [automated-releases.md](releases.md).

## Downloads

| Package | Contents | Use |
| --- | --- | --- |
| `eu4-harmony-<version>-windows-x64.zip` | `version.dll`, `eu4-harmony/eu4_harmony_core.dll`, `eu4-harmony/eu4_harmony_crash_dumps.dll`, `eu4-harmony/fonts/*.ttf` | Extract into the game folder beside `eu4.exe` with the game closed |
| `eu4-harmony-cli-<version>-windows-x64.zip` | `eu4-harmony.exe` | Extract wherever the command line tool is run; it finds the game through Steam or `EU4_GAME_DIR` |

The packages contain no development settings, debug symbols or checksum file. Both come from the same optimized player build. The CLI is optional for players. GitHub also provides its usual source archives.

A framework installed from the ZIP is updated by downloading and extracting a newer ZIP. The local core takes precedence over the Workshop core; Steam does not update that local copy. Remove `eu4_harmony_core.dll`, `eu4_harmony_crash_dumps.dll` and `fonts/` from the game folder's `eu4-harmony/` directory to use the subscribed Workshop core again. See the [installation instructions](../README.md#installing).

## Publishing a version

Raise `workspace.package.version` in `Cargo.toml` (and optionally set `[workspace.metadata.harmony].codename` to a major-version city codename such as `"Constantinople"`), update every internal package version in `Cargo.lock`, and add a nonempty `## X.Y.Z` section to `CHANGELOG.md`. Keep a canonical three-part version, for example `1.0.0`. The same changelog section supplies the GitHub notes and any local public Workshop upload's change note, and the optional codename appears in the GitHub Release title, in-game corner bar and About page, and framework logs.

Complete the local game-dependent checks appropriate to the runtime changes and merge the version PR. That merge is the release approval. The Release workflow compares the workspace version before and after the merge. It publishes an increase, skips an unchanged version and refuses a lowered or malformed version.

The workflow then requires:

- matching workspace and internal lockfile versions, and a nonempty change note;
- the selected source already merged into master;
- the existing CI checks for that exact source;
- a successful optimized player build and exact package validation.

After those checks it creates the annotated version tag at the selected commit, uploads and checks both ZIPs in a draft, and makes the release public. An existing tag must select exactly the same commit; the workflow never moves it. Master advancing while the job runs does not change the selected source.

The tag and Release are created in the same workflow. A tag created with GitHub's normal workflow token does not trigger another push workflow. Only the publication job has contents-write permission; no personal token, Steam credentials or self-hosted runner is required.

CI runs formatting, builds, tests and both clippy configurations. It does not have EU4 and cannot perform offline dependency-point checks or running-game scenarios. The actual packages use the optimized `player` profile, separate from CI's unoptimized release-profile checks. GitHub packaging needs neither the game nor SteamCMD.

## Optional preparation

A manual preparation run on master checks the configuration, source and packages without creating a tag or publishing:

```bash
gh workflow run release.yml --ref master
```

This is optional and is not a step maintainers must perform for every version. Its Actions artifacts are preparation results, not a GitHub Release. Normal publication starts by merging the version PR.

## Failures and retries

Rerun a failed workflow for the same unchanged source. An existing matching tag is reused and a matching draft can have partial attachments repaired before publication. A newer version does not cancel a publication in progress; attempts for the same selected source are serialized.

An already public matching release is complete and its attachments are not overwritten. A conflicting source, tag or missing expected public attachment fails rather than silently repairing a version players may already have downloaded. A delayed older version must not replace a newer version as the latest release.

Do not move a version tag. A fix to source used for publication, including workflow code, needs a new version PR. Changes that keep the workspace version unchanged do not request another publication. Version 0.2.0 remains on its original tag and was published by the earlier tag-triggered workflow.

## Local commands and Workshop upload

`cargo xtask release-check --out target/dist` checks clean source merged into master and writes internal `release.json` metadata. It does not need the tag yet. Add `--tag vX.Y.Z` when checking an existing local and remote tag at HEAD. This metadata is passed between workflow jobs and is not a Release attachment.

`cargo xtask release` creates the two ZIPs in `target/dist`, preserving that metadata, and also packs `target/workshop/harmony` (or `target/workshop/harmony-beta` with `--channel beta`). Use `--out <folder>` for another package folder or `--no-workshop` to skip the Workshop item. Every archive is checked for its exact file set and bytes against the player build.

Workshop upload is independent of GitHub publication. For the public channel, from a clean checkout of the automatically created version tag, build the media covers, pack the item and upload locally:

```bash
python workshop/media/build.py
cargo xtask release
cargo xtask workshop-upload --user <steam-login>
```

For the beta channel (`beta_published_file_id` in `workshop/item.toml`), no version tag is required: pack and upload any clean commit merged into `origin/master` (or any clean branch with `--allow-branch`), optionally passing `--note "..."`:

```bash
python workshop/media/build.py
cargo xtask release --channel beta
cargo xtask workshop-upload --user <steam-login> --channel beta
```

SteamCMD handles authentication locally. Public uploads require the existing local and remote tag to select HEAD, that commit merged into master, and the packed stamp, version, channel and item id to match. A detached checkout is supported. Upload never creates or moves a tag. Initial creation of a channel's Workshop item uses `--create` (with `--channel public` or `--channel beta`) and records its id in `workshop/item.toml`; see [development tools](dev-tools.md#the-first-upload).
