use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// Deserialize an `Option<u64>` field that the server may return as a
/// JSON number, a JSON string, or `null` — Paperless-ngx serializes
/// `Task::related_document` inconsistently across versions.
fn deserialize_opt_u64_lax<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u64),
        Str(String),
    }
    match Option::<NumOrStr>::deserialize(deserializer)? {
        None => Ok(None),
        Some(NumOrStr::Num(n)) => Ok(Some(n)),
        Some(NumOrStr::Str(s)) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<u64>().map(Some).map_err(de::Error::custom)
            }
        }
    }
}

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

/// A note attached to a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Note {
    /// Unique identifier.
    pub id: u64,
    /// The note text.
    pub note: String,
    /// Timestamp when the note was created.
    #[serde(default)]
    pub created: Option<jiff::Timestamp>,
    /// The user who authored the note. Absent on older servers that omit it.
    #[serde(default)]
    pub user: Option<NoteUser>,
}

/// The user who authored a [`Note`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NoteUser {
    /// Unique identifier.
    pub id: u64,
    /// The username.
    pub username: String,
    /// First name, if set.
    #[serde(default)]
    pub first_name: Option<String>,
    /// Last name, if set.
    #[serde(default)]
    pub last_name: Option<String>,
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

/// A storage path used to organize documents on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StoragePath {
    /// Unique identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// URL-safe slug.
    pub slug: String,
    /// Path template (e.g. `{{ correspondent }}/{{ created_year }}`).
    pub path: String,
    /// Number of documents at this storage path.
    pub document_count: Option<u64>,
}

/// Matching algorithm Paperless uses to auto-assign correspondents, document
/// types, tags, and storage paths. Serializes as an integer on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum MatchingAlgorithm {
    /// Never auto-assign (manual only).
    None,
    /// Match if any of the words are present.
    Any,
    /// Match if all words are present.
    All,
    /// Match the literal string.
    Literal,
    /// Match a regular expression.
    Regex,
    /// Fuzzy match.
    Fuzzy,
    /// Paperless decides based on presence of other fields.
    Auto,
}

impl From<MatchingAlgorithm> for u8 {
    fn from(value: MatchingAlgorithm) -> Self {
        match value {
            MatchingAlgorithm::None => 0,
            MatchingAlgorithm::Any => 1,
            MatchingAlgorithm::All => 2,
            MatchingAlgorithm::Literal => 3,
            MatchingAlgorithm::Regex => 4,
            MatchingAlgorithm::Fuzzy => 5,
            MatchingAlgorithm::Auto => 6,
        }
    }
}

impl TryFrom<u8> for MatchingAlgorithm {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Any),
            2 => Ok(Self::All),
            3 => Ok(Self::Literal),
            4 => Ok(Self::Regex),
            5 => Ok(Self::Fuzzy),
            6 => Ok(Self::Auto),
            other => Err(format!("unknown matching algorithm: {other}")),
        }
    }
}

/// Payload for creating a tag. Omitted optional fields take Paperless
/// defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TagCreate {
    /// Display name (required).
    pub name: String,
    /// Hex color code (e.g. `"#ff0000"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// Match expression for the chosen algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// Whether the match is case-insensitive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
    /// Whether documents with this tag appear in the inbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_inbox_tag: Option<bool>,
}

/// Partial update for a tag. Any `None` field is omitted from the request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TagUpdate {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New hex color code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// New auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// New match expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// New case-insensitive flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
    /// New inbox-tag flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_inbox_tag: Option<bool>,
}

/// Payload for creating a correspondent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CorrespondentCreate {
    /// Display name (required).
    pub name: String,
    /// Auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// Match expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// Whether the match is case-insensitive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
}

/// Partial update for a correspondent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CorrespondentUpdate {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// New match expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// New case-insensitive flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
}

/// Payload for creating a document type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DocumentTypeCreate {
    /// Display name (required).
    pub name: String,
    /// Auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// Match expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// Whether the match is case-insensitive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
}

/// Partial update for a document type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DocumentTypeUpdate {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// New match expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// New case-insensitive flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
}

/// Payload for creating a storage path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StoragePathCreate {
    /// Display name (required).
    pub name: String,
    /// Path template (required).
    pub path: String,
    /// Auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// Match expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// Whether the match is case-insensitive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
}

// --- Document write payloads ---------------------------------------------

/// Partial update for a document. All fields are `Option<_>` and skipped
/// when `None`, matching Paperless's PATCH semantics: fields that are
/// absent remain unchanged. The `tags` field, when present, replaces the
/// document's tag list wholesale — use `bulk_edit` with `add_tag` /
/// `remove_tag` for atomic per-tag edits to avoid races.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DocumentPatch {
    /// New title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// New creation date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<jiff::civil::Date>,
    /// New correspondent ID (or `null` to clear — see note).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correspondent: Option<u64>,
    /// New document type ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type: Option<u64>,
    /// New storage path ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<u64>,
    /// New tag list (replaces the existing list wholesale).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<u64>>,
    /// New archive serial number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_serial_number: Option<u64>,
}

/// Methods accepted by `POST /api/documents/bulk_edit/`. Serializes as a
/// `snake_case` string matching Paperless's expected value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkEditMethod {
    /// Set correspondent on all targeted documents.
    SetCorrespondent,
    /// Set document type on all targeted documents.
    SetDocumentType,
    /// Set storage path on all targeted documents.
    SetStoragePath,
    /// Add a single tag to all targeted documents.
    AddTag,
    /// Remove a single tag from all targeted documents.
    RemoveTag,
    /// Add and/or remove multiple tags atomically.
    ModifyTags,
    /// Delete targeted documents (destructive).
    Delete,
    /// Reprocess OCR on targeted documents.
    Reprocess,
    /// Rotate targeted documents.
    Rotate,
    /// Add and/or remove custom fields.
    ModifyCustomFields,
}

/// Body for `POST /api/documents/bulk_edit/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BulkEditRequest {
    /// IDs of documents to operate on.
    pub documents: Vec<u64>,
    /// The operation to perform.
    pub method: BulkEditMethod,
    /// Method-specific parameters (pass `serde_json::json!({})` for
    /// methods that take no parameters, like `delete` or `reprocess`).
    pub parameters: serde_json::Value,
}

/// Response from `POST /api/documents/bulk_edit/`. The exact shape varies
/// by Paperless version; we expose the common fields and preserve the
/// full raw value for callers that need it.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct BulkEditResponse {
    /// Human-readable result summary (present on some versions).
    #[serde(default)]
    pub result: Option<String>,
    /// IDs of documents that were actually modified.
    #[serde(default)]
    pub affected_documents: Vec<u64>,
}

// --- Task polling --------------------------------------------------------

/// Paperless-ngx consumption task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    /// Task is queued but not started.
    Pending,
    /// Task is being processed.
    Started,
    /// Task finished successfully. `Task::related_document` is set.
    Success,
    /// Task failed; `Task::result` contains the error message.
    Failure,
    /// Task was revoked before completion.
    Revoked,
    /// Unknown status (forward-compatibility fallback).
    #[serde(other)]
    Other,
}

/// A single Paperless-ngx consumption task, as returned by
/// `GET /api/tasks/?task_id=<uuid>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Task {
    /// Internal task row ID.
    pub id: Option<u64>,
    /// Celery task UUID (the value returned by `post_document`).
    pub task_id: String,
    /// Current state.
    pub status: TaskStatus,
    /// Human-readable result or error message.
    #[serde(default)]
    pub result: Option<String>,
    /// ID of the document created by this task, populated on `SUCCESS`.
    ///
    /// Paperless-ngx serializes this field as either an integer or the
    /// integer as a JSON string depending on version. We accept both.
    #[serde(default, deserialize_with = "deserialize_opt_u64_lax")]
    pub related_document: Option<u64>,
    /// Filename being consumed.
    #[serde(default)]
    pub task_file_name: Option<String>,
}

/// Metadata to attach to a document during upload. All fields except
/// `title` are optional; Paperless derives sensible defaults when they
/// are omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadMetadata {
    /// Override the document title (defaults to the file's stem).
    pub title: Option<String>,
    /// Override the document creation date.
    pub created: Option<jiff::civil::Date>,
    /// Correspondent ID to assign.
    pub correspondent: Option<u64>,
    /// Document type ID to assign.
    pub document_type: Option<u64>,
    /// Storage path ID to assign.
    pub storage_path: Option<u64>,
    /// Tag IDs to attach.
    pub tags: Vec<u64>,
    /// Archive serial number (ASN) to assign.
    pub archive_serial_number: Option<u64>,
}

/// Partial update for a storage path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StoragePathUpdate {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New path template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// New auto-match algorithm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_algorithm: Option<MatchingAlgorithm>,
    /// New match expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<String>,
    /// New case-insensitive flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_insensitive: Option<bool>,
}
