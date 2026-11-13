import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { GitHub, publishRelease, readArtifacts, resolveTag, validateMetadata } from "./release.mjs";

const commit = "a".repeat(40);
const metadata = {
  version: "0.2.0",
  tag: "v0.2.0",
  commit,
  notes: "- Separate framework and CLI packages.",
  framework_asset: "eu4-harmony-0.2.0-windows-x64.zip",
  cli_asset: "eu4-harmony-cli-0.2.0-windows-x64.zip",
};
const names = [metadata.framework_asset, metadata.cli_asset];
const artifacts = names.map((name) => ({ name, data: Buffer.from("PK\x03\x04archive") }));
const clone = (value) => structuredClone(value);

function release(draft = true, assets = []) {
  return {
    id: 7,
    tag_name: metadata.tag,
    target_commitish: commit,
    name: "Harmony 0.2.0",
    draft,
    prerelease: false,
    html_url: "https://github.com/owner/repo/releases/tag/v0.2.0",
    assets,
  };
}

function asset(name, id = 20, size = 10) {
  return { id, name, state: "uploaded", size };
}

class FakeGitHub {
  constructor(existing) {
    this.release = clone(existing);
    this.otherReleases = [];
    this.calls = [];
    this.tagObject = { type: "commit", sha: commit };
    this.tagObjects = {};
    this.before = () => {};
    this.afterUpload = () => {};
  }

  async request(method, route, body) {
    this.calls.push({ method, route, body: clone(body) });
    this.before(method, route, body);
    if (route.startsWith("/git/ref/tags/")) return { object: clone(this.tagObject) };
    if (route.startsWith("/git/tags/")) return { object: clone(this.tagObjects[route.slice("/git/tags/".length)]) };
    if (method === "GET" && route.startsWith("/releases?")) {
      const page = Number(new URLSearchParams(route.split("?")[1]).get("page"));
      const values = [...this.otherReleases, ...(this.release ? [this.release] : [])];
      return clone(values.slice((page - 1) * 100, page * 100));
    }
    if (method === "GET" && route.startsWith("/releases/7/assets?")) return clone(this.release.assets);
    if (method === "POST" && route === "/releases") {
      this.release = { ...release(), ...body };
      return clone(this.release);
    }
    if (method === "PATCH" && route === "/releases/7") {
      Object.assign(this.release, body);
      return clone(this.release);
    }
    if (method === "DELETE" && route.startsWith("/releases/assets/")) {
      this.release.assets = this.release.assets.filter((value) => value.id !== Number(route.split("/").at(-1)));
      return null;
    }
    throw new Error(`Unexpected request: ${method} ${route}`);
  }

  async upload(id, name, data) {
    this.calls.push({ method: "UPLOAD", name });
    this.before("UPLOAD", name);
    assert.equal(id, 7);
    assert.equal(this.release.draft, true, "uploads must happen while unpublished");
    const uploaded = asset(name, 20 + this.release.assets.length, data.length);
    this.release.assets.push(uploaded);
    this.afterUpload(uploaded);
    return clone(uploaded);
  }
}

function publish(api) {
  return publishRelease({ api, metadata, artifacts });
}

function publicationCalls(api) {
  return api.calls.filter((call) => call.method === "PATCH" && call.body.draft === false);
}

test("publishes only after both uploaded attachments and the source tag are checked", async () => {
  const api = new FakeGitHub();
  const result = await publish(api);
  assert.equal(result.alreadyPublished, false);
  assert.equal(result.release.draft, false);
  assert.deepEqual(api.release.assets.map((value) => value.name), names);
  const visible = api.calls.findIndex((call) => call.method === "PATCH" && call.body.draft === false);
  assert.equal(api.calls.slice(0, visible).filter((call) => call.method === "UPLOAD").length, 2);
  assert.equal(api.calls.slice(0, visible).filter((call) => call.route?.startsWith("/git/ref/tags/")).length, 2);
  assert.equal(publicationCalls(api)[0].body.make_latest, "legacy");
});

test("a failed draft upload can be retried without moving a tag or creating another release", async () => {
  const api = new FakeGitHub();
  api.before = (method, route) => {
    if (method === "UPLOAD" && route === names[1]) throw new Error("upload failed");
  };
  await assert.rejects(publish(api), /upload failed/);
  assert.equal(api.release.draft, true);
  assert.equal(publicationCalls(api).length, 0);
  assert.equal(api.release.assets.length, 1);
  api.before = () => {};
  await publish(api);
  assert.equal(api.calls.filter((call) => call.method === "POST" && call.route === "/releases").length, 1);
  assert.equal(api.calls.filter((call) => call.method === "DELETE").length, 1);
  assert.equal(publicationCalls(api).length, 1);
});

test("a matching public release is not overwritten or compared with a new build's sizes", async () => {
  const api = new FakeGitHub(release(false, [asset(names[0], 1, 100), asset(names[1], 2, 200)]));
  assert.equal((await publish(api)).alreadyPublished, true);
  assert.ok(api.calls.every((call) => call.method === "GET"));
});

test("a public release with a missing attachment is refused without mutation", async () => {
  const api = new FakeGitHub(release(false, [asset(names[0])]));
  await assert.rejects(publish(api), /both expected attachments/);
  assert.ok(api.calls.every((call) => call.method === "GET"));
});

test("unknown attachments are refused for both public releases and recoverable drafts", async () => {
  for (const draft of [false, true]) {
    const api = new FakeGitHub(release(draft, [asset("unexpected.zip")]));
    await assert.rejects(publish(api), /Unexpected release attachment/);
    assert.ok(api.calls.every((call) => call.method === "GET"));
  }
});

test("duplicate or incomplete public attachments cannot count as a completed publication", async () => {
  for (const assets of [
    [asset(names[0], 1), asset(names[0], 2)],
    [asset(names[0], 1), asset(names[1], 2, 0)],
    [asset(names[0], 1), { ...asset(names[1], 2), state: "starter" }],
  ]) {
    const api = new FakeGitHub(release(false, assets));
    await assert.rejects(publish(api), /Duplicate|Incomplete/);
    assert.equal(publicationCalls(api).length, 0);
  }
});

test("an existing draft or public release for another source is never edited", async () => {
  for (const draft of [true, false]) {
    const api = new FakeGitHub({ ...release(draft), target_commitish: "b".repeat(40) });
    await assert.rejects(publish(api), /another source/);
    assert.ok(api.calls.every((call) => call.method === "GET"));
  }
});

test("nested annotated tags resolve to the selected commit", async () => {
  const api = new FakeGitHub();
  api.tagObject = { type: "tag", sha: "b".repeat(40) };
  api.tagObjects["b".repeat(40)] = { type: "tag", sha: "c".repeat(40) };
  api.tagObjects["c".repeat(40)] = { type: "commit", sha: commit };
  assert.equal(await resolveTag(api, metadata.tag), commit);
  assert.equal((await publish(api)).release.draft, false);
});

test("a different tag commit is refused before creating a draft", async () => {
  const api = new FakeGitHub();
  api.tagObject.sha = "b".repeat(40);
  await assert.rejects(publish(api), /validated source/);
  assert.equal(api.release, undefined);
});

test("a tag moved during upload leaves the release unpublished", async () => {
  const api = new FakeGitHub();
  api.afterUpload = () => { api.tagObject.sha = "b".repeat(40); };
  await assert.rejects(publish(api), /changed during preparation/);
  assert.equal(api.release.draft, true);
  assert.equal(publicationCalls(api).length, 0);
});

test("GitHub upload size mismatch or failure never publishes", async () => {
  const api = new FakeGitHub();
  api.afterUpload = (uploaded) => { uploaded.size = 0; };
  await assert.rejects(publish(api), /did not finish uploading/);
  assert.equal(publicationCalls(api).length, 0);
});

test("an older version does not displace a newer published version", async () => {
  const api = new FakeGitHub();
  api.afterUpload = () => {
    api.otherReleases = [{ id: 8, draft: false, prerelease: false, tag_name: "v0.10.0" }];
  };
  await publish(api);
  assert.equal(publicationCalls(api)[0].body.make_latest, "false");
});

test("release discovery includes a draft beyond the first listing page", async () => {
  const api = new FakeGitHub(release());
  api.otherReleases = Array.from({ length: 100 }, (_, i) => ({ id: i + 100, draft: false, prerelease: false, tag_name: `v0.1.${i}` }));
  await publish(api);
  assert.ok(api.calls.some((call) => call.route === "/releases?per_page=100&page=2"));
  assert.ok(!api.calls.some((call) => call.method === "POST" && call.route === "/releases"));
});

test("GitHub API failures are fatal and do not expose authentication values", async () => {
  const api = new GitHub("owner/repo", "private-token", async () => new Response(JSON.stringify({ message: "permission denied" }), { status: 403 }));
  await assert.rejects(api.request("GET", "/releases"), (error) => {
    assert.match(error.message, /HTTP 403.*permission denied/);
    assert.ok(!error.message.includes("private-token"));
    return true;
  });
});

test("an API error before publication leaves a complete draft for a retry", async () => {
  const api = new FakeGitHub();
  api.before = (method, route, body) => {
    if (method === "PATCH" && body?.draft === false) throw new Error("publish request failed");
  };
  await assert.rejects(publish(api), /publish request failed/);
  assert.equal(api.release.draft, true);
  assert.equal(api.release.assets.length, 2);
  api.before = () => {};
  assert.equal((await publish(api)).release.draft, false);
});

test("malformed versions, source identities, notes and archive paths are refused", () => {
  for (const change of [
    { version: "0.2.0-rc.1" },
    { version: "00.2.0" },
    { tag: "v0.1.0" },
    { commit: "master" },
    { notes: " " },
    { framework_asset: "../framework.zip" },
  ]) assert.throws(() => validateMetadata({ ...metadata, ...change }));
});

test("only the two declared ZIP archives are loaded and manual dispatch cannot publish", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "harmony-release-test-"));
  try {
    await writeFile(path.join(directory, "release.json"), JSON.stringify(metadata));
    for (const artifact of artifacts) await writeFile(path.join(directory, artifact.name), artifact.data);
    assert.deepEqual((await readArtifacts(metadata, directory)).map((value) => value.name), names);
    const result = spawnSync(process.execPath, [path.resolve(".github/scripts/release.mjs"), directory], {
      encoding: "utf8",
      env: { ...process.env, GITHUB_EVENT_NAME: "workflow_dispatch", GITHUB_REF: `refs/tags/${metadata.tag}`, GH_TOKEN: "" },
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Only a pushed version tag/);
    await writeFile(path.join(directory, names[0]), "not a zip");
    await assert.rejects(readArtifacts(metadata, directory), /Invalid ZIP archive/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
