use std::sync::LazyLock;

use regex::Regex;

#[allow(clippy::expect_used)]
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

    cleaned = NAV_RE.replace_all(&cleaned, "").into_owned();
    cleaned = HEADER_RE.replace_all(&cleaned, "").into_owned();
    cleaned = FOOTER_RE.replace_all(&cleaned, "").into_owned();

    cleaned
}

#[allow(clippy::expect_used)]
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
