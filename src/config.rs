use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::storage::{load_or_backup, save_json, xdg_path};
use crate::util::{env_value, non_empty, read_secret};

pub(crate) const DEFAULT_MODEL: &str = "deepseek/deepseek-v4.1-flash-1017:nitro";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchMode {
    #[default]
    Auto,
    On,
    Off,
}

impl SearchMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    search: Option<SearchMode>,
}

impl Config {
    pub(crate) fn load() -> Result<Self, String> {
        let path = config_path()?;
        Ok(load_or_backup(&path, "config")?.unwrap_or_default())
    }

    fn save(&self) -> Result<PathBuf, String> {
        let path = config_path()?;
        save_json(&path, self, "config")?;
        Ok(path)
    }

    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().and_then(non_empty)
    }

    pub(crate) fn model(&self) -> Option<&str> {
        self.model.as_deref().and_then(non_empty)
    }

    pub(crate) fn search(&self) -> Option<SearchMode> {
        self.search
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

pub(crate) fn resolved_api_key(config: &Config) -> Option<String> {
    env_api_key().or_else(|| config.api_key().map(str::to_owned))
}

pub(crate) fn resolved_model(config: &Config, option: Option<&str>) -> String {
    option
        .map(str::to_owned)
        .or_else(env_model)
        .or_else(|| config.model().map(str::to_owned))
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

pub(crate) fn resolved_search(config: &Config, option: Option<SearchMode>) -> SearchMode {
    option.or_else(|| config.search()).unwrap_or_default()
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

    let model = resolved_model(&config, None);
    let search = resolved_search(&config, None);

    println!("config:    {}", path.display());
    println!("api key:   {api_key}");
    println!("base url:  {base_url}");
    println!("model:     {model}");
    println!("search:    {}", search.label());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_known_fields_and_drops_legacy_fields() {
        let config: Config = serde_json::from_str(
            r#"{"api_key":"sk-test","model":"vendor/model","search":"on","why_model":"old","sound":false}"#,
        )
        .unwrap();
        assert_eq!(config.api_key(), Some("sk-test"));
        assert_eq!(config.model(), Some("vendor/model"));
        assert_eq!(config.search(), Some(SearchMode::On));

        let serialized = serde_json::to_string(&config).unwrap();
        assert!(!serialized.contains("why_model"));
        assert!(!serialized.contains("sound"));
    }

    #[test]
    fn ignores_blank_api_key() {
        let config: Config = serde_json::from_str(r#"{"api_key":"  "}"#).unwrap();
        assert_eq!(config.api_key(), None);
    }
}
