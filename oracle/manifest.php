<?php
// Lists every PHPUnit test method of one upstream package, using PHP's own
// tokenizer (nothing is loaded or executed).
//
//   php oracle/manifest.php upstream/symfony/routing
//
// Prints one row per test: <test file>\t<Class>::<method>. A method counts
// as a test when it is public and named test*, or carries #[Test] or an
// @test docblock tag (PHPUnit 11's rules). Methods of abstract classes are
// listed under each concrete subclass in the package, as PHPUnit runs them,
// and so are the test methods of the traits a class uses.
//
// A parent class or trait is found by its fully qualified name, resolved
// from the file's namespace and `use` imports as PHP resolves it, so classes
// that share a short name in different namespaces are kept apart. Parents
// are looked up in the whole package (e.g. a `Test/` base class beside
// `Tests/`) and in the pinned checkouts of the packages it requires
// (composer.json `require` and `require-dev`, then their `require`s), found
// beside it as <upstream>/<vendor>/<name>; their own Tests/ are skipped.
// Only classes under the package's Tests/ (or tests/) give rows.

$dir = rtrim($argv[1] ?? '', '/');
if ($dir === '' || !is_dir($dir)) {
    fwrite(STDERR, "usage: php manifest.php PACKAGE_DIR\n");
    exit(2);
}
$testsDir = null;
// Compare names exactly: on a case-insensitive mount (Docker Desktop on
// Windows) is_dir("$dir/Tests") is also true for a `tests/` directory.
$entries = scandir($dir) ?: [];
foreach (['Tests', 'tests'] as $candidate) {
    if (in_array($candidate, $entries, true) && is_dir("$dir/$candidate")) {
        $testsDir = "$dir/$candidate";
        break;
    }
}
if ($testsDir === null) {
    exit(0); // A package without tests has an empty manifest.
}
$upstream = dirname($dir, 2);

/** The .php files under $base, sorted, without vendor/ and the $skip subdirectories. */
function phpFiles(string $base, array $skip): array
{
    $paths = [];
    $iterator = new RecursiveIteratorIterator(new RecursiveCallbackFilterIterator(
        new RecursiveDirectoryIterator($base, FilesystemIterator::SKIP_DOTS),
        function (SplFileInfo $file) use ($base, $skip): bool {
            if (!$file->isDir()) {
                return true;
            }
            $relative = substr($file->getPathname(), strlen($base) + 1);
            return $relative !== 'vendor' && !in_array($relative, $skip, true) && $file->getFilename() !== '.git';
        },
    ));
    foreach ($iterator as $file) {
        if ($file->isFile() && str_ends_with($file->getFilename(), '.php')) {
            $paths[] = $file->getPathname();
        }
    }
    sort($paths, SORT_STRING);
    return $paths;
}

/** Package names a composer.json requires; $dev adds require-dev. */
function requires(string $packageDir, bool $dev): array
{
    $json = @file_get_contents("$packageDir/composer.json");
    $data = $json === false ? null : json_decode($json, true);
    if (!is_array($data)) {
        return [];
    }
    $names = array_keys($data['require'] ?? []);
    if ($dev) {
        $names = array_merge($names, array_keys($data['require-dev'] ?? []));
    }
    return array_values(array_filter($names, fn ($name) => is_string($name) && str_contains($name, '/')));
}

// [path, gives rows?]: the package's tests first, then everything else
// that may declare a parent class or trait.
$paths = [];
foreach (phpFiles($testsDir, []) as $path) {
    $paths[] = [$path, true];
}
foreach (phpFiles($dir, ['Tests', 'tests']) as $path) {
    $paths[] = [$path, false];
}
$queue = requires($dir, true);
$visited = [];
while ($queue !== []) {
    $name = array_shift($queue);
    if (isset($visited[$name]) || !is_dir("$upstream/$name")) {
        continue;
    }
    $visited[$name] = true;
    if (realpath("$upstream/$name") === realpath($dir)) {
        continue;
    }
    foreach (phpFiles("$upstream/$name", ['Tests', 'tests']) as $path) {
        $paths[] = [$path, false];
    }
    $queue = array_merge($queue, requires("$upstream/$name", false));
}

/** Next token index after $i that is not whitespace or a comment. */
function nextSignificant(array $tokens, int $i): int
{
    $n = count($tokens);
    for ($i++; $i < $n; $i++) {
        if (!$tokens[$i]->is([T_WHITESPACE, T_COMMENT, T_DOC_COMMENT])) {
            return $i;
        }
    }
    return $n;
}

/** Previous token index before $i that is not whitespace or a comment. */
function previousSignificant(array $tokens, int $i): int
{
    for ($i--; $i >= 0; $i--) {
        if (!$tokens[$i]->is([T_WHITESPACE, T_COMMENT, T_DOC_COMMENT])) {
            return $i;
        }
    }
    return -1;
}

/** A class name as written in a file, as a lowercase fully qualified name. */
function resolveClass(string $name, string $namespace, array $uses): string
{
    if (str_starts_with($name, '\\')) {
        return strtolower(substr($name, 1));
    }
    if (strncasecmp($name, 'namespace\\', 10) === 0) {
        $name = substr($name, 10);
    } else {
        $first = explode('\\', $name, 2)[0];
        if (isset($uses[strtolower($first)])) {
            return strtolower($uses[strtolower($first)] . substr($name, strlen($first)));
        }
    }
    return strtolower($namespace === '' ? $name : "$namespace\\$name");
}

/**
 * Reads the class imports of the `use` statement whose T_USE is token $i
 * into $uses (lowercase alias => fully qualified name), skipping function
 * and constant imports. Returns the index of the statement's `;`.
 */
function readUse(array $tokens, int $i, array &$uses): int
{
    $n = count($tokens);
    $kind = null; // T_FUNCTION or T_CONST for `use function` / `use const`
    $prefix = '';
    $name = '';
    $alias = null;
    $afterAs = false;
    $add = function () use (&$uses, &$kind, &$prefix, &$name, &$alias): void {
        if ($name !== '' && $kind === null) {
            $full = ltrim($prefix . $name, '\\');
            $parts = explode('\\', $full);
            $uses[strtolower($alias ?? end($parts))] = $full;
        }
        $name = '';
        $alias = null;
    };
    for ($i++; $i < $n; $i++) {
        $t = $tokens[$i];
        if ($t->is([T_WHITESPACE, T_COMMENT, T_DOC_COMMENT])) {
            continue;
        }
        if ($t->text === ';') {
            $add();
            return $i;
        }
        if ($t->is([T_FUNCTION, T_CONST])) {
            $kind = $t->id;
        } elseif ($t->is(T_AS)) {
            $afterAs = true;
        } elseif ($t->text === '{') { // group use: `use A\{B, C as D};`
            $prefix = rtrim($name, '\\') . '\\';
            $name = '';
        } elseif ($t->text === ',' || $t->text === '}') {
            $add();
        } elseif ($t->is([T_STRING, T_NAME_QUALIFIED, T_NAME_FULLY_QUALIFIED, T_NS_SEPARATOR])) {
            if ($afterAs) {
                $alias = $t->text;
                $afterAs = false;
            } else {
                $name .= $t->text;
            }
        }
    }
    return $n;
}

$classes = []; // lowercase fully qualified name => list of class records
foreach ($paths as [$path, $emit]) {
    $tokens = PhpToken::tokenize(file_get_contents($path));
    $n = count($tokens);
    $namespace = '';
    $uses = [];
    $nsDepth = 0; // 1 inside `namespace X { ... }`
    $depth = 0;
    $class = null;
    $classDepth = null;
    $attrTest = false;
    $doc = '';
    $modifiers = [];
    for ($i = 0; $i < $n; $i++) {
        $t = $tokens[$i];
        if ($t->is([T_WHITESPACE, T_COMMENT])) {
            continue;
        }
        if ($t->is(T_DOC_COMMENT)) {
            $doc = $t->text;
            continue;
        }
        if ($t->is(T_ATTRIBUTE)) {
            $text = '';
            $level = 1;
            while (++$i < $n && $level > 0) {
                if ($tokens[$i]->text === '[') {
                    $level++;
                } elseif ($tokens[$i]->text === ']') {
                    $level--;
                }
                $text .= $tokens[$i]->text;
            }
            $i--;
            // #[Test], #[\PHPUnit\Framework\Attributes\Test], #[Test, Group(...)]
            if (preg_match('/(?:^|[\s,\\\\])Test\s*(?:[\](,]|$)/', $text)) {
                $attrTest = true;
            }
            continue;
        }
        if ($t->text === '{' || $t->is([T_CURLY_OPEN, T_DOLLAR_OPEN_CURLY_BRACES])) {
            $depth++;
            $attrTest = false;
            $doc = '';
            $modifiers = [];
            continue;
        }
        if ($t->text === '}') {
            $depth--;
            if ($class !== null && $depth === $classDepth) {
                $classes[$class['key']][] = $class;
                $class = null;
            }
            continue;
        }
        if ($t->is([T_ABSTRACT, T_FINAL, T_READONLY, T_PUBLIC, T_PROTECTED, T_PRIVATE, T_STATIC])) {
            $modifiers[] = $t->id;
            continue;
        }
        if ($t->is(T_NAMESPACE) && $class === null) {
            $j = nextSignificant($tokens, $i);
            if ($j < $n && $tokens[$j]->is([T_STRING, T_NAME_QUALIFIED])) {
                $namespace = $tokens[$j]->text;
                $uses = [];
                $i = $j;
                $k = nextSignificant($tokens, $j);
                $nsDepth = ($k < $n && $tokens[$k]->text === '{') ? 1 : 0;
            } elseif ($j < $n && $tokens[$j]->text === '{') {
                $namespace = ''; // `namespace { ... }` is the global namespace
                $uses = [];
                $nsDepth = 1;
            }
            $modifiers = [];
            continue;
        }
        // An import: top level, not a closure's `use (...)` or a trait use.
        if ($t->is(T_USE) && $class === null && $depth <= $nsDepth) {
            $j = nextSignificant($tokens, $i);
            if ($j < $n && $tokens[$j]->text !== '(') {
                $i = readUse($tokens, $i, $uses);
                $attrTest = false;
                $doc = '';
                $modifiers = [];
                continue;
            }
        }
        // A trait use in a class or trait body: `use A, B;` or `use A { ... }`.
        if ($t->is(T_USE) && $class !== null && $depth === $classDepth + 1) {
            for ($i++; $i < $n && $tokens[$i]->text !== ';' && $tokens[$i]->text !== '{'; $i++) {
                if ($tokens[$i]->is([T_STRING, T_NAME_QUALIFIED, T_NAME_FULLY_QUALIFIED, T_NAME_RELATIVE])) {
                    $class['traits'][] = resolveClass($tokens[$i]->text, $namespace, $uses);
                }
            }
            if ($i < $n && $tokens[$i]->text === '{') { // skip the conflict rules
                for ($level = 1, $i++; $i < $n && $level > 0; $i++) {
                    if ($tokens[$i]->text === '{') {
                        $level++;
                    } elseif ($tokens[$i]->text === '}') {
                        $level--;
                    }
                }
                $i--;
            }
            $attrTest = false;
            $doc = '';
            $modifiers = [];
            continue;
        }
        if ($t->is([T_CLASS, T_TRAIT]) && $class === null) {
            $p = previousSignificant($tokens, $i);
            $j = nextSignificant($tokens, $i);
            // Skip `Foo::class` and anonymous `new class`.
            if (($p >= 0 && $tokens[$p]->is([T_DOUBLE_COLON, T_NEW])) || $j >= $n || !$tokens[$j]->is(T_STRING)) {
                $modifiers = [];
                continue;
            }
            $extends = null;
            $k = $j + 1;
            for (; $k < $n && $tokens[$k]->text !== '{'; $k++) {
                if ($tokens[$k]->is(T_EXTENDS)) {
                    $m = nextSignificant($tokens, $k);
                    if ($m < $n) {
                        $extends = resolveClass($tokens[$m]->text, $namespace, $uses);
                    }
                }
            }
            $class = [
                'short' => $tokens[$j]->text,
                'key' => resolveClass($tokens[$j]->text, $namespace, []),
                'file' => substr($path, strlen($dir) + 1),
                'emit' => $emit,
                // A trait never runs on its own, like an abstract class.
                'abstract' => $t->is(T_TRAIT) || in_array(T_ABSTRACT, $modifiers, true),
                'extends' => $extends,
                'traits' => [],
                'tests' => [],
            ];
            $classDepth = $depth;
            $modifiers = [];
            $i = $k - 1; // the class body's `{` is read next
            continue;
        }
        if ($t->is(T_FUNCTION) && $class !== null && $depth === $classDepth + 1) {
            $j = nextSignificant($tokens, $i);
            if ($j < $n && $tokens[$j]->text === '&') {
                $j = nextSignificant($tokens, $j);
            }
            $name = $j < $n ? $tokens[$j]->text : '';
            $public = !in_array(T_PROTECTED, $modifiers, true) && !in_array(T_PRIVATE, $modifiers, true);
            $abstract = in_array(T_ABSTRACT, $modifiers, true);
            $isTest = str_starts_with($name, 'test') || $attrTest || preg_match('/@test\b/', $doc) === 1;
            if ($public && !$abstract && $isTest) {
                $class['tests'][] = $name;
            }
            $attrTest = false;
            $doc = '';
            $modifiers = [];
            continue;
        }
        if ($t->text === ';' || $t->is([T_VARIABLE, T_CONST, T_CASE, T_USE])) {
            $attrTest = false;
            $doc = '';
            $modifiers = [];
        }
    }
}

/**
 * The test methods $record has from its traits and, through `extends`, its
 * parents and their traits, looked up by qualified name.
 */
function inheritedTests(array $record, array $classes, array &$seen, array &$warnings, string $file): array
{
    $tests = [];
    foreach ($record['traits'] as $trait) {
        if (isset($classes[$trait]) && !isset($seen[$trait])) {
            $seen[$trait] = true;
            $tests = array_merge($tests, $classes[$trait][0]['tests'], inheritedTests($classes[$trait][0], $classes, $seen, $warnings, $file));
        }
    }
    $parent = $record['extends'];
    if ($parent !== null && isset($classes[$parent]) && !isset($seen[$parent])) {
        $seen[$parent] = true;
        if (count($classes[$parent]) > 1) {
            $warnings[] = "ambiguous parent $parent of $file";
        }
        $tests = array_merge($tests, $classes[$parent][0]['tests'], inheritedTests($classes[$parent][0], $classes, $seen, $warnings, $file));
    }
    return $tests;
}

$rows = [];
$warnings = [];
foreach ($classes as $key => $records) {
    foreach ($records as $record) {
        if ($record['abstract'] || !$record['emit']) {
            continue;
        }
        $short = $record['short'];
        $seen = [$key => true];
        $tests = array_merge($record['tests'], inheritedTests($record, $classes, $seen, $warnings, $record['file']));
        foreach (array_unique($tests) as $method) {
            $rows[] = "{$record['file']}\t{$short}::{$method}";
        }
    }
}
$rows = array_unique($rows);
sort($rows, SORT_STRING);
foreach ($rows as $row) {
    echo $row, "\n";
}
foreach (array_unique($warnings) as $warning) {
    fwrite(STDERR, "warning: $warning\n");
}
