use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO_URL: &str = "https://github.com/OliseNS/huh.git";

pub(crate) fn run() -> Result<(), String> {
    let source = source_dir()?;
    println!("Building {}", source.display());

    let status = Command::new("cargo")
        .current_dir(&source)
        .args(["build", "--release", "--locked"])
        .status()
        .map_err(|error| format!("failed to run cargo: {error}"))?;
    if !status.success() {
        return Err("cargo build failed".to_string());
    }

    let built = binary_path(&source);
    let current = env::current_exe()
        .map_err(|error| format!("failed to find the current executable: {error}"))?;
    if built == current {
        println!("Already running the freshly built binary");
        return Ok(());
    }

    replace(&built, &current)?;
    println!("Updated {}", current.display());
    Ok(())
}

fn source_dir() -> Result<PathBuf, String> {
    let compiled = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if compiled.join("Cargo.toml").is_file() {
        return Ok(compiled);
    }

    let cwd = env::current_dir()
        .map_err(|error| format!("failed to read the current directory: {error}"))?;
    if cwd.join("Cargo.toml").is_file() && cwd.join("src").join("main.rs").is_file() {
        return Ok(cwd);
    }

    Err(format!(
        "source tree not found; reinstall with:\n  curl -LsSf https://olisemeka.dev/huh/install.sh | sh\nor:\n  cargo install --git {REPO_URL} --locked --force"
    ))
}

fn binary_path(source: &Path) -> PathBuf {
    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                source.join(path)
            }
        })
        .unwrap_or_else(|| source.join("target"));
    target.join("release").join(exe_name())
}

fn replace(built: &Path, current: &Path) -> Result<(), String> {
    let dir = current.parent().ok_or_else(|| {
        format!(
            "cannot find the install directory for {}",
            current.display()
        )
    })?;
    let temp = dir.join(format!(".{}.{}.tmp", exe_name(), std::process::id()));

    fs::copy(built, &temp)
        .map_err(|error| format!("failed to copy {}: {error}", built.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("failed to set permissions on {}: {error}", temp.display()))?;
    }

    fs::rename(&temp, current).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("failed to replace {}: {error}", current.display())
    })
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "huh.exe" } else { "huh" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_source_tree() {
        let source = source_dir().unwrap();
        assert!(source.join("Cargo.toml").is_file());
        assert!(source.join("src").join("main.rs").is_file());
    }
}
