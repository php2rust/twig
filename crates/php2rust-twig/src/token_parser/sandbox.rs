//! Port of Twig\TokenParser\SandboxTokenParser.php: `{% sandbox %}{% include ... %}{% endsandbox %}`
//! (deprecated since 3.15; the sandbox extension registers it).
//!
//! Upstream: twig/twig src/TokenParser/SandboxTokenParser.php

use symfony_deprecation_contracts::trigger_deprecation;

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Node, NodeClass, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `SandboxTokenParser`.
#[derive(Debug, Default)]
pub struct SandboxTokenParser;

fn decide_block_end(token: &Token) -> bool {
    token.test_name(b"endsandbox")
}

fn ctype_space(text: &[u8]) -> bool {
    !text.is_empty() && text.iter().all(|b| b" \t\n\r\x0b\x0c".contains(b))
}

impl TokenParser for SandboxTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let message = format!(
            "The \"sandbox\" tag is deprecated in \"{}\" at line {}.",
            parser.stream().source_context().name(),
            token.line()
        );
        trigger_deprecation(b"twig/twig", b"3.15", message.as_bytes());
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let body = parser.subparse(Some(EndTest::new("sandbox", decide_block_end)), true)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        if body.is(NodeClass::IncludeNode) {
            body.set_attr("sandboxed", true);
        } else {
            for node in body.child_nodes() {
                if node.is(NodeClass::TextNode)
                    && ctype_space(&node.attr_bytes("data").unwrap_or_default())
                {
                    continue;
                }
                if !node.is(NodeClass::IncludeNode) {
                    return Err(Error::syntax(
                        "Only \"include\" tags are allowed within a \"sandbox\" section.",
                        node.line(),
                        Some(parser.stream().source_context().clone()),
                    ));
                }
                node.set_attr("sandboxed", true);
            }
        }
        Ok(statement::sandbox(body, line_of(token)))
    }

    fn tag(&self) -> &'static str {
        "sandbox"
    }
}
