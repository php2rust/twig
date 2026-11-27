#!/usr/bin/env bash
# Build parity/sources.tsv, the map from Rust files to the upstream files
# they port, from each file's `//! Upstream: <package> <path>...` lines.
#
#   scripts/sources.sh           rewrite parity/sources.tsv
#   scripts/sources.sh --check   fail if it is stale, a Rust file has no
#                                Upstream line, a package is unknown, or
#                                (when upstream/ is fetched) a path is missing
#
# `//! Upstream: none` marks a file that ports nothing (examples, glue).
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
# The development runner opts into the batch implementation with its
# current interpreter. Unset the variable to use the reference Bash check.
if [[ -n "${SYMFONY_RS_PYTHON:-}" ]]; then
    exec "$SYMFONY_RS_PYTHON" "$root/scripts/parity_check.py" sources "$@"
fi

known() { # package -> reference, from parity/packages.tsv or upstream.tsv
    awk -F'\t' -v p="$1" '!/^#/ && $1 == p { print $3; found=1 } END { exit !found }' \
        parity/packages.tsv upstream.tsv
}

status=0
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
printf '# rust file\tpackage\tupstream path\treference\n' > "$tmp"

while IFS= read -r file; do
    lines=$(grep -E '^//! Upstream: ' "$file" | sed 's#^//! Upstream: ##' || true)
    if [[ -z "$lines" ]]; then
        echo "no Upstream line: $file" >&2; status=1; continue
    fi
    while IFS= read -r line; do
        read -r package paths <<< "$line"
        [[ "$package" == none ]] && continue
        if ! ref=$(known "$package"); then
            echo "unknown package '$package' in $file" >&2; status=1; continue
        fi
        if [[ -z "$paths" ]]; then
            echo "no upstream path for $package in $file" >&2; status=1; continue
        fi
        for path in $paths; do
            if [[ -d "upstream/$package" && ! -f "upstream/$package/$path" ]]; then
                echo "missing upstream/$package/$path (named in $file)" >&2; status=1
            fi
            printf '%s\t%s\t%s\t%s\n' "$file" "$package" "$path" "$ref" >> "$tmp"
        done
    done <<< "$lines"
done < <(git ls-files --cached --others --exclude-standard -- 'crates/*.rs' 'examples/*.rs' 'fixtures/*.rs' | sort)

if [[ "${1:-}" == "--check" ]]; then
    if ! diff -u parity/sources.tsv "$tmp"; then
        echo "parity/sources.tsv is stale; run scripts/sources.sh" >&2; status=1
    fi
else
    cp "$tmp" parity/sources.tsv
fi
exit $status
