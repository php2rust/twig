<?php
// Lists the PHP source files of one upstream package, with the classes they
// declare and their size, using PHP's own tokenizer (nothing is loaded or
// executed).
//
//   git -C upstream/symfony/routing ls-files | php oracle/classes.php upstream/symfony/routing
//
// Reads the paths to list from standard input, one per line, relative to
// the package (scripts/classes.py gives the files of the package's dist
// archive, without its tests); paths that do not end in .php are skipped.
// Prints one row per file: <path>\t<classes>\t<code lines>. <classes> is
// the short names of the classes, interfaces, traits and enums the file
// declares, separated by spaces, or `-` for a file that declares none
// (functions, config, data); bytes of a name that is not UTF-8 are written
// as \xNN. <code lines> counts the lines that hold a token other than
// whitespace, a comment, a docblock or the open tag; a token over several
// lines (a heredoc, a long string) counts every line it covers.

$dir = rtrim($argv[1] ?? '', '/');
if ($dir === '' || !is_dir($dir)) {
    fwrite(STDERR, "usage: php classes.php PACKAGE_DIR\n");
    exit(2);
}

$paths = [];
foreach (explode("\n", stream_get_contents(STDIN) ?: '') as $path) {
    $path = rtrim($path, "\r");
    if ($path !== '' && str_ends_with($path, '.php')) {
        $paths[] = $path;
    }
}
sort($paths, SORT_STRING);

$ignored = [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT, T_OPEN_TAG, T_INLINE_HTML];
$declares = [T_CLASS, T_INTERFACE, T_TRAIT, T_ENUM];

foreach ($paths as $path) {
    $source = file_get_contents("$dir/$path");
    if ($source === false) {
        fwrite(STDERR, "cannot read $dir/$path\n");
        exit(1);
    }
    $tokens = token_get_all($source);
    $lines = [];
    $classes = [];
    $previous = null; // last significant token
    $current = 1;     // line where the last token ends
    $count = count($tokens);
    for ($i = 0; $i < $count; $i++) {
        $token = $tokens[$i];
        if (is_string($token)) {
            // A single-character token has no line of its own: it is on
            // the line where the previous token ends.
            $lines[$current] = true;
            $previous = $token;
            continue;
        }
        [$id, $text, $line] = $token;
        $span = substr_count($text, "\n");
        $current = $line + $span;
        if (in_array($id, $ignored, true)) {
            continue;
        }
        for ($l = $line; $l <= $line + $span; $l++) {
            $lines[$l] = true;
        }
        if (in_array($id, $declares, true)) {
            $isConstant = is_array($previous) && $previous[0] === T_DOUBLE_COLON; // Foo::class
            $isAnonymous = is_array($previous) && $previous[0] === T_NEW;        // new class
            if (!$isConstant && !$isAnonymous) {
                for ($j = $i + 1; $j < $count; $j++) {
                    $next = $tokens[$j];
                    if (is_array($next) && in_array($next[0], $ignored, true)) {
                        continue;
                    }
                    if (is_array($next) && $next[0] === T_STRING) {
                        // A name that is not UTF-8 (symfony/cache's \xA9
                        // class) is written with \x escapes.
                        $classes[] = preg_match('//u', $next[1]) === 1 ? $next[1]
                            : preg_replace_callback('/[\x80-\xFF]/', fn (array $m): string => sprintf('\\x%02X', ord($m[0])), $next[1]);
                    }
                    break;
                }
            }
        }
        $previous = $token;
    }
    $names = $classes === [] ? '-' : implode(' ', $classes);
    echo $path, "\t", $names, "\t", count($lines), "\n";
}
