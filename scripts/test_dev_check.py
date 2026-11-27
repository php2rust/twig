"""Selection unit tests: python scripts/test_dev_check.py."""
import json
import subprocess
import sys
import unittest
from unittest.mock import call, patch
from pathlib import Path
from tempfile import TemporaryDirectory

import dev_check
from dev_check import affected

METADATA = {
    "workspace_members": ["a", "b"],
    "packages": [
        {"id": "b", "name": "b", "targets": [{"src_path": "/w/b/src/lib.rs"}]},
        {"id": "a", "name": "a", "targets": [{"src_path": "/w/a/src/lib.rs"}]},
    ],
}


class SelectionTests(unittest.TestCase):
    def setUp(self):
        self.directory = TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()

    def package(self, name, dependencies=()):
        return {
            "id": name,
            "name": name,
            "manifest_path": str(self.root / "crates" / name / "Cargo.toml"),
            "dependencies": [
                {"path": str(self.root / "crates" / dep)} for dep in dependencies
            ],
        }

    def metadata(self):
        packages = [
            self.package("primitive"),
            self.package("middle", ["primitive"]),
            self.package("app", ["middle"]),
            self.package("independent"),
        ]
        return {"workspace_members": [p["id"] for p in packages], "packages": packages}

    def test_reverse_transitive_closure(self):
        self.assertEqual(
            affected(self.metadata(), {"crates/primitive/src/lib.rs"}, self.root)[0],
            ["app", "middle", "primitive"],
        )

    def test_independent(self):
        self.assertEqual(
            affected(self.metadata(), {"crates/independent/tests/test.rs"}, self.root)[0],
            ["independent"],
        )

    def test_global_fallback(self):
        for path in (
            "Cargo.toml", "Cargo.lock", "scripts/record.sh", "parity/done.txt",
            "crates/deleted/src/lib.rs", "crates/app/Cargo.toml",
            "crates/app/build.rs", "crates/app/tests/data.json",
        ):
            with self.subTest(path=path):
                self.assertEqual(affected(self.metadata(), {path}, self.root)[0], [])

    def test_no_changes_is_full(self):
        self.assertEqual(affected(self.metadata(), set(), self.root)[0], [])

    def test_external_path_dependency_is_full(self):
        metadata = self.metadata()
        metadata["packages"][0]["dependencies"].append(
            {"path": str(self.root / "outside")}
        )
        self.assertEqual(
            affected(metadata, {"crates/app/src/lib.rs"}, self.root)[0], []
        )


    def test_cycles_terminate(self):
        metadata = self.metadata()
        metadata["packages"][0]["dependencies"].append(
            {"path": str(self.root / "crates/app")}
        )
        self.assertEqual(
            affected(metadata, {"crates/app/src/lib.rs"}, self.root)[0],
            ["app", "middle", "primitive"],
        )

    def test_multiple_changes_include_both_closures(self):
        self.assertEqual(
            affected(self.metadata(), {
                "crates/middle/src/lib.rs", "crates/independent/src/lib.rs",
            }, self.root)[0],
            ["app", "independent", "middle"],
        )

    def test_all_path_dependency_kinds_are_included(self):
        for flags in (
            {"optional": True}, {"kind": "dev"}, {"kind": "build"},
            {"target": 'cfg(unix)', "rename": "alias"},
        ):
            metadata = self.metadata()
            metadata["packages"][1]["dependencies"][0].update(flags)
            with self.subTest(flags=flags):
                self.assertEqual(
                    affected(metadata, {"crates/primitive/src/lib.rs"}, self.root)[0],
                    ["app", "middle", "primitive"],
                )


class RunnerTests(unittest.TestCase):
    def test_failures_do_not_skip_later_checks(self):
        commands = [("missing-tool",), ("cargo", "test"), ("bash", "parity")]
        with patch.object(dev_check, "run", side_effect=[
            OSError("not installed"), subprocess.CalledProcessError(1, commands[1]), None,
        ]) as run:
            self.assertEqual(dev_check.run_checks(commands), 1)
        self.assertEqual(run.call_args_list, [call(*command) for command in commands])

    def test_success_returns_zero(self):
        with patch.object(dev_check, "run"):
            self.assertEqual(dev_check.run_checks([("cargo", "check")]), 0)

    def test_nextest_failure_still_runs_doctests(self):
        with patch.object(sys, "argv", ["dev_check.py", "test", "-p", "php-value", "--nextest"]):
            with patch.object(dev_check, "run", side_effect=[
                subprocess.CalledProcessError(100, "nextest"), None,
            ]) as run:
                self.assertEqual(dev_check.main(), 1)
        self.assertEqual(run.call_args_list, [
            call("cargo", "nextest", "run", "--locked", "-p", "php-value", "--no-fail-fast"),
            call("cargo", "test", "--locked", "-p", "php-value", "--doc"),
        ])

    def test_full_includes_tool_tests_and_all_gates(self):
        with patch.object(sys, "argv", ["dev_check.py", "full", "--bash", "/chosen/bash"]):
            with patch.object(dev_check, "capture", return_value=json.dumps(METADATA).encode()), \
                    patch.object(dev_check, "run") as run:
                self.assertEqual(dev_check.main(), 0)
        commands = [args.args for args in run.call_args_list]
        self.assertEqual(commands, [
            (sys.executable, "-m", "unittest", "discover", "-s", "scripts", "-p", "test_*.py"),
            ("cargo", "fmt", "--check", "-p", "a", "-p", "b"),
            ("cargo", "clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"),
            ("cargo", "test", "--locked", "--workspace", "--no-fail-fast"),
            ("/chosen/bash", "scripts/sources.sh", "--check"),
            ("/chosen/bash", "scripts/parity-check.sh"),
            (sys.executable, "scripts/classes.py", "check"),
        ])

    def test_fmt_is_batched_under_the_windows_command_line_limit(self):
        long = "C:/" + "x" * 9_000 + ".rs"
        metadata = {
            "workspace_members": ["a", "b", "c"],
            "packages": [
                {"id": "c", "name": "c", "targets": [{"src_path": long}]},
                {"id": "a", "name": "a", "targets": [{"src_path": long}]},
                {"id": "b", "name": "b", "targets": [{"src_path": long}]},
                {"id": "x", "name": "outside", "targets": [{"src_path": long}]},
            ],
        }
        self.assertEqual(dev_check.fmt_commands(metadata), [
            ("cargo", "fmt", "--check", "-p", "a"),
            ("cargo", "fmt", "--check", "-p", "b"),
            ("cargo", "fmt", "--check", "-p", "c"),
        ])

    def test_parity_does_not_start_cargo(self):
        with patch.object(sys, "argv", ["dev_check.py", "parity", "--bash", "chosen-bash"]):
            with patch.object(dev_check, "run") as run:
                self.assertEqual(dev_check.main(), 0)
        self.assertEqual(run.call_args_list, [
            call("chosen-bash", "scripts/sources.sh", "--check"),
            call("chosen-bash", "scripts/parity-check.sh"),
            call(sys.executable, "scripts/classes.py", "check"),
        ])

    def test_explicit_bash_is_preserved(self):
        self.assertEqual(dev_check.bash_executable("C:/path with spaces/bash.exe"),
                         "C:/path with spaces/bash.exe")

    def test_git_bash_precedes_windows_wsl_launcher(self):
        with TemporaryDirectory() as directory:
            install = Path(directory)
            (install / "bin").mkdir()
            bash = install / "bin/bash.exe"
            bash.touch()
            with patch.object(sys, "platform", "win32"):
                with patch.object(dev_check.shutil, "which",
                                  return_value=str(install / "cmd/git.exe")):
                    self.assertEqual(dev_check.bash_executable(None), str(bash))

    def test_metadata_failure_selects_workspace(self):
        with patch.object(dev_check, "capture", side_effect=ValueError("bad metadata")):
            packages, reason = dev_check.selection("missing")
        self.assertEqual(packages, [])
        self.assertIn("checking full workspace", reason)


class GitChangesTests(unittest.TestCase):
    def test_committed_index_worktree_untracked_and_deletion(self):
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            def git(*args):
                return subprocess.check_output(
                    ["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", *args],
                    cwd=root, stderr=subprocess.PIPE,
                )
            git("init", "--quiet")
            (root / "deleted.rs").write_text("initial")
            (root / "staged.rs").write_text("initial")
            (root / "unstaged.rs").write_text("initial")
            (root / ".gitignore").write_text("ignored.rs\n")
            git("add", ".")
            git("commit", "--quiet", "-m", "base")
            git("branch", "base")
            (root / "committed.rs").write_text("committed")
            git("add", "committed.rs")
            git("commit", "--quiet", "-m", "change")
            (root / "deleted.rs").unlink()
            (root / "staged.rs").write_text("staged")
            git("add", "staged.rs")
            (root / "unstaged.rs").write_text("unstaged")
            (root / "untracked space.rs").write_text("untracked")
            (root / "ignored.rs").write_text("ignored")
            with patch.object(dev_check, "ROOT", root):
                self.assertEqual(dev_check.changed_paths("base"), {
                    "committed.rs", "deleted.rs", "staged.rs", "unstaged.rs",
                    "untracked space.rs",
                })


if __name__ == "__main__":
    unittest.main()
