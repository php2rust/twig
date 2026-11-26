//! `Twig\Tests\Sandbox\SandboxTest`: the `Sandbox` class, the security
//! checker and the policy checks of a sandboxed render. The templates of
//! each test are compiled by the build script (`sandbox_sets.rs`, from
//! `tests/data/sandbox_templates.json`), so a test takes its environment
//! from a set and does not add callables after the fact.
//!
//! Upstream: twig/twig tests/Sandbox/SandboxTest.php

#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use std::{rc::Rc, sync::Arc};

use php_value::{
    PhpArray,
    value::{PhpObject, Value},
};
use twig::{
    Autoescape, Environment, EnvironmentOptions,
    error::{Error, ErrorKind},
    extension::{Extension, sandbox::SandboxExtension},
    loader::{CompiledArrayLoader, CompiledEntry},
    markup::Markup,
    sandbox::{Sandbox, SecurityChecker, SecurityPolicy, SecurityPolicyInterface},
    source::Source,
};
use twig_integration::{
    sandbox::{SandboxTestObject, set_trusted_environment},
    sandbox_extension::SandboxTestExtension,
};

mod compiled {
    include!(concat!(env!("OUT_DIR"), "/sandbox_sets.rs"));
}

/// The compiled templates of the set `name`.
fn templates(name: &str) -> &'static [CompiledEntry] {
    let (_, set) = compiled::SANDBOX_SETS
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

/// The named arguments of `strictPolicy()`.
#[derive(Default)]
struct Allow<'a> {
    tags: &'a [&'a str],
    filters: &'a [&'a str],
    methods: &'a [(&'a str, &'a [&'a str])],
    properties: &'a [(&'a str, &'a [&'a str])],
    functions: &'a [&'a str],
    tests: &'a [&'a str],
}

/// `self::strictPolicy(...)`.
fn strict_policy(allow: &Allow<'_>) -> Rc<SecurityPolicy> {
    let policy = SecurityPolicy::new(
        allow.tags,
        allow.filters,
        allow.methods,
        allow.properties,
        allow.functions,
        allow.tests,
    );
    policy.set_strict(true);
    Rc::new(policy)
}

fn options(autoescape: Autoescape) -> EnvironmentOptions {
    EnvironmentOptions {
        autoescape,
        ..EnvironmentOptions::default()
    }
}

fn extensions() -> Vec<Rc<dyn Extension>> {
    vec![Rc::new(SandboxTestExtension)]
}

/// `new Sandbox(self::env($templates), $policy)`.
fn sandbox_with(
    set: &str,
    autoescape: Autoescape,
    policy: Rc<dyn SecurityPolicyInterface>,
) -> Result<Sandbox, Error> {
    let sandbox = Sandbox::new(extensions(), options(autoescape), policy)?;
    sandbox
        .environment()
        .set_loader(Rc::new(CompiledArrayLoader {
            templates: templates(set),
        }));
    Ok(sandbox)
}

fn sandbox(set: &str, policy: Rc<dyn SecurityPolicyInterface>) -> Sandbox {
    sandbox_with(set, Autoescape::Off, policy).unwrap()
}

/// `self::env($templates)` without a sandbox.
fn plain_env(set: &str) -> Environment {
    let env = Environment::with_options(extensions(), options(Autoescape::Off)).unwrap();
    env.set_loader(Rc::new(CompiledArrayLoader {
        templates: templates(set),
    }));
    env
}

fn context(entries: &[(&str, Value)]) -> PhpArray {
    let mut context = PhpArray::new();
    for (name, value) in entries {
        context.insert(*name, value.clone());
    }
    context
}

fn s(text: &str) -> Value {
    Value::from(text)
}

fn object() -> Value {
    Value::Object(Arc::new(SandboxTestObject))
}

fn render(sandbox: &Sandbox, name: &str, entries: &[(&str, Value)]) -> Result<String, Error> {
    sandbox
        .render(name.as_bytes(), context(entries))
        .map(|out| String::from_utf8(out).unwrap())
}

#[track_caller]
fn assert_error(result: Result<impl std::fmt::Debug, Error>, kind: ErrorKind, message: &str) {
    let error = result.expect_err("an error");
    assert_eq!(error.kind(), kind, "{error}");
    let text = String::from_utf8_lossy(error.message()).into_owned();
    assert!(
        text.contains(message),
        "\"{text}\" does not contain \"{message}\""
    );
}

#[test]
fn testRendersAllowedConstructs() {
    let sandbox = sandbox(
        "testRendersAllowedConstructs",
        strict_policy(&Allow {
            tags: &["if"],
            filters: &["upper"],
            ..Allow::default()
        }),
    );
    assert_eq!(
        render(
            &sandbox,
            "index",
            &[("greet", Value::Bool(true)), ("name", s("fabien"))]
        )
        .unwrap(),
        "Hello FABIEN"
    );
}

#[test]
fn testNonStrictSecurityPolicyIsRejected() {
    assert_error(
        Sandbox::new(
            extensions(),
            options(Autoescape::Off),
            Rc::new(SecurityPolicy::default()),
        )
        .map(|_| ()),
        ErrorKind::Logic,
        "The sandbox requires a strict security policy, call \"setStrict(true)\" on a dedicated policy for this sandbox.",
    );
}

#[test]
fn testRenderingDirectlyOnTheSandboxEnvironmentIsSandboxed() {
    let sandbox = sandbox(
        "testRenderingDirectlyOnTheSandboxEnvironmentIsSandboxed",
        strict_policy(&Allow::default()),
    );
    assert_error(
        sandbox.environment().render(b"index", PhpArray::new()),
        ErrorKind::SecurityNotAllowedFilter,
        "",
    );
}

#[test]
fn testTheEnvironmentCanBeConfiguredAfterTheSandboxIsCreated() {
    // The `shout` function comes with the environment's extensions: a Rust
    // environment takes its callables when it is made.
    let sandbox = sandbox(
        "testTheEnvironmentCanBeConfiguredAfterTheSandboxIsCreated",
        strict_policy(&Allow {
            functions: &["shout"],
            ..Allow::default()
        }),
    );
    assert_eq!(render(&sandbox, "index", &[]).unwrap(), "HI");
}

/// The anonymous policy of `testCustomSecurityPolicyImplementationsAreAccepted`.
struct FilterRefusingPolicy;

impl SecurityPolicyInterface for FilterRefusingPolicy {
    fn check_security(
        &self,
        _tags: &[Vec<u8>],
        filters: &[Vec<u8>],
        _functions: &[Vec<u8>],
        _tests: &[Vec<u8>],
    ) -> Result<(), Error> {
        match filters.first() {
            Some(filter) => Err(Error::security_not_allowed(
                ErrorKind::SecurityNotAllowedFilter,
                format!(
                    "Filter \"{}\" is not allowed.",
                    String::from_utf8_lossy(filter)
                ),
                filter,
            )),
            None => Ok(()),
        }
    }

    fn check_method_allowed(&self, _object: &dyn PhpObject, _method: &[u8]) -> Result<(), Error> {
        Ok(())
    }

    fn check_property_allowed(
        &self,
        _object: &dyn PhpObject,
        _property: &[u8],
    ) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn testCustomSecurityPolicyImplementationsAreAccepted() {
    let sandbox = sandbox(
        "testCustomSecurityPolicyImplementationsAreAccepted",
        Rc::new(FilterRefusingPolicy),
    );
    assert_eq!(render(&sandbox, "static", &[]).unwrap(), "static text");
    assert_error(
        render(&sandbox, "filtered", &[]),
        ErrorKind::SecurityNotAllowedFilter,
        "",
    );
}

#[test]
fn testDisallowedTagIsRejected() {
    let sandbox = sandbox(
        "testDisallowedTagIsRejected",
        strict_policy(&Allow::default()),
    );
    assert_error(
        render(&sandbox, "index", &[]),
        ErrorKind::SecurityNotAllowedTag,
        "Tag \"if\" is not allowed",
    );
}

#[test]
fn testSecurityCheckerAddsTheSourceToPolicyErrors() {
    let checker = SecurityChecker::new(strict_policy(&Allow::default()), true, None);
    let source = Arc::new(Source::new("{% if 1 %}x{% endif %}", "index"));
    let error = checker
        .check_security(&[b"if".to_vec()], &[], &[], &[], Some(&source))
        .expect_err("Expected SecurityNotAllowedTagError");
    assert_eq!(error.kind(), ErrorKind::SecurityNotAllowedTag);
    assert!(Arc::ptr_eq(error.source_context().unwrap(), &source));
}

#[test]
fn testDisallowedFilterIsRejected() {
    let sandbox = sandbox(
        "testDisallowedFilterIsRejected",
        strict_policy(&Allow::default()),
    );
    assert_error(
        render(&sandbox, "index", &[]),
        ErrorKind::SecurityNotAllowedFilter,
        "Filter \"upper\" is not allowed",
    );
}

#[test]
fn testDisallowedFunctionIsRejected() {
    let sandbox = sandbox(
        "testDisallowedFunctionIsRejected",
        strict_policy(&Allow::default()),
    );
    assert_error(
        render(&sandbox, "index", &[]),
        ErrorKind::SecurityNotAllowedFunction,
        "Function \"max\" is not allowed",
    );
}

#[test]
fn testDisallowedTestIsRejected() {
    let sandbox = sandbox(
        "testDisallowedTestIsRejected",
        strict_policy(&Allow::default()),
    );
    assert_error(
        render(&sandbox, "index", &[]),
        ErrorKind::SecurityNotAllowedTest,
        "Test \"funky\" is not allowed",
    );
}

#[test]
fn testAllowedTestIsAccepted() {
    let sandbox = sandbox(
        "testAllowedTestIsAccepted",
        strict_policy(&Allow {
            tests: &["funky"],
            ..Allow::default()
        }),
    );
    assert_eq!(render(&sandbox, "index", &[]).unwrap(), "y");
}

#[test]
fn testMethodCallsAreGovernedByThePolicy() {
    let name = "testMethodCallsAreGovernedByThePolicy";
    let allowing = sandbox(
        name,
        strict_policy(&Allow {
            methods: &[(SandboxTestObject::CLASS, &["getName"])],
            ..Allow::default()
        }),
    );
    assert_eq!(
        render(&allowing, "index", &[("obj", object())]).unwrap(),
        "fabien"
    );

    let denying = sandbox(name, strict_policy(&Allow::default()));
    assert_error(
        render(&denying, "index", &[("obj", object())]),
        ErrorKind::SecurityNotAllowedMethod,
        "",
    );
}

#[test]
fn testPropertyAccessesAreGovernedByThePolicy() {
    let name = "testPropertyAccessesAreGovernedByThePolicy";
    let allowing = sandbox(
        name,
        strict_policy(&Allow {
            properties: &[(SandboxTestObject::CLASS, &["name"])],
            ..Allow::default()
        }),
    );
    assert_eq!(
        render(&allowing, "index", &[("obj", object())]).unwrap(),
        "fabien"
    );

    let denying = sandbox(name, strict_policy(&Allow::default()));
    assert_error(
        render(&denying, "index", &[("obj", object())]),
        ErrorKind::SecurityNotAllowedProperty,
        "",
    );
}

#[test]
fn testToStringCoercionIsGovernedByThePolicy() {
    let name = "testToStringCoercionIsGovernedByThePolicy";
    let allowing = sandbox(
        name,
        strict_policy(&Allow {
            methods: &[(SandboxTestObject::CLASS, &["__toString"])],
            ..Allow::default()
        }),
    );
    assert_eq!(
        render(&allowing, "index", &[("obj", object())]).unwrap(),
        "object"
    );

    let denying = sandbox(name, strict_policy(&Allow::default()));
    assert_error(
        render(&denying, "index", &[("obj", object())]),
        ErrorKind::SecurityNotAllowedMethod,
        "",
    );
}

#[test]
fn testMarkupValuesCanBePrinted() {
    let sandbox = sandbox(
        "testMarkupValuesCanBePrinted",
        strict_policy(&Allow::default()),
    );
    let markup = Markup::value(b"<b>b</b>".to_vec(), b"UTF-8".to_vec());
    assert_eq!(
        render(&sandbox, "index", &[("m", markup)]).unwrap(),
        "<b>b</b>"
    );
}

#[test]
fn testIncludedTemplatesAreSandboxed() {
    let sandbox = sandbox(
        "testIncludedTemplatesAreSandboxed",
        strict_policy(&Allow {
            functions: &["include"],
            ..Allow::default()
        }),
    );
    assert_error(
        render(&sandbox, "index", &[]),
        ErrorKind::SecurityNotAllowedFilter,
        "Filter \"upper\" is not allowed",
    );
}

/// `provideForeignTemplateWrapperUsages()`: the case name, tags, functions.
const FOREIGN_TEMPLATE_WRAPPER_USAGES: &[(&str, &[&str], &[&str])] = &[
    ("include tag", &["include"], &[]),
    ("include function", &[], &["include"]),
    ("include function fallback", &[], &["include"]),
    ("include_only function", &[], &["include_only"]),
    ("extends tag", &["extends"], &[]),
    ("embed tag", &["embed", "extends"], &[]),
    ("import tag", &["import"], &[]),
    ("from tag", &["from"], &[]),
    ("block function", &[], &["block"]),
];

#[test]
fn testRejectsTemplateWrapperFromAnotherEnvironment() {
    for (case, tags, functions) in FOREIGN_TEMPLATE_WRAPPER_USAGES {
        let name = format!("testRejectsTemplateWrapperFromAnotherEnvironment {case}");
        let foreign = plain_env(&format!("{name} foreign"));
        let sandbox = sandbox(
            &name,
            strict_policy(&Allow {
                tags,
                functions,
                ..Allow::default()
            }),
        );
        let wrapper = foreign.load(b"foreign").unwrap();
        let result = render(
            &sandbox,
            "index",
            &[("foreign", Value::Object(Arc::new(wrapper)))],
        );
        let error = result.expect_err(case);
        assert_eq!(error.kind(), ErrorKind::Runtime, "{case}: {error}");
        let text = String::from_utf8_lossy(error.message()).into_owned();
        assert!(
            text.contains("can only be used with the \"Twig\\Environment\" that created it"),
            "{case}: {text}"
        );
    }
}

#[test]
fn testTheExtendsTagMustBeAllowed() {
    let name = "testTheExtendsTagMustBeAllowed";
    let allowing = sandbox(
        name,
        strict_policy(&Allow {
            tags: &["extends"],
            ..Allow::default()
        }),
    );
    assert_eq!(render(&allowing, "index", &[]).unwrap(), "layout content");

    let denying = sandbox(name, strict_policy(&Allow::default()));
    assert_error(
        render(&denying, "index", &[]),
        ErrorKind::SecurityNotAllowedTag,
        "Tag \"extends\" is not allowed",
    );
}

#[test]
fn testTheUseTagMustBeAllowed() {
    let allowing = sandbox(
        "testTheUseTagMustBeAllowed",
        strict_policy(&Allow {
            tags: &["use", "block"],
            functions: &["block"],
            ..Allow::default()
        }),
    );
    assert_eq!(render(&allowing, "index", &[]).unwrap(), "trait content");

    let denying = sandbox(
        "testTheUseTagMustBeAllowed denying",
        strict_policy(&Allow::default()),
    );
    assert_error(
        render(&denying, "index", &[]),
        ErrorKind::SecurityNotAllowedTag,
        "Tag \"use\" is not allowed",
    );
}

#[test]
fn testTheUseTagIsCheckedWithoutCheckingTheRestOfThePolicy() {
    // "middle" is only reachable as a trait, and the block carrying the
    // forbidden filter is overridden by "index", so it never renders.
    let sandbox = sandbox(
        "testTheUseTagIsCheckedWithoutCheckingTheRestOfThePolicy",
        strict_policy(&Allow {
            tags: &["use", "block"],
            ..Allow::default()
        }),
    );
    assert_eq!(render(&sandbox, "index", &[]).unwrap(), "SAFE");
}

#[test]
fn testTheUseTagIsCheckedBeforeTheTraitTemplateIsLoaded() {
    let sandbox = sandbox(
        "testTheUseTagIsCheckedBeforeTheTraitTemplateIsLoaded",
        strict_policy(&Allow {
            tags: &["block"],
            functions: &["block"],
            ..Allow::default()
        }),
    );
    assert_error(
        render(&sandbox, "index", &[]),
        ErrorKind::SecurityNotAllowedTag,
        "Tag \"use\" is not allowed",
    );
}

/// `ThrowingOnUseSecurityPolicy`.
struct ThrowingOnUseSecurityPolicy;

impl SecurityPolicyInterface for ThrowingOnUseSecurityPolicy {
    fn check_security(
        &self,
        tags: &[Vec<u8>],
        _filters: &[Vec<u8>],
        _functions: &[Vec<u8>],
        _tests: &[Vec<u8>],
    ) -> Result<(), Error> {
        if tags.iter().any(|tag| tag == b"use") {
            return Err(Error::exception(
                "RuntimeException",
                b"Policy backend unreachable.".to_vec(),
            ));
        }
        Ok(())
    }

    fn check_method_allowed(&self, _object: &dyn PhpObject, _method: &[u8]) -> Result<(), Error> {
        Ok(())
    }

    fn check_property_allowed(
        &self,
        _object: &dyn PhpObject,
        _property: &[u8],
    ) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn testAPolicyFailureWhileResolvingTraitsKeepsItsTwigContext() {
    // The port keeps no previous exception, so `getPrevious()` is not
    // checked; the message holds the wrapped exception's message.
    let sandbox = sandbox(
        "testAPolicyFailureWhileResolvingTraitsKeepsItsTwigContext",
        Rc::new(ThrowingOnUseSecurityPolicy),
    );
    let error =
        render(&sandbox, "index", &[]).expect_err("The policy failure should have been reported.");
    assert_eq!(error.kind(), ErrorKind::Runtime, "{error}");
    let text = String::from_utf8_lossy(error.message()).into_owned();
    assert!(text.contains("Policy backend unreachable"), "{text}");
    assert_eq!(error.source_context().unwrap().name(), "index");
    assert_eq!(error.template_line(), 2);
}

#[test]
fn testARenderOnAnotherEnvironmentDuringASandboxedRenderIsNotSandboxed() {
    let name = "testARenderOnAnotherEnvironmentDuringASandboxedRenderIsNotSandboxed";
    set_trusted_environment(Some(Rc::new(plain_env(&format!("{name} app")))));
    let sandbox = sandbox(
        name,
        strict_policy(&Allow {
            functions: &["render_trusted"],
            ..Allow::default()
        }),
    );
    let result = render(&sandbox, "index", &[]);
    set_trusted_environment(None);
    assert_eq!(result.unwrap(), "OK");
}

#[test]
fn testAnEnvironmentWithASandboxExtensionIsRejected() {
    let mut extensions = extensions();
    extensions.push(Rc::new(SandboxExtension::new(
        strict_policy(&Allow::default()),
        false,
        None,
    )));
    assert_error(
        Sandbox::new(
            extensions,
            options(Autoescape::Off),
            strict_policy(&Allow::default()),
        )
        .map(|_| ()),
        ErrorKind::Logic,
        "must be dedicated to it",
    );
}

#[test]
fn testAutoEscapingRequiresTheEscapeFilterToBeAllowed() {
    let name = "testAutoEscapingRequiresTheEscapeFilterToBeAllowed";
    let html = || Autoescape::Strategy(b"html".to_vec());
    let denying = sandbox_with(name, html(), strict_policy(&Allow::default())).unwrap();
    assert_error(
        render(&denying, "index", &[("v", s("a&b"))]),
        ErrorKind::SecurityNotAllowedFilter,
        "Filter \"escape\" is not allowed",
    );

    let allowing = sandbox_with(
        name,
        html(),
        strict_policy(&Allow {
            filters: &["escape"],
            ..Allow::default()
        }),
    )
    .unwrap();
    assert_eq!(
        render(&allowing, "index", &[("v", s("a&b"))]).unwrap(),
        "a&amp;b"
    );
}

fn display_sandbox() -> Sandbox {
    sandbox(
        "testDisplay",
        strict_policy(&Allow {
            filters: &["upper"],
            ..Allow::default()
        }),
    )
}

/// `display()` writes what `render()` returns: the port has no output
/// buffer, so it renders.
#[test]
fn testDisplay() {
    assert_eq!(
        render(&display_sandbox(), "index", &[("name", s("fabien"))]).unwrap(),
        "Hello FABIEN"
    );
}

/// `stream()` yields the chunks of `render()`; the port renders.
#[test]
fn testStream() {
    assert_eq!(
        render(&display_sandbox(), "index", &[("name", s("fabien"))]).unwrap(),
        "Hello FABIEN"
    );
}

fn block_sandbox() -> Sandbox {
    sandbox(
        "testRenderBlock",
        strict_policy(&Allow {
            tags: &["block"],
            filters: &["upper"],
            ..Allow::default()
        }),
    )
}

fn render_block(sandbox: &Sandbox) -> String {
    let out = sandbox
        .render_block(b"blocks", b"greeting", context(&[("name", s("fabien"))]))
        .unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn testRenderBlock() {
    assert_eq!(render_block(&block_sandbox()), "Hello FABIEN");
}

/// `displayBlock()`: the port has no output buffer, so it renders.
#[test]
fn testDisplayBlock() {
    assert_eq!(render_block(&block_sandbox()), "Hello FABIEN");
}

/// `streamBlock()`: the port renders.
#[test]
fn testStreamBlock() {
    assert_eq!(render_block(&block_sandbox()), "Hello FABIEN");
}
