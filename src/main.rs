mod api;
mod cli;
mod config;
mod context;
mod output;
mod render;
mod session;
mod sound;
mod storage;
mod util;

use std::env;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

use cli::{Options, SearchMode, ThinkingEffort};
use config::Config;
use session::Message;

struct Request {
    prompt: String,
    model: Option<String>,
    effort: Option<ThinkingEffort>,
    search: SearchMode,
    continue_session: bool,
    why: bool,
    no_sound: bool,
}

fn usage_error(program: &str) -> ExitCode {
    eprintln!("USAGE ERR: {program} <prompt>  (or pipe prompt on stdin)");
    ExitCode::FAILURE
}

async fn dispatch() -> Result<ExitCode, String> {
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "huh".to_string());
    let args: Vec<String> = args.collect();

    if args.is_empty() && io::stdin().is_terminal() {
        print!("{}", cli::help());
        return Ok(ExitCode::SUCCESS);
    }

    if let Some((command, rest)) = args.split_first() {
        match command.as_str() {
            "-h" | "--help" => {
                cli::no_extra_args(rest, "--help")?;
                print!("{}", cli::help());
                return Ok(ExitCode::SUCCESS);
            }
            "--login" => {
                cli::no_extra_args(rest, "--login")?;
                config::login()?;
                return Ok(ExitCode::SUCCESS);
            }
            "--logout" => {
                cli::no_extra_args(rest, "--logout")?;
                config::logout()?;
                return Ok(ExitCode::SUCCESS);
            }
            "--config" => {
                cli::no_extra_args(rest, "--config")?;
                config::show_config(api::BASE_URL)?;
                return Ok(ExitCode::SUCCESS);
            }
            "--config-path" => {
                cli::no_extra_args(rest, "--config-path")?;
                println!("{}", config::config_path()?.display());
                return Ok(ExitCode::SUCCESS);
            }
            "--set-model" => {
                config::set_model(cli::required_value(rest, "--set-model")?)?;
                return Ok(ExitCode::SUCCESS);
            }
            "--set-why-model" => {
                config::set_why_model(cli::required_value(rest, "--set-why-model")?)?;
                return Ok(ExitCode::SUCCESS);
            }
            "--set-sound" => {
                config::set_sound(cli::required_value(rest, "--set-sound")?)?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => {}
        }
    }

    let options = Options::parse(&args)?;
    let prompt = build_prompt(&options)?;
    if prompt.trim().is_empty() {
        if options.why {
            eprintln!("USAGE ERR: -y needs a command failure, piped output, a question, or --file");
            return Ok(ExitCode::FAILURE);
        }
        return Ok(usage_error(&program));
    }

    run_request(Request {
        prompt,
        model: options.model,
        effort: options.effort,
        search: options.search,
        continue_session: options.continue_session,
        why: options.why,
        no_sound: options.no_sound,
    })
    .await?;
    Ok(ExitCode::SUCCESS)
}

fn build_prompt(options: &Options) -> Result<String, String> {
    let question = if options.prompt.is_empty() {
        None
    } else {
        Some(options.prompt.join(" "))
    };
    let files = context::load_files(&options.files)?;

    if options.why {
        let mut why = context::WhyContext::from_env();
        if !io::stdin().is_terminal() {
            why.output = Some(util::read_stdin()?);
        }
        Ok(context::compose_why(&why, question.as_deref(), &files))
    } else {
        let question = match question {
            Some(question) => Some(question),
            None if !io::stdin().is_terminal() => Some(util::read_stdin()?),
            None => None,
        };
        Ok(context::compose_question(question.as_deref(), &files))
    }
}

async fn run_request(request: Request) -> Result<(), String> {
    let config = Config::load()?;
    let api_key = config::resolved_api_key(&config)
        .ok_or_else(|| "no API key found; run `huh --login`".to_string())?;
    let model = request.model.as_deref().unwrap_or_else(|| {
        if request.why {
            config.why_model()
        } else {
            config.model()
        }
    });
    let sound = sound::should_play(request.no_sound, config.sound_enabled());

    let mut history = if request.continue_session {
        let mut messages = session::load_session()?;
        session::trim_history(&mut messages);
        messages
    } else {
        Vec::new()
    };
    history.push(Message::user(request.prompt));

    let mode = if request.why {
        api::PromptMode::Why
    } else {
        api::PromptMode::Ask
    };
    let body = api::build_body(model, &history, request.effort, request.search, mode);

    let client = api::client()?;
    let spinner = output::Spinner::start();
    let response = api::post_chat(&client, &api_key, &body).await?;
    let answer = output::stream_answer(response, spinner, sound).await?;

    history.push(Message::assistant(answer));
    session::trim_history(&mut history);
    session::save_session(&history)
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
