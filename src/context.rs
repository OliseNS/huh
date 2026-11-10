use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::util::non_empty;

const FILE_LIMIT_BYTES: u64 = 256 * 1024;
const TOTAL_LIMIT_BYTES: u64 = 512 * 1024;

pub(crate) struct FileContext {
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

pub(crate) fn load_files(paths: &[PathBuf]) -> Result<Vec<FileContext>, String> {
    let mut total = 0u64;
    let mut files = Vec::with_capacity(paths.len());

    for path in paths {
        let metadata = fs::metadata(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "failed to read {}: not a regular file",
                path.display()
            ));
        }
        if metadata.len() > FILE_LIMIT_BYTES {
            return Err(file_too_large(path, metadata.len()));
        }

        let file = File::open(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "failed to read {}: not a regular file",
                path.display()
            ));
        }

        let cap = FILE_LIMIT_BYTES
            .min(TOTAL_LIMIT_BYTES.saturating_sub(total))
            .saturating_add(1);
        let mut bytes = Vec::new();
        file.take(cap)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;

        let size = bytes.len() as u64;
        if size > FILE_LIMIT_BYTES {
            return Err(file_too_large(path, size));
        }
        if total + size > TOTAL_LIMIT_BYTES {
            return Err(format!(
                "attachments exceed the {TOTAL_LIMIT_BYTES} byte total limit"
            ));
        }
        total += size;

        let contents = String::from_utf8(bytes)
            .map_err(|_| format!("failed to read {}: not valid UTF-8", path.display()))?;
        files.push(FileContext {
            path: path.clone(),
            contents,
        });
    }

    Ok(files)
}

fn file_too_large(path: &Path, size: u64) -> String {
    format!(
        "{} is {size} bytes; the limit is {FILE_LIMIT_BYTES} bytes per file (attach a trimmed excerpt instead)",
        path.display()
    )
}

pub(crate) fn compose_question(question: Option<&str>, files: &[FileContext]) -> String {
    let mut sections = Vec::new();
    if let Some(question) = question.and_then(non_empty) {
        sections.push(question.to_string());
    }
    sections.extend(files.iter().map(format_file));
    sections.join("\n\n")
}

fn format_file(file: &FileContext) -> String {
    fenced_block(
        &format!("File: {}", file.path.display()),
        file.contents.trim_end(),
    )
}

fn fenced_block(label: &str, contents: &str) -> String {
    let fence = "`".repeat(longest_run(contents, '`').max(2) + 1);
    format!("{}\n{fence}\n{contents}\n{fence}", sanitize_label(label))
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|character| {
            if character.is_control() || character == '`' {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn longest_run(text: &str, needle: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for character in text.chars() {
        if character == needle {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn file(path: &str, contents: &str) -> FileContext {
        FileContext {
            path: PathBuf::from(path),
            contents: contents.to_string(),
        }
    }

    #[test]
    fn appends_files_to_question_prompts() {
        assert_eq!(
            compose_question(Some("explain this"), &[file("main.rs", "fn main() {}\n")]),
            "explain this\n\nFile: main.rs\n```\nfn main() {}\n```"
        );
        assert_eq!(
            compose_question(None, &[file("build.log", "boom\n")]),
            "File: build.log\n```\nboom\n```"
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
    fn empty_question_without_files_is_empty() {
        assert!(compose_question(None, &[]).is_empty());
    }
}
