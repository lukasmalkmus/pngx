use serde::{Deserialize, Serialize};

/// Selects which version of a document to download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentVersion {
    /// The original uploaded file.
    Original,
    /// The archived (OCR-processed) version.
    Archived,
}

/// A paginated response from the Paperless-ngx API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PaginatedResponse<T> {
    /// Total number of items matching the query.
    pub count: u64,
    /// URL of the next page, if any.
    pub next: Option<String>,
    /// URL of the previous page, if any.
    pub previous: Option<String>,
    /// Items on this page.
    pub results: Vec<T>,
}

/// A subset of the Paperless-ngx UI settings response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UiSettings {
    /// The authenticated user.
    pub user: UiSettingsUser,
    /// The settings object containing the server version.
    pub settings: UiSettingsVersion,
}

/// User object from the UI settings response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UiSettingsUser {
    /// The username.
    pub username: String,
    /// First name, if set.
    #[serde(default)]
    pub first_name: Option<String>,
    /// Last name, if set.
    #[serde(default)]
    pub last_name: Option<String>,
}

impl UiSettingsUser {
    /// Returns the display name: "First Last" if available, otherwise the
    /// username.
    #[must_use]
    pub fn display_name(&self) -> String {
        let first = self.first_name.as_deref().unwrap_or("").trim();
        let last = self.last_name.as_deref().unwrap_or("").trim();
        if first.is_empty() && last.is_empty() {
            self.username.clone()
        } else {
            format!("{first} {last}").trim().to_string()
        }
    }
}

/// Inner settings object from the UI settings response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UiSettingsVersion {
    /// The running Paperless-ngx server version.
    pub version: String,
}

/// A document stored in Paperless-ngx.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Document {
    /// Unique identifier.
    pub id: u64,
    /// Document title.
    pub title: String,
    /// Extracted text content.
    pub content: Option<String>,
    /// ID of the assigned correspondent.
    pub correspondent: Option<u64>,
    /// ID of the assigned document type.
    pub document_type: Option<u64>,
    /// IDs of assigned tags.
    pub tags: Vec<u64>,
    /// Date the document was created.
    pub created: Option<jiff::civil::Date>,
    /// Timestamp when the document was added to Paperless-ngx.
    pub added: Option<jiff::Timestamp>,
    /// Archive serial number.
    pub archive_serial_number: Option<u64>,
    /// Original file name at time of upload.
    pub original_file_name: Option<String>,
}

/// A tag used to categorize documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Tag {
    /// Unique identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// URL-safe slug.
    pub slug: String,
    /// Hex color code (e.g. `#ff0000`).
    pub color: Option<String>,
    /// Whether this tag marks documents as inbox items.
    pub is_inbox_tag: Option<bool>,
    /// Number of documents with this tag.
    pub document_count: Option<u64>,
}

/// A correspondent (sender/recipient) associated with documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Correspondent {
    /// Unique identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// URL-safe slug.
    pub slug: String,
    /// Number of documents from this correspondent.
    pub document_count: Option<u64>,
}

/// A document type used to classify documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DocumentType {
    /// Unique identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// URL-safe slug.
    pub slug: String,
    /// Number of documents with this type.
    pub document_count: Option<u64>,
}
