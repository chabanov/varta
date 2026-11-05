#!/usr/bin/env python3
"""Fetches and verifies the pinned own test sources of uarp-sdk.

The npm tarball of uarp-sdk ships `dist`, `src`, `README.md` and `LICENSE`, not
the package's own test suite. `pins/uarp-sdk-tests.json` names the upstream
commit and the sha256 of every test source (and of the fixtures the tests read)
so that suite can be fetched outside version control and checked byte for byte.

    python3 scripts/fetch-sdk-tests.py           # fetch at the pinned commit, verify, install
    python3 scripts/fetch-sdk-tests.py --check   # offline: verify the installed copy

Fetching contacts only the allowlisted repository below, over HTTPS, without
redirects, credentials or the caller's git configuration. The commit is fetched
by its hash and every pinned file must match its sha256 before the ignored
directory `pins/uarp-sdk-tests/` is replaced; any mismatch, missing file or
extra file fails closed and leaves the previous copy untouched. Fetching also
checks that the pinned commit is the source of the installed npm package: every
file of `node_modules/uarp-sdk` except the build output `dist/` must equal the
same path under the commit's package directory.

Exit status: 0 verified, 1 verification failed, 2 the pin or the environment
is unusable.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tempfile

EXAMPLE_DIR = Path(__file__).resolve().parents[1]
PIN_PATH = EXAMPLE_DIR / "pins" / "uarp-sdk-tests.json"
TARGET_DIR = EXAMPLE_DIR / "pins" / "uarp-sdk-tests"
# Siblings of TARGET_DIR used while installing; ignored by git like the target.
STAGING_PREFIX = ".uarp-sdk-tests.staging-"
NPM_PACKAGE_DIR = EXAMPLE_DIR / "node_modules" / "uarp-sdk"
# Build output of the npm package; the repository does not contain it.
NPM_BUILD_DIR = "dist"
NPM_MANIFEST = "package.json"

PIN_SCHEMA = 1
# The only remote this script ever contacts.
ALLOWED_REPOSITORY = "https://github.com/Snaga-AI/uarp-sdks.git"
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
# Git tree modes of regular files; symlinks and submodules are refused.
REGULAR_BLOB_MODES = ("100644", "100755")

EXIT_OK = 0
EXIT_MISMATCH = 1
EXIT_UNUSABLE = 2

# Isolate git from the caller's system and global configuration (no
# `url.*.insteadOf` rewrites or credential helpers), never prompt, allow HTTPS
# only and never follow a redirect.
GIT_ENV = {
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_TERMINAL_PROMPT": "0",
    "GIT_ASKPASS": "",
    "GIT_ALLOW_PROTOCOL": "https",
}
GIT_OPTIONS = (
    "-c", "protocol.allow=never",
    "-c", "protocol.https.allow=always",
    "-c", "http.followRedirects=false",
    "-c", "credential.helper=",
    "-c", "core.hooksPath=" + os.devnull,
)


class PinError(Exception):
    """The pin file is malformed or names something this script refuses."""


class VerificationError(Exception):
    """Fetched or installed content differs from the pin."""

    def __init__(self, problems: list[str]) -> None:
        super().__init__("; ".join(problems))
        self.problems = problems


class FetchError(Exception):
    """git could not fetch or read the pinned commit."""


def _relative_path(value: object, field: str) -> str:
    if not isinstance(value, str) or not value:
        raise PinError(f"{field}: expected a non-empty path")
    if "\\" in value or "\0" in value or value.startswith("/"):
        raise PinError(f"{field}: {value!r} is not a relative POSIX path")
    parts = value.split("/")
    if any(part in ("", ".", "..") for part in parts):
        raise PinError(f"{field}: {value!r} has an empty, '.' or '..' segment")
    return value


def load_pin(path: Path) -> dict:
    """Reads and validates the pin; raises PinError."""
    try:
        pin = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PinError(f"{path}: {error}") from error
    if not isinstance(pin, dict):
        raise PinError("the pin is not a JSON object")
    if pin.get("schema") != PIN_SCHEMA:
        raise PinError(f"schema: expected {PIN_SCHEMA}")
    if pin.get("repository") != ALLOWED_REPOSITORY:
        raise PinError(f"repository: only {ALLOWED_REPOSITORY} is allowed")
    commit = pin.get("commit")
    if not isinstance(commit, str) or not COMMIT_PATTERN.match(commit):
        raise PinError("commit: expected 40 lowercase hex digits")
    for field in ("package", "version", "testScript"):
        if not isinstance(pin.get(field), str) or not pin[field]:
            raise PinError(f"{field}: expected a non-empty string")
    _relative_path(pin.get("packageDirectory"), "packageDirectory")
    files = pin.get("files")
    if not isinstance(files, list) or not files:
        raise PinError("files: expected a non-empty list")
    seen: set[str] = set()
    for index, entry in enumerate(files):
        if not isinstance(entry, dict):
            raise PinError(f"files[{index}]: expected an object")
        file_path = _relative_path(entry.get("path"), f"files[{index}].path")
        if file_path in seen:
            raise PinError(f"files[{index}].path: {file_path} is listed twice")
        seen.add(file_path)
        digest = entry.get("sha256")
        if not isinstance(digest, str) or not SHA256_PATTERN.match(digest):
            raise PinError(f"files[{index}].sha256: expected 64 lowercase hex digits")
    npm = pin.get("npmPackage")
    if not isinstance(npm, dict) or not isinstance(npm.get("comparedFiles"), int):
        raise PinError("npmPackage.comparedFiles: expected an integer")
    return pin


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _read_regular(path: Path) -> bytes:
    """Reads a regular file without following a symlink at the last component."""
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as handle:
        if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
            raise OSError(f"{path} is not a regular file")
        return handle.read()


def _walk_regular_files(root: Path, problems: list[str]) -> dict[str, Path]:
    """Every regular file under root by relative POSIX path; anything else is a problem."""
    found: dict[str, Path] = {}
    for directory, names, files in os.walk(root, followlinks=False):
        base = Path(directory)
        for name in list(names):
            if (base / name).is_symlink():
                problems.append(f"{(base / name).relative_to(root).as_posix()}: symlinked directory")
                names.remove(name)
        for name in files:
            entry = base / name
            relative = entry.relative_to(root).as_posix()
            if stat.S_ISREG(entry.lstat().st_mode):
                found[relative] = entry
            else:
                problems.append(f"{relative}: not a regular file")
    return found


def verify_tree(root: Path, pin: dict) -> None:
    """The tree holds exactly the pinned files with their hashes; raises VerificationError."""
    if not root.is_dir() or root.is_symlink():
        raise VerificationError([f"{root} is not a directory"])
    problems: list[str] = []
    found = _walk_regular_files(root, problems)
    expected = {entry["path"]: entry["sha256"] for entry in pin["files"]}
    for relative in sorted(expected):
        if relative not in found:
            problems.append(f"{relative}: missing")
            continue
        actual = _sha256(_read_regular(found[relative]))
        if actual != expected[relative]:
            problems.append(f"{relative}: sha256 {actual}, pinned {expected[relative]}")
    for relative in sorted(set(found) - set(expected)):
        problems.append(f"{relative}: not in the pin")
    if problems:
        raise VerificationError(problems)


def _git(git_dir: Path, *args: str) -> bytes:
    command = ["git", *GIT_OPTIONS, "--git-dir", str(git_dir), *args]
    environment = {**os.environ, **GIT_ENV}
    result = subprocess.run(command, env=environment, capture_output=True, check=False)
    if result.returncode != 0:
        message = result.stderr.decode("utf-8", "replace").strip()
        raise FetchError(f"git {' '.join(args)}: {message}")
    return result.stdout


def fetch_commit(git_dir: Path, repository: str, commit: str) -> None:
    """Fetches exactly one commit by hash into a fresh bare repository."""
    _git(git_dir, "init", "--quiet", "--bare")
    _git(git_dir, "fetch", "--quiet", "--depth=1", "--no-tags", repository, commit)
    fetched = _git(git_dir, "rev-parse", "--verify", "FETCH_HEAD^{commit}").decode().strip()
    if fetched != commit:
        raise FetchError(f"fetched {fetched}, pinned {commit}")


def _tree_entries(git_dir: Path, commit: str, path: str) -> dict[str, tuple[str, str]]:
    """Repository path -> (mode, object id) of path itself or every entry under it."""
    listing = _git(git_dir, "ls-tree", "-r", "-z", "--full-tree", commit, "--", path)
    entries: dict[str, tuple[str, str]] = {}
    for record in listing.split(b"\0"):
        if not record:
            continue
        meta, _, name = record.decode("utf-8").partition("\t")
        mode, _kind, object_id = meta.split(" ")
        entries[name] = (mode, object_id)
    return entries


def _regular_blob(git_dir: Path, path: str, entry: tuple[str, str] | None) -> bytes:
    if entry is None:
        raise FetchError(f"{path}: absent at the pinned commit")
    mode, object_id = entry
    if mode not in REGULAR_BLOB_MODES:
        raise FetchError(f"{path}: mode {mode} is not a regular file")
    return _git(git_dir, "cat-file", "blob", object_id)


def stage_files(git_dir: Path, pin: dict, staging: Path) -> None:
    """Writes every pinned file from the commit into staging and verifies it."""
    problems: list[str] = []
    for entry in pin["files"]:
        listed = _tree_entries(git_dir, pin["commit"], entry["path"])
        data = _regular_blob(git_dir, entry["path"], listed.get(entry["path"]))
        if _sha256(data) != entry["sha256"]:
            problems.append(f"{entry['path']}: sha256 {_sha256(data)} at the commit, pinned {entry['sha256']}")
            continue
        destination = staging.joinpath(*PurePosixPath(entry["path"]).parts)
        destination.parent.mkdir(parents=True, exist_ok=True)
        with open(destination, "xb") as handle:
            handle.write(data)
    if problems:
        raise VerificationError(problems)
    verify_tree(staging, pin)


def verify_npm_match(git_dir: Path, pin: dict, npm_dir: Path) -> int:
    """Every non-build file of the npm package equals the commit's package directory."""
    problems: list[str] = []
    found = _walk_regular_files(npm_dir, problems)
    shipped = {
        relative: path for relative, path in found.items()
        if relative.split("/", 1)[0] != NPM_BUILD_DIR
    }
    manifest = shipped.get(NPM_MANIFEST)
    if manifest is None:
        raise VerificationError([f"{npm_dir}: no {NPM_MANIFEST}"])
    try:
        parsed = json.loads(_read_regular(manifest).decode("utf-8"))
    except ValueError as error:
        raise VerificationError([f"npm {NPM_MANIFEST}: {error}"]) from error
    version = parsed.get("version") if isinstance(parsed, dict) else None
    if version != pin["version"]:
        problems.append(f"installed {pin['package']} is {version}, pinned {pin['version']}")
    directory = pin["packageDirectory"]
    blobs = _tree_entries(git_dir, pin["commit"], directory)
    for relative in sorted(shipped):
        repository_path = f"{directory}/{relative}"
        entry = blobs.get(repository_path)
        if entry is None:
            problems.append(f"npm {relative}: absent at the commit")
            continue
        data = _regular_blob(git_dir, repository_path, entry)
        if _sha256(data) != _sha256(_read_regular(shipped[relative])):
            problems.append(f"npm {relative}: differs from the commit")
    expected = pin["npmPackage"]["comparedFiles"]
    if len(shipped) != expected:
        problems.append(f"npm package has {len(shipped)} files outside {NPM_BUILD_DIR}/, pinned {expected}")
    if problems:
        raise VerificationError(problems)
    return len(shipped)


def install(staging: Path, target: Path) -> None:
    """Replaces target with the verified staging directory."""
    previous = None
    if target.exists() or target.is_symlink():
        previous = Path(tempfile.mkdtemp(prefix=STAGING_PREFIX, dir=target.parent))
        previous.rmdir()
        target.rename(previous)
    staging.rename(target)
    if previous is not None:
        if previous.is_symlink():
            previous.unlink()
        else:
            shutil.rmtree(previous)


def fetch(pin: dict, target: Path, npm_dir: Path) -> int:
    """Fetches, verifies and installs; returns the number of npm files compared."""
    if not npm_dir.is_dir():
        raise FetchError(f"missing {npm_dir}: run npm ci --ignore-scripts")
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="uarp-sdk-tests-git.") as scratch:
        git_dir = Path(scratch) / "repository.git"
        fetch_commit(git_dir, pin["repository"], pin["commit"])
        compared = verify_npm_match(git_dir, pin, npm_dir)
        staging = Path(tempfile.mkdtemp(prefix=STAGING_PREFIX, dir=target.parent))
        try:
            stage_files(git_dir, pin, staging)
            install(staging, target)
        finally:
            if staging.exists():
                shutil.rmtree(staging)
    return compared


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--check", action="store_true", help="verify the installed copy without network access")
    arguments = parser.parse_args(argv)
    try:
        pin = load_pin(PIN_PATH)
        if not arguments.check:
            compared = fetch(pin, TARGET_DIR, NPM_PACKAGE_DIR)
            print(f"npm {pin['package']} {pin['version']}: {compared} files equal {pin['commit']}")
        verify_tree(TARGET_DIR, pin)
    except VerificationError as error:
        for problem in error.problems:
            print(f"FAIL {problem}", file=sys.stderr)
        return EXIT_MISMATCH
    except (PinError, FetchError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return EXIT_UNUSABLE
    print(f"ok   {len(pin['files'])} files at {pin['commit']} in {TARGET_DIR.relative_to(EXAMPLE_DIR).as_posix()}")
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
