"""Compare the batch checkers with the original Bash rules on broken trees."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest

from dev_check import bash_executable


class ReferenceParityTests(unittest.TestCase):
    def setUp(self):
        self.temporary = TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        (self.root / "scripts").mkdir()
        source = Path(__file__).parent
        for name in ("sources.sh", "parity-check.sh", "parity_check.py"):
            shutil.copyfile(source / name, self.root / "scripts" / name)
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        self.files = {
            "parity/packages.tsv": "symfony/example\tv1\t123\tunused\n",
            "upstream.tsv": "# name\trepo\tref\n",
            "parity/done.txt": "symfony/example\n",
            "crates/example/src/lib.rs": "//! Upstream: symfony/example Example.php\n",
            "crates/example/tests/test.rs": (
                "//! Upstream: symfony/example Example.php Tests/ExampleTest.php\n"
                "#[test]\nfn testOne() {}\n"
                "#[tokio::test]\nasync fn testTwo() {}\n"
            ),
            "parity/symfony/example.tsv": (
                "# file\ttest\tstatus\trust\n"
                "Tests/ExampleTest.php\ttestOne\tported\tcrates/example/tests/test.rs::testOne\n"
                "Tests/ExampleTest.php\ttestTwo\tported\tcrates/example/tests/test.rs::testTwo\n"
            ),
        }
        for name, content in self.files.items():
            self.write(name, content)
        self.bash = bash_executable(None)

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8", newline="\n")

    def run_check(self, script, fast, *args):
        env = dict(os.environ)
        env.pop("SYMFONY_RS_PYTHON", None)
        if fast:
            env["SYMFONY_RS_PYTHON"] = sys.executable
        return subprocess.run(
            [self.bash, "scripts/" + script, *args], cwd=self.root, env=env,
            capture_output=True, text=True,
        )

    def compare_parity(self, expected):
        reference = self.run_check("parity-check.sh", False)
        fast = self.run_check("parity-check.sh", True)
        self.assertEqual(reference.returncode, expected, reference.stderr)
        self.assertEqual(fast.returncode, reference.returncode, fast.stderr)
        self.assertEqual(fast.stdout, reference.stdout)
        self.assertEqual(fast.stderr, reference.stderr)

    def compare_sources(self, expected):
        reference = self.run_check("sources.sh", False)
        previous = (self.root / "parity/sources.tsv").read_bytes()
        fast = self.run_check("sources.sh", True)
        self.assertEqual(reference.returncode, expected, reference.stderr)
        self.assertEqual(fast.returncode, reference.returncode, fast.stderr)
        self.assertEqual(fast.stderr, reference.stderr)
        self.assertEqual((self.root / "parity/sources.tsv").read_bytes(), previous)

    def test_valid(self):
        self.compare_parity(0)
        self.compare_sources(0)
        for fast in (False, True):
            checked = self.run_check("sources.sh", fast, "--check")
            self.assertEqual(checked.returncode, 0, checked.stderr)

    def test_broken_manifest_rules(self):
        name = "parity/symfony/example.tsv"
        original = self.files[name]
        for replacement in (
            original.replace("\tported\t", "\tunknown\t", 1),
            original.replace("testOne\tported", "\tported", 1),
            original.replace("Tests/ExampleTest.php\ttestOne", "\ttestOne", 1),
            original.replace("::testOne", "::missing", 1),
            original.replace("test.rs::testOne", "absent.rs::testOne", 1),
            original.replace("\tported\t", "\tpending\t", 1),
            original.replace("\tported\t", "\tn/a: reason\t", 1),
        ):
            with self.subTest(replacement=replacement):
                self.write(name, replacement)
                self.compare_parity(1)

    def test_n_a_with_reason_and_pending_are_valid_outside_done(self):
        self.write("parity/done.txt", "")
        self.write("parity/symfony/example.tsv",
                   "Tests/ExampleTest.php\ttestOne\tpending\n"
                   "Tests/ExampleTest.php\ttestTwo\tn/a: PHP reflection\n")
        self.write("crates/example/tests/test.rs", "//! Upstream: none\n")
        self.compare_parity(0)

    def test_unclaimed_context_and_generic_test(self):
        name = "crates/example/tests/test.rs"
        self.write(name, self.files[name] + "#[test]\n// comment\nfn testExtra<T>() {}\n")
        self.compare_parity(1)

    def test_single_and_multiple_test_paths_require_claims(self):
        name = "crates/example/tests/test.rs"
        body = self.files[name].split("\n", 1)[1]
        for paths in (
            "Tests/ExampleTest.php",
            "tests/ExampleTest.php",
            "Component/Tests/ExampleTest.php",
            "Tests/ExampleTest.php Example.php",
            "Example.php Tests/ExampleTest.php",
            "Example.php\tComponent/tests/ExampleTest.php",
        ):
            with self.subTest(paths=paths):
                self.write(name, "//! Upstream: symfony/example " + paths + "\n" + body
                           + "#[test]\nfn testUnclaimed() {}\n")
                self.compare_parity(1)

    def test_production_paths_and_other_lines_do_not_claim_tests(self):
        name = "crates/example/tests/test.rs"
        for mapping in (
            "//! Upstream: none",
            "//! Upstream: symfony/example NotTests/Example.php",
            "//! Upstream: symfony/example Example.php",
            "//! Upstream: symfony/example Example.php\n// Tests/ExampleTest.php",
        ):
            with self.subTest(mapping=mapping):
                self.write(name, mapping + "\n" + self.files[name].split("\n", 1)[1]
                           + "#[test]\nfn testLocalHelper() {}\n")
                self.compare_parity(0)

    def test_broken_source_rules(self):
        name = "crates/example/src/lib.rs"
        for content in (
            "// missing mapping\n",
            "//! Upstream: symfony/unknown Example.php\n",
            "//! Upstream: symfony/example\n",
        ):
            with self.subTest(content=content):
                self.write(name, content)
                self.compare_sources(1)

    def test_fetched_upstream_path_must_exist(self):
        self.write("upstream/symfony/example/Example.php", "specification")
        self.compare_sources(1)  # the test file is still missing
        self.write("upstream/symfony/example/Tests/ExampleTest.php", "tests")
        self.compare_sources(0)

    def test_stale_mapping_fails_both(self):
        self.compare_sources(0)
        self.write("parity/sources.tsv", "# stale\n")
        for fast in (False, True):
            result = self.run_check("sources.sh", fast, "--check")
            self.assertEqual(result.returncode, 1)
            self.assertIn("parity/sources.tsv is stale", result.stderr)


if __name__ == "__main__":
    unittest.main()

