use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, ErrorCode, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::{ErrorData as McpError, ServiceExt, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use pngx_client::{
    BulkEditMethod, BulkEditRequest, Client, CorrespondentCreate, CorrespondentUpdate,
    DocumentPatch, DocumentTypeCreate, DocumentTypeUpdate, MatchingAlgorithm, StoragePathCreate,
    StoragePathUpdate, TagCreate, TagUpdate, UploadMetadata,
};

const CACHE_TTL: Duration = Duration::from_mins(5);

struct CachedResolver {
    tags: HashMap<u64, String>,
    correspondents: HashMap<u64, String>,
    document_types: HashMap<u64, String>,
    fetched_at: Instant,
}

impl CachedResolver {
    fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > CACHE_TTL
    }

    fn tag_name(&self, id: u64) -> String {
        self.tags
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("#{id}"))
    }

    fn correspondent_name(&self, id: u64) -> Option<String> {
        self.correspondents.get(&id).cloned()
    }

    fn document_type_name(&self, id: u64) -> Option<String> {
        self.document_types.get(&id).cloned()
    }
}

#[derive(Serialize)]
struct ResolvedDoc {
    id: u64,
    title: String,
    correspondent: Option<String>,
    document_type: Option<String>,
    tags: Vec<String>,
    created: Option<jiff::civil::Date>,
    added: Option<jiff::Timestamp>,
    archive_serial_number: Option<u64>,
    original_file_name: Option<String>,
}

fn resolve_doc(doc: &pngx_client::Document, resolver: &CachedResolver) -> ResolvedDoc {
    ResolvedDoc {
        id: doc.id,
        title: doc.title.clone(),
        correspondent: doc
            .correspondent
            .and_then(|id| resolver.correspondent_name(id)),
        document_type: doc
            .document_type
            .and_then(|id| resolver.document_type_name(id)),
        tags: doc.tags.iter().map(|&id| resolver.tag_name(id)).collect(),
        created: doc.created,
        added: doc.added,
        archive_serial_number: doc.archive_serial_number,
        original_file_name: doc.original_file_name.clone(),
    }
}

fn to_json_text<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}

#[allow(clippy::needless_pass_by_value)]
fn api_err(e: pngx_client::ApiError) -> McpError {
    McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
}

#[allow(clippy::needless_pass_by_value)]
fn spawn_err(e: tokio::task::JoinError) -> McpError {
    McpError::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
}

fn resolve_by_map(map: &HashMap<u64, String>, input: &str, entity: &str) -> Result<u64, McpError> {
    if let Ok(id) = input.parse::<u64>() {
        return Ok(id);
    }
    let mut matches: Vec<u64> = map
        .iter()
        .filter_map(|(id, name)| if name == input { Some(*id) } else { None })
        .collect();
    match matches.len() {
        0 => Err(McpError::new(
            ErrorCode::INVALID_PARAMS,
            format!("no {entity} named '{input}'"),
            None,
        )),
        1 => Ok(matches.remove(0)),
        _ => {
            let ids: Vec<String> = matches.iter().map(u64::to_string).collect();
            Err(McpError::new(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "{entity} '{input}' is ambiguous (matches IDs: {})",
                    ids.join(", ")
                ),
                None,
            ))
        }
    }
}

fn resolve_tag_ref(resolver: &CachedResolver, input: &str) -> Result<u64, McpError> {
    resolve_by_map(&resolver.tags, input, "tag")
}

fn resolve_correspondent_ref(resolver: &CachedResolver, input: &str) -> Result<u64, McpError> {
    resolve_by_map(&resolver.correspondents, input, "correspondent")
}

fn resolve_document_type_ref(resolver: &CachedResolver, input: &str) -> Result<u64, McpError> {
    resolve_by_map(&resolver.document_types, input, "document type")
}

fn parse_bulk_method(method: &str) -> Result<BulkEditMethod, McpError> {
    use BulkEditMethod as M;
    Ok(match method {
        "set_correspondent" => M::SetCorrespondent,
        "set_document_type" => M::SetDocumentType,
        "set_storage_path" => M::SetStoragePath,
        "add_tag" => M::AddTag,
        "remove_tag" => M::RemoveTag,
        "modify_tags" => M::ModifyTags,
        "delete" => M::Delete,
        "reprocess" => M::Reprocess,
        "rotate" => M::Rotate,
        "modify_custom_fields" => M::ModifyCustomFields,
        other => {
            return Err(McpError::new(
                ErrorCode::INVALID_PARAMS,
                format!("unknown bulk_edit method '{other}'"),
                None,
            ));
        }
    })
}

#[derive(Clone)]
pub struct PngxMcp {
    client: Arc<Client>,
    cache: Arc<RwLock<Option<Arc<CachedResolver>>>>,
    tool_router: ToolRouter<Self>,
}

impl PngxMcp {
    pub fn new(client: Client) -> Self {
        Self {
            client: Arc::new(client),
            cache: Arc::new(RwLock::new(None)),
            tool_router: Self::tool_router(),
        }
    }

    async fn resolver(&self) -> Result<Arc<CachedResolver>, McpError> {
        {
            let guard = self.cache.read().await;
            if let Some(cached) = guard.as_ref()
                && !cached.is_expired()
            {
                return Ok(Arc::clone(cached));
            }
        }

        let client = self.client.clone();
        let resolver = tokio::task::spawn_blocking(move || {
            let (tags, _) = client.collect_tags(None).map_err(api_err)?;
            let (correspondents, _) = client.collect_correspondents(None).map_err(api_err)?;
            let (document_types, _) = client.collect_document_types(None).map_err(api_err)?;
            Ok::<_, McpError>(CachedResolver {
                tags: tags.into_iter().map(|t| (t.id, t.name)).collect(),
                correspondents: correspondents.into_iter().map(|c| (c.id, c.name)).collect(),
                document_types: document_types
                    .into_iter()
                    .map(|dt| (dt.id, dt.name))
                    .collect(),
                fetched_at: Instant::now(),
            })
        })
        .await
        .map_err(spawn_err)??;

        let resolver = Arc::new(resolver);
        *self.cache.write().await = Some(Arc::clone(&resolver));
        Ok(resolver)
    }
}

// --- Tool parameter types ---

#[derive(Deserialize, JsonSchema)]
struct SearchParams {
    /// Search query string
    query: String,
    /// Maximum number of results (omit for default 25)
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
struct LimitParams {
    /// Maximum number of results (omit for default 25)
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentIdsParams {
    /// One or more document IDs
    ids: Vec<u64>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentIdParam {
    /// Document ID
    id: u64,
}

#[derive(Deserialize, JsonSchema)]
struct IdParam {
    /// Numeric ID of the target resource
    id: u64,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum MatchingAlgorithmParam {
    None,
    Any,
    All,
    Literal,
    Regex,
    Fuzzy,
    Auto,
}

impl From<MatchingAlgorithmParam> for MatchingAlgorithm {
    fn from(value: MatchingAlgorithmParam) -> Self {
        match value {
            MatchingAlgorithmParam::None => Self::None,
            MatchingAlgorithmParam::Any => Self::Any,
            MatchingAlgorithmParam::All => Self::All,
            MatchingAlgorithmParam::Literal => Self::Literal,
            MatchingAlgorithmParam::Regex => Self::Regex,
            MatchingAlgorithmParam::Fuzzy => Self::Fuzzy,
            MatchingAlgorithmParam::Auto => Self::Auto,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
struct TagsCreateParams {
    /// Display name
    name: String,
    /// Hex color (e.g. "#c02020")
    color: Option<String>,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    /// Match expression for the chosen algorithm
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
    is_inbox_tag: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct TagsUpdateParams {
    id: u64,
    name: Option<String>,
    color: Option<String>,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
    is_inbox_tag: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct CorrespondentsCreateParams {
    name: String,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct CorrespondentsUpdateParams {
    id: u64,
    name: Option<String>,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentTypesCreateParams {
    name: String,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentTypesUpdateParams {
    id: u64,
    name: Option<String>,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct StoragePathsCreateParams {
    name: String,
    /// Path template (e.g. `{{ correspondent }}/{{ created_year }}`)
    path: String,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct StoragePathsUpdateParams {
    id: u64,
    name: Option<String>,
    path: Option<String>,
    matching_algorithm: Option<MatchingAlgorithmParam>,
    #[serde(rename = "match")]
    matches: Option<String>,
    is_insensitive: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentsUploadParams {
    /// Absolute or relative path to the file to upload
    file_path: String,
    title: Option<String>,
    /// Creation date in ISO 8601 format (YYYY-MM-DD)
    created: Option<String>,
    /// Correspondent ID or exact name
    correspondent: Option<String>,
    /// Document type ID or exact name
    document_type: Option<String>,
    /// Storage path ID (numeric only here; use `storage_paths` to find it)
    storage_path: Option<String>,
    /// List of tag IDs or exact names
    tags: Option<Vec<String>>,
    archive_serial_number: Option<u64>,
    /// Whether to wait for consumption and return the document ID (default true)
    wait: Option<bool>,
    /// Timeout in seconds when `wait` is true (default 120)
    wait_timeout: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentsUpdateParams {
    id: u64,
    title: Option<String>,
    /// Creation date in ISO 8601 format (YYYY-MM-DD)
    created: Option<String>,
    correspondent: Option<String>,
    document_type: Option<String>,
    storage_path: Option<u64>,
    /// Replaces the tag list wholesale (use `documents_tag`/`documents_untag`
    /// for atomic per-tag edits)
    tags: Option<Vec<String>>,
    archive_serial_number: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentsTagParams {
    /// Document IDs to tag/untag
    ids: Vec<u64>,
    /// Tag IDs or exact names to apply
    tags: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentsBulkEditParams {
    /// `bulk_edit` method (e.g. `add_tag`, `set_correspondent`, `delete`)
    method: String,
    /// Target document IDs
    ids: Vec<u64>,
    /// Method-specific parameter object
    parameters: Option<serde_json::Value>,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentsAddNoteParams {
    /// Document ID
    id: u64,
    /// Note text
    note: String,
}

#[derive(Deserialize, JsonSchema)]
struct DocumentsDeleteNoteParams {
    /// Document ID
    id: u64,
    /// Note ID (from `documents_notes`)
    note_id: u64,
}

// --- Tool implementations ---

#[tool_router]
impl PngxMcp {
    /// Search documents matching a query string. Returns matching documents with
    /// metadata (correspondent, type, tags resolved to names).
    #[tool(name = "search", annotations(read_only_hint = true))]
    async fn search(&self, params: Parameters<SearchParams>) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let query = params.0.query;
        let limit = Some(params.0.limit.unwrap_or(25));

        let (docs, total) = tokio::task::spawn_blocking(move || {
            client.collect_search(&query, limit).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;

        let resolver = self.resolver().await?;
        let resolved: Vec<ResolvedDoc> = docs.iter().map(|d| resolve_doc(d, &resolver)).collect();

        to_json_text(&serde_json::json!({
            "results": resolved,
            "total_count": total,
            "showing": resolved.len(),
            "has_more": (resolved.len() as u64) < total,
        }))
    }

    /// List unprocessed inbox documents. Returns documents tagged with the inbox
    /// tag, with metadata resolved to names.
    #[tool(name = "inbox", annotations(read_only_hint = true))]
    async fn inbox(&self, params: Parameters<LimitParams>) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let limit = Some(params.0.limit.unwrap_or(25));

        let (docs, total) = tokio::task::spawn_blocking(move || {
            client.collect_inbox_documents(limit).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;

        let resolver = self.resolver().await?;
        let resolved: Vec<ResolvedDoc> = docs.iter().map(|d| resolve_doc(d, &resolver)).collect();

        to_json_text(&serde_json::json!({
            "results": resolved,
            "total_count": total,
            "showing": resolved.len(),
            "has_more": (resolved.len() as u64) < total,
        }))
    }

    /// List all documents. Returns documents with metadata resolved to names.
    #[tool(name = "documents_list", annotations(read_only_hint = true))]
    async fn documents_list(
        &self,
        params: Parameters<LimitParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let limit = Some(params.0.limit.unwrap_or(25));

        let (docs, total) =
            tokio::task::spawn_blocking(move || client.collect_documents(limit).map_err(api_err))
                .await
                .map_err(spawn_err)??;

        let resolver = self.resolver().await?;
        let resolved: Vec<ResolvedDoc> = docs.iter().map(|d| resolve_doc(d, &resolver)).collect();

        to_json_text(&serde_json::json!({
            "results": resolved,
            "total_count": total,
            "showing": resolved.len(),
            "has_more": (resolved.len() as u64) < total,
        }))
    }

    /// Get one or more documents by ID. Returns full document details with
    /// metadata resolved to names. Partial failures are reported per-item.
    #[tool(name = "documents_get", annotations(read_only_hint = true))]
    async fn documents_get(
        &self,
        params: Parameters<DocumentIdsParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let ids = params.0.ids;

        let docs = tokio::task::spawn_blocking(move || {
            ids.iter()
                .map(|&id| match client.document(id) {
                    Ok(doc) => Ok(doc),
                    Err(e) => Err((id, e)),
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(spawn_err)?;

        let resolver = self.resolver().await?;

        let results: Vec<serde_json::Value> = docs
            .into_iter()
            .map(|result| match result {
                Ok(doc) => serde_json::to_value(resolve_doc(&doc, &resolver))
                    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()})),
                Err((id, e)) => serde_json::json!({"id": id, "error": e.to_string()}),
            })
            .collect();

        to_json_text(&serde_json::json!({ "results": results }))
    }

    /// Get the text content of a document. Useful for reading the OCR-extracted
    /// or original text of a document.
    #[tool(name = "documents_content", annotations(read_only_hint = true))]
    async fn documents_content(
        &self,
        params: Parameters<DocumentIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;

        let content =
            tokio::task::spawn_blocking(move || client.document_content(id).map_err(api_err))
                .await
                .map_err(spawn_err)??;

        to_json_text(&serde_json::json!({ "id": id, "content": content }))
    }

    /// List all tags defined in Paperless-ngx.
    #[tool(name = "tags", annotations(read_only_hint = true))]
    async fn tags(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();

        let (tags, _) =
            tokio::task::spawn_blocking(move || client.collect_tags(None).map_err(api_err))
                .await
                .map_err(spawn_err)??;

        to_json_text(&tags)
    }

    /// List all correspondents defined in Paperless-ngx.
    #[tool(name = "correspondents", annotations(read_only_hint = true))]
    async fn correspondents(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();

        let (correspondents, _) = tokio::task::spawn_blocking(move || {
            client.collect_correspondents(None).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;

        to_json_text(&correspondents)
    }

    /// List all document types defined in Paperless-ngx.
    #[tool(name = "document_types", annotations(read_only_hint = true))]
    async fn document_types(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();

        let (document_types, _) = tokio::task::spawn_blocking(move || {
            client.collect_document_types(None).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;

        to_json_text(&document_types)
    }

    /// Get the Paperless-ngx server version.
    #[tool(name = "version", annotations(read_only_hint = true))]
    async fn version(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();

        let version = tokio::task::spawn_blocking(move || client.server_version().map_err(api_err))
            .await
            .map_err(spawn_err)??;

        to_json_text(&serde_json::json!({ "version": version }))
    }

    // --- Storage path list (read) -----------------------------------------

    /// List all storage paths defined in Paperless-ngx.
    #[tool(name = "storage_paths", annotations(read_only_hint = true))]
    async fn storage_paths(&self) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let (paths, _) = tokio::task::spawn_blocking(move || {
            client.collect_storage_paths(None).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        to_json_text(&paths)
    }

    // --- Taxonomy create / update / delete --------------------------------

    /// Create a new tag.
    #[tool(name = "tags_create", annotations(read_only_hint = false))]
    async fn tags_create(
        &self,
        params: Parameters<TagsCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let payload = TagCreate {
            name: params.0.name,
            color: params.0.color,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
            is_inbox_tag: params.0.is_inbox_tag,
        };
        let tag = tokio::task::spawn_blocking(move || client.create_tag(&payload).map_err(api_err))
            .await
            .map_err(spawn_err)??;
        *self.cache.write().await = None; // invalidate resolver cache
        to_json_text(&tag)
    }

    /// Update a tag by ID.
    #[tool(name = "tags_update", annotations(read_only_hint = false))]
    async fn tags_update(
        &self,
        params: Parameters<TagsUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        let payload = TagUpdate {
            name: params.0.name,
            color: params.0.color,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
            is_inbox_tag: params.0.is_inbox_tag,
        };
        let tag =
            tokio::task::spawn_blocking(move || client.update_tag(id, &payload).map_err(api_err))
                .await
                .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&tag)
    }

    /// Delete a tag by ID.
    #[tool(
        name = "tags_delete",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn tags_delete(&self, params: Parameters<IdParam>) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        tokio::task::spawn_blocking(move || client.delete_tag(id).map_err(api_err))
            .await
            .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&serde_json::json!({"deleted": id}))
    }

    /// Create a new correspondent.
    #[tool(name = "correspondents_create", annotations(read_only_hint = false))]
    async fn correspondents_create(
        &self,
        params: Parameters<CorrespondentsCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let payload = CorrespondentCreate {
            name: params.0.name,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
        };
        let item = tokio::task::spawn_blocking(move || {
            client.create_correspondent(&payload).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&item)
    }

    /// Update a correspondent by ID.
    #[tool(name = "correspondents_update", annotations(read_only_hint = false))]
    async fn correspondents_update(
        &self,
        params: Parameters<CorrespondentsUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        let payload = CorrespondentUpdate {
            name: params.0.name,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
        };
        let item = tokio::task::spawn_blocking(move || {
            client.update_correspondent(id, &payload).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&item)
    }

    /// Delete a correspondent by ID.
    #[tool(
        name = "correspondents_delete",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn correspondents_delete(
        &self,
        params: Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        tokio::task::spawn_blocking(move || client.delete_correspondent(id).map_err(api_err))
            .await
            .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&serde_json::json!({"deleted": id}))
    }

    /// Create a new document type.
    #[tool(name = "document_types_create", annotations(read_only_hint = false))]
    async fn document_types_create(
        &self,
        params: Parameters<DocumentTypesCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let payload = DocumentTypeCreate {
            name: params.0.name,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
        };
        let item = tokio::task::spawn_blocking(move || {
            client.create_document_type(&payload).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&item)
    }

    /// Update a document type by ID.
    #[tool(name = "document_types_update", annotations(read_only_hint = false))]
    async fn document_types_update(
        &self,
        params: Parameters<DocumentTypesUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        let payload = DocumentTypeUpdate {
            name: params.0.name,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
        };
        let item = tokio::task::spawn_blocking(move || {
            client.update_document_type(id, &payload).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&item)
    }

    /// Delete a document type by ID.
    #[tool(
        name = "document_types_delete",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn document_types_delete(
        &self,
        params: Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        tokio::task::spawn_blocking(move || client.delete_document_type(id).map_err(api_err))
            .await
            .map_err(spawn_err)??;
        *self.cache.write().await = None;
        to_json_text(&serde_json::json!({"deleted": id}))
    }

    /// Create a new storage path.
    #[tool(name = "storage_paths_create", annotations(read_only_hint = false))]
    async fn storage_paths_create(
        &self,
        params: Parameters<StoragePathsCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let payload = StoragePathCreate {
            name: params.0.name,
            path: params.0.path,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
        };
        let item = tokio::task::spawn_blocking(move || {
            client.create_storage_path(&payload).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        to_json_text(&item)
    }

    /// Update a storage path by ID.
    #[tool(name = "storage_paths_update", annotations(read_only_hint = false))]
    async fn storage_paths_update(
        &self,
        params: Parameters<StoragePathsUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        let payload = StoragePathUpdate {
            name: params.0.name,
            path: params.0.path,
            matching_algorithm: params.0.matching_algorithm.map(Into::into),
            matches: params.0.matches,
            is_insensitive: params.0.is_insensitive,
        };
        let item = tokio::task::spawn_blocking(move || {
            client.update_storage_path(id, &payload).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        to_json_text(&item)
    }

    /// Delete a storage path by ID.
    #[tool(
        name = "storage_paths_delete",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn storage_paths_delete(
        &self,
        params: Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        tokio::task::spawn_blocking(move || client.delete_storage_path(id).map_err(api_err))
            .await
            .map_err(spawn_err)??;
        to_json_text(&serde_json::json!({"deleted": id}))
    }

    // --- Document write tools ---------------------------------------------

    /// Upload a document from a local file path. Blocks until the task
    /// completes (default 120s timeout) and returns the created document ID
    /// when `wait` is true (the default); otherwise returns the task UUID.
    #[tool(name = "documents_upload", annotations(read_only_hint = false))]
    async fn documents_upload(
        &self,
        params: Parameters<DocumentsUploadParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let file = PathBuf::from(params.0.file_path);
        let title = params.0.title;
        let created = params
            .0
            .created
            .as_deref()
            .map(str::parse::<jiff::civil::Date>)
            .transpose()
            .map_err(|e| {
                McpError::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("invalid `created` date: {e}"),
                    None,
                )
            })?;
        let correspondent = params.0.correspondent;
        let document_type = params.0.document_type;
        let tags = params.0.tags.unwrap_or_default();
        let storage_path = params.0.storage_path;
        let asn = params.0.archive_serial_number;
        let wait = params.0.wait.unwrap_or(true);
        let timeout = Duration::from_secs(params.0.wait_timeout.unwrap_or(120));

        let resolver = self.resolver().await?;
        let metadata = UploadMetadata {
            title,
            created,
            correspondent: correspondent
                .as_deref()
                .map(|s| resolve_correspondent_ref(&resolver, s))
                .transpose()?,
            document_type: document_type
                .as_deref()
                .map(|s| resolve_document_type_ref(&resolver, s))
                .transpose()?,
            storage_path: storage_path
                .as_deref()
                .map(str::parse::<u64>)
                .transpose()
                .map_err(|_| {
                    McpError::new(
                        ErrorCode::INVALID_PARAMS,
                        "storage_path must be a numeric ID (name lookup not supported in MCP)"
                            .to_string(),
                        None,
                    )
                })?,
            tags: tags
                .iter()
                .map(|t| resolve_tag_ref(&resolver, t))
                .collect::<Result<Vec<_>, _>>()?,
            archive_serial_number: asn,
        };

        let result = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, McpError> {
            if wait {
                let id = client
                    .upload_document_and_wait(&file, &metadata, timeout)
                    .map_err(api_err)?;
                Ok(serde_json::json!({"document_id": id}))
            } else {
                let task_uuid = client.upload_document(&file, &metadata).map_err(api_err)?;
                Ok(serde_json::json!({"task_uuid": task_uuid}))
            }
        })
        .await
        .map_err(spawn_err)??;
        to_json_text(&result)
    }

    /// Update a document's metadata (title, correspondent, doctype, etc.).
    /// Per-tag add/remove should go via `documents_bulk_edit` with
    /// method=`add_tag` or `remove_tag` for atomic server-side behavior.
    #[tool(name = "documents_update", annotations(read_only_hint = false))]
    async fn documents_update(
        &self,
        params: Parameters<DocumentsUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;

        let resolver = self.resolver().await?;
        let correspondent = params
            .0
            .correspondent
            .as_deref()
            .map(|s| resolve_correspondent_ref(&resolver, s))
            .transpose()?;
        let document_type = params
            .0
            .document_type
            .as_deref()
            .map(|s| resolve_document_type_ref(&resolver, s))
            .transpose()?;
        let tags = params
            .0
            .tags
            .as_ref()
            .map(|items| {
                items
                    .iter()
                    .map(|t| resolve_tag_ref(&resolver, t))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        let created = params
            .0
            .created
            .as_deref()
            .map(str::parse::<jiff::civil::Date>)
            .transpose()
            .map_err(|e| {
                McpError::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("invalid `created` date: {e}"),
                    None,
                )
            })?;
        let patch = DocumentPatch {
            title: params.0.title,
            created,
            correspondent,
            document_type,
            storage_path: params.0.storage_path,
            tags,
            archive_serial_number: params.0.archive_serial_number,
        };
        let updated = tokio::task::spawn_blocking(move || {
            client.update_document(id, &patch).map_err(api_err)
        })
        .await
        .map_err(spawn_err)??;
        to_json_text(&updated)
    }

    /// Delete a document by ID.
    #[tool(
        name = "documents_delete",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn documents_delete(
        &self,
        params: Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        tokio::task::spawn_blocking(move || client.delete_document(id).map_err(api_err))
            .await
            .map_err(spawn_err)??;
        to_json_text(&serde_json::json!({"deleted": id}))
    }

    /// Add one or more tags to one or more documents (server-side atomic).
    #[tool(name = "documents_tag", annotations(read_only_hint = false))]
    async fn documents_tag(
        &self,
        params: Parameters<DocumentsTagParams>,
    ) -> Result<CallToolResult, McpError> {
        self.documents_tag_impl(params.0, BulkEditMethod::AddTag)
            .await
    }

    /// Remove one or more tags from one or more documents (server-side atomic).
    #[tool(name = "documents_untag", annotations(read_only_hint = false))]
    async fn documents_untag(
        &self,
        params: Parameters<DocumentsTagParams>,
    ) -> Result<CallToolResult, McpError> {
        self.documents_tag_impl(params.0, BulkEditMethod::RemoveTag)
            .await
    }

    /// Run a raw `bulk_edit` operation. Marked destructive because
    /// `method=delete` is a supported runtime choice.
    #[tool(
        name = "documents_bulk_edit",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn documents_bulk_edit(
        &self,
        params: Parameters<DocumentsBulkEditParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let method = parse_bulk_method(&params.0.method)?;
        let request = BulkEditRequest {
            documents: params.0.ids,
            method,
            parameters: params.0.parameters.unwrap_or(serde_json::json!({})),
        };
        let response =
            tokio::task::spawn_blocking(move || client.bulk_edit(&request).map_err(api_err))
                .await
                .map_err(spawn_err)??;
        to_json_text(&serde_json::json!({
            "result": response.result,
            "affected_documents": response.affected_documents,
        }))
    }

    // --- Document note tools ----------------------------------------------

    /// List the notes attached to a document.
    #[tool(name = "documents_notes", annotations(read_only_hint = true))]
    async fn documents_notes(
        &self,
        params: Parameters<DocumentIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        let notes = tokio::task::spawn_blocking(move || client.document_notes(id).map_err(api_err))
            .await
            .map_err(spawn_err)??;
        to_json_text(&notes)
    }

    /// Add a note to a document. Returns the document's updated note list.
    #[tool(name = "documents_add_note", annotations(read_only_hint = false))]
    async fn documents_add_note(
        &self,
        params: Parameters<DocumentsAddNoteParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        let note = params.0.note;
        let notes =
            tokio::task::spawn_blocking(move || client.add_note(id, &note).map_err(api_err))
                .await
                .map_err(spawn_err)??;
        to_json_text(&notes)
    }

    /// Delete a note from a document. Returns the document's updated note list.
    #[tool(
        name = "documents_delete_note",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn documents_delete_note(
        &self,
        params: Parameters<DocumentsDeleteNoteParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let id = params.0.id;
        let note_id = params.0.note_id;
        let notes =
            tokio::task::spawn_blocking(move || client.delete_note(id, note_id).map_err(api_err))
                .await
                .map_err(spawn_err)??;
        to_json_text(&notes)
    }
}

impl PngxMcp {
    async fn documents_tag_impl(
        &self,
        params: DocumentsTagParams,
        method: BulkEditMethod,
    ) -> Result<CallToolResult, McpError> {
        let client = self.client.clone();
        let ids = params.ids;
        let resolver = self.resolver().await?;
        let tag_ids: Vec<u64> = params
            .tags
            .iter()
            .map(|t| resolve_tag_ref(&resolver, t))
            .collect::<Result<_, _>>()?;
        let ids_cloned = ids.clone();
        tokio::task::spawn_blocking(move || -> Result<(), McpError> {
            for tag_id in tag_ids {
                client
                    .bulk_edit(&BulkEditRequest {
                        documents: ids_cloned.clone(),
                        method,
                        parameters: serde_json::json!({"tag": tag_id}),
                    })
                    .map_err(api_err)?;
            }
            Ok(())
        })
        .await
        .map_err(spawn_err)??;
        to_json_text(&serde_json::json!({"documents": ids}))
    }
}

#[rmcp::tool_handler]
impl rmcp::handler::server::ServerHandler for PngxMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("pngx", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Paperless-ngx document management. Search, list, and read documents, \
                 tags, correspondents, and document types.",
            )
    }
}

pub async fn serve(client: Client) -> anyhow::Result<()> {
    let server = PngxMcp::new(client);
    let transport = rmcp::transport::io::stdio();
    let server = server.serve(transport).await?;
    server.waiting().await?;
    Ok(())
}
