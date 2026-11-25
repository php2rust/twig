//! Port of Twig\TwigMacro.php: a compiled macro, its signature, and the
//! checks of a call's arguments before they are bound.
//!
//! PHP binds the arguments to the macro's closure natively (positional
//! ones in order, named ones by parameter name, extra ones into the
//! variadic bucket); [`TwigMacro::call_legacy`] does that binding for the
//! compiled Rust function.
//!
//! Upstream: twig/twig src/TwigMacro.php

use std::{fmt, rc::Rc};

use php_value::{Key, PhpArray, Value};
use symfony_deprecation_contracts::trigger_deprecation;

use crate::{environment::Environment, error::Error, source::Source, template::Template};

/// A compiled macro body: the template, the environment, one slot per
/// declared argument (`None` when the call does not give it), and the
/// variadic bucket.
pub type MacroBody =
    fn(&Rc<Template>, &Environment, Vec<Option<Value>>, PhpArray) -> Result<Value, Error>;

/// `Twig\TwigMacro`.
#[derive(Clone)]
pub struct TwigMacro {
    name: Vec<u8>,
    body: MacroBody,
    /// The declared arguments and whether each has an explicit default.
    arguments: Vec<(Vec<u8>, bool)>,
    variadic: bool,
    required_count: usize,
}

impl fmt::Debug for TwigMacro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwigMacro")
            .field("name", &String::from_utf8_lossy(&self.name))
            .finish_non_exhaustive()
    }
}

impl TwigMacro {
    /// `new TwigMacro($name, $body, $arguments, $variadic)`.
    pub fn new(name: &[u8], body: MacroBody, arguments: &[(&[u8], bool)], variadic: bool) -> Self {
        let arguments: Vec<(Vec<u8>, bool)> =
            arguments.iter().map(|(n, d)| (n.to_vec(), *d)).collect();
        let required_count = arguments
            .iter()
            .rposition(|(_, has_default)| !has_default)
            .map_or(0, |i| i + 1);
        Self {
            name: name.to_vec(),
            body,
            arguments,
            variadic,
            required_count,
        }
    }

    pub fn name(&self) -> &[u8] {
        &self.name
    }

    fn index_of(&self, name: &[u8]) -> Option<usize> {
        self.arguments.iter().position(|(n, _)| n == name)
    }

    fn is_required(&self, name: &[u8]) -> bool {
        self.arguments.iter().any(|(n, d)| n == name && !d)
    }

    fn name_text(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }

    /// `callLegacy($arguments, $source, $lineno)`.
    pub fn call_legacy(
        &self,
        template: &Rc<Template>,
        env: &Environment,
        arguments: &PhpArray,
        source: &Source,
        lineno: i64,
    ) -> Result<Value, Error> {
        if arguments.is_list() {
            let count = arguments.len();
            if count < self.required_count || (!self.variadic && count > self.arguments.len()) {
                self.trigger_legacy_deprecations(arguments, count, source, lineno);
            }
            return self.bind(template, env, arguments);
        }
        let all_named = arguments.len() == self.arguments.len()
            && arguments.keys().all(|k| match k {
                Key::Str(s) => self.index_of(s.as_bytes()).is_some(),
                Key::Int(_) => false,
            });
        if all_named {
            return self.bind(template, env, arguments);
        }
        let mut positional = 0;
        let mut named_required = 0;
        let mut saw_named = false;
        let mut misordered = false;
        let mut duplicate: Option<Vec<u8>> = None;
        let mut unknown_named = false;
        for key in arguments.keys() {
            match key {
                Key::Int(_) => {
                    misordered = misordered || saw_named;
                    positional += 1;
                }
                Key::Str(s) => {
                    saw_named = true;
                    match self.index_of(s.as_bytes()) {
                        None => unknown_named = true,
                        Some(i) => {
                            if duplicate.is_none() && i < positional {
                                duplicate = Some(s.as_bytes().to_vec());
                            }
                            if self.is_required(s.as_bytes()) {
                                named_required += 1;
                            }
                        }
                    }
                }
            }
        }
        let source_arc = std::sync::Arc::new(source.clone());
        if misordered {
            return Err(Error::runtime(
                format!(
                    "Positional arguments cannot be used after named arguments for macro \"{}\".",
                    self.name_text()
                ),
                lineno,
                Some(source_arc),
            ));
        }
        if let Some(duplicate) = duplicate {
            return Err(Error::runtime(
                format!(
                    "Argument \"{}\" is defined twice for macro \"{}\".",
                    String::from_utf8_lossy(&duplicate),
                    self.name_text()
                ),
                lineno,
                Some(source_arc),
            ));
        }
        let required_total = self.arguments.iter().filter(|(_, d)| !d).count();
        let may_miss_required = if positional == 0 {
            named_required < required_total
        } else {
            positional < self.required_count
        };
        if may_miss_required
            || (!self.variadic && (unknown_named || positional > self.arguments.len()))
        {
            self.trigger_legacy_deprecations(arguments, positional, source, lineno);
        }
        self.bind(template, env, arguments)
    }

    /// PHP's binding of the arguments to the closure's parameters.
    fn bind(
        &self,
        template: &Rc<Template>,
        env: &Environment,
        arguments: &PhpArray,
    ) -> Result<Value, Error> {
        let mut slots: Vec<Option<Value>> = vec![None; self.arguments.len()];
        let mut bucket = PhpArray::new();
        let mut position = 0;
        for (key, value) in arguments.iter() {
            match key {
                Key::Int(_) => {
                    if position < slots.len() {
                        slots[position] = Some(value.clone());
                    } else {
                        let _ = bucket.push(value.clone());
                    }
                    position += 1;
                }
                Key::Str(name) => match self.index_of(name.as_bytes()) {
                    Some(i) => slots[i] = Some(value.clone()),
                    None => bucket.insert(key.clone(), value.clone()),
                },
            }
        }
        (self.body)(template, env, slots, bucket)
    }

    /// `triggerLegacyDeprecations()`.
    fn trigger_legacy_deprecations(
        &self,
        arguments: &PhpArray,
        positional: usize,
        source: &Source,
        lineno: i64,
    ) {
        if positional < self.required_count {
            for (i, (name, has_default)) in self.arguments.iter().enumerate() {
                if *has_default || i < positional || arguments.contains_key(&Key::from_bytes(name))
                {
                    continue;
                }
                trigger_deprecation(
                    b"twig/twig",
                    b"3.29",
                    format!(
                        "Not passing a value for the \"{}\" argument of macro \"{}\" is deprecated and the argument will be required in Twig 4.0; give it a default value in the macro definition or pass a value when calling it (in \"{}\" at line {lineno}).",
                        String::from_utf8_lossy(name),
                        self.name_text(),
                        source.name()
                    )
                    .as_bytes(),
                );
            }
        }
        if self.variadic {
            return;
        }
        if positional > self.arguments.len() {
            trigger_deprecation(
                b"twig/twig",
                b"3.29",
                format!(
                    "Passing more arguments than the macro \"{}\" accepts is deprecated and will throw in Twig 4.0; declare a variadic argument (\"...name\") in the macro definition to accept extra arguments (in \"{}\" at line {lineno}).",
                    self.name_text(),
                    source.name()
                )
                .as_bytes(),
            );
        }
        for key in arguments.keys() {
            if let Key::Str(name) = key
                && self.index_of(name.as_bytes()).is_none()
            {
                trigger_deprecation(
                    b"twig/twig",
                    b"3.29",
                    format!(
                        "Passing the unknown named argument \"{}\" to the macro \"{}\" is deprecated and will throw in Twig 4.0; declare a variadic argument (\"...name\") in the macro definition to accept it (in \"{}\" at line {lineno}).",
                        name.to_string_lossy(),
                        self.name_text(),
                        source.name()
                    )
                    .as_bytes(),
                );
            }
        }
    }
}
