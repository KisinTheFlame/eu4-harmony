// A merged version increase requests publication. Create its immutable tag
// here and publish in the same run: workflow-token tag pushes do not start
// another workflow. Preparation and ordinary CI never call this entry point.
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}

export function validateMetadata(metadata) {
  requireThat(metadata && VERSION.test(metadata.version), "Invalid release version");
  requireThat(metadata.tag === `v${metadata.version}`, "The tag and version differ");
  requireThat(/^[a-f0-9]{40}$/.test(metadata.commit), "Invalid release commit");
  requireThat(typeof metadata.notes === "string" && metadata.notes.trim(), "Missing release notes");
  requireThat(metadata.framework_asset === `eu4-harmony-${metadata.version}-windows-x64.zip`, "Unexpected framework archive name");
  requireThat(metadata.cli_asset === `eu4-harmony-cli-${metadata.version}-windows-x64.zip`, "Unexpected CLI archive name");
  return metadata;
}

export async function readArtifacts(metadata, directory) {
  validateMetadata(metadata);
  return Promise.all([metadata.framework_asset, metadata.cli_asset].map(async (name) => {
    const data = await readFile(path.join(directory, name));
    requireThat(data.length > 4 && data.subarray(0, 4).equals(Buffer.from([0x50, 0x4b, 0x03, 0x04])), `Invalid ZIP archive: ${name}`);
    return { name, data };
  }));
}

export function validateAssets(assets, names, complete) {
  const found = new Set();
  for (const asset of assets) {
    requireThat(names.includes(asset.name), `Unexpected release attachment: ${asset.name}`);
    requireThat(!found.has(asset.name), `Duplicate release attachment: ${asset.name}`);
    found.add(asset.name);
    if (complete) {
      requireThat(asset.state === "uploaded" && Number.isSafeInteger(asset.size) && asset.size > 0, `Incomplete release attachment: ${asset.name}`);
    }
  }
  if (complete) requireThat(found.size === names.length, "The release does not have both expected attachments");
}

export async function resolveTag(api, tag) {
  let object = (await api.request("GET", `/git/ref/tags/${encodeURIComponent(tag)}`)).object;
  // Annotated tags can themselves name another annotated tag.
  for (let depth = 0; object?.type === "tag" && depth < 16; depth++) {
    requireThat(/^[a-f0-9]{40}$/.test(object.sha), "Invalid annotated tag identity");
    object = (await api.request("GET", `/git/tags/${object.sha}`)).object;
  }
  requireThat(object?.type === "commit" && /^[a-f0-9]{40}$/.test(object.sha), "The release tag does not resolve to a commit");
  return object.sha;
}

export async function ensureTag(api, metadata) {
  let existing;
  try {
    existing = await resolveTag(api, metadata.tag);
  } catch (error) {
    if (error.status !== 404) throw error;
  }
  if (existing !== undefined) {
    requireThat(existing === metadata.commit, "The existing tag names another source");
    return existing;
  }
  const annotated = await api.request("POST", "/git/tags", {
    tag: metadata.tag,
    message: `Release Harmony ${metadata.version}`,
    object: metadata.commit,
    type: "commit",
  });
  requireThat(/^[a-f0-9]{40}$/.test(annotated.sha), "GitHub did not create the annotated tag");
  try {
    await api.request("POST", "/git/refs", { ref: `refs/tags/${metadata.tag}`, sha: annotated.sha });
  } catch (error) {
    // Another request may have created this exact ref. Never update it.
    if (error.status !== 409 && error.status !== 422) throw error;
  }
  const selected = await resolveTag(api, metadata.tag);
  requireThat(selected === metadata.commit, "The created tag does not name the validated source");
  return selected;
}

async function list(api, route) {
  const values = [];
  for (let page = 1; ; page++) {
    const batch = await api.request("GET", `${route}?per_page=100&page=${page}`);
    requireThat(Array.isArray(batch), `Invalid GitHub listing: ${route}`);
    values.push(...batch);
    if (batch.length < 100) return values;
  }
}

function newerVersion(candidate, current) {
  if (!VERSION.test(candidate)) return false;
  const left = candidate.split(".").map(BigInt);
  const right = current.split(".").map(BigInt);
  for (let i = 0; i < left.length; i++) {
    if (left[i] !== right[i]) return left[i] > right[i];
  }
  return false;
}

function validateRelease(release, metadata) {
  requireThat(Number.isSafeInteger(release.id) && release.id > 0, "Invalid release identity");
  requireThat(release.tag_name === metadata.tag && release.target_commitish === metadata.commit, "The existing release names another source");
  requireThat(!release.prerelease, "The existing release is a prerelease");
}

export async function publishRelease({ api, metadata, artifacts, createTag = false }) {
  validateMetadata(metadata);
  const names = [metadata.framework_asset, metadata.cli_asset];
  requireThat(artifacts.length === 2 && names.every((name) => artifacts.filter((artifact) => artifact.name === name).length === 1), "The build must supply exactly the two expected archives");
  for (const artifact of artifacts) {
    requireThat(Buffer.isBuffer(artifact.data) && artifact.data.length > 0, `Empty build attachment: ${artifact.name}`);
  }
  const selected = createTag ? await ensureTag(api, metadata) : await resolveTag(api, metadata.tag);
  requireThat(selected === metadata.commit, "The remote tag no longer names the validated source");

  // Listing with the publisher token also finds drafts; get-by-tag alone is
  // documented for published releases and cannot be our recovery lookup.
  const releases = await list(api, "/releases");
  const matching = releases.filter((release) => release.tag_name === metadata.tag);
  requireThat(matching.length <= 1, "More than one release names this tag");
  let release = matching[0];
  if (release) {
    validateRelease(release, metadata);
    const assets = await list(api, `/releases/${release.id}/assets`);
    validateAssets(assets, names, !release.draft);
    if (!release.draft) return { release, alreadyPublished: true };
    // A failed attempt may have left either ZIP behind. Replace only the
    // known attachments while unpublished, so both come from this build.
    for (const asset of assets) {
      requireThat(Number.isSafeInteger(asset.id) && asset.id > 0, "Invalid attachment identity");
      await api.request("DELETE", `/releases/assets/${asset.id}`);
    }
    release = await api.request("PATCH", `/releases/${release.id}`, {
      tag_name: metadata.tag,
      target_commitish: metadata.commit,
      name: `Harmony ${metadata.version}`,
      body: metadata.notes,
      draft: true,
      prerelease: false,
    });
  } else {
    release = await api.request("POST", "/releases", {
      tag_name: metadata.tag,
      target_commitish: metadata.commit,
      name: `Harmony ${metadata.version}`,
      body: metadata.notes,
      draft: true,
      prerelease: false,
      make_latest: "false",
    });
  }
  validateRelease(release, metadata);
  requireThat(release.draft, "The release became public before its attachments were ready");
  for (const artifact of artifacts) {
    const uploaded = await api.upload(release.id, artifact.name, artifact.data);
    requireThat(uploaded.name === artifact.name && uploaded.state === "uploaded" && uploaded.size === artifact.data.length, `GitHub did not finish uploading ${artifact.name}`);
  }
  const assets = await list(api, `/releases/${release.id}/assets`);
  validateAssets(assets, names, true);
  // Check the tag again after uploads, before changing public state.
  requireThat(await resolveTag(api, metadata.tag) === metadata.commit, "The remote tag changed during preparation");

  const currentReleases = await list(api, "/releases");
  const hasNewer = currentReleases.some((candidate) => !candidate.draft && !candidate.prerelease
    && candidate.tag_name?.startsWith("v") && newerVersion(candidate.tag_name.slice(1), metadata.version));
  // GitHub documents legacy as choosing by creation date and higher semantic
  // version. An already visible higher version explicitly keeps its place.
  // https://docs.github.com/en/rest/releases/releases#update-a-release
  const published = await api.request("PATCH", `/releases/${release.id}`, {
    draft: false,
    make_latest: hasNewer ? "false" : "legacy",
  });
  validateRelease(published, metadata);
  requireThat(!published.draft, "GitHub did not publish the completed release");
  return { release: published, alreadyPublished: false };
}

export class GitHub {
  constructor(repository, token, fetcher = fetch) {
    requireThat(/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository), "Invalid GitHub repository");
    requireThat(typeof token === "string" && token.length > 0, "Missing GitHub token");
    this.repository = repository;
    this.token = token;
    this.fetcher = fetcher;
  }

  async send(method, url, body, binary = false) {
    const response = await this.fetcher(url, {
      method,
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: `Bearer ${this.token}`,
        "X-GitHub-Api-Version": "2022-11-28",
        ...(body === undefined ? {} : { "Content-Type": binary ? "application/zip" : "application/json" }),
      },
      ...(body === undefined ? {} : { body: binary ? body : JSON.stringify(body) }),
    });
    const text = await response.text();
    let result;
    try {
      result = text ? JSON.parse(text) : null;
    } catch {
      throw new Error(`GitHub ${method} failed to return JSON (HTTP ${response.status})`);
    }
    if (!response.ok) {
      const error = new Error(`GitHub ${method} failed (HTTP ${response.status}): ${result?.message || "request refused"}`);
      error.status = response.status;
      throw error;
    }
    return result;
  }

  request(method, route, body) {
    return this.send(method, `https://api.github.com/repos/${this.repository}${route}`, body);
  }

  upload(releaseId, name, data) {
    return this.send("POST", `https://uploads.github.com/repos/${this.repository}/releases/${releaseId}/assets?name=${encodeURIComponent(name)}`, data, true);
  }
}

async function main() {
  const directory = process.argv[2];
  requireThat(directory, "Usage: node release.mjs <package directory>");
  const metadata = validateMetadata(JSON.parse(await readFile(path.join(directory, "release.json"), "utf8")));
  requireThat(process.env.GITHUB_EVENT_NAME === "push" && process.env.GITHUB_REF === "refs/heads/master"
    && process.env.RELEASE_REQUESTED === "true" && process.env.GITHUB_SHA === metadata.commit,
  "Only a version increase merged into master may publish a release");
  const artifacts = await readArtifacts(metadata, directory);
  const api = new GitHub(process.env.GITHUB_REPOSITORY, process.env.GH_TOKEN);
  const { release, alreadyPublished } = await publishRelease({ api, metadata, artifacts, createTag: true });
  console.log(`${alreadyPublished ? "Already published" : "Published"}: ${release.html_url}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
