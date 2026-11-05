#!/usr/bin/env python3
"""Offline tests of fetch-sdk-tests.py: python3 -I scripts/fetch-sdk-tests.test.py

A local git repository stands in for the upstream commit; no test contacts a
network host. The checked-in pin is validated as data.
"""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().with_name("fetch-sdk-tests.py")
_spec = importlib.util.spec_from_file_location("fetch_sdk_tests", SCRIPT)
assert _spec is not None and _spec.loader is not None  # the script sits beside this file
fetcher = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fetcher)

PACKAGE_DIRECTORY = "packages/typescript"
TEST_PATH = f"{PACKAGE_DIRECTORY}/test/a.test.ts"
FIXTURE_PATH = "contract/fixtures/data.txt"
TEST_BYTES = b"import { test } from 'node:test';\n"
FIXTURE_BYTES = b"data: x\n\n"
VERSION = "1.2.3"
MANIFEST_BYTES = json.dumps({"name": "demo", "version": VERSION}).encode()
SOURCE_BYTES = b"export const a = 1;\n"
# Identity for the throwaway commits; the caller's git configuration is ignored.
GIT_IDENTITY = {
    "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@invalid",
    "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@invalid",
    "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write(root: Path, relative: str, data: bytes) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def git(work: Path, *args: str) -> str:
    environment = {**os.environ, **GIT_IDENTITY}
    result = subprocess.run(["git", "-C", str(work), *args], env=environment,
                            capture_output=True, check=True)
    return result.stdout.decode().strip()


def make_repository(root: Path) -> tuple[Path, str]:
    """A repository with a test, a fixture and a package; returns (git dir, commit)."""
    work = root / "upstream"
    work.mkdir()
    git(work, "init", "--quiet")
    write(work, TEST_PATH, TEST_BYTES)
    write(work, FIXTURE_PATH, FIXTURE_BYTES)
    write(work, f"{PACKAGE_DIRECTORY}/package.json", MANIFEST_BYTES)
    write(work, f"{PACKAGE_DIRECTORY}/src/a.ts", SOURCE_BYTES)
    os.symlink("a.test.ts", work / PACKAGE_DIRECTORY / "test" / "link.test.ts")
    git(work, "add", "-A")
    git(work, "commit", "--quiet", "-m", "fixture")
    return work / ".git", git(work, "rev-parse", "HEAD")


def make_pin(commit: str) -> dict:
    return {
        "schema": fetcher.PIN_SCHEMA,
        "package": "demo",
        "version": VERSION,
        "repository": fetcher.ALLOWED_REPOSITORY,
        "tag": "v1.2.3",
        "commit": commit,
        "packageDirectory": PACKAGE_DIRECTORY,
        "testScript": "node --test test/*.test.ts",
        "npmPackage": {"integrity": "sha512-x", "comparedFiles": 2},
        "files": [
            {"path": FIXTURE_PATH, "sha256": sha256(FIXTURE_BYTES)},
            {"path": TEST_PATH, "sha256": sha256(TEST_BYTES)},
        ],
    }


def make_npm(root: Path) -> Path:
    npm = root / "node_modules" / "demo"
    write(npm, "package.json", MANIFEST_BYTES)
    write(npm, "src/a.ts", SOURCE_BYTES)
    write(npm, "dist/a.js", b"export const a = 1;\n")
    return npm


class Base(unittest.TestCase):
    def setUp(self) -> None:
        self._scratch = tempfile.TemporaryDirectory()
        self.root = Path(self._scratch.name)

    def tearDown(self) -> None:
        self._scratch.cleanup()

    def write_pin(self, pin: dict) -> Path:
        path = self.root / "pin.json"
        path.write_text(json.dumps(pin), encoding="utf-8")
        return path


class PinTests(Base):
    def test_checked_in_pin_is_valid(self) -> None:
        pin = fetcher.load_pin(fetcher.PIN_PATH)
        self.assertEqual(pin["repository"], fetcher.ALLOWED_REPOSITORY)
        self.assertTrue(all(entry["path"].count("/") >= 1 for entry in pin["files"]))

    def test_valid_pin_loads(self) -> None:
        self.assertEqual(fetcher.load_pin(self.write_pin(make_pin("a" * 40)))["commit"], "a" * 40)

    def test_refusals(self) -> None:
        cases = {
            "repository": lambda pin: pin.update(repository="https://example.invalid/uarp-sdks.git"),
            "commit": lambda pin: pin.update(commit="v0.13.2"),
            "schema": lambda pin: pin.update(schema=fetcher.PIN_SCHEMA + 1),
            "traversal": lambda pin: pin["files"][0].update(path="contract/../../escape"),
            "absolute": lambda pin: pin["files"][0].update(path="/etc/hosts"),
            "duplicate": lambda pin: pin["files"].append(dict(pin["files"][0])),
            "sha256": lambda pin: pin["files"][0].update(sha256="ABC"),
            "empty": lambda pin: pin.update(files=[]),
            "npm": lambda pin: pin.update(npmPackage={}),
        }
        for name, mutate in cases.items():
            with self.subTest(name):
                pin = make_pin("a" * 40)
                mutate(pin)
                with self.assertRaises(fetcher.PinError):
                    fetcher.load_pin(self.write_pin(pin))


class TreeTests(Base):
    def setUp(self) -> None:
        super().setUp()
        self.pin = make_pin("a" * 40)
        self.tree = self.root / "tree"
        write(self.tree, TEST_PATH, TEST_BYTES)
        write(self.tree, FIXTURE_PATH, FIXTURE_BYTES)

    def assert_problem(self, fragment: str) -> None:
        with self.assertRaises(fetcher.VerificationError) as raised:
            fetcher.verify_tree(self.tree, self.pin)
        self.assertTrue(any(fragment in problem for problem in raised.exception.problems),
                        raised.exception.problems)

    def test_matching_tree_verifies(self) -> None:
        fetcher.verify_tree(self.tree, self.pin)

    def test_changed_byte_fails(self) -> None:
        write(self.tree, TEST_PATH, TEST_BYTES + b"\n")
        self.assert_problem(f"{TEST_PATH}: sha256")

    def test_missing_file_fails(self) -> None:
        (self.tree / FIXTURE_PATH).unlink()
        self.assert_problem(f"{FIXTURE_PATH}: missing")

    def test_extra_file_fails(self) -> None:
        write(self.tree, "packages/typescript/test/extra.test.ts", b"")
        self.assert_problem("extra.test.ts: not in the pin")

    def test_symlinked_file_fails(self) -> None:
        (self.tree / TEST_PATH).unlink()
        os.symlink(self.tree / FIXTURE_PATH, self.tree / TEST_PATH)
        self.assert_problem(f"{TEST_PATH}: not a regular file")

    def test_symlinked_directory_fails(self) -> None:
        elsewhere = self.root / "elsewhere"
        write(elsewhere, "data.txt", FIXTURE_BYTES)
        (self.tree / FIXTURE_PATH).unlink()
        (self.tree / "contract" / "fixtures").rmdir()
        os.symlink(elsewhere, self.tree / "contract" / "fixtures")
        self.assert_problem("contract/fixtures: symlinked directory")

    def test_absent_tree_fails(self) -> None:
        with self.assertRaises(fetcher.VerificationError):
            fetcher.verify_tree(self.root / "absent", self.pin)


class FetchTests(Base):
    def setUp(self) -> None:
        super().setUp()
        self.git_dir, self.commit = make_repository(self.root)
        self.pin = make_pin(self.commit)
        self.target = self.root / "pins" / "tests"
        write(self.target, "previous.txt", b"kept until a verified copy replaces it")

    def stage_and_install(self) -> None:
        staging = Path(tempfile.mkdtemp(prefix=fetcher.STAGING_PREFIX, dir=self.target.parent))
        fetcher.stage_files(self.git_dir, self.pin, staging)
        fetcher.install(staging, self.target)

    def test_stage_and_install_replace_the_target(self) -> None:
        self.stage_and_install()
        fetcher.verify_tree(self.target, self.pin)
        self.assertEqual(sorted(p.name for p in self.target.parent.iterdir()), ["tests"])

    def test_hash_mismatch_at_the_commit_fails_closed(self) -> None:
        self.pin["files"][1]["sha256"] = sha256(b"something else")
        with self.assertRaises(fetcher.VerificationError):
            self.stage_and_install()
        self.assertTrue((self.target / "previous.txt").is_file())

    def test_path_absent_at_the_commit_fails(self) -> None:
        self.pin["files"].append({"path": "contract/fixtures/absent.txt", "sha256": "0" * 64})
        with self.assertRaises(fetcher.FetchError):
            self.stage_and_install()
        self.assertTrue((self.target / "previous.txt").is_file())

    def test_symlink_at_the_commit_is_refused(self) -> None:
        link = f"{PACKAGE_DIRECTORY}/test/link.test.ts"
        self.pin["files"].append({"path": link, "sha256": sha256(b"a.test.ts")})
        with self.assertRaisesRegex(fetcher.FetchError, "not a regular file"):
            self.stage_and_install()

    def test_npm_package_matches_the_commit(self) -> None:
        self.assertEqual(fetcher.verify_npm_match(self.git_dir, self.pin, make_npm(self.root)), 2)

    def test_npm_package_that_differs_fails(self) -> None:
        npm = make_npm(self.root)
        write(npm, "src/a.ts", b"export const a = 2;\n")
        with self.assertRaises(fetcher.VerificationError) as raised:
            fetcher.verify_npm_match(self.git_dir, self.pin, npm)
        self.assertIn("npm src/a.ts: differs from the commit", raised.exception.problems)

    def test_npm_file_absent_upstream_fails(self) -> None:
        npm = make_npm(self.root)
        write(npm, "src/extra.ts", b"")
        pin = copy.deepcopy(self.pin)
        pin["npmPackage"]["comparedFiles"] = 3
        with self.assertRaises(fetcher.VerificationError) as raised:
            fetcher.verify_npm_match(self.git_dir, pin, npm)
        self.assertIn("npm src/extra.ts: absent at the commit", raised.exception.problems)

    def test_npm_version_mismatch_fails(self) -> None:
        pin = copy.deepcopy(self.pin)
        pin["version"] = "9.9.9"
        with self.assertRaises(fetcher.VerificationError):
            fetcher.verify_npm_match(self.git_dir, pin, make_npm(self.root))

    def test_non_https_remote_is_refused(self) -> None:
        git_dir = self.root / "fetched.git"
        with self.assertRaisesRegex(fetcher.FetchError, "transport 'file' not allowed"):
            fetcher.fetch_commit(git_dir, f"file://{self.git_dir}", self.commit)

    def test_fetch_requires_the_npm_package(self) -> None:
        with self.assertRaisesRegex(fetcher.FetchError, "npm ci"):
            fetcher.fetch(self.pin, self.target, self.root / "node_modules" / "absent")


if __name__ == "__main__":
    unittest.main()
