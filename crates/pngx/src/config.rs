use std::fmt;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::output::OutputFormat;

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

impl RawConfig {
    pub fn load(url_override: Option<&str>, token_override: Option<&str>) -> anyhow::Result<Self> {
        let mut figment = Figment::from(Serialized::defaults(RawConfig::default()))
            .merge(Toml::file(config_file_path()))
            .merge(Env::prefixed("PNGX_"));

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

    pub fn validate(self) -> anyhow::Result<ValidConfig> {
        anyhow::ensure!(
            !self.url.is_empty(),
            "server URL not configured. Run `pngx auth login` or set --url"
        );
        anyhow::ensure!(
            !self.token.is_empty(),
            "API token not configured. Run `pngx auth login` or set --token"
        );

        let url = Url::parse(&self.url)
            .map_err(|e| anyhow::anyhow!("invalid server URL '{}': {e}", self.url))?;

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
