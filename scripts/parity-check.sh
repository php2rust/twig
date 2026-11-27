#!/usr/bin/env bash
# Check the parity manifests (parity/<vendor>/<package>.tsv). No PHP needed.
#
#   scripts/parity-check.sh            check and print a summary
#
# Fails when:
#   - a status is not `pending`, `ported` or `n/a: <reason>`;
#   - a `ported` row does not name an existing Rust test (<file>::<fn>);
#   - a package listed in parity/done.txt still has `pending` rows;
#   - a Rust test file that ports upstream tests (an `//! Upstream:` line
#     naming a Tests/ or tests/ path) has a #[test] fn named test* that no
#     manifest row claims.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
# The development runner opts into the batch implementation with its
# current interpreter. Unset the variable to use the reference Bash check.
if [[ -n "${SYMFONY_RS_PYTHON:-}" ]]; then
    exec "$SYMFONY_RS_PYTHON" "$root/scripts/parity_check.py" parity "$@"
fi
status=0
claims=$(mktemp)
trap 'rm -f "$claims"' EXIT

shopt -s nullglob globstar
for manifest in parity/*/**/*.tsv; do
    [[ "$manifest" == parity/packages.tsv || "$manifest" == parity/sources.tsv ]] && continue
    [[ "$manifest" == parity/classes/* ]] && continue # scripts/classes.py checks these
    package=${manifest#parity/}
    package=${package%.tsv}
    while IFS=$'\t' read -r file test state rust; do
        [[ -z "$file" || "$file" == \#* ]] && continue
        case "$state" in
            pending) ;;
            "n/a: "?*) ;;
            ported)
                rfile=${rust%%::*}
                rfn=${rust##*::}
                if [[ -z "$rust" || "$rfile" == "$rust" || ! -f "$rfile" ]]; then
                    echo "$manifest: $test is ported but names no Rust test file ('$rust')" >&2; status=1
                elif ! grep -Eq "fn ${rfn}[[:space:]]*(<|\()" "$rfile"; then
                    echo "$manifest: $test names $rust, which does not exist" >&2; status=1
                else
                    printf '%s\n' "$rust" >> "$claims"
                fi
                ;;
            *) echo "$manifest: $test has unknown status '$state'" >&2; status=1 ;;
        esac
    done < "$manifest"
    total=$(grep -vc '^#' "$manifest" || true)
    pending=$(awk -F'\t' '!/^#/ && $3 == "pending"' "$manifest" | wc -l)
    ported=$(awk -F'\t' '!/^#/ && $3 == "ported"' "$manifest" | wc -l)
    printf '%-40s %5d tests %5d ported %5d n/a %5d pending\n' \
        "$package" "$total" "$ported" "$((total - ported - pending))" "$pending"
    if [[ -f parity/done.txt ]] && grep -qxF "$package" parity/done.txt && [[ $pending -gt 0 ]]; then
        echo "$package is in parity/done.txt but has $pending pending tests" >&2; status=1
    fi
done

# Rust test files that port upstream tests: every test* fn must be claimed.
while IFS= read -r file; do
    grep -Eq '^//! Upstream: [^[:space:]]+[[:blank:]]+([^[:space:]]+[[:blank:]]+)*([^[:space:]]*/)?[Tt]ests/' "$file" || continue
    while IFS= read -r fn; do
        if ! grep -qxF "$file::$fn" "$claims"; then
            echo "$file::$fn ports an upstream test but no manifest row claims it" >&2; status=1
        fi
    done < <(grep -A3 -E '#\[(tokio::)?test' "$file" | grep -oE 'fn test[A-Za-z0-9_]*' | sed 's/^fn //' | sort -u)
done < <(git ls-files --cached --others --exclude-standard -- 'crates/*.rs' 'examples/*.rs' | sort)

exit $status
