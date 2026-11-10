use std::path::PathBuf;

use crate::config::DEFAULT_MODEL;
use crate::util::non_empty;

pub(crate) fn help() -> String {
    format!(
        "\
huh - ask an LLM from your terminal (OpenRouter)

USAGE:
  huh [OPTIONS] <prompt>       Ask a question
  echo <prompt> | huh          Ask a question from stdin
  huh -- <prompt>              Ask a prompt that starts with -

CONVERSATION:
  -c, --continue                  Continue the previous conversation

DEBUGGING:
  -y, --why                       Explain a command failure (use with huh-why)
  -f, --file <path>               Attach a file to the prompt (repeatable)

WEB SEARCH:
  -s, --search                    Force a web search
  -ns, --no-search                Disable web search for this request

OPTIONS:
  -m, --model <name>              Model for this request
  -V, --version                   Show version
  -h, --help                      Show this help

COMMANDS:
  -u, --update                    Rebuild and reinstall from source
  --login                         Save your OpenRouter API key
  --logout                        Remove the saved API key
  --config                        Show configuration and defaults

DEFAULT MODEL:
  {DEFAULT_MODEL} (override with -m)

ENVIRONMENT:
  OPENROUTER_API_KEY           Overrides the saved API key
  HUH_MODEL                    Default model (overridden by -m)
  NO_COLOR                     Disable colored output
  HUH_LAST_COMMAND             Previous command for --why (set by huh-why)
  HUH_LAST_EXIT_CODE           Previous exit code for --why (set by huh-why)
"
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchMode {
    #[default]
    Auto,
    Force,
    Disabled,
}

#[derive(Default)]
pub(crate) struct Options {
    pub(crate) model: Option<String>,
    pub(crate) search: SearchMode,
    pub(crate) continue_session: bool,
    pub(crate) why: bool,
    pub(crate) files: Vec<PathBuf>,
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
                    "-f" | "--file" => {
                        let value = next_value(args, &mut index, arg)?;
                        if value.is_empty() {
                            return Err(format!("{arg} value cannot be empty"));
                        }
                        if value.starts_with('-') {
                            return Err(format!(
                                "{arg} value cannot start with '-' (use ./ for a file named like that)"
                            ));
                        }
                        options.files.push(PathBuf::from(value));
                    }
                    "-s" | "--search" => options.search = SearchMode::Force,
                    "-ns" | "--no-search" => options.search = SearchMode::Disabled,
                    "-c" | "--continue" => options.continue_session = true,
                    "-y" | "--why" => options.why = true,
                    option if option.starts_with('-') && option != "-" => {
                        return Err(format!(
                            "unknown option: {option}\nRun `huh --help` for usage."
                        ));
                    }
                    _ => options.prompt.push(arg.clone()),
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
        let options = parse(&[
            "-m",
            "vendor/model",
            "-s",
            "-y",
            "-c",
            "-f",
            "a.rs",
            "--file",
            "b.rs",
            "hello",
            "world",
        ]);
        assert_eq!(options.model.as_deref(), Some("vendor/model"));
        assert_eq!(options.search, SearchMode::Force);
        assert!(options.why);
        assert!(options.continue_session);
        assert_eq!(
            options.files,
            vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
        );
        assert_eq!(options.prompt.join(" "), "hello world");
    }

    #[test]
    fn parses_long_aliases() {
        let options = parse(&[
            "--model",
            "vendor/model",
            "--search",
            "--no-search",
            "--continue",
            "--why",
            "--file",
            "a.rs",
            "hi",
        ]);
        assert_eq!(options.model.as_deref(), Some("vendor/model"));
        assert_eq!(options.search, SearchMode::Disabled);
        assert!(options.continue_session);
        assert!(options.why);
        assert_eq!(options.files, vec![PathBuf::from("a.rs")]);
        assert_eq!(options.prompt.join(" "), "hi");
    }

    #[test]
    fn no_search_short_flag_wins() {
        assert_eq!(parse(&["-s", "-ns", "hello"]).search, SearchMode::Disabled);
        assert_eq!(
            parse(&["-ns", "--search", "hello"]).search,
            SearchMode::Force
        );
    }

    #[test]
    fn flags_are_off_by_default() {
        let options = parse(&["hello"]);
        assert_eq!(options.search, SearchMode::Auto);
        assert!(!options.continue_session);
        assert!(!options.why);
        assert!(options.files.is_empty());
    }

    #[test]
    fn double_dash_treats_the_rest_as_prompt() {
        let options = parse(&["--", "-m", "not-a-flag"]);
        assert_eq!(options.model, None);
        assert_eq!(options.prompt.join(" "), "-m not-a-flag");
    }

    #[test]
    fn rejects_unknown_options() {
        let args = vec!["--nope".to_string()];
        assert!(Options::parse(&args).is_err());
    }

    #[test]
    fn rejects_flag_like_file_values() {
        let args: Vec<String> = ["--file", "--why", "hi"]
            .iter()
            .map(|value| value.to_string())
            .collect();
        assert!(Options::parse(&args).is_err());
    }
}
