# idk

Ask a question from your terminal and get a short, streaming answer. `idk` talks to
[OpenRouter](https://openrouter.ai) using the OpenAI-compatible chat completions API.

Answers stream as they arrive. When stdout is a terminal, Markdown is rendered with
styled headings, lists, code blocks, links, and inline code; piped output stays raw
Markdown. Set `NO_COLOR=1` to disable colors. While waiting for the first token, a
spinner is shown on stderr when it is a terminal, and token throughput is reported
there too when stderr is a terminal.

## Requirements

- A Rust toolchain (install via [rustup](https://rustup.rs))
- An [OpenRouter API key](https://openrouter.ai/keys)

## Install

```sh
git clone https://github.com/OliseNS/idk.git
cd idk
./.install.sh
```

The install script builds a release binary and copies it to `~/.local/bin/idk`.
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
idk --login
```

Or provide it through the environment (this overrides the saved key):

```sh
export OPENROUTER_API_KEY="sk-..."
```

The config lives at `${XDG_CONFIG_HOME:-$HOME/.config}/idk/config.json` and is created
with private permissions.

## Usage

```sh
idk "why is the sky blue?"
echo "explain closures in Rust" | idk
idk -- "what does -v mean?"
idk --search "what happened in the news today?"
```

`--search` (or `-s`) grounds the answer with [OpenRouter's web search
plugin](https://openrouter.ai/docs/features/web-search), which adds a per-request
search fee on top of model usage. Results are cited as Markdown links.

Per-request flags override the saved configuration:

```sh
idk -m openai/gpt-5-mini "summarize this diff"
idk -e low "what is a monad?"
```

### Commands

| Command                         | Description                              |
| ------------------------------- | ---------------------------------------- |
| `-s`, `--search`             | Ground the answer with web search        |
| `--login`                    | Save your OpenRouter API key             |
| `--logout`                      | Remove the saved API key                 |
| `--set-model <name>`            | Save the default model                   |
| `--set-effort <low\|medium\|high>` | Save the default reasoning effort     |
| `--config`                      | Show the effective configuration         |
| `--config-path`                 | Print the config file path               |
| `-h`, `--help`                  | Show help                                |

The default model is `openai/gpt-oss-20b:nitro`. Reasoning effort is sent only when
configured with `--set-effort` or `-e`; otherwise the model's default is used.

Requests use a 10s connect timeout and a 120s read timeout. Transient failures
(429 and 5xx responses) are retried up to four times with exponential backoff.

## Development

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## Uninstall

```sh
rm "$HOME/.local/bin/idk"
rm -r "${XDG_CONFIG_HOME:-$HOME/.config}/idk"
```
