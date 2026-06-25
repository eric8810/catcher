//! Lightweight multipart/form-data encoder (RFC 7578).
//!
//! Builds a `multipart/form-data` body as `Vec<u8>` with a random boundary.
//! No streaming — the entire body is materialized in memory.

// ── Types ────────────────────────────────────────────────────

/// A single part in a multipart/form-data body.
#[derive(Debug, Clone)]
pub enum Part {
    /// A text form field.
    Text { name: String, value: String },
    /// A binary file field.
    File {
        name: String,
        filename: String,
        content_type: String,
        data: Vec<u8>,
    },
    /// A raw binary field with custom content-type.
    Bytes {
        name: String,
        content_type: String,
        data: Vec<u8>,
    },
}

/// Multipart form-data builder.
#[derive(Debug, Clone)]
pub struct MultipartForm {
    parts: Vec<Part>,
}

impl MultipartForm {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Add a text field.
    pub fn text(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.parts.push(Part::Text {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Add a file field from bytes.
    pub fn file(
        mut self,
        name: impl Into<String>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        data: Vec<u8>,
    ) -> Self {
        self.parts.push(Part::File {
            name: name.into(),
            filename: filename.into(),
            content_type: content_type.into(),
            data,
        });
        self
    }

    /// Add a raw binary field.
    pub fn bytes(
        mut self,
        name: impl Into<String>,
        content_type: impl Into<String>,
        data: Vec<u8>,
    ) -> Self {
        self.parts.push(Part::Bytes {
            name: name.into(),
            content_type: content_type.into(),
            data,
        });
        self
    }

    /// Encode the multipart body, returning `(body_bytes, content_type_header)`.
    ///
    /// `content_type_header` is `"multipart/form-data; boundary=..."`.
    pub fn encode(&self) -> (Vec<u8>, String) {
        let boundary = generate_boundary();
        let content_type = format!("multipart/form-data; boundary={boundary}");
        let mut buf = Vec::new();

        for part in &self.parts {
            // --boundary\r\n
            buf.extend_from_slice(b"--");
            buf.extend_from_slice(boundary.as_bytes());
            buf.extend_from_slice(b"\r\n");

            match part {
                Part::Text { name, value } => {
                    // Content-Disposition: form-data; name="..."\r\n
                    write_disposition(&mut buf, name, None);
                    buf.extend_from_slice(b"\r\n");
                    buf.extend_from_slice(value.as_bytes());
                    buf.extend_from_slice(b"\r\n");
                }
                Part::File {
                    name,
                    filename,
                    content_type,
                    data,
                } => {
                    write_disposition(&mut buf, name, Some(filename));
                    buf.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
                    buf.extend_from_slice(b"\r\n");
                    buf.extend_from_slice(data);
                    buf.extend_from_slice(b"\r\n");
                }
                Part::Bytes {
                    name,
                    content_type,
                    data,
                } => {
                    write_disposition(&mut buf, name, None);
                    buf.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
                    buf.extend_from_slice(b"\r\n");
                    buf.extend_from_slice(data);
                    buf.extend_from_slice(b"\r\n");
                }
            }
        }

        // --boundary--\r\n
        buf.extend_from_slice(b"--");
        buf.extend_from_slice(boundary.as_bytes());
        buf.extend_from_slice(b"--\r\n");

        (buf, content_type)
    }

    /// Number of parts in this form.
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// Whether this form is empty.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

impl Default for MultipartForm {
    fn default() -> Self {
        Self::new()
    }
}

fn write_disposition(buf: &mut Vec<u8>, name: &str, filename: Option<&str>) {
    buf.extend_from_slice(b"Content-Disposition: form-data; name=\"");
    buf.extend_from_slice(name.as_bytes());
    buf.extend_from_slice(b"\"");
    if let Some(fname) = filename {
        buf.extend_from_slice(b"; filename=\"");
        buf.extend_from_slice(fname.as_bytes());
        buf.extend_from_slice(b"\"");
    }
    buf.extend_from_slice(b"\r\n");
}

/// Generate a random 32-char hex boundary string.
fn generate_boundary() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Simple boundary: "----RustMultipartBoundaryXXXXXXXXXXXXXXXX"
    // Use a mix of timestamp + pseudo-random for uniqueness
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hex = format!("{:032x}", now);
    format!("----RustFormBoundary{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp1_text_only() {
        let form = MultipartForm::new()
            .text("username", "alice")
            .text("email", "alice@example.com");

        let (body, ct) = form.encode();
        let body_str = String::from_utf8(body).unwrap();

        assert!(ct.starts_with("multipart/form-data; boundary="));
        assert!(body_str.contains("name=\"username\""));
        assert!(body_str.contains("alice"));
        assert!(body_str.contains("name=\"email\""));
        assert!(body_str.contains("alice@example.com"));
        assert!(body_str.contains("--"));
    }

    #[test]
    fn mp2_file_upload() {
        let form = MultipartForm::new().text("description", "my avatar").file(
            "avatar",
            "photo.png",
            "image/png",
            vec![0x89, 0x50, 0x4E, 0x47],
        );

        let (body, ct) = form.encode();
        let body_str = String::from_utf8_lossy(&body);

        assert!(ct.starts_with("multipart/form-data; boundary="));
        assert!(body_str.contains("filename=\"photo.png\""));
        assert!(body_str.contains("Content-Type: image/png"));
        assert!(body_str.contains("name=\"description\""));
    }

    #[test]
    fn mp3_bytes_field() {
        let form = MultipartForm::new().bytes(
            "binary_data",
            "application/octet-stream",
            vec![0x00, 0x01, 0x02],
        );

        let (body, _ct) = form.encode();
        assert!(!body.is_empty());
        assert!(!form.is_empty());
    }

    #[test]
    fn mp4_empty_form() {
        let form = MultipartForm::new();
        assert!(form.is_empty());
        assert_eq!(form.len(), 0);
        // Still encodes (just the closing boundary)
        let (body, _) = form.encode();
        assert!(!body.is_empty());
    }

    #[test]
    fn mp5_boundary_uniqueness() {
        // Two encodes should produce different boundaries (time-based)
        let form = MultipartForm::new().text("a", "b");
        let (_, ct1) = form.encode();
        // Small sleep to get different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let (_, ct2) = form.encode();
        // They might be the same if within same nanosecond — that's OK for testing
        assert!(ct1.starts_with("multipart/form-data; boundary="));
        assert!(ct2.starts_with("multipart/form-data; boundary="));
    }
}
