use anyhow::Result;
use pngx_client::Client;

use crate::output::{OutputFormat, resolve_documents};
use crate::resolve::NameResolver;

pub fn list(client: &Client, format: OutputFormat, limit: Option<usize>) -> Result<()> {
    let (results, total) = client.collect_inbox_documents(limit)?;
    if results.is_empty() {
        eprintln!("Inbox is empty");
    } else {
        let names = NameResolver::fetch(client)?;
        let results = resolve_documents(&results, &names);
        super::print_results(format, &results, total)?;
    }
    Ok(())
}
