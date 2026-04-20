use anyhow::Result;
use pngx_client::{
    Client, Correspondent, CorrespondentCreate, CorrespondentUpdate, MatchingAlgorithm,
};

use crate::output::{FieldFilter, OutputFormat};
use crate::resolve::NameOrId;

pub fn list(client: &Client, format: OutputFormat, fields: Option<&FieldFilter>) -> Result<()> {
    let (correspondents, _) = client.collect_correspondents(None)?;
    super::print_all(format, &correspondents, fields)?;
    Ok(())
}

pub fn create(
    client: &Client,
    name: String,
    matching_algorithm: Option<MatchingAlgorithm>,
    matches: Option<String>,
    is_insensitive: Option<bool>,
    format: OutputFormat,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let payload = CorrespondentCreate {
        name,
        matching_algorithm,
        matches,
        is_insensitive,
    };
    let correspondent = client.create_correspondent(&payload)?;
    super::print_one(format, &correspondent, fields)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    client: &Client,
    id_or_name: &str,
    name: Option<String>,
    matching_algorithm: Option<MatchingAlgorithm>,
    matches: Option<String>,
    is_insensitive: Option<bool>,
    format: OutputFormat,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let id = Correspondent::resolve(client, id_or_name)?;
    let payload = CorrespondentUpdate {
        name,
        matching_algorithm,
        matches,
        is_insensitive,
    };
    let correspondent = client.update_correspondent(id, &payload)?;
    super::print_one(format, &correspondent, fields)?;
    Ok(())
}

pub fn delete(client: &Client, id_or_name: &str) -> Result<()> {
    let id = Correspondent::resolve(client, id_or_name)?;
    client.delete_correspondent(id)?;
    eprintln!("Deleted correspondent {id}");
    Ok(())
}
