use anyhow::Result;
use pngx_client::Client;

use crate::output::OutputFormat;

pub fn list(client: &Client, format: OutputFormat) -> Result<()> {
    let (correspondents, _) = client.collect_correspondents(None)?;
    super::print_all(format, &correspondents)?;
    Ok(())
}
