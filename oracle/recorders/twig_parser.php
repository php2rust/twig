<?php
// Twig parser vectors (crates/twig/tests/parser_vectors.rs): every template
// of twig/twig's tests/Fixtures/**/*.test (split as IntegrationTestCase
// does), plus the cases in crates/twig/tests/data/parser_extra.json, each
// parsed by the real Twig\Parser with the node visitors turned off. The
// environment has the extensions of Twig\Tests\IntegrationTest (debug,
// sandbox, string loader and the test extension, rebuilt below without its
// runtime code; no undefined-callable callbacks). For each template: the
// ModuleNode as a compact dump (dump_node() below), or the error (class,
// message, line), and the deprecations parsing triggered. Templates,
// messages and deprecations are base64; a tree is text, or base64 when it
// is not UTF-8. Output: crates/twig/tests/data/parser.json
// (scripts/record.sh twig-parser).

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

use Twig\DeprecatedCallableInfo;
use Twig\Environment;
use Twig\Extension\AbstractExtension;
use Twig\Extension\DebugExtension;
use Twig\Extension\SandboxExtension;
use Twig\Extension\StringLoaderExtension;
use Twig\Loader\ArrayLoader;
use Twig\Node\Expression\ConstantExpression;
use Twig\Node\Node;
use Twig\Node\PrintNode;
use Twig\Parser;
use Twig\Sandbox\SecurityPolicy;
use Twig\Source;
use Twig\Token;
use Twig\TokenParser\AbstractTokenParser;
use Twig\TwigFilter;
use Twig\TwigFunction;
use Twig\TwigTest;

final class FixtureTokenParser extends AbstractTokenParser
{
    public function parse(Token $token): Node
    {
        $this->parser->getStream()->expect(Token::BLOCK_END_TYPE);

        return new PrintNode(new ConstantExpression('§', -1), -1);
    }

    public function getTag(): string
    {
        return '§';
    }
}

// Twig\Tests\TwigTestExtension's definitions; the callables are never run.
final class FixtureExtension extends AbstractExtension
{
    public function getTokenParsers(): array
    {
        return [new FixtureTokenParser()];
    }

    public function getFilters(): array
    {
        $f = static fn () => null;

        return [
            new TwigFilter('§', $f),
            new TwigFilter('escape_and_nl2br', $f, ['needs_environment' => true, 'is_safe' => ['html']]),
            new TwigFilter('nl2br', $f, ['pre_escape' => 'html', 'is_safe' => ['html']]),
            new TwigFilter('escape_something', $f, ['is_safe' => ['something']]),
            new TwigFilter('preserves_safety', $f, ['preserves_safety' => ['html']]),
            new TwigFilter('static_call_string', $f),
            new TwigFilter('static_call_array', $f),
            new TwigFilter('magic_call', $f),
            new TwigFilter('magic_call_closure', $f),
            new TwigFilter('magic_call_string', $f),
            new TwigFilter('magic_call_array', $f),
            new TwigFilter('*_path', $f),
            new TwigFilter('*_foo_*_bar', $f),
            new TwigFilter('not', $f),
            new TwigFilter('anon_foo', $f),
        ];
    }

    public function getFunctions(): array
    {
        $f = static fn () => null;

        return [
            new TwigFunction('§', $f),
            new TwigFunction('safe_br', $f, ['is_safe' => ['html']]),
            new TwigFunction('unsafe_br', $f),
            new TwigFunction('static_call_string', $f),
            new TwigFunction('static_call_array', $f),
            new TwigFunction('*_path', $f),
            new TwigFunction('*_foo_*_bar', $f),
            new TwigFunction('anon_foo', $f),
            new TwigFunction('deprecated_function', $f, ['deprecation_info' => new DeprecatedCallableInfo('foo/bar', '1.1', 'not_deprecated_function')]),
        ];
    }

    public function getTests(): array
    {
        $f = static fn () => null;

        return [
            new TwigTest('multi word', $f),
            new TwigTest('test_*', $f),
        ];
    }
}

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

// A compact tree dump, one line per node in tree order: depth, key, class
// (without "Twig\Node\"), line, then the tag, documentation, attributes
// (var_export(), newlines as \n, callables as __toString()), the private
// state __toString() leaves out (defined test, call parentheses) and the
// source name where it differs from the parent's. A node in an attribute
// (embedded_templates) follows the children, with the key "@name".
function export_value($value): string
{
    if ($value instanceof Node) {
        return 'node';
    }
    if ($value instanceof \Stringable) {
        return (string) $value;
    }

    return str_replace("\n", '\n', var_export($value, true));
}

function dump_node(Node $node, string $key, int $depth, ?string $parentSource, array &$lines): void
{
    $line = $depth.' '.$key.' '.str_replace('Twig\\Node\\', '', get_class($node)).' '.$node->getTemplateLine();
    if ($node->getNodeTag()) {
        $line .= ' tag='.$node->getNodeTag();
    }
    if (null !== $node->getDocumentation()) {
        $line .= ' doc='.export_value($node->getDocumentation());
    }
    $nested = [];
    $r = new ReflectionProperty(Node::class, 'attributes');
    foreach ($r->getValue($node) as $name => $value) {
        $line .= ' '.$name.'='.export_value($value);
        if ($value instanceof Node) {
            $nested[$name] = $value;
        }
    }
    for ($class = new ReflectionClass($node); $class; $class = $class->getParentClass()) {
        if ($class->hasProperty('definedTest') && $class->getProperty('definedTest')->getDeclaringClass()->getName() === $class->getName()) {
            if ($class->getProperty('definedTest')->getValue($node)) {
                $line .= ' +defined';
            }
            break;
        }
    }
    if (method_exists($node, 'hasCallParentheses') && !$node->hasCallParentheses()) {
        $line .= ' -parentheses';
    }
    $source = $node->getSourceContext()?->getName();
    if ($source !== $parentSource) {
        $line .= ' src='.($source ?? '-');
    }
    $lines[] = $line;
    foreach ($node as $k => $child) {
        dump_node($child, (string) $k, $depth + 1, $source, $lines);
    }
    foreach ($nested as $k => $child) {
        dump_node($child, '@'.$k, $depth + 1, $source, $lines);
    }
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
        $cases[] = ['name' => $relative.' '.$name, 'source' => $name, 'template' => $code];
    }
}
foreach (json_decode(file_get_contents('/work/crates/twig/tests/data/parser_extra.json'), true) as $name => $code) {
    $cases[] = ['name' => 'extra '.$name, 'source' => 'index.twig', 'template' => $code];
}

$env = new Environment(new ArrayLoader());
$env->addExtension(new DebugExtension());
$env->addExtension(new SandboxExtension(new SecurityPolicy([], [], [], [], ['dump']), false));
$env->addExtension(new StringLoaderExtension());
$env->addExtension(new FixtureExtension());

$out = [];
foreach ($cases as $case) {
    $deprecations = [];
    set_error_handler(function (int $type, string $message) use (&$deprecations): bool {
        $deprecations[] = $message;

        return true;
    });
    $record = ['name' => $case['name'], 'source' => $case['source'], 'template' => base64_encode($case['template'])];
    try {
        $stream = $env->tokenize(new Source($case['template'], $case['source']));
        $parser = new Parser($env);
        (new ReflectionProperty($parser, 'visitors'))->setValue($parser, []);
        $module = $parser->parse($stream);
        $lines = [];
        dump_node($module, '', 0, null, $lines);
        $tree = implode("\n", $lines);
        $record['tree'] = mb_check_encoding($tree, 'UTF-8') ? $tree : ['base64', base64_encode($tree)];
    } catch (\Throwable $e) {
        $record['error'] = [get_class($e), base64_encode($e->getMessage()), method_exists($e, 'getTemplateLine') ? $e->getTemplateLine() : null];
    } finally {
        restore_error_handler();
    }
    $record['deprecations'] = array_map('base64_encode', $deprecations);
    $out[] = $record;
}

echo "[\n", implode(",\n", array_map(fn ($r) => json_encode($r, \JSON_UNESCAPED_SLASHES | \JSON_THROW_ON_ERROR), $out)), "\n]\n";
