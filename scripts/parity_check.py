#!/usr/bin/env python3
"""Batch equivalents of sources.sh and parity-check.sh; standard library only.

The Bash entry points retain their original implementations when
SYMFONY_RS_PYTHON is unset. dev_check.py supplies its own interpreter to
avoid thousands of grep/awk/sed processes on Windows.
"""
from __future__ import annotations

import argparse
import difflib
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def rust_files(root: Path, *, fixtures: bool = False) -> list[str]:
    patterns = ["crates/*.rs", "examples/*.rs"]
    if fixtures:
        patterns.append("fixtures/*.rs")
    output = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", *patterns],
        cwd=root,
    )
    return sorted(path.decode("utf-8", "surrogateescape") for path in output.split(b"\0") if path)


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="surrogateescape")


def source_map(root: Path) -> tuple[str, list[str]]:
    known: dict[str, str] = {}
    for name in ("parity/packages.tsv", "upstream.tsv"):
        for line in text(root / name).splitlines():
            fields = line.split("\t")
            if line and not line.startswith("#") and len(fields) >= 3:
                known[fields[0]] = fields[2]
    output = ["# rust file\tpackage\tupstream path\treference\n"]
    errors = []
    for name in rust_files(root, fixtures=True):
        lines = [line.removeprefix("//! Upstream: ")
                 for line in text(root / name).splitlines()
                 if line.startswith("//! Upstream: ")]
        if not lines:
            errors.append(f"no Upstream line: {name}")
        for line in lines:
            fields = line.split()
            package = fields[0] if fields else ""
            if package == "none":
                continue
            if package not in known:
                errors.append(f"unknown package '{package}' in {name}")
                continue
            if len(fields) < 2:
                errors.append(f"no upstream path for {package} in {name}")
                continue
            for path in fields[1:]:
                upstream = root / "upstream" / package
                if upstream.is_dir() and not (upstream / path).is_file():
                    errors.append(f"missing upstream/{package}/{path} (named in {name})")
                output.append(f"{name}\t{package}\t{path}\t{known[package]}\n")
    return "".join(output), errors


def check_sources(root: Path, check: bool) -> int:
    output, errors = source_map(root)
    destination = root / "parity/sources.tsv"
    if check:
        previous = text(destination)
        if previous != output:
            sys.stdout.writelines(difflib.unified_diff(
                previous.splitlines(keepends=True), output.splitlines(keepends=True),
                fromfile="parity/sources.tsv", tofile="generated sources",
            ))
            errors.append("parity/sources.tsv is stale; run scripts/sources.sh")
    else:
        destination.write_text(output, encoding="utf-8", newline="\n")
    for error in errors:
        print(error, file=sys.stderr)
    return int(bool(errors))


def check_parity(root: Path) -> int:
    claims = set()
    errors = []
    contents: dict[str, str] = {}

    def read(name: str) -> str:
        if name not in contents:
            contents[name] = text(root / name)
        return contents[name]

    done_file = root / "parity/done.txt"
    done = set(text(done_file).splitlines()) if done_file.is_file() else set()
    manifests = sorted(path for path in (root / "parity").rglob("*.tsv")
                       if len(path.relative_to(root / "parity").parts) >= 2
                       # Class manifests: scripts/classes.py checks them.
                       and path.relative_to(root / "parity").parts[0] != "classes")
    for manifest in manifests:
        name = manifest.relative_to(root).as_posix()
        package = manifest.relative_to(root / "parity").with_suffix("").as_posix()
        lines = text(manifest).splitlines()
        total = sum(not line.startswith("#") for line in lines)
        # The reference summary uses awk's raw TSV columns, whereas
        # validation uses Bash read's whitespace rules.
        columns = [line.split("\t") for line in lines if not line.startswith("#")]
        ported = sum(len(fields) > 2 and fields[2] == "ported" for fields in columns)
        pending = sum(len(fields) > 2 and fields[2] == "pending" for fields in columns)
        for line in lines:
            if not line or line.startswith("#"):
                continue
            # Bash read with IFS=tab collapses adjacent/leading tabs.
            # Preserve that rule so malformed rows cannot bypass a check.
            fields = re.split(r"\t+", line.strip("\t"), maxsplit=3)
            fields += [""] * max(0, 4 - len(fields))
            _, test, state, rust = fields[:4]
            if state == "pending":
                pass
            elif state.startswith("n/a: ") and len(state) > len("n/a: "):
                pass
            elif state == "ported":
                rfile, _, _ = rust.partition("::")
                rfn = rust.rsplit("::", 1)[-1]
                if not rust or rfile == rust or not (root / rfile).is_file():
                    errors.append(f"{name}: {test} is ported but names no Rust test file ('{rust}')")
                elif not re.search(r"fn " + rfn + r"\s*(<|\()", read(rfile), re.ASCII):
                    errors.append(f"{name}: {test} names {rust}, which does not exist")
                else:
                    claims.add(rust)
            else:
                errors.append(f"{name}: {test} has unknown status '{state}'")
        print(f"{package:<40} {total:5} tests {ported:5} ported "
              f"{total - ported - pending:5} n/a {pending:5} pending")
        if package in done and pending > 0:
            errors.append(f"{package} is in parity/done.txt but has {pending} pending tests")

    upstream_tests = re.compile(r"^//! Upstream: \S+[ \t]+(?:\S+[ \t]+)*(?:\S*/)?[Tt]ests/", re.MULTILINE)
    attribute = re.compile(r"#\[(tokio::)?test")
    for name in rust_files(root):
        source = read(name)
        if not upstream_tests.search(source):
            continue
        lines = source.splitlines()
        functions = set()
        for index, line in enumerate(lines):
            if attribute.search(line):
                for context in lines[index:index + 4]:
                    functions.update(re.findall(r"fn (test[A-Za-z0-9_]*)", context))
        for function in sorted(functions):
            if f"{name}::{function}" not in claims:
                errors.append(f"{name}::{function} ports an upstream test but no manifest row claims it")
    for error in errors:
        print(error, file=sys.stderr)
    return int(bool(errors))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["sources", "parity"])
    parser.add_argument("--check", action="store_true")
    options = parser.parse_args()
    if options.mode == "parity":
        if options.check:
            parser.error("--check only applies to sources")
        return check_parity(ROOT)
    return check_sources(ROOT, options.check)


if __name__ == "__main__":
    sys.exit(main())

