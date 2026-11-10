# huh

Ask a question from your terminal and get a short, streaming answer. `huh` talks to
[OpenRouter](https://openrouter.ai) using the OpenAI-compatible chat completions API.

Answers stream as they arrive. When stdout is a terminal, Markdown is rendered with
styled headings, lists, code blocks, links, and inline code; piped output stays raw
Markdown. Set `NO_COLOR=1` to disable colors. While waiting for the first token, a
spinner is shown on stderr when it is a terminal, and end-to-end token throughput is
reported there too when stderr is a terminal.

Web search is on by default: the model decides when a question needs current
information and searches only then. `-s` forces a search; `-ns` disables it.

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

## Update

After changing the source, rebuild and reinstall the running binary:

```sh
huh -u
```

This requires a Rust toolchain and rebuilds from the source tree the binary was built
from. It does not run `git pull`; do that first to pick up remote changes. The build
happens first, so a failed build leaves the current binary untouched. If the source tree
has moved, reinstall from the new checkout with
`cargo build --release --locked && ./.install.sh`.

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
with private permissions. It stores only the API key; use `-m` to override the model
for a request. Older configs still load, and unused fields are dropped on the next save.

## Usage

```sh
huh "why is the sky blue?"
echo "explain closures in Rust" | huh
huh -- "what does -v mean?"
huh -ns "what does this Rust error mean?"
```

Options must come before the prompt; everything after the first prompt word is text. Use
`--` when the prompt itself starts with `-`.

Plain `huh` starts a fresh conversation and replaces the saved session. Use `-c` (or
`--continue`) to follow up on the previous conversation:

```sh
huh "explain Rust ownership"
huh -c "now show a small example"
```

The session lives at `${XDG_STATE_HOME:-$HOME/.local/state}/huh/session.json`. `huh -c`
loads the stored messages, appends the new question, and saves the answer for the next
follow-up. If no previous session exists, it warns and starts a new conversation. History
is capped at 10 question-and-answer pairs (20 messages) and roughly 80,000 characters.
The first pair is kept as the context anchor; the oldest middle pairs are dropped first.
If the first and latest pairs still do not fit, the anchor is dropped too, and the latest
pair is always kept.

Web search uses [OpenRouter's web search server
tool](https://openrouter.ai/docs/guides/features/server-tools/web-search), which adds a
per-search fee on top of model usage. Results are cited as Markdown links.

Per-request flags override the defaults:

```sh
huh -m openai/gpt-5-mini "summarize this diff"
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
`--file` attachments. It does not read files, glob, or search on its own. Without a
command, piped output, or file it exits with an error instead of calling the API. Web
search is off for `-y` unless you pass `-s`. Debugging uses the same model as normal
requests; override it for one request with `-m`. `-y` cannot be combined with `-c`, and
debugging requests are not saved to the conversation session.

## Attaching files

Named files are read and attached to the prompt. Repeat `-f` for multiple files:

```sh
huh -f src/main.rs -f Cargo.toml "review these files"
huh -y -f build.log "what went wrong?"
```

Only files you name explicitly are read. Files are limited to 256 KiB each and 512 KiB
total. Piped input is truncated to the last 256 KiB with a marker in the prompt, so long
logs keep their final errors. Attached file contents are part of the saved conversation,
so `-c` can refer back to them.

## Options and commands

| Option / command                   | Description                              |
| ---------------------------------- | ---------------------------------------- |
| `-c`, `--continue`                 | Continue the previous conversation       |
| `-y`, `--why`                      | Explain a command failure                |
| `-f`, `--file <path>`              | Attach a file to the prompt (repeatable) |
| `-s`, `--search`                   | Force a web search                       |
| `-ns`, `--no-search`               | Disable web search for this request      |
| `-m`, `--model <name>`             | Model for this request                   |
| `-u`, `--update`                   | Rebuild and reinstall from source        |
| `--login`                          | Save your OpenRouter API key             |
| `--logout`                         | Remove the saved API key                 |
| `--config`                         | Show configuration and defaults          |
| `-V`, `--version`                  | Show version                             |
| `-h`, `--help`                     | Show help                                |

The default model is `deepseek/deepseek-v4.1-flash-1017:nitro`, chosen for cheap, fast
answers. Override it for one request with `-m` (for example,
`-m openai/gpt-5-mini` for harder reasoning), or for a shell session with `HUH_MODEL`.

Requests use a 10s connect timeout and a 120s read timeout. Network failures, timeouts,
429, and 5xx responses are retried up to three times (four attempts total) with
exponential backoff; `Retry-After` is honored up to 30 seconds.

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
