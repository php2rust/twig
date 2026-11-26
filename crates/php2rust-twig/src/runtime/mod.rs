//! The runtime the compiled templates call: the PHP expressions and
//! statements the PHP compiler writes inline (`$context['x'] ?? null`,
//! `foreach`, array literals, operators), as functions, and the
//! re-exports the generated code uses (`use ::twig::runtime as rt;`).
//!
//! The PHP code of the callables (`CoreExtension`, `EscaperRuntime`) is
//! ported in the submodules.
//!
//! Upstream: none

pub mod core;
pub mod escaper;
pub mod string;

use std::sync::Arc;

pub use php_value::{Key, PhpArray, PhpString, Value, ops};
pub use std::rc::Rc;

pub use crate::{
    environment::Environment,
    error::{Error, ErrorKind},
    loader::{CompiledArrayLoader, TemplateLoader},
    macro_namespace::MacroNamespace,
    markup::Markup,
    source::Source,
    template::{BlockRef, Blocks, CompiledTemplate, Context, Macros, Template, TemplateWrapper},
    twig_macro::TwigMacro,
};

/// A result of the runtime.
pub type R<T = Value> = Result<T, Error>;

/// What a Twig callable gets besides its arguments: the environment
/// (`needs_environment`, `needs_charset`), the context (`needs_context`),
/// the template that calls it and the line of the call.
pub struct Call<'a> {
    pub env: &'a Environment,
    pub template: &'a Rc<Template>,
    pub context: &'a Context,
    pub line: i64,
}

impl<'a> Call<'a> {
    pub fn new(
        env: &'a Environment,
        template: &'a Rc<Template>,
        context: &'a Context,
        line: i64,
    ) -> Self {
        Self {
            env,
            template,
            context,
            line,
        }
    }

    /// `$this->env->getCharset()`.
    #[must_use]
    pub fn charset(&self) -> &[u8] {
        self.env.charset()
    }

    /// `$this->source`.
    #[must_use]
    pub fn source(&self) -> &Arc<Source> {
        self.template.source()
    }
}

/// A Twig callable compiled to Rust: the call, then the arguments in PHP
/// order (the callable's bound arguments, the input of a filter or test,
/// then the template arguments; a variadic callable gets the extra
/// arguments as one trailing array, or spread for a PHP variadic).
pub type Callable = fn(&Call<'_>, Vec<Value>) -> R;

/// A string literal.
#[must_use]
pub fn s(bytes: &[u8]) -> Value {
    Value::String(PhpString::from(bytes))
}

/// `$context['name'] ?? null`.
#[must_use]
pub fn get(context: &Context, name: &[u8]) -> Value {
    context
        .get(&Key::from_bytes(name))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A variable in strict mode: `isset($context[$name]) ||
/// array_key_exists($name, $context) ? $context[$name] : throw ...`.
pub fn get_strict(context: &Context, name: &[u8], line: i64, template: &Template) -> R {
    match context.get(&Key::from_bytes(name)) {
        Some(value) => Ok(value.clone()),
        None => Err(Error::runtime(
            format!(
                "Variable \"{}\" does not exist.",
                String::from_utf8_lossy(name)
            ),
            line,
            Some(Arc::clone(template.source())),
        )),
    }
}

/// `array_key_exists($name, $context)`.
#[must_use]
pub fn has(context: &Context, name: &[u8]) -> bool {
    context.contains_key(&Key::from_bytes(name))
}

/// `$context[$name] = $value`.
pub fn set(context: &mut Context, name: &[u8], value: Value) {
    context.insert(Key::from_bytes(name), value);
}

/// `$context[$name] = $value` as an expression: the value.
pub fn assign(context: &mut Context, name: &[u8], value: Value) -> Value {
    set(context, name, value.clone());
    value
}

/// `unset($context[$name])`.
pub fn unset(context: &mut Context, name: &[u8]) {
    context.remove(&Key::from_bytes(name));
}

/// `(string) $value`: an array is `"Array"` (PHP warns), an object needs
/// `__toString()`.
pub fn to_php_string(value: &Value) -> R<PhpString> {
    Ok(ops::to_string(value)?)
}

/// `(string) $value` as a value.
pub fn string_cast(value: &Value) -> R {
    Ok(Value::String(to_php_string(value)?))
}

/// The bytes of `(string) $value`.
pub fn bytes(value: &Value) -> R<Vec<u8>> {
    Ok(to_php_string(value)?.as_bytes().to_vec())
}

/// `yield (string) $value;`.
pub fn echo(out: &mut Vec<u8>, value: &Value) -> R<()> {
    out.extend_from_slice(to_php_string(value)?.as_bytes());
    Ok(())
}

/// `(bool) $value`.
#[must_use]
pub fn truthy(value: &Value) -> bool {
    value.is_truthy()
}

/// `$a == $b`.
#[must_use]
pub fn eq(a: &Value, b: &Value) -> Value {
    Value::Bool(php_value::compare::loose_equals(a, b))
}

/// `$a != $b`.
#[must_use]
pub fn ne(a: &Value, b: &Value) -> Value {
    Value::Bool(!php_value::compare::loose_equals(a, b))
}

/// `$a < $b`.
#[must_use]
pub fn lt(a: &Value, b: &Value) -> Value {
    Value::Bool(ops::compare(a, b) == std::cmp::Ordering::Less)
}

/// `$a <= $b`.
#[must_use]
pub fn le(a: &Value, b: &Value) -> Value {
    Value::Bool(ops::compare(a, b) != std::cmp::Ordering::Greater)
}

/// `$a > $b` (PHP compares `$b < $a`).
#[must_use]
pub fn gt(a: &Value, b: &Value) -> Value {
    Value::Bool(ops::compare(b, a) == std::cmp::Ordering::Less)
}

/// `$a >= $b` (PHP compares `$b <= $a`).
#[must_use]
pub fn ge(a: &Value, b: &Value) -> Value {
    Value::Bool(ops::compare(b, a) != std::cmp::Ordering::Greater)
}

/// `$a <=> $b`.
#[must_use]
pub fn spaceship(a: &Value, b: &Value) -> Value {
    Value::Int(ops::spaceship(a, b))
}

/// `$a === $b`.
#[must_use]
pub fn same(a: &Value, b: &Value) -> Value {
    Value::Bool(a.identical(b))
}

/// `(int) floor($a / $b)`.
pub fn floor_div(a: &Value, b: &Value) -> R {
    let quotient = ops::div(a, b)?;
    Ok(Value::Int(ops::float_to_int(
        ops::to_float(&quotient).floor(),
    )))
}

/// `is_string($a) && is_string($b) && str_starts_with($a, $b)`.
#[must_use]
pub fn starts_with(a: &Value, b: &Value) -> Value {
    Value::Bool(match (a, b) {
        (Value::String(a), Value::String(b)) => a.as_bytes().starts_with(b.as_bytes()),
        _ => false,
    })
}

/// `is_string($a) && is_string($b) && str_ends_with($a, $b)`.
#[must_use]
pub fn ends_with(a: &Value, b: &Value) -> Value {
    Value::Bool(match (a, b) {
        (Value::String(a), Value::String(b)) => a.as_bytes().ends_with(b.as_bytes()),
        _ => false,
    })
}

/// `$tmp instanceof Markup ? (string) $tmp : $tmp` (the `true` test,
/// which makes an empty `Markup` false).
#[must_use]
pub fn true_test(value: &Value) -> bool {
    match value {
        Value::Object(o) => match o.downcast_ref::<Markup>() {
            Some(markup) => php_value::string::truthy(markup.content().as_bytes()),
            None => true,
        },
        other => other.is_truthy(),
    }
}

fn illegal_offset(key: &Value) -> Error {
    Error::exception(
        "TypeError",
        format!("Illegal offset type {}", key.debug_type()).into_bytes(),
    )
}

/// An array literal, built element by element.
#[derive(Default)]
pub struct ArrayBuilder(PhpArray);

impl ArrayBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An element without a key (`[$value]`).
    #[must_use]
    pub fn push(mut self, value: Value) -> Self {
        // A literal never fills the last int key.
        let _ = self.0.push(value);
        self
    }

    /// `$key => $value`.
    pub fn key(mut self, key: &Value, value: Value) -> R<Self> {
        let key = Key::from_value(key).ok_or_else(|| illegal_offset(key))?;
        self.0.insert(key, value);
        Ok(self)
    }

    /// `...$value`: int keys are renumbered, string keys kept.
    pub fn spread(mut self, value: &Value) -> R<Self> {
        for (key, item) in spread_pairs(value)? {
            match key {
                Value::String(_) => {
                    self = self.key(&key, item)?;
                }
                _ => {
                    let _ = self.0.push(item);
                }
            }
        }
        Ok(self)
    }

    #[must_use]
    pub fn array(self) -> PhpArray {
        self.0
    }

    #[must_use]
    pub fn done(self) -> Value {
        Value::Array(self.0)
    }
}

fn spread_pairs(value: &Value) -> R<Vec<(Value, Value)>> {
    match value {
        Value::Array(a) => Ok(a.iter().map(|(k, v)| (k.to_value(), v.clone())).collect()),
        Value::Object(o) => match o.traverse() {
            Some(pairs) => Ok(pairs?),
            None => Err(Error::exception(
                "Error",
                "Only arrays and Traversables can be unpacked",
            )),
        },
        _ => Err(Error::exception(
            "Error",
            "Only arrays and Traversables can be unpacked",
        )),
    }
}

/// `...$value` in the arguments of a call: the values in order.
pub fn spread_values(value: &Value) -> R<Vec<Value>> {
    Ok(spread_pairs(value)?.into_iter().map(|(_, v)| v).collect())
}

/// `foreach ($seq as $key => $value)`: the pairs, read before the loop
/// runs (PHP iterates a copy of an array).
pub fn iterate(seq: &Value) -> R<Vec<(Value, Value)>> {
    match seq {
        Value::Array(a) => Ok(a.iter().map(|(k, v)| (k.to_value(), v.clone())).collect()),
        Value::Object(o) => match o.traverse() {
            Some(pairs) => Ok(pairs?),
            None => Ok(Vec::new()),
        },
        _ => Ok(Vec::new()),
    }
}

/// The start of a `for` loop with the `loop` variable: `$context['loop']`
/// and its length when the sequence is an array or `\Countable`.
pub fn loop_start(context: &mut Context, seq: &Value) -> R<()> {
    let mut loop_var = PhpArray::new();
    loop_var.insert("parent", get(context, b"_parent"));
    loop_var.insert("index0", Value::Int(0));
    loop_var.insert("index", Value::Int(1));
    loop_var.insert("first", Value::Bool(true));
    let length = match seq {
        Value::Array(a) => Some(i64::try_from(a.len()).unwrap_or(i64::MAX)),
        Value::Object(o) => o.php_count().transpose()?,
        _ => None,
    };
    if let Some(length) = length {
        loop_var.insert("revindex0", Value::Int(length - 1));
        loop_var.insert("revindex", Value::Int(length));
        loop_var.insert("length", Value::Int(length));
        loop_var.insert("last", Value::Bool(length == 1));
    }
    set(context, b"loop", Value::Array(loop_var));
    Ok(())
}

/// The end of each iteration (`ForLoopNode`): the `loop` counters move.
pub fn loop_next(context: &mut Context) {
    let Some(Value::Array(loop_var)) = context.get_mut(&Key::from_bytes(b"loop")) else {
        return;
    };
    let incr = |v: Option<&Value>, by: i64| -> Value {
        match v {
            Some(Value::Int(i)) => Value::Int(i + by),
            Some(other) => ops::add(other, &Value::Int(by)).unwrap_or(Value::Null),
            None => Value::Int(by),
        }
    };
    let index0 = incr(loop_var.get(&Key::from("index0")), 1);
    loop_var.insert("index0", index0);
    let index = incr(loop_var.get(&Key::from("index")), 1);
    loop_var.insert("index", index);
    loop_var.insert("first", Value::Bool(false));
    let has_rev = loop_var
        .get(&Key::from("revindex0"))
        .is_some_and(|v| !v.is_null())
        && loop_var
            .get(&Key::from("revindex"))
            .is_some_and(|v| !v.is_null());
    if has_rev {
        let revindex0 = incr(loop_var.get(&Key::from("revindex0")), -1);
        let last = Value::Bool(revindex0.identical(&Value::Int(0)));
        loop_var.insert("revindex0", revindex0);
        let revindex = incr(loop_var.get(&Key::from("revindex")), -1);
        loop_var.insert("revindex", revindex);
        loop_var.insert("last", last);
    }
}

/// The end of a `for` loop: `$_parent = $context['_parent']`, the loop's
/// variables are unset, then `$context = array_intersect_key($context,
/// $_parent) + $_parent`.
pub fn loop_end(context: &mut Context, names: &[&[u8]]) {
    let parent = match get(context, b"_parent") {
        Value::Array(a) => a,
        _ => PhpArray::new(),
    };
    for name in names {
        unset(context, name);
    }
    let mut kept = PhpArray::new();
    for (key, value) in context.iter() {
        if parent.contains_key(key) {
            kept.insert(key.clone(), value.clone());
        }
    }
    kept.union(&parent);
    *context = kept;
}

/// The array a template argument must be (`array $context`): PHP throws
/// a `TypeError` for anything else.
pub fn array_arg(value: Value, what: &str) -> R<PhpArray> {
    match value {
        Value::Array(a) => Ok(a),
        other => Err(Error::exception(
            "TypeError",
            format!(
                "{what}: Argument #1 ($context) must be of type array, {} given",
                other.debug_type()
            )
            .into_bytes(),
        )),
    }
}

/// `$macros[$name] ?? $this->throwUninitializedMacroNamespace($line)`.
pub fn macro_namespace(
    macros: &Macros,
    name: &[u8],
    template: &Template,
    line: i64,
) -> R<Rc<MacroNamespace>> {
    macros
        .get(name)
        .cloned()
        .ok_or_else(|| template.uninitialized_macro_namespace(line))
}

/// A capture (`CaptureNode`): the output of a body, as a `Markup` unless
/// empty, or as a string when `raw`.
pub fn capture(env: &Environment, output: Vec<u8>, raw: bool) -> Value {
    if raw || output.is_empty() {
        return Value::String(PhpString::from(output));
    }
    Markup::value(output, env.charset_string())
}

/// `('' === $tmp = $value) ? '' : new Markup($tmp, $charset)` (a `set`
/// of a safe value).
pub fn markup_unless_empty(env: &Environment, value: &Value) -> R {
    if matches!(value, Value::String(s) if s.is_empty()) {
        return Ok(s(b""));
    }
    Ok(Markup::value(to_php_string(value)?, env.charset_string()))
}

/// Calls an arrow function or a closure value: `$arrow(...$arguments)`.
pub fn call_arrow(env: &Environment, arrow: &Value, arguments: &[Value]) -> R {
    if let Value::Object(o) = arrow {
        if let Some(arrow) = o.downcast_ref::<core::Arrow>() {
            let f = env.arrow(arrow.id()).ok_or_else(|| {
                Error::exception("Error", "Object of type Closure is not callable")
            })?;
            return f(env, arguments);
        }
        if let Some(closure) = o.downcast_ref::<php_value::object::Closure>() {
            return Ok(closure.call(arguments));
        }
        return Err(Error::exception(
            "Error",
            format!("Object of type {} is not callable", o.class()).into_bytes(),
        ));
    }
    Err(Error::exception("Error", "Value not callable"))
}

/// Ends a generated method: the line of the last debug info the method
/// passed, and its template, go to the error (`handleException()`).
pub fn guess<T>(result: R<T>, line: i64, template: &Template) -> R<T> {
    result.map_err(|e| e.guess_line(line, template.source()))
}

/// `{% deprecated %}`: `trigger_deprecation($package, $version,
/// $message.' in "name" at line N.')`.
pub fn deprecated(package: &Value, version: &Value, message: &Value, suffix: &[u8]) -> R<()> {
    let mut text = bytes(message)?;
    text.extend_from_slice(suffix);
    symfony_deprecation_contracts::trigger_deprecation(&bytes(package)?, &bytes(version)?, &text);
    Ok(())
}

/// `$this->load($template, $line, $index)` for a template that may be
/// missing (`ignore missing`): `None` on a `LoaderError`.
pub fn load_optional(
    template: &Template,
    env: &Environment,
    name: &Value,
    line: i64,
    index: Option<i64>,
) -> R<Option<Rc<Template>>> {
    match template.load(env, name, line, index) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == ErrorKind::Loader => Ok(None),
        Err(e) => Err(e),
    }
}

/// The arguments of a macro call, compiled as an array expression.
#[must_use]
pub fn args_array(value: Value) -> PhpArray {
    match value {
        Value::Array(a) => a,
        _ => PhpArray::new(),
    }
}

/// `new RuntimeError($message, $line, $this->getSourceContext())`.
pub fn runtime_error(message: Vec<u8>, line: i64, template: &Template) -> Error {
    Error::runtime(message, line, Some(template.source().clone()))
}

/// `SequenceDestructuringSetBinary`: `[$context['a'], $context['b']] =
/// array_pad($v, 2, null)` for an array, or
/// `CoreExtension::destructureSequence()` for a `\Traversable`. The value
/// of the expression is the padded array or the sequence.
pub fn destructure_sequence(context: &mut Context, names: &[Option<&[u8]>], value: Value) -> R {
    match &value {
        Value::Object(o) if o.instance_of("Traversable") => {
            if names.is_empty() {
                return Ok(value);
            }
            let values = iterate(&value)?;
            for (i, name) in names.iter().enumerate() {
                if let Some(name) = name {
                    let v = values.get(i).map_or(Value::Null, |(_, v)| v.clone());
                    set(context, name, v);
                }
            }
            Ok(value)
        }
        Value::Array(a) => {
            let mut padded = a.clone();
            let count = i64::try_from(names.len()).unwrap_or(i64::MAX);
            let mut len = i64::try_from(padded.len()).unwrap_or(i64::MAX);
            while len < count {
                let _ = padded.push(Value::Null);
                len += 1;
            }
            for (i, name) in names.iter().enumerate() {
                if let Some(name) = name {
                    let key = Key::Int(i64::try_from(i).unwrap_or(i64::MAX));
                    let v = padded.get(&key).cloned().unwrap_or(Value::Null);
                    set(context, name, v);
                }
            }
            Ok(Value::Array(padded))
        }
        other => Err(Error::exception(
            "TypeError",
            format!(
                "array_pad(): Argument #1 ($array) must be of type array, {} given",
                other.debug_type()
            )
            .into_bytes(),
        )),
    }
}

/// `$vars + $context + $this->env->getGlobals()` (`WithNode`).
pub fn with_context(vars: PhpArray, context: &Context, env: &Environment) -> Context {
    let mut out = vars;
    out.union(context);
    out.union(&env.globals());
    out
}

/// `EmbedNode` with `ignore missing`: `$this->load($name, $line, $index)`
/// then `->getParent($context)`, `None` on a `LoaderError` of either.
pub fn load_embed_optional(
    template: &Template,
    env: &Environment,
    name: &Value,
    line: i64,
    index: Option<i64>,
    context: &Context,
) -> R<Option<Rc<Template>>> {
    let loaded = template
        .load(env, name, line, index)
        .and_then(|t| t.get_parent(env, context).map(|_| t));
    match loaded {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == ErrorKind::Loader => Ok(None),
        Err(e) => Err(e),
    }
}
