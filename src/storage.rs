use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

pub(crate) enum LoadError {
    Read(String),
    Parse(String),
}

pub(crate) fn backup_invalid(path: &Path) {
    let backup = path.with_extension("json.bad");
    if let Err(error) = fs::rename(path, &backup) {
        eprintln!(
            "warning: could not move invalid file to {}: {error}",
            backup.display()
        );
    }
}

pub(crate) fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, LoadError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| {
            LoadError::Parse(format!("failed to parse {}: {error}", path.display()))
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(LoadError::Read(format!(
            "failed to read {}: {error}",
            path.display()
        ))),
    }
}

pub(crate) fn save_json<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
    label: &str,
) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
        set_private_dir(dir)?;
    }

    let contents = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize {label}: {error}"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("data");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_file_name(format!("{file_name}.{}.{nanos}.tmp", std::process::id()));
    write_private(&temp, contents.as_bytes())?;
    fs::rename(&temp, path).map_err(|error| format!("failed to save {}: {error}", path.display()))
}

pub(crate) fn xdg_path(env_var: &str, fallback: &[&str], file: &str) -> Result<PathBuf, String> {
    if let Some(base) =
        env::var_os(env_var).filter(|value| !value.is_empty() && Path::new(value).is_absolute())
    {
        return Ok(PathBuf::from(base).join("huh").join(file));
    }

    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty() && Path::new(value).is_absolute())
        .ok_or_else(|| "HOME is not set to an absolute path".to_string())?;
    let mut path = PathBuf::from(home);
    path.extend(fallback);
    Ok(path.join("huh").join(file))
}

#[cfg(unix)]
fn set_private_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("failed to set permissions on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_private_dir(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("failed to set permissions on {}: {error}", path.display()))?;
    }

    file.write_all(bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}
