//! Port of Twig\Compiler.php and of the `compile()` methods of the node
//! classes: a module node becomes Rust source code instead of a PHP
//! class.
//!
//! Each template becomes a Rust module with a type implementing
//! [`crate::template::CompiledTemplate`], one function per block and one
//! per macro; the generated code calls [`crate::runtime`] (imported as
//! `rt`). Expressions compile to Rust expressions of type
//! `php_value::Value` (errors leave with `?`), statements to Rust
//! statements writing to `out`. PHP's debug info (template line of each
//! PHP line) becomes the `__line` variable of each generated function,
//! which an error that has no line gets ([`crate::runtime::guess`]).
//!
//! The callables (filters, functions, tests) are called through their
//! Rust implementation: [`TwigCallable::rust`] or, for the core and
//! escaper extensions, [`core_callable`].
//!
//! Not ported: the sandbox nodes (`SandboxNode`, `CheckSecurityNode`,
//! `CheckToStringNode`).
//!
//! Upstream: twig/twig src/Compiler.php src/Node/ModuleNode.php src/Node/BlockNode.php src/Node/MacroNode.php src/Node/ForNode.php src/Node/IfNode.php src/Node/SetNode.php src/Node/IncludeNode.php src/Node/EmbedNode.php src/Node/WithNode.php src/Node/ImportNode.php src/Node/PrintNode.php src/Node/TextNode.php src/Node/CaptureNode.php src/Node/Expression/CallExpression.php src/Node/Expression/GetAttrExpression.php src/Node/Expression/NameExpression.php
//! Upstream: twig/twig src/Node/Expression/FunctionNode/EnumFunction.php src/Node/Expression/FunctionNode/EnumCasesFunction.php
//! Upstream: twig/twig src/Node/Expression/Binary/AddBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/AndBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/BitwiseAndBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/BitwiseOrBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/BitwiseXorBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/ConcatBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/DivBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/EndsWithBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/EqualBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/FloorDivBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/GreaterBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/GreaterEqualBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/HasEveryBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/HasSomeBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/InBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/LessBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/LessEqualBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/MatchesBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/ModBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/MulBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/NotEqualBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/NotInBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/NotSameAsBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/OrBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/PowerBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/RangeBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/SameAsBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/SpaceshipBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/StartsWithBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/SubBinary.php
//! Upstream: twig/twig src/Node/Expression/Binary/XorBinary.php
//! Upstream: twig/twig src/Node/Expression/Test/ConstantTest.php
//! Upstream: twig/twig src/Node/Expression/Test/DivisiblebyTest.php
//! Upstream: twig/twig src/Node/Expression/Test/EvenTest.php src/Node/Expression/Test/NullTest.php
//! Upstream: twig/twig src/Node/Expression/Test/OddTest.php src/Node/Expression/Test/SameasTest.php
//! Upstream: twig/twig src/Node/Expression/Unary/NegUnary.php
//! Upstream: twig/twig src/Node/Expression/Unary/NotUnary.php
//! Upstream: twig/twig src/Node/Expression/Unary/PosUnary.php
//! Upstream: twig/twig src/Node/Expression/Unary/SpreadUnary.php
//! Upstream: twig/twig src/Node/Expression/Unary/StringCastUnary.php
//! Upstream: twig/twig src/Node/Expression/Variable/AssignContextVariable.php
//! Upstream: twig/twig src/Node/Expression/Variable/ContextVariable.php
//! Upstream: twig/twig src/Node/Expression/Variable/LocalVariable.php
//! Upstream: twig/twig src/Node/Expression/ConditionalExpression.php
//! Upstream: twig/twig src/Node/Expression/InlinePrint.php
//! Upstream: twig/twig src/Node/Expression/MethodCallExpression.php
//! Upstream: twig/twig src/Node/Expression/NullCoalesceExpression.php
//! Upstream: twig/twig src/Node/Expression/VariadicExpression.php

use std::fmt::Write as _;

use crate::{
    callable::TwigCallable,
    environment::Environment,
    error::Error,
    node::{
        Attr, Key, Node, NodeClass,
        expression::{RESERVED_NAME_PREFIX, constant},
    },
    util::callable_arguments::{Parameter, extract_arguments},
};

type R<T = String> = Result<T, Error>;

/// The name of the Rust module of a template (`getTemplateClass()`).
#[must_use]
pub fn module_name(name: &[u8], index: Option<i64>) -> String {
    // FNV-1a: stable across builds and platforms
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    match index {
        Some(i) => format!("t_{hash:016x}___{i}"),
        None => format!("t_{hash:016x}"),
    }
}

/// A Rust byte string literal.
#[must_use]
pub fn byte_lit(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 3);
    s.push_str("b\"");
    for &b in bytes {
        match b {
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7e => s.push(char::from(b)),
            _ => {
                let _ = write!(s, "\\x{b:02x}");
            }
        }
    }
    s.push('"');
    s
}

/// A Rust string literal.
fn str_lit(text: &str) -> String {
    format!("\"{}\"", text.escape_default())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The `rt::Value` expression of a constant (`repr()`).
pub fn repr(value: &Attr) -> R {
    Ok(match value {
        Attr::Null => "rt::Value::Null".to_owned(),
        Attr::Bool(b) => format!("rt::Value::Bool({b})"),
        Attr::Int(i) if *i == i64::MIN => "rt::Value::Int(i64::MIN)".to_owned(),
        Attr::Int(i) => format!("rt::Value::Int({i})"),
        Attr::Float(f) => format!("rt::Value::Float(f64::from_bits({:#x}))", f.to_bits()),
        Attr::Str(s) => format!("rt::s({})", byte_lit(s)),
        Attr::Array(items) => {
            let mut code = String::from("rt::ArrayBuilder::new()");
            for (key, value) in items {
                let key = match key {
                    Key::Int(i) => repr(&Attr::Int(*i))?,
                    Key::Str(s) => repr(&Attr::Str(s.clone()))?,
                };
                let _ = write!(code, ".key(&{key}, {})?", repr(value)?);
            }
            code.push_str(".done()");
            code
        }
        Attr::Callable(_) | Attr::Node(_) => {
            return Err(Error::logic("A callable or a node is not a PHP value."));
        }
    })
}

fn unsupported(node: &Node, what: &str) -> Error {
    Error::syntax(
        format!("{what} is not supported by the Rust compiler yet."),
        node.line(),
        node.source_context(),
    )
}

fn name_of(node: &Node) -> Vec<u8> {
    node.attr("name")
        .and_then(|a| a.to_php_string())
        .unwrap_or_default()
}

/// The Rust implementation of a callable of the core and escaper
/// extensions, by PHP callable name, with the PHP parameters (in PHP
/// declaration order) the argument extraction needs.
#[must_use]
#[allow(clippy::too_many_lines)] // a table
pub fn core_callable(php: &str) -> Option<(&'static str, Vec<Parameter>)> {
    use Parameter as P;
    let null = |n: &str| P::optional_null(n);
    let def = |n: &str, v: Attr| P::with_default(n, v);
    let req = |n: &str| P::required(n);
    let charset = || req("charset");
    let env = || req("env");
    let sandboxed = || req("isSandboxed");
    let path = |m: &'static str| m;
    Some(match php {
        "Twig\\Extension\\CoreExtension::sprintf" => (
            path("rt::core::sprintf"),
            vec![req("format"), P::variadic("values")],
        ),
        "Twig\\Extension\\CoreExtension::replace" => {
            (path("rt::core::replace"), vec![req("str"), req("from")])
        }
        "Twig\\Extension\\CoreExtension::formatNumber" => (
            path("rt::core::format_number"),
            vec![
                req("number"),
                null("decimal"),
                null("decimalPoint"),
                null("thousandSep"),
            ],
        ),
        "Twig\\Extension\\CoreExtension::formatDate" => (
            path("rt::core::format_date"),
            vec![req("date"), null("format"), null("timezone")],
        ),
        "Twig\\Extension\\CoreExtension::modifyDate" => (
            path("rt::core::modify_date"),
            vec![req("date"), req("modifier")],
        ),
        "Twig\\Extension\\CoreExtension::convertDate" => (
            path("rt::core::convert_date"),
            vec![null("date"), null("timezone")],
        ),
        "abs" => (path("rt::core::abs"), vec![req("num").coercion_safe()]),
        "Twig\\Extension\\CoreExtension::round" => (
            path("rt::core::round"),
            vec![
                req("value"),
                def("precision", Attr::Int(0)),
                def("method", Attr::str("common")),
            ],
        ),
        "Twig\\Extension\\CoreExtension::urlencode" => {
            (path("rt::core::urlencode"), vec![req("url")])
        }
        "json_encode" => (
            path("rt::core::json_encode"),
            vec![
                req("value"),
                def("flags", Attr::Int(0)).coercion_safe(),
                def("depth", Attr::Int(512)).coercion_safe(),
            ],
        ),
        "Twig\\Extension\\CoreExtension::titleCase" => {
            (path("rt::core::title_case"), vec![charset(), req("string")])
        }
        "Twig\\Extension\\CoreExtension::capitalize" => {
            (path("rt::core::capitalize"), vec![charset(), req("string")])
        }
        "Twig\\Extension\\CoreExtension::upper" => {
            (path("rt::core::upper"), vec![charset(), req("string")])
        }
        "Twig\\Extension\\CoreExtension::lower" => {
            (path("rt::core::lower"), vec![charset(), req("string")])
        }
        "Twig\\Extension\\CoreExtension::convertEncoding" => (
            path("rt::core::convert_encoding"),
            vec![req("string"), req("to"), req("from")],
        ),
        "Twig\\Extension\\CoreExtension::striptags" => (
            path("rt::core::striptags"),
            vec![req("string"), null("allowable_tags")],
        ),
        "Twig\\Extension\\CoreExtension::trim" => (
            path("rt::core::trim"),
            vec![
                req("string"),
                null("characterMask"),
                def("side", Attr::str("both")),
            ],
        ),
        "Twig\\Extension\\CoreExtension::nl2br" => (path("rt::core::nl2br"), vec![req("string")]),
        "Twig\\Extension\\CoreExtension::spaceless" => {
            (path("rt::core::spaceless"), vec![req("content")])
        }
        "Twig\\Extension\\CoreExtension::join" => (
            path("rt::core::join"),
            vec![req("value"), def("glue", Attr::str("")), null("and")],
        ),
        "Twig\\Extension\\CoreExtension::split" => (
            path("rt::core::split"),
            vec![charset(), req("value"), req("delimiter"), null("limit")],
        ),
        "Twig\\Extension\\CoreExtension::sort" => (
            path("rt::core::sort"),
            vec![env(), sandboxed(), req("array"), null("arrow")],
        ),
        "Twig\\Extension\\CoreExtension::merge" => {
            (path("rt::core::merge"), vec![P::variadic("arrays")])
        }
        "Twig\\Extension\\CoreExtension::batch" => (
            path("rt::core::batch"),
            vec![
                req("items"),
                req("size"),
                null("fill"),
                def("preserveKeys", Attr::Bool(true)),
            ],
        ),
        "Twig\\Extension\\CoreExtension::column" => (
            path("rt::core::column"),
            vec![env(), sandboxed(), req("array"), req("name"), null("index")],
        ),
        "Twig\\Extension\\CoreExtension::filter" => (
            path("rt::core::filter"),
            vec![env(), sandboxed(), req("array"), req("arrow")],
        ),
        "Twig\\Extension\\CoreExtension::map" => (
            path("rt::core::map"),
            vec![env(), sandboxed(), req("array"), req("arrow")],
        ),
        "Twig\\Extension\\CoreExtension::reduce" => (
            path("rt::core::reduce"),
            vec![
                env(),
                sandboxed(),
                req("array"),
                req("arrow"),
                null("initial"),
            ],
        ),
        "Twig\\Extension\\CoreExtension::find" => (
            path("rt::core::find_filter"),
            vec![env(), sandboxed(), req("array"), req("arrow")],
        ),
        "Twig\\Extension\\CoreExtension::reverse" => (
            path("rt::core::reverse"),
            vec![
                charset(),
                req("item"),
                def("preserveKeys", Attr::Bool(false)),
            ],
        ),
        "Twig\\Extension\\CoreExtension::shuffle" => {
            (path("rt::core::shuffle"), vec![charset(), req("item")])
        }
        "Twig\\Extension\\CoreExtension::length" => {
            (path("rt::core::length"), vec![charset(), req("thing")])
        }
        "Twig\\Extension\\CoreExtension::slice" => (
            path("rt::core::slice"),
            vec![
                charset(),
                req("item"),
                req("start"),
                null("length"),
                def("preserveKeys", Attr::Bool(false)),
            ],
        ),
        "Twig\\Extension\\CoreExtension::first" => {
            (path("rt::core::first"), vec![charset(), req("item")])
        }
        "Twig\\Extension\\CoreExtension::last" => {
            (path("rt::core::last"), vec![charset(), req("item")])
        }
        "Twig\\Extension\\CoreExtension::default" => (
            path("rt::core::default"),
            vec![req("value"), def("default", Attr::str(""))],
        ),
        "Twig\\Extension\\CoreExtension::keys" => (path("rt::core::keys"), vec![req("array")]),
        "Twig\\Extension\\CoreExtension::invoke" => (
            path("rt::core::invoke"),
            vec![req("arrow").coercion_safe(), P::variadic("arguments")],
        ),
        "max" => (
            path("rt::core::max"),
            vec![req("value"), P::variadic("values")],
        ),
        "min" => (
            path("rt::core::min"),
            vec![req("value"), P::variadic("values")],
        ),
        "range" => (
            path("rt::core::range"),
            vec![
                req("start"),
                req("end"),
                def("step", Attr::Int(1)).coercion_safe(),
            ],
        ),
        "Twig\\Extension\\CoreExtension::enum" => (path("rt::core::enum_first"), vec![req("enum")]),
        "Twig\\Extension\\CoreExtension::enumCases" => {
            (path("rt::core::enum_cases"), vec![req("enum")])
        }
        "Twig\\Extension\\CoreExtension::constant" => (
            path("rt::core::constant"),
            vec![
                req("constant"),
                null("object"),
                def("checkDefined", Attr::Bool(false)).coercion_safe(),
            ],
        ),
        "Twig\\Extension\\CoreExtension::cycle" => (
            path("rt::core::cycle"),
            vec![req("values"), req("position")],
        ),
        "Twig\\Extension\\CoreExtension::random" => (
            path("rt::core::random"),
            vec![charset(), null("values"), null("max")],
        ),
        "Twig\\Extension\\CoreExtension::include" => (
            path("rt::core::include"),
            vec![
                env(),
                req("context"),
                req("template"),
                def("variables", Attr::Array(Vec::new())),
                def("withContext", Attr::Bool(true)),
                def("ignoreMissing", Attr::Bool(false)),
                def("sandboxed", Attr::Bool(false)),
            ],
        ),
        "Twig\\Extension\\CoreExtension::includeOnly" => (
            path("rt::core::include_only"),
            vec![
                env(),
                req("template"),
                P::array_default("variables"),
                def("ignoreMissing", Attr::Bool(false)).coercion_safe(),
            ],
        ),
        "Twig\\Extension\\DebugExtension::dump" => (
            path("rt::debug::dump"),
            vec![env(), req("context"), P::variadic("vars")],
        ),
        "Twig\\Extension\\StringLoaderExtension::templateFromString" => (
            path("rt::string_loader::template_from_string"),
            vec![env(), req("template"), null("name")],
        ),
        "Twig\\Extension\\CoreExtension::source" => (
            path("rt::core::source"),
            vec![env(), req("name"), def("ignoreMissing", Attr::Bool(false))],
        ),
        "Twig\\Extension\\CoreExtension::testEmpty" => {
            (path("rt::core::test_empty"), vec![req("value")])
        }
        "is_iterable" => (path("rt::core::is_iterable"), vec![req("value")]),
        "Twig\\Extension\\CoreExtension::testSequence" => {
            (path("rt::core::test_sequence"), vec![req("value")])
        }
        "Twig\\Extension\\CoreExtension::testMapping" => {
            (path("rt::core::test_mapping"), vec![req("value")])
        }
        "Twig\\Runtime\\EscaperRuntime::escape" => (
            path("rt::core::escape"),
            vec![
                req("string"),
                def("strategy", Attr::str("html")),
                null("charset"),
                def("autoescape", Attr::Bool(false)).coercion_safe(),
            ],
        ),
        _ => return None,
    })
}

/// The first node of `class` in a tree.
fn find_class(node: &Node, class: NodeClass) -> Option<Node> {
    if node.is(class) {
        return Some(node.clone());
    }
    node.child_nodes()
        .iter()
        .find_map(|child| find_class(child, class))
}

/// The `(name, line)` list of a `CheckSecurityNode` attribute.
fn used_list(check: &Node, name: &str) -> (String, Option<i64>) {
    let mut items = Vec::new();
    let mut use_line = None;
    if let Some(Attr::Array(used)) = check.attr(name) {
        for (key, line) in used {
            let line = line.as_int().unwrap_or(0);
            let key = key.to_php_string();
            if key == b"use" {
                use_line = Some(line);
            }
            items.push(format!("(&{}[..], {line})", byte_lit(&key)));
        }
    }
    (format!("&[{}]", items.join(", ")), use_line)
}

/// `CheckSecurityNode::compile()`: the template's `checkTraitsAllowed()`
/// (with a `use` tag) and `ensureSecurityChecked()`.
fn compile_check_security(check: &Node, guessed_line: i64) -> String {
    let (tags, use_line) = used_list(check, "usedTags");
    let (filters, _) = used_list(check, "usedFilters");
    let (functions, _) = used_list(check, "usedFunctions");
    let (tests, _) = used_list(check, "usedTests");
    let mut out = String::new();
    if let Some(line) = use_line {
        let _ = writeln!(
            out,
            "        fn check_traits_allowed(&self, this: &rt::Rc<rt::Template>, env: &rt::Environment) -> Result<(), rt::Error> {{\n            rt::guess(rt::sandbox::check_traits_allowed(env, this.source(), {line}), {guessed_line}, this)\n        }}\n"
        );
    }
    let _ = writeln!(
        out,
        "        fn ensure_security_checked(&self, this: &rt::Rc<rt::Template>, env: &rt::Environment) -> Result<(), rt::Error> {{\n            rt::guess(rt::sandbox::ensure_security_checked(env, this.source(), {tags}, {filters}, {functions}, {tests}), {guessed_line}, this)\n        }}\n"
    );
    out
}

/// The Rust path and PHP parameters of a callable.
fn resolve_callable(node: &Node, callable: &TwigCallable) -> R<(String, Vec<Parameter>)> {
    if let Some(rust) = callable.rust_callable() {
        return Ok((rust.path.clone(), rust.parameters.clone()));
    }
    if let Some(php) = callable.callable()
        && let Some((path, params)) = core_callable(php)
    {
        return Ok((path.to_owned(), params));
    }
    Err(unsupported(
        node,
        &format!(
            "The {} \"{}\" (no Rust implementation)",
            callable.callable_type().as_str(),
            String::from_utf8_lossy(callable.name())
        ),
    ))
}

/// What the function being compiled has in scope.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// A method with `out`, `blocks`, `macros`, `context` and `__line`.
    Method,
    /// An arrow function: no `out`, no `__line`.
    Arrow,
}

/// `Twig\Compiler`.
pub struct Compiler<'a> {
    env: &'a Environment,
    /// The output of the function being compiled.
    source: String,
    indentation: usize,
    var_name_salt: usize,
    /// The template lines of the `__line` assignments, patched once the
    /// statement is compiled (`__line = \0N\0;`).
    lines: Vec<i64>,
    current_line_slot: Option<usize>,
    /// The template line of the last debug info of the module: PHP's line
    /// guess for an error of the methods generated after it
    /// (`checkSecurity()`, `checkTraitsAllowed()` at the end of the class).
    last_debug_line: i64,
    scope: Scope,
    /// The functions of the module (blocks, macros), after the impl.
    functions: Vec<String>,
}

const LINE_MARK: char = '\u{1}';

impl<'a> Compiler<'a> {
    pub fn new(env: &'a Environment) -> Self {
        Self {
            env,
            source: String::new(),
            indentation: 0,
            var_name_salt: 0,
            lines: Vec::new(),
            current_line_slot: None,
            last_debug_line: -1,
            scope: Scope::Method,
            functions: Vec::new(),
        }
    }

    #[must_use]
    pub fn environment(&self) -> &Environment {
        self.env
    }

    /// `getVarName()`.
    pub fn var_name(&mut self) -> String {
        self.var_name_salt += 1;
        format!("_v{}", self.var_name_salt)
    }

    /// `write()`: indented text.
    fn write(&mut self, text: &str) -> &mut Self {
        for _ in 0..self.indentation {
            self.source.push_str("    ");
        }
        self.source.push_str(text);
        self
    }

    fn line(&mut self, text: &str) -> &mut Self {
        self.write(text);
        self.source.push('\n');
        self
    }

    fn indent(&mut self) {
        self.indentation += 1;
    }

    fn outdent(&mut self) {
        self.indentation = self.indentation.saturating_sub(1);
    }

    /// `addDebugInfo()` at the start of a statement: `__line = N;`.
    fn add_debug_info(&mut self, node: &Node) {
        if self.scope == Scope::Arrow {
            return;
        }
        let slot = self.lines.len();
        self.lines.push(node.line());
        self.current_line_slot = Some(slot);
        self.last_debug_line = node.line();
        self.line(&format!("__line = {LINE_MARK}{slot}{LINE_MARK};"));
    }

    /// `addDebugInfo()` inside an expression: PHP maps the whole PHP line
    /// to the last node it was called for.
    fn expression_debug_info(&mut self, node: &Node) {
        if let Some(slot) = self.current_line_slot
            && let Some(line) = self.lines.get_mut(slot)
        {
            *line = node.line();
            if slot + 1 == self.lines.len() {
                self.last_debug_line = node.line();
            }
        }
    }

    /// Replaces the line marks of a compiled function.
    fn patch_lines(&self, code: &str) -> String {
        let mut out = String::with_capacity(code.len());
        let mut parts = code.split(LINE_MARK);
        if let Some(first) = parts.next() {
            out.push_str(first);
        }
        while let Some(slot) = parts.next() {
            let line = slot
                .parse::<usize>()
                .ok()
                .and_then(|i| self.lines.get(i))
                .copied()
                .unwrap_or(-1);
            let _ = write!(out, "{line}");
            if let Some(rest) = parts.next() {
                out.push_str(rest);
            }
        }
        out
    }

    /// `compile()`: the Rust code of a module node and of its embedded
    /// templates.
    pub fn compile(&mut self, module: &Node) -> R {
        let mut code = self.compile_module(module)?;
        for template in embedded_modules(module) {
            let mut compiler = Compiler::new(self.env);
            code.push_str(&compiler.compile_module(&template)?);
        }
        Ok(code)
    }

    /// Takes the code written since `start`.
    fn take_from(&mut self, start: usize) -> String {
        self.source.split_off(start)
    }

    /// Compiles a statement body into a string, at the given indentation.
    fn body_code(&mut self, node: &Node, indentation: usize) -> R {
        let saved = std::mem::replace(&mut self.indentation, indentation);
        let start = self.source.len();
        let result = self.compile_statement(node);
        let code = self.take_from(start);
        self.indentation = saved;
        result?;
        Ok(code)
    }

    #[allow(clippy::too_many_lines)]
    fn compile_module(&mut self, module: &Node) -> R {
        let source = module
            .source_context()
            .ok_or_else(|| Error::logic("A module node has a source."))?;
        let name = source.name().as_bytes().to_vec();
        let index = module.attr("index").and_then(|a| a.as_int());
        let module_name = module_name(&name, index);
        let mut out = String::new();
        let _ = writeln!(out, "/* {} */", source.name().replace("*/", "* /"));
        out.push_str("#[allow(clippy::all, clippy::pedantic, clippy::nursery, clippy::restriction, non_snake_case, unused_mut, unused_variables, unused_assignments, unused_braces, unused_parens, unreachable_code, dead_code)]\n");
        let _ = writeln!(out, "pub mod {module_name} {{");
        out.push_str("    use ::twig::runtime as rt;\n\n");
        out.push_str("    pub struct Template;\n");
        out.push_str("    pub static TEMPLATE: Template = Template;\n\n");
        out.push_str("    impl rt::CompiledTemplate for Template {\n");
        let _ = writeln!(
            out,
            "        fn template_name(&self) -> &'static [u8] {{\n            {}\n        }}\n",
            byte_lit(&name)
        );
        let code: &[u8] = if self.env.is_debug() {
            source.code()
        } else {
            b""
        };
        let _ = writeln!(
            out,
            "        fn source_context(&self) -> rt::Source {{\n            rt::Source::with_path({}.to_vec(), {}, {}.to_vec())\n        }}\n",
            byte_lit(code),
            str_lit(source.name()),
            byte_lit(source.path())
        );
        if !is_traitable(module)? {
            out.push_str(
                "        fn is_traitable(&self) -> bool {\n            false\n        }\n\n",
            );
        }
        if let Some(strategy) = module.attr("strategy")
            && !matches!(strategy, Attr::Bool(false))
        {
            let strategy = strategy.to_php_string().unwrap_or_default();
            let _ = writeln!(
                out,
                "        fn default_escape_strategy(&self) -> Option<&'static [u8]> {{\n            Some({})\n        }}\n",
                byte_lit(&strategy)
            );
        }

        // constructor
        let constructor = self.compile_constructor(module)?;
        out.push_str(&constructor);

        // doGetParent
        if let Some(parent) = module.node("parent") {
            let body = self.method(|c| {
                c.add_debug_info(&parent);
                let expr = c.expr(&parent)?;
                c.line(&format!("let __p = {expr};"));
                let line = parent.line();
                if parent.is(NodeClass::ConstantExpression) {
                    c.line("if let Some(p) = this.fixed_parent() {");
                    c.line("    return Ok(Some(p));");
                    c.line("}");
                    c.line(&format!("let __t = this.load(env, &__p, {line}, None)?;"));
                    c.line("this.set_parent(Some(rt::Rc::clone(&__t)));");
                    c.line("Ok(Some(__t))");
                } else {
                    c.line(&format!("Ok(Some(this.load(env, &__p, {line}, None)?))"));
                }
                Ok(())
            })?;
            out.push_str("        fn do_get_parent(&self, this: &rt::Rc<rt::Template>, env: &rt::Environment, context: &rt::Context) -> Result<Option<rt::Rc<rt::Template>>, rt::Error> {\n");
            out.push_str("            let mut context = context.clone();\n");
            out.push_str("            let blocks = &rt::Blocks::new();\n");
            out.push_str(&wrap_method(&body, "Option<rt::Rc<rt::Template>>"));
            out.push_str("        }\n\n");
        }

        // doDisplay
        let display = self.method(|c| {
            c.compile_statement(&module.get_node("display_start")?)?;
            c.compile_statement(&module.get_node("body")?)?;
            if let Some(parent) = module.node("parent") {
                c.add_debug_info(&parent);
                let line = parent.line();
                if parent.is(NodeClass::ConstantExpression) {
                    let expr = c.expr(&parent)?;
                    c.line(&format!("let __p = this.load(env, &{expr}, {line}, None)?;"));
                    c.line("this.set_parent(Some(rt::Rc::clone(&__p)));");
                } else {
                    c.line("let Some(__p) = this.get_parent(env, &context)? else {");
                    c.line("    return Err(rt::Error::exception(\"Error\", \"Call to a member function unwrap() on false\"));");
                    c.line("};");
                }
                c.line("__p.display(env, context.clone(), &this.blocks().merge(blocks), out)?;");
            }
            c.compile_statement(&module.get_node("display_end")?)?;
            c.line("Ok(())");
            Ok(())
        })?;
        out.push_str("        fn do_display(&self, this: &rt::Rc<rt::Template>, env: &rt::Environment, context: rt::Context, blocks: &rt::Blocks, out: &mut Vec<u8>) -> Result<(), rt::Error> {\n");
        out.push_str("            let mut context = context;\n");
        out.push_str(&wrap_method(&display, "()"));
        out.push_str("        }\n\n");

        // blocks
        let blocks = module.get_node("blocks")?;
        out.push_str("        fn call_block(&self, name: &[u8], this: &rt::Rc<rt::Template>, env: &rt::Environment, context: rt::Context, blocks: &rt::Blocks, out: &mut Vec<u8>) -> Result<(), rt::Error> {\n");
        out.push_str("            match name {\n");
        for (key, block) in blocks.children() {
            let block_name = key.to_php_string();
            let _ = writeln!(
                out,
                "                {} => block_{}(this, env, context, blocks, out),",
                byte_lit(&block_name),
                hex(&block_name)
            );
            let function = self.compile_block(&block_name, &block)?;
            self.functions.push(function);
        }
        out.push_str("                _ => Err(rt::Error::logic(format!(\"Block \\\"{}\\\" is not a method of this template.\", String::from_utf8_lossy(name)))),\n");
        out.push_str("            }\n        }\n\n");

        // macros
        let macros = module.get_node("macros")?;
        if macros.count() > 0 {
            out.push_str("        fn load_declared_macros(&self) -> Vec<rt::TwigMacro> {\n            vec![\n");
            for (_, macro_node) in macros.children() {
                let (registration, function) = self.compile_macro(&macro_node)?;
                let _ = writeln!(out, "                {registration},");
                self.functions.push(function);
            }
            out.push_str("            ]\n        }\n");
        }
        if let Some(check) =
            find_class(&module.get_node("class_end")?, NodeClass::CheckSecurityNode)
        {
            out.push_str(&compile_check_security(&check, self.last_debug_line));
        }
        out.push_str("    }\n\n");
        for function in std::mem::take(&mut self.functions) {
            out.push_str(&function);
            out.push('\n');
        }
        out.push_str("}\n\n");
        Ok(out)
    }

    /// Compiles the body of a generated method, with fresh line slots.
    fn method(&mut self, body: impl FnOnce(&mut Self) -> R<()>) -> R {
        let saved_lines = std::mem::take(&mut self.lines);
        let saved_slot = self.current_line_slot.take();
        let saved_scope = self.scope;
        let saved_indentation = std::mem::replace(&mut self.indentation, 4);
        self.scope = Scope::Method;
        let start = self.source.len();
        let result = body(self);
        let code = self.take_from(start);
        let code = self.patch_lines(&code);
        self.lines = saved_lines;
        self.current_line_slot = saved_slot;
        self.scope = saved_scope;
        self.indentation = saved_indentation;
        result?;
        Ok(code)
    }

    #[allow(clippy::too_many_lines)]
    fn compile_constructor(&mut self, module: &Node) -> R {
        let traits = module.get_node("traits")?;
        let blocks = module.get_node("blocks")?;
        let has_parent = module.has_node("parent");
        let body = self.method(|c| {
            c.compile_statement(&module.get_node("constructor_start")?)?;
            if !has_parent {
                c.line("this.set_parent(None);");
            }
            let count = traits.count();
            if count > 0 {
                c.line("this.ensure_traits_allowed(env)?;");
            }
            for (i, (_, trait_node)) in traits.children().into_iter().enumerate() {
                let template = trait_node.get_node("template")?;
                c.add_debug_info(&template);
                let line = template.line();
                let expr = c.expr(&template)?;
                c.line(&format!("let __t{i} = this.load(env, &{expr}, {line}, None)?;"));
                c.line(&format!("if !__t{i}.is_traitable() {{"));
                c.line(&format!(
                    "    let mut __m = b\"Template \\\"\".to_vec(); __m.extend(rt::bytes(&{expr})?); __m.extend_from_slice(b\"\\\" cannot be used as a trait.\");"
                ));
                c.line(&format!("    return Err(rt::runtime_error(__m, {line}, this));"));
                c.line("}");
                c.line(&format!("let mut __t{i}_blocks = __t{i}.blocks();"));
                for (key, value) in trait_node.get_node("targets")?.children() {
                    let key = key.to_php_string();
                    let value_expr = c.expr(&value)?;
                    c.line(&format!("if !__t{i}_blocks.contains({}) {{", byte_lit(&key)));
                    c.line(&format!(
                        "    let mut __m = b\"Block \\\"\".to_vec(); __m.extend_from_slice({}); __m.extend_from_slice(b\"\\\" is not defined in trait \\\"\"); __m.extend(rt::bytes(&{expr})?); __m.extend_from_slice(b\"\\\".\");",
                        byte_lit(&key)
                    ));
                    c.line(&format!("    return Err(rt::runtime_error(__m, {line}, this));"));
                    c.line("}");
                    c.line(&format!("let __alias = rt::bytes(&{value_expr})?;"));
                    c.line(&format!(
                        "if let Some(__b) = __t{i}_blocks.get({}).cloned() {{ __t{i}_blocks.insert(__alias.clone(), __b); }}",
                        byte_lit(&key)
                    ));
                    c.line(&format!("__t{i}_blocks.remove({});", byte_lit(&key)));
                    c.line(&format!("this.set_trait_alias(__alias, {}.to_vec());", byte_lit(&key)));
                }
            }
            if count > 0 {
                c.line("let mut __traits = rt::Blocks::new();");
                for i in 0..count {
                    c.line(&format!("__traits = __traits.merge(&__t{i}_blocks);"));
                }
                c.line("this.set_traits(__traits.clone());");
                c.line("let mut __blocks = __traits;");
            } else {
                c.line("let mut __blocks = rt::Blocks::new();");
            }
            for (key, _) in blocks.children() {
                let block_name = key.to_php_string();
                c.line(&format!(
                    "__blocks.insert({}.to_vec(), this.block_ref({}));",
                    byte_lit(&block_name),
                    byte_lit(&block_name)
                ));
            }
            c.line("this.set_blocks(__blocks);");
            c.compile_statement(&module.get_node("constructor_end")?)?;
            c.line("Ok(())");
            Ok(())
        })?;
        let mut out = String::new();
        out.push_str("        fn construct(&self, this: &rt::Rc<rt::Template>, env: &rt::Environment) -> Result<(), rt::Error> {\n");
        out.push_str("            let mut context = rt::PhpArray::new();\n");
        out.push_str("            let blocks = &rt::Blocks::new();\n");
        out.push_str(&wrap_method(&body, "()"));
        out.push_str("        }\n\n");
        Ok(out)
    }

    /// `SandboxNode::compile()` (the deprecated `sandbox` tag): the legacy
    /// sandbox state is on while the body runs.
    fn compile_sandbox(&mut self, node: &Node) -> R<()> {
        self.add_debug_info(node);
        let var = self.var_name();
        self.line(&format!("let {var}_sb = rt::sandbox::checker(env)?;"));
        self.line(&format!("let {var}_already = {var}_sb.is_sandboxed(None);"));
        self.line(&format!(
            "if !{var}_already {{ {var}_sb.set_sandboxed(true); }}"
        ));
        self.line(&format!("let {var}_r = (|| -> Result<(), rt::Error> {{"));
        self.indentation += 1;
        self.compile_statement(&node.get_node("body")?)?;
        self.line("Ok(())");
        self.indentation -= 1;
        self.line("})();");
        self.line(&format!(
            "if !{var}_already {{ {var}_sb.set_sandboxed(false); }}"
        ));
        self.line(&format!("{var}_r?;"));
        Ok(())
    }

    fn compile_block(&mut self, name: &[u8], block: &Node) -> R {
        // the parser wraps each block in a `BodyNode`
        let block = &if block.is(NodeClass::BodyNode) {
            block.node(0i64).unwrap_or_else(|| block.clone())
        } else {
            block.clone()
        };
        let body = self.method(|c| {
            c.add_debug_info(block);
            c.compile_statement(&block.get_node("body")?)?;
            c.line("Ok(())");
            Ok(())
        })?;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "    fn block_{}(this: &rt::Rc<rt::Template>, env: &rt::Environment, context: rt::Context, blocks: &rt::Blocks, out: &mut Vec<u8>) -> Result<(), rt::Error> {{",
            hex(name)
        );
        out.push_str("        let mut context = context;\n");
        out.push_str(&dedent(&wrap_method(&body, "()")));
        out.push_str("    }\n");
        Ok(out)
    }

    /// `MacroNode::compile()`: the registration and the function.
    fn compile_macro(&mut self, node: &Node) -> R<(String, String)> {
        let name = name_of(node);
        let variadic_name = node
            .attr("variadic_name")
            .filter(|a| !matches!(a, Attr::Null))
            .and_then(|a| a.to_php_string());
        let bucket_name = variadic_name.clone().unwrap_or_else(|| b"varargs".to_vec());
        let arguments = node.get_node("arguments")?;
        let pairs = arguments.key_value_pairs();
        let mut signature = Vec::new();
        let body = self.method(|c| {
            c.add_debug_info(node);
            c.line("let blocks = &rt::Blocks::new();");
            c.line("let mut context = rt::PhpArray::new();");
            for (i, (key, default)) in pairs.iter().enumerate() {
                let arg_name = strip_reserved_prefix(&name_of(key));
                let default_code = c.expr(default)?;
                c.line(&format!(
                    "let __a = match __args.get({i}).cloned().flatten() {{ Some(v) => v, None => {default_code} }};"
                ));
                c.line(&format!("rt::set(&mut context, {}, __a);", byte_lit(&arg_name)));
            }
            c.line(&format!(
                "rt::set(&mut context, {}, rt::Value::Array(__varargs));",
                byte_lit(&bucket_name)
            ));
            c.line("context.union(&env.globals());");
            let body_node = node.get_node("body")?;
            let capture = c.capture_expr(&body_node, false)?;
            c.line(&format!("Ok({capture})"));
            Ok(())
        })?;
        for (key, default) in &pairs {
            let arg_name = strip_reserved_prefix(&name_of(key));
            let explicit = !(default.attr_true("is_implicit"));
            signature.push(format!("({}, {explicit})", byte_lit(&arg_name)));
        }
        let mut function = String::new();
        let _ = writeln!(
            function,
            "    fn macro_{}(this: &rt::Rc<rt::Template>, env: &rt::Environment, __args: Vec<Option<rt::Value>>, __varargs: rt::PhpArray) -> Result<rt::Value, rt::Error> {{",
            hex(&name)
        );
        function.push_str(&dedent(&wrap_method(&body, "rt::Value")));
        function.push_str("    }\n");
        let registration = format!(
            "rt::TwigMacro::new({}, macro_{}, &[{}], {})",
            byte_lit(&name),
            hex(&name),
            signature.join(", "),
            variadic_name.is_some()
        );
        Ok((registration, function))
    }

    // -----------------------------------------------------------------
    // statements

    #[allow(clippy::too_many_lines)]
    fn compile_statement(&mut self, node: &Node) -> R<()> {
        use NodeClass as C;
        match node.class() {
            C::Node | C::Nodes | C::BodyNode | C::AutoEscapeNode => {
                let children = if node.is(C::AutoEscapeNode) {
                    vec![node.get_node("body")?]
                } else {
                    node.child_nodes()
                };
                for child in children {
                    self.compile_statement(&child)?;
                }
            }
            C::EmptyNode
            | C::ConfigNode
            | C::MacroDeclarationNode
            | C::TypesNode
            | C::TypeNode
            | C::NameDeprecation
            | C::NodeDocumentation
            | C::FlushNode
            | C::CheckSecurityNode => {
                if node.is(C::FlushNode) {
                    self.add_debug_info(node);
                }
            }
            // `$this->sandbox = $env->getExtension(SandboxExtension::class)->getChecker();`
            C::CheckSecurityCallNode => {
                self.line("rt::sandbox::checker(env)?;");
            }
            C::TextNode => {
                self.add_debug_info(node);
                let data = node.attr_bytes("data").unwrap_or_default();
                self.line(&format!("out.extend_from_slice({});", byte_lit(&data)));
            }
            C::PrintNode => {
                self.add_debug_info(node);
                let expr = node.get_node("expr")?;
                let code = self.expr(&expr)?;
                self.line(&format!("rt::echo(out, &{code})?;"));
            }
            C::BlockReferenceNode => {
                self.add_debug_info(node);
                let name = name_of(node);
                self.line(&format!(
                    "this.display_block(env, {}, context.clone(), blocks, true, None, out)?;",
                    byte_lit(&name)
                ));
            }
            C::ParentExpression if node.attr_true("output") => {
                self.add_debug_info(node);
                let name = name_of(node);
                self.line(&format!(
                    "this.display_parent_block(env, {}, context.clone(), blocks, out)?;",
                    byte_lit(&name)
                ));
            }
            C::BlockReferenceExpression if node.attr_true("output") => {
                self.add_debug_info(node);
                let template = match node.node("template") {
                    Some(t) => {
                        let expr = self.expr(&t)?;
                        Some(format!("this.load(env, &{expr}, {}, None)?", node.line()))
                    }
                    None => None,
                };
                let name = self.expr(&node.get_node("name")?)?;
                match template {
                    Some(t) => self.line(&format!(
                        "{{ let __t = {t}; let __n = rt::bytes(&{name})?; __t.display_block(env, &__n, context.clone(), &rt::Blocks::new(), true, None, out)?; }}"
                    )),
                    None => self.line(&format!(
                        "{{ let __n = rt::bytes(&{name})?; this.display_block(env, &__n, context.clone(), blocks, true, None, out)?; }}"
                    )),
                };
            }
            C::DoNode => {
                self.add_debug_info(node);
                let expr = self.expr(&node.get_node("expr")?)?;
                self.line(&format!("let _ = {expr};"));
            }
            C::SetNode => self.compile_set(node)?,
            C::IfNode => self.compile_if(node)?,
            C::ForNode => self.compile_for(node)?,
            C::ForLoopNode => {
                if node.attr_true("else") {
                    self.line("rt::set(&mut context, b\"_iterated\", rt::Value::Bool(true));");
                }
                if node.attr_true("with_loop") {
                    self.line("rt::loop_next(&mut context);");
                }
            }
            C::IncludeNode | C::EmbedNode => self.compile_include(node)?,
            C::WithNode => self.compile_with(node)?,
            C::ImportNode => self.compile_import(node)?,
            C::DeprecatedNode => self.compile_deprecated(node)?,
            C::BlockNode | C::MacroNode | C::MacrosNode | C::ModuleNode => {
                return Err(unsupported(node, "A nested block, macro or module node"));
            }
            C::SandboxNode => self.compile_sandbox(node)?,
            other if node.is_expression() => {
                // an expression used as a statement (`InlinePrint`)
                self.add_debug_info(node);
                let expr = self.expr(node)?;
                let _ = other;
                self.line(&format!("let _ = {expr};"));
            }
            other => {
                return Err(unsupported(node, other.php_class()));
            }
        }
        Ok(())
    }

    fn compile_set(&mut self, node: &Node) -> R<()> {
        self.add_debug_info(node);
        let names = node.get_node("names")?;
        let values = node.get_node("values")?;
        let capture = node.attr_true("capture");
        let safe = node.attr_true("safe");
        let name_nodes: Vec<Node> =
            if names.class() == NodeClass::Nodes || names.class() == NodeClass::Node {
                names.child_nodes()
            } else {
                vec![names.clone()]
            };
        if name_nodes.len() > 1 {
            let value_nodes = values.child_nodes();
            let mut codes = Vec::new();
            for value in &value_nodes {
                codes.push(self.expr(value)?);
            }
            self.line("{");
            self.indent();
            for (i, code) in codes.iter().enumerate() {
                self.line(&format!("let __s{i} = {code};"));
            }
            for (i, name) in name_nodes.iter().enumerate() {
                let target = self.assign_target(name, &format!("__s{i}"))?;
                self.line(&target);
            }
            self.outdent();
            self.line("}");
            return Ok(());
        }
        // `subcompile()` of a list compiles its one expression
        let values = if !capture
            && (values.class() == NodeClass::Nodes || values.class() == NodeClass::Node)
            && values.count() == 1
        {
            values.child_nodes().remove(0)
        } else {
            values
        };
        let value = if capture {
            self.capture_expr(&values, false)?
        } else if safe {
            let code = self.expr(&values)?;
            format!("rt::markup_unless_empty(env, &{code})?")
        } else {
            self.expr(&values)?
        };
        let Some(name) = name_nodes.first() else {
            return Err(Error::logic("A set node has a name."));
        };
        if name.is(NodeClass::LocalVariable) {
            let local = self.local_name(name);
            self.line(&format!("let {local}: rt::Value = {value};"));
            return Ok(());
        }
        self.line("{");
        self.indent();
        self.line(&format!("let __s = {value};"));
        let target = self.assign_target(name, "__s")?;
        self.line(&target);
        self.outdent();
        self.line("}");
        Ok(())
    }

    /// The statement assigning a Rust variable to an assignable node.
    fn assign_target(&mut self, node: &Node, value: &str) -> R {
        match node.class() {
            NodeClass::AssignContextVariable
            | NodeClass::AssignNameExpression
            | NodeClass::ContextVariable => Ok(format!(
                "rt::set(&mut context, {}, {value});",
                byte_lit(&name_of(node))
            )),
            NodeClass::LocalVariable | NodeClass::TempNameExpression => {
                let local = self.local_name(node);
                Ok(format!("let {local}: rt::Value = {value};"))
            }
            _ => Err(unsupported(node, "This assignment target")),
        }
    }

    /// The Rust name of a `LocalVariable` (`TempNameExpression::compile()`).
    fn local_name(&mut self, node: &Node) -> String {
        let name = match node.attr("name") {
            Some(Attr::Null) | None => {
                let generated = self.var_name();
                node.set_attr("name", Attr::Str(generated.clone().into_bytes()));
                generated.into_bytes()
            }
            Some(a) => a.to_php_string().unwrap_or_default(),
        };
        format!("l_{}", hex(&name))
    }

    fn compile_if(&mut self, node: &Node) -> R<()> {
        self.add_debug_info(node);
        let tests = node.get_node("tests")?;
        let count = tests.count();
        let mut i = 0;
        while i < count {
            let test = tests.get_node(Key::from(i))?;
            let cond = self.cond(&test)?;
            if i == 0 {
                self.line(&format!("if {cond} {{"));
            } else {
                self.line(&format!("}} else if {cond} {{"));
            }
            self.indent();
            if let Some(body) = tests.node(Key::from(i + 1)) {
                self.compile_statement(&body)?;
            }
            self.outdent();
            i += 2;
        }
        if let Some(else_node) = node.node("else") {
            self.line("} else {");
            self.indent();
            self.compile_statement(&else_node)?;
            self.outdent();
        }
        self.line("}");
        Ok(())
    }

    fn compile_for(&mut self, node: &Node) -> R<()> {
        self.add_debug_info(node);
        let key_name = name_of(&node.get_node("key_target")?);
        let value_name = name_of(&node.get_node("value_target")?);
        let has_else = node.has_node("else");
        let with_loop = node.attr_true("with_loop");
        let body = node.get_node("body")?;
        if let Some(loop_node) = body.node(1i64)
            && loop_node.is(NodeClass::ForLoopNode)
        {
            loop_node.set_attr("else", has_else);
            loop_node.set_attr("with_loop", with_loop);
        }
        self.line("{");
        self.indent();
        self.line("let __parent = rt::Value::Array(context.clone());");
        self.line("rt::set(&mut context, b\"_parent\", __parent);");
        let seq = self.expr(&node.get_node("seq")?)?;
        self.line(&format!("let __seq = rt::core::ensure_traversable({seq});"));
        self.line("rt::set(&mut context, b\"_seq\", __seq.clone());");
        if has_else {
            self.line("rt::set(&mut context, b\"_iterated\", rt::Value::Bool(false));");
        }
        if with_loop {
            self.line("rt::loop_start(&mut context, &__seq)?;");
        }
        self.line("for (__k, __v) in rt::iterate(&__seq)? {");
        self.indent();
        self.line(&format!(
            "rt::set(&mut context, {}, __k);",
            byte_lit(&key_name)
        ));
        self.line(&format!(
            "rt::set(&mut context, {}, __v);",
            byte_lit(&value_name)
        ));
        self.compile_statement(&body)?;
        self.outdent();
        self.line("}");
        if let Some(else_node) = node.node("else") {
            self.add_debug_info(&else_node);
            self.line("if !rt::truthy(&rt::get(&context, b\"_iterated\")) {");
            self.indent();
            self.compile_statement(&else_node.get_node("body")?)?;
            self.outdent();
            self.line("}");
        }
        let mut names = vec![
            "b\"_seq\"".to_owned(),
            byte_lit(&key_name),
            byte_lit(&value_name),
            "b\"_parent\"".to_owned(),
        ];
        if has_else {
            names.push("b\"_iterated\"".to_owned());
        }
        if with_loop {
            names.push("b\"loop\"".to_owned());
        }
        let list: Vec<String> = names.iter().map(|n| format!("&{n}[..]")).collect();
        self.line(&format!(
            "rt::loop_end(&mut context, &[{}]);",
            list.join(", ")
        ));
        self.outdent();
        self.line("}");
        Ok(())
    }

    /// The variables of an include (`addTemplateArguments()`).
    fn template_arguments(&mut self, node: &Node) -> R {
        let only = node.attr_true("only");
        Ok(match node.node("variables") {
            None if only => "rt::PhpArray::new()".to_owned(),
            None => "context.clone()".to_owned(),
            Some(variables) => {
                let code = self.expr(&variables)?;
                if only {
                    format!(
                        "rt::array_arg(rt::core::to_array(&{code}, true)?, \"Twig\\\\Template::yield()\")?"
                    )
                } else {
                    format!("rt::core::merge_arrays(&[rt::Value::Array(context.clone()), {code}])?")
                }
            }
        })
    }

    fn compile_include(&mut self, node: &Node) -> R<()> {
        self.add_debug_info(node);
        let line = node.line();
        let ignore_missing = node.attr_true("ignore_missing");
        let (template, index) = if node.is(NodeClass::EmbedNode) {
            let name = node.attr_bytes("name").unwrap_or_default();
            let index = node.attr("index").and_then(|a| a.as_int());
            (
                format!("rt::s({})", byte_lit(&name)),
                index.map_or_else(|| "None".to_owned(), |i| format!("Some({i})")),
            )
        } else {
            (self.expr(&node.get_node("expr")?)?, "None".to_owned())
        };
        let variables = self.template_arguments(node)?;
        if ignore_missing {
            let load = if node.is(NodeClass::EmbedNode) {
                format!(
                    "rt::load_embed_optional(this, env, &{template}, {line}, {index}, &context)?"
                )
            } else {
                format!("rt::load_optional(this, env, &{template}, {line}, {index})?")
            };
            self.line(&format!("if let Some(__t) = {load} {{"));
            self.line(&format!(
                "    __t.display(env, {variables}, &rt::Blocks::new(), out)?;"
            ));
            self.line("}");
        } else {
            self.line("{");
            self.line(&format!(
                "    let __t = this.load(env, &{template}, {line}, {index})?;"
            ));
            self.line(&format!(
                "    __t.display(env, {variables}, &rt::Blocks::new(), out)?;"
            ));
            self.line("}");
        }
        Ok(())
    }

    fn compile_with(&mut self, node: &Node) -> R<()> {
        self.add_debug_info(node);
        let parent = self.var_name();
        self.line("{");
        self.indent();
        self.line(&format!("let {parent} = context.clone();"));
        if let Some(variables) = node.node("variables") {
            let code = self.expr(&variables)?;
            let line = variables.line();
            self.line(&format!("let __vars = {code};"));
            self.line("if !rt::core::is_iterable_value(&__vars) {");
            self.line(&format!(
                "    return Err(rt::runtime_error(b\"Variables passed to the \\\"with\\\" tag must be a mapping.\".to_vec(), {line}, this));"
            ));
            self.line("}");
            self.line("let __vars = rt::array_arg(rt::core::to_array(&__vars, true)?, \"with\")?;");
            if node.attr_true("only") {
                self.line("context = rt::PhpArray::new();");
            }
            self.line("context = rt::with_context(__vars, &context, env);");
        }
        self.compile_statement(&node.get_node("body")?)?;
        self.line(&format!("context = {parent};"));
        self.outdent();
        self.line("}");
        Ok(())
    }

    fn compile_import(&mut self, node: &Node) -> R<()> {
        let var = node.get_node("var")?;
        self.add_debug_info(&var);
        let expr = node.get_node("expr")?;
        let namespace = if expr.is(NodeClass::ContextVariable) && name_of(&expr) == b"_self" {
            "this.macro_namespace()".to_owned()
        } else {
            let code = self.expr(&expr)?;
            format!(
                "this.load(env, &{code}, {}, None)?.macro_namespace()",
                node.line()
            )
        };
        let macro_var = var.get_node("var")?;
        let name = name_of(&macro_var);
        self.line("{");
        self.indent();
        self.line(&format!("let __ns = {namespace};"));
        self.line(&format!(
            "macros.insert({}.to_vec(), rt::Rc::clone(&__ns));",
            byte_lit(&name)
        ));
        if var.attr_true("global") {
            self.line(&format!("this.set_macro({}, __ns);", byte_lit(&name)));
        }
        self.outdent();
        self.line("}");
        Ok(())
    }

    fn compile_deprecated(&mut self, node: &Node) -> R<()> {
        self.add_debug_info(node);
        let expr = self.expr(&node.get_node("expr")?)?;
        let package = match node.node("package") {
            Some(p) => self.expr(&p)?,
            None => "rt::s(b\"\")".to_owned(),
        };
        let version = match node.node("version") {
            Some(v) => self.expr(&v)?,
            None => "rt::s(b\"\")".to_owned(),
        };
        let suffix = format!(
            " in \"{}\" at line {}.",
            node.template_name().unwrap_or_default(),
            node.line()
        );
        self.line(&format!("let __d = {expr};"));
        self.line(&format!(
            "rt::deprecated(&{package}, &{version}, &__d, {})?;",
            byte_lit(suffix.as_bytes())
        ));
        Ok(())
    }

    /// A `CaptureNode` (or a body captured like one) as an expression.
    fn capture_expr(&mut self, body: &Node, raw: bool) -> R {
        let body = if body.is(NodeClass::CaptureNode) {
            let raw = raw || body.attr_true("raw");
            let inner = body.get_node("body")?;
            return self.capture_body(&inner, raw);
        } else {
            body
        };
        self.capture_body(body, raw)
    }

    fn capture_body(&mut self, body: &Node, raw: bool) -> R {
        if self.scope == Scope::Arrow {
            return Err(unsupported(body, "A capture in an arrow function"));
        }
        let indentation = self.indentation + 2;
        let code = self.body_code(body, indentation)?;
        let pad = "    ".repeat(self.indentation + 1);
        Ok(format!(
            "{{\n{pad}let mut __buf: Vec<u8> = Vec::new();\n{pad}{{\n{pad}    let out = &mut __buf;\n{code}{pad}}}\n{pad}rt::capture(env, __buf, {raw})\n{}}}",
            "    ".repeat(self.indentation)
        ))
    }

    // -----------------------------------------------------------------
    // expressions

    /// A Rust `bool` expression for a test (`if`, ternaries).
    fn cond(&mut self, node: &Node) -> R {
        if node.is(NodeClass::TrueTest) {
            let inner = self.expr(&node.get_node("node")?)?;
            return Ok(format!("rt::true_test(&{inner})"));
        }
        let code = self.expr(node)?;
        Ok(format!("rt::truthy(&{code})"))
    }

    /// An expression as a Rust expression of type `rt::Value`.
    #[allow(clippy::too_many_lines)]
    pub fn expr(&mut self, node: &Node) -> R {
        use NodeClass as C;
        let class = node.class();
        Ok(match class {
            C::ConstantExpression => {
                if node.is_defined_test_enabled() {
                    "rt::Value::Bool(true)".to_owned()
                } else {
                    repr(&node.attr("value").unwrap_or(Attr::Null))?
                }
            }
            C::ArrayExpression => self.array_expr(node)?,
            C::ContextVariable
            | C::NameExpression
            | C::AssignContextVariable
            | C::AssignNameExpression => self.name_expr(node)?,
            C::LocalVariable | C::TempNameExpression => self.local_name(node),
            C::GetAttrExpression => self.get_attr(node)?,
            C::CheckToStringNode => {
                let expr = node.get_node("expr")?;
                let code = self.expr(&expr)?;
                let function = if node.attr_true("spread") {
                    "ensure_spread_allowed"
                } else {
                    "ensure_to_string_allowed"
                };
                format!(
                    "rt::sandbox::{function}(env, {code}, {}, this.source())?",
                    expr.line()
                )
            }
            C::ConditionalExpression => {
                let test = self.cond(&node.get_node("expr1")?)?;
                let left = self.expr(&node.get_node("expr2")?)?;
                let right = self.expr(&node.get_node("expr3")?)?;
                format!("(if {test} {{ {left} }} else {{ {right} }})")
            }
            C::ConditionalTernary | C::ElvisBinary | C::NullCoalesceBinary => {
                let test = self.cond(&node.get_node("test")?)?;
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                format!("(if {test} {{ {left} }} else {{ {right} }})")
            }
            C::NullCoalesceExpression => {
                let left = self.expr(&node.get_node("expr2")?)?;
                let right = self.expr(&node.get_node("expr3")?)?;
                format!("{{ let __n = {left}; if __n.is_null() {{ {right} }} else {{ __n }} }}")
            }
            C::SetBinary => {
                let left = node.get_node("left")?;
                let right = self.expr(&node.get_node("right")?)?;
                if !matches!(left.class(), C::AssignContextVariable | C::ContextVariable) {
                    return Err(unsupported(node, "This assignment"));
                }
                format!(
                    "{{ let __s = {right}; rt::assign(&mut context, {}, __s) }}",
                    byte_lit(&name_of(&left))
                )
            }
            C::SequenceDestructuringSetBinary => {
                self.expression_debug_info(node);
                let left = node.get_node("left")?;
                let right = self.expr(&node.get_node("right")?)?;
                let mut names = Vec::new();
                for (_, value) in left.key_value_pairs() {
                    if value.is(C::EmptyExpression) {
                        names.push("None".to_owned());
                    } else {
                        names.push(format!("Some(&{}[..])", byte_lit(&name_of(&value))));
                    }
                }
                format!(
                    "{{ let __s = {right}; rt::destructure_sequence(&mut context, &[{}], __s)? }}",
                    names.join(", ")
                )
            }
            C::ObjectDestructuringSetBinary => {
                self.expression_debug_info(node);
                let left = node.get_node("left")?;
                let right_node = node.get_node("right")?;
                let right = self.expr(&right_node)?;
                let line = right_node.line();
                let mut code = format!("{{ let __s = {right}; let mut __values = Vec::new();");
                let mut assigns = String::new();
                for (i, (key, value)) in left.key_value_pairs().into_iter().enumerate() {
                    let property = key.attr("value").unwrap_or(Attr::Null);
                    let _ = write!(
                        code,
                        " let __m{i} = rt::core::get_attribute(env, this.source(), &__s, &{}, &rt::PhpArray::new(), \"any\", false, false, {}, {line})?; __values.push(__m{i}.clone());",
                        repr(&property)?,
                        self.env.has_sandbox()
                    );
                    let _ = write!(
                        assigns,
                        " rt::set(&mut context, {}, __m{i});",
                        byte_lit(&name_of(&value))
                    );
                }
                let _ = write!(
                    code,
                    "{assigns} rt::Value::Array(rt::PhpArray::list(__values)) }}"
                );
                code
            }
            C::AndBinary | C::OrBinary => {
                let left = self.cond(&node.get_node("left")?)?;
                let right = self.cond(&node.get_node("right")?)?;
                let op = if class == C::AndBinary { "&&" } else { "||" };
                format!("rt::Value::Bool({left} {op} {right})")
            }
            C::XorBinary => {
                let left = self.cond(&node.get_node("left")?)?;
                let right = self.cond(&node.get_node("right")?)?;
                format!("rt::Value::Bool({left} ^ {right})")
            }
            // PHP compiles a unary operator without explicit parentheses
            // as ` -x`, so `(-x ** y)` is `-(x ** y)` in PHP
            C::PowerBinary
                if node.get_node("left")?.class().is_a(C::AbstractUnary)
                    && !node.get_node("left")?.has_explicit_parentheses() =>
            {
                let unary = node.get_node("left")?;
                let power = Node::new(
                    C::PowerBinary,
                    vec![
                        ("left".into(), unary.get_node("node")?),
                        ("right".into(), node.get_node("right")?),
                    ],
                    Vec::new(),
                    node.line(),
                );
                let inner = self.expr(&power)?;
                match unary.class() {
                    C::NegUnary => format!("rt::ops::neg(&{inner})?"),
                    C::PosUnary => format!("rt::ops::pos(&{inner})?"),
                    C::NotUnary => format!("rt::Value::Bool(!rt::truthy(&{inner}))"),
                    other => return Err(unsupported(node, other.php_class())),
                }
            }
            C::AddBinary
            | C::SubBinary
            | C::MulBinary
            | C::DivBinary
            | C::ModBinary
            | C::PowerBinary
            | C::BitwiseAndBinary
            | C::BitwiseOrBinary
            | C::BitwiseXorBinary
            | C::ConcatBinary => {
                let function = match class {
                    C::AddBinary => "add",
                    C::SubBinary => "sub",
                    C::MulBinary => "mul",
                    C::DivBinary => "div",
                    C::ModBinary => "modulo",
                    C::PowerBinary => "pow",
                    C::BitwiseAndBinary => "bit_and",
                    C::BitwiseOrBinary => "bit_or",
                    C::BitwiseXorBinary => "bit_xor",
                    _ => "concat",
                };
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                format!("rt::ops::{function}(&{left}, &{right})?")
            }
            C::EqualBinary
            | C::NotEqualBinary
            | C::LessBinary
            | C::GreaterBinary
            | C::LessEqualBinary
            | C::GreaterEqualBinary
            | C::SpaceshipBinary
            | C::SameAsBinary
            | C::StartsWithBinary
            | C::EndsWithBinary => {
                let function = match class {
                    C::EqualBinary => "eq",
                    C::NotEqualBinary => "ne",
                    C::LessBinary => "lt",
                    C::GreaterBinary => "gt",
                    C::LessEqualBinary => "le",
                    C::GreaterEqualBinary => "ge",
                    C::SpaceshipBinary => "spaceship",
                    C::SameAsBinary => "same",
                    C::StartsWithBinary => "starts_with",
                    _ => "ends_with",
                };
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                format!("rt::{function}(&{left}, &{right})")
            }
            C::NotSameAsBinary => {
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                format!("rt::Value::Bool(!rt::same(&{left}, &{right}).is_truthy())")
            }
            C::FloorDivBinary => {
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                format!("rt::floor_div(&{left}, &{right})?")
            }
            C::InBinary | C::NotInBinary => {
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                let not = if class == C::NotInBinary { "!" } else { "" };
                format!("rt::Value::Bool({not}rt::core::in_filter(&{left}, &{right})?)")
            }
            C::MatchesBinary => {
                let right = self.expr(&node.get_node("right")?)?;
                let left = self.expr(&node.get_node("left")?)?;
                format!("{{ let __r = {right}; rt::core::matches(&__r, &{left})? }}")
            }
            C::HasSomeBinary | C::HasEveryBinary => {
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                let function = if class == C::HasSomeBinary {
                    "array_some"
                } else {
                    "array_every"
                };
                format!(
                    "rt::Value::Bool(rt::core::{function}(env, this.source(), &{left}, &{right})?)"
                )
            }
            C::RangeBinary => {
                let left = self.expr(&node.get_node("left")?)?;
                let right = self.expr(&node.get_node("right")?)?;
                format!("rt::core::range_values(&{left}, &{right}, &rt::Value::Int(1))?")
            }
            C::NegUnary => {
                let inner = self.expr(&node.get_node("node")?)?;
                format!("rt::ops::neg(&{inner})?")
            }
            C::PosUnary => {
                let inner = self.expr(&node.get_node("node")?)?;
                format!("rt::ops::pos(&{inner})?")
            }
            C::NotUnary => {
                let inner = self.cond(&node.get_node("node")?)?;
                format!("rt::Value::Bool(!{inner})")
            }
            C::StringCastUnary => {
                let inner = self.expr(&node.get_node("node")?)?;
                format!("rt::string_cast(&{inner})?")
            }
            C::SpreadUnary => {
                return Err(unsupported(
                    node,
                    "The spread operator outside of a sequence or a call",
                ));
            }
            C::TrueTest => {
                let inner = self.expr(&node.get_node("node")?)?;
                format!("rt::Value::Bool(rt::true_test(&{inner}))")
            }
            C::NullTest => {
                let inner = self.expr(&node.get_node("node")?)?;
                format!("rt::Value::Bool(({inner}).is_null())")
            }
            C::EvenTest | C::OddTest => {
                let inner = self.expr(&node.get_node("node")?)?;
                let function = if class == C::EvenTest { "eq" } else { "ne" };
                format!(
                    "rt::{function}(&rt::ops::modulo(&{inner}, &rt::Value::Int(2))?, &rt::Value::Int(0))"
                )
            }
            C::DivisiblebyTest => {
                let inner = self.expr(&node.get_node("node")?)?;
                let divisor = self.expr(&node.get_node("arguments")?.get_node(0i64)?)?;
                format!("rt::eq(&rt::Value::Int(0), &rt::ops::modulo(&{inner}, &{divisor})?)")
            }
            C::SameasTest => {
                let inner = self.expr(&node.get_node("node")?)?;
                let other = self.expr(&node.get_node("arguments")?.get_node(0i64)?)?;
                format!("rt::same(&{inner}, &{other})")
            }
            C::ConstantTest => {
                let inner = self.expr(&node.get_node("node")?)?;
                let arguments = node.get_node("arguments")?;
                let name = self.expr(&arguments.get_node(0i64)?)?;
                let object = match arguments.node(1i64) {
                    Some(o) => self.expr(&o)?,
                    None => "rt::Value::Null".to_owned(),
                };
                format!(
                    "{{ let __c = {inner}; let __o = {object}; rt::same(&__c, &rt::core::constant_value(env, &{name}, &__o, false)?) }}"
                )
            }
            C::DefinedTest | C::DefaultFilter | C::RawFilter => {
                self.expr(&node.get_node("node")?)?
            }
            C::FilterExpression | C::FunctionExpression | C::TestExpression | C::EscapeFilter => {
                self.call_expr(node)?
            }
            C::EnumCasesFunction | C::EnumFunction => self.enum_function(node)?,
            C::RenderSandboxedFunction => return Err(unsupported(node, "The sandbox")),
            C::ArrowFunctionExpression => self.arrow(node)?,
            C::MacroReferenceExpression => self.macro_reference(node)?,
            C::MethodCallExpression => {
                let namespace = format!(
                    "rt::macro_namespace(&macros, {}, this, {})?",
                    byte_lit(&name_of(&node.get_node("node")?)),
                    node.line()
                );
                let method = node.attr_bytes("method").unwrap_or_default();
                let method = method.strip_prefix(b"macro_").unwrap_or(&method).to_vec();
                if node.is_defined_test_enabled() {
                    format!(
                        "rt::Value::Bool({namespace}.has(env, {}, &context)?)",
                        byte_lit(&method)
                    )
                } else {
                    let arguments = self.expr(&node.get_node("arguments")?)?;
                    format!(
                        "{{ let __args = rt::args_array({arguments}); {namespace}.call(env, {}, &__args, &context, {}, this.source())? }}",
                        byte_lit(&method),
                        node.line()
                    )
                }
            }
            C::MacroVariable => Self::macro_variable(node),
            C::BlockReferenceExpression => self.block_reference(node)?,
            C::ParentExpression => format!(
                "rt::Value::String(rt::PhpString::from(this.render_parent_block(env, {}, context.clone(), blocks)?))",
                byte_lit(&name_of(node))
            ),
            C::CaptureNode => self.capture_expr(node, false)?,
            C::EmptyExpression => "rt::Value::Null".to_owned(),
            C::InlinePrint => {
                let inner = self.expr(&node.get_node("node")?)?;
                format!("{{ rt::echo(out, &{inner})?; rt::Value::Null }}")
            }
            C::VariadicExpression | C::ListExpression => {
                return Err(unsupported(node, class.php_class()));
            }
            other => return Err(unsupported(node, other.php_class())),
        })
    }

    fn array_expr(&mut self, node: &Node) -> R {
        if node.is_defined_test_enabled() {
            return Ok("rt::Value::Bool(true)".to_owned());
        }
        let pairs = node.key_value_pairs();
        for (_, value) in &pairs {
            if value.is(NodeClass::EmptyExpression) {
                return Err(Error::syntax(
                    "Empty array elements are only allowed in destructuring assignments.",
                    value.line(),
                    node.source_context(),
                ));
            }
        }
        let mut code = String::from("rt::ArrayBuilder::new()");
        let mut is_sequence = true;
        for (i, (key, value)) in pairs.into_iter().enumerate() {
            let key_attr =
                if key.is(NodeClass::TempNameExpression) || key.is(NodeClass::LocalVariable) {
                    Some(key.attr("name").unwrap_or(Attr::Null))
                } else if key.is(NodeClass::ConstantExpression) {
                    Some(key.attr("value").unwrap_or(Attr::Null))
                } else {
                    None
                };
            let index = i64::try_from(i).unwrap_or(i64::MAX);
            if !matches!(key_attr, Some(Attr::Int(k)) if k == index) {
                is_sequence = false;
            }
            if value.is(NodeClass::SpreadUnary) {
                let inner = self.expr(&value.get_node("node")?)?;
                let _ = write!(code, ".spread(&{inner})?");
                continue;
            }
            let value_code = self.expr(&value)?;
            if is_sequence {
                let _ = write!(code, ".push({value_code})");
                continue;
            }
            let key_code = if let Some(a) = key_attr {
                repr(&a)?
            } else {
                let k = self.expr(&key)?;
                format!("rt::string_cast(&{k})?")
            };
            let _ = write!(code, ".key(&{key_code}, {value_code})?");
        }
        code.push_str(".done()");
        Ok(code)
    }

    #[allow(clippy::unnecessary_wraps)] // the same shape as the other expressions
    fn name_expr(&mut self, node: &Node) -> R {
        self.expression_debug_info(node);
        let name = name_of(node);
        let special = matches!(name.as_slice(), b"_self" | b"_context" | b"_charset");
        if node.is_defined_test_enabled() {
            if special || node.attr_true("always_defined") {
                return Ok("rt::Value::Bool(true)".to_owned());
            }
            return Ok(format!(
                "rt::Value::Bool(rt::has(&context, {}))",
                byte_lit(&name)
            ));
        }
        if special {
            return Ok(match name.as_slice() {
                b"_self" => "rt::s(this.name())".to_owned(),
                b"_context" => "rt::Value::Array(context.clone())".to_owned(),
                _ => "rt::Value::String(env.charset_string())".to_owned(),
            });
        }
        if node.attr_true("always_defined")
            || node.attr_true("ignore_strict_check")
            || !self.env.is_strict_variables()
        {
            return Ok(format!("rt::get(&context, {})", byte_lit(&name)));
        }
        Ok(format!(
            "rt::get_strict(&context, {}, {}, this)?",
            byte_lit(&name),
            node.line()
        ))
    }

    fn get_attr(&mut self, node: &Node) -> R {
        let null_safe = node.attr_true("null_safe");
        let inner = node.get_node("node")?;
        if node.attr_true("ignore_strict_check") {
            inner.set_attr("ignore_strict_check", true);
        }
        // the nearest null-safe access of the chain that is not handled yet
        let mut null_safe_node = if null_safe { Some(node.clone()) } else { None };
        if null_safe_node.is_none() {
            let mut current = inner.clone();
            while current.is(NodeClass::GetAttrExpression) {
                if current.attr_true("null_safe") {
                    null_safe_node = Some(current.clone());
                    break;
                }
                current = current.get_node("node")?;
            }
        }
        let mut prefix = String::new();
        let mut suffix = String::new();
        if let Some(ns) = &null_safe_node
            && !ns.attr_true("is_short_circuited")
        {
            let var = self.null_safe_var(ns);
            let target = self.expr(&ns.get_node("node")?)?;
            prefix = format!(
                "{{ let {var} = {target}; if {var}.is_null() {{ rt::Value::Null }} else {{ "
            );
            " } }".clone_into(&mut suffix);
            ns.set_attr("is_short_circuited", true);
        }
        let object = if null_safe {
            self.null_safe_var(node)
        } else {
            self.expr(&inner)?
        };
        let attribute = self.expr(&node.get_node("attribute")?)?;
        let arguments = match node.node("arguments") {
            Some(a) => {
                let code = self.expr(&a)?;
                format!("&rt::args_array({code})")
            }
            None => "&rt::PhpArray::new()".to_owned(),
        };
        let kind = node.attr_bytes("type").unwrap_or_else(|| b"any".to_vec());
        Ok(format!(
            "{prefix}rt::core::get_attribute(env, this.source(), &{object}, &{attribute}, {arguments}, {}, {}, {}, {}, {})?{suffix}",
            str_lit(&String::from_utf8_lossy(&kind)),
            node.is_defined_test_enabled(),
            node.attr_true("ignore_strict_check"),
            self.env.has_sandbox(),
            inner.line()
        ))
    }

    /// `GetAttrExpression::getVarName()`.
    fn null_safe_var(&mut self, node: &Node) -> String {
        if let Some(name) = node.attr_bytes("var_name") {
            return String::from_utf8_lossy(&name).into_owned();
        }
        let name = self.var_name();
        node.set_attr("var_name", Attr::Str(name.clone().into_bytes()));
        name
    }

    fn macro_variable(node: &Node) -> String {
        let name = name_of(node);
        if name == b"_self" {
            return "this.macro_namespace()".to_owned();
        }
        format!(
            "rt::macro_namespace(&macros, {}, this, {})?",
            byte_lit(&name),
            node.line()
        )
    }

    fn macro_reference(&mut self, node: &Node) -> R {
        let namespace = Self::macro_variable(&node.get_node("template")?);
        let name = match node.node("name") {
            Some(n) => {
                let code = self.expr(&n)?;
                format!("&rt::bytes(&{code})?")
            }
            None => byte_lit(&name_of(node)),
        };
        if node.is_defined_test_enabled() {
            return Ok(format!(
                "{{ let __ns = {namespace}; rt::Value::Bool(__ns.has(env, {name}, &context)?) }}"
            ));
        }
        let arguments = self.expr(&node.get_node("arguments")?)?;
        Ok(format!(
            "{{ let __ns = {namespace}; let __name = {name}.to_vec(); let __args = rt::args_array({arguments}); __ns.call(env, &__name, &__args, &context, {}, this.source())? }}",
            node.line()
        ))
    }

    fn block_reference(&mut self, node: &Node) -> R {
        let template = match node.node("template") {
            Some(t) => {
                let code = self.expr(&t)?;
                Some(format!("this.load(env, &{code}, {}, None)?", node.line()))
            }
            None => None,
        };
        let name = self.expr(&node.get_node("name")?)?;
        if node.is_defined_test_enabled() {
            return Ok(match template {
                Some(t) => format!(
                    "{{ let __t = {t}; let __n = rt::bytes(&{name})?; rt::Value::Bool(__t.has_block(env, &__n, &context, &rt::Blocks::new())?) }}"
                ),
                None => format!(
                    "{{ let __n = rt::bytes(&{name})?; rt::Value::Bool(this.has_block(env, &__n, &context, blocks)?) }}"
                ),
            });
        }
        Ok(match template {
            Some(t) => format!(
                "{{ let __t = {t}; let __n = rt::bytes(&{name})?; rt::Value::String(rt::PhpString::from(__t.render_block(env, &__n, context.clone(), &rt::Blocks::new(), true)?)) }}"
            ),
            None => format!(
                "{{ let __n = rt::bytes(&{name})?; rt::Value::String(rt::PhpString::from(this.render_block(env, &__n, context.clone(), blocks, true)?)) }}"
            ),
        })
    }

    /// `EnumFunction::compile()` and `EnumCasesFunction::compile()`: a
    /// constant enum name is checked at compile time (the application's
    /// enums are in `CoreOptions::enums`) and compiled to its first case or
    /// its cases; any other argument is a call checked at run time.
    fn enum_function(&mut self, node: &Node) -> Result<String, Error> {
        let cases = node.is(NodeClass::EnumCasesFunction);
        let function = if cases { "enum_cases" } else { "enum" };
        let arguments = node.get_node("arguments")?;
        let first = arguments.node("enum").or_else(|| arguments.node(0i64));
        let Some(first) =
            first.filter(|f| f.is(NodeClass::ConstantExpression) && arguments.count() == 1)
        else {
            return self.call_expr(node);
        };
        let syntax = |message: String| Error::syntax(message, node.line(), node.source_context());
        let Some(Attr::Str(class)) = first.attr("value") else {
            return Err(syntax(format!(
                "The first argument of the \"{function}\" function must be a string."
            )));
        };
        let options = self.env.core_options();
        let name = String::from_utf8_lossy(&class).into_owned();
        let Some(objects) = options.enums.cases(&name) else {
            return Err(syntax(format!(
                "The first argument of the \"{function}\" function must be the name of an enum, \"{name}\" given."
            )));
        };
        if cases {
            return Ok(format!(
                "rt::core::enum_cases_of(env, {})?",
                byte_lit(&class)
            ));
        }
        let Some(case) = objects
            .first()
            .and_then(|o| o.downcast_ref::<php_value::object::EnumCase>())
        else {
            return Err(syntax(format!(
                "The first argument of the \"enum\" function must be a non-empty enum, \"{name}\" given."
            )));
        };
        let constant = format!("{name}::{}", case.name());
        Ok(format!(
            "rt::core::enum_constant(env, {})?",
            byte_lit(constant.as_bytes())
        ))
    }

    fn arrow(&mut self, node: &Node) -> R {
        self.expression_debug_info(node);
        let names = node.get_node("names")?;
        let mut sets = String::new();
        for (i, name) in names.child_nodes().into_iter().enumerate() {
            let _ = write!(
                sets,
                "rt::set(&mut context, {}, __args.get({i}).cloned().unwrap_or_default()); ",
                byte_lit(&name_of(&name))
            );
        }
        let saved = self.scope;
        self.scope = Scope::Arrow;
        let body = self.expr(&node.get_node("expr")?);
        self.scope = saved;
        let body = body?;
        Ok(format!(
            "{{ let __ctx = context.clone(); let __macros = macros.clone(); let __this = rt::Rc::clone(this); let __blocks = blocks.clone(); \
             env.register_arrow(rt::Rc::new(move |env: &rt::Environment, __args: &[rt::Value]| -> Result<rt::Value, rt::Error> {{ \
             let mut context = __ctx.clone(); let mut macros = __macros.clone(); let this = &__this; let blocks = &__blocks; {sets}Ok({body}) }})) }}"
        ))
    }

    /// `CallExpression::compileCallable()` and `compileArguments()`.
    fn call_expr(&mut self, node: &Node) -> R {
        let Some(Attr::Callable(callable)) = node.attr("twig_callable") else {
            return Err(Error::logic("A call node has a callable."));
        };
        if node.is(NodeClass::FunctionExpression)
            && callable.name() == b"constant"
            && node.is_defined_test_enabled()
        {
            let arguments = node.get_node("arguments")?;
            arguments.set_node("checkDefined", constant(true, node.line()))?;
        }
        let (path, params) = if node.is(NodeClass::EscapeFilter) {
            core_callable("Twig\\Runtime\\EscaperRuntime::escape")
                .map(|(p, params)| (p.to_owned(), params))
                .ok_or_else(|| Error::logic("The escaper is known."))?
        } else {
            resolve_callable(node, &callable)?
        };
        let mut pushes: Vec<String> = Vec::new();
        for argument in callable.arguments() {
            pushes.push(format!("__a.push(rt::s({}));", byte_lit(argument)));
        }
        if let Some(input) = node.node("node") {
            let code = self.expr(&input)?;
            pushes.push(format!("__a.push({code});"));
        }
        if let Some(arguments) = node.node("arguments") {
            let extracted = extract_arguments(node, &callable, &params, &arguments)?;
            for argument in extracted {
                if argument.is(NodeClass::VariadicExpression) {
                    for (_, value) in argument.key_value_pairs() {
                        if value.is(NodeClass::SpreadUnary) {
                            let code = self.expr(&value.get_node("node")?)?;
                            pushes.push(format!("__a.extend(rt::spread_values(&{code})?);"));
                        } else {
                            let code = self.expr(&value)?;
                            pushes.push(format!("__a.push({code});"));
                        }
                    }
                } else if argument.is(NodeClass::SpreadUnary) {
                    // `...$expr` in the PHP call
                    let code = self.expr(&argument.get_node("node")?)?;
                    pushes.push(format!("__a.extend(rt::spread_values(&{code})?);"));
                } else {
                    let code = self.expr(&argument)?;
                    pushes.push(format!("__a.push({code});"));
                }
            }
        }
        Ok(format!(
            "{{ let mut __a: Vec<rt::Value> = Vec::new(); {} {path}(&rt::Call::new(env, this, &context, {}), __a)? }}",
            pushes.join(" "),
            node.line()
        ))
    }
}

/// Wraps a compiled method body: `__line`, `$macros = $this->macros`,
/// and `handleException()`.
fn wrap_method(body: &str, ok_type: &str) -> String {
    format!(
        "            let mut __line: i64 = -1;\n            let r = (|| -> Result<{ok_type}, rt::Error> {{\n                let mut macros = this.macros();\n{}            }})();\n            rt::guess(r, __line, this)\n",
        indent_code(body, 3)
    )
}

fn indent_code(code: &str, levels: usize) -> String {
    let pad = "    ".repeat(levels);
    let mut out = String::with_capacity(code.len());
    for line in code.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&pad);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Removes one level of indentation (functions are one level less deep
/// than methods).
fn dedent(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    for line in code.lines() {
        out.push_str(line.strip_prefix("    ").unwrap_or(line));
        out.push('\n');
    }
    out
}

fn strip_reserved_prefix(name: &[u8]) -> Vec<u8> {
    name.strip_prefix(RESERVED_NAME_PREFIX)
        .unwrap_or(name)
        .to_vec()
}

/// `ModuleNode::compileIsTraitable()`.
fn is_traitable(module: &Node) -> R<bool> {
    if module.has_node("parent") || module.get_node("macros")?.count() > 0 {
        return Ok(false);
    }
    let body = module.get_node("body")?;
    let nodes = if body.is(NodeClass::BodyNode) {
        body.node(0i64).unwrap_or(body)
    } else {
        body
    };
    let nodes = if nodes.count() == 0 {
        vec![nodes]
    } else {
        nodes.child_nodes()
    };
    Ok(nodes.iter().all(|n| n.count() == 0))
}

/// The embedded templates of a module, and theirs (`subcompile()` of an
/// embedded `ModuleNode` compiles its own embedded templates).
fn embedded_modules(module: &Node) -> Vec<Node> {
    let mut out = Vec::new();
    if let Some(Attr::Node(embedded)) = module.attr("embedded_templates") {
        for template in embedded.child_nodes() {
            let nested = embedded_modules(&template);
            out.push(template);
            out.extend(nested);
        }
    }
    out
}

/// The strings a template passes to `template_from_string()` that are
/// known when it is compiled, with the `name` argument: a string literal,
/// or a variable that a `{% set %}` of the template sets to a literal.
fn string_templates(module: &Node) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    fn literal(node: &Node) -> Option<Vec<u8>> {
        match node.attr("value") {
            Some(Attr::Str(s)) if node.is(NodeClass::ConstantExpression) => Some(s),
            _ => None,
        }
    }
    fn walk(
        node: &Node,
        sets: &mut Vec<(Vec<u8>, Vec<u8>)>,
        calls: &mut Vec<(Node, Option<Node>)>,
    ) {
        if node.is(NodeClass::SetNode)
            && !node.attr_true("capture")
            && let (Some(names), Some(values)) = (node.node("names"), node.node("values"))
        {
            let list = |n: Node| {
                if n.class() == NodeClass::Nodes || n.class() == NodeClass::Node {
                    n.child_nodes()
                } else {
                    vec![n]
                }
            };
            let (names, values) = (list(names), list(values));
            if let ([name], [value]) = (names.as_slice(), values.as_slice())
                && let Some(s) = literal(value)
            {
                sets.push((name_of(name), s));
            }
        }
        if node.is(NodeClass::FunctionExpression)
            && let Some(Attr::Callable(callable)) = node.attr("twig_callable")
            && callable.callable() == Some(crate::runtime::string_loader::CALLABLE)
            && let Some(arguments) = node.node("arguments")
        {
            let mut template = None;
            let mut name = None;
            for (key, argument) in arguments.children() {
                // the sandbox visitor wraps the argument
                let argument = if argument.is(NodeClass::CheckToStringNode) {
                    argument.node("expr").unwrap_or(argument)
                } else {
                    argument
                };
                match key {
                    Key::Int(0) => template = Some(argument),
                    Key::Int(1) => name = Some(argument),
                    Key::Str(ref k) if k.as_slice() == b"template" => template = Some(argument),
                    Key::Str(ref k) if k.as_slice() == b"name" => name = Some(argument),
                    _ => {}
                }
            }
            if let Some(template) = template {
                calls.push((template, name));
            }
        }
        for child in node.child_nodes() {
            walk(&child, sets, calls);
        }
    }
    let mut sets = Vec::new();
    let mut calls = Vec::new();
    walk(module, &mut sets, &mut calls);
    let mut out = Vec::new();
    for (template, name) in calls {
        let name = match name {
            None => None,
            Some(n)
                if n.is(NodeClass::ConstantExpression)
                    && matches!(n.attr("value"), Some(Attr::Null)) =>
            {
                None
            }
            Some(n) => match literal(&n) {
                Some(s) => Some(s),
                None => continue,
            },
        };
        if let Some(s) = literal(&template) {
            out.push((s, name));
        } else if template.is(NodeClass::NameExpression) || template.is(NodeClass::ContextVariable)
        {
            let variable = name_of(&template);
            for (set_name, s) in &sets {
                if *set_name == variable {
                    out.push((s.clone(), name.clone()));
                }
            }
        }
    }
    out
}

/// Compiles a set of templates: their modules, and a static list for a
/// [`crate::loader::CompiledArrayLoader`] named `TEMPLATES`.
pub fn compile_set(env: &Environment, templates: &[(Vec<u8>, Vec<u8>)]) -> R {
    compile_set_with(env, templates, &[], false)
}

/// [`compile_set`] with more templates:
///
/// - `strings` are the `(name, template)` pairs of
///   `createTemplate($template, $name)` calls (`None`: no name), compiled
///   under the names `createTemplate()` gives them, for
///   [`Environment::create_template`];
/// - with `keep_failures`, a template of the set that does not compile
///   is kept as a template that gives the error when it is loaded, as
///   PHP's loader compiles a template only when it is loaded.
///
/// String templates that do not compile are always kept that way.
pub fn compile_set_with(
    env: &Environment,
    templates: &[(Vec<u8>, Vec<u8>)],
    strings: &[(Option<Vec<u8>>, Vec<u8>)],
    keep_failures: bool,
) -> R {
    let mut code = String::new();
    let mut entries = Vec::new();
    // the set's templates, then the string templates they pass to
    // `template_from_string()` (runtime/string_loader.rs)
    let mut queue: std::collections::VecDeque<(Vec<u8>, Vec<u8>, bool)> = templates
        .iter()
        .map(|(n, s)| (n.clone(), s.clone(), keep_failures))
        .collect();
    let mut seen: std::collections::HashSet<Vec<u8>> =
        templates.iter().map(|(n, _)| n.clone()).collect();
    for (name, string) in strings {
        let full = crate::runtime::string_loader::template_name(string, name.as_deref());
        if seen.insert(full.clone()) {
            queue.push_back((full, string.clone(), true));
        }
    }
    while let Some((name, source, from_string)) = queue.pop_front() {
        let src =
            crate::source::Source::new(source.clone(), String::from_utf8_lossy(&name).into_owned());
        let arc = std::sync::Arc::new(src.clone());
        // as `compileSource()`: errors get the source
        let mut strings = Vec::new();
        let compiled = env.parse_source(src).and_then(|module| {
            // before compiling: the compiler rewrites call arguments
            strings = string_templates(&module);
            Ok((Compiler::new(env).compile(&module)?, module))
        });
        let (module_code, module) = match compiled {
            Ok(compiled) => compiled,
            Err(mut e) if from_string => {
                // thrown when the function loads it
                e.set_source_context(Some(arc));
                let module = module_name(&name, None);
                let _ = writeln!(
                    code,
                    "pub static {module}: ::twig::runtime::string_loader::FailedTemplate = ::twig::runtime::string_loader::FailedTemplate {{ name: {}, code: {}, kind: ::twig::error::ErrorKind::{:?}, message: {}, line: {}, column: {:?} }};\n",
                    byte_lit(&name),
                    byte_lit(&source),
                    e.kind(),
                    byte_lit(e.raw_message()),
                    e.template_line(),
                    e.template_column()
                );
                entries.push(format!(
                    "({}, None, &{module}, {})",
                    byte_lit(&name),
                    byte_lit(&source)
                ));
                continue;
            }
            Err(mut e) => {
                e.set_source_context(Some(arc));
                return Err(e);
            }
        };
        code.push_str(&module_code);
        entries.push(format!(
            "({}, None, &{}::TEMPLATE, {})",
            byte_lit(&name),
            module_name(&name, None),
            byte_lit(&source)
        ));
        for template in embedded_modules(&module) {
            let index = template.attr("index").and_then(|a| a.as_int());
            entries.push(format!(
                "({}, {}, &{}::TEMPLATE, b\"\")",
                byte_lit(&name),
                index.map_or_else(|| "None".to_owned(), |i| format!("Some({i})")),
                module_name(&name, index)
            ));
        }
        for (string, string_name) in strings {
            let full =
                crate::runtime::string_loader::template_name(&string, string_name.as_deref());
            if seen.insert(full.clone()) {
                queue.push_back((full, string, true));
            }
        }
    }
    let _ = writeln!(
        code,
        "pub static TEMPLATES: &[(&[u8], Option<i64>, &dyn ::twig::runtime::CompiledTemplate, &[u8])] = &[\n    {}\n];",
        entries.join(",\n    ")
    );
    Ok(code)
}
