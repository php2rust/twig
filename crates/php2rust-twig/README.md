# Twig

Private workspace crate: a port of `twig/twig` v3.30.0 (`8c737079`), as
installed by symfony/demo, for the build-time Twig compiler of PLAN.md
("M4: Twig"). Twig source and tests copyright (c) 2009-present by the
Twig Team, BSD-3-Clause ([LICENSE-Twig](LICENSE-Twig)). Unofficial, not
affiliated with or endorsed by Twig or Symfony.

The crate starts from the November 24 trial (`spikes/twig-trial`), which
stays as it is until this crate replaces it. Templates are bytes, as in
PHP.

| Contract | Source | Evidence |
|---|---|---|
| `Source`: code, name, path; `getColumn()` | `Source.php` | unit test; LexerTest column cases |
| `Error` with kinds `Error`, `SyntaxError`, `LoaderError`, `RuntimeError`: message rebuilt from raw message, ` in "<name>"`, ` at line N`, ` column C`, final `.`/`?` moved to the end; `appendMessage()`, `addSuggestions()` | `Error/*.php` | unit tests; LexerTest messages |
| `Token`: types, `test()` (with the 3.21 punctuation-to-operator rule and its deprecation), `typeToString()`, `typeToEnglish()`, offsets, documentation | `Token.php` | unit tests; LexerTest |
| `TokenStream`: `next()`, `nextIf()`, `expect()` (message text), `look()`, `injectTokens()`, `test()`, `isEOF()` | `TokenStream.php` | LexerTest |
| `Lexer` with the core operators and custom tag options: text, `{{ }}`, `{% %}`, comments, documentation comments, `-` and `~` whitespace control, `verbatim`, `line`, names, numbers, strings with escapes and their deprecations, interpolation, inline comments, brackets, located `SyntaxError`s | `Lexer.php` | LexerTest (38 of 40; 2 pending on the parser); 733 templates (all upstream fixture templates and 72 edge cases) tokenized by real Twig, replayed token by token |

Not yet ported: the parser, nodes, the extension registry (the lexer takes
the core operator list directly), the Rust emitter, the runtime, and the
integration fixtures.

Vectors: `scripts/record.sh twig-lexer` runs
`oracle/recorders/twig_lexer.php` and writes `tests/data/lexer.json`.
