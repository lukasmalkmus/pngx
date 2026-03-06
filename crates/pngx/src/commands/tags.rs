use anyhow::Result;
use pngx_client::Client;

use crate::output::{FieldFilter, OutputFormat};

pub fn list(client: &Client, format: OutputFormat, fields: Option<&FieldFilter>) -> Result<()> {
    let (tags, _) = client.collect_tags(None)?;
    super::print_all(format, &tags, fields)?;
    Ok(())
}
