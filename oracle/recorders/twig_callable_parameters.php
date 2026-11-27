<?php
// The PHP parameters of every filter, function and test of an environment
// with the core, escaper, debug, string loader and sandbox extensions, as
// Twig\Util\CallableParameters sees them for the sandbox
// (crates/php2rust-twig/tests/data/callable_parameters.json, scripts/record.sh
// twig-callable-parameters): for each callable its type, name and PHP
// callable, and its template-level parameters
// (ReflectionCallable::getTwigParameters()) with the name, whether it is
// variadic and CallableParameters::isStringCoercionSafe() of its type.

if (is_file('/demo/vendor/autoload.php')) {
    require '/demo/vendor/autoload.php';
} else {
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

use Twig\Environment;
use Twig\Extension\DebugExtension;
use Twig\Extension\SandboxExtension;
use Twig\Extension\StringLoaderExtension;
use Twig\Loader\ArrayLoader;
use Twig\Sandbox\SecurityPolicy;
use Twig\Util\CallableParameters;
use Twig\Util\ReflectionCallable;

$env = new Environment(new ArrayLoader());
$env->addExtension(new DebugExtension());
$env->addExtension(new StringLoaderExtension());
$env->addExtension(new SandboxExtension(new SecurityPolicy()));

function describe_callable($callable): ?string
{
    if (null === $callable) {
        return null;
    }
    if (is_string($callable)) {
        return $callable;
    }
    if (is_array($callable)) {
        return (is_object($callable[0]) ? get_class($callable[0]) : $callable[0]).'::'.$callable[1];
    }

    return get_debug_type($callable);
}

$out = [];
foreach (['filter' => $env->getFilters(), 'function' => $env->getFunctions(), 'test' => $env->getTests()] as $type => $callables) {
    foreach ($callables as $name => $callable) {
        $row = ['type' => $type, 'name' => $name, 'callable' => describe_callable($callable->getCallable())];
        try {
            $params = null === $callable->getCallable() ? null : (new ReflectionCallable($callable))->getTwigParameters();
        } catch (\LogicException) {
            $params = null;
        }
        $row['parameters'] = null === $params ? null : array_map(static fn (\ReflectionParameter $p) => [
            $p->getName(),
            $p->isVariadic(),
            CallableParameters::isStringCoercionSafe($p->getType(), $p->getDeclaringClass()),
        ], $params);
        $out[] = $row;
    }
}

echo json_encode($out, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR), "\n";
