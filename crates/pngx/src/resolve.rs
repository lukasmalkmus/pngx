use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use pngx_client::{Client, Correspondent, DocumentType, StoragePath, Tag};

use crate::output::FieldFilter;

const RESOLVED_FIELDS: &[&str] = &["correspondent", "document_type", "tags"];

pub struct NameResolver {
    tags: HashMap<u64, String>,
    correspondents: HashMap<u64, String>,
    document_types: HashMap<u64, String>,
}

impl NameResolver {
    pub fn fetch(client: &Client, fields: Option<&FieldFilter>) -> Result<Self> {
        if let Some(f) = fields
            && !f.needs_any(RESOLVED_FIELDS)
        {
            return Ok(Self {
                tags: HashMap::new(),
                correspondents: HashMap::new(),
                document_types: HashMap::new(),
            });
        }

        let (tags, _) = client.collect_tags(None)?;
        let (correspondents, _) = client.collect_correspondents(None)?;
        let (document_types, _) = client.collect_document_types(None)?;

        Ok(Self {
            tags: tags.into_iter().map(|t| (t.id, t.name)).collect(),
            correspondents: correspondents.into_iter().map(|c| (c.id, c.name)).collect(),
            document_types: document_types
                .into_iter()
                .map(|dt| (dt.id, dt.name))
                .collect(),
        })
    }

    pub fn tag_name(&self, id: u64) -> String {
        self.tags
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("#{id}"))
    }

    pub fn correspondent_name(&self, id: u64) -> Option<String> {
        self.correspondents.get(&id).cloned()
    }

    pub fn document_type_name(&self, id: u64) -> Option<String> {
        self.document_types.get(&id).cloned()
    }
}

// --- Name-or-ID resolution for write commands -----------------------------

/// A value that can be resolved into a numeric ID from either an integer
/// literal (`"42"`) or a name (`"Apple"`).
pub trait NameOrId {
    fn resolve(client: &Client, input: &str) -> Result<u64>;
}

impl NameOrId for Tag {
    fn resolve(client: &Client, input: &str) -> Result<u64> {
        if let Ok(id) = input.parse::<u64>() {
            return Ok(id);
        }
        let (items, _) = client.collect_tags(None)?;
        resolve_by_name(&items, input, |t| (t.id, t.name.as_str()), "tag")
    }
}

impl NameOrId for Correspondent {
    fn resolve(client: &Client, input: &str) -> Result<u64> {
        if let Ok(id) = input.parse::<u64>() {
            return Ok(id);
        }
        let (items, _) = client.collect_correspondents(None)?;
        resolve_by_name(&items, input, |c| (c.id, c.name.as_str()), "correspondent")
    }
}

impl NameOrId for DocumentType {
    fn resolve(client: &Client, input: &str) -> Result<u64> {
        if let Ok(id) = input.parse::<u64>() {
            return Ok(id);
        }
        let (items, _) = client.collect_document_types(None)?;
        resolve_by_name(&items, input, |d| (d.id, d.name.as_str()), "document type")
    }
}

impl NameOrId for StoragePath {
    fn resolve(client: &Client, input: &str) -> Result<u64> {
        if let Ok(id) = input.parse::<u64>() {
            return Ok(id);
        }
        let (items, _) = client.collect_storage_paths(None)?;
        resolve_by_name(&items, input, |s| (s.id, s.name.as_str()), "storage path")
    }
}

fn resolve_by_name<T>(
    items: &[T],
    name: &str,
    project: impl Fn(&T) -> (u64, &str),
    entity: &str,
) -> Result<u64> {
    let matches: Vec<(u64, String)> = items
        .iter()
        .map(&project)
        .filter(|(_, n)| *n == name)
        .map(|(id, n)| (id, n.to_string()))
        .collect();
    match matches.len() {
        0 => Err(anyhow!(
            "no {entity} named '{name}'. Use `pngx {plural} create \"{name}\"` to create one, \
             or `pngx {plural} list` to see available options.",
            plural = pluralize(entity)
        )),
        1 => Ok(matches[0].0),
        _ => {
            let ids: Vec<String> = matches.iter().map(|(id, _)| id.to_string()).collect();
            bail!(
                "{entity} name '{name}' is ambiguous (matches IDs: {}). Pass the numeric ID instead.",
                ids.join(", ")
            );
        }
    }
}

fn pluralize(entity: &str) -> &'static str {
    match entity {
        "tag" => "tags",
        "correspondent" => "correspondents",
        "document type" => "document-types",
        "storage path" => "storage-paths",
        _ => "",
    }
}
