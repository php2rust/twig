//! Port of tests/IntegrationTest.php (`Twig\Test\IntegrationTestCase`):
//! every case of upstream's `tests/Fixtures/**/*.test`, compiled to Rust
//! by the build script and run against what real Twig 3.30 did
//! (`data/integration.json`, `oracle/recorders/twig_integration.php`,
//! `scripts/record.sh twig-integration`): the trimmed output or the
//! exception ("Class: message"), and the deprecations.
//!
//! A case is skipped when its `--CONDITION--` is false in PHP, when the
//! Rust compiler does not support a part of it yet (`dump`,
//! `template_from_string`), or when its `--DATA--` holds an
//! object of a class the port does not have (`src/classes.rs` has the
//! test classes). The cases listed in
//! `data/integration_pending.txt` must still fail: the list stays exact.
//!
//! Upstream: twig/twig tests/IntegrationTest.php src/Test/IntegrationTestCase.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeSet, rc::Rc, sync::Arc};

use php_value::{
    Key, PhpArray, PhpString,
    base64::decode,
    value::{PhpObject, Value},
};
use serde_json::Value as Json;
use twig::{Autoescape, Environment, EnvironmentOptions, loader::CompiledArrayLoader};
use twig_integration::extension::FixtureExtension;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/cases.rs"));
}

fn bytes(json: &Json) -> Vec<u8> {
    decode(json.as_str().expect("base64").as_bytes())
}

/// The recorder's `typed()` tree as a value, or `None` for an object of
/// a class the port does not have.
fn value(json: &Json) -> Option<Value> {
    let tag = json[0].as_str().unwrap();
    Some(match tag {
        "n" => Value::Null,
        "b" => Value::Bool(json[1].as_bool().unwrap()),
        "i" => Value::Int(json[1].as_i64().unwrap()),
        "f" => Value::Float(json[1].as_str().unwrap().parse().unwrap()),
        "s" => Value::String(PhpString::from(bytes(&json[1]))),
        "a" => {
            let mut array = PhpArray::new();
            for pair in json[1].as_array().unwrap() {
                let key = match &pair[0] {
                    Json::Number(n) => Key::Int(n.as_i64().unwrap()),
                    other => Key::Str(PhpString::from(bytes(&other[1]))),
                };
                array.insert(key, value(&pair[1])?);
            }
            Value::Array(array)
        }
        "o" => object(json)?,
        _ => return None,
    })
}

/// An object of a class the port has, from its recorded state.
fn object(json: &Json) -> Option<Value> {
    use php_value::object::{ArrayIterator, ArrayObject, StdClass};
    use twig_integration::classes as c;
    let class = json[1].as_str()?;
    let Value::Array(state) = value(json.get(2)?)? else {
        return None;
    };
    let get = |name: &str| state.get(&Key::from_bytes(name.as_bytes())).cloned();
    let array = |name: &str| match get(name) {
        Some(Value::Array(a)) => a,
        _ => PhpArray::new(),
    };
    let string = |name: &str| match get(name) {
        Some(Value::String(s)) => s,
        _ => PhpString::default(),
    };
    let object: Arc<dyn PhpObject> = match class {
        "stdClass" => Arc::new(StdClass::from_array(state.clone())),
        "ArrayObject" => Arc::new(ArrayObject::new(state.clone())),
        "ArrayIterator" => Arc::new(ArrayIterator::new(state.clone())),
        "Twig\\Markup" => {
            return Some(twig::markup::Markup::value(
                string("content"),
                string("charset"),
            ));
        }
        "Twig\\Tests\\TwigTestFoo" => Arc::new(c::TwigTestFoo {
            position: match get("position") {
                Some(Value::Int(i)) => i,
                _ => 0,
            },
            array: array("array"),
        }),
        "Twig\\Tests\\ToStringStub" => Arc::new(c::ToStringStub {
            string: string("string"),
        }),
        "Twig\\Tests\\CountableStub" => Arc::new(c::CountableStub {
            count: match get("count") {
                Some(Value::Int(i)) => i,
                _ => 0,
            },
        }),
        "Twig\\Tests\\IteratorAggregateStub" => Arc::new(c::IteratorAggregateStub {
            data: array("data"),
        }),
        "Twig\\Tests\\SimpleIteratorForTesting" => Arc::new(c::SimpleIteratorForTesting),
        "Twig\\Tests\\MagicCallStub" => Arc::new(c::MagicCallStub),
        "DateTime" | "DateTimeImmutable" => {
            let zone = php_date::DateTimeZone::new(&text(&string("timezone"))).ok()?;
            let date =
                php_date::DateTimeImmutable::new(&text(&string("date")), Some(&zone)).ok()?;
            if class == "DateTime" {
                return Some(php_date::object::DateTime::object(date));
            }
            Arc::new(date)
        }
        "DateTimeZone" => Arc::new(php_date::DateTimeZone::new(&text(&string("timezone"))).ok()?),
        "DateInterval" if get("days") == Some(Value::Bool(false)) => {
            let int = |name: &str| match get(name) {
                Some(Value::Int(i)) => i,
                _ => 0,
            };
            let f = match get("f") {
                Some(Value::Float(f)) => f,
                _ => 0.0,
            };
            #[allow(clippy::cast_possible_truncation)]
            Arc::new(php_value::date_interval::DateInterval {
                y: int("y"),
                m: int("m"),
                d: int("d"),
                h: int("h"),
                i: int("i"),
                s: int("s"),
                microsecond: (f * 1_000_000.0).round() as i64,
                invert: int("invert") == 1,
            })
        }
        _ => return None,
    };
    Some(Value::Object(object))
}

fn text(s: &PhpString) -> String {
    String::from_utf8_lossy(s.as_bytes()).into_owned()
}

/// What the case's `--DATA--` set: PHP's default zone and the core
/// extension's date formats, zone and number format.
fn apply_env(record: &Json, env: &Environment) {
    let Some(Value::Array(state)) = record.get("env").and_then(value) else {
        return;
    };
    let get = |name: &str| state.get(&Key::from_bytes(name.as_bytes())).cloned();
    let string = |v: Option<Value>| match v {
        Some(Value::String(s)) => s.as_bytes().to_vec(),
        _ => Vec::new(),
    };
    if let Some(Value::String(zone)) = get("default_timezone") {
        php_date::date_default_timezone_set(&text(&zone));
    }
    let mut core = env.core_options_mut();
    if let Some(Value::Array(formats)) = get("date_format") {
        let mut formats = formats.values().map(|v| string(Some(v.clone())));
        core.date_formats = [
            formats.next().unwrap_or_default(),
            formats.next().unwrap_or_default(),
        ];
    }
    if let Some(Value::String(zone)) = get("timezone") {
        core.timezone = Some(text(&zone));
    }
    if let Some(Value::Array(number)) = get("number_format") {
        let values: Vec<Value> = number.values().cloned().collect();
        core.number_format = (
            match values.first() {
                Some(Value::Int(i)) => *i,
                _ => 0,
            },
            string(values.get(1).cloned()),
            string(values.get(2).cloned()),
        );
    }
}

fn options(config: &serde_json::Map<String, Json>) -> EnvironmentOptions {
    let mut options = EnvironmentOptions::default();
    for (key, value) in config {
        match key.as_str() {
            "strict_variables" => options.strict_variables = value[1].as_bool().unwrap(),
            "debug" => options.debug = value[1].as_bool().unwrap(),
            "autoescape" if value[0] == "b" => options.autoescape = Autoescape::Off,
            "autoescape" => {
                let strategy = bytes(&value[1]);
                options.autoescape = if strategy == b"name" {
                    Autoescape::Name
                } else {
                    Autoescape::Strategy(strategy)
                };
            }
            "charset" => options.charset = bytes(&value[1]),
            "optimizations" => options.optimizations = value[1].as_i64().unwrap(),
            _ => {}
        }
    }
    options
}

fn error_text(e: &twig::Error) -> Vec<u8> {
    let mut text = e.class().as_bytes().to_vec();
    text.extend_from_slice(b": ");
    text.extend_from_slice(e.message());
    text
}

fn trim(s: &[u8]) -> &[u8] {
    let is_trim = |b: &u8| *b == b'\n' || *b == b' ';
    let start = s.iter().position(|b| !is_trim(b)).unwrap_or(s.len());
    let end = s.iter().rposition(|b| !is_trim(b)).map_or(start, |e| e + 1);
    &s[start..end]
}

fn list(json: &Json) -> Vec<Vec<u8>> {
    json.as_array()
        .map(|a| a.iter().map(bytes).collect())
        .unwrap_or_default()
}

enum Outcome {
    Pass,
    Skip,
    Fail(String),
}

fn run(record: &Json, compiled: &generated::Compiled) -> Outcome {
    let lossy = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let (templates, compile_deprecations) = match compiled {
        Ok(c) => *c,
        Err(e) => {
            let e: &[u8] = e;
            if e == b"skipped"
                || e.starts_with(b"unsupported:")
                || e.windows(35)
                    .any(|w| w == b"is not supported by the Rust compil")
            {
                return Outcome::Skip;
            }
            return match record.get("load_error") {
                Some(expected) if bytes(expected) == e => Outcome::Pass,
                Some(expected) => Outcome::Fail(format!(
                    "compile error\n  got:      {}\n  expected: {}",
                    lossy(e),
                    lossy(&bytes(expected))
                )),
                None => Outcome::Fail(format!("compile error {}", lossy(e))),
            };
        }
    };
    let env = Environment::with_options(
        vec![Rc::new(FixtureExtension)],
        options(record["config"].as_object().unwrap()),
    )
    .unwrap();
    twig_integration::extension::register_enums(&env);
    env.add_global("global", Value::String(PhpString::from(b"global".to_vec())));
    // TwigTestFoo::BAR_NAME
    env.core_options_mut().constants.push((
        b"Twig\\Tests\\TwigTestFoo::BAR_NAME".to_vec(),
        Value::String(PhpString::from(b"bar".to_vec())),
    ));
    env.set_loader(Rc::new(CompiledArrayLoader { templates }));
    let names: Vec<Vec<u8>> = record["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| bytes(&t[0]))
        .collect();
    let (loaded, load_deprecations) = symfony_deprecation_contracts::collect(|| {
        for name in &names {
            env.load(name)?;
        }
        Ok::<(), twig::Error>(())
    });
    let mut deprecations: Vec<Vec<u8>> = compile_deprecations.iter().map(|d| d.to_vec()).collect();
    deprecations.extend(load_deprecations.into_iter().map(|d| d.message));
    let expected_load = list(&record["load_deprecations"]);
    if deprecations != expected_load {
        return Outcome::Fail(format!(
            "load deprecations\n  got:      {:?}\n  expected: {:?}",
            deprecations.iter().map(|d| lossy(d)).collect::<Vec<_>>(),
            expected_load.iter().map(|d| lossy(d)).collect::<Vec<_>>()
        ));
    }
    if let Err(e) = loaded {
        let got = error_text(&e);
        return match record.get("load_error") {
            Some(expected) if bytes(expected) == got => Outcome::Pass,
            other => Outcome::Fail(format!(
                "load error\n  got:      {}\n  expected: {}",
                lossy(&got),
                other.map_or_else(|| "no error".to_owned(), |e| lossy(&bytes(e)))
            )),
        };
    }
    if let Some(expected) = record.get("load_error") {
        return Outcome::Fail(format!(
            "no load error, expected {}",
            lossy(&bytes(expected))
        ));
    }
    render(record, &env)
}

/// Renders `index.twig` with the case's data.
fn render(record: &Json, env: &Environment) -> Outcome {
    let lossy = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let Some(data) = value(&record["data"]) else {
        return Outcome::Skip;
    };
    apply_env(record, env);
    let context = match data {
        Value::Array(a) => a,
        _ => PhpArray::new(),
    };
    let (rendered, render_deprecations) =
        symfony_deprecation_contracts::collect(|| env.render(b"index.twig", context));
    let got_deprecations: Vec<Vec<u8>> =
        render_deprecations.into_iter().map(|d| d.message).collect();
    let expected_deprecations = list(&record["render_deprecations"]);
    let got = match rendered {
        Ok(out) => ("output", trim(&out).to_vec()),
        Err(e) => ("render_error", error_text(&e)),
    };
    let expected = if let Some(out) = record.get("output") {
        ("output", bytes(out))
    } else {
        ("render_error", bytes(&record["render_error"]))
    };
    if got != expected {
        return Outcome::Fail(format!(
            "{}\n  got:      {:?}\n  expected: {} {:?}",
            got.0,
            lossy(&got.1),
            expected.0,
            lossy(&expected.1)
        ));
    }
    if got_deprecations != expected_deprecations {
        return Outcome::Fail(format!(
            "render deprecations\n  got:      {:?}\n  expected: {:?}",
            got_deprecations
                .iter()
                .map(|d| lossy(d))
                .collect::<Vec<_>>(),
            expected_deprecations
                .iter()
                .map(|d| lossy(d))
                .collect::<Vec<_>>()
        ));
    }
    Outcome::Pass
}

/// `IntegrationTest::testIntegration()`.
#[test]
fn testIntegration() {
    let records: Vec<Json> = serde_json::from_str(include_str!("data/integration.json")).unwrap();
    assert_eq!(records.len(), generated::COMPILED.len());
    let pending: BTreeSet<String> = include_str!("data/integration_pending.txt")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split(" # ").next().unwrap_or(l).trim().to_owned())
        .collect();
    let (mut passed, mut skipped) = (0, 0);
    let mut failures = Vec::new();
    let mut fixed = Vec::new();
    for (record, compiled) in records.iter().zip(generated::COMPILED) {
        let id = format!(
            "{} #{}",
            record["file"].as_str().unwrap(),
            record["case"].as_i64().unwrap_or(0)
        );
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(record, compiled)))
                .unwrap_or_else(|_| Outcome::Fail("panicked".to_owned()));
        match outcome {
            Outcome::Pass if pending.contains(&id) => fixed.push(id),
            Outcome::Pass => passed += 1,
            Outcome::Skip => skipped += 1,
            Outcome::Fail(_) if pending.contains(&id) => {}
            Outcome::Fail(why) => failures.push(format!("{id}: {why}")),
        }
    }
    println!(
        "{passed} passed, {skipped} skipped, {} pending, {} failed",
        pending.len(),
        failures.len()
    );
    assert!(
        failures.is_empty() && fixed.is_empty(),
        "{} failed:\n{}\n\n{} pending cases pass now (remove them from data/integration_pending.txt):\n{}",
        failures.len(),
        failures.join("\n"),
        fixed.len(),
        fixed.join("\n")
    );
}
