use anyhow::Result;
use pngx_client::Client;

use crate::output::{FieldFilter, OutputFormat};

pub fn list(client: &Client, format: OutputFormat, fields: Option<&FieldFilter>) -> Result<()> {
    let (correspondents, _) = client.collect_correspondents(None)?;
    super::print_all(format, &correspondents, fields)?;
    Ok(())
}
