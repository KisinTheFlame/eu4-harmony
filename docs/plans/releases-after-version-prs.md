# Releases after version PRs merge

Status: decided 2026-11-14 at the owner's request after the first tagged release. Being implemented. Supersedes the manual tag-creation step of [automated-releases.md](automated-releases.md); keeps its packages, source checks, publication gates and local Workshop behavior. No new glossary terms or ADRs.

## Goal and scope

Merging a version PR into master is the approval to publish. The owner maintains the workspace version, internal lockfile versions and change note in that PR. GitHub performs the checks, builds both packages, creates the immutable version tag and publishes the completed Release without a second manual action.

Do not infer versions from commit-message prefixes or introduce a second bot-maintained changelog. Ordinary code PRs do not publish. Manual preparation remains optional and never publishes. Do not move the existing `v0.2.0` tag or rebuild its published attachments.

## Behavior

1. A push to master that changes `Cargo.toml` compares `workspace.package.version` before and after the merge.
2. A canonical three-part version increase requests publication. An unchanged version does not. A lower or malformed version is refused.
3. The selected merged commit must pass the existing source, lockfile and changelog checks, CI and optimized package validation.
4. The publication job creates the annotated `vX.Y.Z` tag at that commit. An existing tag is reused only when it selects the exact same source; it is never moved.
5. The same job prepares the draft, uploads and checks both ZIPs, and publishes it. A workflow-token tag push does not trigger another workflow, so tag creation and publication must stay in this run.
6. Failed attempts can be retried for the same source. A public matching release is not overwritten; source changes require a new version PR. A newer version does not cancel an earlier publication. Attempts for the same source are serialized.
7. A manual workflow dispatch on master only checks and packages. It neither creates a tag nor publishes.

Keep the normal GitHub workflow token, scoped contents-write permission in the publication job, two separate Windows x64 ZIPs and local Steam authentication. Steam upload uses the automatically created tag and remains a separate local command.

## How it fits together

- `.github/scripts/release_request.py` reads the workspace version through `tomllib` and selects publication only for a validated master push with a version increase. Its tests cover patch, minor and major increases, unchanged versions, invalid or lowered versions, event identity and optional preparation.
- `.github/workflows/release.yml` adds the cheap request check before source validation and the shared CI. It creates and publishes from the selected merged source, with no tag-push trigger.
- `.github/scripts/release.mjs` creates a missing annotated tag and reference through the GitHub API, checks any existing or racing reference, then uses the existing draft recovery and publication checks.
- `.github/workflows/ci.yml` runs the request and publication tests. User and developer documentation describe merging the version PR as the last release action.

## Parts

- **V1 Detect the version PR.** Check the before and after source, canonical versions and increased version; skip ordinary merges and preserve nonpublishing manual preparation.
- **V2 Create the tag and publish.** Create the immutable tag only after checks and packaging, handle retries and reference races without moving existing tags, and publish in the same run.
- **V3 Verify and document.** Add offline tests for request selection, event guards and tag creation; lint the workflow, update the paired READMEs and release guide, and verify cloud preparation after merge.

Every part is done when its tests pass and documentation agrees. An actual new-version publication through this new trigger is only verified when a later version PR is merged; do not publish an unnecessary version or move the first release tag solely to test the trigger.

## Evidence and limits

Version 0.2.0 was published through the original tag-triggered workflow. Both public ZIPs were downloaded, their exact file sets and CRCs checked, and their binaries identified as Windows x64. The framework ZIP holds the two DLLs and the CLI ZIP holds the executable. That proves the packaging and publication path, not this new version-merge trigger.

GitHub documents that events caused by `GITHUB_TOKEN` do not start another workflow, except explicit `workflow_dispatch` and `repository_dispatch`. The Git data API requires creating both an annotated tag object and its reference. The tagger is optional. Sources: [workflow triggers](https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow), [creating tag objects](https://docs.github.com/en/rest/git/tags#create-a-tag-object).
