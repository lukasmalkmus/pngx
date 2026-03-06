use std::fmt;

use comfy_table::presets::ASCII_MARKDOWN;
use comfy_table::{ContentArrangement, Table};
use serde::{Deserialize, Serialize};

use pngx_client::{Correspondent, Document, DocumentType, Tag};

use crate::resolve::NameResolver;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Markdown,
    Json,
}

impl OutputFormat {
    pub fn format_list<T: Tabular + Serialize>(
        self,
        items: &[T],
        fields: Option<&FieldFilter>,
    ) -> Result<String, serde_json::Error> {
        match self {
            Self::Json => {
                let value = serde_json::to_value(items)?;
                if let Some(filter) = fields {
                    serde_json::to_string_pretty(&filter.filter_json_array(value))
                } else {
                    serde_json::to_string_pretty(&value)
                }
            }
            Self::Markdown => {
                let all_headers = T::headers();
                let indices = fields.map(|f| f.column_indices(all_headers));

                let visible_headers: Vec<&str> = match &indices {
                    Some(idx) => idx.iter().map(|&i| all_headers[i]).collect(),
                    None => all_headers.to_vec(),
                };

                let mut table = new_markdown_table(&visible_headers);
                for item in items {
                    let all_cols = item.row();
                    let row: Vec<String> = match &indices {
                        Some(idx) => idx.iter().map(|&i| all_cols[i].clone()).collect(),
                        None => all_cols,
                    };
                    table.add_row(row);
                }
                Ok(table.to_string())
            }
        }
    }

    pub fn format_detail<T: DetailView + Serialize>(
        self,
        item: &T,
        fields: Option<&FieldFilter>,
    ) -> Result<String, serde_json::Error> {
        match self {
            Self::Json => {
                let value = serde_json::to_value(item)?;
                if let Some(filter) = fields {
                    serde_json::to_string_pretty(&filter.filter_json_object(value))
                } else {
                    serde_json::to_string_pretty(&value)
                }
            }
            Self::Markdown => {
                let mut table = new_markdown_table(&["Field", "Value"]);
                for (field, value) in item.fields() {
                    if let Some(filter) = fields
                        && !filter.includes_display_name(field)
                    {
                        continue;
                    }
                    table.add_row(vec![field.to_string(), value]);
                }
                Ok(table.to_string())
            }
        }
    }
}

// --- FieldFilter ---

/// Maps between user-facing field names, JSON keys, and display headers.
pub trait FieldNames {
    /// Valid user-facing field names for this entity type.
    fn valid_fields() -> &'static [&'static str];

    /// Mapping from user-facing field name to the actual JSON key in serde output.
    /// Only needed when they differ (e.g., `correspondent` → `correspondent_name`).
    fn json_key_map() -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

/// Validated set of fields to include in output.
#[derive(Debug, Clone)]
pub struct FieldFilter {
    fields: Vec<String>,
    /// Maps user-facing field name → actual JSON key (only for non-identity mappings).
    json_key_map: Vec<(String, String)>,
}

impl fmt::Display for FieldFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.fields.join(", "))
    }
}

impl FieldFilter {
    /// Parse and validate a comma-separated field list against valid fields.
    pub fn parse<T: FieldNames>(input: &str) -> Result<Self, FieldFilterError> {
        let fields: Vec<String> = input.split(',').map(|s| s.trim().to_string()).collect();
        let valid = T::valid_fields();

        for field in &fields {
            if !valid.contains(&field.as_str()) {
                return Err(FieldFilterError {
                    invalid: field.clone(),
                    valid: valid.iter().map(|s| (*s).to_string()).collect(),
                });
            }
        }

        let json_key_map = T::json_key_map()
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();

        Ok(Self {
            fields,
            json_key_map,
        })
    }

    /// Whether any of the given field names are in the filter.
    pub fn needs_any(&self, names: &[&str]) -> bool {
        names.iter().any(|n| self.fields.contains(&n.to_string()))
    }

    /// Resolve a user-facing field name to its JSON key.
    fn resolve_json_key<'a>(&'a self, field: &'a str) -> &'a str {
        for (user_name, json_key) in &self.json_key_map {
            if user_name == field {
                return json_key;
            }
        }
        field
    }

    /// Filter a JSON array, keeping only the requested keys in each object.
    pub fn filter_json_array(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(arr) => serde_json::Value::Array(
                arr.into_iter()
                    .map(|v| self.filter_json_object(v))
                    .collect(),
            ),
            other => other,
        }
    }

    /// Filter a JSON object, keeping only the requested keys.
    /// Renames mapped keys back to user-facing names.
    fn filter_json_object(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut filtered = serde_json::Map::new();
                for field in &self.fields {
                    let json_key = self.resolve_json_key(field);
                    if let Some(val) = map.get(json_key) {
                        // Use the user-facing field name as the output key
                        filtered.insert(field.clone(), val.clone());
                    }
                }
                serde_json::Value::Object(filtered)
            }
            other => other,
        }
    }

    /// Get column indices matching the filter from Tabular headers.
    fn column_indices(&self, headers: &[&str]) -> Vec<usize> {
        headers
            .iter()
            .enumerate()
            .filter(|(_, h)| self.includes_display_name(h))
            .map(|(i, _)| i)
            .collect()
    }

    /// Check if a display header name is included in the filter.
    fn includes_display_name(&self, display_name: &str) -> bool {
        self.fields.iter().any(|f| {
            f.eq_ignore_ascii_case(display_name)
                || match display_name {
                    "ID" => f == "id",
                    "Title" => f == "title",
                    "Correspondent" => f == "correspondent",
                    "Type" | "Document Type" => f == "document_type",
                    "Tags" => f == "tags",
                    "Created" => f == "created",
                    "Added" => f == "added",
                    "Name" => f == "name",
                    "Color" => f == "color",
                    "Documents" => f == "document_count",
                    "Slug" => f == "slug",
                    "Inbox Tag" => f == "is_inbox_tag",
                    "Original File" => f == "original_file_name",
                    "ASN" => f == "archive_serial_number",
                    _ => false,
                }
        })
    }
}

#[derive(Debug)]
pub struct FieldFilterError {
    pub invalid: String,
    pub valid: Vec<String>,
}

impl fmt::Display for FieldFilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown field '{}'. Valid fields: {}",
            self.invalid,
            self.valid.join(", ")
        )
    }
}

impl std::error::Error for FieldFilterError {}

pub trait Tabular {
    fn headers() -> &'static [&'static str];
    fn row(&self) -> Vec<String>;
}

pub trait DetailView {
    fn fields(&self) -> Vec<(&'static str, String)>;
}

fn display_opt<T: std::fmt::Display>(opt: Option<&T>, default: &str) -> String {
    opt.map_or_else(|| default.to_string(), std::string::ToString::to_string)
}

fn new_markdown_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table.load_preset(ASCII_MARKDOWN);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(headers.iter());
    table
}

// --- ResolvedDocument ---

impl FieldNames for ResolvedDocument {
    fn valid_fields() -> &'static [&'static str] {
        &[
            "id",
            "title",
            "correspondent",
            "document_type",
            "tags",
            "created",
            "added",
            "archive_serial_number",
            "original_file_name",
        ]
    }
}

#[derive(Serialize)]
#[allow(dead_code)]
pub struct ResolvedDocument {
    pub id: u64,
    pub title: String,
    #[serde(skip)]
    pub correspondent: Option<u64>,
    #[serde(rename = "correspondent")]
    pub correspondent_name: Option<String>,
    #[serde(skip)]
    pub document_type: Option<u64>,
    #[serde(rename = "document_type")]
    pub document_type_name: Option<String>,
    #[serde(skip)]
    pub tags: Vec<u64>,
    #[serde(rename = "tags")]
    pub tag_names: Vec<String>,
    pub created: Option<jiff::civil::Date>,
    pub added: Option<jiff::Timestamp>,
    pub archive_serial_number: Option<u64>,
    pub original_file_name: Option<String>,
}

impl Tabular for ResolvedDocument {
    fn headers() -> &'static [&'static str] {
        &["ID", "Title", "Correspondent", "Type", "Created", "Tags"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.title.clone(),
            self.correspondent_name.clone().unwrap_or_default(),
            self.document_type_name.clone().unwrap_or_default(),
            display_opt(self.created.as_ref(), ""),
            self.tag_names.join(", "),
        ]
    }
}

impl DetailView for ResolvedDocument {
    fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("ID", self.id.to_string()),
            ("Title", self.title.clone()),
            ("Created", display_opt(self.created.as_ref(), "N/A")),
            ("Added", display_opt(self.added.as_ref(), "N/A")),
            (
                "Correspondent",
                self.correspondent_name
                    .clone()
                    .unwrap_or_else(|| "N/A".to_string()),
            ),
            (
                "Document Type",
                self.document_type_name
                    .clone()
                    .unwrap_or_else(|| "N/A".to_string()),
            ),
            ("Tags", self.tag_names.join(", ")),
        ];
        if let Some(ref name) = self.original_file_name {
            fields.push(("Original File", name.clone()));
        }
        if let Some(asn) = self.archive_serial_number {
            fields.push(("ASN", asn.to_string()));
        }
        fields
    }
}

pub fn resolve_documents(docs: &[Document], resolver: &NameResolver) -> Vec<ResolvedDocument> {
    docs.iter()
        .map(|doc| ResolvedDocument {
            id: doc.id,
            title: doc.title.clone(),
            correspondent: doc.correspondent,
            correspondent_name: doc
                .correspondent
                .and_then(|id| resolver.correspondent_name(id)),
            document_type: doc.document_type,
            document_type_name: doc
                .document_type
                .and_then(|id| resolver.document_type_name(id)),
            tags: doc.tags.clone(),
            tag_names: doc.tags.iter().map(|&id| resolver.tag_name(id)).collect(),
            created: doc.created,
            added: doc.added,
            archive_serial_number: doc.archive_serial_number,
            original_file_name: doc.original_file_name.clone(),
        })
        .collect()
}

// --- FieldNames impls for metadata types ---

impl FieldNames for Tag {
    fn valid_fields() -> &'static [&'static str] {
        &[
            "id",
            "name",
            "slug",
            "color",
            "is_inbox_tag",
            "document_count",
        ]
    }
}

impl FieldNames for Correspondent {
    fn valid_fields() -> &'static [&'static str] {
        &["id", "name", "slug", "document_count"]
    }
}

impl FieldNames for DocumentType {
    fn valid_fields() -> &'static [&'static str] {
        &["id", "name", "slug", "document_count"]
    }
}

// --- Tabular impls ---

impl Tabular for Tag {
    fn headers() -> &'static [&'static str] {
        &["ID", "Name", "Color", "Documents"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.name.clone(),
            self.color.clone().unwrap_or_default(),
            self.document_count
                .map(|c| c.to_string())
                .unwrap_or_default(),
        ]
    }
}

impl Tabular for Correspondent {
    fn headers() -> &'static [&'static str] {
        &["ID", "Name", "Documents"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.name.clone(),
            self.document_count
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ]
    }
}

impl Tabular for DocumentType {
    fn headers() -> &'static [&'static str] {
        &["ID", "Name", "Documents"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.name.clone(),
            self.document_count
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ]
    }
}
