use std::collections::HashMap;

use anyhow::Result;
use pngx_client::Client;

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
