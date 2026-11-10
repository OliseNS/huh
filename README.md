<p align="center">
  <img src="assets/huh-logo.svg" alt="huh!" width="280">
</p>

Ask a question from your terminal and get a short, streaming answer. `huh` talks to
[OpenRouter](https://openrouter.ai) using the OpenAI-compatible chat completions API.

Answers stream as they arrive. When stdout is a terminal, Markdown is rendered with
colored headings, lists, code blocks, links, inline code, and aligned tables that wrap
to the terminal width; piped output stays raw Markdown. Set `NO_COLOR=1` to disable
colors. While waiting for the
first visible output, a spinner is shown on stderr when it is a terminal, and end-to-end
token throughput is reported there too when stderr is a terminal.

Web search is on by default: the model decides when a question needs current
information and searches only then. `-s` forces a search; `-ns` disables it.

![huh answering a Git question in the terminal](assets/huh-demo.png)

The interactive file picker, with path autocomplete:

![huh file picker with autocomplete](assets/huh-picker.png)

## Requirements

- An [OpenRouter API key](https://openrouter.ai/keys)
- A Rust toolchain (install via [rustup](https://rustup.rs)) only if building from source

## Install

```sh
curl -LsSf https://olisemeka.dev/huh/install.sh | sh
```

The installer downloads the latest release for Linux (x86_64 or aarch64) or macOS
(Intel or Apple silicon), verifies its SHA-256 checksum, and installs to
`~/.local/bin/huh`. Set `HUH_INSTALL_DIR` to choose another directory, or
`HUH_VERSION=v0.1.0` to pin a release.

Windows (PowerShell):

```powershell
irm https://olisemeka.dev/huh/install.ps1 | iex
```

The PowerShell installer installs to `%USERPROFILE%\.local\bin` and adds that directory
to your user PATH.

![Installing huh with the curl installer](assets/huh-install.png)

### From source

Build a release binary, then install it with the local script:

```sh
cargo build --release --locked
./.install.sh
```

`./.install.sh` does not need a Rust toolchain; it only copies a prebuilt binary. It
installs to `~/.local/bin/huh`. Override the binary with `HUH_BINARY` and the
destination with `BIN_DIR`:

```sh
HUH_BINARY=/path/to/huh BIN_DIR="$HOME/.cargo/bin" ./.install.sh
```

Alternatively, install straight from the source tree with Cargo:

```sh
cargo install --path . --locked
```

## Update

If you installed with the curl script, re-run it to install the latest release:

```sh
curl -LsSf https://olisemeka.dev/huh/install.sh | sh
```

For source installs, `huh -u` rebuilds and reinstalls the running binary:

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
with private permissions. `--login` writes the API key there; the file can also hold a
default model and search mode:

```json
{
  "api_key": "sk-...",
  "model": "openai/gpt-oss-120b:nitro",
  "search": "auto"
}
```

`search` accepts `auto`, `on`, or `off`. `-m`, `HUH_MODEL`, `-s`, and `-ns` override
the matching config values. Older configs still load, and unknown fields are dropped on
the next save.

## Usage

```sh
huh why is the sky blue
huh how do I create a new Git repo and push it to GitHub
echo "explain closures in Rust" | huh
huh -- what does -v mean
huh -ns what does this Rust error mean
```

Options must come before the prompt; the remaining words are joined into the question.
Quotes are optional for ordinary words. Use them when the question contains shell
characters such as `?`, `*`, `|`, `$`, or `(`, and use `--` when the question starts
with `-`.

Plain `huh` starts a fresh conversation and replaces the saved session. Use `-c` (or
`--continue`) to follow up on the previous conversation:

```sh
huh explain Rust ownership
huh -c now show a small example
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
huh -m openai/gpt-5-mini summarize this diff
```

## Examples

Ask for a command when you only remember the shape of it:

```sh
huh how do I create a new Git repo and push it to GitHub
huh git: undo the last commit but keep the changes
huh how do I rebase onto main and resolve conflicts
huh what is the rsync command to copy a folder but skip node_modules
```

![huh creating a Git repository and pushing it to GitHub](assets/huh-git.png)

Explain code, errors, or logs without leaving the shell:

```sh
huh -f src/main.rs
huh -f build.log why did this build fail
huh -f Cargo.toml -f src/main.rs review these files for problems
```

Look something up when the answer needs to be current, or turn search off when it does
not:

```sh
huh -s what changed in the latest Rust release
huh -ns difference between TCP and UDP
huh compare Postgres and SQLite for a small local app
```

![huh using web search for the latest Rust release](assets/huh-search.png)

Keep a thread going:

```sh
huh explain Rust ownership
huh -c now show a small example
```

Use a stronger model for one question:

```sh
huh -m openai/gpt-5-mini review this architecture and list the risks
```

## Attaching files

Named files are read and attached to the prompt. Repeat `-f` for multiple files:

```sh
huh -f src/main.rs -f Cargo.toml review these files
```

Omit the path to pick a file interactively:

```sh
huh -f
```

Type to filter, Tab completes, and Enter opens a directory or selects a file. After
picking, huh asks for your question; press Enter to accept the default, "Explain this
file." (or "Explain these files."). The same question prompt appears for
`huh -f <path>` when no question is given. Interactive prompts need a terminal; piped
invocations must pass a path and the question.

![huh asking for a question after picking a file](assets/huh-question.png)

Only files you name explicitly are read. Files are limited to 256 KiB each and 512 KiB
total. Piped input is read up to 16 MiB and truncated to the last 256 KiB with a marker
in the prompt, so long logs keep their final errors. Attached file contents are part of
the saved conversation, so `-c` can refer back to them.

## Options and commands

| Option / command                   | Description                                       |
| ---------------------------------- | ------------------------------------------------- |
| `-c`, `--continue`                 | Continue the previous conversation                |
| `-f`, `--file [path]`              | Attach a file; omit path to pick interactively    |
| `-s`, `--search`                   | Force a web search                                |
| `-ns`, `--no-search`               | Disable web search for this request               |
| `-m`, `--model <name>`             | Model for this request                            |
| `-u`, `--update`                   | Rebuild and install from source                   |
| `--login`                          | Save your OpenRouter API key                      |
| `--logout`                         | Remove the saved API key                          |
| `--config`                         | Show configuration and defaults                   |
| `-V`, `--version`                  | Show version                                      |
| `-h`, `--help`                     | Show help                                         |

The default model is `openai/gpt-oss-120b:nitro`. Override it for one request with `-m`
(for example, `-m openai/gpt-5-mini` for harder reasoning), or for a shell session with
`HUH_MODEL`.

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
