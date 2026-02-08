use std::fmt;
use std::io::{self, Write};
use std::time::Duration;

use url::Url;

use crate::error::ApiError;
use crate::types::{
    Correspondent, Document, DocumentType, DocumentVersion, PaginatedResponse, Tag, UiSettings,
};

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

    fn get<T: serde::de::DeserializeOwned>(&self, url: &Url) -> Result<T, ApiError> {
        let mut resp = self
            .agent
            .get(url.as_str())
            .header("Accept", "application/json; version=9")
            .header("Authorization", &format!("Token {}", self.token))
            .call()?;
        let body: T = resp.body_mut().read_json()?;
        Ok(body)
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
}
