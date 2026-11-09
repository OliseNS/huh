use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThinkingEffort {
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

    fn save(&self) -> Result<(), String> {
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
            .map_err(|error| format!("failed to save {}: {error}", path.display()))
    }
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
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

fn configured_api_key(config: &Config) -> Option<String> {
    config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| key.to_string())
}

fn configured_model(config: &Config) -> String {
    config
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or(DEFAULT_MODEL)
        .to_string()
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

fn usage_error(program: &str) -> Result<ExitCode, String> {
    eprintln!("USAGE ERR: {program} <prompt>  (or pipe prompt on stdin)");
    Ok(ExitCode::FAILURE)
}

fn ensure_no_args(args: &[String], option: &str) -> Result<(), String> {
    if args.len() > 1 {
        Err(format!("{option} takes no arguments"))
    } else {
        Ok(())
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
    let key = key.trim();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let mut config = Config::load()?;
    config.api_key = Some(key.to_string());
    config.save()?;

    println!("Saved API key to {}", config_path()?.display());
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
        None => println!("{}", configured_model(&config)),
        Some(value) => {
            let value = value.trim();
            if value.is_empty() {
                return Err("model name cannot be empty".to_string());
            }
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
        None => println!("{}", config.effort.unwrap_or(ThinkingEffort::Low)),
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
    let model = configured_model(&config);
    let effort = config.effort.unwrap_or(ThinkingEffort::Low);
    let api_key = if env_api_key().is_some() {
        "set (OPENROUTER_API_KEY environment variable)".to_string()
    } else if configured_api_key(&config).is_some() {
        "set (saved in config)".to_string()
    } else {
        "not set (run `idk --login`)".to_string()
    };

    println!("config:   {}", path.display());
    println!("model:    {model}");
    println!("effort:   {effort}");
    println!("api key:  {api_key}");
    println!("base url: {BASE_URL}");
    Ok(())
}

async fn dispatch() -> Result<ExitCode, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let program = env::args().next().unwrap_or_else(|| "idk".to_string());

    if args.is_empty() {
        if !io::stdin().is_terminal() {
            let prompt = read_stdin_prompt()?;
            if prompt.is_empty() {
                return usage_error(&program);
            }
            run_prompt(prompt).await?;
            return Ok(ExitCode::SUCCESS);
        }

        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    }

    match args[0].as_str() {
        "-h" | "--help" => {
            ensure_no_args(&args, "--help")?;
            print!("{HELP}");
        }
        "--login" => {
            ensure_no_args(&args, "--login")?;
            login()?;
        }
        "--logout" => {
            ensure_no_args(&args, "--logout")?;
            logout()?;
        }
        "--config" => {
            ensure_no_args(&args, "--config")?;
            show_config()?;
        }
        "--config-path" => {
            ensure_no_args(&args, "--config-path")?;
            println!("{}", config_path()?.display());
        }
        "--model" => {
            set_or_show_model(single_value(&args[1..], "--model")?)?;
        }
        "--effort" => {
            set_or_show_effort(single_value(&args[1..], "--effort")?)?;
        }
        "--" => {
            let prompt = args[1..].join(" ");
            if prompt.trim().is_empty() {
                return usage_error(&program);
            }
            run_prompt(prompt).await?;
        }
        option if option.starts_with('-') => {
            return Err(format!(
                "unknown option: {option}\nRun `idk --help` for usage."
            ));
        }
        _ => {
            run_prompt(args.join(" ")).await?;
        }
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

async fn run_prompt(prompt: String) -> Result<(), String> {
    let config = Config::load()?;
    let api_key = env_api_key()
        .or_else(|| configured_api_key(&config))
        .ok_or_else(|| "no API key found; run `idk --login`".to_string())?;
    let model = configured_model(&config);
    let effort = config.effort.unwrap_or(ThinkingEffort::Low);

    let response = reqwest::Client::new()
        .post(format!("{BASE_URL}/chat/completions"))
        .bearer_auth(api_key)
        .json(&json!({
            "model": model,
            "stream": true,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": prompt },
            ],
            "reasoning": { "effort": effort.as_str() },
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

    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    let mut answer = String::new();
    let mut generation_start: Option<Instant> = None;
    let mut generation_end = Instant::now();
    let mut fallback_tokens: u64 = 0;
    let mut usage_tokens: Option<u64> = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("stream failed: {error}"))?;
        pending.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(newline) = pending.find('\n') {
            let line = pending[..newline].trim_end_matches('\r').to_string();
            pending.drain(..=newline);

            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            let event: Value =
                serde_json::from_str(data).map_err(|error| format!("bad stream event: {error}"))?;

            if let Some(error) = event.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(format!("stream error: {message}"));
            }

            if let Some(tokens) = event["usage"]["completion_tokens"].as_u64()
                && tokens > 0
            {
                usage_tokens = Some(tokens);
            }

            let delta = &event["choices"][0]["delta"];
            let content = delta["content"].as_str().filter(|text| !text.is_empty());
            let reasoning = delta["reasoning"].as_str().filter(|text| !text.is_empty());

            if content.is_some() || reasoning.is_some() {
                let now = Instant::now();
                generation_start.get_or_insert(now);
                generation_end = now;
                fallback_tokens += 1;
            }

            if let Some(text) = content {
                answer.push_str(text);
            }
        }
    }

    if let Some(start) = generation_start {
        let seconds = generation_end
            .saturating_duration_since(start)
            .as_secs_f64();
        if let Some(tokens) =
            usage_tokens.or_else(|| (fallback_tokens > 0).then_some(fallback_tokens))
            && seconds > 0.0
        {
            let tps = tokens as f64 / seconds;
            if usage_tokens.is_some() {
                eprintln!("tps: {tps:.1}");
            } else {
                eprintln!("tps: {tps:.1} (estimated)");
            }
        }
    }

    print!("{answer}");
    if !answer.ends_with('\n') {
        println!();
    }
    Ok(())
}
