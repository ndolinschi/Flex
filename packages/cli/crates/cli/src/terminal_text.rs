//! Normalize raw terminal output for ratatui display.
//!
//! Tool, shell, and MCP output may contain ANSI color sequences and carriage
//! returns from progress spinners. Ratatui lays out one [`Line`] per row; lone
//! `\r` bytes written to a real terminal move the cursor and produce overlapping
//! text unless we strip escapes and resolve overwrites first.

/// Strip ANSI escapes, normalize newlines, and resolve `\r` overwrites.
pub(crate) fn normalize_terminal_text(input: &str) -> String {
    let stripped = strip_ansi(input);
    normalize_line_endings(&stripped)
}

/// Split normalized terminal output into display lines.
pub(crate) fn terminal_lines(input: &str) -> Vec<String> {
    let text = normalize_terminal_text(input);
    if text.is_empty() {
        return Vec::new();
    }
    text.lines().map(str::to_owned).collect()
}

fn normalize_line_endings(input: &str) -> String {
    input
        .replace("\r\n", "\n")
        .split('\n')
        .map(|line| line.rsplit('\r').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove ANSI CSI/OSC escape sequences.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {
                chars.next();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYWRIGHT_HELP: &str = concat!(
        "Usage: npx playwright [options] [command]\r\n",
        "\r\n",
        "Options:\r\n",
        "  -V, --version  output the version number\r\n",
        "  -h, --help     display help for command\r\n",
        "\r\n",
        "Commands:\r\n",
        "  codegen [options] [url]        open codegen recorder\r\n",
        "  install [options] [browser...] install browsers\r\n",
        "  test [options] [test-filter...] run tests\r\n",
    );

    #[test]
    fn strips_ansi_color_codes() {
        let raw = "\u{1b}[32mUsage:\u{1b}[0m npx playwright";
        assert_eq!(normalize_terminal_text(raw), "Usage: npx playwright");
    }

    #[test]
    fn resolves_carriage_return_overwrite() {
        assert_eq!(
            normalize_terminal_text("Downloading...\rUsage: npx playwright"),
            "Usage: npx playwright"
        );
    }

    #[test]
    fn playwright_help_splits_into_lines() {
        let lines = terminal_lines(PLAYWRIGHT_HELP);
        assert_eq!(lines[0], "Usage: npx playwright [options] [command]");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "Options:");
        assert!(lines.iter().any(|line| line.starts_with("  codegen")));
        assert!(lines.iter().any(|line| line.starts_with("  install")));
    }

    #[test]
    fn progress_prefix_overwrite_then_help() {
        let raw = format!("Installing browsers...\r{PLAYWRIGHT_HELP}");
        let lines = terminal_lines(&raw);
        assert_eq!(lines[0], "Usage: npx playwright [options] [command]");
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("Installing browsers"))
        );
    }

    #[test]
    fn empty_input_yields_no_lines() {
        assert!(terminal_lines("").is_empty());
    }
}
