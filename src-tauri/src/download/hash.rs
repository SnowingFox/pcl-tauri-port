//! Hashing helpers (SHA-1 / SHA-256) used to verify downloaded files.
//! Part of a third-party derivative of Plain Craft Launcher; see /LICENCE attribution notes.

use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashKind {
    Sha1,
    Sha256,
}

/// Hex digest of an in-memory buffer.
#[cfg(test)]
pub fn hex_of(kind: HashKind, bytes: &[u8]) -> String {
    match kind {
        HashKind::Sha1 => hex::encode(Sha1::digest(bytes)),
        HashKind::Sha256 => hex::encode(Sha256::digest(bytes)),
    }
}

/// Hex digest of a file, computed on a blocking thread.
pub async fn file_hash(kind: HashKind, path: PathBuf) -> std::io::Result<String> {
    tokio::task::spawn_blocking(move || file_hash_sync(kind, &path))
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?
}

pub fn file_hash_sync(kind: HashKind, path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    match kind {
        HashKind::Sha1 => {
            let mut h = Sha1::new();
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(h.finalize()))
        }
        HashKind::Sha256 => {
            let mut h = Sha256::new();
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex::encode(h.finalize()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert_eq!(hex_of(HashKind::Sha1, b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            hex_of(HashKind::Sha256, b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn file_matches_buffer() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, b"hello world").unwrap();
        assert_eq!(
            file_hash(HashKind::Sha1, p.clone()).await.unwrap(),
            hex_of(HashKind::Sha1, b"hello world")
        );
        assert_eq!(
            file_hash(HashKind::Sha256, p).await.unwrap(),
            hex_of(HashKind::Sha256, b"hello world")
        );
    }
}
