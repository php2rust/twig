//! `Twig\Tests\Extension\SandboxTest`: the deprecated stateful sandbox
//! (`SandboxExtension` with a policy, `sandboxed` and a source policy).
//! The templates of each test are compiled by the build script
//! (`legacy_sandbox_sets.rs`, from `tests/data/legacy_sandbox_templates.json`),
//! with the callables and tags the tests add
//! (`twig_integration::legacy_sandbox_extension`). `createTemplate()`
//! loads a string compiled with its set.
//!
//! The deprecations that PHP triggers while it compiles (the `sandbox`
//! tag, a token parser without `isAlwaysAllowedInSandbox()`) are
//! triggered by the build script here, so the tests do not see them.
//!
//! Upstream: twig/twig tests/Extension/SandboxTest.php

#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]
#![allow(
    clippy::struct_excessive_bools,
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::type_complexity,
    reason = "test helpers shaped as the PHP test's arguments and data providers"
)]
#![allow(clippy::approx_constant, reason = "3.14 is the PHP test's value")]

use std::{rc::Rc, sync::Arc};

use php_value::{
    PhpArray,
    object::{ArrayIterator, ArrayObject},
    value::{PhpObject, Thrown, Value},
};
use symfony_deprecation_contracts::collect;
use twig::{
    Autoescape, Environment, EnvironmentOptions,
    error::{Error, ErrorKind},
    extension::{Extension, sandbox::SandboxExtension, string_loader::StringLoaderExtension},
    loader::{CompiledArrayLoader, CompiledEntry},
    sandbox::{SecurityPolicy, SecurityPolicyInterface, SourcePolicyInterface},
    source::Source,
};
use twig_integration::{
    legacy_sandbox::{
        ArrayLikeObject, ChildClass, ColumnObject, CyclicTraversableObject, FooObject,
        MagicCallObject, MagicObject, StringablePlainObject, StringableTraversableObject,
        unsafe_test_calls,
    },
    legacy_sandbox_extension::LegacySandboxTestExtension,
};

mod compiled {
    include!(concat!(env!("OUT_DIR"), "/legacy_sandbox_sets.rs"));
}

const FOO: &str = FooObject::CLASS;
const SOURCE_POLICY_DEPRECATION: &str = "Since twig/twig 3.27.0: The \"Twig\\Sandbox\\SourcePolicyInterface\" interface is deprecated with no replacement, do not pass an instance to \"Twig\\Extension\\SandboxExtension\".";

/// The compiled templates of the set `name`.
fn templates(name: &str) -> &'static [CompiledEntry] {
    let (_, set) = compiled::LEGACY_SANDBOX_SETS
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no template set \"{name}\""));
    match set {
        Ok(templates) => templates,
        Err(e) => panic!(
            "set \"{name}\" did not compile: {}",
            String::from_utf8_lossy(e)
        ),
    }
}

/// The arguments of `getEnvironment()` after `$templates`, and the
/// environment options the tests give.
#[derive(Default)]
struct Cfg<'a> {
    sandboxed: bool,
    tags: &'a [&'a str],
    filters: &'a [&'a str],
    methods: &'a [(&'a str, &'a [&'a str])],
    properties: &'a [(&'a str, &'a [&'a str])],
    functions: &'a [&'a str],
    source_policy: Option<Rc<dyn SourcePolicyInterface>>,
    strict: bool,
    tests: &'a [&'a str],
    /// `['strict_variables' => true]`.
    strict_variables: bool,
    /// `['autoescape' => 'html']` (also the set's).
    html: bool,
    /// The set's callables (see `LegacySandboxTestExtension`).
    variant: &'a str,
    /// An environment without the sandbox extension.
    no_sandbox_extension: bool,
}

fn sandboxed<'a>() -> Cfg<'a> {
    Cfg {
        sandboxed: true,
        ..Cfg::default()
    }
}

/// `$this->getEnvironment(...)` with the templates of the set `set`.
fn env(set: &str, cfg: Cfg<'_>) -> Environment {
    let policy = SecurityPolicy::new(
        cfg.tags,
        cfg.filters,
        cfg.methods,
        cfg.properties,
        cfg.functions,
        cfg.tests,
    );
    policy.set_strict(cfg.strict);
    env_with(set, &cfg, Rc::new(policy))
}

fn env_with(set: &str, cfg: &Cfg<'_>, policy: Rc<dyn SecurityPolicyInterface>) -> Environment {
    let mut extensions: Vec<Rc<dyn Extension>> = vec![
        Rc::new(LegacySandboxTestExtension {
            variant: cfg.variant.to_owned(),
        }),
        Rc::new(StringLoaderExtension),
    ];
    if !cfg.no_sandbox_extension {
        extensions.push(Rc::new(SandboxExtension::new(
            policy,
            cfg.sandboxed,
            cfg.source_policy.clone(),
        )));
    }
    let options = EnvironmentOptions {
        debug: true,
        autoescape: if cfg.html {
            Autoescape::Strategy(b"html".to_vec())
        } else {
            Autoescape::Off
        },
        strict_variables: cfg.strict_variables,
        ..EnvironmentOptions::default()
    };
    let env = Environment::with_options(extensions, options).unwrap();
    env.set_loader(Rc::new(CompiledArrayLoader {
        templates: templates(set),
    }));
    env
}

/// `$twig->getExtension(SandboxExtension::class)`.
fn sandbox(env: &Environment) -> &SandboxExtension {
    env.sandbox().expect("the sandbox extension")
}

/// The policy of the environment's sandbox extension, as the tests'
/// `getSecurityPolicy()` gives it.
fn policy_of(env: &Environment) -> Rc<dyn SecurityPolicyInterface> {
    sandbox(env).security_policy()
}

fn with_policy(env: &Environment, f: impl FnOnce(&SecurityPolicy)) {
    let policy = policy_of(env);
    f(policy.as_security_policy().expect("a SecurityPolicy"));
}

fn s(text: &str) -> Value {
    Value::from(text)
}

fn object(object: impl PhpObject) -> Value {
    Value::Object(Arc::new(object))
}

fn array(values: Vec<Value>) -> Value {
    let mut array = PhpArray::new();
    for value in values {
        array.push(value).unwrap();
    }
    Value::Array(array)
}

fn map(entries: Vec<(&str, Value)>) -> PhpArray {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(key, value);
    }
    array
}

fn iterator(values: Vec<Value>) -> Value {
    let Value::Array(storage) = array(values) else {
        unreachable!()
    };
    object(ArrayIterator::new(storage))
}

/// `self::$params`. PHP's `recursion` holds a reference to itself; a
/// Rust array is a value, so it holds a copy of `[4]` here.
fn params() -> PhpArray {
    let iterator_map = object(ArrayIterator::new(map(vec![(
        "__toString",
        FooObject::value(),
    )])));
    map(vec![
        ("name", s("Fabien")),
        ("obj", FooObject::value()),
        ("arr", Value::Array(map(vec![("obj", FooObject::value())]))),
        ("child_obj", object(ChildClass)),
        (
            "some_array",
            array(vec![
                Value::Int(5),
                Value::Int(6),
                Value::Int(7),
                FooObject::value(),
            ]),
        ),
        ("array_like", object(ArrayLikeObject)),
        ("magic", object(MagicObject)),
        (
            "recursion",
            array(vec![
                Value::Int(4),
                array(vec![Value::Int(4)]),
                FooObject::value(),
            ]),
        ),
        ("iterator", iterator(vec![s("a"), FooObject::value()])),
        ("iterator_map", iterator_map),
        (
            "iterator_nested",
            iterator(vec![s("a"), iterator(vec![s("b"), FooObject::value()])]),
        ),
        (
            "stringable_iterator",
            StringableTraversableObject::value(seq(vec![s("a"), FooObject::value()])),
        ),
        (
            "stringable_iterator_map",
            StringableTraversableObject::value(map(vec![("__toString", FooObject::value())])),
        ),
    ])
}

fn seq(values: Vec<Value>) -> PhpArray {
    let Value::Array(array) = array(values) else {
        unreachable!()
    };
    array
}

fn render(env: &Environment, name: &str, context: PhpArray) -> Result<String, Error> {
    env.load(name.as_bytes())?
        .render(env, context)
        .map(|out| String::from_utf8(out).unwrap())
}

/// `$twig->createTemplate($template, 'index')->render([])`.
fn render_string(env: &Environment, template: &str) -> Result<String, Error> {
    env.create_template(template.as_bytes(), Some(b"index"))?
        .render(env, PhpArray::new())
        .map(|out| String::from_utf8(out).unwrap())
}

fn message(error: &Error) -> String {
    String::from_utf8_lossy(error.message()).into_owned()
}

#[track_caller]
fn assert_kind(result: Result<impl std::fmt::Debug, Error>, kind: ErrorKind) -> Error {
    let error = result.expect_err("an error");
    assert_eq!(error.kind(), kind, "{error}");
    error
}

#[track_caller]
fn assert_error(result: Result<impl std::fmt::Debug, Error>, kind: ErrorKind, text: &str) {
    let error = assert_kind(result, kind);
    assert!(
        message(&error).contains(text),
        "\"{}\" does not contain \"{text}\"",
        message(&error)
    );
}

/// A `SecurityNotAllowed{Method,Property}Error` on `class`, member `name`.
#[track_caller]
fn assert_member(
    result: Result<impl std::fmt::Debug, Error>,
    kind: ErrorKind,
    class: &str,
    name: &str,
) {
    let error = assert_kind(result, kind);
    assert_eq!(error.security_class_name(), Some(class), "{error}");
    assert_eq!(error.security_name(), Some(name.as_bytes()), "{error}");
}

#[track_caller]
fn assert_to_string_refused(result: Result<impl std::fmt::Debug, Error>) {
    assert_member(
        result,
        ErrorKind::SecurityNotAllowedMethod,
        FOO,
        "__tostring",
    );
}

/// A `SecurityNotAllowed{Tag,Filter,Function,Test}Error` for `name`.
#[track_caller]
fn assert_refused(result: Result<impl std::fmt::Debug, Error>, kind: ErrorKind, name: &str) {
    let error = assert_kind(result, kind);
    assert_eq!(error.security_name(), Some(name.as_bytes()), "{error}");
}

#[track_caller]
fn assert_deprecations(
    deprecations: &[symfony_deprecation_contracts::Deprecation],
    expected: &str,
) {
    assert!(
        deprecations
            .iter()
            .any(|d| d.message == expected.as_bytes()),
        "no deprecation \"{expected}\" in {:?}",
        deprecations
            .iter()
            .map(|d| String::from_utf8_lossy(&d.message).into_owned())
            .collect::<Vec<_>>()
    );
}

/// The anonymous `SourcePolicyInterface` classes: the sandbox is on for
/// the templates `enable` accepts.
struct SourcePolicy(fn(&str) -> bool);

impl SourcePolicyInterface for SourcePolicy {
    fn enable_sandbox(&self, source: &Source) -> bool {
        (self.0)(source.name())
    }
}

fn source_policy(enable: fn(&str) -> bool) -> Option<Rc<dyn SourcePolicyInterface>> {
    Some(Rc::new(SourcePolicy(enable)))
}

fn always(_: &str) -> bool {
    true
}

fn never(_: &str) -> bool {
    false
}

const CORE_TAGS: &[(&str, &str)] = &[
    ("apply", "{% apply upper %}foo{% endapply %}"),
    ("autoescape", "{% autoescape %}foo{% endautoescape %}"),
    ("block", "{% block foo %}foo{% endblock %}"),
    ("deprecated", "{% deprecated \"message\" %}"),
    ("do", "{% do 1 + 2 %}"),
    ("embed", "{% embed \"base.twig\" %}{% endembed %}"),
    ("flush", "{% flush %}"),
    ("for", "{% for i in 1..2 %}{% endfor %}"),
    ("from", "{% from \"macros\" import foo %}"),
    ("if", "{% if false %}{% endif %}"),
    ("import", "{% import \"macros\" as macros %}"),
    ("include", "{% include \"macros\" %}"),
    ("macro", "{% macro foo() %}{% endmacro %}"),
    ("set", "{% set foo = 1 %}"),
    ("with", "{% with foo %}{% endwith %}"),
];

#[test]
fn testSandboxForCoreTags() {
    for (tag, template) in CORE_TAGS {
        let twig = env("templates", sandboxed());
        let error = assert_kind(
            render_string(&twig, template),
            ErrorKind::SecurityNotAllowedTag,
        );
        let text = message(&error);
        assert!(
            text.starts_with(&format!(
                "Tag \"{tag}\" is not allowed in \"index (string template "
            )) && text.ends_with(")\" at line 1."),
            "{text}"
        );
    }
}

const EXTENDS_USE: &[(&str, &str)] = &[
    ("extends", "{% extends \"1_empty\" %}"),
    ("use", "{% use \"1_empty\" %}"),
];

#[test]
fn testSandboxForExtendsAndUseTags() {
    for (tag, template) in EXTENDS_USE {
        let twig = env("templates", sandboxed());
        let (result, deprecations) = collect(|| render_string(&twig, template));
        result.unwrap();
        assert_deprecations(
            &deprecations,
            &format!(
                "Since twig/twig 3.12: The \"{tag}\" tag is always allowed in sandboxes, but won't be in 4.0, please enable it explicitly in your sandbox policy if needed (or enable strict mode on the security policy to opt-in to the 4.0 behavior now)."
            ),
        );
    }
}

/// `data` => `['secret' => 'LEAK']` and the like.
fn data(key: &str, value: &str) -> PhpArray {
    map(vec![("data", Value::Array(map(vec![(key, s(value))])))])
}

#[test]
fn testSandboxForParserCallableFunctions() {
    let obj = || map(vec![("obj", FooObject::value())]);
    let cases: Vec<(
        &str,
        &str,
        &[&str],
        &[(&str, &[&str])],
        &[(&str, &[&str])],
        PhpArray,
        &str,
    )> = vec![
        (
            "attribute",
            "attr_secret",
            &[],
            &[],
            &[],
            data("secret", "LEAK"),
            "LEAK",
        ),
        (
            "attribute",
            "attr_bar",
            &[],
            &[],
            &[(FOO, &["bar"])],
            obj(),
            "bar",
        ),
        (
            "attribute",
            "attr_foo",
            &[],
            &[(FOO, &["foo"])],
            &[],
            obj(),
            "foo",
        ),
        (
            "block",
            "block",
            &["block"],
            &[],
            &[],
            PhpArray::new(),
            "BB",
        ),
        (
            "parent",
            "child",
            &["block"],
            &[],
            &[],
            PhpArray::new(),
            "PARENT CHILD",
        ),
    ];
    for (function, name, tags, methods, properties, context, expected) in cases {
        let twig = env(
            "parser_callables",
            Cfg {
                tags,
                methods,
                properties,
                ..sandboxed()
            },
        );
        let (result, deprecations) = collect(|| render(&twig, name, context));
        assert_eq!(result.unwrap(), expected, "{name}");
        assert_deprecations(
            &deprecations,
            &format!(
                "Since twig/twig 3.27: The \"{function}\" function is always allowed in sandboxes, but won't be in 4.0, please enable it explicitly in your sandbox policy if needed (or enable strict mode on the security policy to opt-in to the 4.0 behavior now)."
            ),
        );
    }
}

#[test]
fn testSandboxWithAllowedParserCallableFunctions() {
    let cases: Vec<(&str, &[&str], &[&str], PhpArray, &str)> = vec![
        ("attr_x", &[], &["attribute"], data("x", "OK"), "OK"),
        ("block", &["block"], &["block"], PhpArray::new(), "BB"),
        (
            "child",
            &["block", "extends"],
            &["parent"],
            PhpArray::new(),
            "PARENT CHILD",
        ),
    ];
    for (name, tags, functions, context, expected) in cases {
        let twig = env(
            "parser_callables",
            Cfg {
                tags,
                functions,
                ..sandboxed()
            },
        );
        assert_eq!(render(&twig, name, context).unwrap(), expected, "{name}");
    }
}

const STRICT_TAGS: &[(&str, &str)] = &[
    ("extends", "{% extends \"1_empty\" %}"),
    ("use", "{% use \"1_empty\" %}"),
    // use of a missing template is rejected before the loader is reached
    ("use", "{% use \"does_not_exist\" %}"),
    // use of a missing block is rejected before the trait is resolved
    ("use", "{% use \"1_layout\" with does_not_exist as alias %}"),
    // use of a non-traitable template is rejected before the trait is resolved
    ("use", "{% use \"1_child\" %}"),
];

#[test]
fn testStrictSandboxRejectsGrandfatheredTags() {
    for (tag, template) in STRICT_TAGS {
        let twig = env(
            "templates",
            Cfg {
                strict: true,
                ..sandboxed()
            },
        );
        assert_error(
            render_string(&twig, template),
            ErrorKind::SecurityNotAllowedTag,
            &format!("Tag \"{tag}\" is not allowed"),
        );
    }
}

#[test]
fn testStrictSandboxRejectsGrandfatheredFunctions() {
    let cases: Vec<(&str, &str, &[&str], PhpArray)> = vec![
        ("attribute", "attr_secret", &[], data("secret", "LEAK")),
        ("block", "block", &["block"], PhpArray::new()),
        ("parent", "child", &["extends", "block"], PhpArray::new()),
    ];
    for (function, name, tags, context) in cases {
        let twig = env(
            "parser_callables",
            Cfg {
                tags,
                strict: true,
                ..sandboxed()
            },
        );
        assert_error(
            render(&twig, name, context),
            ErrorKind::SecurityNotAllowedFunction,
            &format!("Function \"{function}\" is not allowed"),
        );
    }
}

#[test]
fn testStrictSandboxStillAllowsExplicitlyAllowedGrandfatheredNames() {
    let twig = env(
        "parser_callables",
        Cfg {
            tags: &["extends", "block"],
            functions: &["parent", "attribute"],
            strict: true,
            ..sandboxed()
        },
    );
    assert_eq!(
        render(&twig, "child_attr", data("x", "OK")).unwrap(),
        "PARENT CHILD - OK"
    );
}

fn names(list: &[&str]) -> Vec<Vec<u8>> {
    list.iter().map(|n| n.as_bytes().to_vec()).collect()
}

#[test]
fn testStrictModeCanBeEnabledViaSetterAfterConstruction() {
    let policy = SecurityPolicy::new(&[], &[], &[], &[], &[], &[]);
    policy.set_strict(true);
    assert_kind(
        policy.check_security(&names(&["extends"]), &[], &[], &[]),
        ErrorKind::SecurityNotAllowedTag,
    );
}

const CORE_TESTS: &[&str] = &[
    "{{ \"\" is empty ? \"y\" }}",
    "{{ [] is iterable ? \"y\" }}",
    "{{ null is null ? \"y\" }}",
    "{{ null is none ? \"y\" }}",
    "{{ 1 is defined ? \"y\" }}",
    "{{ 2 is even ? \"y\" }}",
    "{{ 3 is odd ? \"y\" }}",
    "{{ true is true ? \"y\" }}",
    "{{ 1 is same as(1) ? \"y\" }}",
    "{{ 4 is divisible by(2) ? \"y\" }}",
    "{{ [] is sequence ? \"y\" }}",
    "{{ {\"a\": 1} is mapping ? \"y\" }}",
];

#[test]
fn testSandboxAllowsAlwaysAllowedCoreTests() {
    for template in CORE_TESTS {
        let twig = env(
            "templates",
            Cfg {
                strict: true,
                ..sandboxed()
            },
        );
        assert_eq!(render_string(&twig, template).unwrap(), "y", "{template}");
    }
}

#[test]
fn testSandboxForConstantTest() {
    let twig = env("tests", sandboxed());
    let (result, deprecations) = collect(|| render(&twig, "constant", PhpArray::new()));
    result.unwrap();
    assert_deprecations(
        &deprecations,
        "Since twig/twig 3.28: The \"constant\" test is always allowed in sandboxes, but won't be in 4.0, please enable it explicitly in your sandbox policy if needed (or enable strict mode on the security policy to opt-in to the 4.0 behavior now).",
    );
}

#[test]
fn testSandboxForUserDefinedTest() {
    let twig = env("tests", sandboxed());
    let (result, deprecations) = collect(|| render(&twig, "unsafe", PhpArray::new()));
    assert_eq!(result.unwrap(), "y");
    assert_eq!(unsafe_test_calls(), 1);
    assert_deprecations(
        &deprecations,
        "Since twig/twig 3.28: The \"unsafe\" test is always allowed in sandboxes, but won't be in 4.0, please enable it explicitly in your sandbox policy if needed (or enable strict mode on the security policy to opt-in to the 4.0 behavior now).",
    );
}

#[test]
fn testSandboxAllowsAllowListedTest() {
    let twig = env(
        "tests",
        Cfg {
            tests: &["unsafe"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "unsafe", PhpArray::new()).unwrap(), "y");
}

#[test]
fn testStrictSandboxRejectsConstantTest() {
    let twig = env(
        "tests",
        Cfg {
            strict: true,
            ..sandboxed()
        },
    );
    assert_error(
        render(&twig, "constant", PhpArray::new()),
        ErrorKind::SecurityNotAllowedTest,
        "Test \"constant\" is not allowed",
    );
}

#[test]
fn testStrictSandboxStillAllowsAllowListedTest() {
    let twig = env(
        "tests",
        Cfg {
            strict: true,
            tests: &["constant"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "constant_n", PhpArray::new()).unwrap(), "n");
}

#[test]
fn testStrictSandboxRejectsUserDefinedTest() {
    let twig = env(
        "tests",
        Cfg {
            strict: true,
            ..sandboxed()
        },
    );
    assert_error(
        render(&twig, "unsafe", PhpArray::new()),
        ErrorKind::SecurityNotAllowedTest,
        "Test \"unsafe\" is not allowed",
    );
}

#[test]
fn testImplicitBooleanCoercionDoesNotRequireAllowListingTests() {
    let twig = env(
        "tests",
        Cfg {
            tags: &["if"],
            filters: &["default"],
            strict: true,
            ..sandboxed()
        },
    );
    assert_eq!(
        render(
            &twig,
            "implicit",
            map(vec![("x", Value::Bool(true)), ("y", Value::Bool(false))])
        )
        .unwrap(),
        "acdef"
    );
}

#[test]
fn testStrictSandboxRejectedTestCarriesSourceAndLine() {
    let twig = env(
        "test_line",
        Cfg {
            strict: true,
            ..sandboxed()
        },
    );
    let error = assert_kind(
        render(&twig, "index", PhpArray::new()),
        ErrorKind::SecurityNotAllowedTest,
    );
    assert_eq!(error.security_name(), Some(&b"constant"[..]));
    assert_eq!(error.template_line(), 2);
    assert_eq!(error.source_context().unwrap().name(), "index");
}

#[test]
fn testStrictModeRejectsTestsViaSetter() {
    let policy = SecurityPolicy::new(&[], &[], &[], &[], &[], &[]);
    policy.set_strict(true);
    assert_kind(
        policy.check_security(&[], &[], &[], &names(&["empty"])),
        ErrorKind::SecurityNotAllowedTest,
    );
}

#[test]
fn testAllowedTestsCanBeUpdatedViaSetter() {
    let policy = SecurityPolicy::new(&[], &[], &[], &[], &[], &[]);
    policy.set_strict(true);
    policy.set_allowed_tests(&["empty"]);
    policy
        .check_security(&[], &[], &[], &names(&["empty"]))
        .unwrap();
    assert_kind(
        policy.check_security(&[], &[], &[], &names(&["null"])),
        ErrorKind::SecurityNotAllowedTest,
    );
}

#[test]
fn testSandboxWithInheritance() {
    let twig = env(
        "templates",
        Cfg {
            tags: &["extends", "block"],
            ..sandboxed()
        },
    );
    let error = assert_kind(
        render(&twig, "1_child", PhpArray::new()),
        ErrorKind::SecurityNotAllowedFilter,
    );
    assert!(
        message(&error).contains("Filter \"json_encode\" is not allowed in \"1_child\" at line 3."),
        "{error}"
    );
}

#[test]
fn testSandboxGloballySet() {
    let twig = env("templates", Cfg::default());
    assert_eq!(
        render(&twig, "1_basic", params()).unwrap(),
        "FOO",
        "Sandbox does nothing if it is disabled globally"
    );
}

#[test]
fn testSandboxUnallowedPropertyAccessor() {
    let twig = env("templates", sandboxed());
    assert_member(
        render(&twig, "1_basic1", map(vec![("obj", object(MagicObject))])),
        ErrorKind::SecurityNotAllowedProperty,
        "Twig\\Tests\\Extension\\MagicObject",
        "foo",
    );
}

#[test]
fn testSandboxUnallowedArrayIndexAccessor() {
    let twig = env("templates", sandboxed());
    // ArrayObject and other internal array-like classes are exempted from sandbox restrictions
    let array_object = object(ArrayObject::new(map(vec![("foo", s("bar"))])));
    assert_eq!(
        render(
            &twig,
            "1_array_like",
            map(vec![("array_like", array_object)])
        )
        .unwrap(),
        "bar"
    );
    assert_member(
        render(&twig, "1_array_like", params()),
        ErrorKind::SecurityNotAllowedProperty,
        "Twig\\Tests\\Extension\\ArrayLikeObject",
        "foo",
    );
}

/// The anonymous `\ArrayAccess` class of
/// `testSandboxNonStringKeyAccessDoesNotTriggerImplicitConversionDeprecation`.
#[derive(Debug)]
struct NullArrayAccess;

impl PhpObject for NullArrayAccess {
    fn class(&self) -> &'static str {
        "ArrayAccess@anonymous"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        class.eq_ignore_ascii_case("ArrayAccess")
    }

    fn offset_get(&self, _offset: &Value) -> Option<Result<Value, Thrown>> {
        Some(Ok(Value::Null))
    }

    fn offset_exists(&self, _offset: &Value) -> Option<Result<bool, Thrown>> {
        Some(Ok(false))
    }
}

#[test]
fn testSandboxNonStringKeyAccessDoesNotTriggerImplicitConversionDeprecation() {
    // `new Environment($loader)`: the default options (autoescape html)
    for (name, key) in [("float", "3"), ("true", "1"), ("false", "0")] {
        let twig = env(
            "non_string_keys",
            Cfg {
                filters: &["escape"],
                html: true,
                ..sandboxed()
            },
        );
        let (result, deprecations) =
            collect(|| render(&twig, name, map(vec![("obj", object(NullArrayAccess))])));
        assert!(deprecations.is_empty(), "{name}");
        assert_member(
            result,
            ErrorKind::SecurityNotAllowedProperty,
            "ArrayAccess@anonymous",
            key,
        );
    }
}

#[test]
fn testIfSandBoxIsDisabledAfterSyntaxErrorLegacy() {
    let twig = env("templates", Cfg::default());
    let (result, _) = collect(|| render(&twig, "1_syntax_error_wrapper_legacy", params()));
    if let Err(e) = result
        && e.kind() == ErrorKind::Syntax
    {
        assert!(!sandbox(&twig).is_sandboxed(None));
    }
}

#[test]
fn testIfSandBoxIsDisabledAfterSyntaxError() {
    let twig = env("templates", Cfg::default());
    if let Err(e) = render(&twig, "1_syntax_error_wrapper", params())
        && e.kind() == ErrorKind::Syntax
    {
        assert!(!sandbox(&twig).is_sandboxed(None));
    }
}

#[test]
fn testSandboxGloballyFalseUnallowedFilterWithIncludeTemplateFromStringSandboxed() {
    let twig = env("templates", Cfg::default());
    assert_refused(
        render(
            &twig,
            "1_basic2_include_template_from_string_sandboxed",
            params(),
        ),
        ErrorKind::SecurityNotAllowedFilter,
        "upper",
    );
}

#[test]
fn testSandboxGloballyTrueUnallowedFilterWithIncludeTemplateFromStringSandboxed() {
    let twig = env(
        "templates",
        Cfg {
            functions: &["include", "template_from_string"],
            ..sandboxed()
        },
    );
    assert_refused(
        render(
            &twig,
            "1_basic2_include_template_from_string_sandboxed",
            params(),
        ),
        ErrorKind::SecurityNotAllowedFilter,
        "upper",
    );
}

#[test]
fn testSandboxGloballyFalseUnallowedFilterWithIncludeTemplateFromStringNotSandboxed() {
    let twig = env("templates", Cfg::default());
    assert_eq!(
        render(&twig, "1_basic2_include_template_from_string", params()).unwrap(),
        "FABIEN"
    );
}

#[test]
fn testSandboxGloballyTrueUnallowedFilterWithIncludeTemplateFromStringNotSandboxed() {
    let twig = env(
        "templates",
        Cfg {
            functions: &["include", "template_from_string"],
            ..sandboxed()
        },
    );
    assert_refused(
        render(&twig, "1_basic2_include_template_from_string", params()),
        ErrorKind::SecurityNotAllowedFilter,
        "upper",
    );
}

#[test]
fn testSandboxUnallowedFilter() {
    let twig = env("templates", sandboxed());
    assert_refused(
        render(&twig, "1_basic2", params()),
        ErrorKind::SecurityNotAllowedFilter,
        "upper",
    );
}

#[test]
fn testSandboxUnallowedTag() {
    let twig = env("templates", sandboxed());
    assert_refused(
        render(&twig, "1_basic3", params()),
        ErrorKind::SecurityNotAllowedTag,
        "if",
    );
}

#[test]
fn testSandboxUnallowedProperty() {
    let twig = env("templates", sandboxed());
    assert_member(
        render(&twig, "1_basic4", params()),
        ErrorKind::SecurityNotAllowedProperty,
        FOO,
        "bar",
    );
}

fn magic_call() -> PhpArray {
    map(vec![("o", object(MagicCallObject))])
}

#[test]
fn testSandboxAllowedMagicCallMethod() {
    let twig = env(
        "magic_call",
        Cfg {
            methods: &[(MagicCallObject::CLASS, &["hello"])],
            ..sandboxed()
        },
    );
    assert_eq!(
        render(&twig, "property", magic_call()).unwrap(),
        "call:hello"
    );
}

#[test]
fn testSandboxUnallowedMagicCallMethod() {
    let twig = env("magic_call", sandboxed());
    assert_member(
        render(&twig, "property", magic_call()),
        ErrorKind::SecurityNotAllowedProperty,
        MagicCallObject::CLASS,
        "hello",
    );
}

#[test]
fn testSandboxUnallowedMagicCallMethodWithMethodSyntax() {
    let twig = env("magic_call", sandboxed());
    assert_member(
        render(&twig, "method", magic_call()),
        ErrorKind::SecurityNotAllowedMethod,
        MagicCallObject::CLASS,
        "hello",
    );
}

#[test]
fn testSandboxAllowingCallLiteralDoesNotAllowMagicCallMethod() {
    let twig = env(
        "magic_call",
        Cfg {
            methods: &[(MagicCallObject::CLASS, &["__call"])],
            ..sandboxed()
        },
    );
    assert_member(
        render(&twig, "property", magic_call()),
        ErrorKind::SecurityNotAllowedProperty,
        MagicCallObject::CLASS,
        "hello",
    );
}

#[test]
fn testSandboxFallsBackToMagicCallMethodForUnallowedProperty() {
    let twig = env(
        "magic_call",
        Cfg {
            methods: &[(MagicCallObject::CLASS, &["secret"])],
            ..sandboxed()
        },
    );
    assert_eq!(
        render(&twig, "secret", magic_call()).unwrap(),
        "call:secret"
    );
}

/// The names of the templates of a set (its data provider's cases).
fn cases(set: &str) -> Vec<String> {
    templates(set)
        .iter()
        .filter(|(_, index, _, _)| index.is_none())
        .map(|(name, _, _, _)| String::from_utf8_lossy(name).into_owned())
        .collect()
}

#[test]
fn testSandboxUnallowedToString() {
    let names = cases("unallowed_to_string");
    assert_eq!(names.len(), 70);
    for name in names {
        let twig = env(
            "unallowed_to_string",
            Cfg {
                tags: &["if", "do", "for", "set"],
                filters: &["upper", "join", "replace", "format", "split"],
                methods: &[(FOO, &["getAnotherFooObject"])],
                functions: &["random", "range", "my_func"],
                ..sandboxed()
            },
        );
        let result = render(&twig, &name, params());
        let error = result.expect_err(&name);
        assert_eq!(
            error.kind(),
            ErrorKind::SecurityNotAllowedMethod,
            "{name}: {error}"
        );
        assert_eq!(error.security_class_name(), Some(FOO), "{name}");
        assert_eq!(error.security_name(), Some(&b"__tostring"[..]), "{name}");
    }
}

#[test]
fn testSandboxBlocksToStringOnFunctionReturn() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            functions: &["make_obj"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "make_obj", PhpArray::new()));
}

#[test]
fn testSandboxBlocksToStringOnFilterReturn() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            filters: &["to_obj"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "to_obj", PhpArray::new()));
}

#[test]
fn testSandboxBlocksToStringOnDynamicAttributeName() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            strict_variables: true,
            methods: &[(FOO, &["getAnotherFooObject"])],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "dynamic_attribute", params()));
}

#[test]
fn testSandboxBlocksToStringOnDynamicMacroName() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["import", "macro"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "dynamic_macro", params()));
}

#[test]
fn testSandboxBlocksToStringOnIncludeTemplateName() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["include"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "include", params()));
}

#[test]
fn testSandboxBlocksToStringOnExtendsTemplateName() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["extends"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "extends", params()));
}

#[test]
fn testSandboxBlocksToStringOnBlockFunctionTemplateName() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            functions: &["block"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "block_function", params()));
}

#[test]
fn testSandboxBlocksToStringOnEmbedTemplateName() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["embed", "extends"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "embed", params()));
}

#[test]
fn testSandboxBlocksToStringOnIsConstantTestArgument() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["if"],
            tests: &["constant"],
            ..sandboxed()
        },
    );
    assert_to_string_refused(render(&twig, "constant_argument", params()));
}

#[test]
fn testSandboxBlocksToStringOnDeprecatedMessage() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["deprecated"],
            ..sandboxed()
        },
    );
    let (result, _) = collect(|| render(&twig, "deprecated", params()));
    assert_to_string_refused(result);
}

#[test]
fn testSandboxKeepsSelfImportShortcut() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["from", "macro", "import"],
            filters: &["lower"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "self_import", PhpArray::new()).unwrap(), "a");
}

#[test]
fn testSandboxAllowedToString() {
    let cases = [
        ("constant_test", ""),
        ("set_object", "foo"),
        ("do_object_discarded", ""),
        ("set_object_assigned", "1"),
        ("is_defined1", "1"),
        ("is_defined2", ""),
        ("is_null", ""),
        ("is_sameas", "1"),
        ("is_sameas_no_brackets", "1"),
        ("is_sameas_from_array", "1"),
        ("is_sameas_from_array_no_brackets", "1"),
        ("is_sameas_from_another_method", ""),
        ("is_sameas_from_another_method_no_brackets", ""),
    ];
    assert_eq!(cases.len(), self::cases("allowed_to_string").len());
    for (name, output) in cases {
        let twig = env(
            "allowed_to_string",
            Cfg {
                tags: &["set", "do"],
                methods: &[(FOO, &["foo", "getAnotherFooObject"])],
                tests: &["constant"],
                ..sandboxed()
            },
        );
        assert_eq!(render(&twig, name, params()).unwrap(), output, "{name}");
    }
}

#[test]
fn testSandboxAllowMethodToString() {
    let twig = env(
        "templates",
        Cfg {
            methods: &[(FOO, &["__toString"])],
            ..sandboxed()
        },
    );
    let context = params();
    FooObject::reset();
    assert_eq!(render(&twig, "1_basic5", context).unwrap(), "foo");
    assert_eq!(
        FooObject::called("__toString"),
        1,
        "Sandbox only calls method once"
    );
}

#[test]
fn testSandboxAllowsArrayDynamicKeyWhenToStringAllowed() {
    let twig = env(
        "to_string_blocks",
        Cfg {
            tags: &["set"],
            filters: &["join", "keys"],
            methods: &[(FOO, &["__toString", "getAnotherFooObject"])],
            ..sandboxed()
        },
    );
    assert_eq!(
        render(&twig, "dynamic_key_allowed", params()).unwrap(),
        "foo"
    );
}

#[test]
fn testSandboxAllowMethodToStringDisabled() {
    let twig = env("templates", Cfg::default());
    let context = params();
    FooObject::reset();
    assert_eq!(render(&twig, "1_basic5", context).unwrap(), "foo");
    assert_eq!(
        FooObject::called("__toString"),
        1,
        "Sandbox only calls method once"
    );
}

#[test]
fn testSandboxUnallowedFunction() {
    let twig = env("templates", sandboxed());
    assert_refused(
        render(&twig, "1_basic7", params()),
        ErrorKind::SecurityNotAllowedFunction,
        "cycle",
    );
}

#[test]
fn testSandboxUnallowedRangeOperator() {
    let twig = env("templates", sandboxed());
    assert_refused(
        render(&twig, "1_range_operator", params()),
        ErrorKind::SecurityNotAllowedFunction,
        "range",
    );
}

#[test]
fn testSandboxAllowMethodFoo() {
    let twig = env(
        "templates",
        Cfg {
            methods: &[(FOO, &["foo"])],
            ..sandboxed()
        },
    );
    FooObject::reset();
    assert_eq!(render(&twig, "1_basic1", params()).unwrap(), "foo");
    assert_eq!(
        FooObject::called("foo"),
        1,
        "Sandbox only calls method once"
    );
}

#[test]
fn testSandboxAllowFilter() {
    let twig = env(
        "templates",
        Cfg {
            filters: &["upper"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "1_basic2", params()).unwrap(), "FABIEN");
}

#[test]
fn testSandboxAllowTag() {
    let twig = env(
        "templates",
        Cfg {
            tags: &["if"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "1_basic3", params()).unwrap(), "foo");
}

#[test]
fn testSandboxAllowProperty() {
    let twig = env(
        "templates",
        Cfg {
            properties: &[(FOO, &["bar"])],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "1_basic4", params()).unwrap(), "bar");
}

#[test]
fn testSandboxAllowDestructuring() {
    let twig = env(
        "destructuring",
        Cfg {
            tags: &["do"],
            methods: &[(FOO, &["foo"])],
            properties: &[(FOO, &["bar"])],
            ..sandboxed()
        },
    );
    FooObject::reset();
    assert_eq!(render(&twig, "both", params()).unwrap(), "bar-foo");
}

#[test]
fn testSandboxUnallowedDestructuringProperty() {
    let twig = env(
        "destructuring",
        Cfg {
            tags: &["do"],
            ..sandboxed()
        },
    );
    assert_member(
        render(&twig, "property", params()),
        ErrorKind::SecurityNotAllowedProperty,
        FOO,
        "bar",
    );
}

#[test]
fn testSandboxUnallowedDestructuringMethod() {
    let twig = env(
        "destructuring",
        Cfg {
            tags: &["do"],
            properties: &[(FOO, &["foo"])],
            ..sandboxed()
        },
    );
    assert_member(
        render(&twig, "method", params()),
        ErrorKind::SecurityNotAllowedMethod,
        FOO,
        "foo",
    );
}

#[test]
fn testSandboxAllowFunction() {
    let twig = env(
        "templates",
        Cfg {
            functions: &["cycle"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "1_basic7", params()).unwrap(), "bar");
}

#[test]
fn testSandboxAllowRangeOperator() {
    let twig = env(
        "templates",
        Cfg {
            functions: &["range"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "1_range_operator", params()).unwrap(), "1");
}

#[test]
fn testSandboxAllowMethodsCaseInsensitive() {
    for name in ["getfoobar", "getFoobar", "getFooBar"] {
        let methods: &[(&str, &[&str])] = &[(FOO, &[name])];
        let twig = env(
            "templates",
            Cfg {
                methods,
                ..sandboxed()
            },
        );
        FooObject::reset();
        assert_eq!(render(&twig, "1_basic8", params()).unwrap(), "foobarfoobar");
        assert_eq!(
            FooObject::called("getFooBar"),
            2,
            "Sandbox only calls method once"
        );
        assert_eq!(render(&twig, "1_basic9", params()).unwrap(), "foobarfoobar");
    }
}

#[test]
fn testSandboxLocallySetForAnInclude() {
    let twig = env("include_locally", Cfg::default());
    assert_eq!(render(&twig, "2_basic", params()).unwrap(), "fooFOOfoo");

    let twig = env(
        "include_locally",
        Cfg {
            functions: &["include"],
            ..sandboxed()
        },
    );
    assert_refused(
        render(&twig, "3_basic", params()),
        ErrorKind::SecurityNotAllowedTag,
        "if",
    );
}

#[test]
fn testMacrosInASandbox() {
    let twig = env(
        "macros_html",
        Cfg {
            html: true,
            tags: &["macro", "import"],
            filters: &["escape"],
            ..sandboxed()
        },
    );
    assert_eq!(
        render(&twig, "import", PhpArray::new()).unwrap(),
        "<p>username</p>"
    );
}

/// The Rust code of a template (`compileSource()`).
fn compiled_source(twig: &Environment, name: &str) -> String {
    let source = twig
        .loader()
        .unwrap()
        .source_context(name.as_bytes())
        .unwrap();
    twig.compile_source(source).unwrap()
}

#[test]
fn testSelfMacroReferenceWithStringLiteralDoesNotInjectPhp() {
    // The compiled code is Rust here: the payload must stay in a string
    // literal, and rendering must not run it (there is no PHP to run, so
    // the render fails or prints, but triggers nothing).
    let twig = env("macro_injection", sandboxed());
    let compiled = compiled_source(&twig, "self");
    assert!(!compiled.contains("trigger_error(\"BAD-MACRO-REF\")"));
    assert!(!compiled.contains("->macro_foo + 1;"));
    let (_, deprecations) = collect(|| render(&twig, "self", PhpArray::new()));
    assert!(deprecations.is_empty());
}

#[test]
fn testImportedTemplateMacroReferenceWithBadIdentifierDoesNotInjectPhp() {
    let twig = env(
        "macro_injection",
        Cfg {
            tags: &["import"],
            ..sandboxed()
        },
    );
    let compiled = compiled_source(&twig, "imported");
    assert!(!compiled.contains("trigger_error(\"BAD-IMPORT-REF\")"));
    let (_, deprecations) = collect(|| render(&twig, "imported", PhpArray::new()));
    assert!(deprecations.is_empty());
}

#[test]
fn testSelfMacroReferenceWithValidIdentifierStillWorks() {
    let twig = env(
        "macros_html",
        Cfg {
            html: true,
            tags: &["macro"],
            filters: &["escape"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "greet", PhpArray::new()).unwrap(), "Hi World");
}

#[test]
fn testSandboxDisabledAfterIncludeFunctionError() {
    let twig = env("templates", Cfg::default());
    assert!(
        render(&twig, "1_include", params()).is_err(),
        "An exception should be thrown for this test to be valid."
    );
    assert!(
        !sandbox(&twig).is_sandboxed(None),
        "Sandboxed include() function call should not leave Sandbox enabled when an error occurs."
    );
}

#[test]
fn testSandboxTagIncludeWithPreloadedTemplate() {
    // The deprecation of the `sandbox` tag is triggered when the template
    // is compiled, by the build script.
    let twig = env("sandbox_tag", Cfg::default());
    twig.load(b"included").unwrap();
    let (result, _) = collect(|| render(&twig, "index", PhpArray::new()));
    assert_kind(result, ErrorKind::SecurityNotAllowedFilter);
}

#[test]
fn testSandboxTagIncludeIgnoreMissingWithPreloadedTemplate() {
    let twig = env("sandbox_tag", Cfg::default());
    twig.load(b"included").unwrap();
    let (result, _) = collect(|| render(&twig, "ignore_missing", PhpArray::new()));
    assert_kind(result, ErrorKind::SecurityNotAllowedFilter);
}

#[test]
fn testSandboxWithNoClosureFilter() {
    let twig = env(
        "filter_html",
        Cfg {
            html: true,
            filters: &["escape", "filter", "join"],
            ..sandboxed()
        },
    );
    assert_error(
        render(&twig, "index", PhpArray::new()),
        ErrorKind::Runtime,
        "The callable passed to the \"filter\" filter must be a Closure in sandbox mode in \"index\" at line 1.",
    );
}

#[test]
fn testSandboxWithClosureFilter() {
    let twig = env(
        "filter_html",
        Cfg {
            html: true,
            filters: &["escape", "filter", "join"],
            ..sandboxed()
        },
    );
    assert_eq!(
        render(&twig, "closure", PhpArray::new()).unwrap(),
        "foo, bar"
    );
}

#[test]
fn testMultipleClassMatchesViaInheritanceInAllowedMethods() {
    const CHILD: &str = "Twig\\Tests\\Extension\\ChildClass";
    const PARENT: &str = "Twig\\Tests\\Extension\\ParentClass";
    let child_first = env(
        "templates",
        Cfg {
            methods: &[(CHILD, &["ChildMethod"]), (PARENT, &["ParentMethod"])],
            ..sandboxed()
        },
    );
    let parent_first = env(
        "templates",
        Cfg {
            methods: &[(PARENT, &["ParentMethod"]), (CHILD, &["ChildMethod"])],
            ..sandboxed()
        },
    );
    render(&child_first, "1_childobj_childmethod", params()).unwrap();
    render(&parent_first, "1_childobj_parentmethod", params()).unwrap();
    render(&parent_first, "1_childobj_childmethod", params()).unwrap();
    render(&child_first, "1_childobj_parentmethod", params()).unwrap();
}

fn column_object() -> PhpArray {
    map(vec![("obj", object(ColumnObject))])
}

#[test]
fn testSandboxAllowsColumnFilterOnAllowedProperty() {
    let twig = env(
        "column",
        Cfg {
            filters: &["column", "first"],
            properties: &[(ColumnObject::CLASS, &["bar"])],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "bar", column_object()).unwrap(), "bar");
}

#[test]
fn testSandboxBlocksColumnFilterOnDisallowedProperty() {
    let twig = env(
        "column",
        Cfg {
            filters: &["column", "first"],
            ..sandboxed()
        },
    );
    assert_member(
        render(&twig, "bar", column_object()),
        ErrorKind::SecurityNotAllowedProperty,
        ColumnObject::CLASS,
        "bar",
    );
}

#[test]
fn testSandboxBlocksColumnFilterOnDisallowedIndex() {
    let twig = env(
        "column",
        Cfg {
            filters: &["column", "first", "keys"],
            properties: &[(ColumnObject::CLASS, &["bar"])],
            ..sandboxed()
        },
    );
    assert_member(
        render(&twig, "index", column_object()),
        ErrorKind::SecurityNotAllowedProperty,
        ColumnObject::CLASS,
        "foo",
    );
}

#[test]
fn testSandboxBlocksColumnFilterOnMagicGetter() {
    let twig = env(
        "column",
        Cfg {
            filters: &["column", "first"],
            ..sandboxed()
        },
    );
    assert_member(
        render(&twig, "magic", map(vec![("magic", object(MagicObject))])),
        ErrorKind::SecurityNotAllowedProperty,
        "Twig\\Tests\\Extension\\MagicObject",
        "anything",
    );
}

const STRINGABLE_TRAVERSABLE: &[(&str, &[&str])] =
    &[(StringableTraversableObject::CLASS, &["__tostring"])];

#[test]
fn testSandboxBlocksToStringInStringableTraversable() {
    for name in ["join", "replace"] {
        let twig = env(
            "traversable",
            Cfg {
                filters: &["join", "replace"],
                methods: STRINGABLE_TRAVERSABLE,
                ..sandboxed()
            },
        );
        assert_to_string_refused(render(&twig, name, params()));
    }
}

#[test]
fn testSourcePolicySandboxBlocksToStringInStringableTraversable() {
    for name in ["join", "replace"] {
        let (twig, deprecations) = collect(|| {
            env(
                "traversable",
                Cfg {
                    filters: &["join", "replace"],
                    methods: STRINGABLE_TRAVERSABLE,
                    source_policy: source_policy(always),
                    ..Cfg::default()
                },
            )
        });
        assert_deprecations(&deprecations, SOURCE_POLICY_DEPRECATION);
        assert_to_string_refused(render(&twig, name, params()));
    }
}

#[test]
fn testSandboxAllowsPrintingStringableTraversableWhenToStringAllowed() {
    let twig = env(
        "traversable_html",
        Cfg {
            html: true,
            filters: &["escape"],
            methods: STRINGABLE_TRAVERSABLE,
            ..sandboxed()
        },
    );
    let context = map(vec![(
        "obj",
        StringableTraversableObject::value(seq(vec![s("a"), s("b")])),
    )]);
    assert_eq!(
        render(&twig, "index", context).unwrap(),
        "stringable-traversable"
    );
}

#[test]
fn testSandboxHandlesCyclicTraversableWithoutStackOverflow() {
    for name in ["cyclic_join", "cyclic_replace", "cyclic_spread"] {
        let twig = env(
            "traversable",
            Cfg {
                filters: &["join", "replace"],
                ..sandboxed()
            },
        );
        let context = map(vec![("obj", CyclicTraversableObject::value())]);
        assert_kind(render(&twig, name, context), ErrorKind::Runtime);
    }
}

#[test]
fn testSourcePolicySandboxBlocksToStringInTraversableJoin() {
    let (twig, deprecations) = collect(|| {
        env(
            "traversable",
            Cfg {
                filters: &["join"],
                source_policy: source_policy(always),
                ..Cfg::default()
            },
        )
    });
    assert_deprecations(&deprecations, SOURCE_POLICY_DEPRECATION);
    assert_to_string_refused(render(&twig, "iterator_join", params()));
}

#[test]
fn testSourcePolicySandboxBlocksToStringInTraversableReplace() {
    let (twig, deprecations) = collect(|| {
        env(
            "traversable",
            Cfg {
                filters: &["replace"],
                source_policy: source_policy(always),
                ..Cfg::default()
            },
        )
    });
    assert_deprecations(&deprecations, SOURCE_POLICY_DEPRECATION);
    assert_to_string_refused(render(&twig, "iterator_replace", params()));
}

#[test]
fn testSandboxPreservesTraversableArgumentIdentity() {
    let twig = env(
        "traversable",
        Cfg {
            methods: STRINGABLE_TRAVERSABLE,
            ..sandboxed()
        },
    );
    with_policy(&twig, |p| p.set_allowed_functions(&["render_traversable"]));
    let context = map(vec![(
        "obj",
        StringableTraversableObject::value(seq(vec![s("a"), s("b")])),
    )]);
    assert_eq!(
        render(&twig, "render_obj", context).unwrap(),
        "stringable-traversable"
    );
}

#[test]
fn testSandboxStillBlocksDisallowedToStringInTraversableArgument() {
    let twig = env(
        "traversable",
        Cfg {
            methods: STRINGABLE_TRAVERSABLE,
            ..sandboxed()
        },
    );
    with_policy(&twig, |p| p.set_allowed_functions(&["render_traversable"]));
    assert_to_string_refused(render(&twig, "render_iterator", params()));
}

/// An environment of the set `set` (variant `variant`) where `function`
/// is allowed, as the tests do with `setAllowedFunctions()`.
fn param_env(set: &str, variant: &str, function: &str) -> Environment {
    let twig = env(
        set,
        Cfg {
            variant,
            ..sandboxed()
        },
    );
    with_policy(&twig, |p| p.set_allowed_functions(&[function]));
    twig
}

#[test]
fn testSafePhpParamTypesSkipToStringWrap() {
    // `int`, `float`, `bool`, a non-Stringable class, `?int` and
    // `int|float` are all coercion-safe parameter types: one `safe_fn`
    // stands for the six closures.
    let cases: Vec<(&str, &str, Value)> = vec![
        ("int param", "safe_n", Value::Int(42)),
        ("float param", "safe_n", Value::Float(3.14)),
        ("bool param", "safe_b", Value::Bool(true)),
        (
            "non-stringable class param",
            "safe_obj",
            object(ColumnObject),
        ),
        ("nullable int param with null value", "safe_n", Value::Null),
        ("int|float union param", "safe_n", Value::Int(7)),
    ];
    for (case, name, value) in cases {
        let twig = param_env("param_types", "", "safe_fn");
        let key = match name {
            "safe_b" => "b",
            "safe_obj" => "obj",
            _ => "n",
        };
        assert_eq!(
            render(&twig, name, map(vec![(key, value)])).unwrap(),
            "ok",
            "{case}"
        );
    }
}

#[test]
fn testUnsafePhpParamTypesStillWrap() {
    // untyped, `mixed`, `string`, `object` and `\Stringable` parameters
    // all keep the check: one `unsafe_fn` stands for the five closures.
    for case in [
        "untyped param",
        "mixed param",
        "string param",
        "object param",
        "Stringable param",
    ] {
        let twig = param_env("param_types", "", "unsafe_fn");
        let result = render(&twig, "unsafe_obj", map(vec![("obj", FooObject::value())]));
        let error = result.expect_err(case);
        assert_eq!(error.kind(), ErrorKind::SecurityNotAllowedMethod, "{case}");
    }
}

#[test]
fn testOpenPhpParamTypesStillWrap() {
    let cases = [
        (FooObject::countable(), FooObject::COUNTABLE_CLASS),
        (object(StringablePlainObject), StringablePlainObject::CLASS),
    ];
    for (obj, class) in cases {
        let twig = param_env("param_types", "", "unsafe_fn");
        assert_member(
            render(&twig, "unsafe_obj", map(vec![("obj", obj)])),
            ErrorKind::SecurityNotAllowedMethod,
            class,
            "__tostring",
        );
    }
}

#[test]
fn testTestArgumentsMapAfterTheTestedValueParameter() {
    let twig = env("param_types", sandboxed());
    with_policy(&twig, |p| p.set_allowed_tests(&["my_test"]));
    assert_to_string_refused(render(
        &twig,
        "my_test",
        map(vec![("obj", FooObject::value())]),
    ));
}

#[test]
fn testSafeVariadicPhpTypeSkipsToStringWrap() {
    let twig = param_env("safe_variadic", "safe_variadic", "safe_fn");
    assert_eq!(render(&twig, "index", PhpArray::new()).unwrap(), "ok");
}

#[test]
fn testSpreadIntoUnsafeVariadicStillWraps() {
    let twig = param_env("unsafe_variadic", "unsafe_variadic", "unsafe_fn");
    assert_to_string_refused(render(
        &twig,
        "index",
        map(vec![("args", array(vec![FooObject::value()]))]),
    ));
}

#[test]
fn testNormalizedNamedArgumentDoesNotFallThroughToSafeVariadic() {
    let twig = param_env("named_variadic", "named_variadic", "unsafe_fn");
    assert_to_string_refused(render(
        &twig,
        "index",
        map(vec![("obj", FooObject::value())]),
    ));
}

#[test]
fn testNormalizedNamedFilterArgumentDoesNotFallThroughToSafeVariadic() {
    let twig = env("param_types", sandboxed());
    with_policy(&twig, |p| p.set_allowed_filters(&["unsafe_filter"]));
    assert_to_string_refused(render(
        &twig,
        "unsafe_filter",
        map(vec![("obj", FooObject::value())]),
    ));
}

#[test]
fn testNormalizedNamedTestArgumentDoesNotFallThroughToSafeVariadic() {
    let twig = env("param_types", sandboxed());
    with_policy(&twig, |p| p.set_allowed_tests(&["unsafe_test"]));
    assert_to_string_refused(render(
        &twig,
        "unsafe_test",
        map(vec![("obj", FooObject::value())]),
    ));
}

#[test]
fn testFilterInputTypeSkipsToStringWrap() {
    let twig = env("param_types", sandboxed());
    with_policy(&twig, |p| p.set_allowed_filters(&["safe_filter"]));
    assert_eq!(
        render(&twig, "safe_filter", map(vec![("n", Value::Int(42))])).unwrap(),
        "ok"
    );
}

#[test]
fn testColumnFilterUnaffectedOutsideSandbox() {
    let twig = env("column", Cfg::default());
    assert_eq!(render(&twig, "bar", column_object()).unwrap(), "bar");
}

/// `getEnvironment(false, ..., $sourcePolicy)` for the set `set`, with
/// the deprecation the source policy triggers checked.
fn source_policy_env(set: &str, cfg: Cfg<'_>) -> Environment {
    let (twig, deprecations) = collect(|| env(set, cfg));
    assert_deprecations(&deprecations, SOURCE_POLICY_DEPRECATION);
    twig
}

#[test]
fn testSourcePolicySandboxBlocksColumnFilterOnDisallowedProperty() {
    let twig = source_policy_env(
        "column",
        Cfg {
            filters: &["column", "first"],
            source_policy: source_policy(always),
            ..Cfg::default()
        },
    );
    assert_member(
        render(&twig, "bar", column_object()),
        ErrorKind::SecurityNotAllowedProperty,
        ColumnObject::CLASS,
        "bar",
    );
}

#[test]
fn testSourcePolicySandboxBlocksColumnFilterOnDisallowedIndex() {
    let twig = source_policy_env(
        "column",
        Cfg {
            filters: &["column", "first", "keys"],
            properties: &[(ColumnObject::CLASS, &["bar"])],
            source_policy: source_policy(always),
            ..Cfg::default()
        },
    );
    assert_member(
        render(&twig, "index", column_object()),
        ErrorKind::SecurityNotAllowedProperty,
        ColumnObject::CLASS,
        "foo",
    );
}

#[test]
fn testSourcePolicySandboxAllowsColumnFilterOnAllowedProperty() {
    let twig = source_policy_env(
        "column",
        Cfg {
            filters: &["column", "first"],
            properties: &[(ColumnObject::CLASS, &["bar"])],
            source_policy: source_policy(always),
            ..Cfg::default()
        },
    );
    assert_eq!(render(&twig, "bar", column_object()).unwrap(), "bar");
}

#[test]
fn testSourcePolicySandboxBlocksColumnFilterOnMagicGetter() {
    let twig = source_policy_env(
        "column",
        Cfg {
            filters: &["column", "first"],
            source_policy: source_policy(always),
            ..Cfg::default()
        },
    );
    assert_member(
        render(&twig, "magic", map(vec![("magic", object(MagicObject))])),
        ErrorKind::SecurityNotAllowedProperty,
        "Twig\\Tests\\Extension\\MagicObject",
        "anything",
    );
}

#[test]
fn testSandboxSourcePolicyEnableReturningFalse() {
    let twig = source_policy_env(
        "templates",
        Cfg {
            source_policy: source_policy(|name| name != "1_basic"),
            ..Cfg::default()
        },
    );
    assert_eq!(render(&twig, "1_basic", params()).unwrap(), "FOO");
}

#[test]
fn testSandboxSourcePolicyEnableReturningTrue() {
    let twig = source_policy_env(
        "templates",
        Cfg {
            source_policy: source_policy(|name| name == "1_basic"),
            ..Cfg::default()
        },
    );
    let error = render(&twig, "1_basic", PhpArray::new()).expect_err("a security error");
    assert!(error.kind().is_security_error(), "{error}");
}

#[test]
fn testSandboxSourcePolicyFalseDoesntOverrideOtherEnables() {
    let twig = source_policy_env(
        "templates",
        Cfg {
            source_policy: source_policy(never),
            ..sandboxed()
        },
    );
    let error = render(&twig, "1_basic", PhpArray::new()).expect_err("a security error");
    assert!(error.kind().is_security_error(), "{error}");
}

#[test]
fn testSourcePolicyBlocksNonClosureCallableInArrow() {
    for name in [
        "sort",
        "filter",
        "map",
        "reduce",
        "find",
        "has_some",
        "has_every",
    ] {
        let twig = source_policy_env(
            "arrow",
            Cfg {
                filters: &["sort", "filter", "map", "reduce", "find", "join"],
                source_policy: source_policy(always),
                ..Cfg::default()
            },
        );
        assert_error(
            render(&twig, name, PhpArray::new()),
            ErrorKind::Runtime,
            "must be a Closure in sandbox mode",
        );
    }
}

#[test]
fn testSourcePolicyAllowsClosureInArrow() {
    let twig = source_policy_env(
        "arrow",
        Cfg {
            filters: &["sort", "join"],
            source_policy: source_policy(always),
            ..Cfg::default()
        },
    );
    assert_eq!(render(&twig, "closure", PhpArray::new()).unwrap(), "a,b");
}

#[test]
fn testNonSandboxedSourcePolicyAllowsNonClosureCallable() {
    let twig = source_policy_env(
        "arrow",
        Cfg {
            filters: &["sort", "join"],
            source_policy: source_policy(never),
            ..Cfg::default()
        },
    );
    let (result, deprecations) = collect(|| render(&twig, "natcase", PhpArray::new()));
    assert_eq!(result.unwrap(), "a,b");
    assert_deprecations(
        &deprecations,
        "Since twig/twig 3.15: Passing a callable that is not a PHP \\Closure as an argument to the \"sort\" filter is deprecated.",
    );
}

#[test]
fn testNeedsIsSandboxedFilterReceivesTrueWhenSandboxed() {
    let twig = env(
        "sandbox_aware",
        Cfg {
            filters: &["sandbox_aware"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "index", PhpArray::new()).unwrap(), "foo:on");
}

#[test]
fn testNeedsIsSandboxedFilterReceivesFalseWhenNotSandboxed() {
    let twig = env("sandbox_aware", Cfg::default());
    assert_eq!(render(&twig, "index", PhpArray::new()).unwrap(), "foo:off");
}

#[test]
fn testNeedsIsSandboxedFilterFollowsSourcePolicy() {
    let twig = source_policy_env(
        "sandbox_aware",
        Cfg {
            filters: &["sandbox_aware"],
            source_policy: source_policy(|name| name == "in"),
            ..Cfg::default()
        },
    );
    assert_eq!(render(&twig, "in", PhpArray::new()).unwrap(), "foo:on");
    assert_eq!(render(&twig, "out", PhpArray::new()).unwrap(), "foo:off");
}

#[test]
fn testNeedsIsSandboxedFunctionWithoutSandboxExtension() {
    let twig = env(
        "no_sandbox",
        Cfg {
            no_sandbox_extension: true,
            ..Cfg::default()
        },
    );
    assert_eq!(render(&twig, "index", PhpArray::new()).unwrap(), "foo:off");
}

#[test]
fn testNeedsIsSandboxedTestReceivesTrueWhenSandboxed() {
    let twig = env(
        "sandbox_aware",
        Cfg {
            tests: &["sandbox_aware"],
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "test", PhpArray::new()).unwrap(), "on");
}

#[test]
fn testNeedsIsSandboxedTestReceivesFalseWhenNotSandboxed() {
    let twig = env(
        "not_sandboxed_test",
        Cfg {
            variant: "not_sandboxed_test",
            ..Cfg::default()
        },
    );
    assert_eq!(render(&twig, "index", PhpArray::new()).unwrap(), "on");
}

#[test]
fn testAlwaysAllowedInSandboxFilterBypassesAllowList() {
    let twig = env("always_allowed", sandboxed());
    assert_eq!(
        render(&twig, "safe_upper", PhpArray::new()).unwrap(),
        "FABIEN"
    );
}

#[test]
fn testAlwaysAllowedInSandboxFilterStillEnforcedWhenFlagNotSet() {
    let twig = env("always_allowed", sandboxed());
    assert_error(
        render(&twig, "gated_upper", PhpArray::new()),
        ErrorKind::SecurityNotAllowedFilter,
        "Filter \"gated_upper\" is not allowed",
    );
}

#[test]
fn testAlwaysAllowedInSandboxFunctionBypassesAllowList() {
    let twig = env("always_allowed", sandboxed());
    assert_eq!(
        render(&twig, "safe_greet", PhpArray::new()).unwrap(),
        "hi fabien"
    );
}

#[test]
fn testAlwaysAllowedInSandboxFunctionStillEnforcedWhenFlagNotSet() {
    let twig = env("always_allowed", sandboxed());
    assert_error(
        render(&twig, "gated_greet", PhpArray::new()),
        ErrorKind::SecurityNotAllowedFunction,
        "Function \"gated_greet\" is not allowed",
    );
}

#[test]
fn testAlwaysAllowedInSandboxTestBypassesAllowList() {
    let twig = env(
        "always_allowed",
        Cfg {
            strict: true,
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "safe_even", PhpArray::new()).unwrap(), "yes");
}

#[test]
fn testAlwaysAllowedInSandboxTestStillEnforcedWhenFlagNotSet() {
    let twig = env(
        "always_allowed",
        Cfg {
            strict: true,
            ..sandboxed()
        },
    );
    assert_error(
        render(&twig, "gated_even", PhpArray::new()),
        ErrorKind::SecurityNotAllowedTest,
        "Test \"gated_even\" is not allowed",
    );
}

#[test]
fn testAlwaysAllowedInSandboxFunctionAlsoCoversRangeOperator() {
    let twig = env(
        "range",
        Cfg {
            variant: "range",
            ..sandboxed()
        },
    );
    assert_eq!(render(&twig, "index", PhpArray::new()).unwrap(), "1");
}

#[test]
fn testAlwaysAllowedInSandboxTagBypassesAllowList() {
    let twig = env("always_allowed", sandboxed());
    assert_eq!(
        render(&twig, "always_allowed_tag", PhpArray::new()).unwrap(),
        "always-allowed"
    );
}

#[test]
fn testAlwaysAllowedInSandboxTagStillEnforcedWhenFlagNotSet() {
    let twig = env("always_allowed", sandboxed());
    assert_error(
        render(&twig, "gated_tag", PhpArray::new()),
        ErrorKind::SecurityNotAllowedTag,
        "Tag \"gated_tag\" is not allowed",
    );
}

#[test]
fn testAlwaysAllowedInSandboxFilterStillEnforcesToStringPolicyOnArguments() {
    let twig = env("always_allowed", sandboxed());
    assert_error(
        render(
            &twig,
            "safe_upper_obj",
            map(vec![("obj", FooObject::value())]),
        ),
        ErrorKind::SecurityNotAllowedMethod,
        &format!("Calling \"__tostring\" method on a \"{FOO}\" object is not allowed"),
    );
}

#[test]
fn testAlwaysAllowedInSandboxFunctionStillEnforcesToStringPolicyOnArguments() {
    let twig = env("always_allowed", sandboxed());
    assert_error(
        render(
            &twig,
            "safe_greet_obj",
            map(vec![("obj", FooObject::value())]),
        ),
        ErrorKind::SecurityNotAllowedMethod,
        &format!("Calling \"__tostring\" method on a \"{FOO}\" object is not allowed"),
    );
}
