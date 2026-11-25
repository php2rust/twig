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
| `Lexer` with the core operators and custom tag options: text, `{{ }}`, `{% %}`, comments, documentation comments, `-` and `~` whitespace control, `verbatim`, `line`, names, numbers, strings with escapes and their deprecations, interpolation, inline comments, brackets, located `SyntaxError`s | `Lexer.php` | LexerTest (40 of 40); 733 templates (all upstream fixture templates and 72 edge cases) tokenized by real Twig, replayed token by token |
| Nodes: the `Node` handle (shared like PHP objects, `deep_clone()` is `clone`), `Nodes`, `EmptyNode`, documentation, `__toString()`; the constructors of the expression and statement node classes with their checks and rewrites (`TrueTest` wraps, `??`, `?:`, destructuring, `matches` regex check, `set` capture optimizations, macro arguments) | `Node/**/*.php` | NodeTest (3; 7 n/a); parser vectors |
| `TwigFilter`, `TwigFunction`, `TwigTest` and `DeprecatedCallableInfo`: options, dynamic `*` names, `getMinimalNumberOfRequiredArguments()`; the PHP callable is kept as its name | `AbstractTwigCallable.php`, `Twig{Filter,Function,Test}.php`, `DeprecatedCallableInfo.php` | parser vectors |
| `CallableArgumentsExtractor` (named and positional arguments against parameter specs) | `Util/CallableArgumentsExtractor.php` | unit tests; parser vectors |
| Extensions: `ExtensionInterface` definitions, `ExtensionSet` lookups with PHP's order and dynamic names; `CoreExtension` and `EscaperExtension` filters, functions, tests, tags and operators (definitions only) | `Extension/*.php`, `ExtensionSet.php` | parser vectors |
| Expression parsers: prefix, infix, precedence, associativity, precedence-change deprecations, arguments | `ExpressionParser/**/*.php` | ExpressionParserTest (22; 9 n/a; 8 pending on the compiler or runtime); parser vectors |
| `Parser`: subparse, blocks, macros, imports, traits, embedded templates, documentation targets, child-template body cleanup, unknown tag/filter/function/test messages with suggestions | `Parser.php` | ParserTest (22; 8 pending on node visitors) |
| Token parsers: every core tag (`apply`, `autoescape`, `block`, `deprecated`, `do`, `embed`, `extends`, `flush`, `for`, `from`, `guard`, `if`, `import`, `include`, `macro`, `sandbox`, `set`, `types`, `use`, `with`) | `TokenParser/*.php` | BlockTokenParserTest (5; 2 pending on the runtime), TypesTokenParserTest (2); parser vectors |
| `Environment`: extension registry, lexer from the registered operators, `tokenize()`, `parse()` | `Environment.php` (parse side) | all of the above |

Parser vectors: 806 templates (all upstream fixture templates and 145 edge
cases) parsed by real Twig with the node visitors off and the extensions of
`IntegrationTest`. The full node tree (classes, attributes, lines, tags,
documentation, source names), or the error's class, message and line, and
the deprecations must match.

Not yet ported: node visitors (escaper, optimizer, correctness, safe
analysis, sandbox), the Rust emitter, the runtime, loaders, the
undefined-callable callbacks (GuardTokenParserTest pending), the
`path()`/`app.request` hooks (after the HTTP path merges), and the
integration fixtures.

Vectors: `scripts/record.sh twig-lexer` runs
`oracle/recorders/twig_lexer.php` and writes `tests/data/lexer.json`;
`scripts/record.sh twig-parser` runs `oracle/recorders/twig_parser.php` and
writes `tests/data/parser.json`.

Dependencies: `php-pcre` (workspace) checks the `matches` regex as PHP's
`preg_match()` does; `symfony-deprecation-contracts` (workspace) for
`trigger_deprecation()`.
