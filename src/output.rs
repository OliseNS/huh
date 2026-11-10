use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use futures_util::StreamExt;

use crate::api::{StreamUpdate, parse_stream_line};
use crate::render::{DIM, GREEN, RESET, Renderer, color_allowed};

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_INTERVAL: Duration = Duration::from_millis(80);

pub(crate) struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    pub(crate) fn start() -> Self {
        let mut spinner = Self::inactive();
        if !io::stderr().is_terminal() {
            return spinner;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let color = color_allowed();
        let handle = thread::spawn(move || {
            let mut frame = 0;
            while !thread_stop.load(Ordering::Relaxed) {
                let glyph = SPINNER_FRAMES[frame % SPINNER_FRAMES.len()];
                let mut stderr = io::stderr().lock();
                let _ = if color {
                    write!(stderr, "\r{DIM}{glyph}{RESET} thinking…")
                } else {
                    write!(stderr, "\r{glyph} thinking…")
                };
                let _ = stderr.flush();
                drop(stderr);
                frame += 1;
                thread::sleep(SPINNER_INTERVAL);
            }
        });

        spinner.stop = stop;
        spinner.handle = Some(handle);
        spinner
    }

    fn inactive() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    fn stop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        self.stop.store(true, Ordering::Relaxed);
        let _ = handle.join();
        let mut stderr = io::stderr().lock();
        let _ = write!(stderr, "\r\x1b[K");
        let _ = stderr.flush();
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) async fn stream_answer(
    response: reqwest::Response,
    spinner: Spinner,
    request_start: Instant,
) -> Result<String, String> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut state = StreamState::new(Renderer::stdout());
    state.spinner = spinner;
    let mut stream = response.bytes_stream();
    let mut pending: Vec<u8> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("stream failed: {error}"))?;
        pending.extend_from_slice(&chunk);

        let mut start = 0;
        while let Some(offset) = pending[start..].iter().position(|&byte| byte == b'\n') {
            let end = start + offset;
            state.handle_line(&String::from_utf8_lossy(&pending[start..end]), &mut out)?;
            start = end + 1;
        }
        if start > 0 {
            pending.drain(..start);
        }
    }

    if !pending.is_empty() {
        state.handle_line(&String::from_utf8_lossy(&pending), &mut out)?;
    }

    let elapsed = request_start.elapsed();
    state.finish(&mut out)?;
    state.report_tps(elapsed);
    Ok(state.answer)
}

struct StreamState {
    renderer: Renderer,
    spinner: Spinner,
    fallback_chars: u64,
    usage_tokens: Option<u64>,
    answer: String,
}

impl StreamState {
    fn new(renderer: Renderer) -> Self {
        Self {
            renderer,
            spinner: Spinner::inactive(),
            fallback_chars: 0,
            usage_tokens: None,
            answer: String::new(),
        }
    }

    fn handle_line(&mut self, line: &str, out: &mut impl Write) -> Result<(), String> {
        match parse_stream_line(line)? {
            StreamUpdate::Ignore => Ok(()),
            StreamUpdate::Error(message) => Err(format!("stream error: {message}")),
            StreamUpdate::Delta {
                content,
                reasoning,
                tokens,
            } => {
                if let Some(tokens) = tokens {
                    self.usage_tokens = Some(tokens);
                }

                let generated_chars = content.as_ref().map_or(0, |text| text.chars().count())
                    + reasoning.as_ref().map_or(0, |text| text.chars().count());

                if generated_chars > 0 {
                    self.spinner.stop();
                    self.fallback_chars += generated_chars as u64;
                }

                if let Some(text) = content {
                    self.answer.push_str(&text);
                    self.renderer
                        .write(&text, out)
                        .map_err(|error| format!("failed to write output: {error}"))?;
                }
                Ok(())
            }
        }
    }

    fn finish(&mut self, out: &mut impl Write) -> Result<(), String> {
        self.spinner.stop();
        self.renderer
            .finish(out)
            .map_err(|error| format!("failed to write output: {error}"))
    }

    fn report_tps(&self, elapsed: Duration) {
        if !io::stderr().is_terminal() {
            return;
        }

        let (tokens, estimated) = match self.usage_tokens {
            Some(tokens) => (tokens as f64, false),
            None if self.fallback_chars > 0 => ((self.fallback_chars as f64 / 4.0).ceil(), true),
            None => return,
        };

        let Some(message) = tps_message(tokens, elapsed, estimated) else {
            return;
        };

        let color = color_allowed();
        if color {
            eprintln!("{GREEN}{message}{RESET}");
        } else {
            eprintln!("{message}");
        }
    }
}

fn tps_message(tokens: f64, elapsed: Duration, estimated: bool) -> Option<String> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }

    let tps = tokens / seconds;
    Some(if estimated {
        format!("tps: {tps:.1} (estimated)")
    } else {
        format!("tps: {tps:.1}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stream_events() {
        let mut state = StreamState::new(Renderer::plain());
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
        assert_eq!(state.answer, "hello\n");
    }

    #[test]
    fn surfaces_stream_errors() {
        let mut state = StreamState::new(Renderer::plain());
        let mut out = Vec::new();
        let error = state
            .handle_line(r#"data: {"error":{"message":"nope"}}"#, &mut out)
            .unwrap_err();
        assert_eq!(error, "stream error: nope");
    }

    #[test]
    fn reports_end_to_end_throughput() {
        assert_eq!(
            tps_message(100.0, Duration::from_secs(2), false).as_deref(),
            Some("tps: 50.0")
        );
        assert_eq!(
            tps_message(100.0, Duration::from_secs(2), true).as_deref(),
            Some("tps: 50.0 (estimated)")
        );
        assert_eq!(tps_message(100.0, Duration::ZERO, false), None);
    }
}
