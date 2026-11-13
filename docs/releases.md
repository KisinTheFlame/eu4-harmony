# Releases

A pushed `vX.Y.Z` tag requests a GitHub Release from that exact source. Ordinary merges into master run CI without publishing. The release page becomes public only after its two binary packages are ready. The decisions and implementation record are in [automated-releases.md](plans/automated-releases.md).

## Downloads

| Package | Contents | Use |
| --- | --- | --- |
| `eu4-harmony-<version>-windows-x64.zip` | `version.dll`, `eu4-harmony/eu4_harmony_core.dll` | Extract into the game folder beside `eu4.exe` with the game closed |
| `eu4-harmony-cli-<version>-windows-x64.zip` | `eu4-harmony.exe` | Extract wherever the command line tool is run; it finds the game through Steam or `EU4_GAME_DIR` |

The packages contain no development settings, debug symbols or checksum file. Both come from the same optimized player build. The CLI is optional for players. GitHub also provides its usual source archives.

A framework installed from the ZIP is updated by downloading and extracting a newer ZIP. The local core takes precedence over the Workshop core; Steam does not update that local copy. Remove `eu4-harmony/eu4_harmony_core.dll` from the game folder to use the subscribed Workshop core again. See the [installation instructions](../README.md#installing).

## Preparing a version

Raise `workspace.package.version` in `Cargo.toml`, update every internal package version in `Cargo.lock`, and add a nonempty `## X.Y.Z` section to `CHANGELOG.md`. Keep a canonical three-part version, for example `0.2.0`. The same changelog section supplies the GitHub notes and any local Workshop upload's change note.

Merge these changes through a pull request and complete the local game-dependent checks appropriate to the runtime changes. GitHub runs formatting, builds, tests and both clippy configurations; it does not have EU4 and cannot perform the offline dependency-point checks or running-game scenarios.

Before the first tag, the Release workflow's manual preparation run on master exercises validation, checks and package creation without publishing:

```bash
gh workflow run release.yml --ref master
```

Its Actions artifacts are preparation results, not an additional publication channel. A preparation run never creates a public Release or a version tag.

## Publishing

From clean source already merged into master, create and push the version tag. For example, after checking out the intended merged commit:

```bash
git tag -a v0.2.0 -m "Release Harmony 0.2.0"
```

```bash
git push origin v0.2.0
```

The Release workflow checks that:

- the tag, workspace and internal lockfile versions agree;
- the version has a nonempty change note;
- the tagged commit is merged into master;
- the exact selected source passes the existing CI checks;
- the player-profile build and both archive checks succeed.

Master can advance after the tag without changing the selected source. The release build uses `cargo xtask release --no-workshop`, separate from CI's unoptimized release-profile builds. It neither needs the game nor renders Workshop images.

Only the publication job can write a Release, using GitHub's normal workflow token. No personal token, Steam credentials or self-hosted runner is required.

## Failures and retries

Rerun a failed workflow for the same unchanged tag. A matching draft can have partial attachments repaired before publication. A newer version does not cancel a publication in progress; concurrent attempts for the same tag are serialized.

An already public matching release is complete and its attachments are not overwritten. A conflicting source or missing expected public attachment fails rather than silently repairing a version players may already have downloaded. A delayed older version must not replace a newer version as the latest release.

Do not move a version tag. A fix to the tagged source, including its workflow code, needs a new version and tag even if the first attempt never became public. Test preparation before the initial tag to catch configuration and packaging failures first.

## Local commands and Workshop upload

`cargo xtask release-check --out target/dist` checks clean source merged into master and writes internal `release.json` metadata. Add `--tag vX.Y.Z` to require the existing local and remote tag at HEAD. This metadata is passed between workflow jobs and is not a Release attachment.

`cargo xtask release` creates the two ZIPs in `target/dist`, preserving that metadata, and also packs `target/workshop/harmony` and renders its comparison images. Use `--out <folder>` for another package folder or `--no-workshop` to skip the Workshop and image work. Every archive is checked for its exact file set and bytes against the player build.

Workshop upload is independent of GitHub publication. From a clean checkout of the version tag, pack the item and upload locally:

```bash
cargo xtask release
```

```bash
cargo xtask workshop-upload --user <steam-login>
```

SteamCMD handles account authentication locally. The command requires the existing local and remote tag to select HEAD, that commit merged into master, and the packed stamp, version and item id to match. A detached checkout is supported. Upload never creates or moves a tag. The initial creation of a Workshop item still uses `--create` and records its id in `workshop/item.toml`; see [development tools](dev-tools.md#the-first-upload).
