use anyhow::Result;
use pngx_client::Client;

use crate::output::{FieldFilter, OutputFormat, resolve_documents};
use crate::resolve::NameResolver;

pub fn list(
    client: &Client,
    format: OutputFormat,
    limit: Option<usize>,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let (results, total) = client.collect_inbox_documents(limit)?;
    if results.is_empty() {
        super::print_empty(format, "Inbox is empty")?;
    } else {
        let names = NameResolver::fetch(client, fields)?;
        let results = resolve_documents(&results, &names);
        super::print_results(format, &results, total, fields)?;
    }
    Ok(())
}
