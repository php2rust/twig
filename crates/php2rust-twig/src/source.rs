//! Port of Twig\Source.php: a template's code, logical name and path.
//!
//! The code is bytes, as a PHP string is. The path is the filesystem
//! path PHP shows (`Error::getFile()`), kept as PHP's bytes.
//!
//! Upstream: twig/twig src/Source.php

/// Holds information about a non-compiled Twig template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    code: Vec<u8>,
    name: String,
    path: Vec<u8>,
}

impl Source {
    /// `new Source($code, $name)`.
    pub fn new(code: impl Into<Vec<u8>>, name: impl Into<String>) -> Self {
        Self::with_path(code, name, Vec::new())
    }

    /// `new Source($code, $name, $path)`.
    pub fn with_path(
        code: impl Into<Vec<u8>>,
        name: impl Into<String>,
        path: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
            path: path.into(),
        }
    }

    #[must_use]
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn path(&self) -> &[u8] {
        &self.path
    }

    /// `getColumn()`: the 1-based column of a 0-based byte offset, counting
    /// `\r\n` and `\r` as one newline. An offset past the end counts up to
    /// the end, as `substr()` does.
    #[must_use]
    pub fn column(&self, offset: usize) -> usize {
        let before = normalize_newlines(&self.code[..offset.min(self.code.len())]);
        match before.iter().rposition(|&b| b == b'\n') {
            Some(line_start) => before.len() - line_start,
            None => before.len() + 1,
        }
    }
}

/// `str_replace(["\r\n", "\r"], "\n", $text)`.
pub(crate) fn normalize_newlines(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\r' {
            out.push(b'\n');
            if text.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
        } else {
            out.push(text[i]);
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_counts_bytes_after_the_last_newline() {
        let source = Source::new(&b"ab\r\ncd\ref"[..], "index");
        assert_eq!(source.column(0), 1);
        assert_eq!(source.column(2), 3);
        assert_eq!(source.column(4), 1);
        assert_eq!(source.column(5), 2);
        assert_eq!(source.column(7), 1);
        assert_eq!(source.column(100), 3);
    }
}
