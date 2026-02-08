use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use pngx_client::Client;

use crate::config::{RawConfig, config_dir, config_file_path};

pub fn login(url: Option<&str>, token: Option<&str>) -> Result<()> {
    let (url, token) = match (url, token) {
        (Some(u), Some(t)) => (u.to_string(), t.to_string()),
        (None, None) => {
            if !io::stdin().is_terminal() {
                anyhow::bail!(
                    "cannot run interactive login without a terminal.\n\
                     Use: pngx auth login --url <URL> --token <TOKEN>"
                );
            }

            let stdin = io::stdin();
            let mut stdout = io::stdout();

            print!("Paperless NGX URL: ");
            stdout.flush()?;
            let mut url = String::new();
            stdin.lock().read_line(&mut url)?;

            let token =
                rpassword::prompt_password("API Token: ").context("failed to read token")?;

            (url.trim().to_string(), token.trim().to_string())
        }
        _ => {
            anyhow::bail!(
                "both --url and --token are required for non-interactive login.\n\
                 Either provide both flags or omit both for interactive mode."
            );
        }
    };

    if url.is_empty() || token.is_empty() {
        anyhow::bail!("URL and token must not be empty");
    }

    let dir = config_dir();
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create config directory: {}", dir.display()))?;

    let content = format!("url = \"{url}\"\ntoken = \"{token}\"\n");
    let path = config_file_path();
    write_config_file(&path, content.as_bytes())
        .with_context(|| format!("failed to write config file: {}", path.display()))?;

    println!("Credentials saved to {}", path.display());
    Ok(())
}

#[cfg(unix)]
fn write_config_file(path: &std::path::Path, content: &[u8]) -> io::Result<()> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(content)
}

#[cfg(not(unix))]
fn write_config_file(path: &std::path::Path, content: &[u8]) -> io::Result<()> {
    fs::write(path, content)
}

pub fn logout() -> Result<()> {
    let path = config_file_path();
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove config file: {}", path.display()))?;
        println!("Logged out. Config removed from {}", path.display());
    } else {
        println!("No config file found at {}", path.display());
    }
    Ok(())
}

pub fn status(url_override: Option<&str>, token_override: Option<&str>) -> Result<()> {
    let path = config_file_path();
    if !path.exists() {
        println!("Not configured. Run `pngx auth login` to set up.");
        return Ok(());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;

    println!("Config file: {}", path.display());
    println!();

    for line in content.lines() {
        if line.trim_start().starts_with("token") {
            println!("token = \"***\"");
        } else {
            println!("{line}");
        }
    }

    match try_verify_server(url_override, token_override) {
        Ok((user, version)) => {
            println!("\nUser: {user}");
            println!("Server: connected (paperless-ngx {version})");
        }
        Err(err) => eprintln!("\nWarning: could not verify server: {err}"),
    }

    Ok(())
}

fn try_verify_server(
    url_override: Option<&str>,
    token_override: Option<&str>,
) -> anyhow::Result<(String, String)> {
    let raw = RawConfig::load(url_override, token_override)?;
    let config = raw.validate()?;
    let client = Client::builder(config.url.as_str(), &config.token)
        .timeout(Duration::from_secs(config.timeout))
        .page_size(config.page_size)
        .build()?;
    let settings = client.ui_settings()?;
    let user = settings.user.display_name();
    let version = settings.settings.version;
    Ok((user, version))
}
