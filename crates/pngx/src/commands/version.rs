use std::time::Duration;

use anyhow::Result;
use pngx_client::Client;

use crate::config::RawConfig;

pub fn print(url: Option<&str>, token: Option<&str>) -> Result<()> {
    println!("pngx {}", env!("CARGO_PKG_VERSION"));

    let Ok(raw) = RawConfig::load(url, token) else {
        return Ok(());
    };
    let Ok(config) = raw.validate() else {
        return Ok(());
    };

    let client = Client::builder(config.url.as_str(), &config.token)
        .timeout(Duration::from_secs(config.timeout))
        .page_size(config.page_size)
        .build()?;

    let version = client.server_version()?;
    println!("paperless-ngx {version}");
    Ok(())
}
