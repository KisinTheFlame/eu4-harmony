"""Detect the version increase that requests publication after a PR merges."""
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib


def workspace_version(text):
    value = tomllib.loads(text)["workspace"]["package"]["version"]
    if not isinstance(value, str) or not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value):
        raise ValueError("The workspace version must be a canonical X.Y.Z version")
    return value


def version_increased(before_text, after_text):
    before = tuple(map(int, workspace_version(before_text).split(".")))
    after = tuple(map(int, workspace_version(after_text).split(".")))
    if after < before:
        raise ValueError("A version PR must not lower the workspace version")
    return after > before


def release_request(event_name, ref, event, commit, current_text, read_before):
    if ref != "refs/heads/master":
        raise ValueError("Release preparation and publication must select master")
    version = workspace_version(current_text)
    if event_name == "workflow_dispatch":
        return False, version
    if event_name != "push":
        raise ValueError("Only a version increase merged into master requests publication")
    before = event.get("before", "")
    if not re.fullmatch(r"[a-f0-9]{40}", before) or before == "0" * 40:
        raise ValueError("A version PR requires existing master history")
    if event.get("after") != commit or not re.fullmatch(r"[a-f0-9]{40}", commit):
        raise ValueError("The push event does not select the checked-out source")
    return version_increased(read_before(before), current_text), version


def main():
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
    current = Path("Cargo.toml").read_text(encoding="utf-8")
    should_publish, version = release_request(
        os.environ["GITHUB_EVENT_NAME"], os.environ["GITHUB_REF"], event,
        os.environ["GITHUB_SHA"], current,
        lambda before: subprocess.check_output(["git", "show", f"{before}:Cargo.toml"], text=True, encoding="utf-8"),
    )
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(f"publish={str(should_publish).lower()}\nversion={version}\n")
    print(f"Version {version}: {'publication requested by version increase' if should_publish else 'preparation only; no publication requested'}")


if __name__ == "__main__":
    main()
