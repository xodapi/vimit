use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use super::args::{FailOn, Preset};
use super::constants;
use super::theme::Theme;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    pub api_base: Option<String>,
    pub api_key_env: Option<String>,
    pub env_file: Option<String>,
    pub demo: Option<bool>,
    pub mock: Option<String>,
    pub monitor: Option<bool>,
    pub preset: Option<String>,
    pub theme: Option<String>,
    pub with_abtop: Option<bool>,
    pub notify: Option<bool>,
    pub watch: Option<u64>,
    pub fail_on: Option<String>,
    pub warning: Option<f64>,
    pub danger: Option<f64>,
    pub threshold: Option<String>,
    pub auto_failover: Option<bool>,
    pub cache_ttl_secs: Option<u64>,
}

impl Config {
    pub fn load(path: Option<&PathBuf>) -> Result<Self, String> {
        let config_path = match path {
            Some(p) => Some(p.clone()),
            None => default_config_path(),
        };

        let Some(config_path) = config_path else {
            return Ok(Config::default());
        };

        if !config_path.is_file() {
            if path.is_some() {
                return Err(format!(
                    "config file not found: {}\n  hint: run `vimit --init` to create one",
                    config_path.display()
                ));
            }
            return Ok(Config::default());
        }

        let raw = fs::read_to_string(&config_path)
            .map_err(|error| format!("cannot read config {}: {error}", config_path.display()))?;
        let config: Config = toml::from_str(&raw)
            .map_err(|error| format!("invalid config {}: {error}", config_path.display()))?;
        Ok(config)
    }

    pub fn theme(&self) -> Theme {
        self.theme
            .as_deref()
            .and_then(Theme::from_name)
            .unwrap_or(Theme::Btop)
    }

    pub fn fail_on(&self) -> FailOn {
        match self.fail_on.as_deref() {
            Some("warning") => FailOn::Warning,
            Some("danger") => FailOn::Danger,
            _ => FailOn::Never,
        }
    }

    pub fn warning(&self) -> f64 {
        self.warning.unwrap_or(constants::DEFAULT_WARNING_THRESHOLD)
    }

    pub fn danger(&self) -> f64 {
        self.danger.unwrap_or(constants::DEFAULT_DANGER_THRESHOLD)
    }

    pub fn window_thresholds(&self) -> Result<HashMap<String, (f64, f64)>, String> {
        match &self.threshold {
            Some(value) => super::args::parse_window_thresholds(value),
            None => Ok(HashMap::new()),
        }
    }

    pub fn preset(&self) -> Preset {
        match self.preset.as_deref() {
            Some("compact") => Preset::Compact,
            Some("mini") => Preset::Mini,
            _ => Preset::Full,
        }
    }

    pub fn merge_with_defaults(&self) -> Result<MergedConfig, String> {
        Ok(MergedConfig {
            api_base: self.api_base.clone().or_else(|| {
                std::env::var("VIBEMODE_API_BASE")
                    .ok()
                    .or_else(|| std::env::var("VIBEMOD_API_BASE").ok())
                    .or_else(|| std::env::var("NEUROGATE_API_BASE").ok())
            }),
            api_key_env: self
                .api_key_env
                .clone()
                .unwrap_or_else(|| constants::DEFAULT_API_KEY_ENV.to_string()),
            env_file: self.env_file.as_ref().map(PathBuf::from),
            demo: self.demo.unwrap_or(false),
            mock: self.mock.clone(),
            monitor: self.monitor.unwrap_or(false),
            preset: self.preset(),
            theme: self.theme(),
            with_abtop: self.with_abtop.unwrap_or(false),
            notify: self.notify.unwrap_or(false),
            watch: self.watch.unwrap_or(0),
            fail_on: self.fail_on(),
            warning_threshold: self.warning(),
            danger_threshold: self.danger(),
            window_thresholds: self.window_thresholds()?,
            auto_failover: self.auto_failover.unwrap_or(true),
            cache_ttl_secs: self
                .cache_ttl_secs
                .unwrap_or(super::cache::DEFAULT_TTL_SECS),
        })
    }
}

pub struct MergedConfig {
    pub api_base: Option<String>,
    pub api_key_env: String,
    pub env_file: Option<PathBuf>,
    pub demo: bool,
    pub mock: Option<String>,
    pub monitor: bool,
    pub preset: Preset,
    pub theme: Theme,
    pub with_abtop: bool,
    pub notify: bool,
    pub watch: u64,
    pub fail_on: FailOn,
    pub warning_threshold: f64,
    pub danger_threshold: f64,
    pub window_thresholds: HashMap<String, (f64, f64)>,
    pub auto_failover: bool,
    pub cache_ttl_secs: u64,
}

fn default_config_path() -> Option<PathBuf> {
    let home = dirs_or_default()?;
    let config_dir = if cfg!(windows) {
        home.join("vimit")
    } else {
        home.join(".config").join("vimit")
    };
    let config_file = config_dir.join("config.toml");
    if config_file.is_file() {
        Some(config_file)
    } else {
        None
    }
}

pub(crate) fn dirs_or_default() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
    } else {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_empty() {
        let config = Config::default();
        assert!(config.api_base.is_none());
        assert_eq!(config.theme(), Theme::Btop);
        assert_eq!(config.fail_on(), FailOn::Never);
        assert_eq!(config.warning(), 75.0);
        assert_eq!(config.danger(), 90.0);
        assert!(matches!(config.preset(), Preset::Full));
    }

    #[test]
    fn config_parses_valid_toml() {
        let toml_str = r#"
            theme = "dracula"
            warning = 80.0
            danger = 95.0
            watch = 10
            preset = "compact"
            notify = true
            fail_on = "warning"
            cache_ttl_secs = 120
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme(), Theme::Dracula);
        assert_eq!(config.warning(), 80.0);
        assert_eq!(config.danger(), 95.0);
        assert_eq!(config.watch, Some(10));
        assert_eq!(config.preset.as_deref(), Some("compact"));
        assert!(matches!(config.preset(), Preset::Compact));
        assert_eq!(config.notify, Some(true));
        assert_eq!(config.fail_on(), FailOn::Warning);
        assert_eq!(config.cache_ttl_secs, Some(120));
    }

    #[test]
    fn config_rejects_invalid_theme() {
        let toml_str = r#"theme = "invalid""#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme(), Theme::Btop);
    }

    #[test]
    fn config_window_thresholds() {
        let toml_str = r#"threshold = "5h=80:95,7d=85:95""#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let thresholds = config.window_thresholds().unwrap();
        assert_eq!(thresholds.get("5h"), Some(&(80.0, 95.0)));
        assert_eq!(thresholds.get("7d"), Some(&(85.0, 95.0)));
    }

    #[test]
    fn config_load_nonexistent_explicit_path_fails() {
        let path = PathBuf::from("/nonexistent/config.toml");
        let result = Config::load(Some(&path));
        assert!(result.is_err());
    }

    #[test]
    fn config_load_no_default_path_returns_empty() {
        let result = Config::load(None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().theme(), Theme::Btop);
    }

    #[test]
    fn merged_config_uses_defaults() {
        let config = Config::default();
        let merged = config.merge_with_defaults().unwrap();
        assert_eq!(merged.theme, Theme::Btop);
        assert_eq!(merged.warning_threshold, 75.0);
        assert_eq!(merged.danger_threshold, 90.0);
        assert!(!merged.demo);
        assert!(!merged.monitor);
        assert!(merged.auto_failover);
        assert_eq!(merged.cache_ttl_secs, super::super::cache::DEFAULT_TTL_SECS);
    }

    #[test]
    fn config_can_disable_auto_failover() {
        let config: Config = toml::from_str("auto_failover = false").unwrap();
        let merged = config.merge_with_defaults().unwrap();
        assert!(!merged.auto_failover);
    }
}
