use std::sync::LazyLock;

use regex::Regex;

#[allow(clippy::expect_used)]
static MD_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").expect("static regex"));

pub(super) fn strip_navigation_blocks(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    if lines.is_empty() {
        return markdown.to_owned();
    }

    let mut is_nav = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_nav_line_by_links(trimmed) {
            is_nav[i] = true;
        }
    }

    let mut i = 0;
    while i < lines.len() {
        if is_list_item_line(lines[i]) {
            let start = i;
            while i < lines.len() && (is_list_item_line(lines[i]) || lines[i].trim().is_empty()) {
                i += 1;
            }
            let consecutive = (start..i).filter(|j| is_list_item_line(lines[*j])).count();
            if consecutive >= 3 {
                for j in start..i {
                    if !lines[j].trim().is_empty() {
                        is_nav[j] = true;
                    }
                }
            }
        } else {
            i += 1;
        }
    }

    let mut cleaned: Vec<&str> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if is_nav[i] {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            cleaned.push(line);
            continue;
        }
        if cleaned.iter().any(|prev| prev.trim() == trimmed) {
            continue;
        }
        cleaned.push(line);
    }

    let result = cleaned.join("\n").trim().to_owned();

    if result.len() < 100 {
        return markdown.to_owned();
    }

    result
}

pub(super) fn is_nav_line_by_links(line: &str) -> bool {
    let (_, link_count) = count_markdown_links(line);
    if link_count == 0 {
        return false;
    }

    let plain = MD_LINK_RE.replace_all(line, "").trim().to_string();

    if link_count >= 2 && plain.len() < 15 {
        return true;
    }

    if link_count >= 3 {
        return true;
    }

    let total = line.len();
    if total < 80 && link_count == 1 && plain.len() < 10 {
        return true;
    }

    false
}

pub(super) fn count_markdown_links(text: &str) -> (usize, usize) {
    let mut chars = 0usize;
    let mut count = 0usize;
    for m in MD_LINK_RE.find_iter(text) {
        chars += m.len();
        count += 1;
    }
    (chars, count)
}

pub(super) fn is_list_item_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('*') || trimmed.starts_with('-') {
        return true;
    }
    if let Some(first) = trimmed.chars().next() {
        return first.is_ascii_digit() && trimmed.contains(". ");
    }
    false
}
