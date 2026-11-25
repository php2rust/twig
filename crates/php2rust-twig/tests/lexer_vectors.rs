//! The lexer against real Twig 3.30: every template of upstream's
//! `tests/Fixtures/**/*.test` and the cases in `data/lexer_extra.json`,
//! tokenized by `Twig\Lexer` with the core operators
//! (`oracle/recorders/twig_lexer.php`, `scripts/record.sh twig-lexer`).
//! Each token's type, value, line, offset and documentation must match,
//! or the `SyntaxError`'s message, line and column, and the deprecations.
//!
//! Upstream: none
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_possible_wrap)]

use php_value::base64::decode;
use serde_json::Value as Json;
use twig::{Lexer, LexerOptions, Source, TokenValue, token::TokenType};

fn bytes(json: &Json) -> Vec<u8> {
    decode(json.as_str().expect("base64").as_bytes())
}

fn type_code(ty: TokenType) -> i64 {
    ty.code()
}

fn value(json: &Json) -> TokenValue {
    match json {
        Json::String(_) => TokenValue::Bytes(bytes(json)),
        Json::Array(a) if a[0] == "int" => {
            TokenValue::Number(php_value::numeric::Number::Int(a[1].as_i64().unwrap()))
        }
        Json::Array(a) => {
            let hex = a[1].as_str().unwrap();
            let bits = u64::from_str_radix(hex, 16).unwrap();
            // pack('E') is big-endian
            TokenValue::Number(php_value::numeric::Number::Float(f64::from_bits(bits)))
        }
        _ => panic!("bad value {json}"),
    }
}

#[test]
fn tokens_match_php() {
    let data: Vec<Json> =
        serde_json::from_str(include_str!("data/lexer.json")).expect("lexer.json");
    assert!(data.len() > 700, "{} cases", data.len());
    let lexer = Lexer::new(LexerOptions::default());
    let mut failures = Vec::new();
    for case in &data {
        let name = case["name"].as_str().unwrap();
        let template = bytes(&case["template"]);
        let (result, deprecations) = symfony_deprecation_contracts::collect(|| {
            lexer.tokenize(Source::new(template.clone(), "index"))
        });
        let deprecations: Vec<Vec<u8>> = deprecations.into_iter().map(|d| d.message).collect();
        let expected_deprecations: Vec<Vec<u8>> = case["deprecations"]
            .as_array()
            .unwrap()
            .iter()
            .map(bytes)
            .collect();
        if deprecations != expected_deprecations {
            failures.push(format!("{name}: deprecations {deprecations:?}"));
        }
        match (&case.get("tokens"), result) {
            (Some(tokens), Ok(mut stream)) => {
                let tokens = tokens.as_array().unwrap();
                for (i, t) in tokens.iter().enumerate() {
                    let token = stream.current().clone();
                    let got = (
                        type_code(token.token_type()),
                        token.value().clone(),
                        token.line() as i64,
                        token.offset().map(|o| o as i64),
                        token.documentation().map(<[u8]>::to_vec),
                    );
                    let want = (
                        t[0].as_i64().unwrap(),
                        value(&t[1]),
                        t[2].as_i64().unwrap(),
                        t[3].as_i64(),
                        t[4].as_str().map(|_| bytes(&t[4])),
                    );
                    if got != want {
                        failures.push(format!("{name}: token {i}: got {got:?}, want {want:?}"));
                        break;
                    }
                    if i + 1 < tokens.len() && stream.next().is_err() {
                        failures.push(format!("{name}: stream ends at token {i}"));
                        break;
                    }
                }
                if !stream.is_eof() {
                    failures.push(format!("{name}: more tokens than PHP"));
                }
            }
            (None, Err(e)) => {
                let want = &case["error"];
                let got = (
                    e.message().to_vec(),
                    e.template_line(),
                    e.template_column().map(|c| c as i64),
                );
                let want = (bytes(&want[0]), want[1].as_i64().unwrap(), want[2].as_i64());
                if got != want {
                    failures.push(format!(
                        "{name}: error {:?}, want {:?}",
                        String::from_utf8_lossy(&got.0),
                        String::from_utf8_lossy(&want.0)
                    ));
                }
            }
            (Some(_), Err(e)) => failures.push(format!("{name}: unexpected error {e}")),
            (None, Ok(_)) => failures.push(format!("{name}: no error")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        data.len(),
        failures.join("\n")
    );
}
