//! The prompt editor: a `tui_textarea` wrapper with history and the slash
//! autocomplete popup (non-modal, so it lives here rather than in
//! [`crate::overlay`]).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{CursorMove, TextArea};

use crate::commands::{CommandEntry, CommandIndex};
use crate::files::{FileIndex, MentionSpan, active_mention, cursor_byte_offset, replace_mention};

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
    pub matches: Vec<String>,
    pub selected: usize,
    pub span: MentionSpan,
}

/// Active inline autocomplete.
#[derive(Debug, Clone)]
pub enum InputPopup {
    Command(CommandPopup),
    File(FilePopup),
}

/// The prompt editor state.
pub struct InputState {
    pub textarea: TextArea<'static>,
    pub history: Vec<String>,
    history_pos: Option<usize>,
    /// Text stashed while browsing history.
    stash: Option<String>,
    pub popup: Option<InputPopup>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            textarea: new_textarea(),
            history: Vec::new(),
            history_pos: None,
            stash: None,
            popup: None,
        }
    }
}

fn new_textarea() -> TextArea<'static> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Type a message, / for commands, @ for files");
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

    /// Insert pasted text at the cursor.
    pub fn paste(&mut self, text: &str) {
        self.textarea.insert_str(text);
    }

    /// Handle one key press. `commands` feeds the slash autocomplete popup;
    /// `files` feeds the `@` mention popup.
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        commands: &CommandIndex,
        files: &FileIndex,
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
                self.refresh_popup(commands, files);
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
                self.refresh_popup(commands, files);
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
        let text = self.text();
        let replacement = format!("@{path} ");
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
        let line = trimmed.to_owned();
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }
        self.history_pos = None;
        self.stash = None;
        self.popup = None;
        self.textarea = new_textarea();
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
    pub fn refresh_popup(&mut self, commands: &CommandIndex, files: &FileIndex) {
        if let Some(popup) = self.refresh_command_popup(commands) {
            self.popup = Some(popup);
            return;
        }
        if let Some(popup) = self.refresh_file_popup(files) {
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

    fn refresh_file_popup(&mut self, files: &FileIndex) -> Option<InputPopup> {
        let lines = self.textarea.lines();
        let text = self.text();
        let cursor = cursor_byte_offset(lines, self.textarea.cursor());
        let span = active_mention(&text, cursor)?;
        let matches = files.matches(&span.query);
        if matches.is_empty() {
            return None;
        }
        let selected = self
            .popup
            .as_ref()
            .and_then(|popup| match popup {
                InputPopup::File(popup) if popup.span == span => {
                    Some(popup.selected.min(matches.len() - 1))
                }
                _ => None,
            })
            .unwrap_or(0);
        Some(InputPopup::File(FilePopup {
            filter: span.query.clone(),
            matches,
            selected,
            span,
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
    fn file_popup_inserts_selected_path() {
        let files = FileIndex::from_paths(vec!["src/foo.rs".to_owned(), "src/bar.rs".to_owned()]);
        let mut input = InputState::default();
        input.set_text("fix @src/f");
        input.refresh_popup(&CommandIndex::default(), &files);
        assert!(matches!(input.popup, Some(InputPopup::File(_))));

        let outcome = input.handle_key(key(KeyCode::Enter), &CommandIndex::default(), &files);
        assert_eq!(outcome, InputOutcome::Consumed);
        assert_eq!(input.text(), "fix @src/foo.rs ");
        assert!(input.popup.is_none());
    }

    #[test]
    fn file_popup_esc_keeps_partial_mention() {
        let files = FileIndex::from_paths(vec!["src/foo.rs".to_owned()]);
        let mut input = InputState::default();
        input.set_text("see @src");
        input.refresh_popup(&CommandIndex::default(), &files);
        let outcome = input.handle_key(key(KeyCode::Esc), &CommandIndex::default(), &files);
        assert_eq!(outcome, InputOutcome::Consumed);
        assert_eq!(input.text(), "see @src");
        assert!(input.popup.is_none());
    }
}
