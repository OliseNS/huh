use std::fs;
use std::path::PathBuf;

use crate::util::{env_value, non_empty};

pub(crate) struct FileContext {
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

pub(crate) fn load_files(paths: &[PathBuf]) -> Result<Vec<FileContext>, String> {
    paths
        .iter()
        .map(|path| {
            fs::read_to_string(path)
                .map(|contents| FileContext {
                    path: path.clone(),
                    contents,
                })
                .map_err(|error| format!("failed to read {}: {error}", path.display()))
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct WhyContext {
    pub(crate) command: Option<String>,
    pub(crate) exit_code: Option<String>,
    pub(crate) output: Option<String>,
}

impl WhyContext {
    pub(crate) fn from_env() -> Self {
        Self {
            command: env_value("HUH_LAST_COMMAND"),
            exit_code: env_value("HUH_LAST_EXIT_CODE"),
            output: None,
        }
    }

    pub(crate) fn has_context(&self, files: &[FileContext]) -> bool {
        self.command.is_some()
            || self.output.as_deref().and_then(non_empty).is_some()
            || !files.is_empty()
    }
}

pub(crate) fn compose_question(question: Option<&str>, files: &[FileContext]) -> String {
    let mut sections = Vec::new();
    if let Some(question) = question.and_then(non_empty) {
        sections.push(question.to_string());
    }
    sections.extend(files.iter().map(format_file));
    sections.join("\n\n")
}

pub(crate) fn compose_why(
    context: &WhyContext,
    question: Option<&str>,
    files: &[FileContext],
) -> String {
    let mut sections = Vec::new();
    if let Some(command) = context.command.as_deref().and_then(non_empty) {
        sections.push(format!("Command:\n```sh\n{}\n```", command.trim()));
    }
    if let Some(exit_code) = context.exit_code.as_deref().and_then(non_empty) {
        sections.push(format!("Exit code: {}", exit_code.trim()));
    }
    if let Some(output) = context.output.as_deref().and_then(non_empty) {
        sections.push(format!("Output:\n```\n{}\n```", output.trim()));
    }
    if let Some(question) = question.and_then(non_empty) {
        sections.push(format!("Question: {question}"));
    }
    sections.extend(files.iter().map(format_file));
    sections.join("\n\n")
}

fn format_file(file: &FileContext) -> String {
    format!(
        "File: {}\n```\n{}\n```",
        file.path.display(),
        file.contents.trim_end()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, contents: &str) -> FileContext {
        FileContext {
            path: PathBuf::from(path),
            contents: contents.to_string(),
        }
    }

    #[test]
    fn composes_question_with_files() {
        let prompt = compose_question(Some("explain this"), &[file("main.rs", "fn main() {}\n")]);
        assert_eq!(
            prompt,
            "explain this\n\nFile: main.rs\n```\nfn main() {}\n```"
        );
    }

    #[test]
    fn composes_why_context() {
        let context = WhyContext {
            command: Some("cargo build".to_string()),
            exit_code: Some("101".to_string()),
            output: Some("error[E0308]: mismatched types".to_string()),
        };
        let prompt = compose_why(&context, Some("how do I fix it?"), &[]);
        assert_eq!(
            prompt,
            "Command:\n```sh\ncargo build\n```\n\nExit code: 101\n\nOutput:\n```\nerror[E0308]: mismatched types\n```\n\nQuestion: how do I fix it?"
        );
    }

    #[test]
    fn composes_why_from_piped_output_only() {
        let context = WhyContext {
            output: Some("error: could not compile".to_string()),
            ..WhyContext::default()
        };
        assert_eq!(
            compose_why(&context, None, &[]),
            "Output:\n```\nerror: could not compile\n```"
        );
    }

    #[test]
    fn composes_why_with_files() {
        let prompt = compose_why(
            &WhyContext::default(),
            Some("what now?"),
            &[file("build.log", "boom\n")],
        );
        assert_eq!(
            prompt,
            "Question: what now?\n\nFile: build.log\n```\nboom\n```"
        );
    }

    #[test]
    fn loads_named_files() {
        let path =
            std::env::temp_dir().join(format!("huh-context-test-{}.txt", std::process::id()));
        fs::write(&path, "contents").unwrap();
        let files = load_files(std::slice::from_ref(&path)).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].contents, "contents");
    }

    #[test]
    fn empty_context_produces_empty_prompt() {
        assert!(compose_why(&WhyContext::default(), None, &[]).is_empty());
        assert!(compose_question(None, &[]).is_empty());
    }

    #[test]
    fn detects_missing_why_context() {
        assert!(!WhyContext::default().has_context(&[]));
        assert!(
            WhyContext {
                command: Some("cargo build".to_string()),
                ..WhyContext::default()
            }
            .has_context(&[])
        );
        assert!(
            WhyContext {
                output: Some("error: nope".to_string()),
                ..WhyContext::default()
            }
            .has_context(&[])
        );
        assert!(WhyContext::default().has_context(&[file("build.log", "boom")]));
    }
}
