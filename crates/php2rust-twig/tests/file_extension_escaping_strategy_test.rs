//! FileExtensionEscapingStrategyTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test. `false` is `None`.
//!
//! Upstream: twig/twig tests/FileExtensionEscapingStrategyTest.php
#![allow(non_snake_case)]

use twig::file_extension_escaping_strategy::guess;

#[test]
fn testGuess() {
    for (strategy, filename) in [
        // default
        (Some("html"), "foo.html"),
        (Some("html"), "foo.html.twig"),
        (Some("html"), "foo"),
        (Some("html"), "foo.bar.twig"),
        (Some("html"), "foo.txt/foo"),
        (Some("html"), "foo.txt/foo.js/"),
        // css
        (Some("css"), "foo.css"),
        (Some("css"), "foo.css.twig"),
        (Some("css"), "foo.twig.css"),
        (Some("css"), "foo.js.css"),
        (Some("css"), "foo.js.css.twig"),
        // js
        (Some("js"), "foo.js"),
        (Some("js"), "foo.js.twig"),
        (Some("js"), "foo.txt/foo.js"),
        (Some("js"), "foo.txt.twig/foo.js"),
        // txt
        (None, "foo.txt"),
        (None, "foo.txt.twig"),
    ] {
        assert_eq!(
            guess(filename.as_bytes()),
            strategy.map(str::as_bytes),
            "{filename}"
        );
    }
}
