use agentloop_delegator_common::{DelegatorEvent, DelegatorMapError, LineMapper};

#[derive(Debug, Default)]
pub struct CopilotLineMapper {
    emitted_anything: bool,
}

impl CopilotLineMapper {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LineMapper for CopilotLineMapper {
    fn map_line(&mut self, line: &str) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
        let cleaned = strip_ansi(line);
        let trimmed = cleaned.trim_end();

        if is_footer_noise(trimmed) {
            return Ok(Vec::new());
        }
        if trimmed.is_empty() && !self.emitted_anything {
            return Ok(Vec::new());
        }

        self.emitted_anything = true;
        Ok(vec![DelegatorEvent::AssistantDelta {
            text: format!("{trimmed}\n"),
        }])
    }
}

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

fn is_footer_noise(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("Total usage est:")
        || line.starts_with("Total duration")
        || line.starts_with("Usage:")
        || line.starts_with("Suggestion:")
        || line.starts_with("? for shortcuts")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(events: Vec<DelegatorEvent>) -> String {
        events
            .into_iter()
            .map(|event| match event {
                DelegatorEvent::AssistantDelta { text } => text,
                other => panic!("unexpected event: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn plain_markdown_passes_through() {
        let mut mapper = CopilotLineMapper::new();
        let mut out = String::new();
        for line in ["# Answer", "", "The result is `pong`."] {
            out.push_str(&text_of(mapper.map_line(line).expect("maps")));
        }
        assert_eq!(out, "# Answer\n\nThe result is `pong`.\n");
    }

    #[test]
    fn ansi_escapes_are_stripped() {
        let mut mapper = CopilotLineMapper::new();
        let events = mapper
            .map_line(
                "\u{1b}[1mBold answer\u{1b}[0m and \u{1b}]8;;https://x\u{7}link\u{1b}]8;;\u{7}",
            )
            .expect("maps");
        assert_eq!(text_of(events), "Bold answer and link\n");
    }

    #[test]
    fn leading_blanks_and_footer_are_suppressed() {
        let mut mapper = CopilotLineMapper::new();
        assert!(mapper.map_line("").expect("maps").is_empty());
        assert!(mapper.map_line("   ").expect("maps").is_empty());
        let events = mapper.map_line("pong").expect("maps");
        assert_eq!(text_of(events), "pong\n");
        assert!(
            mapper
                .map_line("Total usage est: 1 premium request")
                .expect("maps")
                .is_empty()
        );
        assert_eq!(text_of(mapper.map_line("").expect("maps")), "\n");
    }
}
