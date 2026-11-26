//! Compiles the templates of each case of `tests/data/integration.json`
//! (Twig's integration fixtures, recorded by
//! `oracle/recorders/twig_integration.php`) with `twig::compiler`, as a
//! Twig cache would hold them: one module per case, with its templates and
//! the deprecations the compiler triggered. A case that does not compile
//! keeps its error ("Class: message").
//!
//! It also compiles the template sets of `tests/data/sandbox_templates.json`
//! (the templates of tests/Sandbox/SandboxTest.php, run by
//! `tests/sandbox_test.rs`) into `sandbox_sets.rs`: a set marked `sandbox`
//! with the sandbox extension, as the environment of a `Sandbox` has it.
//! The sets of `tests/data/legacy_sandbox_templates.json` (the templates
//! of tests/Extension/SandboxTest.php, run by
//! `tests/legacy_sandbox_test.rs`) go into `legacy_sandbox_sets.rs`.
//!
//! Upstream: none

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{fmt::Write as _, rc::Rc};

use serde_json::Value as Json;
use twig::{Autoescape, Environment, EnvironmentOptions};

#[path = "src/extension.rs"]
mod extension;
#[path = "src/legacy_sandbox_extension.rs"]
mod legacy_sandbox_extension;
#[path = "src/sandbox_extension.rs"]
mod sandbox_extension;

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

/// The stack of the build thread. The parser and the compiler recurse
/// once for each level of a node tree, and Windows gives the main thread
/// only 1 MB (8 MB on Linux), which some fixtures need more than.
const STACK_SIZE: usize = 64 * 1024 * 1024;

fn main() {
    std::thread::Builder::new()
        .name("twig-integration-build".to_owned())
        .stack_size(STACK_SIZE)
        .spawn(build)
        .unwrap()
        .join()
        .unwrap();
}

fn build() {
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
        let env = match Environment::with_options(extension::extensions(), options) {
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
    sandbox_sets();
    legacy_sandbox_sets();
}

/// The template sets of `tests/data/sandbox_templates.json`.
fn sandbox_sets() {
    template_sets("sandbox_templates", "sandbox_sets", "SANDBOX_SETS", false);
}

/// The template sets of `tests/data/legacy_sandbox_templates.json`: the
/// templates of tests/Extension/SandboxTest.php, run by
/// `tests/legacy_sandbox_test.rs`.
fn legacy_sandbox_sets() {
    println!("cargo:rerun-if-changed=src/legacy_sandbox_extension.rs");
    template_sets(
        "legacy_sandbox_templates",
        "legacy_sandbox_sets",
        "LEGACY_SANDBOX_SETS",
        true,
    );
}

/// Compiles the sets of `tests/data/<data>.json` into `<out>.rs`, as the
/// static `<name>`. A legacy set is compiled with the extensions of
/// tests/Extension/SandboxTest.php; it keeps its templates that do not
/// compile, and compiles its `strings` as `createTemplate()` would.
fn template_sets(data: &str, out: &str, name: &str, legacy: bool) {
    let data = format!("{}/tests/data/{data}.json", env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed={data}");
    println!("cargo:rerun-if-changed=src/sandbox_extension.rs");
    let sets: Vec<Json> = serde_json::from_str(&std::fs::read_to_string(data).unwrap()).unwrap();
    let mut code = format!(
        "/// A set's compiled templates, or why the set did not compile.\npub type Set = Result<&'static [::twig::loader::CompiledEntry], &'static [u8]>;\n\n/// The compiled template sets, by name.\npub static {name}: &[(&str, Set)] = &[\n",
    );
    let mut modules = String::new();
    for (i, set) in sets.iter().enumerate() {
        let name = set["name"].as_str().unwrap();
        let mut extensions: Vec<Rc<dyn twig::extension::Extension>> = if legacy {
            vec![
                Rc::new(legacy_sandbox_extension::LegacySandboxTestExtension {
                    variant: set["variant"].as_str().unwrap_or_default().to_owned(),
                }),
                Rc::new(twig::extension::string_loader::StringLoaderExtension),
            ]
        } else {
            vec![Rc::new(sandbox_extension::SandboxTestExtension)]
        };
        if set["sandbox"].as_bool().unwrap_or(true) {
            extensions.push(Rc::new(twig::extension::sandbox::SandboxExtension::new(
                Rc::new(twig::sandbox::SecurityPolicy::default()),
                false,
                None,
            )));
        }
        let mut options = EnvironmentOptions::default();
        if let Some(strategy) = set["autoescape"].as_str() {
            options.autoescape = Autoescape::Strategy(strategy.as_bytes().to_vec());
        } else {
            options.autoescape = Autoescape::Off;
        }
        let env = Environment::with_options(extensions, options).unwrap();
        let templates: Vec<(Vec<u8>, Vec<u8>)> = set["templates"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(n, t)| {
                (
                    n.as_bytes().to_vec(),
                    t.as_str().unwrap().as_bytes().to_vec(),
                )
            })
            .collect();
        let strings: Vec<(Option<Vec<u8>>, Vec<u8>)> = set["strings"]
            .as_array()
            .map(|strings| {
                strings
                    .iter()
                    .map(|pair| {
                        (
                            pair[0].as_str().map(|n| n.as_bytes().to_vec()),
                            pair[1].as_str().unwrap().as_bytes().to_vec(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        match twig::compiler::compile_set_with(&env, &templates, &strings, legacy) {
            Ok(module) => {
                let _ = writeln!(
                    modules,
                    "#[allow(warnings, clippy::all, clippy::pedantic, clippy::nursery, clippy::restriction)]\npub mod s{i} {{\n{module}\n}}"
                );
                let _ = writeln!(code, "    ({name:?}, Ok(s{i}::TEMPLATES)),");
            }
            Err(e) => {
                let _ = writeln!(code, "    ({name:?}, Err({})),", byte_lit(&error_text(&e)));
            }
        }
    }
    code.push_str("];\n");
    code.push_str(&modules);
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join(format!("{out}.rs"));
    std::fs::write(out, code).unwrap();
}

/// `sprintf('%s: %s', $e::class, $e->getMessage())`.
fn error_text(e: &twig::Error) -> Vec<u8> {
    let mut text = e.class().as_bytes().to_vec();
    text.extend_from_slice(b": ");
    text.extend_from_slice(e.message());
    text
}
