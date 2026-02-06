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
    ) -> Result<String, serde_json::Error> {
        match self {
            Self::Json => serde_json::to_string_pretty(items),
            Self::Markdown => {
                let mut table = new_markdown_table(T::headers());
                for item in items {
                    table.add_row(item.row());
                }
                Ok(table.to_string())
            }
        }
    }

    pub fn format_detail<T: DetailView + Serialize>(
        self,
        item: &T,
    ) -> Result<String, serde_json::Error> {
        match self {
            Self::Json => serde_json::to_string_pretty(item),
            Self::Markdown => {
                let mut table = new_markdown_table(&["Field", "Value"]);
                for (field, value) in item.fields() {
                    table.add_row(vec![field.to_string(), value]);
                }
                Ok(table.to_string())
            }
        }
    }
}

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

#[derive(Serialize)]
pub struct ResolvedDocument {
    pub id: u64,
    pub title: String,
    pub correspondent: Option<u64>,
    pub correspondent_name: Option<String>,
    pub document_type: Option<u64>,
    pub document_type_name: Option<String>,
    pub tags: Vec<u64>,
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
