use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::storage::{load_json, save_json, xdg_path};
use crate::util::{env_value, non_empty, read_secret};

pub(crate) const DEFAULT_MODEL: &str = "openai/gpt-oss-20b:nitro";
pub(crate) const DEFAULT_WHY_MODEL: &str = "openai/gpt-5-mini";

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    why_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<bool>,
}

impl Config {
    pub(crate) fn load() -> Result<Self, String> {
        Ok(load_json(&config_path()?)
            .map_err(crate::storage::LoadError::into_message)?
            .unwrap_or_default())
    }

    fn save(&self) -> Result<PathBuf, String> {
        let path = config_path()?;
        save_json(&path, self, "config")?;
        Ok(path)
    }

    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().and_then(non_empty)
    }

    pub(crate) fn model(&self) -> &str {
        self.model
            .as_deref()
            .and_then(non_empty)
            .unwrap_or(DEFAULT_MODEL)
    }

    pub(crate) fn why_model(&self) -> &str {
        self.why_model
            .as_deref()
            .and_then(non_empty)
            .unwrap_or(DEFAULT_WHY_MODEL)
    }

    pub(crate) fn sound_enabled(&self) -> bool {
        self.sound.unwrap_or(true)
    }
}

pub(crate) fn config_path() -> Result<PathBuf, String> {
    xdg_path("XDG_CONFIG_HOME", &[".config"], "config.json")
}

pub(crate) fn env_api_key() -> Option<String> {
    env_value("OPENROUTER_API_KEY")
}

pub(crate) fn resolved_api_key(config: &Config) -> Option<String> {
    env_api_key().or_else(|| config.api_key().map(str::to_owned))
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

pub(crate) fn set_model(value: &str) -> Result<(), String> {
    let value = non_empty(value).ok_or_else(|| "model name cannot be empty".to_string())?;
    update_config(|config| config.model = Some(value.to_string()))?;
    println!("default model set to {value}");
    Ok(())
}

pub(crate) fn set_why_model(value: &str) -> Result<(), String> {
    let value = non_empty(value).ok_or_else(|| "model name cannot be empty".to_string())?;
    update_config(|config| config.why_model = Some(value.to_string()))?;
    println!("default --why model set to {value}");
    Ok(())
}

pub(crate) fn set_sound(value: &str) -> Result<(), String> {
    let enabled = parse_sound(value)
        .ok_or_else(|| format!("invalid sound setting: {value} (use on or off)"))?;
    update_config(|config| config.sound = Some(enabled))?;
    println!("sound {}", if enabled { "on" } else { "off" });
    Ok(())
}

fn parse_sound(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Some(true),
        "off" | "false" | "no" | "0" => Some(false),
        _ => None,
    }
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
    println!("model:     {}", config.model());
    println!("why model: {}", config.why_model());
    println!(
        "sound:     {}",
        if config.sound_enabled() { "on" } else { "off" }
    );
    println!("api key:   {api_key}");
    println!("base url:  {base_url}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_falls_back_to_defaults() {
        let config = Config::default();
        assert_eq!(config.model(), DEFAULT_MODEL);
        assert_eq!(config.why_model(), DEFAULT_WHY_MODEL);
        assert!(config.sound_enabled());
        assert_eq!(config.api_key(), None);
    }

    #[test]
    fn ignores_legacy_effort_field() {
        let config: Config =
            serde_json::from_str(r#"{"model":"vendor/model","effort":"high"}"#).unwrap();
        assert_eq!(config.model(), "vendor/model");
        assert!(!serde_json::to_string(&config).unwrap().contains("effort"));
    }

    #[test]
    fn reads_sound_setting() {
        let config: Config = serde_json::from_str(r#"{"sound":false}"#).unwrap();
        assert!(!config.sound_enabled());
    }

    #[test]
    fn parses_sound_values() {
        assert_eq!(parse_sound("on"), Some(true));
        assert_eq!(parse_sound(" OFF "), Some(false));
        assert_eq!(parse_sound("maybe"), None);
    }
}
