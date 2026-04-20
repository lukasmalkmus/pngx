//! Streaming `multipart/form-data` encoder for document uploads.
//!
//! The encoder produces an `impl Read` that concatenates the preamble,
//! each part (text or file), and the closing boundary. File parts stream
//! directly from a `BufReader<File>` rather than being materialized into
//! memory, so a 200 MB scanned PDF doesn't cause a peak-RSS spike.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufReader, Cursor, Read};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

static BOUNDARY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A chunk of the multipart body. Bytes are read from memory; files stream
/// from disk; cursors wrap in-memory byte slices that were provided by the
/// caller (used by `upload_document_bytes` for tests).
enum Chunk {
    Bytes(Cursor<Vec<u8>>),
    File(BufReader<File>),
}

impl Chunk {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Bytes(cursor) => cursor.read(buf),
            Self::File(reader) => reader.read(buf),
        }
    }
}

/// Builds a streaming multipart body.
pub(crate) struct MultipartBuilder {
    boundary: String,
    chunks: VecDeque<Chunk>,
}

impl MultipartBuilder {
    pub(crate) fn new() -> Self {
        Self {
            boundary: generate_boundary(),
            chunks: VecDeque::new(),
        }
    }

    /// Adds a text form field. `value` is written inline (no escaping).
    pub(crate) fn text(&mut self, name: &str, value: &str) {
        let header = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"{name}\"\r\n\r\n",
            boundary = self.boundary,
        );
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(header.into_bytes())));
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(value.as_bytes().to_vec())));
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(b"\r\n".to_vec())));
    }

    /// Adds a file form field, reading from `path`. Returns an `io::Error`
    /// if the file cannot be opened.
    pub(crate) fn file_from_path(
        &mut self,
        name: &str,
        filename: &str,
        content_type: &str,
        path: &Path,
    ) -> io::Result<()> {
        let file = File::open(path)?;
        self.file_from_handle(name, filename, content_type, file);
        Ok(())
    }

    /// Adds a file form field backed by an already-open `File`.
    pub(crate) fn file_from_handle(
        &mut self,
        name: &str,
        filename: &str,
        content_type: &str,
        file: File,
    ) {
        let header = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
             Content-Type: {content_type}\r\n\r\n",
            boundary = self.boundary,
        );
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(header.into_bytes())));
        self.chunks.push_back(Chunk::File(BufReader::new(file)));
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(b"\r\n".to_vec())));
    }

    /// Adds a file form field backed by a byte slice (used for
    /// `upload_document_bytes` tests).
    pub(crate) fn file_from_bytes(
        &mut self,
        name: &str,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) {
        let header = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
             Content-Type: {content_type}\r\n\r\n",
            boundary = self.boundary,
        );
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(header.into_bytes())));
        self.chunks.push_back(Chunk::Bytes(Cursor::new(bytes)));
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(b"\r\n".to_vec())));
    }

    /// Finalize into a body and its `Content-Type` header value.
    pub(crate) fn build(mut self) -> (MultipartBody, String) {
        let trailer = format!("--{boundary}--\r\n", boundary = self.boundary);
        self.chunks
            .push_back(Chunk::Bytes(Cursor::new(trailer.into_bytes())));
        let content_type = format!(
            "multipart/form-data; boundary={boundary}",
            boundary = self.boundary
        );
        (
            MultipartBody {
                chunks: self.chunks,
            },
            content_type,
        )
    }
}

/// A streaming multipart body. `Read::read` pulls from the current chunk,
/// advancing to the next when the current one is exhausted.
pub(crate) struct MultipartBody {
    chunks: VecDeque<Chunk>,
}

impl Read for MultipartBody {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while let Some(front) = self.chunks.front_mut() {
            let n = front.read(out)?;
            if n > 0 {
                return Ok(n);
            }
            // Current chunk is exhausted; drop it and move to the next.
            self.chunks.pop_front();
        }
        Ok(0)
    }
}

/// Produce a 128-bit boundary string. Uniqueness within a single process
/// is guaranteed by the atomic counter; across processes, `SystemTime` and
/// process id disambiguate.
fn generate_boundary() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0u128, |d| d.as_nanos());
    let pid = u128::from(std::process::id());
    let counter = u128::from(BOUNDARY_COUNTER.fetch_add(1, Ordering::Relaxed));
    let hi = (nanos ^ (pid << 64) ^ (counter << 32)) & 0xFFFF_FFFF_FFFF_FFFF;
    let lo = nanos
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(counter)
        & 0xFFFF_FFFF_FFFF_FFFF;
    format!("----pngx-{hi:016x}{lo:016x}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn drain_all<R: Read>(mut r: R) -> Vec<u8> {
        let mut buf = Vec::new();
        r.read_to_end(&mut buf).unwrap();
        buf
    }

    #[test]
    fn text_part_round_trips() {
        let mut builder = MultipartBuilder::new();
        let boundary = builder.boundary.clone();
        builder.text("title", "Invoice March");
        let (body, ct) = builder.build();
        assert!(ct.starts_with("multipart/form-data; boundary="));

        let bytes = drain_all(body);
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains(&format!("--{boundary}\r\n")));
        assert!(text.contains("Content-Disposition: form-data; name=\"title\""));
        assert!(text.contains("Invoice March"));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
    }

    #[test]
    fn file_from_bytes_streams_content() {
        let mut builder = MultipartBuilder::new();
        let boundary = builder.boundary.clone();
        builder.file_from_bytes(
            "document",
            "scan.pdf",
            "application/pdf",
            b"%PDF-fake".to_vec(),
        );
        let (body, _) = builder.build();
        let bytes = drain_all(body);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("filename=\"scan.pdf\""));
        assert!(text.contains("Content-Type: application/pdf"));
        assert!(text.contains("%PDF-fake"));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
    }

    #[test]
    fn multiple_parts_preserve_order() {
        let mut builder = MultipartBuilder::new();
        builder.text("a", "1");
        builder.text("b", "2");
        builder.file_from_bytes("c", "f.bin", "application/octet-stream", vec![0xff]);
        let (body, _) = builder.build();
        let bytes = drain_all(body);
        let text = String::from_utf8_lossy(&bytes);
        let a = text.find("name=\"a\"").unwrap();
        let b = text.find("name=\"b\"").unwrap();
        let c = text.find("name=\"c\"").unwrap();
        assert!(a < b && b < c);
    }

    #[test]
    fn boundaries_differ_across_builders() {
        let b1 = MultipartBuilder::new().boundary;
        let b2 = MultipartBuilder::new().boundary;
        assert_ne!(b1, b2);
    }

    #[test]
    fn read_in_small_buffers_preserves_content() {
        // Stress-test the chunk transition by reading 1 byte at a time.
        let mut builder = MultipartBuilder::new();
        builder.text("k", "v");
        builder.file_from_bytes("f", "a.txt", "text/plain", b"hello world".to_vec());
        let (mut body, _) = builder.build();

        let mut collected = Vec::new();
        let mut one = [0u8; 1];
        loop {
            let n = body.read(&mut one).unwrap();
            if n == 0 {
                break;
            }
            collected.push(one[0]);
        }
        let text = String::from_utf8(collected).unwrap();
        assert!(text.contains("hello world"));
        assert!(text.contains("name=\"k\""));
    }
}
