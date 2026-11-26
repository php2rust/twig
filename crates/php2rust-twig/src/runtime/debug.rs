//! Port of the runtime of Twig\Extension\DebugExtension.php: `dump()`.
//!
//! Upstream: twig/twig src/Extension/DebugExtension.php

use php_value::{PhpArray, PhpString, Value, var_dump::var_dump};

use super::{Call, R};
use crate::{error::Error, template::TemplateWrapper};

/// `DebugExtension::dump($env, $context, ...$vars)`: `var_dump()` of the
/// arguments, or of the context without its templates; null when the
/// environment is not in debug mode. An object is refused (a logic
/// error): php2rust-value's `var_dump()` cannot print object handles.
pub fn dump(call: &Call<'_>, vars: Vec<Value>) -> R {
    if !call.env.is_debug() {
        return Ok(Value::Null);
    }
    let vars = if vars.is_empty() {
        let mut context = PhpArray::new();
        for (key, value) in call.context.iter() {
            if !is_template(value) {
                context.insert(key.clone(), value.clone());
            }
        }
        vec![Value::Array(context)]
    } else {
        vars
    };
    let text = var_dump(&vars).map_err(|e| Error::logic(e.to_string()))?;
    Ok(Value::String(PhpString::from(text)))
}

/// `$value instanceof Template || $value instanceof TemplateWrapper`.
fn is_template(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let class = object.class();
            class == TemplateWrapper::CLASS
                || class == "Twig\\Template"
                || class.starts_with("__TwigTemplate_")
        }
        _ => false,
    }
}
