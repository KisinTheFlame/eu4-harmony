# Automated releases

Status: decided on 2026-11-14; Q1 to Q9 and the complete implementation plan accepted by the owner. A1 to A5 built and verified on 2026-11-14: [Harmony 0.2.0](https://github.com/KisinTheFlame/eu4-harmony/releases/tag/v0.2.0) published and both public downloads checked. The manual tag-creation step is superseded by [releases-after-version-prs.md](releases-after-version-prs.md), following the owner's subsequent preference for publication when a version PR merges. GitHub Releases, version-tag triggers, separate framework and CLI packages, source validation, CI gates, publication after attachment completion and retry rules are accepted. Steam upload remains a local command. Version 0.2.0 is the first automated release. The local checks and hidden scenarios have passed ([the run](../prototype-log/2026-11-14-release-0.2.0-smoke.md)); v0.2.0 has been published and both public ZIPs verified. No new glossary terms or ADRs are needed: the terms are existing project language or general release terminology, and the Workshop's delivery and core-loading decision remains in force.

## Goal and scope

Automate new Harmony releases on GitHub. Keep the version PR, with the workspace version in `Cargo.toml`, its internal lockfile records and the version's section of `CHANGELOG.md`. After merging it, pushing a `vX.Y.Z` tag explicitly requests publication. An ordinary master merge runs CI without publishing.

Publish two Windows x64 binary packages: the framework installation package and the CLI package. Do not publish debug symbols or a SHA-256 checksum file. Finish preparation and validation before making the release public.

Keep Workshop uploads on this computer through the existing command, adapted to the same tagged version. Do not add Steam credentials, an unattended Steam login, self-hosted runners or a game installation to the GitHub workflow. The Workshop delivery model and core lookup order remain those of [workshop-release.md](workshop-release.md) and [ADR the-framework-ships-on-the-workshop](../adr/2026-11-11-the-framework-ships-on-the-workshop.md).

## Accepted design tree

- **D1 / Q1 Publication channels.** GitHub Releases are the primary automated publication target. Steam automation was evaluated and is deferred by Q9.
- **D2 / Q2 Release trigger.** After the version PR merges, pushing `vX.Y.Z` requests an automated GitHub Release. Merging the PR alone does not publish. This replaces the initial, unaccepted proposal to publish on a version increase merged into master.
- **D3 / Q3 First automated release.** Version 0.2.0 is the first release through the new automation; do not publish it manually while building the automation.
- **D4 / Q4 Release attachments.** Publish separate framework and CLI packages. The framework package holds `version.dll` and `eu4-harmony/eu4_harmony_core.dll`; the CLI package holds `eu4-harmony.exe`. Do not publish a debug-symbol ZIP or a SHA-256 checksum file. The version's changelog section supplies the release notes.
- **D5 / Q5 Publication visibility.** Complete the build, upload and attachment validation before making the GitHub Release public. Failure leaves a failed run or draft rather than a public incomplete release.
- **D6 / Q6 Source and version consistency.** The tagged commit must already be merged into master, but need not remain master's latest commit while the release runs. The tag, workspace version, internal lockfile versions and changelog section must agree. Reject inconsistencies without editing source or moving the tag.
- **D7 / Q7 Required checks.** The exact tagged source must pass the existing CI checks and the optimized player build and package checks. Offline dependency-point checks and running-game scenarios remain local checks; a hosted GitHub runner does not require EU4.
- **D8 / Q8 Failure, duplicate requests and retries.** Serialize publication attempts for the same tag. A newer version does not cancel a publication in progress. Retry a failed attempt for the same unchanged tag; never overwrite the attachments of an already public version. A source fix requires a new version and tag.
- **D9 / Q9 Workshop scope.** Retain local Workshop upload capability for the same version and omit unattended Steam authentication from this release workflow. Adapt the local uploader to the tag-first flow instead of creating the tag after uploading.

## Behavior

### Requesting and validating a release

1. Merge the version and release-workflow changes through the existing pull request process.
2. Push a version tag, beginning with `v0.2.0` for this work.
3. Resolve the tag to its exact commit and verify that the commit is reachable from master. Master advancing afterward does not change the selected source.
4. Verify the tag's version against the workspace, all internal lockfile records and a nonempty changelog section. A failed validation makes no source edits or public release.
5. Require the existing checks for the selected source: formatting, the default build, tests, the developer build and both clippy configurations. Share the check definitions with ordinary CI where useful without changing their requirements.
6. Build the framework and CLI with the optimized `player` profile, with no development features, and validate the packages.

Use hosted Windows runners for the Windows binaries. The normal GitHub Actions token is sufficient for release publication; grant write access only to the publication job. No Steam account or personal access token is part of this plan.

### Artifacts and installation

| Artifact | Contents |
| --- | --- |
| `eu4-harmony-<version>-windows-x64.zip` | `version.dll`, `eu4-harmony/eu4_harmony_core.dll` |
| `eu4-harmony-cli-<version>-windows-x64.zip` | `eu4-harmony.exe` |

Both artifacts come from the same selected source and optimized build. Validate their exact expected file sets and successful archive creation. The CLI package is independently usable outside the game folder.

The framework archive is a manual installation and update route. As already implemented, a core copied into the game folder takes precedence over the Workshop core. Document that behavior beside the download and installation instructions so subscribing does not imply that Steam will update this local copy.

### Publication and recovery

- Prepare the version's release notes and both archives before publication. Create or reuse a draft for the selected version, finish its attachments, then publish.
- A retry may repair incomplete attachments in the unpublished draft for the same tagged commit. Refuse a conflicting release or source identity.
- An already public matching release is a completed publication; a rerun does not replace its attachments. A public release with conflicting or incomplete expected content is reported for investigation rather than silently overwritten.
- Keep the version tag unchanged. A fix to tagged source or workflow code requires a new version and tag, even if the original attempt did not publish.
- For final confirmation, use the usual latest-version behavior: a delayed older release must not replace a newer version as the latest release.
- Check preparation without publishing before pushing the initial release tag. A preparation or dry-run entry point must not become a second publication trigger.

### Local Workshop upload

The local command accepts the existing version tag when it resolves to the exact clean source being uploaded and that source is merged into master. Keep the version, changelog, item identity, descriptor and packed-commit checks. Do not require a local branch literally named master or equality with its advancing tip. Do not create or move a version tag during upload.

GitHub publication does not wait for a Workshop upload. Steam authentication continues to happen locally through SteamCMD. No automated upload to the Workshop is part of this implementation.

## How it fits together

- `.github/workflows/ci.yml`: retain the current checks and expose shared definitions if the release workflow needs them.
- `.github/workflows/release.yml`: validate the tag, require checks, build the player packages and publish after attachment validation. Scope write permission to the publication stage.
- `xtask/src/files.rs` and release helpers: separate the framework and CLI archives, retain the optimized build and check package contents.
- `xtask/src/workshop.rs`: align local source and tag checks with tag-first publication, preserving local authentication and packed-item validation.
- `README.md`, `README.zh-CN.md`, `docs/dev-tools.md` and `workshop/README.md` where applicable: explain downloads, manual installation, the separate CLI, tagging, retry behavior and local Workshop uploads. Keep translations matched.

## Parts

- **A1 Version and source checks.** Validate the requested version, tag target, master ancestry, workspace and lockfile versions and change note. Tests cover mismatches, missing notes, detached checkouts and master advancing beyond the tag.
- **A2 Separate packages.** Produce the framework and CLI ZIPs from one player build. Verify exact archive contents, absence of development settings and absence of CLI files from the framework package.
- **A3 Release workflow.** Connect the tag trigger, existing checks, optimized build, draft attachments and publication. Check the failure and repeat-run paths without publishing a test version.
- **A4 Local Workshop compatibility.** Upload an existing tagged version from a clean matching source, without creating a tag or requiring a branch named master. Test validation and refusal behavior without an unintended live upload.
- **A5 Documentation and first release.** Update user and developer documentation, finish the existing release PR, merge through the repository's required process, and push `v0.2.0` only after preparation passes. Verify the real GitHub Release and both downloadable packages.

Every part is done when its behavior is built and checked, the documentation matches, and A5 has a real successful publication to show. The implementation remains one effort and one pull request. Do not add a separate symbols or checksum deliverable.

## Research and existing evidence

The current uploader tags after Workshop upload and rejects a pre-existing tag. Its current branch-name and `HEAD == origin/master` checks cannot be reused unchanged for tag-triggered publication or a detached checkout.

A hosted Windows runner can build the framework and CLI without EU4. Packing uses Windows `tar.exe`. Workshop comparison-image rendering optionally uses Edge; GitHub binary publication does not require rendering those images. Current CI disables release-profile optimization and LTO for speed, so its normal build output is not the player package.

Read-only GitHub inspection found no repository Actions secrets, environments or self-hosted runners configured. Valve documents retaining SteamCMD login state for CI/CD, but restoration for this Workshop account on a new GitHub runner has not been tested. The owner chose local Workshop upload after considering that constraint.

GitHub supports pushed-tag workflows. cargo-dist documents that binary-release pattern; release-please documents automatically maintained release PRs that create tags and Releases when merged. These examples establish supported workflows, not relative usage. The owner chose tag triggers with the existing manually maintained version and changelog. The `release.published` event happens after publication, so it is not the trigger for preparing this release's initial attachments.

Sources: [GitHub workflow events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows), [cargo-dist](https://github.com/axodotdev/cargo-dist), [release-please](https://github.com/googleapis/release-please), [Valve SteamPipe CI/CD](https://partner.steamgames.com/doc/sdk/uploading#Using_SteamPipe_In_A_CI_CD_Environment), [Valve SteamCMD Workshop integration](https://partner.steamgames.com/doc/features/workshop/implementation#SteamCmd).
