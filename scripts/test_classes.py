"""Check scripts/classes.py on small trees (no PHP needed)."""
from contextlib import redirect_stderr, redirect_stdout
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import classes  # noqa: E402
from dev_check import bash_executable  # noqa: E402

HEADER = "# symfony/example at 123: " + classes.HEADER + "\n"


class ClassManifestTests(unittest.TestCase):
    def setUp(self):
        self.temporary = TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.write("parity/packages.tsv", "# package\tversion\tref\trepo\nsymfony/example\tv1\t123\tunused\n")
        self.write("parity/sources.tsv", (
            "# rust file\tpackage\tupstream path\treference\n"
            "crates/example/src/a.rs\tsymfony/example\tA.php\t123\n"
            "crates/example/src/b.rs\tsymfony/example\tA.php\t123\n"
            "crates/example/tests/a.rs\tsymfony/example\tTests/ATest.php\t123\n"
        ))
        self.write("parity/classes/symfony/example.tsv", HEADER + (
            "A.php\tA\t10\tported\tcrates/example/src/a.rs crates/example/src/b.rs\n"
            "B.php\tB AInterface\t30\tpending\t\n"
            "Resources/data/x.php\t-\t60\tpending\t\n"
            "C.php\t-\t5\tn/a: PHP-only helper\t\n"
        ))

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8", newline="\n")

    def run_mode(self, function, *args):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = function(self.root, *args)
        return code, out.getvalue(), err.getvalue()

    def test_valid(self):
        code, _, err = self.run_mode(classes.check)
        self.assertEqual(code, 0, err)

    def test_report(self):
        code, out, _ = self.run_mode(classes.report, False)
        self.assertEqual(code, 0)
        self.assertRegex(out, r"symfony/example\s+1/3\s+files\s+10/100\s+lines\s+10\.0%")
        self.assertRegex(out, r"code \(no Resources/data/\)\s+1/2\s+files\s+10/40\s+lines\s+25\.0%")

    def test_pending_but_mapped(self):
        self.write("parity/sources.tsv", (self.root / "parity/sources.tsv").read_text()
                   + "crates/example/src/b2.rs\tsymfony/example\tB.php\t123\n")
        code, _, err = self.run_mode(classes.check)
        self.assertEqual(code, 1)
        self.assertIn("B.php is pending but crates/example/src/b2.rs ports it", err)

    def test_sync(self):
        self.write("parity/sources.tsv", (
            "# header\n"
            "crates/example/src/b2.rs\tsymfony/example\tB.php\t123\n"
        ))
        code, out, err = self.run_mode(classes.sync)
        self.assertEqual(code, 0, err)
        self.assertIn("updated parity/classes/symfony/example.tsv", out)
        self.assertEqual(self.run_mode(classes.check)[0], 0)
        text = (self.root / "parity/classes/symfony/example.tsv").read_text()
        self.assertIn("A.php\tA\t10\tpending\t\n", text)
        self.assertIn("B.php\tB AInterface\t30\tported\tcrates/example/src/b2.rs\n", text)
        self.assertIn("C.php\t-\t5\tn/a: PHP-only helper\t\n", text)
        self.write("parity/sources.tsv", "# header\ncrates/x.rs\tsymfony/example\tD.php\t123\n")
        code, _, err = self.run_mode(classes.sync)
        self.assertEqual(code, 1)
        self.assertIn("run scripts/classes.py generate symfony/example", err)

    def test_ported_but_unmapped(self):
        self.write("parity/sources.tsv", "# header\n")
        code, _, err = self.run_mode(classes.check)
        self.assertEqual(code, 1)
        self.assertIn("A.php is ported but no Rust file names it", err)

    def test_unlisted_mapped_file(self):
        self.write("parity/sources.tsv", (self.root / "parity/sources.tsv").read_text()
                   + "crates/example/src/d.rs\tsymfony/example\tD.php\t123\n")
        code, _, err = self.run_mode(classes.check)
        self.assertEqual(code, 1)
        self.assertIn("sources.tsv names D.php, which the manifest does not list", err)

    def test_stale_pin_and_bad_status(self):
        path = self.root / "parity/classes/symfony/example.tsv"
        text = path.read_text().replace("at 123", "at 456").replace("n/a: PHP-only helper", "skipped")
        path.write_text(text)
        code, _, err = self.run_mode(classes.check)
        self.assertEqual(code, 1)
        self.assertIn("header does not match the pin", err)
        self.assertIn("C.php has unknown status 'skipped'", err)

    def test_missing_manifest(self):
        (self.root / "parity/classes/symfony/example.tsv").unlink()
        code, _, err = self.run_mode(classes.check)
        self.assertEqual(code, 1)
        self.assertIn("parity/classes/symfony/example.tsv is missing", err)

    def test_test_manifest_checks_skip_class_manifests(self):
        (self.root / "scripts").mkdir()
        for name in ("parity-check.sh", "parity_check.py"):
            shutil.copyfile(Path(__file__).parent / name, self.root / "scripts" / name)
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        self.write("upstream.tsv", "# name\trepo\tref\n")
        self.write("parity/symfony/example.tsv", "# file\ttest\tstatus\trust\n")
        bash = bash_executable(None)
        reference = subprocess.run([bash, "scripts/parity-check.sh"], cwd=self.root,
                                   capture_output=True, text=True,
                                   env={k: v for k, v in os.environ.items()
                                        if k != "SYMFONY_RS_PYTHON"})
        self.assertEqual(reference.returncode, 0, reference.stderr)
        self.assertNotIn("classes", reference.stdout)
        fast = subprocess.run([sys.executable, "scripts/parity_check.py", "parity"], cwd=self.root,
                              capture_output=True, text=True)
        self.assertEqual(fast.returncode, 0, fast.stderr)
        self.assertEqual(fast.stdout, reference.stdout)


if __name__ == "__main__":
    unittest.main()
