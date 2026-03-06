use std::collections::HashMap;
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

use pngx_client::Client;

const CACHE_TTL: Duration = Duration::from_secs(300);

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
