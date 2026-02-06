use anyhow::Result;
use pngx_client::Client;

use crate::output::OutputFormat;

pub fn list(client: &Client, format: OutputFormat) -> Result<()> {
    let (tags, _) = client.collect_tags(None)?;
    super::print_all(format, &tags)?;
    Ok(())
}
