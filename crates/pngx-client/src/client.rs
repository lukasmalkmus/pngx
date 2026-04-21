use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt;
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::error::ApiError;
use crate::multipart::MultipartBuilder;
use crate::types::{
    BulkEditRequest, BulkEditResponse, Correspondent, CorrespondentCreate, CorrespondentUpdate,
    Document, DocumentPatch, DocumentType, DocumentTypeCreate, DocumentTypeUpdate, DocumentVersion,
    PaginatedResponse, StoragePath, StoragePathCreate, StoragePathUpdate, Tag, TagCreate,
    TagUpdate, Task, TaskStatus, UiSettings, UploadMetadata,
};

const WAIT_POLL_MIN: Duration = Duration::from_millis(250);
const WAIT_POLL_MAX: Duration = Duration::from_secs(5);
/// Number of consecutive empty task-list responses after which we switch
/// from `TaskPending` to `TaskUnknown` (see plan decision 2).
const TASK_UNKNOWN_THRESHOLD: usize = 10;

const DEFAULT_PAGE_SIZE: u32 = 100;

const DOCUMENT_LIST_FIELDS: &str = "id,title,correspondent,document_type,tags,created,added,archive_serial_number,original_file_name";

/// A synchronous client for the Paperless-ngx REST API.
pub struct Client {
    base_url: Url,
    token: String,
    agent: ureq::Agent,
    page_size: u32,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.base_url)
            .field("token", &"[REDACTED]")
            .field("page_size", &self.page_size)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Creates a new client with the given base URL and API token.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::InvalidUrl`] if `base_url` cannot be parsed.
    pub fn new(base_url: &str, token: &str) -> Result<Self, ApiError> {
        Self::builder(base_url, token).build()
    }

    /// Returns a [`ClientBuilder`] for configuring a new client.
    #[must_use]
    pub fn builder(base_url: &str, token: &str) -> ClientBuilder {
        ClientBuilder {
            base_url: base_url.to_string(),
            token: token.to_string(),
            timeout: None,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    /// Returns the running Paperless-ngx server version.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn server_version(&self) -> Result<String, ApiError> {
        let settings = self.ui_settings()?;
        Ok(settings.settings.version)
    }

    /// Fetches UI settings including user info and server version.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn ui_settings(&self) -> Result<UiSettings, ApiError> {
        let url = self.url("api/ui_settings/")?;
        self.get(&url)
    }

    /// Fetches the first page of documents.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure, authentication issues, or
    /// deserialization problems.
    pub fn documents(&self) -> Result<PaginatedResponse<Document>, ApiError> {
        let mut url = self.url("api/documents/")?;
        url.query_pairs_mut()
            .append_pair("fields", DOCUMENT_LIST_FIELDS)
            .append_pair("page_size", &self.page_size.to_string());
        self.get(&url)
    }

    /// Fetches a single document by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the document does not exist.
    pub fn document(&self, id: u64) -> Result<Document, ApiError> {
        let url = self.url(&format!("api/documents/{id}/"))?;
        self.get(&url)
    }

    /// Fetches the extracted text content of a document.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the document does not exist.
    pub fn document_content(&self, id: u64) -> Result<String, ApiError> {
        let url = self.url(&format!("api/documents/{id}/"))?;
        let doc: Document = self.get(&url)?;
        Ok(doc.content.unwrap_or_default())
    }

    /// Downloads a document file and streams it into `dest`.
    ///
    /// Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the document does not exist, or
    /// [`ApiError::Io`] if writing to `dest` fails.
    pub fn download_document<W: Write>(
        &self,
        id: u64,
        version: DocumentVersion,
        dest: &mut W,
    ) -> Result<u64, ApiError> {
        let path = match version {
            DocumentVersion::Original => format!("api/documents/{id}/download/"),
            DocumentVersion::Archived => format!("api/documents/{id}/preview/"),
        };
        let url = self.url(&path)?;
        let mut resp = self
            .agent
            .get(url.as_str())
            .header("Authorization", &format!("Token {}", self.token))
            .call()?;
        let bytes = io::copy(&mut resp.body_mut().as_reader(), dest)?;
        Ok(bytes)
    }

    /// Fetches the first page of tags.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn tags(&self) -> Result<PaginatedResponse<Tag>, ApiError> {
        let mut url = self.url("api/tags/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.get(&url)
    }

    /// Fetches the first page of correspondents.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn correspondents(&self) -> Result<PaginatedResponse<Correspondent>, ApiError> {
        let mut url = self.url("api/correspondents/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.get(&url)
    }

    /// Fetches the first page of document types.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn document_types(&self) -> Result<PaginatedResponse<DocumentType>, ApiError> {
        let mut url = self.url("api/document_types/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.get(&url)
    }

    /// Fetches the first page of inbox documents.
    ///
    /// Inbox documents are those tagged with an inbox tag
    /// (`is_in_inbox=true`).
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn inbox_documents(&self) -> Result<PaginatedResponse<Document>, ApiError> {
        let mut url = self.url("api/documents/")?;
        url.query_pairs_mut()
            .append_pair("is_in_inbox", "true")
            .append_pair("fields", DOCUMENT_LIST_FIELDS)
            .append_pair("page_size", &self.page_size.to_string());
        self.get(&url)
    }

    /// Fetches inbox documents across pages up to `limit`.
    ///
    /// Pass `None` to fetch all inbox documents. Returns the collected items
    /// and the total count reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn collect_inbox_documents(
        &self,
        limit: Option<usize>,
    ) -> Result<(Vec<Document>, u64), ApiError> {
        let mut url = self.url("api/documents/")?;
        url.query_pairs_mut()
            .append_pair("is_in_inbox", "true")
            .append_pair("fields", DOCUMENT_LIST_FIELDS)
            .append_pair("page_size", &self.page_size.to_string());
        self.paginate(&url, limit)
    }

    /// Searches documents matching `query`, returning the first page.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn search(&self, query: &str) -> Result<PaginatedResponse<Document>, ApiError> {
        let mut url = self.url("api/documents/")?;
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("page_size", &self.page_size.to_string());
        self.get(&url)
    }

    /// Fetches documents across pages up to `limit`.
    ///
    /// Pass `None` to fetch all documents. Returns the collected items and
    /// the total count reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn collect_documents(
        &self,
        limit: Option<usize>,
    ) -> Result<(Vec<Document>, u64), ApiError> {
        let mut url = self.url("api/documents/")?;
        url.query_pairs_mut()
            .append_pair("fields", DOCUMENT_LIST_FIELDS)
            .append_pair("page_size", &self.page_size.to_string());
        self.paginate(&url, limit)
    }

    /// Fetches tags across pages up to `limit`.
    ///
    /// Pass `None` to fetch all tags. Returns the collected items and the
    /// total count reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn collect_tags(&self, limit: Option<usize>) -> Result<(Vec<Tag>, u64), ApiError> {
        let mut url = self.url("api/tags/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.paginate(&url, limit)
    }

    /// Fetches correspondents across pages up to `limit`.
    ///
    /// Pass `None` to fetch all correspondents. Returns the collected items
    /// and the total count reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn collect_correspondents(
        &self,
        limit: Option<usize>,
    ) -> Result<(Vec<Correspondent>, u64), ApiError> {
        let mut url = self.url("api/correspondents/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.paginate(&url, limit)
    }

    /// Fetches document types across pages up to `limit`.
    ///
    /// Pass `None` to fetch all document types. Returns the collected items
    /// and the total count reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn collect_document_types(
        &self,
        limit: Option<usize>,
    ) -> Result<(Vec<DocumentType>, u64), ApiError> {
        let mut url = self.url("api/document_types/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.paginate(&url, limit)
    }

    /// Searches documents matching `query` across pages up to `limit`.
    ///
    /// Pass `None` to fetch all matching documents. Returns the collected
    /// items and the total count reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn collect_search(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<(Vec<Document>, u64), ApiError> {
        let mut url = self.url("api/documents/")?;
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("page_size", &self.page_size.to_string());
        self.paginate(&url, limit)
    }

    fn paginate<T: serde::de::DeserializeOwned>(
        &self,
        url: &Url,
        limit: Option<usize>,
    ) -> Result<(Vec<T>, u64), ApiError> {
        let first: PaginatedResponse<T> = self.get(url)?;
        let total = first.count;
        let mut results = first.results;

        let max = limit.unwrap_or(usize::MAX);
        if results.len() >= max {
            results.truncate(max);
            return Ok((results, total));
        }

        let mut next = first.next;
        while let Some(next_url) = next {
            if results.len() >= max {
                break;
            }
            let parsed = Url::parse(&next_url)?;
            if parsed.scheme() != self.base_url.scheme() {
                return Err(ApiError::SchemeMismatch {
                    expected: self.base_url.scheme().to_string(),
                    returned: parsed.scheme().to_string(),
                });
            }
            let page: PaginatedResponse<T> = self.get(&parsed)?;
            next = page.next;
            results.extend(page.results);
        }

        results.truncate(max);
        Ok((results, total))
    }

    fn url(&self, path: &str) -> Result<Url, ApiError> {
        Ok(self.base_url.join(path)?)
    }

    fn get<T: DeserializeOwned>(&self, url: &Url) -> Result<T, ApiError> {
        let mut resp = self
            .agent
            .get(url.as_str())
            .header("Accept", "application/json; version=9")
            .header("Authorization", &format!("Token {}", self.token))
            .call()?;
        let body: T = resp.body_mut().read_json()?;
        Ok(body)
    }

    fn post_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, ApiError> {
        let url = self.url(path)?;
        let resp = self
            .agent
            .post(url.as_str())
            .config()
            .http_status_as_error(false)
            .build()
            .header("Accept", "application/json; version=9")
            .header("Authorization", &format!("Token {}", self.token))
            .send_json(body)?;
        decode_body_json(resp)
    }

    fn patch_json<T: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, ApiError> {
        let url = self.url(path)?;
        let resp = self
            .agent
            .patch(url.as_str())
            .config()
            .http_status_as_error(false)
            .build()
            .header("Accept", "application/json; version=9")
            .header("Authorization", &format!("Token {}", self.token))
            .send_json(body)?;
        decode_body_json(resp)
    }

    fn delete_path(&self, path: &str) -> Result<(), ApiError> {
        let url = self.url(path)?;
        let resp = self
            .agent
            .delete(url.as_str())
            .config()
            .http_status_as_error(false)
            .build()
            .header("Accept", "application/json; version=9")
            .header("Authorization", &format!("Token {}", self.token))
            .call()?;
        expect_no_content(resp)
    }

    // --- Tag CRUD ----------------------------------------------------------

    /// Creates a new tag.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::ValidationError`] if the server rejects the
    /// payload (e.g. duplicate name).
    pub fn create_tag(&self, payload: &TagCreate) -> Result<Tag, ApiError> {
        self.post_json("api/tags/", payload)
    }

    /// Partially updates a tag by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the tag does not exist.
    pub fn update_tag(&self, id: u64, payload: &TagUpdate) -> Result<Tag, ApiError> {
        self.patch_json(&format!("api/tags/{id}/"), payload)
    }

    /// Deletes a tag by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the tag does not exist.
    pub fn delete_tag(&self, id: u64) -> Result<(), ApiError> {
        self.delete_path(&format!("api/tags/{id}/"))
    }

    // --- Correspondent CRUD ------------------------------------------------

    /// Creates a new correspondent.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::ValidationError`] if the server rejects the payload.
    pub fn create_correspondent(
        &self,
        payload: &CorrespondentCreate,
    ) -> Result<Correspondent, ApiError> {
        self.post_json("api/correspondents/", payload)
    }

    /// Partially updates a correspondent by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the correspondent does not exist.
    pub fn update_correspondent(
        &self,
        id: u64,
        payload: &CorrespondentUpdate,
    ) -> Result<Correspondent, ApiError> {
        self.patch_json(&format!("api/correspondents/{id}/"), payload)
    }

    /// Deletes a correspondent by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the correspondent does not exist.
    pub fn delete_correspondent(&self, id: u64) -> Result<(), ApiError> {
        self.delete_path(&format!("api/correspondents/{id}/"))
    }

    // --- Document type CRUD ------------------------------------------------

    /// Creates a new document type.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::ValidationError`] if the server rejects the payload.
    pub fn create_document_type(
        &self,
        payload: &DocumentTypeCreate,
    ) -> Result<DocumentType, ApiError> {
        self.post_json("api/document_types/", payload)
    }

    /// Partially updates a document type by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the document type does not exist.
    pub fn update_document_type(
        &self,
        id: u64,
        payload: &DocumentTypeUpdate,
    ) -> Result<DocumentType, ApiError> {
        self.patch_json(&format!("api/document_types/{id}/"), payload)
    }

    /// Deletes a document type by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the document type does not exist.
    pub fn delete_document_type(&self, id: u64) -> Result<(), ApiError> {
        self.delete_path(&format!("api/document_types/{id}/"))
    }

    // --- Storage path read + CRUD ------------------------------------------

    /// Fetches the first page of storage paths.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn storage_paths(&self) -> Result<PaginatedResponse<StoragePath>, ApiError> {
        let mut url = self.url("api/storage_paths/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.get(&url)
    }

    /// Fetches storage paths across pages up to `limit`.
    ///
    /// Pass `None` to fetch all storage paths. Returns the collected items
    /// and the total count reported by the server.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn collect_storage_paths(
        &self,
        limit: Option<usize>,
    ) -> Result<(Vec<StoragePath>, u64), ApiError> {
        let mut url = self.url("api/storage_paths/")?;
        url.query_pairs_mut()
            .append_pair("page_size", &self.page_size.to_string());
        self.paginate(&url, limit)
    }

    /// Creates a new storage path.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::ValidationError`] if the server rejects the payload.
    pub fn create_storage_path(
        &self,
        payload: &StoragePathCreate,
    ) -> Result<StoragePath, ApiError> {
        self.post_json("api/storage_paths/", payload)
    }

    /// Partially updates a storage path by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the storage path does not exist.
    pub fn update_storage_path(
        &self,
        id: u64,
        payload: &StoragePathUpdate,
    ) -> Result<StoragePath, ApiError> {
        self.patch_json(&format!("api/storage_paths/{id}/"), payload)
    }

    /// Deletes a storage path by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the storage path does not exist.
    pub fn delete_storage_path(&self, id: u64) -> Result<(), ApiError> {
        self.delete_path(&format!("api/storage_paths/{id}/"))
    }

    // --- Document update / delete / bulk edit ------------------------------

    /// Partially update a document by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the document does not exist, or
    /// [`ApiError::ValidationError`] on DRF field errors.
    pub fn update_document(&self, id: u64, patch: &DocumentPatch) -> Result<Document, ApiError> {
        self.patch_json(&format!("api/documents/{id}/"), patch)
    }

    /// Delete a document by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::NotFound`] if the document does not exist.
    pub fn delete_document(&self, id: u64) -> Result<(), ApiError> {
        self.delete_path(&format!("api/documents/{id}/"))
    }

    /// Perform a server-side bulk edit on a set of documents.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::ValidationError`] if Paperless rejects the
    /// payload.
    pub fn bulk_edit(&self, request: &BulkEditRequest) -> Result<BulkEditResponse, ApiError> {
        self.post_json("api/documents/bulk_edit/", request)
    }

    // --- Document upload + task polling ------------------------------------

    /// Uploads a document, streaming its bytes from disk. Returns the Celery
    /// task UUID that Paperless assigns to the consumption job.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::Io`] if the file cannot be opened,
    /// [`ApiError::BadRequest`] if Paperless rejects the upload (e.g.
    /// duplicate content hash), or [`ApiError::ValidationError`] on a 400
    /// response with per-field errors.
    pub fn upload_document(
        &self,
        file: &Path,
        metadata: &UploadMetadata,
    ) -> Result<String, ApiError> {
        let filename = file
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("document")
            .to_string();
        let mut builder = MultipartBuilder::new();
        populate_upload_parts(&mut builder, metadata);
        builder.file_from_path("document", &filename, "application/octet-stream", file)?;
        let (mut body, content_type) = builder.build();
        self.send_upload(&content_type, &mut body)
    }

    /// Uploads a document from an in-memory byte buffer. Intended for tests
    /// and small payloads; the CLI path uses [`Client::upload_document`]
    /// instead, which streams from disk.
    ///
    /// # Errors
    ///
    /// Same as [`Client::upload_document`].
    pub fn upload_document_bytes(
        &self,
        bytes: Vec<u8>,
        filename: &str,
        metadata: &UploadMetadata,
    ) -> Result<String, ApiError> {
        let mut builder = MultipartBuilder::new();
        populate_upload_parts(&mut builder, metadata);
        builder.file_from_bytes("document", filename, "application/octet-stream", bytes);
        let (mut body, content_type) = builder.build();
        self.send_upload(&content_type, &mut body)
    }

    fn send_upload(
        &self,
        content_type: &str,
        body: &mut crate::multipart::MultipartBody,
    ) -> Result<String, ApiError> {
        // Buffer the body so the request advertises `Content-Length`
        // instead of `Transfer-Encoding: chunked`. Paperless-ngx under
        // granian (the WSGI server shipped in recent versions) rejects
        // chunked `multipart/form-data` uploads with
        // `{"document": ["No file was submitted."]}` — the dechunker
        // presents an empty body to the Django multipart parser.
        //
        // The multipart encoder itself remains streaming (see
        // `crates/pngx-client/src/multipart.rs`); only this final hop
        // materializes the bytes for the sake of the server. For typical
        // tax-document PDFs (single-digit MB) this is a non-issue; for
        // the occasional multi-hundred-MB scan, memory still spikes but
        // no more than the file size plus small framing overhead.
        let mut buf = Vec::new();
        io::copy(body, &mut buf)?;

        let url = self.url("api/documents/post_document/")?;
        let resp = self
            .agent
            .post(url.as_str())
            .config()
            .http_status_as_error(false)
            .build()
            .header("Accept", "application/json; version=9")
            .header("Authorization", &format!("Token {}", self.token))
            .header("Content-Type", content_type)
            .send(&buf[..])?;
        let (parts, mut resp_body) = resp.into_parts();
        let status = parts.status.as_u16();
        if !(200..300).contains(&status) {
            return Err(status_to_error(status, resp_body));
        }
        // Response body is a bare JSON string (the task UUID).
        let task_uuid: String = resp_body.read_json()?;
        Ok(task_uuid)
    }

    /// Fetches a Paperless consumption task by its Celery UUID.
    ///
    /// Paperless returns a list; on success, the first element is returned.
    /// `Ok(None)` indicates the task is not (yet) visible — it may have
    /// just been queued, or Celery's result expiry may have reaped it.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure or authentication issues.
    pub fn task(&self, task_uuid: &str) -> Result<Option<Task>, ApiError> {
        let mut url = self.url("api/tasks/")?;
        url.query_pairs_mut().append_pair("task_id", task_uuid);
        let tasks: Vec<Task> = self.get(&url)?;
        Ok(tasks.into_iter().next())
    }

    /// Upload a document and wait for Paperless to finish consuming it,
    /// returning the created document ID.
    ///
    /// # Errors
    ///
    /// - [`ApiError::TaskPending`] if `timeout` elapses while the task is
    ///   still running (non-empty responses).
    /// - [`ApiError::TaskUnknown`] if the task list returns empty for ten
    ///   consecutive polls — the task was likely reaped by Celery and the
    ///   outcome cannot be determined.
    /// - [`ApiError::BadRequest`] if Paperless reports `FAILURE` or
    ///   `REVOKED`.
    pub fn upload_document_and_wait(
        &self,
        file: &Path,
        metadata: &UploadMetadata,
        timeout: Duration,
    ) -> Result<u64, ApiError> {
        let task_uuid = self.upload_document(file, metadata)?;
        self.wait_for_task(&task_uuid, timeout)
    }

    fn wait_for_task(&self, task_uuid: &str, timeout: Duration) -> Result<u64, ApiError> {
        let deadline = Instant::now() + timeout;
        let mut backoff = WAIT_POLL_MIN;
        let mut empty_streak = 0usize;
        loop {
            if let Some(task) = self.task(task_uuid)? {
                empty_streak = 0;
                match task.status {
                    TaskStatus::Success => {
                        return task.related_document.ok_or_else(|| ApiError::BadRequest {
                            message: "task succeeded but no document id returned".to_string(),
                        });
                    }
                    TaskStatus::Failure => {
                        return Err(ApiError::BadRequest {
                            message: task.result.unwrap_or_else(|| "upload failed".to_string()),
                        });
                    }
                    TaskStatus::Revoked => {
                        return Err(ApiError::BadRequest {
                            message: "task revoked".to_string(),
                        });
                    }
                    TaskStatus::Pending | TaskStatus::Started | TaskStatus::Other => {
                        // keep polling
                    }
                }
            } else {
                empty_streak += 1;
                if empty_streak >= TASK_UNKNOWN_THRESHOLD {
                    return Err(ApiError::TaskUnknown {
                        task_uuid: task_uuid.to_string(),
                    });
                }
            }
            if Instant::now() >= deadline {
                return Err(ApiError::TaskPending {
                    task_uuid: task_uuid.to_string(),
                });
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(WAIT_POLL_MAX);
        }
    }
}

/// Populate the non-file multipart fields from an [`UploadMetadata`].
fn populate_upload_parts(builder: &mut MultipartBuilder, metadata: &UploadMetadata) {
    if let Some(title) = &metadata.title {
        builder.text("title", title);
    }
    if let Some(created) = metadata.created {
        builder.text("created", &created.to_string());
    }
    if let Some(id) = metadata.correspondent {
        builder.text("correspondent", &id.to_string());
    }
    if let Some(id) = metadata.document_type {
        builder.text("document_type", &id.to_string());
    }
    if let Some(id) = metadata.storage_path {
        builder.text("storage_path", &id.to_string());
    }
    if let Some(asn) = metadata.archive_serial_number {
        builder.text("archive_serial_number", &asn.to_string());
    }
    for tag_id in &metadata.tags {
        builder.text("tags", &tag_id.to_string());
    }
}

// --- Write-response decoding ---------------------------------------------
//
// Write endpoints are called with `http_status_as_error(false)` so we can
// read the body on 4xx responses. These helpers centralize that decoding.

fn decode_body_json<R: DeserializeOwned>(
    resp: ureq::http::Response<ureq::Body>,
) -> Result<R, ApiError> {
    let (parts, body) = resp.into_parts();
    let status = parts.status.as_u16();
    if (200..300).contains(&status) {
        let mut body = body;
        Ok(body.read_json()?)
    } else {
        Err(status_to_error(status, body))
    }
}

fn expect_no_content(resp: ureq::http::Response<ureq::Body>) -> Result<(), ApiError> {
    let (parts, body) = resp.into_parts();
    let status = parts.status.as_u16();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(status_to_error(status, body))
    }
}

fn status_to_error(status: u16, mut body: ureq::Body) -> ApiError {
    match status {
        401 | 403 => ApiError::Unauthorized,
        404 => ApiError::NotFound,
        400 | 422 => parse_write_error_body(body.read_to_string().unwrap_or_default()),
        _ => ApiError::Server {
            status,
            message: body
                .read_to_string()
                .unwrap_or_else(|_| "unexpected status".to_string()),
        },
    }
}

/// Try to parse a DRF error body into [`ApiError::ValidationError`]; fall
/// back to [`ApiError::BadRequest`] with the raw body as the message when
/// the shape is unfamiliar.
fn parse_write_error_body(body: String) -> ApiError {
    if body.is_empty() {
        return ApiError::BadRequest {
            message: "empty response body".to_string(),
        };
    }
    match serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&body) {
        Ok(map) => {
            let mut field_errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (field, value) in map {
                match value {
                    serde_json::Value::Array(items) => {
                        let msgs = items
                            .into_iter()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s,
                                other => other.to_string(),
                            })
                            .collect();
                        field_errors.insert(field, msgs);
                    }
                    serde_json::Value::String(s) => {
                        field_errors.insert(field, vec![s]);
                    }
                    other => {
                        field_errors.insert(field, vec![other.to_string()]);
                    }
                }
            }
            if field_errors.is_empty() {
                ApiError::BadRequest { message: body }
            } else {
                ApiError::ValidationError { field_errors }
            }
        }
        Err(_) => ApiError::BadRequest { message: body },
    }
}

/// A builder for configuring a [`Client`].
pub struct ClientBuilder {
    base_url: String,
    token: String,
    timeout: Option<Duration>,
    page_size: u32,
}

impl ClientBuilder {
    /// Sets the global request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the number of items to request per API page.
    #[must_use]
    pub fn page_size(mut self, page_size: u32) -> Self {
        self.page_size = page_size;
        self
    }

    /// Builds the [`Client`].
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::InvalidUrl`] if the base URL cannot be parsed.
    pub fn build(self) -> Result<Client, ApiError> {
        let base_url = Url::parse(&self.base_url)?;

        let agent = if let Some(timeout) = self.timeout {
            let config = ureq::Agent::config_builder()
                .timeout_global(Some(timeout))
                .build();
            ureq::Agent::new_with_config(config)
        } else {
            ureq::Agent::new_with_defaults()
        };

        Ok(Client {
            base_url,
            token: self.token,
            agent,
            page_size: self.page_size,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::types::MatchingAlgorithm;

    async fn setup() -> (MockServer, Client) {
        let server = MockServer::start().await;
        let client =
            Client::new(&server.uri(), "test-token").expect("client creation should succeed");
        (server, client)
    }

    #[tokio::test]
    async fn test_documents_list() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{
                "id": 1,
                "title": "Test Document",
                "content": null,
                "correspondent": 2,
                "document_type": 3,
                "tags": [1, 2],
                "created": "2024-01-01",
                "added": "2024-01-01T00:00:00Z",
                "archive_serial_number": null,
                "original_file_name": "test.pdf"
            }]
        });

        Mock::given(method("GET"))
            .and(path("/api/documents/"))
            .and(header("Accept", "application/json; version=9"))
            .and(header("Authorization", "Token test-token"))
            .and(query_param("fields", DOCUMENT_LIST_FIELDS))
            .and(query_param("page_size", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result = client
            .documents()
            .expect("documents request should succeed");
        assert_eq!(result.count, 1);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].title, "Test Document");
        assert_eq!(result.results[0].id, 1);
    }

    #[tokio::test]
    async fn test_document_by_id() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "id": 42,
            "title": "Specific Document",
            "content": "Full content here",
            "correspondent": null,
            "document_type": null,
            "tags": [],
            "created": null,
            "added": null,
            "archive_serial_number": null,
            "original_file_name": null
        });

        Mock::given(method("GET"))
            .and(path("/api/documents/42/"))
            .and(header("Authorization", "Token test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let doc = client
            .document(42)
            .expect("document request should succeed");
        assert_eq!(doc.id, 42);
        assert_eq!(doc.title, "Specific Document");
        assert_eq!(doc.content, Some("Full content here".to_string()));
    }

    #[tokio::test]
    async fn test_document_content() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "id": 1,
            "title": "Doc",
            "content": "The full text content",
            "correspondent": null,
            "document_type": null,
            "tags": [],
            "created": null,
            "added": null,
            "archive_serial_number": null,
            "original_file_name": null
        });

        Mock::given(method("GET"))
            .and(path("/api/documents/1/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let content = client
            .document_content(1)
            .expect("content request should succeed");
        assert_eq!(content, "The full text content");
    }

    #[tokio::test]
    async fn test_download_document() {
        let (server, client) = setup().await;

        let pdf_bytes = b"%PDF-fake-content";

        Mock::given(method("GET"))
            .and(path("/api/documents/10/download/"))
            .and(header("Authorization", "Token test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pdf_bytes.as_slice()))
            .expect(1)
            .mount(&server)
            .await;

        let mut buf = Vec::new();
        let bytes = client
            .download_document(10, DocumentVersion::Original, &mut buf)
            .expect("download should succeed");
        assert_eq!(buf, pdf_bytes);
        assert_eq!(bytes, pdf_bytes.len() as u64);
    }

    #[tokio::test]
    async fn test_download_preview() {
        let (server, client) = setup().await;

        let preview_bytes = b"preview-data";

        Mock::given(method("GET"))
            .and(path("/api/documents/10/preview/"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(preview_bytes.as_slice()))
            .expect(1)
            .mount(&server)
            .await;

        let mut buf = Vec::new();
        let bytes = client
            .download_document(10, DocumentVersion::Archived, &mut buf)
            .expect("preview download should succeed");
        assert_eq!(buf, preview_bytes);
        assert_eq!(bytes, preview_bytes.len() as u64);
    }

    #[tokio::test]
    async fn test_tags() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "count": 2,
            "next": null,
            "previous": null,
            "results": [
                {"id": 1, "name": "Invoice", "slug": "invoice", "color": "#ff0000", "is_inbox_tag": false, "document_count": 10},
                {"id": 2, "name": "Receipt", "slug": "receipt", "color": null, "is_inbox_tag": true, "document_count": 5}
            ]
        });

        Mock::given(method("GET"))
            .and(path("/api/tags/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result = client.tags().expect("tags request should succeed");
        assert_eq!(result.count, 2);
        assert_eq!(result.results[0].name, "Invoice");
        assert_eq!(result.results[1].name, "Receipt");
    }

    #[tokio::test]
    async fn test_correspondents() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{"id": 1, "name": "ACME Corp", "slug": "acme-corp", "document_count": 3}]
        });

        Mock::given(method("GET"))
            .and(path("/api/correspondents/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result = client
            .correspondents()
            .expect("correspondents request should succeed");
        assert_eq!(result.count, 1);
        assert_eq!(result.results[0].name, "ACME Corp");
    }

    #[tokio::test]
    async fn test_document_types_list() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{"id": 1, "name": "Invoice", "slug": "invoice", "document_count": 15}]
        });

        Mock::given(method("GET"))
            .and(path("/api/document_types/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result = client
            .document_types()
            .expect("document_types request should succeed");
        assert_eq!(result.count, 1);
        assert_eq!(result.results[0].name, "Invoice");
    }

    #[tokio::test]
    async fn test_inbox_documents() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{
                "id": 7,
                "title": "Unprocessed Invoice",
                "content": null,
                "correspondent": 2,
                "document_type": 3,
                "tags": [1],
                "created": "2024-06-15",
                "added": "2024-06-15T12:00:00Z",
                "archive_serial_number": null,
                "original_file_name": "scan.pdf"
            }]
        });

        Mock::given(method("GET"))
            .and(path("/api/documents/"))
            .and(header("Accept", "application/json; version=9"))
            .and(header("Authorization", "Token test-token"))
            .and(query_param("is_in_inbox", "true"))
            .and(query_param("fields", DOCUMENT_LIST_FIELDS))
            .and(query_param("page_size", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result = client
            .inbox_documents()
            .expect("inbox documents request should succeed");
        assert_eq!(result.count, 1);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].title, "Unprocessed Invoice");
        assert_eq!(result.results[0].id, 7);
    }

    #[tokio::test]
    async fn test_search() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{
                "id": 5,
                "title": "Search Result",
                "content": null,
                "correspondent": null,
                "document_type": null,
                "tags": [],
                "created": null,
                "added": null,
                "archive_serial_number": null,
                "original_file_name": null
            }]
        });

        Mock::given(method("GET"))
            .and(path("/api/documents/"))
            .and(query_param("query", "tax return"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result = client.search("tax return").expect("search should succeed");
        assert_eq!(result.count, 1);
        assert_eq!(result.results[0].title, "Search Result");
    }

    #[tokio::test]
    async fn test_unauthorized_error() {
        let (server, client) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api/documents/"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .documents()
            .expect_err("should return unauthorized error");
        assert!(matches!(err, ApiError::Unauthorized));
    }

    #[tokio::test]
    async fn test_forbidden_error() {
        let (server, client) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api/documents/"))
            .respond_with(ResponseTemplate::new(403))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .documents()
            .expect_err("should return unauthorized error");
        assert!(matches!(err, ApiError::Unauthorized));
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let (server, client) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api/documents/999/"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .document(999)
            .expect_err("should return not found error");
        assert!(matches!(err, ApiError::NotFound));
    }

    #[tokio::test]
    async fn test_server_error() {
        let (server, client) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api/documents/"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;

        let err = client.documents().expect_err("should return server error");
        assert!(matches!(err, ApiError::Server { status: 500, .. }));
    }

    #[tokio::test]
    async fn test_custom_page_size() {
        let (server, _) = setup().await;

        let client = Client::builder(&server.uri(), "test-token")
            .page_size(25)
            .build()
            .expect("client builder should succeed");

        let body = serde_json::json!({
            "count": 0,
            "next": null,
            "previous": null,
            "results": []
        });

        Mock::given(method("GET"))
            .and(path("/api/documents/"))
            .and(query_param("page_size", "25"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result: PaginatedResponse<Document> =
            client.documents().expect("request should succeed");
        assert_eq!(result.count, 0);
    }

    #[tokio::test]
    async fn test_builder_with_timeout() {
        let client = Client::builder("http://localhost:9999", "tok")
            .timeout(Duration::from_secs(30))
            .page_size(50)
            .build()
            .expect("builder should succeed");

        assert_eq!(client.page_size, 50);
    }

    #[test]
    fn test_invalid_base_url() {
        let err = Client::new("not a url", "token").expect_err("should fail with invalid URL");
        assert!(matches!(err, ApiError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn test_server_version() {
        let (server, client) = setup().await;

        let body = serde_json::json!({
            "user": {"id": 1, "username": "admin"},
            "settings": {
                "version": "2.14.7"
            },
            "permissions": []
        });

        Mock::given(method("GET"))
            .and(path("/api/ui_settings/"))
            .and(header("Authorization", "Token test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let version = client
            .server_version()
            .expect("server_version should succeed");
        assert_eq!(version, "2.14.7");
    }

    #[tokio::test]
    async fn test_server_version_unauthorized() {
        let (server, client) = setup().await;

        Mock::given(method("GET"))
            .and(path("/api/ui_settings/"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .server_version()
            .expect_err("should return unauthorized error");
        assert!(matches!(err, ApiError::Unauthorized));
    }

    // --- Write helpers: taxonomy CRUD ------------------------------------

    #[tokio::test]
    async fn test_create_tag_happy_path() {
        use wiremock::matchers::body_json_string;

        let (server, client) = setup().await;
        let request_body = serde_json::json!({
            "name": "Steuer",
            "color": "#c02020",
            "matching_algorithm": 1,
            "is_inbox_tag": false
        });
        let response_body = serde_json::json!({
            "id": 42,
            "name": "Steuer",
            "slug": "steuer",
            "color": "#c02020",
            "is_inbox_tag": false,
            "document_count": 0
        });

        Mock::given(method("POST"))
            .and(path("/api/tags/"))
            .and(header("Authorization", "Token test-token"))
            .and(header("Accept", "application/json; version=9"))
            .and(body_json_string(request_body.to_string()))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_body))
            .expect(1)
            .mount(&server)
            .await;

        let payload = TagCreate {
            name: "Steuer".to_string(),
            color: Some("#c02020".to_string()),
            matching_algorithm: Some(MatchingAlgorithm::Any),
            is_inbox_tag: Some(false),
            ..Default::default()
        };
        let tag = client
            .create_tag(&payload)
            .expect("create_tag should succeed");
        assert_eq!(tag.id, 42);
        assert_eq!(tag.name, "Steuer");
    }

    #[tokio::test]
    async fn test_create_tag_omits_none_fields() {
        // Verify that `Option::None` fields do not appear in the JSON body —
        // Paperless would interpret explicit `null` differently from absent.
        use wiremock::matchers::body_json_string;

        let (server, client) = setup().await;
        // Only `name` should be sent when other fields are None.
        let request_body = serde_json::json!({"name": "Minimal"});
        let response_body = serde_json::json!({
            "id": 1,
            "name": "Minimal",
            "slug": "minimal",
            "color": null,
            "is_inbox_tag": false,
            "document_count": 0
        });

        Mock::given(method("POST"))
            .and(path("/api/tags/"))
            .and(body_json_string(request_body.to_string()))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_body))
            .expect(1)
            .mount(&server)
            .await;

        let payload = TagCreate {
            name: "Minimal".to_string(),
            ..Default::default()
        };
        client
            .create_tag(&payload)
            .expect("create_tag should succeed");
    }

    #[tokio::test]
    async fn test_update_correspondent_partial() {
        use wiremock::matchers::body_json_string;

        let (server, client) = setup().await;
        // Only `matches` should be sent; `name`, `matching_algorithm`, etc.
        // should be absent.
        let request_body = serde_json::json!({"matches": "apple.com"});
        let response_body = serde_json::json!({
            "id": 7,
            "name": "Apple",
            "slug": "apple",
            "document_count": 3
        });

        Mock::given(method("PATCH"))
            .and(path("/api/correspondents/7/"))
            .and(header("Authorization", "Token test-token"))
            .and(body_json_string(request_body.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .expect(1)
            .mount(&server)
            .await;

        let payload = CorrespondentUpdate {
            matches: Some("apple.com".to_string()),
            ..Default::default()
        };
        let updated = client
            .update_correspondent(7, &payload)
            .expect("update_correspondent should succeed");
        assert_eq!(updated.name, "Apple");
    }

    #[tokio::test]
    async fn test_delete_document_type_no_content() {
        let (server, client) = setup().await;

        Mock::given(method("DELETE"))
            .and(path("/api/document_types/9/"))
            .and(header("Authorization", "Token test-token"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        client
            .delete_document_type(9)
            .expect("delete_document_type should succeed");
    }

    #[tokio::test]
    async fn test_delete_tag_not_found() {
        let (server, client) = setup().await;

        Mock::given(method("DELETE"))
            .and(path("/api/tags/999/"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .delete_tag(999)
            .expect_err("delete_tag should fail on 404");
        assert!(matches!(err, ApiError::NotFound), "got: {err:?}");
    }

    #[tokio::test]
    async fn test_create_tag_validation_error() {
        let (server, client) = setup().await;
        let error_body = serde_json::json!({
            "name": ["tag with this name already exists"]
        });

        Mock::given(method("POST"))
            .and(path("/api/tags/"))
            .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
            .expect(1)
            .mount(&server)
            .await;

        let payload = TagCreate {
            name: "Duplicate".to_string(),
            ..Default::default()
        };
        let err = client
            .create_tag(&payload)
            .expect_err("should fail with 400");
        match err {
            ApiError::ValidationError { field_errors } => {
                assert_eq!(
                    field_errors.get("name").map(Vec::as_slice),
                    Some(&["tag with this name already exists".to_string()][..])
                );
            }
            other => panic!("expected ValidationError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_create_tag_bad_request_non_json() {
        let (server, client) = setup().await;

        Mock::given(method("POST"))
            .and(path("/api/tags/"))
            .respond_with(ResponseTemplate::new(400).set_body_string("plain text error"))
            .expect(1)
            .mount(&server)
            .await;

        let payload = TagCreate {
            name: "X".to_string(),
            ..Default::default()
        };
        let err = client.create_tag(&payload).expect_err("should fail");
        match err {
            ApiError::BadRequest { message } => assert_eq!(message, "plain text error"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_matching_algorithm_serde_is_integer() {
        // Guard the serde representation: MatchingAlgorithm must wire as an
        // integer, not a string; Paperless's API expects u8.
        for (variant, expected) in [
            (MatchingAlgorithm::None, 0u8),
            (MatchingAlgorithm::Any, 1),
            (MatchingAlgorithm::All, 2),
            (MatchingAlgorithm::Literal, 3),
            (MatchingAlgorithm::Regex, 4),
            (MatchingAlgorithm::Fuzzy, 5),
            (MatchingAlgorithm::Auto, 6),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected.to_string(), "variant {variant:?}");
            let round_trip: MatchingAlgorithm = serde_json::from_str(&json).unwrap();
            assert_eq!(round_trip, variant);
        }
    }

    // --- Document update / delete / bulk edit ---------------------------

    #[tokio::test]
    async fn test_update_document_sends_only_non_none_fields() {
        use wiremock::matchers::body_json_string;

        let (server, client) = setup().await;
        let request_body = serde_json::json!({"title": "New title"});
        let response_body = serde_json::json!({
            "id": 42,
            "title": "New title",
            "content": null,
            "correspondent": null,
            "document_type": null,
            "tags": [],
            "created": null,
            "added": null,
            "archive_serial_number": null,
            "original_file_name": null
        });

        Mock::given(method("PATCH"))
            .and(path("/api/documents/42/"))
            .and(header("Authorization", "Token test-token"))
            .and(body_json_string(request_body.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let patch = DocumentPatch {
            title: Some("New title".to_string()),
            ..Default::default()
        };
        let updated = client
            .update_document(42, &patch)
            .expect("update_document should succeed");
        assert_eq!(updated.title, "New title");
    }

    #[tokio::test]
    async fn test_update_document_with_tag_replace_sends_tags_array() {
        use wiremock::matchers::body_json_string;

        let (server, client) = setup().await;
        let request_body = serde_json::json!({"tags": [1, 2, 3]});
        let response_body = serde_json::json!({
            "id": 5,
            "title": "x",
            "content": null,
            "correspondent": null,
            "document_type": null,
            "tags": [1, 2, 3],
            "created": null,
            "added": null,
            "archive_serial_number": null,
            "original_file_name": null
        });

        Mock::given(method("PATCH"))
            .and(path("/api/documents/5/"))
            .and(body_json_string(request_body.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let patch = DocumentPatch {
            tags: Some(vec![1, 2, 3]),
            ..Default::default()
        };
        client
            .update_document(5, &patch)
            .expect("update_document should succeed");
    }

    #[tokio::test]
    async fn test_delete_document_no_content() {
        let (server, client) = setup().await;
        Mock::given(method("DELETE"))
            .and(path("/api/documents/42/"))
            .and(header("Authorization", "Token test-token"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        client.delete_document(42).expect("delete should succeed");
    }

    #[tokio::test]
    async fn test_bulk_edit_add_tag_body_shape() {
        use crate::types::BulkEditMethod;
        use wiremock::matchers::body_json_string;

        let (server, client) = setup().await;
        let request_body = serde_json::json!({
            "documents": [1, 2, 3],
            "method": "add_tag",
            "parameters": {"tag": 5}
        });
        let response_body = serde_json::json!({
            "result": "ok",
            "affected_documents": [1, 2, 3]
        });

        Mock::given(method("POST"))
            .and(path("/api/documents/bulk_edit/"))
            .and(body_json_string(request_body.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let response = client
            .bulk_edit(&BulkEditRequest {
                documents: vec![1, 2, 3],
                method: BulkEditMethod::AddTag,
                parameters: serde_json::json!({"tag": 5}),
            })
            .expect("bulk_edit should succeed");
        assert_eq!(response.affected_documents, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_bulk_edit_delete_method_snake_case() {
        use crate::types::BulkEditMethod;
        use wiremock::matchers::body_json_string;

        let (server, client) = setup().await;
        let request_body = serde_json::json!({
            "documents": [9],
            "method": "delete",
            "parameters": {}
        });

        Mock::given(method("POST"))
            .and(path("/api/documents/bulk_edit/"))
            .and(body_json_string(request_body.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&server)
            .await;

        client
            .bulk_edit(&BulkEditRequest {
                documents: vec![9],
                method: BulkEditMethod::Delete,
                parameters: serde_json::json!({}),
            })
            .expect("bulk_edit delete should succeed");
    }

    // --- Upload + task polling ------------------------------------------

    #[tokio::test]
    async fn test_upload_document_bytes_returns_task_uuid() {
        use wiremock::matchers::header_exists;

        let (server, client) = setup().await;
        let task_uuid = "11111111-2222-3333-4444-555555555555";

        Mock::given(method("POST"))
            .and(path("/api/documents/post_document/"))
            .and(header("Authorization", "Token test-token"))
            .and(header_exists("content-type"))
            .respond_with(ResponseTemplate::new(200).set_body_json(task_uuid))
            .expect(1)
            .mount(&server)
            .await;

        let metadata = UploadMetadata {
            title: Some("Invoice".to_string()),
            ..Default::default()
        };
        let returned = client
            .upload_document_bytes(b"%PDF-fake".to_vec(), "invoice.pdf", &metadata)
            .expect("upload should succeed");
        assert_eq!(returned, task_uuid);
    }

    #[tokio::test]
    async fn test_upload_body_contains_all_metadata_parts() {
        // Assert the multipart body contains each metadata field as a form
        // part. We match on substrings because the boundary is random.
        use wiremock::matchers::body_string_contains;

        let (server, client) = setup().await;
        let task_uuid = "22222222-2222-2222-2222-222222222222";

        Mock::given(method("POST"))
            .and(path("/api/documents/post_document/"))
            .and(body_string_contains("name=\"title\""))
            .and(body_string_contains("name=\"created\""))
            .and(body_string_contains("name=\"correspondent\""))
            .and(body_string_contains("name=\"document_type\""))
            .and(body_string_contains("name=\"tags\""))
            .and(body_string_contains("name=\"storage_path\""))
            .and(body_string_contains("name=\"archive_serial_number\""))
            .and(body_string_contains("name=\"document\""))
            .and(body_string_contains("filename=\"scan.pdf\""))
            .and(body_string_contains(
                "Content-Type: application/octet-stream",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(task_uuid))
            .expect(1)
            .mount(&server)
            .await;

        let metadata = UploadMetadata {
            title: Some("Jan 2026".to_string()),
            created: Some(jiff::civil::date(2026, 1, 15)),
            correspondent: Some(5),
            document_type: Some(2),
            storage_path: Some(7),
            tags: vec![1, 2, 3],
            archive_serial_number: Some(1001),
        };
        client
            .upload_document_bytes(b"%PDF-x".to_vec(), "scan.pdf", &metadata)
            .expect("upload should succeed");
    }

    #[tokio::test]
    async fn test_upload_omits_empty_metadata_fields() {
        // With the default UploadMetadata, only the `document` part should
        // be present. The task UUID response is a bare JSON string.
        use wiremock::matchers::body_string_contains;

        let (server, client) = setup().await;
        let task_uuid = "33333333-3333-3333-3333-333333333333";

        Mock::given(method("POST"))
            .and(path("/api/documents/post_document/"))
            .and(body_string_contains("name=\"document\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(task_uuid))
            .expect(1)
            .mount(&server)
            .await;

        client
            .upload_document_bytes(b"%PDF-min".to_vec(), "min.pdf", &UploadMetadata::default())
            .expect("upload should succeed");
    }

    #[tokio::test]
    async fn test_upload_400_bad_request() {
        let (server, client) = setup().await;

        Mock::given(method("POST"))
            .and(path("/api/documents/post_document/"))
            .respond_with(ResponseTemplate::new(400).set_body_string("unsupported file type: .xyz"))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .upload_document_bytes(b"x".to_vec(), "f.xyz", &UploadMetadata::default())
            .expect_err("upload should fail");
        match err {
            ApiError::BadRequest { message } => {
                assert!(message.contains("unsupported file type"));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_task_returns_some_when_list_has_one() {
        use wiremock::matchers::query_param;

        let (server, client) = setup().await;
        let task_uuid = "44444444-4444-4444-4444-444444444444";

        Mock::given(method("GET"))
            .and(path("/api/tasks/"))
            .and(query_param("task_id", task_uuid))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 42,
                    "task_id": task_uuid,
                    "status": "SUCCESS",
                    "result": null,
                    "related_document": 99,
                    "task_file_name": "invoice.pdf"
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;

        let task = client.task(task_uuid).expect("task request should succeed");
        assert!(task.is_some());
        let task = task.unwrap();
        assert_eq!(task.status, TaskStatus::Success);
        assert_eq!(task.related_document, Some(99));
    }

    #[tokio::test]
    async fn test_task_returns_none_when_list_is_empty() {
        let (server, client) = setup().await;
        Mock::given(method("GET"))
            .and(path("/api/tasks/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Vec::<serde_json::Value>::new()))
            .expect(1)
            .mount(&server)
            .await;

        let task = client.task("unknown-uuid").expect("task request OK");
        assert!(task.is_none());
    }

    #[tokio::test]
    async fn test_wait_returns_document_id_on_success() {
        use wiremock::matchers::query_param;

        let (server, client) = setup().await;
        let task_uuid = "55555555-5555-5555-5555-555555555555";

        // Upload: return UUID.
        Mock::given(method("POST"))
            .and(path("/api/documents/post_document/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(task_uuid))
            .expect(1)
            .mount(&server)
            .await;

        // First task poll: STARTED.
        Mock::given(method("GET"))
            .and(path("/api/tasks/"))
            .and(query_param("task_id", task_uuid))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 1,
                    "task_id": task_uuid,
                    "status": "STARTED",
                    "result": null,
                    "related_document": null
                }])),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Second task poll: SUCCESS.
        Mock::given(method("GET"))
            .and(path("/api/tasks/"))
            .and(query_param("task_id", task_uuid))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 1,
                    "task_id": task_uuid,
                    "status": "SUCCESS",
                    "result": null,
                    "related_document": 77
                }])),
            )
            .mount(&server)
            .await;

        let doc_id = client
            .upload_document_bytes(b"x".to_vec(), "scan.pdf", &UploadMetadata::default())
            .and_then(|task_uuid| client.wait_for_task(&task_uuid, Duration::from_secs(5)))
            .expect("wait should return doc id");
        assert_eq!(doc_id, 77);
    }

    #[tokio::test]
    async fn test_wait_surfaces_failure_message() {
        let (server, client) = setup().await;
        let task_uuid = "66666666-6666-6666-6666-666666666666";

        Mock::given(method("GET"))
            .and(path("/api/tasks/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 1,
                    "task_id": task_uuid,
                    "status": "FAILURE",
                    "result": "file is password-protected",
                    "related_document": null
                }])),
            )
            .mount(&server)
            .await;

        let err = client
            .wait_for_task(task_uuid, Duration::from_secs(5))
            .expect_err("wait should fail on FAILURE");
        match err {
            ApiError::BadRequest { message } => {
                assert!(message.contains("password-protected"), "got: {message}");
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_wait_returns_task_unknown_after_empty_threshold() {
        let (server, client) = setup().await;
        let task_uuid = "77777777-7777-7777-7777-777777777777";

        Mock::given(method("GET"))
            .and(path("/api/tasks/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Vec::<serde_json::Value>::new()))
            .mount(&server)
            .await;

        // Use a long timeout so we hit the empty-streak threshold, not the
        // deadline. Polls take 250ms initially and double; 10 polls reach
        // roughly 15s worth of exponential backoff (capped at 5s each).
        let err = client
            .wait_for_task(task_uuid, Duration::from_mins(2))
            .expect_err("wait should fail on empty streak");
        match err {
            ApiError::TaskUnknown { task_uuid: t } => assert_eq!(t, task_uuid),
            other => panic!("expected TaskUnknown, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_wait_returns_task_pending_on_deadline() {
        let (server, client) = setup().await;
        let task_uuid = "88888888-8888-8888-8888-888888888888";

        // Task keeps reporting PENDING forever — caller deadline must fire.
        Mock::given(method("GET"))
            .and(path("/api/tasks/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": 1,
                    "task_id": task_uuid,
                    "status": "PENDING",
                    "result": null,
                    "related_document": null
                }])),
            )
            .mount(&server)
            .await;

        let start = Instant::now();
        let err = client
            .wait_for_task(task_uuid, Duration::from_millis(600))
            .expect_err("wait should time out");
        match err {
            ApiError::TaskPending { task_uuid: t } => assert_eq!(t, task_uuid),
            other => panic!("expected TaskPending, got {other:?}"),
        }
        // Sanity: didn't spin for hours.
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_storage_paths_list() {
        let (server, client) = setup().await;
        let body = serde_json::json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{
                "id": 3,
                "name": "Steuer",
                "slug": "steuer",
                "path": "{{ correspondent }}/{{ created_year }}",
                "document_count": 12
            }]
        });

        Mock::given(method("GET"))
            .and(path("/api/storage_paths/"))
            .and(header("Authorization", "Token test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .expect(1)
            .mount(&server)
            .await;

        let result = client
            .storage_paths()
            .expect("storage_paths should succeed");
        assert_eq!(result.count, 1);
        assert_eq!(result.results[0].name, "Steuer");
        assert_eq!(
            result.results[0].path,
            "{{ correspondent }}/{{ created_year }}"
        );
    }
}
