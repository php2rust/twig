"""Class manifests: every upstream PHP source file, and whether it is ported.

    python scripts/classes.py generate PACKAGE...   (re)write parity/classes/<package>.tsv
    python scripts/classes.py generate --all        every package of parity/packages.tsv
    python scripts/classes.py sync                  update the status columns from
                                                    parity/sources.tsv (no PHP needed)
    python scripts/classes.py check                 fail if a manifest disagrees with
                                                    parity/sources.tsv (no PHP needed)
    python scripts/classes.py report [--markdown]   size-weighted port percentage

`generate` takes the .php files of the package's dist archive (`git archive`
of the pinned checkout, so .gitattributes export-ignore paths such as
twig/twig's extra/ or doctrine's ci/ are out), without Tests/, tests/ and
vendor/, and lists them with oracle/classes.php (PHP's own tokenizer, in
the oracle image; set SYMFONY_RS_PHP to a local php binary to run it
without Docker) on the checkouts of scripts/fetch-upstream.sh.

Columns: upstream file, classes it declares, code lines, status, Rust files.
Status: `ported` when parity/sources.tsv maps the file (a Rust module's
`//! Upstream:` line names it), else `pending`, or `n/a: <reason>` set by
hand and kept by `generate`. Rust files are the mapped files, separated by
spaces. The percentage counts code lines: ported / (all - n/a), for all
files and for the files outside Resources/data/ (generated data tables).

A file counts as ported as soon as one Rust module names it: the column
records the source map, not completeness. Pending upstream test rows in
parity/<package>.tsv show what of a mapped file is still open.
"""
import argparse
import io
import os
from pathlib import Path
import subprocess
import sys
import tarfile

ROOT = Path(__file__).resolve().parent.parent
HEADER = "upstream file\tclasses\tcode lines\tstatus\trust files"


def read_tsv(path: Path) -> list[list[str]]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            rows.append(line.split("\t"))
    return rows


def packages(root: Path) -> dict[str, str]:
    """Package -> pinned reference, from parity/packages.tsv."""
    return {row[0]: row[2] for row in read_tsv(root / "parity/packages.tsv")}


def is_test_path(path: str) -> bool:
    first = path.split("/", 1)[0]
    return first in ("Tests", "tests")


def is_data_path(path: str) -> bool:
    """Generated data tables (symfony/intl's ICU data, string's width
    tables): a port generates them from their sources, so the report also
    gives the percentage without them."""
    return path.startswith("Resources/data/") or "/Resources/data/" in path


def source_map(root: Path) -> dict[str, dict[str, list[str]]]:
    """Package -> upstream path -> Rust files, from parity/sources.tsv."""
    result: dict[str, dict[str, list[str]]] = {}
    for rust, package, path, *_ in read_tsv(root / "parity/sources.tsv"):
        if is_test_path(path):
            continue
        files = result.setdefault(package, {}).setdefault(path, [])
        if rust not in files:
            files.append(rust)
    return result


def manifest_path(root: Path, package: str) -> Path:
    return root / "parity/classes" / f"{package}.tsv"


def dist_files(root: Path, package: str) -> list[str]:
    """The files of the package's dist archive (what Composer installs):
    `git archive` honours .gitattributes export-ignore. The top-level test
    directories are left out too, for packages that ship them."""
    archive = subprocess.run(["git", "-C", f"upstream/{package}", "archive", "--format=tar", "HEAD"],
                             cwd=root, check=True, capture_output=True).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        names = [member.name for member in tar.getmembers() if member.isfile()]
    return sorted(name for name in names
                  if not is_test_path(name) and name.split("/", 1)[0] != "vendor")


def list_files(root: Path, package: str) -> list[list[str]]:
    paths = "".join(f"{name}\n" for name in dist_files(root, package) if name.endswith(".php"))
    php = os.environ.get("SYMFONY_RS_PHP")
    if php:
        command = [php, "oracle/classes.php", f"upstream/{package}"]
        env = None
    else:
        command = ["docker", "compose", "-f", "oracle/compose.yaml", "run", "--rm", "-T",
                   "php", "oracle/classes.php", f"upstream/{package}"]
        env = dict(os.environ, MSYS_NO_PATHCONV="1")
    output = subprocess.run(command, cwd=root, env=env, check=True, input=paths,
                            capture_output=True, text=True, encoding="utf-8").stdout
    return [line.split("\t") for line in output.splitlines() if line]


def generate(root: Path, package: str, refs: dict[str, str],
             mapped: dict[str, dict[str, list[str]]]) -> None:
    if not (root / "upstream" / package).is_dir():
        subprocess.run(["bash", "scripts/fetch-upstream.sh", package], cwd=root, check=True)
    out = manifest_path(root, package)
    kept = {}
    if out.is_file():
        kept = {row[0]: row[3] for row in read_tsv(out)
                if len(row) > 3 and row[3].startswith("n/a: ")}
    files = mapped.get(package, {})
    lines = [f"# {package} at {refs[package]}: {HEADER}"]
    listed = set()
    for path, classes, size in list_files(root, package):
        listed.add(path)
        rust = files.get(path, [])
        status = "ported" if rust else kept.get(path, "pending")
        lines.append("\t".join([path, classes, size, status, " ".join(rust)]))
    for path in sorted(set(files) - listed):
        print(f"{package}: sources.tsv names {path}, which is not a listed source file",
              file=sys.stderr)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")
    print(f"{out.relative_to(root).as_posix()}: {len(lines) - 1} files")


def sync(root: Path) -> int:
    """Update the status and Rust files columns from parity/sources.tsv,
    without PHP or upstream/: run it after scripts/sources.sh."""
    mapped = source_map(root)
    refs = packages(root)
    status = 0
    for package in refs:
        path = manifest_path(root, package)
        if not path.is_file():
            continue
        files = mapped.get(package, {})
        out = []
        listed = set()
        for line in path.read_text(encoding="utf-8").splitlines():
            row = line.split("\t")
            if line.startswith("#") or len(row) != 5:
                out.append(line)
                continue
            listed.add(row[0])
            rust = files.get(row[0], [])
            if rust:
                row[3], row[4] = "ported", " ".join(rust)
            elif row[3] == "ported":
                row[3], row[4] = "pending", ""
            out.append("\t".join(row))
        for file in sorted(set(files) - listed):
            print(f"{package}: sources.tsv names {file}, which the manifest does not list; "
                  f"run scripts/classes.py generate {package}", file=sys.stderr)
            status = 1
        text = "\n".join(out) + "\n"
        if text != path.read_text(encoding="utf-8"):
            with path.open("w", encoding="utf-8", newline="\n") as handle:
                handle.write(text)
            print(f"updated {path.relative_to(root).as_posix()}")
    return status


def check(root: Path) -> int:
    refs = packages(root)
    mapped = source_map(root)
    errors = []
    for package in refs:
        path = manifest_path(root, package)
        name = path.relative_to(root).as_posix()
        if not path.is_file():
            errors.append(f"{name} is missing; run scripts/classes.py generate {package}")
            continue
        text = path.read_text(encoding="utf-8")
        first = text.split("\n", 1)[0]
        if first != f"# {package} at {refs[package]}: {HEADER}":
            errors.append(f"{name}: header does not match the pin in parity/packages.tsv")
        files = mapped.get(package, {})
        seen = set()
        for row in read_tsv(path):
            if len(row) != 5:
                errors.append(f"{name}: row '{row[0]}' has {len(row)} columns, not 5")
                continue
            file, _, size, status, rust = row
            seen.add(file)
            if not size.isdigit():
                errors.append(f"{name}: {file} has code lines '{size}'")
            expected = " ".join(files.get(file, []))
            if status == "ported":
                if not expected:
                    errors.append(f"{name}: {file} is ported but no Rust file names it; "
                                  "run scripts/classes.py sync")
                elif rust != expected:
                    errors.append(f"{name}: {file} names '{rust}', sources.tsv has '{expected}'")
            elif status == "pending" or (status.startswith("n/a: ") and len(status) > 5):
                if expected:
                    errors.append(f"{name}: {file} is {status} but {expected} ports it; "
                                  "run scripts/classes.py sync")
                elif rust:
                    errors.append(f"{name}: {file} is {status} but names Rust files")
            else:
                errors.append(f"{name}: {file} has unknown status '{status}'")
        for file in sorted(set(files) - seen):
            errors.append(f"{name}: sources.tsv names {file}, which the manifest does not list")
    known = {manifest_path(root, p) for p in refs}
    for path in sorted((root / "parity/classes").rglob("*.tsv")):
        if path not in known:
            errors.append(f"{path.relative_to(root).as_posix()}: package not in parity/packages.tsv")
    for error in errors:
        print(error, file=sys.stderr)
    return int(bool(errors))


def report(root: Path, markdown: bool) -> int:
    rows = []
    totals = [0, 0, 0, 0]  # files, ported files, lines, ported lines
    data = [0, 0, 0, 0]    # the same, for Resources/data/ files only
    for package in packages(root):
        path = manifest_path(root, package)
        if not path.is_file():
            print(f"missing {path.relative_to(root).as_posix()}", file=sys.stderr)
            return 1
        files = ported_files = lines = ported_lines = 0
        for file, _, size, status, *_ in read_tsv(path):
            if status.startswith("n/a: "):
                continue
            files += 1
            lines += int(size)
            if status == "ported":
                ported_files += 1
                ported_lines += int(size)
            if is_data_path(file):
                data[0] += 1
                data[2] += int(size)
                if status == "ported":
                    data[1] += 1
                    data[3] += int(size)
        rows.append((package, files, ported_files, lines, ported_lines))
        for i, value in enumerate((files, ported_files, lines, ported_lines)):
            totals[i] += value

    def percent(part: int, whole: int) -> str:
        return f"{100 * part / whole:.1f}%" if whole else "-"

    code = [total - part for total, part in zip(totals, data)]
    summary = [("all", totals), ("code (no Resources/data/)", code)]
    if markdown:
        print("| Package | Files | Ported files | Code lines | Ported lines | Ported |")
        print("|---|---:|---:|---:|---:|---:|")
        for package, files, pf, lines, pl in rows:
            print(f"| {package} | {files} | {pf} | {lines} | {pl} | {percent(pl, lines)} |")
        for label, (files, pf, lines, pl) in summary:
            print(f"| **{label}** | {files} | {pf} | {lines} | {pl} | {percent(pl, lines)} |")
    else:
        for package, files, pf, lines, pl in rows:
            print(f"{package:<40} {pf:5}/{files:<5} files {pl:7}/{lines:<7} lines "
                  f"{percent(pl, lines):>6}")
        for label, (files, pf, lines, pl) in summary:
            print(f"{label:<40} {pf:5}/{files:<5} files {pl:7}/{lines:<7} lines "
                  f"{percent(pl, lines):>6}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="mode", required=True)
    gen = sub.add_parser("generate")
    gen.add_argument("packages", nargs="*")
    gen.add_argument("--all", action="store_true")
    sub.add_parser("check")
    sub.add_parser("sync")
    rep = sub.add_parser("report")
    rep.add_argument("--markdown", action="store_true")
    options = parser.parse_args()
    if options.mode == "check":
        return check(ROOT)
    if options.mode == "sync":
        return sync(ROOT)
    if options.mode == "report":
        return report(ROOT, options.markdown)
    refs = packages(ROOT)
    names = list(refs) if options.all else options.packages
    if not names:
        parser.error("name packages or pass --all")
    mapped = source_map(ROOT)
    for package in names:
        if package not in refs:
            print(f"unknown package {package} (not in parity/packages.tsv)", file=sys.stderr)
            return 1
        generate(ROOT, package, refs, mapped)
    return 0


if __name__ == "__main__":
    sys.exit(main())
