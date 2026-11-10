# huh

Ask a question from your terminal and get a short, streaming answer. `huh` talks to
[OpenRouter](https://openrouter.ai) using the OpenAI-compatible chat completions API.

Answers stream as they arrive. When stdout is a terminal, Markdown is rendered with
styled headings, lists, code blocks, links, and inline code; piped output stays raw
Markdown. Set `NO_COLOR=1` to disable colors. While waiting for the first token, a
spinner is shown on stderr when it is a terminal, and token throughput is reported
there too when stderr is a terminal.

Web search is on by default: the model decides when a question needs current
information and searches only then. Pass `--no-search` to skip search for a request.

## Requirements

- A Rust toolchain (install via [rustup](https://rustup.rs))
- An [OpenRouter API key](https://openrouter.ai/keys)

## Install

From a clone of this repository:

```sh
./.install.sh
```

The install script builds a release binary and copies it to `~/.local/bin/huh`.
Override the destination with `BIN_DIR`:

```sh
BIN_DIR="$HOME/.cargo/bin" ./.install.sh
```

Alternatively, install straight from the source tree with Cargo:

```sh
cargo install --path . --locked
```

## Setup

Save your API key once:

```sh
huh --login
```

Or provide it through the environment (this overrides the saved key):

```sh
export OPENROUTER_API_KEY="sk-..."
```

The config lives at `${XDG_CONFIG_HOME:-$HOME/.config}/huh/config.json` and is created
with private permissions.

## Usage

```sh
huh "why is the sky blue?"
echo "explain closures in Rust" | huh
huh -- "what does -v mean?"
huh --no-search "what does this Rust error mean?"
```

Plain `huh` starts a fresh conversation and replaces the saved session. Use `-c` (or
`--continue`) to follow up on the previous conversation:

```sh
huh "explain Rust ownership"
huh -c "now show a small example"
```

The session lives at `${XDG_STATE_HOME:-$HOME/.local/state}/huh/session.json`. `huh -c`
loads the stored messages, appends the new question, and saves the answer for the next
follow-up. History is capped at 5 question-and-answer pairs (10 messages) and roughly
40,000 characters; the oldest pairs are dropped first.

Web search uses [OpenRouter's web search server
tool](https://openrouter.ai/docs/guides/features/server-tools/web-search), which adds a
per-search fee on top of model usage. Results are cited as Markdown links.

Per-request flags override the saved configuration:

```sh
huh -m openai/gpt-5-mini "summarize this diff"
huh -e low "what is a monad?"
```

### Options and commands

| Option / command                | Description                              |
| ------------------------------- | ---------------------------------------- |
| `-c`, `--continue`              | Continue the previous conversation       |
| `--no-search`                   | Disable web search for this request      |
| `-m`, `--model <name>`          | Model for this request                   |
| `-e`, `--effort <low\|medium\|high>` | Reasoning effort for this request   |
| `--login`                       | Save your OpenRouter API key             |
| `--logout`                      | Remove the saved API key                 |
| `--set-model <name>`            | Save the default model                   |
| `--set-effort <low\|medium\|high>` | Save the default reasoning effort     |
| `--config`                      | Show the effective configuration         |
| `--config-path`                 | Print the config file path               |
| `-h`, `--help`                  | Show help                                |

The default model is `openai/gpt-oss-20b:nitro`. Reasoning effort is sent only when
configured with `--set-effort` or `-e`; otherwise the model's default is used.

Requests use a 10s connect timeout and a 120s read timeout. Transient failures
(429 and 5xx responses) are retried up to three times (four attempts total) with
exponential backoff.

## Development

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## Uninstall

```sh
rm "$HOME/.local/bin/huh"
rm -r "${XDG_CONFIG_HOME:-$HOME/.config}/huh"
rm -r "${XDG_STATE_HOME:-$HOME/.local/state}/huh"
```
