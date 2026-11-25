//! Port of Twig\FileExtensionEscapingStrategy.php: the `'name'`
//! auto-escaping strategy, from the template name's extension.
//!
//! Upstream: twig/twig src/FileExtensionEscapingStrategy.php

/// `FileExtensionEscapingStrategy::guess()`: the strategy, or `None`
/// (`false`) to disable escaping.
#[must_use]
pub fn guess(name: &[u8]) -> Option<&'static [u8]> {
    if matches!(name.last(), Some(b'/' | b'\\')) {
        return Some(b"html"); // return html for directories
    }
    let name = name.strip_suffix(b".twig").unwrap_or(name);
    // pathinfo($name, PATHINFO_EXTENSION): after the last "." of the base
    // name (PHP on Linux splits paths on "/" only).
    let base = name.rsplit(|&b| b == b'/').next().unwrap_or(name);
    let extension = base
        .iter()
        .rposition(|&b| b == b'.')
        .map_or(&b""[..], |i| &base[i + 1..]);
    match extension {
        b"js" | b"json" => Some(b"js"),
        b"css" => Some(b"css"),
        b"txt" => None,
        _ => Some(b"html"),
    }
}
