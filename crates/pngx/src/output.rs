use std::fmt;

use comfy_table::presets::ASCII_MARKDOWN;
use comfy_table::{ContentArrangement, Table};
use serde::{Deserialize, Serialize};

use pngx_client::{Correspondent, Document, DocumentType, Note, StoragePath, Tag};

use crate::resolve::NameResolver;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Markdown,
    Json,
    Ndjson,
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
            Self::Ndjson => {
                let mut lines = Vec::new();
                for item in items {
                    let value = serde_json::to_value(item)?;
                    let line = match fields {
                        Some(f) => f.filter_json_object(value),
                        None => value,
                    };
                    lines.push(serde_json::to_string(&line)?);
                }
                Ok(lines.join("\n"))
            }
            Self::Markdown => {
                let table = if let Some(filter) = fields {
                    let labels: Vec<&str> = filter.fields.iter().map(String::as_str).collect();
                    let mut table = new_markdown_table(&labels);
                    for item in items {
                        table.add_row(filter.cells(&serde_json::to_value(item)?));
                    }
                    table
                } else {
                    let mut table = new_markdown_table(T::headers());
                    for item in items {
                        table.add_row(item.row());
                    }
                    table
                };
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
            Self::Ndjson => {
                let value = serde_json::to_value(item)?;
                let line = match fields {
                    Some(f) => f.filter_json_object(value),
                    None => value,
                };
                serde_json::to_string(&line)
            }
            Self::Markdown => {
                let mut table = new_markdown_table(&["Field", "Value"]);
                if let Some(filter) = fields {
                    let values = filter.cells(&serde_json::to_value(item)?);
                    for (field, value) in filter.fields.iter().zip(values) {
                        table.add_row(vec![field.clone(), value]);
                    }
                } else {
                    for (field, value) in item.fields() {
                        table.add_row(vec![field.to_string(), value]);
                    }
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
        let valid = T::valid_fields();

        let mut fields: Vec<String> = Vec::new();
        for field in input.split(',').map(str::trim) {
            if !valid.contains(&field) {
                return Err(FieldFilterError {
                    invalid: field.to_string(),
                    valid: valid.iter().map(|s| (*s).to_string()).collect(),
                });
            }
            if !fields.iter().any(|seen| seen == field) {
                fields.push(field.to_string());
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
    pub fn filter_json_object(&self, value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut filtered = serde_json::Map::new();
                for field in &self.fields {
                    let json_key = self.resolve_json_key(field);
                    let val = map
                        .get(json_key)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    // Use the user-facing field name as the output key
                    filtered.insert(field.clone(), val);
                }
                serde_json::Value::Object(filtered)
            }
            other => other,
        }
    }

    /// Render the requested fields of a serialized record as table cells.
    fn cells(&self, value: &serde_json::Value) -> Vec<String> {
        self.fields
            .iter()
            .map(|field| match value.get(self.resolve_json_key(field)) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "null".to_string(),
            })
            .collect()
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
    table.load_style(ASCII_MARKDOWN);
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

impl FieldNames for StoragePath {
    fn valid_fields() -> &'static [&'static str] {
        &["id", "name", "slug", "path", "document_count"]
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

impl Tabular for StoragePath {
    fn headers() -> &'static [&'static str] {
        &["ID", "Name", "Path", "Documents"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.name.clone(),
            self.path.clone(),
            self.document_count
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ]
    }
}

impl FieldNames for Note {
    fn valid_fields() -> &'static [&'static str] {
        &["id", "note", "created", "user"]
    }
}

impl Tabular for Note {
    fn headers() -> &'static [&'static str] {
        &["ID", "Note", "Created", "User"]
    }

    fn row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.note.clone(),
            display_opt(self.created.as_ref(), ""),
            self.user
                .as_ref()
                .map(|u| u.username.clone())
                .unwrap_or_default(),
        ]
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests assert on rendered output of static fixtures"
)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use super::*;

    fn from_json<T: DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn document() -> ResolvedDocument {
        ResolvedDocument {
            id: 42,
            title: "Rechnung".to_string(),
            correspondent: Some(1),
            correspondent_name: Some("ACME".to_string()),
            document_type: Some(2),
            document_type_name: Some("Invoice".to_string()),
            tags: vec![3],
            tag_names: vec!["Steuer".to_string()],
            created: Some(jiff::civil::date(2026, 1, 8)),
            added: Some(jiff::Timestamp::UNIX_EPOCH),
            archive_serial_number: Some(7),
            original_file_name: Some("r.pdf".to_string()),
        }
    }

    fn document_unset() -> ResolvedDocument {
        ResolvedDocument {
            correspondent: None,
            correspondent_name: None,
            document_type: None,
            document_type_name: None,
            created: None,
            added: None,
            archive_serial_number: None,
            original_file_name: None,
            ..document()
        }
    }

    fn cell(table: &str, line: usize) -> &str {
        table.lines().nth(line).unwrap().trim_matches('|').trim()
    }

    fn assert_selects_only(record: &Value, field: &str, may_be_null: bool) {
        let object = record.as_object().unwrap();
        assert_eq!(object.len(), 1, "-F {field}");
        let value = object.get(field).unwrap();
        assert!(may_be_null || !value.is_null(), "-F {field}");
    }

    fn assert_every_field_appears_in_list<T>(item: &T, may_be_null: bool)
    where
        T: Tabular + FieldNames + Serialize,
    {
        let items = std::slice::from_ref(item);
        for field in T::valid_fields() {
            let filter = FieldFilter::parse::<T>(field).unwrap();

            let json = OutputFormat::Json.format_list(items, Some(&filter));
            let json: Value = serde_json::from_str(&json.unwrap()).unwrap();
            assert_selects_only(&json[0], field, may_be_null);

            let ndjson = OutputFormat::Ndjson.format_list(items, Some(&filter));
            let record: Value = serde_json::from_str(&ndjson.unwrap()).unwrap();
            assert_selects_only(&record, field, may_be_null);

            let table = OutputFormat::Markdown.format_list(items, Some(&filter));
            assert_eq!(cell(&table.unwrap(), 0), *field);
        }
    }

    fn assert_every_field_appears_in_detail(item: &ResolvedDocument, may_be_null: bool) {
        for field in ResolvedDocument::valid_fields() {
            let filter = FieldFilter::parse::<ResolvedDocument>(field).unwrap();

            let json = OutputFormat::Json.format_detail(item, Some(&filter));
            let json: Value = serde_json::from_str(&json.unwrap()).unwrap();
            assert_selects_only(&json, field, may_be_null);

            let ndjson = OutputFormat::Ndjson
                .format_detail(item, Some(&filter))
                .unwrap();
            assert_eq!(ndjson.lines().count(), 1, "-F {field}");
            let record: Value = serde_json::from_str(&ndjson).unwrap();
            assert_selects_only(&record, field, may_be_null);

            let table = OutputFormat::Markdown.format_detail(item, Some(&filter));
            let table = table.unwrap();
            let row = table.lines().nth(2).unwrap();
            assert!(row.starts_with(&format!("| {field} ")), "{row}");
        }
    }

    #[test]
    fn every_document_field_appears_in_every_format() {
        assert_every_field_appears_in_list(&document(), false);
        assert_every_field_appears_in_list(&document_unset(), true);
        assert_every_field_appears_in_detail(&document(), false);
        assert_every_field_appears_in_detail(&document_unset(), true);
    }

    #[test]
    fn every_metadata_field_appears_in_every_format() {
        let tag: Tag = from_json(json!({
            "id": 1, "name": "n", "slug": "s", "color": "#fff",
            "is_inbox_tag": true, "document_count": 3,
        }));
        assert_every_field_appears_in_list(&tag, false);
        let tag: Tag = from_json(json!({
            "id": 1, "name": "n", "slug": "s", "color": null,
            "is_inbox_tag": null, "document_count": null,
        }));
        assert_every_field_appears_in_list(&tag, true);

        let correspondent: Correspondent =
            from_json(json!({"id": 1, "name": "n", "slug": "s", "document_count": 3}));
        assert_every_field_appears_in_list(&correspondent, false);

        let document_type: DocumentType =
            from_json(json!({"id": 1, "name": "n", "slug": "s", "document_count": 3}));
        assert_every_field_appears_in_list(&document_type, false);

        let storage_path: StoragePath = from_json(json!({
            "id": 1, "name": "n", "slug": "s", "path": "p", "document_count": 3,
        }));
        assert_every_field_appears_in_list(&storage_path, false);

        let note: Note = from_json(json!({
            "id": 1, "note": "n", "created": "2026-01-08T00:00:00Z",
            "user": {"id": 2, "username": "u"},
        }));
        assert_every_field_appears_in_list(&note, false);
        let note: Note = from_json(json!({"id": 1, "note": "n"}));
        assert_every_field_appears_in_list(&note, true);
    }

    #[test]
    fn table_cells_show_the_selected_value_or_null() {
        let select = |fields| FieldFilter::parse::<ResolvedDocument>(fields).unwrap();
        let list = |item, fields| {
            let items = [item];
            OutputFormat::Markdown
                .format_list(&items, Some(&select(fields)))
                .unwrap()
        };

        assert_eq!(cell(&list(document(), "original_file_name"), 2), "r.pdf");
        assert_eq!(cell(&list(document_unset(), "added"), 2), "null");
        assert_eq!(cell(&list(document(), "tags"), 2), r#"["Steuer"]"#);

        let detail = OutputFormat::Markdown
            .format_detail(&document_unset(), Some(&select("archive_serial_number")))
            .unwrap();
        assert!(
            detail.contains("| archive_serial_number | null"),
            "{detail}"
        );
    }

    #[test]
    fn table_columns_follow_the_requested_order_once_each() {
        let filter = FieldFilter::parse::<ResolvedDocument>("title,id,title").unwrap();
        let table = OutputFormat::Markdown
            .format_list(&[document()], Some(&filter))
            .unwrap();
        assert_eq!(table.lines().next(), Some("| title    | id |"));
    }

    #[test]
    fn selection_fills_a_missing_key_with_null() {
        let filter = FieldFilter::parse::<Tag>("id,color").unwrap();
        let record = json!({"id": 1});
        assert_eq!(filter.cells(&record), ["1", "null"]);
        let record = filter.filter_json_object(record);
        assert_eq!(record, json!({"id": 1, "color": null}));
    }

    #[test]
    fn output_without_selection_keeps_its_shape() {
        let table = OutputFormat::Markdown
            .format_list(&[document()], None)
            .unwrap();
        let header: Vec<&str> = cell(&table, 0).split('|').map(str::trim).collect();
        assert_eq!(header, ResolvedDocument::headers());

        let detail = OutputFormat::Markdown
            .format_detail(&document(), None)
            .unwrap();
        assert!(detail.contains("| Document Type "), "{detail}");

        let json = OutputFormat::Json.format_detail(&document(), None).unwrap();
        let json: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(json["correspondent"], "ACME");
    }
}
