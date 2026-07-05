//! The prompt editor: a `tui_textarea` wrapper with history and the slash
//! autocomplete popup (non-modal, so it lives here rather than in
//! [`crate::overlay`]).

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{CursorMove, TextArea};

use crate::commands::{CommandEntry, CommandIndex};
use crate::files::{
    FileIndex, MentionPreview, MentionSpan, active_mention, build_mention_preview,
    cursor_byte_offset, parse_line_slice, replace_mention, resolve_mention_path,
    split_mention_query,
};

/// What a key did to the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputOutcome {
    /// The user submitted this text (already cleared from the editor).
    Submitted(String),
    /// The key was handled (edit, popup navigation, history).
    Consumed,
    /// Not an editor key.
    Ignored,
}

/// Max rows shown in the inline `/` and `@` autocomplete lists.
pub(crate) const POPUP_LIST_MAX_ROWS: usize = 8;

/// Scroll offset so `selected` stays within the visible popup window.
pub(crate) fn popup_list_scroll_offset(
    selected: usize,
    visible_rows: usize,
    total: usize,
) -> usize {
    if total <= visible_rows {
        return 0;
    }
    let window = visible_rows.max(1);
    let max_scroll = total - window;
    if selected < window {
        0
    } else if selected >= max_scroll {
        max_scroll
    } else {
        selected + 1 - window
    }
}

/// Slash-command autocomplete state.
#[derive(Debug, Clone)]
pub struct CommandPopup {
    /// The first token after `/`, used as the filter.
    pub filter: String,
    pub matches: Vec<CommandEntry>,
    pub selected: usize,
}

/// `@` file mention autocomplete state.
#[derive(Debug, Clone)]
pub struct FilePopup {
    pub filter: String,
    pub path_part: String,
    pub matches: Vec<String>,
    pub selected: usize,
    pub span: MentionSpan,
    pub preview: Option<MentionPreview>,
}

/// Active inline autocomplete.
#[derive(Debug, Clone)]
pub enum InputPopup {
    Command(CommandPopup),
    File(FilePopup),
}

/// A large paste collapsed to a placeholder in the visible editor. The full
/// text is substituted back in at submit time, so it still reaches the model
/// — this keeps the editor readable and keeps arbitrarily large/adversarial
/// pasted content from ever being run through the live `@`-mention and
/// render pipeline before submission.
#[derive(Debug, Clone)]
struct PastedBlock {
    placeholder: String,
    full_text: String,
}

/// Pastes past either threshold are collapsed to a `[Pasted text #N +K
/// lines]` placeholder instead of being inserted inline.
const PASTE_PLACEHOLDER_MIN_LINES: usize = 5;
const PASTE_PLACEHOLDER_MIN_CHARS: usize = 400;

/// The prompt editor state.
pub struct InputState {
    pub textarea: TextArea<'static>,
    pub history: Vec<String>,
    history_pos: Option<usize>,
    /// Text stashed while browsing history.
    stash: Option<String>,
    pub popup: Option<InputPopup>,
    /// Large pastes made since the last submit, keyed by their placeholder.
    pasted_blocks: Vec<PastedBlock>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            textarea: new_textarea(),
            history: Vec::new(),
            history_pos: None,
            stash: None,
            popup: None,
            pasted_blocks: Vec::new(),
        }
    }
}

fn new_textarea() -> TextArea<'static> {
    let mut textarea = TextArea::default();
    textarea
        .set_placeholder_text("Type a message, / commands, @ files · @path:[0:12] python slice");
    textarea.set_cursor_line_style(ratatui::style::Style::default());
    textarea
}

impl InputState {
    /// The current editor text (lines joined by newlines).
    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Whether the editor is empty.
    pub fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    /// Replace the editor contents.
    pub fn set_text(&mut self, text: &str) {
        self.textarea = new_textarea();
        self.textarea.insert_str(text);
    }

    /// Insert pasted text at the cursor. Large pastes are collapsed to a
    /// short placeholder; the full text is restored at submit time.
    pub fn paste(&mut self, text: &str) {
        let line_count = text.lines().count();
        let is_large =
            line_count > PASTE_PLACEHOLDER_MIN_LINES || text.chars().count() > PASTE_PLACEHOLDER_MIN_CHARS;
        if !is_large {
            self.textarea.insert_str(text);
            return;
        }
        let index = self.pasted_blocks.len() + 1;
        let placeholder = format!("[Pasted text #{index} +{line_count} lines]");
        self.textarea.insert_str(&placeholder);
        self.pasted_blocks.push(PastedBlock {
            placeholder,
            full_text: text.to_owned(),
        });
    }

    /// Substitute every pasted-block placeholder in `text` with its stored
    /// full text (in insertion order). The visible editor only ever shows
    /// the short placeholder; the submitted message carries the real paste.
    fn expand_pasted_blocks(&self, text: &str) -> String {
        let mut expanded = text.to_owned();
        for block in &self.pasted_blocks {
            expanded = expanded.replace(&block.placeholder, &block.full_text);
        }
        expanded
    }

    /// Handle one key press. `commands` feeds the slash autocomplete popup;
    /// `files` feeds the `@` mention popup.
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        commands: &CommandIndex,
        files: &FileIndex,
        workdir: &Path,
    ) -> InputOutcome {
        if self.popup.is_some() {
            match self.handle_popup_key(key) {
                PopupOutcome::Consumed => return InputOutcome::Consumed,
                PopupOutcome::Submit => return self.submit(),
                PopupOutcome::Fallthrough => {}
            }
        }
        match (key.code, key.modifiers) {
            (KeyCode::Enter, KeyModifiers::NONE) => self.submit(),
            (KeyCode::Enter, KeyModifiers::ALT)
            | (KeyCode::Enter, KeyModifiers::SHIFT)
            | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                self.textarea.insert_newline();
                self.refresh_popup(commands, files, workdir);
                InputOutcome::Consumed
            }
            (KeyCode::Up, KeyModifiers::NONE) if self.single_line() => {
                self.history_prev();
                InputOutcome::Consumed
            }
            (KeyCode::Down, KeyModifiers::NONE) if self.single_line() => {
                self.history_next();
                InputOutcome::Consumed
            }
            _ => {
                let changed = self.textarea.input(key);
                if changed {
                    self.history_pos = None;
                    self.stash = None;
                }
                self.refresh_popup(commands, files, workdir);
                InputOutcome::Consumed
            }
        }
    }

    fn handle_popup_key(&mut self, key: KeyEvent) -> PopupOutcome {
        let Some(popup) = self.popup.as_mut() else {
            return PopupOutcome::Fallthrough;
        };
        match popup {
            InputPopup::Command(popup) => match (key.code, key.modifiers) {
                (KeyCode::Up, _) => {
                    if popup.selected == 0 {
                        popup.selected = popup.matches.len().saturating_sub(1);
                    } else {
                        popup.selected -= 1;
                    }
                    PopupOutcome::Consumed
                }
                (KeyCode::Down, _) => {
                    popup.selected = (popup.selected + 1) % popup.matches.len().max(1);
                    PopupOutcome::Consumed
                }
                (KeyCode::Tab, _) => {
                    self.complete_command_selected();
                    PopupOutcome::Consumed
                }
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    self.complete_command_selected();
                    PopupOutcome::Submit
                }
                (KeyCode::Esc, _) => {
                    self.popup = None;
                    PopupOutcome::Consumed
                }
                _ => PopupOutcome::Fallthrough,
            },
            InputPopup::File(popup) => match (key.code, key.modifiers) {
                (KeyCode::Up, _) => {
                    if popup.selected == 0 {
                        popup.selected = popup.matches.len().saturating_sub(1);
                    } else {
                        popup.selected -= 1;
                    }
                    PopupOutcome::Consumed
                }
                (KeyCode::Down, _) => {
                    popup.selected = (popup.selected + 1) % popup.matches.len().max(1);
                    PopupOutcome::Consumed
                }
                (KeyCode::Tab, _) => {
                    self.complete_file_selected();
                    PopupOutcome::Consumed
                }
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    self.complete_file_selected();
                    PopupOutcome::Consumed
                }
                (KeyCode::Esc, _) => {
                    self.popup = None;
                    PopupOutcome::Consumed
                }
                _ => PopupOutcome::Fallthrough,
            },
        }
    }

    fn complete_command_selected(&mut self) {
        let Some(InputPopup::Command(popup)) = self.popup.take() else {
            return;
        };
        let Some(entry) = popup.matches.get(popup.selected) else {
            return;
        };
        // Preserve any arguments already typed after the first token.
        let text = self.text();
        let rest = text
            .split_once(char::is_whitespace)
            .map(|(_, rest)| rest.to_owned());
        let line = match rest {
            Some(rest) if !rest.is_empty() => format!("/{} {rest}", entry.name),
            _ => format!("/{} ", entry.name),
        };
        self.set_text(&line);
    }

    fn complete_file_selected(&mut self) {
        let Some(InputPopup::File(popup)) = self.popup.take() else {
            return;
        };
        let Some(path) = popup.matches.get(popup.selected) else {
            return;
        };
        let (_, slice_part) = split_mention_query(&popup.span.query);
        let slice_suffix = slice_part
            .map(|slice| format!(":{slice}"))
            .unwrap_or_default();
        let text = self.text();
        let replacement = format!("@{path}{slice_suffix} ");
        let updated = replace_mention(&text, &popup.span, &replacement);
        self.set_text(&updated);
        let cursor_offset = popup.span.start + replacement.len();
        let (row, col) = crate::files::byte_offset_to_cursor(self.textarea.lines(), cursor_offset);
        self.textarea
            .move_cursor(CursorMove::Jump(row as u16, col as u16));
    }

    fn submit(&mut self) -> InputOutcome {
        let text = self.text();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return InputOutcome::Consumed;
        }
        let line = self.expand_pasted_blocks(trimmed);
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }
        self.history_pos = None;
        self.stash = None;
        self.popup = None;
        self.textarea = new_textarea();
        self.pasted_blocks.clear();
        InputOutcome::Submitted(line)
    }

    fn single_line(&self) -> bool {
        self.textarea.lines().len() <= 1
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.history_pos {
            None => {
                self.stash = Some(self.text());
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(p) => p - 1,
        };
        self.history_pos = Some(pos);
        let entry = self.history[pos].clone();
        self.set_text(&entry);
    }

    fn history_next(&mut self) {
        let Some(pos) = self.history_pos else {
            return;
        };
        if pos + 1 < self.history.len() {
            self.history_pos = Some(pos + 1);
            let entry = self.history[pos + 1].clone();
            self.set_text(&entry);
        } else {
            self.history_pos = None;
            let stash = self.stash.take().unwrap_or_default();
            self.set_text(&stash);
        }
    }

    /// Recompute inline autocomplete from the current text and cursor.
    pub fn refresh_popup(&mut self, commands: &CommandIndex, files: &FileIndex, workdir: &Path) {
        if let Some(popup) = self.refresh_command_popup(commands) {
            self.popup = Some(popup);
            return;
        }
        if let Some(popup) = self.refresh_file_popup(files, workdir) {
            self.popup = Some(popup);
            return;
        }
        self.popup = None;
    }

    fn refresh_command_popup(&mut self, commands: &CommandIndex) -> Option<InputPopup> {
        let lines = self.textarea.lines();
        let active = lines.len() == 1
            && lines[0].starts_with('/')
            && !lines[0].contains(char::is_whitespace);
        if !active {
            return None;
        }
        let filter = lines[0][1..].to_owned();
        let matches = commands.matches(&filter);
        if matches.is_empty() {
            return None;
        }
        let selected = self
            .popup
            .as_ref()
            .and_then(|popup| match popup {
                InputPopup::Command(popup) => Some(popup.selected.min(matches.len() - 1)),
                InputPopup::File(_) => None,
            })
            .unwrap_or(0);
        Some(InputPopup::Command(CommandPopup {
            filter,
            matches,
            selected,
        }))
    }

    fn refresh_file_popup(&mut self, files: &FileIndex, workdir: &Path) -> Option<InputPopup> {
        let lines = self.textarea.lines();
        let text = self.text();
        let cursor = cursor_byte_offset(lines, self.textarea.cursor());
        let span = active_mention(&text, cursor)?;
        let (path_part, slice_part) = split_mention_query(&span.query);
        let matches = files.matches(path_part);
        let selected = self
            .popup
            .as_ref()
            .and_then(|popup| match popup {
                InputPopup::File(popup) if popup.span == span => {
                    Some(popup.selected.min(matches.len().saturating_sub(1)))
                }
                _ => None,
            })
            .unwrap_or(0);
        let preview = slice_part
            .and_then(|slice_raw| parse_line_slice(slice_raw).ok())
            .and_then(|slice| {
                resolve_mention_path(workdir, path_part, &matches, selected)
                    .map(|path| build_mention_preview(workdir, &path, &slice))
            });
        if matches.is_empty() && preview.is_none() {
            return None;
        }
        Some(InputPopup::File(FilePopup {
            filter: span.query.clone(),
            path_part: path_part.to_owned(),
            matches,
            selected,
            span,
            preview,
        }))
    }
}

enum PopupOutcome {
    Consumed,
    Submit,
    Fallthrough,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn popup_scroll_offset_keeps_selection_visible() {
        assert_eq!(popup_list_scroll_offset(0, 8, 20), 0);
        assert_eq!(popup_list_scroll_offset(7, 8, 20), 0);
        assert_eq!(popup_list_scroll_offset(8, 8, 20), 1);
        assert_eq!(popup_list_scroll_offset(11, 8, 20), 4);
        assert_eq!(popup_list_scroll_offset(15, 8, 20), 12);
        assert_eq!(popup_list_scroll_offset(3, 8, 5), 0);
    }

    #[test]
    fn file_popup_inserts_selected_path() {
        let files = FileIndex::from_paths(vec!["src/foo.rs".to_owned(), "src/bar.rs".to_owned()]);
        let workdir = std::env::temp_dir();
        let mut input = InputState::default();
        input.set_text("fix @src/f");
        input.refresh_popup(&CommandIndex::default(), &files, &workdir);
        assert!(matches!(input.popup, Some(InputPopup::File(_))));

        let outcome = input.handle_key(
            key(KeyCode::Enter),
            &CommandIndex::default(),
            &files,
            &workdir,
        );
        assert_eq!(outcome, InputOutcome::Consumed);
        assert_eq!(input.text(), "fix @src/foo.rs ");
        assert!(input.popup.is_none());
    }

    #[test]
    fn file_popup_preserves_slice_suffix_on_complete() {
        let files = FileIndex::from_paths(vec!["src/foo.rs".to_owned()]);
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        std::fs::write(dir.path().join("src/foo.rs"), "line\n").expect("write");
        let mut input = InputState::default();
        input.set_text("see @src/foo.rs:[2:");
        input.refresh_popup(&CommandIndex::default(), &files, dir.path());
        let outcome = input.handle_key(
            key(KeyCode::Tab),
            &CommandIndex::default(),
            &files,
            dir.path(),
        );
        assert_eq!(outcome, InputOutcome::Consumed);
        assert_eq!(input.text(), "see @src/foo.rs:[2: ");
    }

    #[test]
    fn paste_keeps_small_block_inline() {
        let mut input = InputState::default();
        input.paste("just two\nshort lines");
        assert_eq!(input.text(), "just two\nshort lines");
        assert!(input.pasted_blocks.is_empty());
    }

    #[test]
    fn paste_collapses_large_block_to_placeholder() {
        let mut input = InputState::default();
        let big = (0..50).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        input.paste(&big);
        assert_eq!(input.text(), "[Pasted text #1 +50 lines]");
        assert_eq!(input.pasted_blocks.len(), 1);
        assert_eq!(input.pasted_blocks[0].full_text, big);
    }

    #[test]
    fn paste_collapses_long_single_line_by_char_count() {
        let mut input = InputState::default();
        let long_line = "x".repeat(500);
        input.paste(&long_line);
        assert_eq!(input.text(), "[Pasted text #1 +1 lines]");
    }

    #[test]
    fn submit_expands_placeholder_back_to_full_pasted_text() {
        let mut input = InputState::default();
        let big = (0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        input.paste(&big);
        let outcome = input.handle_key(
            key(KeyCode::Enter),
            &CommandIndex::default(),
            &FileIndex::default(),
            &std::env::temp_dir(),
        );
        assert_eq!(outcome, InputOutcome::Submitted(big));
        assert!(input.pasted_blocks.is_empty());
    }

    #[test]
    fn submit_expands_multiple_pasted_blocks_in_order() {
        let mut input = InputState::default();
        let first = (0..10).map(|i| format!("a{i}")).collect::<Vec<_>>().join("\n");
        let second = (0..10).map(|i| format!("b{i}")).collect::<Vec<_>>().join("\n");
        input.paste(&first);
        input.textarea.insert_str(" then ");
        input.paste(&second);
        let outcome = input.handle_key(
            key(KeyCode::Enter),
            &CommandIndex::default(),
            &FileIndex::default(),
            &std::env::temp_dir(),
        );
        assert_eq!(
            outcome,
            InputOutcome::Submitted(format!("{first} then {second}"))
        );
    }

    #[test]
    fn text_around_pasted_placeholder_is_still_editable() {
        let mut input = InputState::default();
        let big = (0..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        input.paste(&big);
        input.textarea.insert_str(" — please review");
        assert_eq!(
            input.text(),
            "[Pasted text #1 +30 lines] — please review"
        );
    }

    #[test]
    fn file_popup_esc_keeps_partial_mention() {
        let files = FileIndex::from_paths(vec!["src/foo.rs".to_owned()]);
        let workdir = std::env::temp_dir();
        let mut input = InputState::default();
        input.set_text("see @src");
        input.refresh_popup(&CommandIndex::default(), &files, &workdir);
        let outcome = input.handle_key(
            key(KeyCode::Esc),
            &CommandIndex::default(),
            &files,
            &workdir,
        );
        assert_eq!(outcome, InputOutcome::Consumed);
        assert_eq!(input.text(), "see @src");
        assert!(input.popup.is_none());
    }
}
