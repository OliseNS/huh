use std::path::PathBuf;

use crate::config::{DEFAULT_MODEL, SearchMode};
use crate::util::non_empty;

pub(crate) fn help() -> String {
    format!(
        "\
huh - ask an LLM from your terminal

USAGE
  huh [options] <question>
  echo <question> | huh
  huh -- <question>             ask a question starting with -

OPTIONS
  -c, --continue                continue the previous conversation
  -f, --file [path]             attach a file; omit path to pick one
  -m, --model <name>            model for this request
  -s, --search                  force web search
  -ns, --no-search              disable web search

COMMANDS
  --login                       save your OpenRouter API key
  --logout                      remove the saved key
  --config                      show configuration
  -u, --update                  rebuild and install from source
  -V, --version
  -h, --help

Options come before the question.
Default model: {DEFAULT_MODEL}
Env: OPENROUTER_API_KEY, HUH_MODEL, NO_COLOR
"
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileSpec {
    Path(PathBuf),
    Pick,
}

#[derive(Default)]
pub(crate) struct Options {
    pub(crate) model: Option<String>,
    pub(crate) search: Option<SearchMode>,
    pub(crate) continue_session: bool,
    pub(crate) files: Vec<FileSpec>,
    pub(crate) prompt: Vec<String>,
}

impl Options {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut literal = false;
        let mut index = 0;

        while index < args.len() {
            let arg = &args[index];
            if literal {
                options.prompt.push(arg.clone());
            } else {
                match arg.as_str() {
                    "--" => literal = true,
                    "-m" | "--model" => {
                        let value = next_value(args, &mut index, arg)?;
                        options.model = Some(
                            non_empty(value)
                                .ok_or_else(|| format!("{arg} value cannot be empty"))?
                                .to_string(),
                        );
                    }
                    "-f" | "--file" => match args.get(index + 1) {
                        Some(value) if !value.starts_with('-') => {
                            if value.is_empty() {
                                return Err(format!("{arg} value cannot be empty"));
                            }
                            options.files.push(FileSpec::Path(PathBuf::from(value)));
                            index += 1;
                        }
                        _ => options.files.push(FileSpec::Pick),
                    },
                    "-s" | "--search" => options.search = Some(SearchMode::On),
                    "-ns" | "--no-search" => options.search = Some(SearchMode::Off),
                    "-c" | "--continue" => options.continue_session = true,
                    option if option.starts_with('-') && option != "-" => {
                        return Err(format!(
                            "unknown option: {option}\nRun `huh --help` for usage."
                        ));
                    }
                    _ => {
                        options.prompt.push(arg.clone());
                        literal = true;
                    }
                }
            }
            index += 1;
        }

        Ok(options)
    }
}

fn next_value<'a>(args: &'a [String], index: &mut usize, option: &str) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}

pub(crate) fn no_extra_args(args: &[String], option: &str) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("{option} takes no arguments"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        let args: Vec<String> = args.iter().map(|value| value.to_string()).collect();
        Options::parse(&args).unwrap()
    }

    #[test]
    fn parses_short_and_long_options() {
        let short = parse(&[
            "-m",
            "vendor/model",
            "-s",
            "-c",
            "-f",
            "a.rs",
            "--file",
            "b.rs",
            "hello",
            "world",
        ]);
        assert_eq!(short.model.as_deref(), Some("vendor/model"));
        assert_eq!(short.search, Some(SearchMode::On));
        assert!(short.continue_session);
        assert_eq!(
            short.files,
            vec![
                FileSpec::Path(PathBuf::from("a.rs")),
                FileSpec::Path(PathBuf::from("b.rs"))
            ]
        );
        assert_eq!(short.prompt.join(" "), "hello world");

        let long = parse(&[
            "--model",
            "vendor/model",
            "--search",
            "--no-search",
            "--continue",
            "--file",
            "a.rs",
            "hi",
        ]);
        assert_eq!(long.model.as_deref(), Some("vendor/model"));
        assert_eq!(long.search, Some(SearchMode::Off));
        assert!(long.continue_session);
        assert_eq!(long.files, vec![FileSpec::Path(PathBuf::from("a.rs"))]);
        assert_eq!(long.prompt.join(" "), "hi");
    }

    #[test]
    fn parses_interactive_file_picks() {
        assert_eq!(parse(&["-f"]).files, vec![FileSpec::Pick]);

        let with_flag = parse(&["-f", "-s", "hello"]);
        assert_eq!(with_flag.files, vec![FileSpec::Pick]);
        assert_eq!(with_flag.search, Some(SearchMode::On));
        assert_eq!(with_flag.prompt.join(" "), "hello");

        let mixed = parse(&["-f", "a.rs", "-f", "-ns", "hello"]);
        assert_eq!(
            mixed.files,
            vec![FileSpec::Path(PathBuf::from("a.rs")), FileSpec::Pick]
        );
    }

    #[test]
    fn no_search_short_flag_wins() {
        assert_eq!(parse(&["-s", "-ns", "hello"]).search, Some(SearchMode::Off));
        assert_eq!(
            parse(&["-ns", "--search", "hello"]).search,
            Some(SearchMode::On)
        );
    }

    #[test]
    fn treats_remaining_words_as_prompt_and_keeps_defaults() {
        let options = parse(&["--", "-m", "not-a-flag"]);
        assert_eq!(options.model, None);
        assert_eq!(options.prompt.join(" "), "-m not-a-flag");
        assert_eq!(options.search, None);
        assert!(!options.continue_session);
        assert!(options.files.is_empty());

        let mid_prompt = parse(&["-c", "is", "-m", "main?"]);
        assert_eq!(mid_prompt.model, None);
        assert!(mid_prompt.continue_session);
        assert_eq!(mid_prompt.prompt.join(" "), "is -m main?");
    }

    #[test]
    fn rejects_invalid_options() {
        let unknown = vec!["--nope".to_string()];
        assert!(Options::parse(&unknown).is_err());

        let empty_file: Vec<String> = ["--file", "", "hi"]
            .iter()
            .map(|value| value.to_string())
            .collect();
        assert!(Options::parse(&empty_file).is_err());
    }

    #[test]
    fn help_lists_every_public_flag() {
        let help = help();
        for flag in [
            "-c",
            "--continue",
            "-f",
            "--file",
            "-m",
            "--model",
            "-s",
            "--search",
            "-ns",
            "--no-search",
            "--login",
            "--logout",
            "--config",
            "-u",
            "--update",
            "-V",
            "--version",
            "-h",
            "--help",
        ] {
            assert!(help.contains(flag), "help is missing {flag}");
        }
        assert!(help.len() < 1400);
    }
}
