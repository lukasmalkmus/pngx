use anyhow::Result;
use pngx_client::Client;

use crate::output::{FieldFilter, OutputFormat};

pub fn list(client: &Client, format: OutputFormat, fields: Option<&FieldFilter>) -> Result<()> {
    let (types, _) = client.collect_document_types(None)?;
    super::print_all(format, &types, fields)?;
    Ok(())
}
