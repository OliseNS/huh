use std::io::{self, IsTerminal};
use std::path::PathBuf;

use inquire::autocompletion::{Autocomplete, Replacement};
use inquire::{CustomUserError, InquireError, Text};

use crate::cli::FileSpec;

const MAX_SUGGESTIONS: usize = 20;
const PAGE_SIZE: usize = 10;

pub(crate) fn resolve_files(specs: &[FileSpec]) -> Result<Vec<PathBuf>, String> {
    specs
        .iter()
        .map(|spec| match spec {
            FileSpec::Path(path) => Ok(path.clone()),
            FileSpec::Pick => pick_file(),
        })
        .collect()
}

pub(crate) fn ask_question(files: &[PathBuf]) -> Result<String, String> {
    require_terminal()?;
    let default = if files.len() == 1 {
        "Explain this file."
    } else {
        "Explain these files."
    };
    let message = match files {
        [file] => format!(
            "Ask about {}:",
            file.file_name()
                .unwrap_or(file.as_os_str())
                .to_string_lossy()
        ),
        _ => "Ask about these files:".to_string(),
    };

    Text::new(&message)
        .with_default(default)
        .with_help_message("Type your question, or press Enter for the default")
        .prompt()
        .map_err(prompt_error)
}

fn pick_file() -> Result<PathBuf, String> {
    require_terminal()?;
    let mut initial = None;
    let mut notice = None;

    loop {
        let help = notice
            .take()
            .unwrap_or_else(|| "Type to filter · Tab completes · Enter selects".to_string());
        let mut prompt = Text::new("Attach a file:")
            .with_autocomplete(FileCompleter)
            .with_page_size(PAGE_SIZE)
            .with_help_message(&help);
        if let Some(value) = initial.as_deref() {
            prompt = prompt.with_initial_value(value);
        }

        let answer = prompt.prompt().map_err(prompt_error)?;
        let answer = answer.trim().to_string();
        match resolve_path(&answer) {
            Some(path) if path.is_dir() => {
                let dir = path.display().to_string();
                initial = Some(format!("{}/", dir.trim_end_matches('/')));
            }
            Some(path) => return Ok(path),
            None => {
                initial = Some(answer.clone());
                notice = Some(format!("No such file: {answer}"));
            }
        }
    }
}

fn resolve_path(answer: &str) -> Option<PathBuf> {
    let path = PathBuf::from(answer);
    if path.exists() {
        return Some(path);
    }
    let mut matches = suggestions(answer);
    if matches.len() == 1 {
        let path = PathBuf::from(matches.remove(0));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn require_terminal() -> Result<(), String> {
    if io::stdin().is_terminal() && io::stderr().is_terminal() {
        Ok(())
    } else {
        Err("interactive prompts need a terminal; pass -f <path> and a question".to_string())
    }
}

fn prompt_error(error: InquireError) -> String {
    match error {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => {
            "prompt cancelled".to_string()
        }
        other => format!("prompt failed: {other}"),
    }
}

#[derive(Clone)]
struct FileCompleter;

impl Autocomplete for FileCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        Ok(suggestions(input))
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        Ok(highlighted_suggestion
            .or_else(|| suggestions(input).into_iter().next())
            .map(Replacement::Some)
            .unwrap_or(Replacement::None))
    }
}

fn suggestions(input: &str) -> Vec<String> {
    let (dir, prefix, needle) = split(input);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let needle = needle.to_lowercase();
    let show_hidden = needle.starts_with('.');

    let mut matches: Vec<(bool, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.to_lowercase().starts_with(&needle) || (name.starts_with('.') && !show_hidden)
            {
                return None;
            }
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            let suffix = if is_dir { "/" } else { "" };
            Some((is_dir, format!("{prefix}{name}{suffix}")))
        })
        .collect();

    matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    matches.truncate(MAX_SUGGESTIONS);
    matches.into_iter().map(|(_, path)| path).collect()
}

fn split(input: &str) -> (PathBuf, &str, &str) {
    match input.rfind('/') {
        Some(index) => (
            PathBuf::from(&input[..=index]),
            &input[..=index],
            &input[index + 1..],
        ),
        None => (PathBuf::from("."), "", input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_path_into_directory_prefix_and_needle() {
        assert_eq!(split("src/ma"), (PathBuf::from("src/"), "src/", "ma"));
        assert_eq!(split("src/"), (PathBuf::from("src/"), "src/", ""));
        assert_eq!(split("main"), (PathBuf::from("."), "", "main"));
        assert_eq!(split("/etc/ho"), (PathBuf::from("/etc/"), "/etc/", "ho"));
    }

    #[test]
    fn lists_directories_first_and_filters_by_prefix() {
        let root = std::env::temp_dir().join(format!("huh-picker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("main.rs"), "").unwrap();
        std::fs::write(root.join("notes.txt"), "").unwrap();

        let prefix = format!("{}/", root.display());
        assert_eq!(
            suggestions(&format!("{prefix}m")),
            vec![format!("{prefix}main.rs")]
        );
        assert_eq!(
            suggestions(&prefix),
            vec![
                format!("{prefix}src/"),
                format!("{prefix}main.rs"),
                format!("{prefix}notes.txt")
            ]
        );
        assert_eq!(
            resolve_path(&format!("{prefix}ma")),
            Some(root.join("main.rs"))
        );
        assert_eq!(resolve_path(&format!("{prefix}s")), Some(root.join("src")));

        std::fs::remove_dir_all(root).unwrap();
    }
}
