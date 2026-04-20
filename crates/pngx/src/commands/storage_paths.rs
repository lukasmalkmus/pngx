use anyhow::Result;
use pngx_client::{Client, MatchingAlgorithm, StoragePath, StoragePathCreate, StoragePathUpdate};

use crate::output::{FieldFilter, OutputFormat};
use crate::resolve::NameOrId;

pub fn list(client: &Client, format: OutputFormat, fields: Option<&FieldFilter>) -> Result<()> {
    let (paths, _) = client.collect_storage_paths(None)?;
    super::print_all(format, &paths, fields)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    client: &Client,
    name: String,
    path: String,
    matching_algorithm: Option<MatchingAlgorithm>,
    matches: Option<String>,
    is_insensitive: Option<bool>,
    format: OutputFormat,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let payload = StoragePathCreate {
        name,
        path,
        matching_algorithm,
        matches,
        is_insensitive,
    };
    let storage_path = client.create_storage_path(&payload)?;
    super::print_one(format, &storage_path, fields)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    client: &Client,
    id_or_name: &str,
    name: Option<String>,
    path: Option<String>,
    matching_algorithm: Option<MatchingAlgorithm>,
    matches: Option<String>,
    is_insensitive: Option<bool>,
    format: OutputFormat,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let id = StoragePath::resolve(client, id_or_name)?;
    let payload = StoragePathUpdate {
        name,
        path,
        matching_algorithm,
        matches,
        is_insensitive,
    };
    let storage_path = client.update_storage_path(id, &payload)?;
    super::print_one(format, &storage_path, fields)?;
    Ok(())
}

pub fn delete(client: &Client, id_or_name: &str) -> Result<()> {
    let id = StoragePath::resolve(client, id_or_name)?;
    client.delete_storage_path(id)?;
    eprintln!("Deleted storage path {id}");
    Ok(())
}
