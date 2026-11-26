//! Port of Twig\Util\CallableArgumentsExtractor.php: maps the positional
//! and named arguments of a call to the parameters of the PHP callable.
//!
//! PHP reads the parameters by reflection; here the caller gives them as
//! [`Parameter`] values, in the PHP declaration order (all of them: the
//! input, charset, environment, context and sandbox parameters too, as
//! `ReflectionCallable::getTwigParameters()` strips them).
//!
//! Upstream: twig/twig src/Util/CallableArgumentsExtractor.php src/Util/ReflectionCallable.php

use symfony_deprecation_contracts::trigger_deprecation;

use crate::{
    callable::TwigCallable,
    error::Error,
    node::{
        Attr, Key, Node, NodeClass,
        expression::{array_of_class, constant},
    },
};

/// A parameter of a PHP callable, as reflection shows it.
#[derive(Debug, Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one flag for each ReflectionParameter answer"
)]
pub struct Parameter {
    pub name: String,
    /// `getDefaultValue()`, when `isDefaultValueAvailable()`.
    pub default: Option<Attr>,
    /// `isOptional()` without a default (a parameter of an internal PHP
    /// function).
    pub optional_without_default: bool,
    /// `isVariadic()`.
    pub variadic: bool,
    /// An `array` type with `[]` as default.
    pub array_default_empty: bool,
    /// `CallableParameters::isStringCoercionSafe()` of the declared type:
    /// a value of this type is never a `Stringable` or `Traversable`
    /// (`int`, `float`, `bool`, `null`, a final class such as `\Closure`),
    /// so the sandbox does not check what it passes.
    pub coercion_safe: bool,
}

impl Parameter {
    #[must_use]
    pub fn required(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            default: None,
            optional_without_default: false,
            variadic: false,
            array_default_empty: false,
            coercion_safe: false,
        }
    }

    #[must_use]
    pub fn with_default(name: &str, default: Attr) -> Self {
        Self {
            default: Some(default),
            ..Self::required(name)
        }
    }

    /// `$name = null`.
    #[must_use]
    pub fn optional_null(name: &str) -> Self {
        Self::with_default(name, Attr::Null)
    }

    /// `array $name = []`.
    #[must_use]
    pub fn array_default(name: &str) -> Self {
        Self {
            array_default_empty: true,
            ..Self::with_default(name, Attr::Array(Vec::new()))
        }
    }

    /// The same parameter with a type that cannot be coerced to a string
    /// (see [`Self::coercion_safe`]).
    #[must_use]
    pub fn coercion_safe(mut self) -> Self {
        self.coercion_safe = true;
        self
    }

    /// `...$name`.
    #[must_use]
    pub fn variadic(name: &str) -> Self {
        Self {
            variadic: true,
            ..Self::required(name)
        }
    }

    fn is_optional(&self) -> bool {
        self.default.is_some() || self.optional_without_default || self.variadic
    }
}

/// `ReflectionFunctionAbstract::getNumberOfRequiredParameters()`.
fn required_count(params: &[Parameter]) -> usize {
    params
        .iter()
        .rposition(|p| !p.is_optional())
        .map_or(0, |i| i + 1)
}

/// `ReflectionCallable::getTwigParameters()`: the parameters without the
/// ones Twig passes itself (charset, environment, context, sandbox state,
/// the callable's own `arguments`) and, with `strip_input`, the input
/// value of a filter or test.
pub fn twig_parameters<'a>(
    callable: &TwigCallable,
    params: &'a [Parameter],
    strip_input: bool,
) -> &'a [Parameter] {
    let skip = usize::from(strip_input)
        + usize::from(callable.get_needs_charset())
        + usize::from(callable.get_needs_environment())
        + usize::from(callable.get_needs_context())
        + usize::from(callable.get_needs_is_sandboxed())
        + callable.arguments().len();
    params.get(skip..).unwrap_or_default()
}

/// `normalizeName()`: lower case, no underscores.
fn normalize_name(name: &[u8]) -> Vec<u8> {
    name.iter()
        .filter(|&&b| b != b'_')
        .map(u8::to_ascii_lowercase)
        .collect()
}

/// `toSnakeCase()`: `/([A-Z]+)([A-Z][a-z])/` and `/([a-z0-9])([A-Z])/`
/// become `\1_\2`, then lower case.
pub fn to_snake_case(name: &[u8]) -> Vec<u8> {
    // first pattern: a run of capitals followed by a capital and a lower
    let mut first = Vec::with_capacity(name.len() + 4);
    let mut i = 0;
    while i < name.len() {
        if name[i].is_ascii_uppercase() {
            let start = i;
            let mut end = i;
            while end < name.len() && name[end].is_ascii_uppercase() {
                end += 1;
            }
            // greedy run, then backtrack one capital for `[A-Z][a-z]`
            if end - start >= 2 && name.get(end).is_some_and(u8::is_ascii_lowercase) {
                first.extend_from_slice(&name[start..end - 1]);
                first.push(b'_');
                first.push(name[end - 1]);
                i = end;
                continue;
            }
            first.extend_from_slice(&name[start..end]);
            i = end;
            continue;
        }
        first.push(name[i]);
        i += 1;
    }
    let mut out = Vec::with_capacity(first.len() + 4);
    let mut i = 0;
    while i < first.len() {
        let b = first[i];
        out.push(b);
        if (b.is_ascii_lowercase() || b.is_ascii_digit())
            && first.get(i + 1).is_some_and(u8::is_ascii_uppercase)
        {
            out.push(b'_');
            out.push(first[i + 1]);
            i += 2;
            continue;
        }
        i += 1;
    }
    out.make_ascii_lowercase();
    out
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `(new CallableArgumentsExtractor($node, $twigCallable))->extractArguments($arguments)`.
#[allow(clippy::too_many_lines)]
pub fn extract_arguments(
    node: &Node,
    callable: &TwigCallable,
    params: &[Parameter],
    arguments: &Node,
) -> Result<Vec<Node>, Error> {
    let kind = callable.callable_type().as_str();
    let call_name = text(callable.name());
    let syntax = |message: String| Error::syntax(message, node.line(), node.source_context());

    let mut extracted: Vec<(Key, Node)> = Vec::new();
    let mut original_names: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut named = false;
    for (name, value) in arguments.children() {
        let normalized = match &name {
            Key::Str(s) => {
                named = true;
                let normalized = normalize_name(s);
                original_names.push((normalized.clone(), s.clone()));
                Key::from_bytes(&normalized)
            }
            Key::Int(_) if named => {
                return Err(syntax(format!(
                    "Positional arguments cannot be used after named arguments for {kind} \"{call_name}\"."
                )));
            }
            Key::Int(i) => Key::Int(*i),
        };
        if let Some(slot) = extracted.iter_mut().find(|(k, _)| *k == normalized) {
            slot.1 = value;
        } else {
            extracted.push((normalized, value));
        }
    }

    if !named && !callable.is_variadic() {
        let min = callable.minimal_number_of_required_arguments();
        let required = required_count(params);
        if extracted.len() + min < required {
            let name = params
                .get(min + extracted.len())
                .map(|p| to_snake_case(p.name.as_bytes()))
                .unwrap_or_default();
            return Err(syntax(format!(
                "Value for argument \"{}\" is required for {kind} \"{call_name}\".",
                text(&name)
            )));
        }
        return Ok(extracted.into_iter().map(|(_, n)| n).collect());
    }

    let Some(php_callable) = callable.callable() else {
        let message = if named {
            format!("Named arguments are not supported for {kind} \"{call_name}\".")
        } else {
            format!("Arbitrary positional arguments are not supported for {kind} \"{call_name}\".")
        };
        return Err(Error::syntax(message, -1, None));
    };

    let mut parameters = twig_parameters(callable, params, node.has_node("node")).to_vec();
    let mut is_php_variadic = false;
    if callable.is_variadic() {
        match parameters.last() {
            Some(last) if last.array_default_empty => {
                parameters.pop();
            }
            Some(last) if last.variadic => {
                parameters.pop();
                is_php_variadic = true;
            }
            _ => {
                return Err(Error::syntax(
                    format!(
                        "The last parameter of \"{php_callable}\" for {kind} \"{call_name}\" must be an array with default value, eg. \"array $arg = []\"."
                    ),
                    -1,
                    None,
                ));
            }
        }
    }

    let take = |extracted: &mut Vec<(Key, Node)>, key: &Key| -> Option<Node> {
        let i = extracted.iter().position(|(k, _)| k == key)?;
        Some(extracted.remove(i).1)
    };

    let mut result: Vec<Node> = Vec::new();
    let mut parameter_names: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut optional: Vec<Node> = Vec::new();
    let mut pos: i64 = 0;
    for parameter in &parameters {
        let mut name = parameter.name.clone();
        if php_callable == "range" {
            if name == "start" {
                "low".clone_into(&mut name);
            } else if name == "end" {
                "high".clone_into(&mut name);
            }
        }
        parameter_names.push(name.clone());
        let normalized = Key::from_bytes(&normalize_name(name.as_bytes()));
        if extracted.iter().any(|(k, _)| *k == normalized) {
            if extracted.iter().any(|(k, _)| *k == Key::Int(pos)) {
                return Err(syntax(format!(
                    "Argument \"{name}\" is defined twice for {kind} \"{call_name}\"."
                )));
            }
            if !missing.is_empty() {
                let snake: Vec<String> = parameter_names
                    .iter()
                    .map(|n| text(&to_snake_case(n.as_bytes())))
                    .collect();
                return Err(syntax(format!(
                    "Argument \"{name}\" could not be assigned for {kind} \"{call_name}({})\" because it is mapped to an internal PHP function which cannot determine default value for optional argument{} \"{}\".",
                    snake.join(", "),
                    if missing.len() > 1 { "s" } else { "" },
                    missing.join("\", \"")
                )));
            }
            result.append(&mut optional);
            if let Some(value) = take(&mut extracted, &normalized) {
                result.push(value);
            }
        } else if let Some(value) = take(&mut extracted, &Key::Int(pos)) {
            result.append(&mut optional);
            result.push(value);
            pos += 1;
        } else if let Some(default) = &parameter.default {
            optional.push(constant(default.clone(), node.line()));
        } else if parameter.is_optional() {
            if extracted.is_empty() {
                break;
            }
            missing.push(name);
        } else {
            return Err(syntax(format!(
                "Value for argument \"{}\" is required for {kind} \"{call_name}\".",
                text(&to_snake_case(name.as_bytes()))
            )));
        }
    }

    if callable.is_variadic() {
        let class = if is_php_variadic {
            NodeClass::VariadicExpression
        } else {
            NodeClass::ArrayExpression
        };
        let arbitrary = array_of_class(class, Vec::new(), node.line());
        for (key, value) in std::mem::take(&mut extracted) {
            match key {
                Key::Int(_) => arbitrary.add_element(value, None),
                Key::Str(normalized) => {
                    let original = original_names
                        .iter()
                        .find(|(n, _)| *n == normalized)
                        .map_or_else(|| normalized.clone(), |(_, o)| o.clone());
                    let snake = to_snake_case(&original);
                    if original != snake {
                        let mut message = b"Using \"snake_case\" for variadic arguments is required for a smooth upgrade with Twig 4.0; rename \"".to_vec();
                        message.extend_from_slice(&original);
                        message.extend_from_slice(b"\" to \"");
                        message.extend_from_slice(&snake);
                        message.extend_from_slice(b"\" in \"");
                        message
                            .extend_from_slice(node.template_name().unwrap_or_default().as_bytes());
                        message
                            .extend_from_slice(format!("\" at line {}.", node.line()).as_bytes());
                        trigger_deprecation(b"twig/twig", b"3.15", &message);
                    }
                    arbitrary.add_element(value, Some(constant(Attr::Str(snake), node.line())));
                }
            }
        }
        if arbitrary.count() > 0 {
            result.append(&mut optional);
            result.push(arbitrary);
        }
    }

    if !extracted.is_empty() {
        let unknown = extracted.first().map(|(_, n)| n.clone());
        let unknown_names: Vec<String> = extracted.iter().map(|(k, _)| k.to_string()).collect();
        let parameter_names: Vec<String> = parameter_names
            .iter()
            .map(|n| text(&to_snake_case(n.as_bytes())))
            .collect();
        let message = format!(
            "Unknown argument{} \"{}\" for {kind} \"{call_name}({})\".",
            if extracted.len() > 1 { "s" } else { "" },
            unknown_names.join("\", \""),
            parameter_names.join(", ")
        );
        return Err(match unknown {
            Some(n) => Error::syntax(message, n.line(), n.source_context()),
            None => syntax(message),
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::to_snake_case;

    #[test]
    fn snake_case_copies_the_two_patterns() {
        assert_eq!(to_snake_case(b"fooBar"), b"foo_bar");
        assert_eq!(to_snake_case(b"HTMLParser"), b"html_parser");
        assert_eq!(to_snake_case(b"already_snake"), b"already_snake");
        assert_eq!(to_snake_case(b"a1B"), b"a1_b");
        assert_eq!(to_snake_case(b"ABC"), b"abc");
    }
}
