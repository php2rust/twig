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
| `Parser`: subparse, blocks, macros, imports, traits, embedded templates, documentation targets, child-template body cleanup, unknown tag/filter/function/test messages with suggestions, the node visitors at the end of each parse | `Parser.php` | ParserTest (30 of 30) |
| Node visitors and the traverser: correctness checks (content outside blocks, `extends` placement, `block` under a tag, macro calls without parentheses), optimizer (constant prints to text, direct block/parent output, `for` without `loop`), safe analysis, auto-escaping (`autoescape` option, `'name'` strategy, `autoescape` tag, `pre_escape`); one visitor set for each environment, as in PHP | `NodeTraverser.php`, `NodeVisitor/*.php`, `FileExtensionEscapingStrategy.php`, `Extension/OptimizerExtension.php` | CorrectnessTest (3), OptimizerTest (5), SafeAnalysisTest (1), FileExtensionEscapingStrategyTest (1); EscaperTest pending on the runtime; visitor vectors |
| Token parsers: every core tag (`apply`, `autoescape`, `block`, `deprecated`, `do`, `embed`, `extends`, `flush`, `for`, `from`, `guard`, `if`, `import`, `include`, `macro`, `sandbox`, `set`, `types`, `use`, `with`) | `TokenParser/*.php` | BlockTokenParserTest (5; 2 pending on the runtime), TypesTokenParserTest (2); parser vectors |
| `Environment`: options `autoescape` and `optimizations`, extension registry, lexer from the registered operators, node visitors, `tokenize()`, `parse()` | `Environment.php` (parse side) | all of the above |
| `Compiler`: Rust code generation, one module per template (and per embedded template) with a `CompiledTemplate` impl; the `compile()` of every node class but the sandbox nodes, `enum`/`enum_cases` and the date callables; PHP's line guess (`__line`) and unary-before-`**` precedence kept; `compile_set()` for a build script | `Compiler.php`, `Node/**/*.php` (`compile()`) | integration fixtures |
| Runtime: `Template` (blocks, parents, traits, macros, `load()`, `display()`, `render()`, error line guess), `TemplateWrapper`, `Markup`, `MacroNamespace`, `TwigMacro`, the compiled-template loader, `EscaperRuntime` (all built-in strategies, custom escapers, safe classes), the `CoreExtension` runtime methods (filters, functions, tests, `getAttribute()`, `include`, `source`, arrow functions) and the multibyte string functions they call | `Template.php`, `TemplateWrapper.php`, `Markup.php`, `MacroNamespace.php`, `TwigMacro.php`, `Loader/*.php`, `Runtime/EscaperRuntime.php`, `Extension/CoreExtension.php` | integration fixtures |

Parser vectors: 806 templates (all upstream fixture templates and 145 edge
cases) parsed by real Twig with the node visitors off and the extensions of
`IntegrationTest`. The full node tree (classes, attributes, lines, tags,
documentation, source names), or the error's class, message and line, and
the deprecations must match. Visitor vectors: the same templates and 112
more cases (`tests/data/parser_visitors_extra.json`, each with its
`autoescape` and `optimizations` options) parsed with the node visitors
on, except the sandbox visitor.

Integration fixtures: `crates/twig-integration` compiles every case of
upstream's `tests/Fixtures/**/*.test` to Rust in its build script and runs
it against what real Twig did (`IntegrationTest::testIntegration`): 361 of
457 cases pass, 2 are pending (listed with the reason in
`tests/data/integration_pending.txt`), 94 are skipped (data with objects,
which need the test classes; the sandbox, `date`, enums, `dump`,
`template_from_string`; a false `--CONDITION--`).

Not yet ported: the sandbox, the date filters, `enum`/`enum_cases`, the
debug and string loader extensions, the filesystem loader and the cache,
the undefined-callable callbacks (GuardTokenParserTest pending), the
`path()`/`app.request` hooks (after the HTTP path merges), the PHP test
classes of the integration fixtures, and the legacy fixtures.

Vectors: `scripts/record.sh twig-lexer` runs
`oracle/recorders/twig_lexer.php` and writes `tests/data/lexer.json`;
`scripts/record.sh twig-parser` runs `oracle/recorders/twig_parser.php` and
writes `tests/data/parser.json` (visitors off) and
`tests/data/parser_visitors.json` (visitors on).

Dependencies: `php-pcre` (workspace) checks the `matches` regex as PHP's
`preg_match()` does; `symfony-deprecation-contracts` (workspace) for
`trigger_deprecation()`.
