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
filters, functions, tests and the `§` tag. The cases that do not pass yet
are in `tests/data/integration_pending.txt`, with the reason; the test
fails when one of them passes, so the list stays exact.

Vectors: `scripts/record.sh twig-integration` runs
`oracle/recorders/twig_integration.php` and writes
`tests/data/integration.json`. Each case has the template sources, the
`--CONFIG--` options, the `--DATA--` context as a typed tree, and the
trimmed output or the exception (`Class: message`) with the deprecations of
loading and of rendering.
