#!/usr/bin/env bash
# Fetch the pinned upstream sources, with their tests, into upstream/.
#
#   scripts/fetch-upstream.sh                 fetch every checkout
#   scripts/fetch-upstream.sh PACKAGE...      fetch only these packages
#   scripts/fetch-upstream.sh --verify DIR    fetch all, then compare each
#                                             package with the Composer
#                                             vendor directory DIR
#
# Composer packages are checked out at the exact commit symfony/demo
# installed (parity/packages.tsv); other sources at a tag or a commit
# (upstream.tsv), after the packages, so one can sit inside a package.
# upstream/ is ignored by Git and is read-only reference material.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

verify=
if [[ "${1:-}" == "--verify" ]]; then
    verify=${2:?usage: --verify VENDOR_DIR}
    shift 2
fi
only=("$@")

wanted() {
    [[ ${#only[@]} -eq 0 ]] && return 0
    local x
    for x in "${only[@]}"; do [[ "$x" == "$1" ]] && return 0; done
    return 1
}

# Byte-exact files: no CRLF conversion on Windows checkouts.
git_() { git -c core.autocrlf=false -c advice.detachedHead=false "$@"; }

checkout() { # dir repository ref
    local dir=$1 repo=$2 ref=$3
    if [[ -d "$dir/.git" ]] && [[ "$(git -C "$dir" config --get symfony-rs.ref || true)" == "$ref" ]]; then
        return 0
    fi
    rm -rf "$dir"
    mkdir -p "$dir"
    git_ -C "$dir" init --quiet
    git -C "$dir" config core.autocrlf false
    git_ -C "$dir" fetch --quiet --depth 1 "$repo" "$ref"
    git_ -C "$dir" checkout --quiet FETCH_HEAD
    git -C "$dir" config symfony-rs.ref "$ref"
    echo "fetched $dir at $ref"
}

while IFS=$'\t' read -r package version reference repo; do
    [[ -z "$package" || "$package" == \#* ]] && continue
    wanted "$package" || continue
    checkout "upstream/$package" "$repo" "$reference"
done < parity/packages.tsv

while IFS=$'\t' read -r name repo tag; do
    [[ -z "$name" || "$name" == \#* ]] && continue
    wanted "$name" || continue
    # a tag, or a commit (40 hex digits)
    ref=refs/tags/$tag
    [[ "$tag" =~ ^[0-9a-f]{40}$ ]] && ref=$tag
    checkout "upstream/$name" "$repo" "$ref"
done < upstream.tsv

[[ -n "$verify" ]] || exit 0
status=0
# Packages pinned past the vendor's lock: the oracle mounts them from
# upstream/ (oracle/compose.yaml), so they are expected to differ.
moved=" $(sed -n 's/^# moved: \([^(]*\).*/\1/p' parity/packages.tsv) "
while IFS=$'\t' read -r package version reference repo; do
    [[ -z "$package" || "$package" == \#* ]] && continue
    wanted "$package" || continue
    if [[ "$moved" == *" $package "* ]]; then
        if grep -qF -- "- ../upstream/$package:/demo/vendor/$package:ro" oracle/compose.yaml; then
            echo "moved   $package $version (mounted over the vendor copy)"
        else
            echo "MOVED   $package $version, but oracle/compose.yaml does not mount it"; status=1
        fi
        continue
    fi
    # Composer dist archives leave out what .gitattributes marks
    # export-ignore (tests, CI files); compare what both contain.
    out=$(diff -rq -x .git "upstream/$package" "$verify/$package" | grep -v '^Only in ' || true)
    if [[ -n "$out" ]]; then
        echo "DIFFERS $package $version"; echo "$out" | sed 's/^/    /'; status=1
    else
        echo "same    $package $version"
    fi
done < parity/packages.tsv
exit $status
