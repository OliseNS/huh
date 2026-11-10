use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::{LoadError, load_json, save_json, xdg_path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Message {
    pub(crate) role: String,
    pub(crate) content: String,
}

impl Message {
    pub(crate) fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub(crate) fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

const MAX_HISTORY_MESSAGES: usize = 10;
const MAX_HISTORY_CHARS: usize = 40_000;

pub(crate) fn trim_history(messages: &mut Vec<Message>) {
    let mut chars: usize = messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum();

    while messages.len() > MAX_HISTORY_MESSAGES
        || (chars > MAX_HISTORY_CHARS && !messages.is_empty())
    {
        let drop_count = 2.min(messages.len());
        for message in messages.drain(..drop_count) {
            chars -= message.content.chars().count();
        }
    }
}

fn state_path() -> Result<PathBuf, String> {
    xdg_path("XDG_STATE_HOME", &[".local", "state"], "session.json")
}

fn valid_history(messages: &[Message]) -> bool {
    messages.len().is_multiple_of(2)
        && messages.iter().enumerate().all(|(index, message)| {
            let expected = if index.is_multiple_of(2) {
                "user"
            } else {
                "assistant"
            };
            message.role == expected
        })
}

fn backup_invalid_session(path: &Path) {
    let backup = path.with_extension("json.bad");
    if let Err(error) = fs::rename(path, &backup) {
        eprintln!(
            "warning: could not move invalid session to {}: {error}",
            backup.display()
        );
    }
}

pub(crate) fn load_session() -> Result<Vec<Message>, String> {
    let path = state_path()?;
    match load_json::<Vec<Message>>(&path) {
        Ok(Some(messages)) if valid_history(&messages) => Ok(messages),
        Ok(Some(_)) => {
            eprintln!("warning: ignoring invalid session at {}", path.display());
            backup_invalid_session(&path);
            Ok(Vec::new())
        }
        Ok(None) => Ok(Vec::new()),
        Err(LoadError::Parse(error)) => {
            eprintln!("warning: ignoring invalid session ({error})");
            backup_invalid_session(&path);
            Ok(Vec::new())
        }
        Err(LoadError::Read(error)) => Err(error),
    }
}

pub(crate) fn save_session(messages: &[Message]) -> Result<(), String> {
    let path = state_path()?;
    save_json(&path, messages, "session")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(label: &str, width: usize, fill: char) -> String {
        let mut content = label.to_string();
        while content.chars().count() < width {
            content.push(fill);
        }
        content
    }

    fn conversation(pairs: usize, chars_per_message: usize) -> Vec<Message> {
        (0..pairs)
            .flat_map(|index| {
                [
                    Message::user(padded(&format!("q{index}"), chars_per_message, 'x')),
                    Message::assistant(padded(&format!("a{index}"), chars_per_message, 'y')),
                ]
            })
            .collect()
    }

    #[test]
    fn validates_session_shape() {
        assert!(valid_history(&[]));
        assert!(valid_history(&[
            Message::user("q"),
            Message::assistant("a")
        ]));
        assert!(!valid_history(&[Message::user("q")]));
        assert!(!valid_history(&[
            Message::assistant("a"),
            Message::user("q")
        ]));
        assert!(!valid_history(&[Message::user("q"), Message::user("a")]));
    }

    #[test]
    fn trims_history_to_five_pairs() {
        let mut messages = conversation(8, 0);
        trim_history(&mut messages);
        assert_eq!(messages.len(), MAX_HISTORY_MESSAGES);
        assert_eq!(messages[0], Message::user("q3"));
        assert_eq!(messages[9], Message::assistant("a7"));
    }

    #[test]
    fn trims_history_by_character_budget_dropping_whole_pairs() {
        let mut messages = conversation(5, 10_000);
        trim_history(&mut messages);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0], Message::user(padded("q3", 10_000, 'x')));
        assert_eq!(messages[3], Message::assistant(padded("a4", 10_000, 'y')));
    }

    #[test]
    fn keeps_history_at_the_character_budget() {
        let mut messages = conversation(2, 10_000);
        trim_history(&mut messages);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0], Message::user(padded("q0", 10_000, 'x')));
    }
}
