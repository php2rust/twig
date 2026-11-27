#!/usr/bin/env bash
# Record test vectors from real PHP and twig/twig (the oracle in oracle/,
# Docker) into the crates' tests/data/.
#
#   scripts/record.sh twig-lexer               crates/php2rust-twig/tests/data/lexer.json (Twig\Lexer on the upstream fixtures and lexer_extra.json)
#   scripts/record.sh twig-parser              crates/php2rust-twig/tests/data/parser.json, parser_visitors.json and parser_sandbox.json (parser trees without and with the node visitors, and with the sandbox visitor)
#   scripts/record.sh twig-integration         crates/php2rust-twig-integration/tests/data/integration.json (the integration fixtures run by real Twig)
#   scripts/record.sh twig-callable-parameters crates/php2rust-twig/tests/data/callable_parameters.json (the sandbox's view of the core callables' parameters)
#   scripts/record.sh all
#
# Run scripts/fetch-upstream.sh first: the recorders load twig/twig and
# symfony/deprecation-contracts from upstream/.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
export MSYS_NO_PATHCONV=1

if [[ ! -d upstream/twig/twig || ! -d upstream/symfony/deprecation-contracts ]]; then
    echo "upstream/ is not fetched: run scripts/fetch-upstream.sh" >&2
    exit 1
fi

# Build the oracle image first, with its log on stderr, so that the
# build log does not go into a recorder's JSON output.
docker compose -f oracle/compose.yaml build php >&2
oracle() { docker compose -f oracle/compose.yaml run --rm -T php "$@"; }

# record OUT RECORDER [ARG...]: run RECORDER, check that it wrote JSON, move it to OUT.
record() {
    local out=$1 tmp
    shift
    mkdir -p oracle/out "$(dirname "$out")"
    tmp=$(mktemp -p oracle/out)
    oracle "oracle/recorders/$@" > "$tmp"
    python -c 'import json,sys; print(len(json.load(open(sys.argv[1], encoding="utf-8"))), "entries")' "$tmp"
    mv "$tmp" "$out"
    echo "written to $out"
}

twig=crates/php2rust-twig/tests/data
integration=crates/php2rust-twig-integration/tests/data

twig_lexer() { record "$twig/lexer.json" twig_lexer.php; }
twig_parser() {
    record "$twig/parser.json" twig_parser.php plain
    record "$twig/parser_visitors.json" twig_parser.php visitors
    record "$twig/parser_sandbox.json" twig_parser.php sandbox
}
twig_integration() { record "$integration/integration.json" twig_integration.php; }
twig_callable_parameters() { record "$twig/callable_parameters.json" twig_callable_parameters.php; }

case "${1:-}" in
    twig-lexer) twig_lexer ;;
    twig-parser) twig_parser ;;
    twig-integration) twig_integration ;;
    twig-callable-parameters) twig_callable_parameters ;;
    all) twig_lexer; twig_parser; twig_integration; twig_callable_parameters ;;
    *) echo "usage: scripts/record.sh twig-lexer|twig-parser|twig-integration|twig-callable-parameters|all" >&2; exit 2 ;;
esac
