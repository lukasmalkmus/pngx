use anyhow::Result;
use pngx_client::{Client, MatchingAlgorithm, Tag, TagCreate, TagUpdate};

use crate::output::{FieldFilter, OutputFormat};
use crate::resolve::NameOrId;

pub fn list(client: &Client, format: OutputFormat, fields: Option<&FieldFilter>) -> Result<()> {
    let (tags, _) = client.collect_tags(None)?;
    super::print_all(format, &tags, fields)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    client: &Client,
    name: String,
    color: Option<String>,
    matching_algorithm: Option<MatchingAlgorithm>,
    matches: Option<String>,
    is_insensitive: Option<bool>,
    is_inbox: Option<bool>,
    format: OutputFormat,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let payload = TagCreate {
        name,
        color,
        matching_algorithm,
        matches,
        is_insensitive,
        is_inbox_tag: is_inbox,
    };
    let tag = client.create_tag(&payload)?;
    super::print_one(format, &tag, fields)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    client: &Client,
    id_or_name: &str,
    name: Option<String>,
    color: Option<String>,
    matching_algorithm: Option<MatchingAlgorithm>,
    matches: Option<String>,
    is_insensitive: Option<bool>,
    is_inbox: Option<bool>,
    format: OutputFormat,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let id = Tag::resolve(client, id_or_name)?;
    let payload = TagUpdate {
        name,
        color,
        matching_algorithm,
        matches,
        is_insensitive,
        is_inbox_tag: is_inbox,
    };
    let tag = client.update_tag(id, &payload)?;
    super::print_one(format, &tag, fields)?;
    Ok(())
}

pub fn delete(client: &Client, id_or_name: &str) -> Result<()> {
    let id = Tag::resolve(client, id_or_name)?;
    client.delete_tag(id)?;
    eprintln!("Deleted tag {id}");
    Ok(())
}
