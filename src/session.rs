use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::storage::{backup_invalid, load_or_backup, save_json, xdg_path};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
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

const MAX_HISTORY_MESSAGES: usize = 20;
const MAX_HISTORY_CHARS: usize = 80_000;

pub(crate) fn trim_history(messages: &mut Vec<Message>) {
    debug_assert!(
        messages.len().is_multiple_of(2),
        "history must contain complete request/answer pairs"
    );
    let mut chars: usize = messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum();

    while (messages.len() > MAX_HISTORY_MESSAGES || chars > MAX_HISTORY_CHARS) && messages.len() > 4
    {
        for message in messages.drain(2..4) {
            chars -= message.content.chars().count();
        }
    }

    while chars > MAX_HISTORY_CHARS && messages.len() > 2 {
        for message in messages.drain(..2) {
            chars -= message.content.chars().count();
        }
    }
}

fn state_path() -> Result<PathBuf, String> {
    xdg_path("XDG_STATE_HOME", &[".local", "state"], "session.json")
}

fn valid_history(messages: &[Message]) -> bool {
    let (pairs, remainder) = messages.as_chunks::<2>();
    remainder.is_empty()
        && pairs
            .iter()
            .all(|pair| pair[0].role == "user" && pair[1].role == "assistant")
}

pub(crate) fn last_answer(messages: &[Message]) -> Option<&str> {
    messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant")
        .map(|message| message.content.as_str())
}

pub(crate) fn load_session() -> Result<Vec<Message>, String> {
    let path = state_path()?;
    let Some(messages) = load_or_backup::<Vec<Message>>(&path, "session")? else {
        return Ok(Vec::new());
    };
    if valid_history(&messages) {
        return Ok(messages);
    }
    eprintln!("warning: ignoring invalid session at {}", path.display());
    backup_invalid(&path);
    Ok(Vec::new())
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
    fn finds_the_latest_answer() {
        let messages = vec![
            Message::user("q1"),
            Message::assistant("a1"),
            Message::user("q2"),
            Message::assistant("a2"),
        ];
        assert_eq!(last_answer(&messages), Some("a2"));
        assert_eq!(last_answer(&[]), None);
        assert_eq!(last_answer(&[Message::user("q")]), None);
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
    fn trims_history_to_ten_pairs_keeping_the_original_pair() {
        let mut messages = conversation(12, 0);
        trim_history(&mut messages);
        assert_eq!(messages.len(), MAX_HISTORY_MESSAGES);
        assert_eq!(messages[0], Message::user("q0"));
        assert_eq!(messages[1], Message::assistant("a0"));
        assert_eq!(messages[2], Message::user("q3"));
        assert_eq!(messages[19], Message::assistant("a11"));
    }

    #[test]
    fn trims_history_by_character_budget_keeping_the_original_pair() {
        let mut messages = conversation(5, 10_000);
        trim_history(&mut messages);
        assert_eq!(messages.len(), 8);
        assert_eq!(messages[0], Message::user(padded("q0", 10_000, 'x')));
        assert_eq!(messages[1], Message::assistant(padded("a0", 10_000, 'y')));
        assert_eq!(messages[2], Message::user(padded("q2", 10_000, 'x')));
        assert_eq!(messages[7], Message::assistant(padded("a4", 10_000, 'y')));
    }

    #[test]
    fn keeps_original_and_latest_pairs_at_the_character_budget() {
        let mut messages = conversation(4, 20_000);
        trim_history(&mut messages);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0], Message::user(padded("q0", 20_000, 'x')));
        assert_eq!(messages[1], Message::assistant(padded("a0", 20_000, 'y')));
        assert_eq!(messages[2], Message::user(padded("q3", 20_000, 'x')));
        assert_eq!(messages[3], Message::assistant(padded("a3", 20_000, 'y')));
    }

    #[test]
    fn drops_the_anchor_when_history_stays_over_budget() {
        let mut messages = conversation(3, 30_000);
        trim_history(&mut messages);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], Message::user(padded("q2", 30_000, 'x')));
        assert_eq!(messages[1], Message::assistant(padded("a2", 30_000, 'y')));
    }
}
