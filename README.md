# huh

Ask a question from your terminal and get a short, streaming answer. `huh` talks to
[OpenRouter](https://openrouter.ai) using the OpenAI-compatible chat completions API.

Answers stream as they arrive. When stdout is a terminal, Markdown is rendered with
styled headings, lists, code blocks, links, and inline code; piped output stays raw
Markdown. Set `NO_COLOR=1` to disable colors. While waiting for the first token, a
spinner is shown on stderr when it is a terminal, and token throughput is reported
there too when stderr is a terminal.

Web search is on by default: the model decides when a question needs current
information and searches only then. `-s` forces a search; `-ns` disables it.

A short "huh" sound plays when an answer starts streaming on a terminal. `-q` mutes it
for one request, or save the setting with `huh --set-sound off`.

## Requirements

- An [OpenRouter API key](https://openrouter.ai/keys)
- A Rust toolchain (install via [rustup](https://rustup.rs)) only if building from source

## Install

Build a release binary, then run the install script:

```sh
cargo build --release --locked
./.install.sh
```

The install script only installs a prebuilt binary; it does not need a Rust toolchain.
It copies the binary to `~/.local/bin/huh`. Override the binary with `HUH_BINARY` and
the destination with `BIN_DIR`:

```sh
HUH_BINARY=/path/to/huh BIN_DIR="$HOME/.cargo/bin" ./.install.sh
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
with private permissions. It stores the API key, the default model, the `--why` model,
and the sound setting. Older configs with an `effort` field still load; unused fields
are dropped on the next save.

## Usage

```sh
huh "why is the sky blue?"
echo "explain closures in Rust" | huh
huh -- "what does -v mean?"
huh -ns "what does this Rust error mean?"
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

## Debugging a failed command

Add this function to your shell startup file (for example `~/.bashrc`). It is not
installed automatically:

```bash
huh-why() {
    local status=$?
    local command
    command=$(fc -ln -1)
    HUH_LAST_COMMAND="$command" HUH_LAST_EXIT_CODE="$status" huh -y "$@"
}
```

Then run `huh-why` after a command fails, optionally with a follow-up question:

```sh
huh-why
huh-why "why did the linker fail?"
```

Piping command output works too:

```sh
cargo build 2>&1 | huh -y
```

`-y` sends the previous command, its exit code, piped output, your extra words, and any
`--file` attachments. It does not read files, glob, or search on its own. Web search is
off for `-y` unless you pass `-s`. Debugging uses a stronger model than the default
(`openai/gpt-5-mini`); change it with `huh --set-why-model <name>`, or override it for
one request with `-m`.

## Attaching files

Named files are read and attached to the prompt. Repeat `-f` for multiple files:

```sh
huh -f src/main.rs -f Cargo.toml "review these files"
huh -y -f build.log "what went wrong?"
```

Only files you name explicitly are read.

## Sound

The bundled MP3 plays when the first answer token arrives, only when stdout is a
terminal. `-q` (or `--no-sound`) mutes one request. Save the setting permanently with:

```sh
huh --set-sound off
huh --set-sound on
```

## Options and commands

| Option / command                   | Description                              |
| ---------------------------------- | ---------------------------------------- |
| `-c`, `--continue`                 | Continue the previous conversation       |
| `-y`, `--why`                      | Explain a command failure                |
| `-f`, `--file <path>`              | Attach a file to the prompt (repeatable) |
| `-s`, `--search`                   | Force a web search                       |
| `-ns`, `--no-search`               | Disable web search for this request      |
| `-q`, `--no-sound`                 | Mute the answer sound effect             |
| `-m`, `--model <name>`             | Model for this request                   |
| `-e`, `--effort <low\|medium\|high>` | Reasoning effort for this request      |
| `--login`                          | Save your OpenRouter API key             |
| `--logout`                         | Remove the saved API key                 |
| `--set-model <name>`               | Save the default model                   |
| `--set-why-model <name>`           | Save the model used by `--why`           |
| `--set-sound <on\|off>`            | Enable or disable the answer sound       |
| `--config`                         | Show the effective configuration         |
| `--config-path`                    | Print the config file path               |
| `-h`, `--help`                     | Show help                                |

The default model is `openai/gpt-oss-20b:nitro`; `--why` defaults to
`openai/gpt-5-mini`. Reasoning effort is sent only when passed with `-e`; otherwise the
model's default is used.

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
rm "${BIN_DIR:-$HOME/.local/bin}/huh"
rm -r "${XDG_CONFIG_HOME:-$HOME/.config}/huh"
rm -r "${XDG_STATE_HOME:-$HOME/.local/state}/huh"
```
