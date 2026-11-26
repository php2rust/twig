//! Compiles the templates of each case of `tests/data/integration.json`
//! (Twig's integration fixtures, recorded by
//! `oracle/recorders/twig_integration.php`) with `twig::compiler`, as a
//! Twig cache would hold them: one module per case, with its templates and
//! the deprecations the compiler triggered. A case that does not compile
//! keeps its error ("Class: message").
//!
//! Upstream: none

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{fmt::Write as _, rc::Rc};

use serde_json::Value as Json;
use twig::{Autoescape, Environment, EnvironmentOptions};

#[path = "src/extension.rs"]
mod extension;

fn decode(json: &Json) -> Vec<u8> {
    php_value::base64::decode(json.as_str().expect("base64").as_bytes())
}

fn byte_lit(bytes: &[u8]) -> String {
    let mut out = String::from("b\"");
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7E => out.push(char::from(b)),
            _ => {
                let _ = write!(out, "\\x{b:02x}");
            }
        }
    }
    out.push('"');
    out
}

/// The environment options of a case's `--CONFIG--`, or the name of an
/// option this port does not read.
pub fn options(config: &serde_json::Map<String, Json>) -> Result<EnvironmentOptions, String> {
    let mut options = EnvironmentOptions::default();
    for (key, value) in config {
        let tag = value[0].as_str().unwrap_or_default();
        match (key.as_str(), tag) {
            ("strict_variables", "b") => options.strict_variables = value[1].as_bool().unwrap(),
            ("debug", "b") => options.debug = value[1].as_bool().unwrap(),
            ("autoescape", "b") if value[1] == Json::Bool(false) => {
                options.autoescape = Autoescape::Off;
            }
            ("autoescape", "s") => {
                let strategy = decode(&value[1]);
                options.autoescape = if strategy == b"name" {
                    Autoescape::Name
                } else {
                    Autoescape::Strategy(strategy)
                };
            }
            ("charset", "s") => options.charset = decode(&value[1]),
            ("optimizations", "i") => options.optimizations = value[1].as_i64().unwrap(),
            _ => return Err(format!("the \"{key}\" option")),
        }
    }
    Ok(options)
}

fn main() {
    let data = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/integration.json");
    println!("cargo:rerun-if-changed={data}");
    println!("cargo:rerun-if-changed=src/extension.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let records: Vec<Json> = serde_json::from_str(&std::fs::read_to_string(data).unwrap()).unwrap();
    let mut code = String::from(
        "/// A case's templates and the deprecations their compilation triggered, or why it did not compile.\npub type Compiled = Result<(&'static [::twig::loader::CompiledEntry], &'static [&'static [u8]]), &'static [u8]>;\n",
    );
    let mut entries = Vec::new();
    for (i, record) in records.iter().enumerate() {
        if record.get("skipped").is_some() {
            entries.push("Err(b\"skipped\")".to_owned());
            continue;
        }
        let options = match options(record["config"].as_object().unwrap()) {
            Ok(o) => o,
            Err(e) => {
                entries.push(format!(
                    "Err({})",
                    byte_lit(format!("unsupported: {e}").as_bytes())
                ));
                continue;
            }
        };
        let env =
            match Environment::with_options(vec![Rc::new(extension::FixtureExtension)], options) {
                Ok(env) => env,
                Err(e) => {
                    entries.push(format!("Err({})", byte_lit(&error_text(&e))));
                    continue;
                }
            };
        extension::register_enums(&env);
        let templates: Vec<(Vec<u8>, Vec<u8>)> = record["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (decode(&t[0]), decode(&t[1])))
            .collect();
        let (result, deprecations) = symfony_deprecation_contracts::collect(|| {
            twig::compiler::compile_set(&env, &templates)
        });
        match result {
            Ok(module) => {
                let _ = writeln!(
                    code,
                    "#[allow(warnings, clippy::all, clippy::pedantic, clippy::nursery, clippy::restriction)]\npub mod c{i} {{\n{module}\n}}"
                );
                let deprecations: Vec<String> =
                    deprecations.iter().map(|d| byte_lit(&d.message)).collect();
                entries.push(format!(
                    "Ok((c{i}::TEMPLATES, &[{}]))",
                    deprecations.join(", ")
                ));
            }
            Err(e) => entries.push(format!("Err({})", byte_lit(&error_text(&e)))),
        }
    }
    let _ = writeln!(
        code,
        "pub static COMPILED: &[Compiled] = &[\n    {}\n];",
        entries.join(",\n    ")
    );
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("cases.rs");
    std::fs::write(out, code).unwrap();
}

/// `sprintf('%s: %s', $e::class, $e->getMessage())`.
fn error_text(e: &twig::Error) -> Vec<u8> {
    let mut text = e.class().as_bytes().to_vec();
    text.extend_from_slice(b": ");
    text.extend_from_slice(e.message());
    text
}
