pub mod auth;
pub mod correspondents;
pub mod document_types;
pub mod documents;
pub mod inbox;
pub mod mcp;
pub mod search;
pub mod tags;
pub mod version;

use crate::output::{FieldFilter, OutputFormat, Tabular};

pub fn print_results<T: Tabular + serde::Serialize>(
    format: OutputFormat,
    items: &[T],
    total: u64,
    fields: Option<&FieldFilter>,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            let value = serde_json::to_value(items)?;
            let results = match fields {
                Some(f) => f.filter_json_array(value),
                None => value,
            };
            let wrapper = serde_json::json!({
                "results": results,
                "total_count": total,
                "showing": items.len(),
                "has_more": (items.len() as u64) < total,
            });
            println!("{}", serde_json::to_string_pretty(&wrapper)?);
        }
        OutputFormat::Ndjson => {
            print_ndjson_meta(items.len(), total);
            print_ndjson_items(items, fields)?;
        }
        OutputFormat::Markdown => {
            println!("{}", format.format_list(items, fields)?);
        }
    }
    if (items.len() as u64) < total {
        eprintln!(
            "Showing {} of {} results (use -n to change limit or --all to fetch all)",
            items.len(),
            total,
        );
    }
    Ok(())
}

pub fn print_all<T: Tabular + serde::Serialize>(
    format: OutputFormat,
    items: &[T],
    fields: Option<&FieldFilter>,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            let value = serde_json::to_value(items)?;
            let output = match fields {
                Some(f) => f.filter_json_array(value),
                None => value,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Ndjson => {
            print_ndjson_items(items, fields)?;
        }
        OutputFormat::Markdown => {
            println!("{}", format.format_list(items, fields)?);
        }
    }
    Ok(())
}

/// Handle empty results: emit structured output for JSON/NDJSON, human
/// message for markdown.
pub fn print_empty(format: OutputFormat, message: &str) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            let wrapper = serde_json::json!({
                "results": [],
                "total_count": 0,
                "showing": 0,
                "has_more": false,
            });
            println!("{}", serde_json::to_string_pretty(&wrapper)?);
        }
        OutputFormat::Ndjson => {
            print_ndjson_meta(0, 0);
        }
        OutputFormat::Markdown => {
            eprintln!("{message}");
        }
    }
    Ok(())
}

fn print_ndjson_meta(showing: usize, total: u64) {
    let meta = serde_json::json!({
        "_meta": true,
        "total_count": total,
        "showing": showing,
        "has_more": (showing as u64) < total,
    });
    println!("{meta}");
}

fn print_ndjson_items<T: serde::Serialize>(
    items: &[T],
    fields: Option<&FieldFilter>,
) -> anyhow::Result<()> {
    for item in items {
        let value = serde_json::to_value(item)?;
        let line = match fields {
            Some(f) => f.filter_json_object(value),
            None => value,
        };
        println!("{}", serde_json::to_string(&line)?);
    }
    Ok(())
}
