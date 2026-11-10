mod api;
mod cli;
mod config;
mod context;
mod output;
mod render;
mod session;
mod storage;
mod update;
mod util;

use std::env;
use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::time::Instant;

use cli::Options;
use config::Config;
use session::Message;

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
            "-V" | "--version" => {
                cli::no_extra_args(rest, "--version")?;
                println!("huh {}", env!("CARGO_PKG_VERSION"));
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
            "-u" | "--update" => {
                cli::no_extra_args(rest, "--update")?;
                update::run()?;
                return Ok(ExitCode::SUCCESS);
            }
            _ => {}
        }
    }

    let options = Options::parse(&args)?;
    let prompt = build_prompt(&options)?;
    if prompt.trim().is_empty() {
        eprintln!("USAGE: {program} <prompt>  (or pipe prompt on stdin)");
        return Ok(ExitCode::FAILURE);
    }

    run_request(options, prompt).await?;
    Ok(ExitCode::SUCCESS)
}

fn build_prompt(options: &Options) -> Result<String, String> {
    let question = (!options.prompt.is_empty()).then(|| options.prompt.join(" "));
    let files = context::load_files(&options.files)?;
    let question = match question {
        Some(question) => Some(question),
        None if !io::stdin().is_terminal() => Some(util::read_prompt_stdin()?),
        None => None,
    };
    Ok(context::compose_question(question.as_deref(), &files))
}

async fn run_request(options: Options, prompt: String) -> Result<(), String> {
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("warning: {error}");
            Config::default()
        }
    };
    let api_key = config::resolved_api_key(&config)
        .ok_or_else(|| "no API key found; run `huh --login`".to_string())?;
    let model = config::resolved_model(&config, options.model.as_deref());
    let search = config::resolved_search(&config, options.search);

    let mut history = if options.continue_session {
        let mut messages = session::load_session()?;
        if messages.is_empty() {
            eprintln!("warning: no previous session found; starting a new conversation");
        }
        session::trim_history(&mut messages);
        messages
    } else {
        Vec::new()
    };
    history.push(Message::user(prompt));

    let body = api::build_body(&model, &history, search);

    let client = api::client()?;
    let spinner = output::Spinner::start();
    let started = Instant::now();
    let response = api::post_chat(&client, &api_key, &body).await?;
    let answer = output::stream_answer(response, spinner, started).await?;
    if answer.trim().is_empty() {
        return Err("model returned no answer content".to_string());
    }

    history.push(Message::assistant(answer));
    session::trim_history(&mut history);
    if let Err(error) = session::save_session(&history) {
        eprintln!("warning: {error}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match dispatch().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ERROR: {}", render::sanitize(&error));
            ExitCode::FAILURE
        }
    }
}
