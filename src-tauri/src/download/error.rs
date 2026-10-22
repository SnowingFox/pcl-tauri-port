use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum DlError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("bad status {0} for {1}")]
    Status(u16, String),
    #[error("hash mismatch for {path}: expected {expected}, got {actual}")]
    Hash { path: String, expected: String, actual: String },
    #[error("size mismatch for {path}: expected {expected}, got {actual}")]
    Size { path: String, expected: u64, actual: u64 },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl Serialize for DlError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type DlResult<T> = Result<T, DlError>;

impl From<String> for DlError {
    fn from(s: String) -> Self {
        DlError::Other(s)
    }
}
impl From<&str> for DlError {
    fn from(s: &str) -> Self {
        DlError::Other(s.to_string())
    }
}
