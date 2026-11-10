use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::storage::{LoadError, backup_invalid, load_json, save_json, xdg_path};
use crate::util::{env_value, non_empty, read_secret};

pub(crate) const DEFAULT_MODEL: &str = "deepseek/deepseek-v4.1-flash-1017:nitro";

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

impl Config {
    pub(crate) fn load() -> Result<Self, String> {
        let path = config_path()?;
        match load_json(&path) {
            Ok(Some(config)) => Ok(config),
            Ok(None) => Ok(Self::default()),
            Err(LoadError::Parse(error)) => {
                eprintln!("warning: ignoring invalid config ({error})");
                backup_invalid(&path);
                Ok(Self::default())
            }
            Err(LoadError::Read(error)) => Err(error),
        }
    }

    fn save(&self) -> Result<PathBuf, String> {
        let path = config_path()?;
        save_json(&path, self, "config")?;
        Ok(path)
    }

    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().and_then(non_empty)
    }
}

pub(crate) fn config_path() -> Result<PathBuf, String> {
    xdg_path("XDG_CONFIG_HOME", &[".config"], "config.json")
}

pub(crate) fn env_api_key() -> Option<String> {
    env_value("OPENROUTER_API_KEY")
}

pub(crate) fn env_model() -> Option<String> {
    env_value("HUH_MODEL")
}

fn update_config(update: impl FnOnce(&mut Config)) -> Result<PathBuf, String> {
    let mut config = Config::load()?;
    update(&mut config);
    config.save()
}

pub(crate) fn login() -> Result<(), String> {
    let key = read_secret("OpenRouter API key: ")?;
    let key = non_empty(&key).ok_or_else(|| "API key cannot be empty".to_string())?;

    let path = update_config(|config| config.api_key = Some(key.to_string()))?;

    println!("Saved API key to {}", path.display());
    if env_api_key().is_some() {
        eprintln!("note: OPENROUTER_API_KEY is set and overrides the saved key");
    }
    Ok(())
}

pub(crate) fn logout() -> Result<(), String> {
    let mut config = Config::load()?;
    if config.api_key.take().is_some() {
        config.save()?;
        println!("Removed saved API key");
    } else {
        println!("No saved API key");
    }
    Ok(())
}

pub(crate) fn show_config(base_url: &str) -> Result<(), String> {
    let path = config_path()?;
    let config = Config::load()?;
    let api_key = if env_api_key().is_some() {
        "set (OPENROUTER_API_KEY environment variable)"
    } else if config.api_key().is_some() {
        "set (saved in config)"
    } else {
        "not set (run `huh --login`)"
    };

    println!("config:    {}", path.display());
    println!("api key:   {api_key}");
    println!("base url:  {base_url}");
    match env_model() {
        Some(model) => println!("model:     {model} (HUH_MODEL)"),
        None => println!("model:     {DEFAULT_MODEL} (default; override with -m or HUH_MODEL)"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_api_key_and_drops_legacy_fields() {
        let config: Config = serde_json::from_str(
            r#"{"api_key":"sk-test","model":"old","why_model":"old","sound":false}"#,
        )
        .unwrap();
        assert_eq!(config.api_key(), Some("sk-test"));
        assert!(!serde_json::to_string(&config).unwrap().contains("model"));
    }

    #[test]
    fn ignores_blank_api_key() {
        let config: Config = serde_json::from_str(r#"{"api_key":"  "}"#).unwrap();
        assert_eq!(config.api_key(), None);
    }
}
