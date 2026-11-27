# php2rust-twig

An unofficial Rust port of [Twig](https://github.com/twigphp/Twig) 3.30.0,
behaviour for behaviour: the same output, messages, errors and deprecations
as the PHP library. Templates are compiled to Rust ahead of time, in a
build script (`twig::compiler::compile_set()`), and run with the crate's
runtime. The upstream PHPUnit tests and integration fixtures are ported
with the same names, and vectors recorded from real PHP 8.4.27 and
twig/twig pin what the tests leave open. Not affiliated with or endorsed by
the Twig Team or Symfony.

Part of the php2rust project. The crates came from
[symfony-rs](https://github.com/php2rust/symfony-rs), with their git
history. PHP's own behaviour (values, arrays, dates, regex, iconv) comes
from [php2rust/compatibility-tools-rs](https://github.com/php2rust/compatibility-tools-rs).

| Crate | What it is |
|---|---|
| [php2rust-twig](crates/php2rust-twig) | The port: lexer, parser, node visitors, the Rust compiler, the runtime, the sandbox (library name `twig`) |
| [php2rust-twig-integration](crates/php2rust-twig-integration) | Test crate: upstream's integration fixtures and sandbox tests, compiled by its build script |

**Status.** Work in progress: see the contract table in
[crates/php2rust-twig/README.md](crates/php2rust-twig/README.md), its list
of known differences from PHP, and the test manifest `parity/twig/twig.tsv`
(`ported`, `n/a` or `pending` for every upstream test). Not published to
crates.io (`publish = false`); use a git dependency pinned to a commit:

```toml
[dependencies]
php2rust-twig = { git = "https://github.com/php2rust/twig-rs", rev = "<commit>" }
```

The library is named `twig`, as the compiled templates call it
(`::twig::runtime::...`). Its `trigger_deprecation()` comes from
`php2rust-symfony-deprecation-contracts` in
[php2rust/symfony-deprecation-contracts-rs](https://github.com/php2rust/symfony-deprecation-contracts-rs). A
workspace that uses Twig takes that crate from the same git source, so
that one deprecation handler sees all deprecations. Twig does not depend
on symfony-rs.

To change php2rust/compatibility-tools-rs and see the result here at once,
add an uncommitted `.cargo/config.toml`:

```toml
[patch."https://github.com/php2rust/compatibility-tools-rs"]
php2rust-value = { path = "../compatibility-tools/crates/php2rust-value" }
```

When the change is merged there, update `rev` in the root `Cargo.toml`.

## Checks

There is no hosted CI. Run the checks locally:

```sh
python scripts/dev_check.py full     # fmt, clippy -D warnings, tests, parity
```

or one at a time:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/sources.sh --check      # parity/sources.tsv from `//! Upstream:` lines
bash scripts/parity-check.sh         # parity/twig/twig.tsv against the tests
python scripts/classes.py check      # parity/classes/ against parity/sources.tsv
```

Build scripts and tests must also pass on Windows, where the main thread
has a 1 MB stack: to check on Linux, run the build and tests under
`ulimit -s 1024`.

## Upstream and vectors

`scripts/fetch-upstream.sh` checks out twig/twig and
symfony/deprecation-contracts at the pinned commits
(`parity/packages.tsv`, `upstream.tsv`) into `upstream/` (ignored by Git, read only).
`scripts/record.sh twig-lexer|twig-parser|twig-integration|twig-callable-parameters|all`
runs the recorders in `oracle/recorders/` in the PHP 8.4.27 image
(`oracle/`, Docker) and writes the crates' `tests/data/`. Without the
image, `docker run --rm -v "$PWD:/work:ro" -w /work php:8.4.27-cli-bookworm
php -d display_errors=stderr oracle/recorders/<recorder>.php` gives the
same vectors.

## Licence

BSD-3-Clause, see [LICENSE](LICENSE). Twig's source and tests are
copyright the Twig Team (BSD-3-Clause); the port keeps their notice.
