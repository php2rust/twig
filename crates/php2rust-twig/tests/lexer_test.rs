//! LexerTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test. `expectExceptionMessage` checks that the message
//! contains the text. `set_error_handler()` in
//! testErrorsAtTheEndOfTheStream has no Rust equivalent: the lexer cannot
//! raise PHP warnings, so the test checks that it returns.
//!
//! Upstream: twig/twig tests/LexerTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use twig::{Error, ErrorKind, Lexer, LexerOptions, Source, Token, TokenStream, TokenType};

fn lexer() -> Lexer {
    Lexer::new(LexerOptions::default())
}

fn tokenize(template: impl Into<Vec<u8>>) -> TokenStream {
    lexer()
        .tokenize(Source::new(template, "index"))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn tokenize_error(template: impl Into<Vec<u8>>) -> Error {
    match lexer().tokenize(Source::new(template, "index")) {
        Ok(stream) => panic!("expected a SyntaxError, got {stream}"),
        Err(e) => {
            assert_eq!(e.kind(), ErrorKind::Syntax);
            e
        }
    }
}

/// `$env->parse($env->tokenize($source))`, which must fail.
fn parse_error(template: &str) -> Error {
    match twig::Environment::new(Vec::new()).parse_source(Source::new(template, "index")) {
        Ok(module) => panic!("A SyntaxError should have been thrown, got {module}"),
        Err(e) => {
            assert_eq!(e.kind(), ErrorKind::Syntax);
            e
        }
    }
}

fn assert_message_contains(e: &Error, text: &str) {
    let message = String::from_utf8_lossy(e.message());
    assert!(
        message.contains(text),
        "{message:?} does not contain {text:?}"
    );
}

fn expect(stream: &mut TokenStream, ty: TokenType) -> Token {
    stream.expect(ty).unwrap_or_else(|e| panic!("{e}"))
}

fn expect_value(stream: &mut TokenStream, ty: TokenType, value: &str) -> Token {
    stream
        .expect_value(ty, value.as_bytes())
        .unwrap_or_else(|e| panic!("{e}"))
}

fn count_token(template: &str, ty: TokenType, value: Option<&str>) -> usize {
    let mut stream = tokenize(template);
    let mut count = 0;
    while !stream.is_eof() {
        let token = stream.next().unwrap_or_else(|e| panic!("{e}"));
        if token.test(ty) && value.is_none_or(|v| token.value_bytes() == v.as_bytes()) {
            count += 1;
        }
    }
    count
}

#[test]
fn testNameLabelForTag() {
    let mut stream = tokenize("{% § %}");
    expect(&mut stream, TokenType::BlockStart);
    assert_eq!(
        expect(&mut stream, TokenType::Name).value_bytes(),
        "§".as_bytes()
    );
}

#[test]
fn testNameLabelForFunction() {
    let mut stream = tokenize("{{ §() }}");
    expect(&mut stream, TokenType::VarStart);
    assert_eq!(
        expect(&mut stream, TokenType::Name).value_bytes(),
        "§".as_bytes()
    );
}

#[test]
fn testBracketsNesting() {
    let template = r#"{{ {"a":{"b":"c"}} }}"#;
    assert_eq!(count_token(template, TokenType::Punctuation, Some("{")), 2);
    assert_eq!(count_token(template, TokenType::Punctuation, Some("}")), 2);
}

#[test]
fn testLineDirective() {
    let template = "foo\nbar\n{% line 10 %}\n{{\nbaz\n}}\n";
    let mut stream = tokenize(template);
    // foo\nbar\n
    assert_eq!(expect(&mut stream, TokenType::Text).line(), 1);
    // \n (after {% line %})
    assert_eq!(expect(&mut stream, TokenType::Text).line(), 10);
    // {{
    assert_eq!(expect(&mut stream, TokenType::VarStart).line(), 11);
    // baz
    assert_eq!(expect(&mut stream, TokenType::Name).line(), 12);
}

#[test]
fn testLineDirectiveInline() {
    let template = "foo\nbar{% line 10 %}{{\nbaz\n}}\n";
    let mut stream = tokenize(template);
    // foo\nbar
    assert_eq!(expect(&mut stream, TokenType::Text).line(), 1);
    // {{
    assert_eq!(expect(&mut stream, TokenType::VarStart).line(), 10);
    // baz
    assert_eq!(expect(&mut stream, TokenType::Name).line(), 11);
}

#[test]
fn testLongComments() {
    tokenize(format!("{{# {} #}}", "*".repeat(100_000)));
}

#[test]
fn testLongVerbatim() {
    tokenize(format!(
        "{{% verbatim %}}{}{{% endverbatim %}}",
        "*".repeat(100_000)
    ));
}

#[test]
fn testLongVar() {
    tokenize(format!("{{{{ {} }}}}", "x".repeat(100_000)));
}

#[test]
fn testLongBlock() {
    tokenize(format!("{{% {} %}}", "x".repeat(100_000)));
}

#[test]
fn testBigNumbers() {
    let mut stream = tokenize("{{ 922337203685477580700 }}");
    stream.next().unwrap();
    let node = stream.next().unwrap();
    // assertEquals: '922337203685477580700' == 9.2233720368547758E+20
    assert!(node.test_value(TokenType::Number, b"922337203685477580700"));
}

#[test]
fn testStringWithEscapedDelimiter() {
    let cases: &[(&str, &[u8])] = &[
        (r"{{ '\x6' }}", b"\x06"),
        (r"{{ '\065\x64' }}", b"\x35\x64"),
        // octal escape overflowing a single byte is constrained with % 256
        (r"{{ '\777' }}", b"\xff"),
        // octal escape exactly equal to 256 wraps around to a NUL byte
        (r"{{ '\400' }}", b"\x00"),
        (r"{{ 'App\\Test' }}", br"App\Test"),
        (r#"{{ "App\#{var}" }}"#, b"App#{var}"),
        (r"{{ 'foo \' bar' }}", b"foo ' bar"),
        (r#"{{ "foo \" bar" }}"#, b"foo \" bar"),
        (r"{{ '\f\n\r\t\v' }}", b"\x0c\n\r\t\x0b"),
        (r"{{ '\\f\\n\\r\\t\\v' }}", br"\f\n\r\t\v"),
        (r"{{ 'Ymd\\THis' }}", br"Ymd\THis"),
    ];
    for (template, expected) in cases {
        let mut stream = tokenize(*template);
        expect(&mut stream, TokenType::VarStart);
        let token = expect(&mut stream, TokenType::String);
        assert_eq!(token.value_bytes(), *expected, "{template}");
    }
}

#[test]
fn testStringWithEscapedDelimiterProducingDeprecation() {
    let cases = [
        (
            r"{{ 'App\Test' }}",
            "AppTest",
            r#"Since twig/twig 3.12: Character "T" should not be escaped; the "\" character is ignored in Twig 3 but will not be in Twig 4. Please remove the extra "\" character at position 5 in "index" at line 1."#,
        ),
        (
            r#"{{ "foo \' bar" }}"#,
            "foo ' bar",
            r#"Since twig/twig 3.12: Character "'" should not be escaped; the "\" character is ignored in Twig 3 but will not be in Twig 4. Please remove the extra "\" character at position 6 in "index" at line 1."#,
        ),
        (
            r#"{{ 'foo \" bar' }}"#,
            "foo \" bar",
            r#"Since twig/twig 3.12: Character """ should not be escaped; the "\" character is ignored in Twig 3 but will not be in Twig 4. Please remove the extra "\" character at position 6 in "index" at line 1."#,
        ),
    ];
    for (template, expected, deprecation) in cases {
        let ((), deprecations) = symfony_deprecation_contracts::collect(|| {
            let mut stream = tokenize(template);
            expect(&mut stream, TokenType::VarStart);
            expect_value(&mut stream, TokenType::String, expected);
        });
        let messages: Vec<_> = deprecations
            .iter()
            .map(|d| String::from_utf8_lossy(&d.message).into_owned())
            .collect();
        assert_eq!(messages, [deprecation], "{template}");
    }
}

#[test]
fn testStringWithInterpolation() {
    let mut stream = tokenize(r#"foo {{ "bar #{ baz + 1 }" }}"#);
    expect_value(&mut stream, TokenType::Text, "foo ");
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::String, "bar ");
    expect(&mut stream, TokenType::InterpolationStart);
    expect_value(&mut stream, TokenType::Name, "baz");
    expect_value(&mut stream, TokenType::Operator, "+");
    expect_value(&mut stream, TokenType::Number, "1");
    expect(&mut stream, TokenType::InterpolationEnd);
    expect(&mut stream, TokenType::VarEnd);
}

#[test]
fn testStringWithEscapedInterpolation() {
    let mut stream = tokenize(r#"{{ "bar \#{baz+1}" }}"#);
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::String, "bar #{baz+1}");
    expect(&mut stream, TokenType::VarEnd);
}

#[test]
fn testStringWithHash() {
    let mut stream = tokenize(r#"{{ "bar # baz" }}"#);
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::String, "bar # baz");
    expect(&mut stream, TokenType::VarEnd);
}

#[test]
fn testStringWithUnterminatedInterpolation() {
    let e = tokenize_error(r#"{{ "bar #{x" }}"#);
    assert_message_contains(&e, r#"Unclosed """#);
}

#[test]
fn testStringWithNestedInterpolations() {
    let mut stream = tokenize(r#"{{ "bar #{ "foo#{bar}" }" }}"#);
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::String, "bar ");
    expect(&mut stream, TokenType::InterpolationStart);
    expect_value(&mut stream, TokenType::String, "foo");
    expect(&mut stream, TokenType::InterpolationStart);
    expect_value(&mut stream, TokenType::Name, "bar");
    expect(&mut stream, TokenType::InterpolationEnd);
    expect(&mut stream, TokenType::InterpolationEnd);
    expect(&mut stream, TokenType::VarEnd);
}

#[test]
fn testStringWithNestedInterpolationsInBlock() {
    let mut stream = tokenize(r#"{% foo "bar #{ "foo#{bar}" }" %}"#);
    expect(&mut stream, TokenType::BlockStart);
    expect_value(&mut stream, TokenType::Name, "foo");
    expect_value(&mut stream, TokenType::String, "bar ");
    expect(&mut stream, TokenType::InterpolationStart);
    expect_value(&mut stream, TokenType::String, "foo");
    expect(&mut stream, TokenType::InterpolationStart);
    expect_value(&mut stream, TokenType::Name, "bar");
    expect(&mut stream, TokenType::InterpolationEnd);
    expect(&mut stream, TokenType::InterpolationEnd);
    expect(&mut stream, TokenType::BlockEnd);
}

#[test]
fn testOperatorEndingWithALetterAtTheEndOfALine() {
    let mut stream = tokenize("{{ 1 and\n0}}");
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::Number, "1");
    expect_value(&mut stream, TokenType::Operator, "and");
}

#[test]
fn testFilterAndAttributeNamedAfterOperator() {
    // Ensure that filters/attributes aren't mistaken for operators when their names conflict
    // (see https://github.com/twigphp/Twig/issues/4767)
    let template = concat!(
        "{{ 'foo'|and }}",
        "{{ 'bar' | and }}",
        "{{ foo.and }}",
        "{{ bar . and }}",
        "{{ foo and bar }}",
    );
    let mut stream = tokenize(template);
    for value in ["foo", "bar"] {
        expect(&mut stream, TokenType::VarStart);
        expect_value(&mut stream, TokenType::String, value);
        expect_value(&mut stream, TokenType::Operator, "|");
        expect_value(&mut stream, TokenType::Name, "and");
        expect(&mut stream, TokenType::VarEnd);
    }
    for value in ["foo", "bar"] {
        expect(&mut stream, TokenType::VarStart);
        expect_value(&mut stream, TokenType::Name, value);
        expect_value(&mut stream, TokenType::Operator, ".");
        expect_value(&mut stream, TokenType::Name, "and");
        expect(&mut stream, TokenType::VarEnd);
    }
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::Name, "foo");
    expect_value(&mut stream, TokenType::Operator, "and");
    expect_value(&mut stream, TokenType::Name, "bar");
    expect(&mut stream, TokenType::VarEnd);
}

#[test]
fn testLiteralIsNotAnOperator() {
    // "literal" is the name of the LiteralExpressionParser but should not be treated as an operator token
    let mut stream = tokenize("{{ literal }}");
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::Name, "literal");
    expect(&mut stream, TokenType::VarEnd);
}

#[test]
fn testUnterminatedVariable() {
    let e = tokenize_error("\n\n{{\n\nbar\n\n\n");
    assert_message_contains(&e, r#"Unclosed "variable" in "index" at line 3"#);
}

#[test]
fn testUnterminatedBlock() {
    let e = tokenize_error("\n\n{%\n\nbar\n\n\n");
    assert_message_contains(&e, r#"Unclosed "block" in "index" at line 3"#);
}

#[test]
fn testOverridingSyntax() {
    let template = "[# comment #]{# variable #}/# if true #/true/# endif #/";
    let lexer = Lexer::new(LexerOptions {
        tag_comment: (b"[#".to_vec(), b"#]".to_vec()),
        tag_block: (b"/#".to_vec(), b"#/".to_vec()),
        tag_variable: (b"{#".to_vec(), b"#}".to_vec()),
        ..LexerOptions::default()
    });
    let mut stream = lexer.tokenize(Source::new(template, "index")).unwrap();
    expect(&mut stream, TokenType::VarStart);
    expect_value(&mut stream, TokenType::Name, "variable");
    expect(&mut stream, TokenType::VarEnd);
    expect(&mut stream, TokenType::BlockStart);
    expect_value(&mut stream, TokenType::Name, "if");
    expect_value(&mut stream, TokenType::Name, "true");
    expect(&mut stream, TokenType::BlockEnd);
    expect_value(&mut stream, TokenType::Text, "true");
    expect(&mut stream, TokenType::BlockStart);
    expect_value(&mut stream, TokenType::Name, "endif");
    expect(&mut stream, TokenType::BlockEnd);
}

#[test]
fn testErrorsAtTheEndOfTheStream() {
    for template in ["{{ =", "{{ .."] {
        tokenize(template);
    }
}

#[test]
fn testStrings() {
    let cases = [
        "日本では、春になると桜の花が咲きます。多くの人々は、公園や川の近くに集まり、お花見を楽しみます。桜の花びらが風に舞い、まるで雪のように見える瞬間は、とても美しいです。",
        "في العالم العربي، يُعتبر الخط العربي أحد أجمل أشكال الفن. يُستخدم الخط في تزيين المساجد والكتب والمخطوطات القديمة. يتميز الخط العربي بجماله وتناسقه، ويُعتبر رمزًا للثقافة الإسلامية.",
    ];
    for expected in cases {
        let mut stream = tokenize(format!("{{{{ \"{expected}\" }}}}"));
        expect(&mut stream, TokenType::VarStart);
        expect_value(&mut stream, TokenType::String, expected);

        let mut stream = tokenize(format!("{{{{ '{expected}' }}}}"));
        expect(&mut stream, TokenType::VarStart);
        expect_value(&mut stream, TokenType::String, expected);
    }
}

#[test]
fn testInlineCommentWithHashInString() {
    let mut stream = tokenize(r#"{{ "me # this is NOT an inline comment" }}"#);
    expect(&mut stream, TokenType::VarStart);
    expect_value(
        &mut stream,
        TokenType::String,
        "me # this is NOT an inline comment",
    );
    expect(&mut stream, TokenType::VarEnd);
    assert!(stream.is_eof());
}

#[test]
fn testInlineCommentForVariable() {
    let cases = [
        "{{\n            \"me\"\n            # this is an inline comment\n        }}",
        "{{\n            # this is an inline comment\n            \"me\"\n        }}",
        "{{\n            \"me\" # this is an inline comment\n        }}",
        "{{\n            # this is an inline comment\n            \"me\" # this is an inline comment\n            # this is an inline comment\n        }}",
    ];
    for template in cases {
        let mut stream = tokenize(template);
        expect(&mut stream, TokenType::VarStart);
        expect_value(&mut stream, TokenType::String, "me");
        expect(&mut stream, TokenType::VarEnd);
        assert!(stream.is_eof());
    }
}

#[test]
fn testInlineCommentForBlock() {
    let cases = [
        "{%\n            if true\n            # this is an inline comment\n        %}me{% endif %}",
        "{%\n            # this is an inline comment\n            if true\n        %}me{% endif %}",
        "{%\n            if true # this is an inline comment\n        %}me{% endif %}",
        "{%\n            # this is an inline comment\n            if true # this is an inline comment\n            # this is an inline comment\n        %}me{% endif %}",
    ];
    for template in cases {
        let mut stream = tokenize(template);
        expect(&mut stream, TokenType::BlockStart);
        expect_value(&mut stream, TokenType::Name, "if");
        expect_value(&mut stream, TokenType::Name, "true");
        expect(&mut stream, TokenType::BlockEnd);
        expect_value(&mut stream, TokenType::Text, "me");
        expect(&mut stream, TokenType::BlockStart);
        expect_value(&mut stream, TokenType::Name, "endif");
        expect(&mut stream, TokenType::BlockEnd);
        assert!(stream.is_eof());
    }
}

#[test]
fn testInlineCommentForComment() {
    let stream =
        tokenize("{#\n            Some regular comment # this is an inline comment\n        #}");
    assert!(stream.is_eof());
}

#[test]
fn testUnclosedBracketInExpression() {
    let cases = [
        ("{{ (1 + 3 }}", "("),
        (r#"{{ obj["a" }}"#, "["),
        ("{{ ({ a: 1) }}", "{"),
        ("{{ (([1]) + 3 }}", "("),
    ];
    for (template, bracket) in cases {
        let e = tokenize_error(template);
        assert_message_contains(
            &e,
            &format!(r#"Unclosed "{bracket}" in "index" at line 1."#),
        );
    }
}

#[test]
fn testUnexpectedBracketInExpression() {
    let cases = [
        ("{{ 1 + 3) }}", ")", 9),
        ("{{ obj] }}", "]", 7),
        ("{{ { a: 1 }}", "}", 12),
        ("{{ ([1] + 3)) }}", ")", 13),
    ];
    for (template, bracket, column) in cases {
        let e = tokenize_error(template);
        assert_message_contains(
            &e,
            &format!(r#"Unexpected "{bracket}" in "index" at line 1 column {column}."#),
        );
    }
}

#[test]
fn testTokensCarryTheirSourceOffset() {
    let mut stream = tokenize("Hello {{ name }}!");
    // [type, offset] pairs; offsets point at the start of each lexeme in the source
    let expected = [
        (TokenType::Text, 0),     // "Hello "
        (TokenType::VarStart, 6), // "{{"
        (TokenType::Name, 9),     // "name"
        (TokenType::VarEnd, 14),  // "}}"
        (TokenType::Text, 16),    // "!"
        (TokenType::Eof, 17),
    ];
    for (ty, offset) in expected {
        let token = stream.current().clone();
        assert!(token.test(ty), "Expected token \"{}\".", ty.to_english());
        assert_eq!(token.offset(), Some(offset));
        if !stream.is_eof() {
            stream.next().unwrap();
        }
    }
}

#[test]
fn testOffsetsAllowRecoveringTheRawExpressionSource() {
    let template = "Hello {{ name|upper ~ '!' }}";
    let mut stream = tokenize(template);
    expect(&mut stream, TokenType::Text);
    let start = expect(&mut stream, TokenType::VarStart).offset().unwrap();
    while !stream.test(TokenType::VarEnd) {
        stream.next().unwrap();
    }
    let end = stream.current().offset().unwrap();
    // slice the raw expression out of the original source, between "{{" and "}}"
    let raw = template[start + 2..end].trim();
    assert_eq!(raw, "name|upper ~ '!'");
}

#[test]
fn testOffsetsReferToTheOriginalSourceWhenLineEndingsAreNormalized() {
    let template = "Hello\r\n{{ name }}";
    let mut stream = tokenize(template);
    expect(&mut stream, TokenType::Text);
    let start = expect(&mut stream, TokenType::VarStart).offset().unwrap();
    assert_eq!(&template[start..start + 2], "{{");
    let name = expect(&mut stream, TokenType::Name).offset().unwrap();
    assert_eq!(&template[name..name + 4], "name");
    let end = expect(&mut stream, TokenType::VarEnd).offset().unwrap();
    assert_eq!(template[start + 2..end].trim(), "name");
}

#[test]
fn testBlockTagDelimitersPointAtTheMarkers() {
    let template = "{% set x = 1 %}";
    let mut stream = tokenize(template);
    // the opening "{%" and the closing "%}" both point at the marker itself,
    // not at the whitespace the closing regex also consumes
    assert_eq!(expect(&mut stream, TokenType::BlockStart).offset(), Some(0));
    assert_eq!(&template[0..2], "{%");
    while !stream.test(TokenType::BlockEnd) {
        stream.next().unwrap();
    }
    let end = stream.current().offset().unwrap();
    assert_eq!(&template[end..end + 2], "%}");
}

#[test]
fn testClosingDelimiterLineMatchesTheMarkerLine() {
    let e = parse_error("{% from 'forms.twig'\n  %}");
    assert_eq!(e.template_line(), 2);
    assert_eq!(e.template_column(), Some(3));
    assert!(e.message().ends_with(b"at line 2 column 3."));
}

#[test]
fn testSyntheticTokensHaveNoOffset() {
    assert_eq!(Token::new(TokenType::Name, "foo", 1).offset(), None);
}

#[test]
fn testSyntaxErrorReportsTheColumn() {
    let e = tokenize_error("{{ 1 + 3) }}\n{{ ok }}");
    assert_eq!(e.template_line(), 1);
    assert_eq!(e.template_column(), Some(9));
    assert!(e.message().ends_with(b"at line 1 column 9."));
}

#[test]
fn testSyntaxErrorColumnUsesOriginalSourceOffsets() {
    let e = parse_error("x\r\n{{ 1__2 }}");
    assert_eq!(e.template_line(), 2);
    assert_eq!(e.template_column(), Some(5));
    assert!(e.message().ends_with(b"at line 2 column 5."));
}
