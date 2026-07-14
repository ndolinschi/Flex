use super::*;

#[test]
fn parse_extracts_result_blocks() {
    let html = r#"
        <div class="result results_links results_links_deep web-result">
          <div class="links_main links_deep result__body">
            <a class="result__a" href="https://example.com/rust">The Rust Programming Language</a>
            <div class="result__extras"><div class="result__extras__url"><a class="result__url" href="https://example.com/rust">example.com/rust</a></div></div>
            <a class="result__snippet">A language empowering everyone to build reliable and efficient software.</a>
          </div>
        </div>
        "#;
    let results = parse_duckduckgo_html(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "The Rust Programming Language");
    assert_eq!(results[0].url, "https://example.com/rust");
    assert!(
        results[0].snippet.contains("reliable and efficient"),
        "{}",
        results[0].snippet
    );
}

#[test]
fn parse_skips_ad_results() {
    let html = r#"
        <div class="result results_links results_links_deep web-result">
          <div class="links_main links_deep result__body">
            <a class="result__a" href="https://duckduckgo.com/y.js?u3=...">Ad Title</a>
            <a class="result__snippet">Ad snippet</a>
          </div>
        </div>
        "#;
    let results = parse_duckduckgo_html(html);
    assert!(results.is_empty());
}

#[test]
fn html_entities_are_decoded() {
    assert_eq!(html_entity_decode("hello &amp; world"), "hello & world");
    assert_eq!(html_entity_decode("a &lt; b"), "a < b");
}

#[test]
fn block_page_detector() {
    assert!(looks_like_ddg_block_page(
        "<html><body>Please complete the following challenge</body></html>"
    ));
    assert!(!looks_like_ddg_block_page(
        "<div class=\"result__body\"><a class=\"result__a\" href=\"https://x\">x</a></div>"
    ));
}
