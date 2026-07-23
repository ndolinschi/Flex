use super::content::clean_html_for_model;
use super::nav::{
    count_markdown_links, is_list_item_line, is_nav_line_by_links, strip_navigation_blocks,
};

#[test]
fn list_item_detection() {
    assert!(is_list_item_line("* item one"));
    assert!(is_list_item_line("- item two"));
    assert!(is_list_item_line("1. numbered item"));
    assert!(is_list_item_line("  * indented item"));
    assert!(!is_list_item_line(""));
    assert!(!is_list_item_line("This is just regular text."));
    assert!(is_list_item_line("*bold* text"));
}

#[test]
fn markdown_link_counting() {
    let (chars, count) = count_markdown_links("[Home](/) [About](/about)");
    assert_eq!(count, 2);
    assert!(chars > 10);
    let (_, count) = count_markdown_links("Just plain text, no links here.");
    assert_eq!(count, 0);
}

#[test]
fn nav_line_detection_short_link_heavy() {
    assert!(is_nav_line_by_links(
        "[Home](/) [Docs](/docs) [API](/api) [Blog](/blog)"
    ));
    assert!(!is_nav_line_by_links(
        "Read the [installation guide](/install) before proceeding."
    ));
}

#[test]
fn nav_line_detection_many_links() {
    assert!(is_nav_line_by_links(
        "Check [A](/a) [B](/b) and [C](/c) for a very long description of something interesting here"
    ));
}

#[test]
fn nav_line_detection_body_text_with_link() {
    assert!(!is_nav_line_by_links(
        "For more details please refer to the [official documentation](https://example.com)."
    ));
}

#[test]
fn strip_nav_removes_link_dense_lines() {
    let md = "\
[Home](/) [Products](/p) [About](/a) [Contact](/c)

## Introduction

This is the main content of the page. It provides a thorough overview of the
topic and contains enough characters to pass the 200-char safety threshold so
the cleaned version is returned instead of the original.";
    let cleaned = strip_navigation_blocks(md);
    assert!(
        !cleaned.contains("[Home]"),
        "nav line should be stripped: {cleaned}"
    );
    assert!(
        cleaned.contains("## Introduction"),
        "heading should survive"
    );
    assert!(
        cleaned.contains("thorough overview"),
        "body text should survive"
    );
}

#[test]
fn strip_nav_removes_consecutive_list_items() {
    let md = "\
* Navigation Item 1 with a long enough description here okay
* Navigation Item 2 with a long enough description here okay
* Navigation Item 3 with a long enough description here okay

## Article

The actual article content starts here and provides meaningful information
that should be preserved by the content extraction pipeline during scraping.";
    let cleaned = strip_navigation_blocks(md);
    assert!(
        !cleaned.contains("Navigation Item 1"),
        "consecutive list should be stripped: {cleaned}"
    );
    assert!(cleaned.contains("## Article"), "heading should survive");
    assert!(
        cleaned.contains("actual article"),
        "body text should survive"
    );
}

#[test]
fn strip_nav_collapses_duplicate_lines() {
    let md = "\
Copyright 2024 Example Inc.

Copyright 2024 Example Inc.

Unique content that appears only once in the document and must be retained
for proper content extraction to work correctly during web scraping.";
    let cleaned = strip_navigation_blocks(md);
    let copyright_count = cleaned.matches("Copyright 2024 Example Inc.").count();
    assert_eq!(
        copyright_count, 1,
        "duplicate line should be collapsed to one: {cleaned}"
    );
    assert!(
        cleaned.contains("Unique content"),
        "unique content should survive"
    );
}

#[test]
fn strip_nav_preserves_short_content() {
    let md = "[Home](/) [About](/a)";
    let cleaned = strip_navigation_blocks(md);
    assert_eq!(
        cleaned, md,
        "short content after stripping should return original"
    );
}

#[test]
fn strip_nav_preserves_normal_text() {
    let md = "\
# Welcome

This is a normal page with some [links](https://example.com) but not enough
to be considered navigation. The content is substantial and well-structured
so it should pass through the cleaning pipeline without any modification.";
    let cleaned = strip_navigation_blocks(md);
    assert_eq!(
        cleaned, md,
        "normal text with few links should not be modified"
    );
}

#[test]
fn clean_html_for_model_integration() {
    let html = r#"<html><body>
    <nav>
      <a href="/">Home</a>
      <a href="/news">News</a>
      <a href="/science">Science</a>
      <a href="/tech">Tech</a>
      <a href="/contact">Contact</a>
    </nav>
    <article>
      <h1>Fusion Energy Breakthrough</h1>
      <p>Scientists at the ITER project have achieved a significant milestone
      in plasma confinement, sustaining a reaction for over five minutes at
      temperatures exceeding 150 million degrees Celsius.</p>
      <p>The breakthrough represents years of collaborative research across
      multiple international partners and brings commercial fusion energy
      one step closer to reality.</p>
    </article>
    </body></html>"#;
    let result = clean_html_for_model(html);
    assert!(
        result.contains("plasma") || result.contains("ITER"),
        "article body should appear in cleaned output: {result}"
    );
    assert!(
        result.contains("collaborative") || result.contains("commercial"),
        "second paragraph should survive cleaning: {result}"
    );
}
