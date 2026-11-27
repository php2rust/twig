# Instructions for agents

This repository ports twig/twig to Rust, behaviour for behaviour.
Templates are compiled to Rust ahead of time; the runtime runs them.

## Per-file loop

1. Port one PHP file at a time. Read the whole file and its tests first.
   Each Rust module's first doc lines name the PHP file it ports
   (`//! Upstream: twig/twig src/X.php`); `scripts/sources.sh` builds
   `parity/sources.tsv` from those lines. Then run
   `python scripts/classes.py sync`.
2. Port the upstream tests in the same change, with the same names and
   assertions, and mark them `ported` in `parity/twig/twig.tsv`. Record
   vectors from real PHP for what tests leave open (a recorder in
   `oracle/recorders/`, a target in `scripts/record.sh`).
3. Update the crate README's contract table, and its "Known differences
   from PHP" table for any behaviour that cannot be kept. Nothing outside
   the contract table is claimed.

## Rules

- No PHP source code (php-src) in this repository. twig/twig's source and
  tests (BSD-3-Clause) are the specification. PHP's own behaviour belongs
  in php2rust/compatibility-tools-rs (php2rust-value and the other crates),
  learned from the oracle.
- Copy behaviour, not style. Use upstream's message texts; never reword
  one. If PHP's behaviour cannot be kept, stop and ask the owner.
- No `unsafe`. No `unwrap`/`expect` on anything from a template, a file or
  the user.
- Build scripts and binaries that recurse deeply (the parser, the
  compiler) run on a spawned thread with a large stack: Windows gives the
  main thread 1 MB. Check with `ulimit -s 1024`.
- Avoid file names that Windows reserves (`nul`, `con`, `aux`, ...).
- Keep `publish = false` until the owner decides to publish.
- Run `python scripts/dev_check.py full` before a push. There is no hosted
  CI; do not add GitHub workflows.
- Small commits named after the PHP file or the area.
- Keep the BSD-3-Clause licence and the Twig Team's copyright line. No
  wording that suggests the Twig Team or Symfony endorses this work.
