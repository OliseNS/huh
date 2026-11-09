mod render;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;

use render::{GREEN, RESET, Renderer, color_allowed};

const BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL: &str = "openai/gpt-oss-20b:nitro";
const SYSTEM_PROMPT: &str = "You are the answer engine behind the terminal command `idk <question>`. Print only the answer: no greeting, no restating the question, and no closing remarks. Be accurate, direct, and as concise as completeness allows: a simple question gets a sentence or two, while a complex one gets only the structure needed to explain it fully. Use plain text suitable for a terminal; include Markdown only when it genuinely improves clarity. If something is uncertain or unknown, say so plainly instead of guessing.";

const HELP: &str = "\
idk - ask an LLM from your terminal (OpenRouter)

USAGE:
  idk <prompt>                 Ask a question
  echo <prompt> | idk          Ask a question from stdin
  idk -- <prompt>              Ask a prompt that starts with -

COMMANDS:
  --login                      Save your OpenRouter API key
  --logout                     Remove the saved API key
  --model                      Show the current model
  --model <name>               Set the model
  --effort                     Show the current reasoning effort
  --effort <low|medium|high>   Set reasoning effort
  --config                     Show the effective configuration
  --config-path                Print the config file path
  -h, --help                   Show this help

DEFAULT MODEL:
  openai/gpt-oss-20b:nitro

ENVIRONMENT:
  OPENROUTER_API_KEY           Overrides the saved API key
";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThinkingEffort {
    #[default]
    Low,
    Medium,
    High,
}

impl ThinkingEffort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

impl std::fmt::Display for ThinkingEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<ThinkingEffort>,
}

impl Config {
    fn load() -> Result<Self, String> {
        let path = config_path()?;
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|error| format!("failed to parse {}: {error}", path.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("failed to read {}: {error}", path.display())),
        }
    }

    fn save(&self) -> Result<PathBuf, String> {
        let path = config_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
            set_private_dir(dir)?;
        }

        let contents = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize config: {error}"))?;
        let temp = path.with_file_name("config.json.tmp");
        write_private(&temp, contents.as_bytes())?;
        fs::rename(&temp, &path)
            .map_err(|error| format!("failed to save {}: {error}", path.display()))?;
        Ok(path)
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().and_then(non_empty)
    }

    fn model(&self) -> &str {
        self.model
            .as_deref()
            .and_then(non_empty)
            .unwrap_or(DEFAULT_MODEL)
    }

    fn effort(&self) -> ThinkingEffort {
        self.effort.unwrap_or_default()
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn config_path() -> Result<PathBuf, String> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(xdg).join("idk").join("config.json"));
    }

    let home = env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("idk")
        .join("config.json"))
}

#[cfg(unix)]
fn set_private_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("failed to set permissions on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_private_dir(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("failed to set permissions on {}: {error}", path.display()))?;
    }

    file.write_all(bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn env_api_key() -> Option<String> {
    env::var("OPENROUTER_API_KEY")
        .ok()
        .and_then(|key| non_empty(&key).map(str::to_owned))
}

fn resolved_api_key(config: &Config) -> Option<String> {
    env_api_key().or_else(|| config.api_key().map(str::to_owned))
}

fn read_stdin_prompt() -> Result<String, String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    Ok(buffer.trim().to_string())
}

fn read_secret(label: &str) -> Result<String, String> {
    if io::stdin().is_terminal() {
        rpassword::prompt_password(label)
            .map_err(|error| format!("failed to read API key: {error}"))
    } else {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| format!("failed to read stdin: {error}"))?;
        Ok(input.lines().next().unwrap_or_default().to_string())
    }
}

fn usage_error(program: &str) -> ExitCode {
    eprintln!("USAGE ERR: {program} <prompt>  (or pipe prompt on stdin)");
    ExitCode::FAILURE
}

fn no_extra_args(args: &[String], option: &str) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("{option} takes no arguments"))
    }
}

fn single_value<'a>(args: &'a [String], option: &str) -> Result<Option<&'a str>, String> {
    match args {
        [] => Ok(None),
        [value] => Ok(Some(value.as_str())),
        _ => Err(format!("{option} takes at most one value")),
    }
}

fn login() -> Result<(), String> {
    let key = read_secret("OpenRouter API key: ")?;
    let key = non_empty(&key).ok_or_else(|| "API key cannot be empty".to_string())?;

    let mut config = Config::load()?;
    config.api_key = Some(key.to_string());
    let path = config.save()?;

    println!("Saved API key to {}", path.display());
    if env_api_key().is_some() {
        eprintln!("note: OPENROUTER_API_KEY is set and overrides the saved key");
    }
    Ok(())
}

fn logout() -> Result<(), String> {
    let mut config = Config::load()?;
    if config.api_key.take().is_some() {
        config.save()?;
        println!("Removed saved API key");
    } else {
        println!("No saved API key");
    }
    Ok(())
}

fn set_or_show_model(value: Option<&str>) -> Result<(), String> {
    let mut config = Config::load()?;
    match value {
        None => println!("{}", config.model()),
        Some(value) => {
            let value = non_empty(value).ok_or_else(|| "model name cannot be empty".to_string())?;
            config.model = Some(value.to_string());
            config.save()?;
            println!("model set to {value}");
        }
    }
    Ok(())
}

fn set_or_show_effort(value: Option<&str>) -> Result<(), String> {
    let mut config = Config::load()?;
    match value {
        None => println!("{}", config.effort()),
        Some(value) => {
            let effort = ThinkingEffort::parse(value)
                .ok_or_else(|| format!("invalid effort: {value} (use low, medium, or high)"))?;
            config.effort = Some(effort);
            config.save()?;
            println!("effort set to {effort}");
        }
    }
    Ok(())
}

fn show_config() -> Result<(), String> {
    let path = config_path()?;
    let config = Config::load()?;
    let api_key = if env_api_key().is_some() {
        "set (OPENROUTER_API_KEY environment variable)"
    } else if config.api_key().is_some() {
        "set (saved in config)"
    } else {
        "not set (run `idk --login`)"
    };

    println!("config:   {}", path.display());
    println!("model:    {}", config.model());
    println!("effort:   {}", config.effort());
    println!("api key:  {api_key}");
    println!("base url: {BASE_URL}");
    Ok(())
}

async fn dispatch() -> Result<ExitCode, String> {
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "idk".to_string());
    let args: Vec<String> = args.collect();

    let Some((command, rest)) = args.split_first() else {
        if io::stdin().is_terminal() {
            print!("{HELP}");
            return Ok(ExitCode::SUCCESS);
        }

        let prompt = read_stdin_prompt()?;
        if prompt.is_empty() {
            return Ok(usage_error(&program));
        }
        run_prompt(prompt).await?;
        return Ok(ExitCode::SUCCESS);
    };

    match command.as_str() {
        "-h" | "--help" => {
            no_extra_args(rest, "--help")?;
            print!("{HELP}");
        }
        "--login" => {
            no_extra_args(rest, "--login")?;
            login()?;
        }
        "--logout" => {
            no_extra_args(rest, "--logout")?;
            logout()?;
        }
        "--config" => {
            no_extra_args(rest, "--config")?;
            show_config()?;
        }
        "--config-path" => {
            no_extra_args(rest, "--config-path")?;
            println!("{}", config_path()?.display());
        }
        "--model" => set_or_show_model(single_value(rest, "--model")?)?,
        "--effort" => set_or_show_effort(single_value(rest, "--effort")?)?,
        "--" => {
            let prompt = rest.join(" ");
            if prompt.trim().is_empty() {
                return Ok(usage_error(&program));
            }
            run_prompt(prompt).await?;
        }
        option if option.starts_with('-') => {
            return Err(format!(
                "unknown option: {option}\nRun `idk --help` for usage."
            ));
        }
        _ => run_prompt(args.join(" ")).await?,
    }

    Ok(ExitCode::SUCCESS)
}

#[tokio::main]
async fn main() -> ExitCode {
    match dispatch().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct StreamEvent {
    choices: Vec<Choice>,
    usage: Option<Usage>,
    error: Option<StreamError>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Choice {
    delta: Delta,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Delta {
    content: Option<String>,
    reasoning: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    completion_tokens: u64,
}

#[derive(Deserialize)]
struct StreamError {
    message: Option<String>,
}

fn print_tps(tps: f64, estimated: bool) {
    let message = if estimated {
        format!("tps: {tps:.1} (estimated)")
    } else {
        format!("tps: {tps:.1}")
    };

    let color = io::stderr().is_terminal() && color_allowed();
    if color {
        eprintln!("{GREEN}{message}{RESET}");
    } else {
        eprintln!("{message}");
    }
}

async fn run_prompt(prompt: String) -> Result<(), String> {
    let config = Config::load()?;
    let api_key = resolved_api_key(&config)
        .ok_or_else(|| "no API key found; run `idk --login`".to_string())?;

    let response = reqwest::Client::new()
        .post(format!("{BASE_URL}/chat/completions"))
        .bearer_auth(api_key)
        .json(&json!({
            "model": config.model(),
            "stream": true,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": prompt },
            ],
            "reasoning": { "effort": config.effort().as_str() },
            "stream_options": { "include_usage": true },
        }))
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut state = StreamState::new();
    let mut stream = response.bytes_stream();
    let mut pending: Vec<u8> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("stream failed: {error}"))?;
        pending.extend_from_slice(&chunk);

        while let Some(position) = pending.iter().position(|&byte| byte == b'\n') {
            let rest = pending.split_off(position + 1);
            let line = std::mem::replace(&mut pending, rest);
            state.handle_line(&String::from_utf8_lossy(&line), &mut out)?;
        }
    }

    if !pending.is_empty() {
        state.handle_line(&String::from_utf8_lossy(&pending), &mut out)?;
    }

    state.finish(&mut out)?;
    state.report_tps();
    Ok(())
}

struct StreamState {
    renderer: Renderer,
    generation_start: Option<Instant>,
    generation_end: Instant,
    fallback_tokens: u64,
    usage_tokens: Option<u64>,
}

impl StreamState {
    fn new() -> Self {
        Self::with_renderer(Renderer::stdout())
    }

    fn with_renderer(renderer: Renderer) -> Self {
        Self {
            renderer,
            generation_start: None,
            generation_end: Instant::now(),
            fallback_tokens: 0,
            usage_tokens: None,
        }
    }

    fn handle_line(&mut self, line: &str, out: &mut impl Write) -> Result<(), String> {
        let line = line.trim_end_matches('\n').trim_end_matches('\r');
        let Some(data) = line.strip_prefix("data:") else {
            return Ok(());
        };
        let data = data.trim_start();
        if data.is_empty() || data == "[DONE]" {
            return Ok(());
        }

        let event: StreamEvent =
            serde_json::from_str(data).map_err(|error| format!("bad stream event: {error}"))?;

        if let Some(error) = &event.error {
            let message = error.message.as_deref().unwrap_or("unknown error");
            return Err(format!("stream error: {message}"));
        }

        if let Some(usage) = &event.usage
            && usage.completion_tokens > 0
        {
            self.usage_tokens = Some(usage.completion_tokens);
        }

        let Some(choice) = event.choices.first() else {
            return Ok(());
        };
        let content = choice
            .delta
            .content
            .as_deref()
            .filter(|text| !text.is_empty());
        let reasoning = choice
            .delta
            .reasoning
            .as_deref()
            .filter(|text| !text.is_empty());

        if content.is_some() || reasoning.is_some() {
            let now = Instant::now();
            self.generation_start.get_or_insert(now);
            self.generation_end = now;
            self.fallback_tokens += 1;
        }

        if let Some(text) = content {
            self.renderer
                .write(text, out)
                .map_err(|error| format!("failed to write output: {error}"))?;
        }
        Ok(())
    }

    fn finish(&mut self, out: &mut impl Write) -> Result<(), String> {
        self.renderer
            .finish(out)
            .map_err(|error| format!("failed to write output: {error}"))
    }

    fn report_tps(&self) {
        let Some(start) = self.generation_start else {
            return;
        };
        let seconds = self
            .generation_end
            .saturating_duration_since(start)
            .as_secs_f64();
        let tokens = self
            .usage_tokens
            .or((self.fallback_tokens > 0).then_some(self.fallback_tokens));
        if let Some(tokens) = tokens
            && seconds > 0.0
        {
            print_tps(tokens as f64 / seconds, self.usage_tokens.is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_effort_case_insensitively() {
        assert_eq!(ThinkingEffort::parse(" HIGH "), Some(ThinkingEffort::High));
        assert_eq!(
            ThinkingEffort::parse("Medium"),
            Some(ThinkingEffort::Medium)
        );
        assert_eq!(ThinkingEffort::parse("extreme"), None);
    }

    #[test]
    fn trims_blank_values() {
        assert_eq!(non_empty("   "), None);
        assert_eq!(non_empty("  value \n"), Some("value"));
    }

    #[test]
    fn config_falls_back_to_defaults() {
        let config = Config::default();
        assert_eq!(config.model(), DEFAULT_MODEL);
        assert_eq!(config.effort(), ThinkingEffort::Low);
        assert_eq!(config.api_key(), None);
    }

    #[test]
    fn parses_stream_events() {
        let mut state = StreamState::with_renderer(Renderer::plain());
        let mut out = Vec::new();
        state
            .handle_line(
                r#"data: {"choices":[{"delta":{"content":"hel"}}]}"#,
                &mut out,
            )
            .unwrap();
        state
            .handle_line(
                r#"data: {"choices":[{"delta":{"content":"lo\n"}}]}"#,
                &mut out,
            )
            .unwrap();
        state.handle_line("", &mut out).unwrap();
        state.handle_line("data: [DONE]", &mut out).unwrap();
        state.finish(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "hello\n");
    }

    #[test]
    fn surfaces_stream_errors() {
        let mut state = StreamState::with_renderer(Renderer::plain());
        let mut out = Vec::new();
        let error = state
            .handle_line(r#"data: {"error":{"message":"nope"}}"#, &mut out)
            .unwrap_err();
        assert_eq!(error, "stream error: nope");
    }
}
