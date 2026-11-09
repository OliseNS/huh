# idk

Ask a question from your terminal and get a short, streaming answer. `idk` talks to
[OpenRouter](https://openrouter.ai) using the OpenAI-compatible chat completions API.

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
```

### Commands

| Command                      | Description                              |
| ---------------------------- | ---------------------------------------- |
| `--login`                    | Save your OpenRouter API key             |
| `--logout`                   | Remove the saved API key                 |
| `--model`                    | Show the current model                   |
| `--model <name>`             | Set the model                            |
| `--effort`                   | Show the current reasoning effort        |
| `--effort <low\|medium\|high>` | Set reasoning effort                   |
| `--config`                   | Show the effective configuration         |
| `--config-path`              | Print the config file path               |
| `-h`, `--help`               | Show help                                |

The default model is `openai/gpt-oss-20b:nitro`.

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
