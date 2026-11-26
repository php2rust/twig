<?php
// Twig integration vectors (crates/twig-fixtures/tests/integration.rs):
// every case of twig/twig's tests/Fixtures/**/*.test (not the .legacy.test
// files), run as Twig\Test\IntegrationTestCase::doIntegrationTest() runs
// it, with the extensions, callbacks and test classes of
// Twig\Tests\IntegrationTest (rebuilt below). For each case (one per
// --DATA-- section): the template sources (with the case's spaces), the
// options of --CONFIG--, the context of --DATA-- as a typed tree, and what
// Twig did: the trimmed output, or the exception as "Class: message" when
// loading or rendering threw, and the deprecations of each step (Twig
// triggers the deprecations of the compiler while loading). Strings are
// base64.
// Output: crates/twig-fixtures/tests/data/integration.json
// (scripts/record.sh twig-integration).

namespace {
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
require_once '/work/upstream/twig/twig/src/Resources/core.php';
require_once '/work/upstream/twig/twig/src/Resources/debug.php';
require_once '/work/upstream/twig/twig/src/Resources/escaper.php';
require_once '/work/upstream/twig/twig/src/Resources/string_loader.php';
}

namespace Twig\Tests {

use Twig\DeprecatedCallableInfo;
use Twig\Environment;
use Twig\Error\SyntaxError;
use Twig\Extension\AbstractExtension;
use Twig\Extension\DebugExtension;
use Twig\Extension\SandboxExtension;
use Twig\Extension\StringLoaderExtension;
use Twig\Loader\ArrayLoader;
use Twig\Node\Expression\ConstantExpression;
use Twig\Node\Node;
use Twig\Node\PrintNode;
use Twig\Runtime\EscaperRuntime;
use Twig\Sandbox\SecurityPolicy;
use Twig\Token;
use Twig\TokenParser\AbstractTokenParser;
use Twig\TwigFilter;
use Twig\TwigFunction;
use Twig\TwigTest;

// The test classes of tests/IntegrationTest.php.
function test_foo($value = 'foo')
{
    return $value;
}

class TwigTestFoo implements \Iterator
{
    public const BAR_NAME = 'bar';

    public $position = 0;
    public $array = [1, 2];

    public static $foo = 'Foo';

    public function bar($param1 = null, $param2 = null)
    {
        return 'bar'.($param1 ? '_'.$param1 : '').($param2 ? '-'.$param2 : '');
    }

    public function getFoo()
    {
        return 'foo';
    }

    public function getEmpty()
    {
        return '';
    }

    public function getNull()
    {
        return null;
    }

    public function getSelf()
    {
        return $this;
    }

    public function is()
    {
        return 'is';
    }

    public function in()
    {
        return 'in';
    }

    public function not()
    {
        return 'not';
    }

    public function strToLower($value)
    {
        return strtolower($value);
    }

    public function rewind(): void
    {
        $this->position = 0;
    }

    #[\ReturnTypeWillChange]
    public function current()
    {
        return $this->array[$this->position];
    }

    #[\ReturnTypeWillChange]
    public function key()
    {
        return 'a';
    }

    public function next(): void
    {
        ++$this->position;
    }

    public function valid(): bool
    {
        return isset($this->array[$this->position]);
    }
}

class TwigTestTokenParser_§ extends AbstractTokenParser
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

class TwigTestExtension extends AbstractExtension
{
    public function getTokenParsers(): array
    {
        return [new TwigTestTokenParser_§()];
    }

    public function getFilters(): array
    {
        return [
            new TwigFilter('§', [$this, '§Filter']),
            new TwigFilter('escape_and_nl2br', [$this, 'escape_and_nl2br'], ['needs_environment' => true, 'is_safe' => ['html']]),
            new TwigFilter('nl2br', [$this, 'nl2br'], ['pre_escape' => 'html', 'is_safe' => ['html']]),
            new TwigFilter('escape_something', [$this, 'escape_something'], ['is_safe' => ['something']]),
            new TwigFilter('preserves_safety', [$this, 'preserves_safety'], ['preserves_safety' => ['html']]),
            new TwigFilter('static_call_string', 'Twig\\Tests\\TwigTestExtension::staticCall'),
            new TwigFilter('static_call_array', ['Twig\\Tests\\TwigTestExtension', 'staticCall']),
            new TwigFilter('magic_call', [$this, 'magicCall']),
            new TwigFilter('magic_call_closure', \Closure::fromCallable([$this, 'magicCall'])),
            new TwigFilter('magic_call_string', 'Twig\\Tests\\TwigTestExtension::magicStaticCall'),
            new TwigFilter('magic_call_array', ['Twig\\Tests\\TwigTestExtension', 'magicStaticCall']),
            new TwigFilter('*_path', [$this, 'dynamic_path']),
            new TwigFilter('*_foo_*_bar', [$this, 'dynamic_foo']),
            new TwigFilter('not', [$this, 'notFilter']),
            new TwigFilter('anon_foo', static function ($name) { return '*'.$name.'*'; }),
        ];
    }

    public function getFunctions(): array
    {
        return [
            new TwigFunction('§', [$this, '§Function']),
            new TwigFunction('safe_br', [$this, 'br'], ['is_safe' => ['html']]),
            new TwigFunction('unsafe_br', [$this, 'br']),
            new TwigFunction('static_call_string', 'Twig\\Tests\\TwigTestExtension::staticCall'),
            new TwigFunction('static_call_array', ['Twig\\Tests\\TwigTestExtension', 'staticCall']),
            new TwigFunction('*_path', [$this, 'dynamic_path']),
            new TwigFunction('*_foo_*_bar', [$this, 'dynamic_foo']),
            new TwigFunction('anon_foo', static function ($name) { return '*'.$name.'*'; }),
            new TwigFunction('deprecated_function', static function () { return 'foo'; }, ['deprecation_info' => new DeprecatedCallableInfo('foo/bar', '1.1', 'not_deprecated_function')]),
        ];
    }

    public function getTests(): array
    {
        return [
            new TwigTest('multi word', [$this, 'is_multi_word']),
            new TwigTest('test_*', [$this, 'dynamic_test']),
        ];
    }

    public function notFilter($value)
    {
        return 'not '.$value;
    }

    public function §Filter($value)
    {
        return "§{$value}§";
    }

    public function §Function($value)
    {
        return "§{$value}§";
    }

    public function escape_and_nl2br($env, $value, $sep = '<br />')
    {
        return $this->nl2br($env->getRuntime(EscaperRuntime::class)->escape($value, 'html'), $sep);
    }

    public function nl2br($value, $sep = '<br />')
    {
        return str_replace("\n", "$sep\n", $value ?? '');
    }

    public function dynamic_path($element, $item)
    {
        return $element.'/'.$item;
    }

    public function dynamic_foo($foo, $bar, $item)
    {
        return $foo.'/'.$bar.'/'.$item;
    }

    public function dynamic_test($element, $item)
    {
        return $element === $item;
    }

    public function escape_something($value)
    {
        return strtoupper($value);
    }

    public function preserves_safety($value)
    {
        return strtoupper($value);
    }

    public static function staticCall($value)
    {
        return "*$value*";
    }

    public function br()
    {
        return '<br />';
    }

    public function is_multi_word($value)
    {
        return str_contains($value, ' ');
    }

    public function __call($method, $arguments)
    {
        if ('magicCall' !== $method) {
            throw new \BadMethodCallException('Unexpected call to __call.');
        }

        return 'magic_'.$arguments[0];
    }

    public static function __callStatic($method, $arguments)
    {
        if ('magicStaticCall' !== $method) {
            throw new \BadMethodCallException('Unexpected call to __callStatic.');
        }

        return 'static_magic_'.$arguments[0];
    }
}

class MagicCallStub
{
    public function __call($name, $args): void
    {
        throw new \Exception('__call shall not be called.');
    }
}

class ToStringStub
{
    private $string;

    public function __construct($string)
    {
        $this->string = $string;
    }

    public function __toString()
    {
        return $this->string;
    }
}

class CountableStub implements \Countable
{
    private $count;

    public function __construct($count)
    {
        $this->count = $count;
    }

    public function count(): int
    {
        return $this->count;
    }

    public function __toString()
    {
        throw new \Exception('__toString shall not be called on \Countables.');
    }
}

class IteratorAggregateStub implements \IteratorAggregate
{
    private $data;

    public function __construct(array $data)
    {
        $this->data = $data;
    }

    public function getIterator(): \Traversable
    {
        return new \ArrayIterator($this->data);
    }
}

class SimpleIteratorForTesting implements \Iterator
{
    private $data = [1, 2, 3, 4, 5, 6, 7];
    private $key = 0;

    #[\ReturnTypeWillChange]
    public function current()
    {
        return $this->key;
    }

    public function next(): void
    {
        ++$this->key;
    }

    #[\ReturnTypeWillChange]
    public function key()
    {
        return $this->key;
    }

    public function valid(): bool
    {
        return isset($this->data[$this->key]);
    }

    public function rewind(): void
    {
        $this->key = 0;
    }

    public function __toString()
    {
        return str_repeat('X', iterator_count($this) + 10);
    }
}

}

namespace {
use Twig\Environment;
use Twig\Error\SyntaxError;
use Twig\Extension\DebugExtension;
use Twig\Extension\SandboxExtension;
use Twig\Extension\StringLoaderExtension;
use Twig\Loader\ArrayLoader;
use Twig\Sandbox\SecurityPolicy;
use Twig\Tests\TwigTestExtension;
use Twig\TwigTest;

// A value as a typed tree: ["n"], ["b", bool], ["i", int], ["f", repr],
// ["s", base64], ["a", [[key, value], ...]] with int keys or ["s", base64]
// keys, ["o", class] for an object (not read by the Rust tests yet).
function typed($value)
{
    if (null === $value) {
        return ['n'];
    }
    if (is_bool($value)) {
        return ['b', $value];
    }
    if (is_int($value)) {
        return ['i', $value];
    }
    if (is_float($value)) {
        return ['f', var_export($value, true)];
    }
    if (is_string($value)) {
        return ['s', base64_encode($value)];
    }
    if (is_array($value)) {
        $pairs = [];
        foreach ($value as $k => $v) {
            $pairs[] = [is_int($k) ? $k : ['s', base64_encode($k)], typed($v)];
        }

        return ['a', $pairs];
    }

    return ['o', is_object($value) ? get_class($value) : get_debug_type($value)];
}

function make_env(array $templates, array $config): Environment
{
    $twig = new Environment(new ArrayLoader($templates), $config);
    $twig->addGlobal('global', 'global');
    $twig->addExtension(new DebugExtension());
    $twig->addExtension(new SandboxExtension(new SecurityPolicy([], [], [], [], ['dump']), false));
    $twig->addExtension(new StringLoaderExtension());
    $twig->addExtension(new TwigTestExtension());
    $twig->registerUndefinedFunctionCallback(static function (string $name) {
        if ('throwing_undefined_function' === $name) {
            throw new SyntaxError('This function is undefined in the tests.');
        }

        return false;
    });
    $twig->registerUndefinedTestCallback(static function (string $name) {
        if ('throwing_undefined_test' === $name) {
            throw new SyntaxError('This test is undefined in the tests.');
        }
        if ('throwing_undefined_two words_test' === $name) {
            throw new SyntaxError('This test is undefined in the tests.');
        }
        if (\in_array($name, ['divisible', 'same'], true)) {
            return new TwigTest($name, static fn () => '');
        }

        return false;
    });
    $twig->registerUndefinedFilterCallback(static function (string $name) {
        if ('throwing_undefined_filter' === $name) {
            throw new SyntaxError('This filter is undefined in the tests.');
        }

        return false;
    });

    return $twig;
}

function parse_templates(string $test): array
{
    $templates = [];
    preg_match_all('/--TEMPLATE(?:\((.*?)\))?--(.*?)(?=\-\-TEMPLATE|$)/s', $test, $matches, \PREG_SET_ORDER);
    foreach ($matches as $match) {
        $templates[$match[1] ?: 'index.twig'] = $match[2];
    }

    return $templates;
}

$fixtures = '/work/upstream/twig/twig/tests/Fixtures';
$files = [];
foreach (new RecursiveIteratorIterator(new RecursiveDirectoryIterator($fixtures, FilesystemIterator::SKIP_DOTS)) as $file) {
    $path = $file->getPathname();
    if (str_ends_with($path, '.test') && !str_contains($path, '.legacy.test')) {
        $files[] = $path;
    }
}
sort($files);

$out = [];
foreach ($files as $path) {
    $relative = substr($path, strlen($fixtures) + 1);
    $test = file_get_contents($path);
    // IntegrationTestCase::assembleTests()
    if (preg_match('/--TEST--\s*(.*?)\s*(?:--CONDITION--\s*(.*))?\s*(?:--DEPRECATION--\s*(.*?))?\s*((?:--TEMPLATE(?:\(.*?\))?--(?:.*?))+)\s*(?:--DATA--\s*(.*))?\s*--EXCEPTION--\s*(.*)/sx', $test, $match)) {
        $condition = $match[2];
        $deprecation = $match[3];
        $sources = parse_templates($match[4]);
        $exception = $match[6];
        $outputs = [[null, $match[5], null, '']];
    } elseif (preg_match('/--TEST--\s*(.*?)\s*(?:--CONDITION--\s*(.*))?\s*(?:--DEPRECATION--\s*(.*?))?\s*((?:--TEMPLATE(?:\(.*?\))?--(?:.*?))+)--DATA--.*?--EXPECT--.*/s', $test, $match)) {
        $condition = $match[2];
        $deprecation = $match[3];
        $sources = parse_templates($match[4]);
        $exception = false;
        preg_match_all('/--DATA--(.*?)(?:--CONFIG--(.*?))?--EXPECT--(.*?)(?=\-\-DATA\-\-|$)/s', $test, $outputs, \PREG_SET_ORDER);
    } else {
        continue;
    }
    if ($condition) {
        $ret = '';
        eval('$ret = '.$condition.';');
        if (!$ret) {
            $out[] = ['file' => $relative, 'skipped' => $condition];
            continue;
        }
    }
    foreach ($outputs as $i => $case) {
        $config = array_merge(['cache' => false, 'strict_variables' => true], ($case[2] ?? null) ? eval($case[2].';') : []);
        $templates = [];
        foreach ($sources as $name => $source) {
            $templates[$name] = $source.str_repeat(' ', $i);
        }
        $record = [
            'file' => $relative,
            'case' => $i,
            'templates' => array_map(fn ($name) => [base64_encode($name), base64_encode($templates[$name])], array_keys($templates)),
            'config' => array_map('typed', array_diff_key($config, ['cache' => 1])),
            'exception' => false !== $exception,
            'deprecation' => base64_encode($deprecation),
        ];
        $deprecations = [];
        set_error_handler(static function ($type, $msg) use (&$deprecations) {
            if (\E_USER_DEPRECATED === $type) {
                $deprecations[] = $msg;

                return true;
            }

            return false;
        });
        try {
            $twig = make_env($templates, $config);
            $loaded = [];
            foreach (array_keys($templates) as $name) {
                $loaded[$name] = $twig->load($name);
            }
            $record['load_deprecations'] = array_map('base64_encode', $deprecations);
            $deprecations = [];
            $data = eval($case[1].';');
            $record['data'] = typed($data);
            try {
                $record['output'] = base64_encode(trim($loaded['index.twig']->render($data), "\n "));
            } catch (\Exception $e) {
                $record['render_error'] = base64_encode(sprintf('%s: %s', $e::class, $e->getMessage()));
            }
        } catch (\Exception $e) {
            $record['load_error'] = base64_encode(sprintf('%s: %s', $e::class, $e->getMessage()));
        } finally {
            restore_error_handler();
        }
        $record[isset($record['load_deprecations']) ? 'render_deprecations' : 'load_deprecations'] = array_map('base64_encode', $deprecations);
        $out[] = $record;
    }
}

echo "[\n", implode(",\n", array_map(fn ($r) => json_encode($r, \JSON_UNESCAPED_SLASHES | \JSON_THROW_ON_ERROR), $out)), "\n]\n";
}
