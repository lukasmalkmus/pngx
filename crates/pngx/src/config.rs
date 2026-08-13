use std::fmt;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::output::OutputFormat;

/// Configuration validation error. Uses a distinct exit code (5) so agents
/// can detect missing config and suggest running `pngx auth login`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("server URL not configured. Run `pngx auth login` or set --url")]
    MissingUrl,
    #[error("API token not configured. Run `pngx auth login` or set --token")]
    MissingToken,
    #[error("invalid server URL '{url}': {source}")]
    InvalidUrl {
        url: String,
        source: url::ParseError,
    },
}

#[derive(Deserialize, Serialize)]
pub struct RawConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub output_format: OutputFormat,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_page_size() -> u32 {
    100
}

fn default_timeout() -> u64 {
    30
}

impl Default for RawConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            token: String::new(),
            output_format: OutputFormat::Markdown,
            page_size: default_page_size(),
            timeout: default_timeout(),
        }
    }
}

impl fmt::Debug for RawConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawConfig")
            .field("url", &self.url)
            .field("token", &"[REDACTED]")
            .field("output_format", &self.output_format)
            .field("page_size", &self.page_size)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Whether `PNGX_<key>` holds something other than whitespace. Env layers over
/// the config file, so a blank value would otherwise erase a stored login. The
/// plugin passes unset `userConfig` options through as empty strings.
fn env_var_is_set(key: &figment::value::UncasedStr) -> bool {
    std::env::var(format!("PNGX_{}", key.as_str().to_uppercase()))
        .is_ok_and(|value| !value.trim().is_empty())
}

impl RawConfig {
    pub fn load(url_override: Option<&str>, token_override: Option<&str>) -> anyhow::Result<Self> {
        let mut figment = Figment::from(Serialized::defaults(RawConfig::default()))
            .merge(Toml::file(config_file_path()))
            .merge(Env::prefixed("PNGX_").filter(env_var_is_set));

        if let Some(url) = url_override {
            figment = figment.merge(Serialized::default("url", url));
        }
        if let Some(token) = token_override {
            figment = figment.merge(Serialized::default("token", token));
        }

        let config: RawConfig = figment.extract()?;

        if !config.url.is_empty() && config.url.starts_with("http://") {
            tracing::warn!("using insecure HTTP connection to {}", config.url);
        }

        Ok(config)
    }

    pub fn validate(self) -> Result<ValidConfig, ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::MissingUrl);
        }
        if self.token.is_empty() {
            return Err(ConfigError::MissingToken);
        }

        let url = Url::parse(&self.url).map_err(|e| ConfigError::InvalidUrl {
            url: self.url.clone(),
            source: e,
        })?;

        Ok(ValidConfig {
            url,
            token: self.token,
            output_format: self.output_format,
            page_size: self.page_size,
            timeout: self.timeout,
        })
    }
}

pub struct ValidConfig {
    pub url: Url,
    pub token: String,
    pub output_format: OutputFormat,
    pub page_size: u32,
    pub timeout: u64,
}

impl fmt::Debug for ValidConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidConfig")
            .field("url", &self.url)
            .field("token", &"[REDACTED]")
            .field("output_format", &self.output_format)
            .field("page_size", &self.page_size)
            .field("timeout", &self.timeout)
            .finish()
    }
}

pub fn config_dir() -> PathBuf {
    etcetera::choose_base_strategy().ok().map_or_else(
        || PathBuf::from("."),
        |s| etcetera::BaseStrategy::config_dir(&s).join("pngx"),
    )
}

pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

#[cfg(test)]
// Jail::expect_with hands back figment::Error, which is wide enough to trip
// result_large_err. Not ours to shrink.
#[allow(clippy::result_large_err)]
mod tests {
    use super::*;

    #[test]
    fn blank_env_does_not_erase_stored_login() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("PNGX_URL", "");
            jail.set_env("PNGX_TOKEN", "   ");

            let config: RawConfig = Figment::from(Serialized::defaults(RawConfig::default()))
                .merge(Serialized::default("url", "https://paperless.example.com"))
                .merge(Serialized::default("token", "stored-token"))
                .merge(Env::prefixed("PNGX_").filter(env_var_is_set))
                .extract()?;

            assert_eq!(config.url, "https://paperless.example.com");
            assert_eq!(config.token, "stored-token");
            Ok(())
        });
    }

    #[test]
    fn populated_env_still_wins() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("PNGX_URL", "https://from-env.example.com");

            let config: RawConfig = Figment::from(Serialized::defaults(RawConfig::default()))
                .merge(Serialized::default("url", "https://from-file.example.com"))
                .merge(Env::prefixed("PNGX_").filter(env_var_is_set))
                .extract()?;

            assert_eq!(config.url, "https://from-env.example.com");
            Ok(())
        });
    }
}
