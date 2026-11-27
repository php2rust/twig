#!/usr/bin/env bash
# (Re)generate parity manifests from the pinned upstream tests.
#
#   scripts/manifest.sh symfony/routing [symfony/http-kernel ...]
#
# Lists every upstream PHPUnit test method with oracle/manifest.php (PHP's
# own tokenizer, in the oracle image) and merges the list into
# parity/<package>.tsv, keeping each existing row's status and Rust test.
# New tests start as `pending`; tests no longer upstream are dropped and
# printed. Needs Docker and scripts/fetch-upstream.sh run first.
#
# Columns: upstream test file, Class::method, status, Rust test.
# Status: pending | ported | n/a: <reason>. A ported row names its Rust
# test as <file>::<fn>; scripts/parity-check.sh verifies it exists.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
[[ $# -gt 0 ]] || { echo "usage: scripts/manifest.sh PACKAGE..." >&2; exit 2; }

for package in "$@"; do
    if ! awk -F'\t' -v p="$package" '!/^#/ && $1 == p { found=1 } END { exit !found }' parity/packages.tsv; then
        echo "unknown package $package (not in parity/packages.tsv)" >&2; exit 1
    fi
    [[ -d "upstream/$package" ]] || scripts/fetch-upstream.sh "$package"
    out="parity/$package.tsv"
    mkdir -p "$(dirname "$out")"
    fresh=$(mktemp)
    MSYS_NO_PATHCONV=1 docker compose -f oracle/compose.yaml run --rm -T php \
        oracle/manifest.php "upstream/$package" > "$fresh"
    old=$(mktemp)
    if [[ -f "$out" ]]; then grep -v '^#' "$out" > "$old" || true; fi
    {
        printf '# %s at %s: upstream test\tmethod\tstatus\trust test\n' "$package" \
            "$(awk -F'\t' -v p="$package" '$1 == p { print $3 }' parity/packages.tsv)"
        awk -F'\t' -v OFS='\t' '
            FILENAME == ARGV[1] { status[$1 FS $2] = $3; rust[$1 FS $2] = $4; next }
            {
                key = $1 FS $2
                s = (key in status) ? status[key] : "pending"
                print $1, $2, s, (key in rust) ? rust[key] : ""
                seen[key] = 1
            }
            END {
                for (key in status) if (!(key in seen))
                    print "dropped (no longer upstream): " key > "/dev/stderr"
            }' "$old" "$fresh"
    } > "$out.tmp"
    mv "$out.tmp" "$out"
    rm -f "$fresh" "$old"
    echo "$out: $(grep -vc '^#' "$out") tests"
done
