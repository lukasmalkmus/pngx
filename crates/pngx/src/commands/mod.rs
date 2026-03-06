pub mod auth;
pub mod correspondents;
pub mod document_types;
pub mod documents;
pub mod inbox;
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
    if matches!(format, OutputFormat::Json) {
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
    } else {
        println!("{}", format.format_list(items, fields)?);
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
        OutputFormat::Markdown => {
            println!("{}", format.format_list(items, fields)?);
        }
    }
    Ok(())
}
