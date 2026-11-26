//! The parameters of the core filters, functions and tests as the sandbox
//! sees them (`CallableParameters`): name, variadic and
//! `isStringCoercionSafe()`, recorded from real Twig in
//! `data/callable_parameters.json` (`scripts/record.sh
//! twig-callable-parameters`).
//!
//! Upstream: none

use serde_json::Value as Json;
use twig::{Environment, util::callable_parameters};

#[test]
fn core_parameters_match_php() {
    let data: Vec<Json> = serde_json::from_str(include_str!("data/callable_parameters.json"))
        .expect("callable_parameters.json");
    let env = Environment::new(Vec::new());
    let mut checked = 0;
    let mut failures = Vec::new();
    for row in &data {
        let name = row["name"].as_str().unwrap();
        let callable = match row["type"].as_str().unwrap() {
            "filter" => env.filter(name.as_bytes()),
            "function" => env.function(name.as_bytes()),
            _ => env.test(name.as_bytes()),
        };
        // The debug and string loader extensions are not in a plain
        // environment.
        let Some(callable) = callable else {
            assert!(
                ["dump", "template_from_string"].contains(&name),
                "{name} is missing"
            );
            continue;
        };
        checked += 1;
        let got: Option<Vec<(String, bool, bool)>> = callable_parameters::of_callable(&callable)
            .map(|params| {
                params
                    .into_iter()
                    .map(|p| (p.name, p.variadic, p.coercion_safe))
                    .collect()
            });
        let want: Option<Vec<(String, bool, bool)>> = row["parameters"].as_array().map(|ps| {
            ps.iter()
                .map(|p| {
                    (
                        p[0].as_str().unwrap().to_owned(),
                        p[1].as_bool().unwrap(),
                        p[2].as_bool().unwrap(),
                    )
                })
                .collect()
        });
        // convert_encoding has no Rust implementation yet; the sandbox
        // then checks all its arguments, as PHP does for its untyped ones.
        if name == "convert_encoding" && got.is_none() {
            continue;
        }
        if got != want {
            failures.push(format!(
                "{} {name}:\n  got  {got:?}\n  want {want:?}",
                row["type"]
            ));
        }
    }
    assert!(checked > 60, "{checked} checked");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
