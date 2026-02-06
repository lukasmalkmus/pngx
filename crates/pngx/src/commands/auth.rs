use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{Context, Result};

use crate::config::{config_dir, config_file_path};

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

pub fn status() -> Result<()> {
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

    Ok(())
}
