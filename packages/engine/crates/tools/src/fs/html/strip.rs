//! HTML tag/boilerplate stripping.

use std::sync::LazyLock;

use regex::Regex;

/// Strip boilerplate HTML tags that are never useful content for a model:
/// `<script>`, `<style>`, `<noscript>`, `<link>`, and `<meta>`.
///
/// Uses simple regex removal — no DOM parser.
#[allow(clippy::expect_used)]
/// Strip inline boilerplate tags and semantic chrome elements whose content is
/// never useful to a model.
///
/// Removed:
/// - `<script>`, `<style>`, `<noscript>`, `<link>`, `<meta>` (tags + content)
/// - `<nav>`, `<header>`, `<footer>` (entire element with all descendants)
pub(super) fn strip_html_boilerplate(html: &str) -> String {
    static SCRIPT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("static regex"));
    static STYLE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("static regex"));
    static NOSCRIPT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").expect("static regex"));
    static LINK_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<link[^>]*/?>").expect("static regex"));
    static META_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<meta[^>]*/?>").expect("static regex"));

    // Semantic chrome: remove the entire element including all nested content.
    // These tags are never nested within themselves, so a non-greedy match
    // correctly captures the outermost pair.
    static NAV_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<nav[^>]*>.*?</nav>").expect("static regex"));
    static HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<header[^>]*>.*?</header>").expect("static regex"));
    static FOOTER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<footer[^>]*>.*?</footer>").expect("static regex"));

    let mut cleaned = SCRIPT_RE.replace_all(html, "").into_owned();
    cleaned = STYLE_RE.replace_all(&cleaned, "").into_owned();
    cleaned = NOSCRIPT_RE.replace_all(&cleaned, "").into_owned();
    cleaned = LINK_RE.replace_all(&cleaned, "").into_owned();
    cleaned = META_RE.replace_all(&cleaned, "").into_owned();

    // Strip semantic chrome elements (with content) before markdown conversion.
    // Order matters: strip nav first so we don't miss a closing </nav> that's
    // inside a header.
    cleaned = NAV_RE.replace_all(&cleaned, "").into_owned();
    cleaned = HEADER_RE.replace_all(&cleaned, "").into_owned();
    cleaned = FOOTER_RE.replace_all(&cleaned, "").into_owned();

    cleaned
}

/// Strip `<nav>`, `<header>`, and `<footer>` elements from HTML while
/// preserving the rest. Used before link extraction to exclude chrome links.
#[allow(clippy::expect_used)] // static regexes are infallible
pub(super) fn strip_semantic_chrome(html: &str) -> String {
    static NAV_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<nav[^>]*>.*?</nav>").expect("static regex"));
    static HEADER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<header[^>]*>.*?</header>").expect("static regex"));
    static FOOTER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<footer[^>]*>.*?</footer>").expect("static regex"));

    let mut cleaned = NAV_RE.replace_all(html, "").into_owned();
    cleaned = HEADER_RE.replace_all(&cleaned, "").into_owned();
    cleaned = FOOTER_RE.replace_all(&cleaned, "").into_owned();
    cleaned
}
