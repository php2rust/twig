//! Port of the callables of `Twig\Tests\TwigTestExtension`
//! (tests/IntegrationTest.php), as compiled templates call them: the
//! arguments in order, after the environment.
//!
//! Upstream: twig/twig tests/IntegrationTest.php

// The callables share one signature (`fn(&Call, Vec<Value>) -> R`), so
// they take the arguments by value even when they only read them.
#![allow(clippy::needless_pass_by_value)]

use twig::runtime::{Call, R, Value, bytes, s, same};

fn arg(arguments: &[Value], i: usize) -> R<Vec<u8>> {
    arguments.get(i).map_or_else(|| Ok(Vec::new()), bytes)
}

fn joined(parts: &[&[u8]]) -> Value {
    s(&parts.concat())
}

/// `§Filter()` and `§Function()`: `"§{$value}§"`.
pub fn section(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(joined(&[
        "§".as_bytes(),
        &arg(&arguments, 0)?,
        "§".as_bytes(),
    ]))
}

/// `nl2br($value, $sep = '<br />')`: `str_replace("\n", "$sep\n", $value
/// ?? '')`.
pub fn nl2br(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(s(&nl2br_bytes(&arguments)?))
}

fn nl2br_bytes(arguments: &[Value]) -> R<Vec<u8>> {
    let value = arg(arguments, 0)?;
    let sep = match arguments.get(1) {
        Some(v) => bytes(v)?,
        None => b"<br />".to_vec(),
    };
    let mut out = Vec::with_capacity(value.len());
    for &b in &value {
        if b == b'\n' {
            out.extend_from_slice(&sep);
        }
        out.push(b);
    }
    Ok(out)
}

/// `escape_and_nl2br($env, $value, $sep = '<br />')`.
pub fn escape_and_nl2br(call: &Call<'_>, mut arguments: Vec<Value>) -> R {
    let value = arguments.first().cloned().unwrap_or(Value::Null);
    let escaped = call.env.escaper().escape(&value, b"html", None, false)?;
    if let Some(first) = arguments.first_mut() {
        *first = escaped;
    } else {
        arguments.push(escaped);
    }
    nl2br(call, arguments)
}

/// `escape_something()` and `preserves_safety()`: `strtoupper($value)`.
pub fn upper(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(s(&arg(&arguments, 0)?.to_ascii_uppercase()))
}

/// `staticCall($value)`: `"*$value*"`.
pub fn static_call(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(joined(&[b"*", &arg(&arguments, 0)?, b"*"]))
}

/// `__call('magicCall', $arguments)`: `'magic_'.$arguments[0]`.
pub fn magic_call(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(joined(&[b"magic_", &arg(&arguments, 0)?]))
}

/// `__callStatic('magicStaticCall', $arguments)`.
pub fn magic_static_call(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(joined(&[b"static_magic_", &arg(&arguments, 0)?]))
}

/// `dynamic_path($element, $item)`: `$element.'/'.$item`.
pub fn dynamic_path(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(joined(&[&arg(&arguments, 0)?, b"/", &arg(&arguments, 1)?]))
}

/// `dynamic_foo($foo, $bar, $item)`.
pub fn dynamic_foo(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(joined(&[
        &arg(&arguments, 0)?,
        b"/",
        &arg(&arguments, 1)?,
        b"/",
        &arg(&arguments, 2)?,
    ]))
}

/// `notFilter($value)`: `'not '.$value`.
pub fn not(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(joined(&[b"not ", &arg(&arguments, 0)?]))
}

/// The `anon_foo` closure: `'*'.$name.'*'`.
pub fn anon_foo(call: &Call<'_>, arguments: Vec<Value>) -> R {
    static_call(call, arguments)
}

/// `br()`.
pub fn br(_call: &Call<'_>, _arguments: Vec<Value>) -> R {
    Ok(s(b"<br />"))
}

/// The `deprecated_function` closure.
pub fn deprecated_function(_call: &Call<'_>, _arguments: Vec<Value>) -> R {
    Ok(s(b"foo"))
}

/// `is_multi_word($value)`: `str_contains($value, ' ')`.
pub fn is_multi_word(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(Value::Bool(arg(&arguments, 0)?.contains(&b' ')))
}

/// `dynamic_test($element, $item)`: `$element === $item`.
pub fn dynamic_test(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    let a = arguments.first().cloned().unwrap_or(Value::Null);
    let b = arguments.get(1).cloned().unwrap_or(Value::Null);
    Ok(same(&a, &b))
}
