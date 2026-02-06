use anyhow::Result;
use pngx_client::Client;

use crate::output::OutputFormat;

pub fn list(client: &Client, format: OutputFormat) -> Result<()> {
    let (types, _) = client.collect_document_types(None)?;
    super::print_all(format, &types)?;
    Ok(())
}
