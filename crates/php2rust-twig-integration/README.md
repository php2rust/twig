# Twig integration fixtures

Private workspace test crate for `crates/twig`: a port of twig/twig's
`tests/IntegrationTest.php` (v3.30.0). Its build script compiles every case
of upstream's `tests/Fixtures/**/*.test` to Rust with `twig::compiler`, and
`tests/integration.rs` runs each one against the result real Twig gave
(`tests/data/integration.json`). Twig source and tests copyright (c)
2009-present by the Twig Team, BSD-3-Clause
([LICENSE-Twig](../twig/LICENSE-Twig)). Unofficial, not affiliated with or
endorsed by Twig or Symfony.

`src/extension.rs` and `src/callables.rs` port `TwigTestExtension`: its
filters, functions, tests and the `§` tag. `src/classes.rs` ports the
classes the `--DATA--` sections create (`TwigTestFoo` and the stubs). The cases that do not pass yet
are in `tests/data/integration_pending.txt`, with the reason; the test
fails when one of them passes, so the list stays exact.

The sandbox tests use template sets that the build script compiles too:
`tests/sandbox_test.rs` (`tests/Sandbox/SandboxTest.php`, sets in
`tests/data/sandbox_templates.json`, callables in `src/sandbox*.rs`) and
`tests/legacy_sandbox_test.rs` (`tests/Extension/SandboxTest.php`, sets in
`tests/data/legacy_sandbox_templates.json`, classes and callables in
`src/legacy_sandbox*.rs`).

Vectors: `scripts/record.sh twig-integration` runs
`oracle/recorders/twig_integration.php` and writes
`tests/data/integration.json`. Each case has the template sources, the
`--CONFIG--` options, the `--DATA--` context as a typed tree (objects with
their state), and the
trimmed output or the exception (`Class: message`) with the deprecations of
loading and of rendering.
