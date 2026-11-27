<?php
// Twig lexer vectors (crates/php2rust-twig/tests/lexer_vectors.rs): every template
// of twig/twig's tests/Fixtures/**/*.test (split as IntegrationTestCase
// does), plus the cases in crates/php2rust-twig/tests/data/lexer_extra.json, each
// tokenized by the real Twig\Lexer with the core extension's operators.
// For each template: the tokens (type, value, line, offset,
// documentation) or the SyntaxError (message, line, column), and the
// deprecations the lexer triggered. Bytes are base64 (templates need not
// be UTF-8). Output: crates/php2rust-twig/tests/data/lexer.json
// (scripts/record.sh twig-lexer).

if (is_file('/demo/vendor/autoload.php')) {
    require '/demo/vendor/autoload.php';
} else {
    // Without the demo's vendor/: the pinned checkouts in upstream/.
    require '/work/upstream/symfony/deprecation-contracts/function.php';
    spl_autoload_register(function (string $class): void {
        if (str_starts_with($class, 'Twig\\')) {
            $file = '/work/upstream/twig/twig/src/'.str_replace('\\', '/', substr($class, 5)).'.php';
            if (is_file($file)) {
                require $file;
            }
        }
    });
}

use Twig\Environment;
use Twig\Error\SyntaxError;
use Twig\Lexer;
use Twig\Loader\ArrayLoader;
use Twig\Source;

$fixtures = '/work/upstream/twig/twig/tests/Fixtures';

// IntegrationTestCase::getTests() and parseTemplates()
function templates_of(string $test): array
{
    if (preg_match('/--TEST--\s*(.*?)\s*(?:--CONDITION--\s*(.*))?\s*(?:--DEPRECATION--\s*(.*?))?\s*((?:--TEMPLATE(?:\(.*?\))?--(?:.*?))+)\s*(?:--DATA--\s*(.*))?\s*--EXCEPTION--\s*(.*)/sx', $test, $match)) {
        $templates = $match[4];
    } elseif (preg_match('/--TEST--\s*(.*?)\s*(?:--CONDITION--\s*(.*))?\s*(?:--DEPRECATION--\s*(.*?))?\s*((?:--TEMPLATE(?:\(.*?\))?--(?:.*?))+)--DATA--.*?--EXPECT--.*/s', $test, $match)) {
        $templates = $match[4];
    } else {
        return [];
    }
    $result = [];
    preg_match_all('/--TEMPLATE(?:\((.*?)\))?--(.*?)(?=\-\-TEMPLATE|$)/s', $templates, $matches, \PREG_SET_ORDER);
    foreach ($matches as $m) {
        $result[$m[1] ?: 'index.twig'] = $m[2];
    }

    return $result;
}

$cases = [];
$it = new RecursiveIteratorIterator(new RecursiveDirectoryIterator($fixtures, FilesystemIterator::SKIP_DOTS));
$files = [];
foreach ($it as $file) {
    if (str_ends_with($file->getFilename(), '.test')) {
        $files[] = $file->getPathname();
    }
}
sort($files);
foreach ($files as $path) {
    $relative = substr($path, strlen($fixtures) + 1);
    foreach (templates_of(file_get_contents($path)) as $name => $code) {
        $cases[] = ['name' => $relative.' '.$name, 'template' => $code];
    }
}
foreach (json_decode(file_get_contents('/work/crates/php2rust-twig/tests/data/lexer_extra.json'), true) as $name => $code) {
    $cases[] = ['name' => 'extra '.$name, 'template' => $code];
}

$lexer = new Lexer(new Environment(new ArrayLoader()));
$out = [];
foreach ($cases as $case) {
    $deprecations = [];
    set_error_handler(function (int $type, string $message) use (&$deprecations): bool {
        $deprecations[] = $message;

        return true;
    });
    $record = ['name' => $case['name'], 'template' => base64_encode($case['template'])];
    try {
        $stream = $lexer->tokenize(new Source($case['template'], 'index'));
        $tokens = [];
        $r = new ReflectionProperty($stream, 'tokens');
        foreach ($r->getValue($stream) as $token) {
            $type = (new ReflectionProperty($token, 'type'))->getValue($token);
            $value = $token->getValue();
            $tokens[] = [
                $type,
                \is_string($value) ? base64_encode($value) : (\is_int($value) ? ['int', $value] : ['float', bin2hex(pack('E', $value))]),
                $token->getLine(),
                $token->getOffset(),
                null === $token->getDocumentation() ? null : base64_encode($token->getDocumentation()),
            ];
        }
        $record['tokens'] = $tokens;
    } catch (SyntaxError $e) {
        $record['error'] = [base64_encode($e->getMessage()), $e->getTemplateLine(), $e->getTemplateColumn()];
    } finally {
        restore_error_handler();
    }
    $record['deprecations'] = array_map('base64_encode', $deprecations);
    $out[] = $record;
}

echo "[\n", implode(",\n", array_map(fn ($r) => json_encode($r, \JSON_UNESCAPED_SLASHES | \JSON_THROW_ON_ERROR), $out)), "\n]\n";
