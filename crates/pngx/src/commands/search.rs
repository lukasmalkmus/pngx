use anyhow::Result;
use pngx_client::Client;

use crate::output::{FieldFilter, OutputFormat, resolve_documents};
use crate::resolve::NameResolver;

pub fn search(
    client: &Client,
    query: &str,
    format: OutputFormat,
    limit: Option<usize>,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let (results, total) = client.collect_search(query, limit)?;
    if results.is_empty() {
        eprintln!("No documents found for query: {query}");
    } else {
        let names = NameResolver::fetch(client, fields)?;
        let results = resolve_documents(&results, &names);
        super::print_results(format, &results, total, fields)?;
    }
    Ok(())
}
